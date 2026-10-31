//! The Tauri shell: holds the analysed tracks and the rules file, and
//! hands both to the page.
//!
//! Analysis is slow, so a track is analysed once and kept; changing the
//! rules only re-places the cues. Songs come as a playlist of rekordbox's
//! own library (read from a copy of `master.db`) or of a rekordbox XML,
//! which replaces whatever was loaded, or one file at a time.

use std::path::PathBuf;
use std::sync::Mutex;

use kfbox_core::{write_xml, Applied, RbLibrary, Rules, Track, DEFAULT_RULES};
use serde::Serialize;
use tauri::{Manager, State};

#[derive(Default)]
struct Library {
    tracks: Mutex<Vec<Track>>,
    /// The rekordbox XML last opened.
    rekordbox: Mutex<Option<RbLibrary>>,
}

/// A folder or playlist, for the picker.
#[derive(Serialize)]
struct PlaylistItem {
    index: usize,
    path: String,
    folder: bool,
    count: usize,
}

/// What a library or XML offers: its whole collection and its playlists.
#[derive(Serialize)]
struct Listing {
    total: usize,
    playlists: Vec<PlaylistItem>,
}

/// A track of the chosen playlist, before it is analysed.
#[derive(Serialize)]
struct PlaylistTrack {
    id: String,
    /// Empty when the collection entry is not a file.
    path: String,
    title: String,
}

fn rules_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("rules.toml"))
}

/// The saved rules, or the defaults before any are saved.
#[tauri::command]
fn load_rules(app: tauri::AppHandle) -> Result<String, String> {
    let path = rules_path(&app)?;
    Ok(std::fs::read_to_string(path).unwrap_or_else(|_| DEFAULT_RULES.to_owned()))
}

/// Songs named on the command line, to load at start.
#[tauri::command]
fn initial_paths() -> Vec<String> {
    std::env::args().skip(1).filter(|a| std::path::Path::new(a).is_file()).collect()
}

#[tauri::command]
fn default_rules() -> String {
    DEFAULT_RULES.to_owned()
}

/// Decodes and analyses a song and keeps it. A song already loaded is
/// analysed again and replaced.
#[tauri::command]
async fn analyse(path: String, library: State<'_, Library>) -> Result<Track, String> {
    let path = PathBuf::from(path);
    let track = tauri::async_runtime::spawn_blocking(move || Track::analyse(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    keep(&library, &track)?;
    Ok(track)
}

/// Reads a rekordbox XML and lists its folders and playlists.
#[tauri::command]
async fn open_rekordbox(path: String, library: State<'_, Library>) -> Result<Listing, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed = RbLibrary::parse(&text).map_err(|e| e.to_string())?;
    keep_library(&library, parsed)
}

/// Reads rekordbox's own library, from a copy of `master.db`, and lists
/// its folders and playlists.
#[tauri::command]
async fn open_rekordbox_library(library: State<'_, Library>) -> Result<Listing, String> {
    let parsed = tauri::async_runtime::spawn_blocking(kfbox_core::master::load_installed)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    keep_library(&library, parsed)
}

fn keep_library(library: &Library, parsed: RbLibrary) -> Result<Listing, String> {
    let total = parsed.all_tracks().len();
    let playlists = parsed
        .playlists
        .iter()
        .enumerate()
        .map(|(index, p)| PlaylistItem { index, path: p.path.clone(), folder: p.folder, count: p.track_ids.len() })
        .collect();
    *library.rekordbox.lock().map_err(|e| e.to_string())? = Some(parsed);
    Ok(Listing { total, playlists })
}

/// Makes a playlist of the open library or XML the songs being worked on,
/// in its order, or the whole collection when `index` is `None`. Nothing is
/// analysed yet.
#[tauri::command]
fn load_playlist(index: Option<usize>, library: State<'_, Library>) -> Result<Vec<PlaylistTrack>, String> {
    let rekordbox = library.rekordbox.lock().map_err(|e| e.to_string())?;
    let parsed = rekordbox.as_ref().ok_or("no rekordbox library or XML is open")?;
    library.tracks.lock().map_err(|e| e.to_string())?.clear();
    let tracks = index.map_or_else(|| parsed.all_tracks(), |i| parsed.playlist_tracks(i));
    Ok(tracks
        .iter()
        .map(|t| PlaylistTrack {
            id: t.id.clone(),
            path: t.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            title: [t.attribute("Artist"), t.attribute("Name")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" – "),
        })
        .collect())
}

/// Decodes a track of the open rekordbox XML, on its own grid where it has
/// one, and keeps it with its cues.
#[tauri::command]
async fn analyse_rekordbox(id: String, library: State<'_, Library>) -> Result<Track, String> {
    let source = {
        let rekordbox = library.rekordbox.lock().map_err(|e| e.to_string())?;
        rekordbox.as_ref().and_then(|r| r.track(&id)).cloned().ok_or("the track is not in the open rekordbox XML")?
    };
    let track = tauri::async_runtime::spawn_blocking(move || Track::from_rekordbox(&source))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    keep(&library, &track)?;
    Ok(track)
}

fn keep(library: &Library, track: &Track) -> Result<(), String> {
    let mut tracks = library.tracks.lock().map_err(|e| e.to_string())?;
    tracks.retain(|t| t.path != track.path);
    tracks.push(track.clone());
    Ok(())
}

#[tauri::command]
fn remove(path: String, library: State<'_, Library>) -> Result<(), String> {
    let path = PathBuf::from(path);
    library.tracks.lock().map_err(|e| e.to_string())?.retain(|t| t.path != path);
    Ok(())
}

/// Checks the rules and places the cues on every loaded track, keyed by
/// path. Saves them only when asked: a track finishing its analysis also
/// applies them, and that must not write over the saved file.
#[tauri::command]
fn apply_rules(
    rules: String,
    save: bool,
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Vec<(String, Applied)>, String> {
    let parsed = Rules::parse(&rules).map_err(|e| e.to_string())?;
    if save {
        let path = rules_path(&app)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &rules).map_err(|e| e.to_string())?;
    }
    let tracks = library.tracks.lock().map_err(|e| e.to_string())?;
    Ok(tracks.iter().map(|t| (t.path.to_string_lossy().into_owned(), parsed.apply(t))).collect())
}

/// Writes every loaded track with its cues to `dest`. Returns how many.
#[tauri::command]
fn export_xml(dest: String, rules: String, library: State<'_, Library>) -> Result<usize, String> {
    let parsed = Rules::parse(&rules).map_err(|e| e.to_string())?;
    let tracks = library.tracks.lock().map_err(|e| e.to_string())?;
    let applied: Vec<Applied> = tracks.iter().map(|t| parsed.apply(t)).collect();
    let pairs: Vec<(&Track, &[kfbox_core::Cue])> = tracks.iter().zip(&applied).map(|(t, a)| (t, a.cues.as_slice())).collect();
    std::fs::write(&dest, write_xml(&pairs)).map_err(|e| e.to_string())?;
    Ok(pairs.len())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Library::default())
        .invoke_handler(tauri::generate_handler![load_rules,
            initial_paths,
            default_rules,
            open_rekordbox,
            open_rekordbox_library,
            load_playlist,
            analyse,
            analyse_rekordbox,
            remove,
            apply_rules,
            export_xml])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("kfbox: {e}");
        std::process::exit(1);
    }
}
