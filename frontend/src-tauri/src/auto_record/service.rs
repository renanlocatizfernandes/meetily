use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{info, warn};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::audio::recording_commands::{self, RecordingArgs, StopOutcome};
use crate::notifications::commands::{show_recording_started_notification, NotificationManagerState};
use crate::state::AppState;

use super::detect::{self, MeetingApp};
use super::machine::{Action, DetectorMachine, StepInput};
use super::settings::{self, AutoRecordSettings};
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
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let user_hold = USER_HOLD.load(Ordering::SeqCst);
            let enabled = load_enabled(&app).await;
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
            });
            match action {
                Some(Action::Start(app_kind)) if !USER_HOLD.load(Ordering::SeqCst) => {
                    match begin(&app, app_kind).await {
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
            let now = Instant::now();
            publish_watch(Watch {
                active: owned && recording_commands::is_recording().await,
                app: signal.map(MeetingApp::label).map(str::to_string),
                phase: machine.phase_name(),
                hint: if enabled {
                    detection.hint
                } else {
                    "Gravação automática desligada nas configurações."
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

async fn load_enabled(app: &AppHandle<Wry>) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    match settings::load_settings(state.db_manager.pool()).await {
        Ok(settings) => settings.enabled,
        Err(error) => {
            warn!("Automatic recording settings were not read ({error})");
            false
        }
    }
}

async fn begin(app: &AppHandle<Wry>, kind: MeetingApp) -> Result<(), String> {
    if USER_HOLD.load(Ordering::SeqCst) || recording_commands::is_recording().await {
        return Err("recording is already active or automatic start is on hold".to_string());
    }
    let meeting_name = format!("{} (gravação automática)", kind.label());
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

pub async fn save(app: AppHandle<Wry>, enabled: bool) -> Result<AutoRecordSettings, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?;
    let settings = AutoRecordSettings { enabled };
    settings::save_settings(state.db_manager.pool(), &settings)
        .await
        .map_err(|_| "Não foi possível salvar a gravação automática.".to_string())?;
    Ok(settings)
}
