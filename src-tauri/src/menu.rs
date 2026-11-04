//! The macOS menu bar. Each kfbox item sends its ID to the page as a `menu`
//! event, and the page runs the same command its palette and keys do.

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Emitter, Runtime};

/// kfbox's own items: ID, label and shortcut.
const FILE: [(&str, &str, Option<&str>); 5] = [
    ("open_xml", "Open rekordbox XML…", Some("CmdOrCtrl+O")),
    ("add_songs", "Import Songs…", Some("Shift+CmdOrCtrl+O")),
    ("reload", "Reload Library", None),
    ("export", "Export XML…", Some("CmdOrCtrl+E")),
    ("export_usb", "Export to USB…", Some("Shift+CmdOrCtrl+E")),
];

/// The main area's views, and the rules wizard: Rules and History live here
/// and in ⌘K rather than in the sidebar.
const VIEW: [(&str, &str, Option<&str>); 5] = [
    ("toggle_sidebar", "Hide Sidebar", Some("Ctrl+Super+S")),
    ("view_tracks", "Tracks", Some("CmdOrCtrl+1")),
    ("view_rules", "Rules", Some("CmdOrCtrl+2")),
    ("view_history", "History", Some("CmdOrCtrl+3")),
    ("wizard", "Rules Wizard…", None),
];

/// Undo and redo here are the library's: the page sends them on to a text
/// field instead when one has focus.
const EDIT: [(&str, &str, Option<&str>); 2] = [("undo", "Undo", Some("CmdOrCtrl+Z")), ("redo", "Redo", Some("Shift+CmdOrCtrl+Z"))];

fn item<R: Runtime>(app: &App<R>, (id, label, keys): (&str, &str, Option<&str>)) -> tauri::Result<tauri::menu::MenuItem<R>> {
    let builder = MenuItemBuilder::with_id(id, label);
    match keys {
        Some(keys) => builder.accelerator(keys).build(app),
        None => builder.build(app),
    }
}

/// Builds the menu bar.
pub fn build<R: Runtime>(app: &App<R>) -> tauri::Result<Menu<R>> {
    let settings = item(app, ("settings", "Settings…", Some("CmdOrCtrl+,")))?;
    let app_menu = SubmenuBuilder::new(app, "kfbox")
        .about(None)
        .separator()
        .item(&settings)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let mut file = SubmenuBuilder::new(app, "File");
    for entry in FILE {
        file = file.item(&item(app, entry)?);
    }
    let file = file.separator().close_window().build()?;
    let mut edit = SubmenuBuilder::new(app, "Edit");
    for entry in EDIT {
        edit = edit.item(&item(app, entry)?);
    }
    // The standard text items, so copy and paste work in text fields.
    let edit = edit.separator().cut().copy().paste().select_all().build()?;
    let mut view = SubmenuBuilder::new(app, "View");
    for entry in VIEW {
        view = view.item(&item(app, entry)?);
    }
    let view = view.build()?;
    let window = SubmenuBuilder::new(app, "Window").minimize().maximize().separator().fullscreen().build()?;
    MenuBuilder::new(app).items(&[&app_menu, &file, &edit, &view, &window]).build()
}

/// Sends kfbox's menu items to the page. The standard items handle themselves.
pub fn forward<R: Runtime>(app: &tauri::AppHandle<R>, event: &tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    let ours = id == "settings" || FILE.iter().chain(&EDIT).chain(&VIEW).any(|(known, _, _)| *known == id);
    if ours {
        let _ = app.emit("menu", id);
    }
}

/// Names the sidebar item for what it will do: Hide Sidebar while the
/// sidebar shows, Show Sidebar while it's hidden.
///
/// Async on purpose: a command that isn't runs on the main thread, and a
/// menu call waits for the main thread, so it would wait on itself and the
/// window would freeze, blank if it happened at launch.
#[tauri::command]
pub async fn sidebar_shown<R: Runtime>(app: tauri::AppHandle<R>, shown: bool) -> Result<(), String> {
    let Some(menu) = app.menu() else { return Ok(()) };
    for entry in menu.items().map_err(|e| e.to_string())? {
        if let Some(item) = entry.as_submenu().and_then(|view| view.get("toggle_sidebar")).and_then(|item| item.as_menuitem().cloned()) {
            item.set_text(if shown { "Hide Sidebar" } else { "Show Sidebar" }).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
