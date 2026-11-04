//! The Tauri shell: a thin wrapper over `kfbox_core::session`, where the
//! work lives so the MCP server and any later front end share it. Each
//! command here only moves slow work off the main thread and turns errors
//! into text for the page. Playback, which owns the audio device, is in
//! `player`.

// Tauri deserialises every command argument, so each must be owned.
#![allow(clippy::needless_pass_by_value, reason = "Tauri command arguments are owned")]

pub mod assistant;
pub mod autocue;
mod menu;
mod player;
mod usb;
pub mod watch;

use std::path::PathBuf;

use kfbox_core::import::ImportReport;
use kfbox_core::library::{GridEdit, TagMode};
use kfbox_core::safe::HistoryEntry;
use kfbox_core::session::{Analysis, CommandResult, CueEdit, Listing, Origin, Saved, Session, TrackRow};
use kfbox_core::{Applied, Track};
use tauri::{AppHandle, Builder, Manager, Runtime, State};

fn text<T>(result: CommandResult<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

/// Runs slow work off the main thread, with the session.
async fn blocking<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    work: impl FnOnce(&Session) -> CommandResult<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || text(work(&app.state::<Session>()))).await.map_err(|e| e.to_string())?
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
    let listing = blocking(app.clone(), Session::open_library).await?;
    autocue::kick(&app);
    Ok(listing)
}

#[tauri::command]
fn auto_cue_enabled(session: State<'_, Session>) -> bool {
    session.auto_cue_enabled()
}

/// Turns auto-cue on or off; on, it looks for tracks to cue straight away.
#[tauri::command]
fn set_auto_cue<R: Runtime>(on: bool, app: AppHandle<R>) -> Result<bool, String> {
    text(app.state::<Session>().set_auto_cue(on))?;
    if on {
        autocue::kick(&app);
    }
    Ok(on)
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

/// Writes the cues the rules place on these tracks, in one change.
#[tauri::command]
async fn write_rule_cues<R: Runtime>(ids: Vec<String>, rules: String, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.write_rule_cues(&ids, Some(&rules))).await
}

/// Adds dropped songs, and the songs in dropped folders, to the library where
/// they are: analysed and cued, copies and files already there left out.
#[tauri::command]
async fn import_files<R: Runtime>(paths: Vec<String>, app: AppHandle<R>) -> Result<ImportReport, String> {
    let report = blocking(app.clone(), move |s| s.import_files(&paths.iter().map(PathBuf::from).collect::<Vec<_>>())).await?;
    autocue::kick(&app);
    Ok(report)
}

/// Points missing tracks at files of the same name in a folder.
#[tauri::command]
async fn relocate_missing<R: Runtime>(folder: String, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.relocate_missing(&PathBuf::from(folder))).await
}

#[tauri::command]
async fn set_rating<R: Runtime>(ids: Vec<String>, stars: u8, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.set_rating(&ids, stars)).await
}

/// My Tags from the page are the person's own choice.
#[tauri::command]
async fn set_my_tags<R: Runtime>(ids: Vec<String>, tags: Vec<String>, mode: TagMode, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.set_my_tags(&ids, &tags, mode, Origin::Hand)).await
}

#[tauri::command]
async fn create_playlist<R: Runtime>(name: String, folder: Option<usize>, ids: Vec<String>, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.create_playlist(&name, folder, &ids)).await
}

#[tauri::command]
async fn add_to_playlist<R: Runtime>(index: usize, ids: Vec<String>, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.add_to_playlist(index, &ids)).await
}

#[tauri::command]
async fn remove_from_playlist<R: Runtime>(index: usize, ids: Vec<String>, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.remove_from_playlist(index, &ids)).await
}

#[tauri::command]
async fn rename_playlist<R: Runtime>(index: usize, name: String, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.rename_playlist(index, &name)).await
}

#[tauri::command]
async fn delete_playlist<R: Runtime>(index: usize, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.delete_playlist(index)).await
}

/// A person's grid edit: the page is the only way in to one.
#[tauri::command]
async fn edit_grid<R: Runtime>(id: String, edit: GridEdit, app: AppHandle<R>) -> Result<Saved, String> {
    blocking(app, move |s| s.edit_grid(&id, edit)).await
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
        apply_rules,
        apply_rules_to,
        export_xml,
        save_cues,
        write_rule_cues,
        import_files,
        relocate_missing,
        set_rating,
        set_my_tags,
        create_playlist,
        add_to_playlist,
        remove_from_playlist,
        rename_playlist,
        delete_playlist,
        edit_grid,
        auto_cue_enabled,
        set_auto_cue,
        usb::usb_drives,
        usb::export_usb,
        usb::cancel_usb,
        undo,
        redo,
        history,
        restore,
        assistant::assistant_settings,
        assistant::set_assistant_model,
        assistant::save_api_key,
        assistant::remove_api_key,
        assistant::run_quick,
        assistant::wizard_turn,
        assistant::wizard_reset,
        assistant::has_saved_rules,
        assistant::save_rules_keeping_previous,
        assistant::restore_previous_rules,
        assistant::score_rules
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
            app.set_menu(menu::build(app)?)?;
            app.on_menu_event(|app, event| menu::forward(app, &event));
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(player::Player::default());
            app.manage(Session::open(&dir)?);
            app.manage(assistant::Assistant::new(dir, Box::new(kfbox_assist::Keychain), Box::new(kfbox_assist::HttpTransport::default())));
            autocue::start(app.handle().clone());
            watch::start(app.handle().clone());
            Ok(())
        })
        .run(context());
    if let Err(e) = result {
        eprintln!("kfbox: {e}");
        std::process::exit(1);
    }
}
