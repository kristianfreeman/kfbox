//! The Tauri shell: a thin wrapper over `kfbox_core::session`, where the
//! work lives so the MCP server and any later front end share it. Each
//! command here only moves slow work off the main thread and turns errors
//! into text for the page. Playback, which owns the audio device, is in
//! `player`.

mod player;

use std::path::PathBuf;

use std::collections::HashMap;

use kfbox_core::safe::HistoryEntry;
use kfbox_core::session::{Analysis, CommandResult, CueEdit, Listing, Saved, Session, TrackRow};
use kfbox_core::{Applied, Track};
use tauri::{AppHandle, Builder, Manager, Runtime, State};

fn text<T>(result: CommandResult<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

/// Runs slow work off the main thread, with the session.
async fn blocking<R: Runtime, T: Send + 'static>(app: AppHandle<R>, work: impl FnOnce(&Session) -> CommandResult<T> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || text(work(&app.state::<Session>())))
        .await
        .map_err(|e| e.to_string())?
}

/// The saved rules, or the defaults before any are saved.
#[tauri::command]
fn load_rules(session: State<'_, Session>) -> String {
    session.load_rules()
}

/// Songs named on the command line, to load at start.
#[tauri::command]
fn initial_paths() -> Vec<String> {
    std::env::args().skip(1).filter(|a| std::path::Path::new(a).is_file()).collect()
}

#[tauri::command]
fn default_rules() -> String {
    Session::default_rules()
}

#[tauri::command]
async fn analyse<R: Runtime>(path: String, app: AppHandle<R>) -> Result<Analysis, String> {
    blocking(app, move |s| s.analyse_file(&PathBuf::from(path))).await
}

#[tauri::command]
async fn open_rekordbox<R: Runtime>(path: String, app: AppHandle<R>) -> Result<Listing, String> {
    blocking(app, move |s| s.open_xml(&PathBuf::from(path))).await
}

#[tauri::command]
async fn open_rekordbox_library<R: Runtime>(app: AppHandle<R>) -> Result<Listing, String> {
    blocking(app, Session::open_library).await
}

#[tauri::command]
fn load_playlist(index: Option<usize>, session: State<'_, Session>) -> Result<Vec<TrackRow>, String> {
    text(session.load_playlist(index))
}

#[tauri::command]
async fn analyse_rekordbox<R: Runtime>(id: String, app: AppHandle<R>) -> Result<Analysis, String> {
    blocking(app, move |s| s.analyse_track(&id)).await
}

#[tauri::command]
fn track_detail(path: String, session: State<'_, Session>) -> Result<Track, String> {
    text(session.track(&path))
}

#[tauri::command]
fn remove(path: String, session: State<'_, Session>) {
    session.remove(&path);
}

#[tauri::command]
fn apply_rules(rules: String, save: bool, session: State<'_, Session>) -> Result<Vec<(String, Applied)>, String> {
    text(session.apply_rules(&rules, save))
}

#[tauri::command]
async fn waveform<R: Runtime>(path: String, app: AppHandle<R>) -> Result<Vec<u8>, String> {
    blocking(app, move |s| s.waveform(&PathBuf::from(path))).await
}

#[tauri::command]
fn apply_rules_to(path: String, rules: String, session: State<'_, Session>) -> Result<Applied, String> {
    text(session.apply_rules_to(&path, &rules))
}

#[tauri::command]
fn export_xml(dest: String, rules: String, session: State<'_, Session>) -> Result<usize, String> {
    text(session.export_xml(&PathBuf::from(dest), &rules))
}

/// Saves cue edits to rekordbox's library, all together or not at all.
#[tauri::command]
async fn save_cues<R: Runtime>(summary: String, edits: Vec<CueEdit>, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.save_cues(&summary, &edits)).await
}

/// Saves the rules' suggested cues on these tracks, in one change.
#[tauri::command]
async fn accept_suggestions<R: Runtime>(ids: Vec<String>, rules: String, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.accept_suggestions(&ids, Some(&rules))).await
}

#[tauri::command]
async fn undo<R: Runtime>(app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, Session::undo).await
}

#[tauri::command]
async fn redo<R: Runtime>(app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, Session::redo).await
}

#[tauri::command]
async fn history<R: Runtime>(limit: usize, app: AppHandle<R>) -> Result<Vec<HistoryEntry>, String> {
    blocking(app, move |s| s.history(limit)).await
}

#[tauri::command]
async fn restore<R: Runtime>(commit: String, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.restore(&commit)).await
}

/// Records tracks as reviewed: `accepted` or `skipped`, or back in review for `None`.
#[tauri::command]
fn set_review(ids: Vec<String>, state: Option<String>, session: State<'_, Session>) -> Result<(), String> {
    text(session.set_review(&ids, state.as_deref()))
}

#[tauri::command]
fn reviews(session: State<'_, Session>) -> Result<HashMap<String, String>, String> {
    text(session.reviews())
}

/// Every command the page can call, on any runtime, so tests can call them
/// without a window.
pub fn commands<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        load_rules,
        initial_paths,
        default_rules,
        open_rekordbox,
        open_rekordbox_library,
        load_playlist,
        analyse,
        analyse_rekordbox,
        track_detail,
        waveform,
        player::player_load,
        player::player_play,
        player::player_pause,
        player::player_seek,
        player::player_set_loop,
        player::player_clear_loop,
        player::player_scrub_begin,
        player::player_scrub_to,
        player::player_scrub_end,
        remove,
        apply_rules,
        apply_rules_to,
        export_xml,
        save_cues,
        accept_suggestions,
        undo,
        redo,
        history,
        restore,
        set_review,
        reviews
    ])
}

/// The app's configuration and capabilities, on any runtime. Built here once:
/// it embeds an `Info.plist`, which a second copy would duplicate.
#[must_use]
pub fn context<R: Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = commands(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(player::Player::default());
            app.manage(Session::open(&dir)?);
            Ok(())
        })
        .run(context());
    if let Err(e) = result {
        eprintln!("kfbox: {e}");
        std::process::exit(1);
    }
}
