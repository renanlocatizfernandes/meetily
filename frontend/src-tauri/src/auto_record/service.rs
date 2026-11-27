use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{info, warn};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::audio::recording_commands::{self, RecordingArgs, StopOutcome};
use crate::notifications::commands::{show_recording_started_notification, NotificationManagerState};
use crate::state::AppState;

use super::detect::{self, MeetingApp};
use super::end_service::EndWatcher;
use super::machine::{Action, DetectorMachine, Phase, StepInput};
use super::settings::{self, mode_after_toggle, AutoRecordSettings, Mode};
use super::snapshot;

static USER_HOLD: AtomicBool = AtomicBool::new(false);
static OWNED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static WATCH: Mutex<Watch> = Mutex::new(Watch {
    active: false,
    app: None,
    phase: "idle",
    hint: "Nenhuma reunião detectada. Abra o Teams, o Zoom ou o Meet, entre numa chamada e acompanhe o status aqui.",
    seconds_left: None,
});

/// The open "record this call?" prompt of the "ask" mode.
struct PendingPrompt {
    id: u64,
    answer: Option<bool>,
}

static PROMPT: Mutex<Option<PendingPrompt>> = Mutex::new(None);
static NEXT_PROMPT_ID: AtomicU64 = AtomicU64::new(1);
/// Wakes the detector loop as soon as a prompt is answered.
static WAKE: once_cell::sync::Lazy<tokio::sync::Notify> = once_cell::sync::Lazy::new(tokio::sync::Notify::new);

const HINT_DISABLED: &str = "Gravação automática desligada nas configurações.";
const HINT_PRIVACY: &str = "Gravação automática em espera: falta responder à escolha de privacidade.";
const HINT_PROMPT: &str = "Reunião detectada. Aguardando a sua confirmação para gravar.";

/// Prompt ids are unique across the "record?" and the "meeting ended?" warnings.
pub(super) fn next_prompt_id() -> u64 {
    NEXT_PROMPT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Runs the next detector tick right away.
pub(super) fn wake() {
    WAKE.notify_one();
}

fn open_prompt() -> u64 {
    let id = next_prompt_id();
    if let Ok(mut slot) = PROMPT.lock() {
        *slot = Some(PendingPrompt { id, answer: None });
    }
    id
}

/// Takes the answer of the open prompt, if the user already gave one.
fn take_answer() -> Option<bool> {
    let mut slot = PROMPT.lock().ok()?;
    let answer = slot.as_ref()?.answer?;
    *slot = None;
    Some(answer)
}

/// Closes a prompt nobody answered. Returns its id.
fn close_unanswered_prompt() -> Option<u64> {
    let mut slot = PROMPT.lock().ok()?;
    let id = slot.as_ref()?.id;
    *slot = None;
    Some(id)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Watch {
    pub active: bool,
    pub app: Option<String>,
    pub phase: &'static str,
    pub hint: &'static str,
    pub seconds_left: Option<u64>,
}

pub fn start(app: AppHandle<Wry>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut machine = DetectorMachine::default();
        let origin = Instant::now();
        let mut owned = false;
        let mut end_watcher = EndWatcher::default();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = WAKE.notified() => {}
            }
            let user_hold = USER_HOLD.load(Ordering::SeqCst);
            let (mode, privacy_pending) = load_effective_mode(&app).await;
            let enabled = mode != Mode::Off;
            let (windows, inputs) = snapshot::capture();
            let detection = detect::inspect(&windows, &inputs);
            let signal = detection.app;
            let recording = recording_commands::is_recording().await;
            if user_hold && signal.is_none() && !recording {
                USER_HOLD.store(false, Ordering::SeqCst);
            }
            let action = machine.step(&StepInput {
                now: Instant::now(),
                origin,
                signal,
                recording,
                owned,
                enabled,
                user_hold: USER_HOLD.load(Ordering::SeqCst),
                ask: mode == Mode::Ask,
                answer: take_answer(),
            });
            match action {
                Some(Action::Prompt(app_kind)) => {
                    let prompt_id = open_prompt();
                    let _ = app.emit(
                        "auto-record-prompt",
                        serde_json::json!({
                            "promptId": prompt_id,
                            "app": app_kind.label(),
                        }),
                    );
                    info!("Automatic recording is waiting for confirmation");
                }
                Some(Action::Start(app_kind)) if !USER_HOLD.load(Ordering::SeqCst) => {
                    let title = if app_kind == MeetingApp::Teams {
                        detect::teams_meeting_title(&windows)
                    } else {
                        None
                    };
                    match begin(&app, app_kind, title).await {
                        Ok(()) => {
                            owned = true;
                            OWNED.store(true, Ordering::SeqCst);
                            emit_status(&app, true, Some(app_kind.label()));
                        }
                        Err(error) => warn!("Automatic recording did not start: {error}"),
                    }
                }
                Some(Action::Stop) if owned => {
                    if stop_owned(&app).await {
                        owned = false;
                        OWNED.store(false, Ordering::SeqCst);
                        emit_status(&app, false, None);
                    }
                }
                _ => {
                    if !recording {
                        owned = false;
                        OWNED.store(false, Ordering::SeqCst);
                    }
                }
            }
            if !matches!(machine.phase(), Phase::Prompting { .. }) {
                // The call ended, a recording started by hand, or the mode changed.
                if let Some(prompt_id) = close_unanswered_prompt() {
                    let _ = app.emit(
                        "auto-record-prompt-cancelled",
                        serde_json::json!({ "promptId": prompt_id }),
                    );
                }
            }
            // End of the meeting, for any recording: call gone or long silence.
            // Read the recording state again: the block above may have stopped it.
            let recording_now = recording_commands::is_recording().await;
            if end_watcher.tick(&app, signal, recording_now, owned).await.is_some() && owned {
                owned = false;
                OWNED.store(false, Ordering::SeqCst);
                emit_status(&app, false, None);
            }
            let now = Instant::now();
            publish_watch(Watch {
                active: owned && recording_commands::is_recording().await,
                app: signal.map(MeetingApp::label).map(str::to_string),
                phase: machine.phase_name(),
                hint: if privacy_pending {
                    HINT_PRIVACY
                } else if !enabled {
                    HINT_DISABLED
                } else if matches!(machine.phase(), Phase::Prompting { .. }) {
                    HINT_PROMPT
                } else {
                    detection.hint
                },
                seconds_left: machine.seconds_until_start(now, origin),
            }, &app);
        }
    });
}

fn publish_watch(watch: Watch, app: &AppHandle<Wry>) {
    if let Ok(mut slot) = WATCH.lock() {
        *slot = watch.clone();
    }
    let _ = app.emit("auto-record-watch", &watch);
}

pub fn current_watch() -> Watch {
    WATCH.lock().map(|watch| watch.clone()).unwrap_or_else(|_| Watch {
        active: false,
        app: None,
        phase: "idle",
        hint: "Ainda não foi possível observar as reuniões.",
        seconds_left: None,
    })
}

pub async fn dismiss(app: AppHandle<Wry>) -> Result<(), String> {
    USER_HOLD.store(true, Ordering::SeqCst);
    if OWNED.load(Ordering::SeqCst) || recording_commands::is_recording().await {
        // Only stop a recording this detector started. A manual recording stays up.
        if OWNED.load(Ordering::SeqCst) {
            stop_owned(&app).await;
            OWNED.store(false, Ordering::SeqCst);
            emit_status(&app, false, None);
        }
    }
    Ok(())
}

/// Answers the open "ask" prompt. Accepting starts the recording on the
/// detector loop; refusing keeps this call quiet until it ends.
pub fn answer_prompt(prompt_id: u64, accept: bool) -> Result<(), String> {
    {
        let mut slot = PROMPT
            .lock()
            .map_err(|_| "Não foi possível responder ao aviso de gravação.".to_string())?;
        match slot.as_mut() {
            Some(pending) if pending.id == prompt_id && pending.answer.is_none() => {
                pending.answer = Some(accept);
            }
            _ => return Err("Este aviso de gravação não está mais ativo.".to_string()),
        }
    }
    WAKE.notify_one();
    Ok(())
}

/// The stored mode, forced to off until the one-time privacy choice is made.
/// The second value says whether that choice is still pending.
async fn load_effective_mode(app: &AppHandle<Wry>) -> (Mode, bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return (Mode::Off, false);
    };
    let pool = state.db_manager.pool();
    match crate::assist_plus::privacy::is_done(pool).await {
        Ok(true) => {}
        Ok(false) => return (Mode::Off, true),
        Err(error) => {
            warn!("Privacy choice was not read ({error})");
            return (Mode::Off, false);
        }
    }
    match settings::load_settings(pool).await {
        Ok(settings) => (settings.mode(), false),
        Err(error) => {
            warn!("Automatic recording settings were not read ({error})");
            (Mode::Off, false)
        }
    }
}

async fn begin(app: &AppHandle<Wry>, kind: MeetingApp, title: Option<String>) -> Result<(), String> {
    if USER_HOLD.load(Ordering::SeqCst) || recording_commands::is_recording().await {
        return Err("recording is already active or automatic start is on hold".to_string());
    }
    // O nome da reunião no Teams, quando a janela mostra um; senão o do app.
    let meeting_name = title.unwrap_or_else(|| format!("{} (gravação automática)", kind.label()));
    recording_commands::start_recording_with_devices_and_meeting(
        app.clone(),
        None,
        None,
        Some(meeting_name.clone()),
    )
    .await?;
    crate::set_recording_flag(true);
    let notifications = app.state::<NotificationManagerState<Wry>>();
    if let Err(error) = show_recording_started_notification(app, &notifications, Some(meeting_name)).await {
        warn!("Automatic recording notification failed: {error}");
    }
    info!("Automatic recording started for {}", kind.label());
    Ok(())
}

/// Stops whatever is being recorded through the normal stop path (channels
/// drained and flushed, file saved, post-processing triggered).
pub(super) async fn stop_recording_now(app: &AppHandle<Wry>) -> bool {
    stop_owned(app).await
}

async fn stop_owned(app: &AppHandle<Wry>) -> bool {
    let Ok(data_dir) = app.path().app_data_dir() else {
        warn!("Automatic recording could not resolve the save folder");
        return false;
    };
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S");
    let save_path = data_dir.join(format!("recording-{timestamp}.wav"));
    match recording_commands::stop_recording(
        app.clone(),
        RecordingArgs {
            save_path: save_path.to_string_lossy().to_string(),
        },
    )
    .await
    {
        Ok(StopOutcome::Stopped) => {
            crate::set_recording_flag(false);
            let _ = app.emit("recording-stop-complete", true);
            info!("Automatic recording stopped");
            true
        }
        Ok(_) => false,
        Err(error) => {
            warn!("Automatic recording could not stop: {error}");
            false
        }
    }
}

fn emit_status(app: &AppHandle<Wry>, active: bool, app_name: Option<&str>) {
    let _ = app.emit(
        "auto-record-status",
        serde_json::json!({
            "active": active,
            "app": app_name,
        }),
    );
}

pub async fn get_settings(app: AppHandle<Wry>) -> Result<AutoRecordSettings, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?;
    settings::load_settings(state.db_manager.pool())
        .await
        .map_err(|_| "Não foi possível ler a gravação automática.".to_string())
}

/// `mode` wins when it is given. Without it the older `enabled` switch
/// applies: off turns it off, on keeps the current mode ("ask" if it was off).
pub async fn save(app: AppHandle<Wry>, enabled: bool, mode: Option<String>) -> Result<AutoRecordSettings, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?;
    let pool = state.db_manager.pool();
    let next = match mode.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => Mode::parse(value)
            .ok_or_else(|| "Modo de gravação automática inválido. Use off, ask ou auto.".to_string())?,
        None => {
            let current = settings::load_settings(pool)
                .await
                .map_err(|_| "Não foi possível ler a gravação automática.".to_string())?
                .mode();
            mode_after_toggle(current, enabled)
        }
    };
    settings::save_mode(pool, next)
        .await
        .map_err(|_| "Não foi possível salvar a gravação automática.".to_string())?;
    WAKE.notify_one();
    Ok(AutoRecordSettings::from_mode(next))
}
