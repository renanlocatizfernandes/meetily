use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{debug, warn};
use reqwest::Client;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use crate::summary::llm_client::generate_summary;

use super::debounce::{describe_trigger, frequency_gap, InFlightGate, RequestDecision};
use super::parse::parse_assist_json;
use super::prompt::{system_prompt, user_prompt};
use super::settings_store::{load_settings, LiveAssistSettings};
use super::similarity::ShownSuggestions;
use super::target::{resolve_target, LlmTarget, SummaryHint};
use super::trigger::{classify_segment, PauseWatch, TriggerKind, PAUSE_THRESHOLD};
use super::window::{TranscriptPiece, TranscriptWindow};
use crate::assist_plus::{self, FeatureToggles};

const LIVE_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

struct SessionState {
    window: TranscriptWindow,
    pause: PauseWatch,
    gate: InFlightGate,
    shown: ShownSuggestions,
    settings: LiveAssistSettings,
    toggles: FeatureToggles,
}

struct Session {
    cancel: CancellationToken,
    listener_id: tauri::EventId,
    pause_task: JoinHandle<()>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

#[derive(Clone, Serialize)]
struct AssistItem {
    id: String,
    section: String,
    text: String,
}

#[derive(Clone, Serialize)]
struct AssistUpdate {
    items: Vec<AssistItem>,
}

#[derive(Clone, Serialize)]
struct AssistStatus {
    enabled: bool,
    recording: bool,
    busy: bool,
    note: Option<String>,
}

struct Job<R: Runtime> {
    app: AppHandle<R>,
    token: CancellationToken,
    state: Arc<Mutex<SessionState>>,
    pool: SqlitePool,
    target: LlmTarget,
    kind: TriggerKind,
    generation: u64,
}

pub fn on_recording_started<R: Runtime>(app: AppHandle<R>) {
    on_recording_stopped(&app);

    let pool = match app.try_state::<AppState>() {
        Some(state) => state.db_manager.pool().clone(),
        None => {
            warn!("Live assist skipped: database is not ready");
            return;
        }
    };

    let cancel = CancellationToken::new();
    let state = Arc::new(Mutex::new(SessionState {
        window: TranscriptWindow::default(),
        pause: PauseWatch::default(),
        gate: InFlightGate::new(cancel.clone(), frequency_gap("media")),
        shown: ShownSuggestions::default(),
        settings: LiveAssistSettings::default(),
        toggles: FeatureToggles::default(),
    }));

    let app_for_events = app.clone();
    let state_for_events = state.clone();
    let pool_for_events = pool.clone();
    let listener_id = app.listen("transcript-update", move |event| {
        let app = app_for_events.clone();
        let state = state_for_events.clone();
        let pool = pool_for_events.clone();
        let payload = event.payload().to_string();
        tauri::async_runtime::spawn(async move {
            let Ok(update) = serde_json::from_str::<crate::audio::TranscriptUpdate>(&payload) else {
                return;
            };
            // `is_partial` in this app means the audio clip was under 15s, which is
            // the normal live segment. It is not an unstable hypothesis.
            if update.text.trim().is_empty() {
                return;
            }
            handle_transcript(app, pool, state, update).await;
        });
    });

    let pause_cancel = cancel.clone();
    let pause_state = state.clone();
    let pause_app = app.clone();
    let pause_pool = pool;
    let pause_task = tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = pause_cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    consider_pause(pause_app.clone(), pause_pool.clone(), pause_state.clone()).await;
                }
            }
        }
    });

    if let Ok(mut slot) = SESSION.lock() {
        *slot = Some(Session {
            cancel,
            listener_id,
            pause_task,
        });
    }

    emit_status(&app, true, true, false, None);
    debug!("Live assist is listening to the transcript stream");
}

pub fn on_recording_stopped<R: Runtime>(app: &AppHandle<R>) {
    let session = SESSION.lock().ok().and_then(|mut slot| slot.take());
    let Some(session) = session else {
        return;
    };
    session.cancel.cancel();
    app.unlisten(session.listener_id);
    session.pause_task.abort();
    emit_status(app, false, false, false, None);
}

async fn refresh_settings(pool: &SqlitePool, state: &Mutex<SessionState>) -> bool {
    let settings = match load_settings(pool).await {
        Ok(settings) => settings,
        Err(error) => {
            warn!("Live assist settings were not read ({})", error);
            return false;
        }
    };
    let enabled = settings.enabled;
    let toggles = assist_plus::load(pool).await.unwrap_or_default();
    if let Ok(mut guard) = state.lock() {
        guard.gate.set_min_gap(frequency_gap(&settings.frequency));
        guard.settings = settings;
        guard.toggles = toggles;
    }
    enabled
}

pub(crate) async fn load_hint(pool: &SqlitePool) -> SummaryHint {
    let Ok(Some(config)) = SettingsRepository::get_model_config(pool).await else {
        return SummaryHint::default();
    };
    let api_key = SettingsRepository::get_api_key(pool, &config.provider)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let custom = if config.provider.eq_ignore_ascii_case("custom-openai") {
        SettingsRepository::get_custom_openai_config(pool).await.ok().flatten()
    } else {
        None
    };
    SummaryHint {
        provider: config.provider,
        model: config.model,
        api_key,
        ollama_endpoint: config.ollama_endpoint,
        custom_endpoint: custom.as_ref().map(|config| config.endpoint.clone()),
        custom_api_key: custom.as_ref().and_then(|config| config.api_key.clone()),
        max_tokens: custom.as_ref().and_then(|config| config.max_tokens).and_then(|value| u32::try_from(value).ok()),
        temperature: custom.as_ref().and_then(|config| config.temperature),
        top_p: custom.as_ref().and_then(|config| config.top_p),
    }
}

async fn handle_transcript<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    state: Arc<Mutex<SessionState>>,
    update: crate::audio::TranscriptUpdate,
) {
    if !refresh_settings(&pool, &state).await {
        return;
    }
    let channel = if update.source == "system" { "system" } else { "mic" };
    let source = update
        .speaker_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(channel);
    if channel == "system" {
        let alerts = {
            let Ok(guard) = state.lock() else { return };
            guard.toggles.commitment_alerts
        };
        if alerts {
            for commitment in assist_plus::detect_commitments(&update.text) {
                let _ = app.emit(
                    "commitment-alert",
                    serde_json::json!({
                        "kind": commitment.kind,
                        "text": commitment.text,
                    }),
                );
            }
        }
    }
    let kind = classify_segment(channel, &update.text);
    let hint = if kind.is_some() {
        Some(load_hint(&pool).await)
    } else {
        None
    };

    let job = {
        let Ok(mut guard) = state.lock() else {
            return;
        };
        if !guard.settings.enabled {
            return;
        }
        guard.window.push(TranscriptPiece {
            at: update.audio_end_time,
            source: source.to_string(),
            text: update.text.clone(),
        });
        guard.pause.note_activity(Instant::now());
        let Some(kind) = kind else {
            return;
        };
        let Some(hint) = hint else {
            return;
        };
        begin_job(&app, &state, &pool, &mut guard, kind, &hint)
    };
    spawn_job(job);
}

async fn consider_pause<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    state: Arc<Mutex<SessionState>>,
) {
    if !refresh_settings(&pool, &state).await {
        return;
    }
    let ready = {
        let Ok(guard) = state.lock() else {
            return;
        };
        guard.settings.enabled && !guard.window.is_empty()
    };
    if !ready {
        return;
    }
    let hint = load_hint(&pool).await;
    let job = {
        let Ok(mut guard) = state.lock() else {
            return;
        };
        if !guard.pause.take_pause(Instant::now(), PAUSE_THRESHOLD) {
            return;
        }
        begin_job(&app, &state, &pool, &mut guard, TriggerKind::Pause, &hint)
    };
    spawn_job(job);
}

fn begin_job<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<Mutex<SessionState>>,
    pool: &SqlitePool,
    guard: &mut SessionState,
    kind: TriggerKind,
    hint: &SummaryHint,
) -> Option<Job<R>> {
    if guard.window.is_empty() {
        return None;
    }
    let target = match resolve_target(hint, &guard.settings.model) {
        Ok(target) => target,
        Err(error) => {
            warn!("Live assist provider was not recognized");
            debug!("provider error chars={}", error.chars().count());
            return None;
        }
    };
    let RequestDecision::Start {
        token,
        cancelled_previous,
        generation,
    } = guard.gate.request(Instant::now())
    else {
        return None;
    };
    debug!(
        "Live assist call; trigger={}, local={}, cancelled_previous={}",
        describe_trigger(kind),
        !target.leaves_machine,
        cancelled_previous,
    );
    emit_status(app, guard.settings.enabled, true, true, None);
    Some(Job {
        app: app.clone(),
        token,
        state: state.clone(),
        pool: pool.clone(),
        target,
        kind,
        generation,
    })
}

fn spawn_job<R: Runtime>(job: Option<Job<R>>) {
    let Some(job) = job else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        run_job(job).await;
    });
}

async fn run_job<R: Runtime>(job: Job<R>) {
    // A replacement call or a stopped recording already published its own status.
    if job.token.is_cancelled() {
        return;
    }

    let client = Client::new();
    let target = &job.target;
    let snapshot = {
        let Ok(guard) = job.state.lock() else { return };
        (
            guard.settings.language.clone(),
            guard.settings.meeting_context.clone(),
            guard.window.clone(),
            guard.shown.lines(),
            guard.toggles.internet_search,
            guard.toggles.translation.clone(),
        )
    };
    let (language, context, window, shown, internet, translation) = snapshot;
    let mut sources = String::new();
    if internet {
        let recent = window
            .recent()
            .filter(|piece| piece.source != "Você" && piece.source != "mic")
            .map(|piece| piece.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let terms = assist_plus::unknown_terms(&recent, &context);
        for hit in assist_plus::lookup_terms(&job.pool, &terms).await {
            let line = assist_plus::contexto_line(&hit);
            sources.push_str(&format!("- {line}\n"));
            let _ = job.app.emit(
                "live-assist-update",
                AssistUpdate {
                    items: vec![AssistItem {
                        id: Uuid::new_v4().to_string(),
                        section: "contexto".to_string(),
                        text: line,
                    }],
                },
            );
        }
    }
    if assist_plus::should_translate(&translation, &window.recent().map(|piece| piece.text.as_str()).collect::<Vec<_>>().join(" ")) {
        sources.push_str("Traduza para pt-BR a fala que não estiver em português e sugira a resposta no idioma da reunião.\n");
    }
    let prompt = user_prompt(&language, &context, &window, &shown, &sources);
    let request = generate_summary(
        &client,
        &target.provider,
        &target.model,
        &target.api_key,
        system_prompt(),
        &prompt,
        target.ollama_endpoint.as_deref(),
        target.custom_openai_endpoint.as_deref(),
        target.max_tokens,
        target.temperature,
        target.top_p,
        None,
        Some(&job.token),
    );

    let timed = tokio::time::timeout(LIVE_REQUEST_TIMEOUT, request).await;
    if job.token.is_cancelled() {
        return;
    }

    let note = match timed {
        Ok(Ok(completion)) => match parse_assist_json(&completion.content) {
            Some(parsed) => {
                publish(&job, parsed);
                None
            }
            None => {
                warn!("Live assist discarded an invalid model payload");
                Some("A resposta do modelo não estava no formato esperado.".to_string())
            }
        },
        Ok(Err(error)) if error.to_ascii_lowercase().contains("cancel") => None,
        Ok(Err(_)) => {
            warn!("Live assist model call failed");
            Some("Não foi possível gerar uma sugestão agora.".to_string())
        }
        Err(_) => {
            job.token.cancel();
            warn!("Live assist model call exceeded the live timeout");
            Some("A sugestão demorou demais e foi descartada.".to_string())
        }
    };

    let enabled = {
        let Ok(guard) = job.state.lock() else {
            return;
        };
        if !guard.gate.is_current(job.generation) {
            return;
        }
        guard.settings.enabled
    };
    emit_status(&job.app, enabled, true, false, note);
}

fn publish<R: Runtime>(job: &Job<R>, parsed: super::parse::AssistLists) {
    let mut items = Vec::new();
    let mut accepted = Vec::new();
    for (section, texts) in [
        ("perguntas", parsed.perguntas),
        ("respostas", parsed.respostas),
        ("contexto", parsed.contexto),
    ] {
        for text in texts {
            let repeated = job
                .state
                .lock()
                .map(|guard| guard.shown.is_repeat(section, &text))
                .unwrap_or(true);
            if repeated {
                continue;
            }
            accepted.push((section.to_string(), text.clone()));
            items.push(AssistItem {
                id: Uuid::new_v4().to_string(),
                section: section.to_string(),
                text,
            });
        }
    }

    if items.is_empty() {
        debug!("Live assist had no new items after similarity filtering; trigger={}", describe_trigger(job.kind));
        return;
    }

    if let Ok(mut guard) = job.state.lock() {
        for (section, text) in accepted {
            guard.shown.remember(&section, &text);
        }
    }
    let _ = job.app.emit("live-assist-update", AssistUpdate { items });
}

fn emit_status<R: Runtime>(
    app: &AppHandle<R>,
    enabled: bool,
    recording: bool,
    busy: bool,
    note: Option<String>,
) {
    let _ = app.emit(
        "live-assist-status",
        AssistStatus {
            enabled,
            recording,
            busy,
            note,
        },
    );
}
