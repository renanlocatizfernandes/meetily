//! Automatic recording when Teams, Zoom, or Google Meet is in a call.
//!
//! macOS observation uses `CGWindowListCopyWindowInfo` and Core Audio
//! `System::processes` / `is_running_input` (the same cidre bindings the
//! system-audio detector already uses). Other platforms observe nothing.

mod detect;
mod end_machine;
mod end_service;
pub(crate) mod end_settings;
mod machine;
mod service;
pub(crate) mod settings;
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
    mode: Option<String>,
) -> Result<settings::AutoRecordSettings, String> {
    save(app, enabled, mode).await
}

/// Answers the `auto-record-prompt` of the "ask" mode.
#[tauri::command]
pub async fn confirm_auto_recording(prompt_id: u64, accept: bool) -> Result<(), String> {
    service::answer_prompt(prompt_id, accept)
}

#[tauri::command]
pub async fn dismiss_auto_recording(app: tauri::AppHandle<tauri::Wry>) -> Result<(), String> {
    dismiss(app).await
}

#[tauri::command]
pub async fn auto_record_watch() -> Watch {
    current_watch()
}

#[tauri::command]
pub async fn get_meeting_end_settings(
    app: tauri::AppHandle<tauri::Wry>,
) -> Result<end_settings::MeetingEndSettings, String> {
    end_service::get_settings(&app).await
}

/// `mode` is "off", "ask" or "auto". A missing time keeps its stored value.
#[tauri::command]
pub async fn save_meeting_end_settings(
    app: tauri::AppHandle<tauri::Wry>,
    mode: String,
    silence_minutes: Option<u32>,
    prompt_seconds: Option<u32>,
) -> Result<end_settings::MeetingEndSettings, String> {
    end_service::save_settings(&app, &mode, silence_minutes, prompt_seconds).await
}

/// Answers the `meeting-end-detected` warning. `action` is "stop" or "continue".
#[tauri::command]
pub async fn respond_meeting_end(prompt_id: u64, action: String) -> Result<(), String> {
    end_service::respond(prompt_id, &action)
}
