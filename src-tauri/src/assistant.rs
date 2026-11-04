//! The page's way in to the LLM features. The API key is read from the
//! Keychain here and handed straight to the request: no command returns it,
//! and the page only ever learns whether one is saved.

use std::path::PathBuf;
use std::sync::Mutex;

use kfbox_assist::keys::{check_key, KeyStore};
use kfbox_assist::quick::{self, Context, QuickResult};
use kfbox_assist::wizard::{Wizard, WizardTurn};
use kfbox_assist::{AssistError, Model, Settings, Transport};
use kfbox_core::scoring::Score;
use kfbox_core::session::Session;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};

/// What the assistant needs between calls.
pub struct Assistant {
    dir: PathBuf,
    keys: Box<dyn KeyStore>,
    transport: Box<dyn Transport>,
    wizard: Mutex<Wizard>,
}

impl Assistant {
    /// `dir` holds its settings; `keys` the API key; `transport` reaches the API.
    #[must_use]
    pub fn new(dir: PathBuf, keys: Box<dyn KeyStore>, transport: Box<dyn Transport>) -> Self {
        Self { dir, keys, transport, wizard: Mutex::default() }
    }

    fn key(&self) -> Result<String, AssistError> {
        self.keys.get()?.ok_or(AssistError::NoKey)
    }

    fn model(&self) -> Model {
        Settings::load(&self.dir).model
    }
}

fn text<T>(result: Result<T, AssistError>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ModelChoice {
    id: &'static str,
    label: &'static str,
}

/// The assistant's settings as the page sees them: never the key itself.
#[derive(Serialize)]
pub struct AssistantSettings {
    model: &'static str,
    models: Vec<ModelChoice>,
    key_saved: bool,
}

fn settings(assistant: &Assistant) -> Result<AssistantSettings, String> {
    Ok(AssistantSettings {
        model: assistant.model().id(),
        models: Model::ALL.iter().map(|m| ModelChoice { id: m.id(), label: m.label() }).collect(),
        key_saved: text(assistant.keys.has_key())?,
    })
}

/// Every command here that touches the Keychain is async and runs on a
/// blocking thread: one that isn't runs on the main thread, and a Keychain
/// prompt there would freeze the window until someone answered it.
async fn off_main<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    work: impl FnOnce(&Assistant) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app.state::<Assistant>())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn assistant_settings<R: Runtime>(app: AppHandle<R>) -> Result<AssistantSettings, String> {
    off_main(app, settings).await
}

#[tauri::command]
pub async fn set_assistant_model<R: Runtime>(model: String, app: AppHandle<R>) -> Result<AssistantSettings, String> {
    let model = Model::ALL.into_iter().find(|m| m.id() == model).ok_or_else(|| format!("{model} isn't a model kfbox offers"))?;
    off_main(app, move |assistant| {
        Settings { model }.save(&assistant.dir).map_err(|e| e.to_string())?;
        settings(assistant)
    })
    .await
}

/// Puts a key in the Keychain. It comes in once, from the settings screen, and
/// never goes back out.
#[tauri::command]
pub async fn save_api_key<R: Runtime>(key: String, app: AppHandle<R>) -> Result<AssistantSettings, String> {
    let key = text(check_key(&key))?;
    off_main(app, move |assistant| {
        text(assistant.keys.set(&key))?;
        settings(assistant)
    })
    .await
}

#[tauri::command]
pub async fn remove_api_key<R: Runtime>(app: AppHandle<R>) -> Result<AssistantSettings, String> {
    off_main(app, |assistant| {
        text(assistant.keys.delete())?;
        settings(assistant)
    })
    .await
}

/// Runs a quick function: an instruction the model carries out with kfbox's tools.
#[tauri::command]
pub async fn run_quick<R: Runtime>(
    instruction: String,
    track: Option<String>,
    playlist: Option<String>,
    app: AppHandle<R>,
) -> Result<QuickResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let assistant = app.state::<Assistant>();
        let session = app.state::<Session>();
        let key = text(assistant.key())?;
        let context = Context { track, playlist };
        text(quick::run(&session, assistant.transport.as_ref(), &key, assistant.model(), &instruction, &context))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One turn of the rules wizard: a description of how you cue, or more guidance.
#[tauri::command]
pub async fn wizard_turn<R: Runtime>(said: String, app: AppHandle<R>) -> Result<WizardTurn, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let assistant = app.state::<Assistant>();
        let session = app.state::<Session>();
        let key = text(assistant.key())?;
        let mut wizard = assistant.wizard.lock().map_err(|e| e.to_string())?;
        text(wizard.turn(&session, assistant.transport.as_ref(), &key, assistant.model(), &said))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn wizard_reset(assistant: State<'_, Assistant>) -> Result<(), String> {
    assistant.wizard.lock().map_err(|e| e.to_string())?.reset();
    Ok(())
}

#[tauri::command]
#[must_use]
pub fn has_saved_rules(session: State<'_, Session>) -> bool {
    session.has_saved_rules()
}

/// Saves rules, keeping the ones they replace.
#[tauri::command]
pub fn save_rules_keeping_previous(rules: String, session: State<'_, Session>) -> Result<(), String> {
    session.save_rules_keeping_previous(&rules).map_err(|e| e.to_string())
}

/// Swaps the saved rules with the ones they replaced, and returns the rules now in use.
#[tauri::command]
pub fn restore_previous_rules(session: State<'_, Session>) -> Result<String, String> {
    session.restore_previous_rules().map_err(|e| e.to_string())
}

/// Scores rules against the tracks cued by hand.
#[tauri::command]
pub async fn score_rules<R: Runtime>(rules: String, app: AppHandle<R>) -> Result<Score, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Session>().score_rules(&rules).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}
