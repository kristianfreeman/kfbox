//! The app's side of the perf run (`pnpm perf`, `scripts/perf-app.mjs`).
//! With `KFBOX_PERF` set (to how long, in seconds, its track-switching phase
//! runs), the page plays its scenario and marks each phase
//! here; each mark is printed with how the main thread kept up since the
//! last one, which a probe measures by asking it to run a closure every
//! 50 ms and timing how late it ran. Nothing here runs without the variable.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

/// How often the probe asks the main thread to run.
const PROBE: Duration = Duration::from_millis(50);

/// The main thread's lateness since the last mark.
#[derive(Default)]
struct Lateness {
    probes: u32,
    worst: Duration,
    /// Runs more than 50 ms late: a stall the page would feel.
    stalls: u32,
}

static LATENESS: Mutex<Lateness> = Mutex::new(Lateness { probes: 0, worst: Duration::ZERO, stalls: 0 });

pub fn on() -> bool {
    std::env::var_os("KFBOX_PERF").is_some()
}

/// Starts the main thread probe, in a perf run only.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    if !on() {
        return;
    }
    let app = app.clone();
    let _ = std::thread::Builder::new().name("kfbox-perf-probe".into()).spawn(move || loop {
        let sent = Instant::now();
        let _ = app.run_on_main_thread(move || {
            let late = sent.elapsed();
            if let Ok(mut lateness) = LATENESS.lock() {
                lateness.probes += 1;
                lateness.worst = lateness.worst.max(late);
                if late > Duration::from_millis(50) {
                    lateness.stalls += 1;
                }
            }
        });
        std::thread::sleep(PROBE);
    });
}

/// How long the perf run's track-switching phase runs, in seconds; 0 when
/// this isn't a perf run.
#[tauri::command]
pub async fn perf_mode() -> u64 {
    std::env::var("KFBOX_PERF").ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Prints one of the page's marks with the main thread's lateness since the
/// last, for the runner to read.
#[tauri::command]
pub async fn perf_mark(name: String, data: Value) {
    if !on() {
        return;
    }
    let lateness = LATENESS.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
    let line = json!({
        "name": name,
        "data": data,
        "main_thread": {
            "probes": lateness.probes,
            "worst_ms": lateness.worst.as_secs_f64() * 1000.0,
            "stalls_over_50ms": lateness.stalls,
        },
    });
    println!("kfbox perf: {line}");
}

/// Shows or hides the window, for the hidden phase.
#[tauri::command]
pub async fn perf_window<R: Runtime>(shown: bool, app: AppHandle<R>) -> Result<(), String> {
    if !on() {
        return Ok(());
    }
    let window = app.get_webview_window("main").ok_or("no window")?;
    if shown { window.show() } else { window.hide() }.map_err(|e| e.to_string())
}

/// The end of the run: the app quits.
#[tauri::command]
pub async fn perf_done<R: Runtime>(app: AppHandle<R>) {
    if on() {
        println!("kfbox perf: {}", json!({ "name": "done" }));
        app.exit(0);
    }
}
