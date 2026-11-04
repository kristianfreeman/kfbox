//! The app's auto-cue worker: one background thread that cues tracks with
//! empty slots when the library opens and whenever something brings in or
//! changes tracks, and tells the page how it's going as `auto-cue` events.
//! While rekordbox is open it waits, looking again every few seconds and
//! whenever the library changes, which it does when rekordbox quits.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

use kfbox_core::autocue::AutoCueProgress;
use kfbox_core::session::{Session, TrackRow};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// How often a worker waiting for rekordbox to close looks again.
const WAITING_LOOK: Duration = Duration::from_secs(4);

/// What the page hears.
#[derive(Debug, Clone, Serialize)]
pub struct AutoCueEvent {
    /// `working`, `waiting` (for rekordbox to close), `done` or `off`.
    pub state: &'static str,
    pub done: usize,
    pub total: usize,
    pub cued: usize,
    pub waiting: usize,
    /// Tracks just written, to update on screen.
    pub changed: Vec<TrackRow>,
}

#[derive(Default)]
struct Flags {
    asked: bool,
    waiting: bool,
}

/// The worker's doorbell.
#[derive(Default)]
pub struct AutoCue {
    flags: Mutex<Flags>,
    bell: Condvar,
}

fn flags(cue: &AutoCue) -> std::sync::MutexGuard<'_, Flags> {
    cue.flags.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Asks the worker to look for tracks to cue. Cheap, and safe to call often.
pub fn kick<R: Runtime>(app: &AppHandle<R>) {
    if let Some(cue) = app.try_state::<AutoCue>() {
        flags(&cue).asked = true;
        cue.bell.notify_one();
    }
}

/// One look: cues what needs it, if auto-cue is on and a library is open.
pub fn run_once<R: Runtime>(app: &AppHandle<R>) -> Option<AutoCueEvent> {
    let session = app.state::<Session>();
    if !session.auto_cue_enabled() {
        return Some(AutoCueEvent { state: "off", done: 0, total: 0, cued: 0, waiting: 0, changed: Vec::new() });
    }
    // No library open yet, or an XML: nothing to cue.
    let ids = session.auto_cue_candidates().ok()?;
    if ids.is_empty() {
        return None;
    }
    let mut progress = |p: &AutoCueProgress| {
        let event = AutoCueEvent { state: "working", done: p.done, total: p.total, cued: p.cued, waiting: 0, changed: p.changed.clone() };
        let _ = app.emit("auto-cue", event);
    };
    let outcome = session.auto_cue(&ids, &mut progress, &|| !session.auto_cue_enabled()).ok()?;
    let state = if outcome.waiting > 0 { "waiting" } else { "done" };
    Some(AutoCueEvent { state, done: ids.len(), total: ids.len(), cued: outcome.cued, waiting: outcome.waiting, changed: Vec::new() })
}

/// Starts the worker; it runs for as long as the app does.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    app.manage(AutoCue::default());
    let _ = std::thread::Builder::new().name("kfbox-auto-cue".into()).spawn(move || loop {
        {
            let cue = app.state::<AutoCue>();
            let mut held = flags(&cue);
            while !held.asked {
                if held.waiting {
                    let (next, timeout) = cue.bell.wait_timeout(held, WAITING_LOOK).unwrap_or_else(std::sync::PoisonError::into_inner);
                    held = next;
                    if timeout.timed_out() {
                        break;
                    }
                } else {
                    held = cue.bell.wait(held).unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
            held.asked = false;
        }
        let event = run_once(&app);
        flags(&app.state::<AutoCue>()).waiting = event.as_ref().is_some_and(|e| e.state == "waiting");
        if let Some(event) = event {
            let _ = app.emit("auto-cue", event);
        }
    });
}
