//! The macOS menu bar. Each kfbox item sends its ID to the page as a `menu`
//! event, and the page runs the same command its palette and keys do.

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Emitter, Runtime};

/// kfbox's own items: ID, label and shortcut.
const FILE: [(&str, &str, Option<&str>); 4] = [
    ("open_xml", "Open rekordbox XML…", Some("CmdOrCtrl+O")),
    ("add_songs", "Add Songs…", Some("Shift+CmdOrCtrl+O")),
    ("reload", "Reload Library", None),
    ("export", "Export XML…", Some("CmdOrCtrl+E")),
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
    let window = SubmenuBuilder::new(app, "Window").minimize().maximize().separator().fullscreen().build()?;
    MenuBuilder::new(app).items(&[&app_menu, &file, &edit, &window]).build()
}

/// Sends kfbox's menu items to the page. The standard items handle themselves.
pub fn forward<R: Runtime>(app: &tauri::AppHandle<R>, event: &tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    let ours = id == "settings" || FILE.iter().chain(&EDIT).any(|(known, _, _)| *known == id);
    if ours {
        let _ = app.emit("menu", id);
    }
}
