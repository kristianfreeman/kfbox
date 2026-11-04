//! Live reload: notices when something else writes to the library, such as an
//! agent through `kfbox-mcp` or rekordbox itself, and tells the page which
//! tracks changed. kfbox's own writes already update the session, so they
//! don't show up here twice.

use std::time::Duration;

use kfbox_core::session::Session;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// How often the library and the history are looked at. Each look is a
/// `stat` of two files; the library is only read again when one changed.
const EVERY: Duration = Duration::from_millis(700);

/// One look: emits `library-changed` with what changed, or `history-changed`
/// when only the history moved. Returns whether anything was emitted.
pub fn check<R: Runtime>(app: &AppHandle<R>, history: &mut Option<(std::time::SystemTime, u64)>) -> bool {
    let session = app.state::<Session>();
    let marker = session.history_marker();
    let history_moved = marker != *history;
    *history = marker;
    match session.refresh_if_changed() {
        Ok(Some(change)) => {
            // New or changed tracks may want cues; rekordbox quitting lands here too.
            crate::autocue::kick(app);
            app.emit("library-changed", change).is_ok()
        }
        // A library caught mid-write by rekordbox reads again next time.
        Ok(None) | Err(_) if history_moved => app.emit("history-changed", ()).is_ok(),
        Ok(None) | Err(_) => false,
    }
}

/// Looks every [`EVERY`] for as long as the app runs.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    let _ = std::thread::Builder::new().name("kfbox-watch".into()).spawn(move || {
        let mut history = app.state::<Session>().history_marker();
        loop {
            std::thread::sleep(EVERY);
            check(&app, &mut history);
        }
    });
}
