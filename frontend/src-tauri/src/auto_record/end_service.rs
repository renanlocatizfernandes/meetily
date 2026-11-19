//! Runs the meeting-end machine on the detector loop: reads the speech stamp
//! and the settings, shows or withdraws the warning, and stops the recording
//! through the same path the automatic stop already uses.

use std::sync::Mutex;

use log::{info, warn};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::audio::{recording_commands, speech_activity};
use crate::notifications::commands::NotificationManagerState;
use crate::notifications::types::{Notification, NotificationPriority, NotificationTimeout, NotificationType};
use crate::state::AppState;

use super::detect::MeetingApp;
use super::end_machine::{CancelCause, EndAction, EndAnswer, EndInput, EndMachine, EndReason, StopCause};
use super::end_settings::{self, MeetingEndSettings};

/// The open "the meeting seems to be over" warning.
struct PendingEnd {
    id: u64,
    answer: Option<EndAnswer>,
}

static PENDING: Mutex<Option<PendingEnd>> = Mutex::new(None);

fn open_prompt() -> u64 {
    let id = super::service::next_prompt_id();
    if let Ok(mut slot) = PENDING.lock() {
        *slot = Some(PendingEnd { id, answer: None });
    }
    id
}

/// The answer of the open warning, if the user already gave one. The warning
/// stays open until the machine acts on it.
fn pending_answer() -> Option<EndAnswer> {
    PENDING.lock().ok()?.as_ref()?.answer
}

fn close_prompt() -> Option<u64> {
    PENDING.lock().ok()?.take().map(|pending| pending.id)
}

/// Answers the open warning. `action` is "stop" or "continue".
pub fn respond(prompt_id: u64, action: &str) -> Result<(), String> {
    let answer = match action.trim().to_ascii_lowercase().as_str() {
        "stop" => EndAnswer::Stop,
        "continue" => EndAnswer::Continue,
        _ => return Err("Resposta inválida. Use stop ou continue.".to_string()),
    };
    {
        let mut slot = PENDING
            .lock()
            .map_err(|_| "Não foi possível responder ao aviso de fim de reunião.".to_string())?;
        match slot.as_mut() {
            Some(pending) if pending.id == prompt_id && pending.answer.is_none() => {
                pending.answer = Some(answer);
            }
            _ => return Err("Este aviso de fim de reunião não está mais ativo.".to_string()),
        }
    }
    super::service::wake();
    Ok(())
}

pub async fn get_settings(app: &AppHandle<Wry>) -> Result<MeetingEndSettings, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?;
    end_settings::load(state.db_manager.pool())
        .await
        .map_err(|_| "Não foi possível ler a detecção de fim de reunião.".to_string())
}

/// Saves the mode and, when given, the two times. A missing time keeps its stored value.
pub async fn save_settings(
    app: &AppHandle<Wry>,
    mode: &str,
    silence_minutes: Option<u32>,
    prompt_seconds: Option<u32>,
) -> Result<MeetingEndSettings, String> {
    let mode = super::end_machine::EndMode::parse(mode)
        .ok_or_else(|| "Modo de fim de reunião inválido. Use off, ask ou auto.".to_string())?;
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?;
    let pool = state.db_manager.pool();
    let current = end_settings::load(pool)
        .await
        .map_err(|_| "Não foi possível ler a detecção de fim de reunião.".to_string())?;
    let next = MeetingEndSettings::new(
        mode,
        silence_minutes.unwrap_or(current.silence_minutes),
        prompt_seconds.unwrap_or(current.prompt_seconds),
    );
    end_settings::save(pool, &next)
        .await
        .map_err(|_| "Não foi possível salvar a detecção de fim de reunião.".to_string())?;
    super::service::wake();
    Ok(next)
}

#[derive(Default)]
pub(super) struct EndWatcher {
    machine: EndMachine,
}

impl EndWatcher {
    /// One detector tick. Returns the cause when this tick stopped the recording.
    pub(super) async fn tick(
        &mut self,
        app: &AppHandle<Wry>,
        signal: Option<MeetingApp>,
        recording: bool,
        owned: bool,
    ) -> Option<StopCause> {
        if !recording && self.machine.is_idle() {
            return None;
        }
        // Only read while something is being recorded.
        let settings = if recording {
            load_settings(app).await
        } else {
            MeetingEndSettings::default()
        };
        let paused = recording && recording_commands::is_recording_paused().await;
        let action = self.machine.step(&EndInput {
            now_ms: speech_activity::now_ms(),
            recording,
            paused,
            owned,
            signal: signal.is_some(),
            last_speech_ms: speech_activity::last_speech_ms(),
            mode: settings.mode(),
            silence_limit_ms: u64::from(settings.silence_minutes) * 60_000,
            prompt_timeout_ms: u64::from(settings.prompt_seconds) * 1_000,
            answer: pending_answer(),
        });
        match action? {
            EndAction::Prompt(reason) => {
                let prompt_id = open_prompt();
                let _ = app.emit(
                    "meeting-end-detected",
                    serde_json::json!({
                        "promptId": prompt_id,
                        "reason": reason.as_str(),
                        "timeoutSeconds": settings.prompt_seconds,
                        "silenceMinutes": settings.silence_minutes,
                    }),
                );
                info!("Meeting end detected; reason={}, outcome=asking", reason.as_str());
                notify(app, reason, &settings).await;
                None
            }
            EndAction::Cancel(cause) => {
                emit_cancelled(app, cause.as_str());
                if cause != CancelCause::RecordingStopped {
                    info!("Meeting end warning withdrawn; reason={}", cause.as_str());
                }
                None
            }
            EndAction::Stop(cause) => {
                emit_cancelled(app, "stopping");
                if super::service::stop_recording_now(app).await {
                    info!("Recording stopped because the meeting ended; reason={}", cause.as_str());
                    let _ = app.emit(
                        "meeting-end-stopped",
                        serde_json::json!({ "reason": cause.as_str() }),
                    );
                    Some(cause)
                } else {
                    warn!("Meeting end stop did not take effect; reason={}", cause.as_str());
                    None
                }
            }
        }
    }
}

fn emit_cancelled(app: &AppHandle<Wry>, reason: &str) {
    if let Some(prompt_id) = close_prompt() {
        let _ = app.emit(
            "meeting-end-cancelled",
            serde_json::json!({ "promptId": prompt_id, "reason": reason }),
        );
    }
}

async fn load_settings(app: &AppHandle<Wry>) -> MeetingEndSettings {
    match get_settings(app).await {
        Ok(settings) => settings,
        Err(_) => {
            // Without the stored choice, do not stop anything.
            warn!("Meeting end settings were not read; detection is off for this tick");
            MeetingEndSettings::new(
                super::end_machine::EndMode::Off,
                end_settings::DEFAULT_SILENCE_MINUTES,
                end_settings::DEFAULT_PROMPT_SECONDS,
            )
        }
    }
}

/// System notification through the app's notification manager. It has no
/// buttons (the desktop notification plugin does not offer them): the answer
/// is given in the app.
async fn notify(app: &AppHandle<Wry>, reason: EndReason, settings: &MeetingEndSettings) {
    let Some(state) = app.try_state::<NotificationManagerState<Wry>>() else {
        return;
    };
    let evidence = match reason {
        EndReason::Signal => "A chamada terminou.".to_string(),
        EndReason::Silence => "Ninguém fala há algum tempo.".to_string(),
    };
    let body = format!(
        "{evidence} A gravação encerra em {} s. Abra o Meetily Live Plus para continuar gravando.",
        settings.prompt_seconds
    );
    let notification = Notification::new("A reunião parece ter terminado", body, NotificationType::MeetingEndDetected)
        .with_priority(NotificationPriority::High)
        .with_timeout(NotificationTimeout::Seconds(u64::from(settings.prompt_seconds)));
    let manager = state.read().await;
    match manager.as_ref() {
        Some(manager) => {
            if manager.show_notification(notification).await.is_err() {
                warn!("Meeting end notification failed");
            }
        }
        None => info!("Meeting end notification skipped; manager not initialized"),
    }
}
