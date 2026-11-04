//! USB export from the page: the drives to pick from, the export itself with
//! its progress as `usb-progress` events, and a way to stop it.

use std::sync::atomic::{AtomicBool, Ordering};

use kfbox_core::session::Session;
use kfbox_core::usb::{UsbProgress, UsbReport};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Set to stop the export under way. One export runs at a time.
static STOP: AtomicBool = AtomicBool::new(false);

/// A volume to export to.
#[derive(Debug, Clone, Serialize)]
pub struct Drive {
    pub name: String,
    pub path: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub file_system: String,
    pub removable: bool,
}

/// The mounted volumes worth offering, as rbl-devices finds them.
#[tauri::command]
pub async fn usb_drives() -> Result<Vec<Drive>, String> {
    // Asking the OS can take a second while a card reader wakes.
    tauri::async_runtime::spawn_blocking(|| {
        rbl_devices::list()
            .into_iter()
            .map(|d| Drive {
                name: d.name,
                path: d.mount_point.to_string_lossy().into_owned(),
                total_bytes: d.total_bytes,
                free_bytes: d.free_bytes,
                file_system: d.file_system,
                removable: d.removable,
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Writes the chosen playlists to the drive at `destination`, reporting
/// progress as it goes.
#[tauri::command]
pub async fn export_usb<R: Runtime>(
    destination: String,
    playlists: Vec<usize>,
    file_system: Option<String>,
    app: AppHandle<R>,
) -> Result<UsbReport, String> {
    STOP.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        let session = app.state::<Session>();
        let mut progress = |p: &UsbProgress| {
            let _ = app.emit("usb-progress", p);
        };
        session
            .export_usb(std::path::Path::new(&destination), &playlists, file_system.as_deref(), &mut progress, &|| {
                STOP.load(Ordering::SeqCst)
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stops the export under way, leaving the stick as it was before it began.
#[tauri::command]
pub fn cancel_usb() {
    STOP.store(true, Ordering::SeqCst);
}
