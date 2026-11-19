//! Automatic recording when Teams, Zoom, or Google Meet is in a call.
//!
//! macOS observation uses `CGWindowListCopyWindowInfo` and Core Audio
//! `System::processes` / `is_running_input` (the same cidre bindings the
//! system-audio detector already uses). Other platforms observe nothing.

mod detect;
mod machine;
mod service;
mod settings;
mod snapshot;

pub use service::{current_watch, dismiss, get_settings, save, start, Watch};

#[tauri::command]
pub async fn get_auto_record_settings(
    app: tauri::AppHandle<tauri::Wry>,
) -> Result<settings::AutoRecordSettings, String> {
    get_settings(app).await
}

#[tauri::command]
pub async fn save_auto_record_settings(
    app: tauri::AppHandle<tauri::Wry>,
    enabled: bool,
) -> Result<settings::AutoRecordSettings, String> {
    save(app, enabled).await
}

#[tauri::command]
pub async fn dismiss_auto_recording(app: tauri::AppHandle<tauri::Wry>) -> Result<(), String> {
    dismiss(app).await
}

#[tauri::command]
pub async fn auto_record_watch() -> Watch {
    current_watch()
}
