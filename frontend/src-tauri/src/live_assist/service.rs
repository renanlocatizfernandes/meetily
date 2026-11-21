use std::sync::atomic::{AtomicU64, Ordering};
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
use crate::summary::LLMProvider;

use super::context::{
    direct_items, infer_client, render_context, resolve_origin, AssistOrigin, Budget, ContextSummary, LiveContext, SourceRef,
    ORIGIN_AGENDA, ORIGIN_TALK,
};
use super::context_store::{load_context_settings, load_known_clients, load_past_meetings};
use super::debounce::{describe_trigger, frequency_gap, InFlightGate, RequestDecision};
use super::parse::parse_assist_json;
use super::partial::{extract_partial_items, section_for_kind, PartialItem};
use super::prompt::{system_prompt, user_prompt};
use super::settings_store::{load_settings, LiveAssistSettings};
use super::contradiction::find_divergence;
use super::recap::{
    answer_system_prompt, build_answer_prompt, build_recap_prompt, clamp_minutes, parse_bullets, AnswerResult,
    HistoryLine, RecapResult, TalkHistory,
};
use super::similarity::{too_similar, ShownSuggestions};
use super::smart_settings::{load_smart_settings, SmartSettings};
use super::usage::{
    economy_enabled, economy_gap, economy_starts_immediately, estimate_tokens, UsageMeter, UsageSnapshot,
    ECONOMY_GROUP_GAP, ECONOMY_MAX_RECENT_CHARS, ECONOMY_QUESTION_GAP,
};
use super::stream::{stream_ollama_chat, StreamError, StreamLimits};
use super::target::{resolve_target, LlmTarget, SummaryHint};
use super::trigger::{classify_segment, PauseWatch, TriggerKind, PAUSE_THRESHOLD};
use super::window::{TranscriptPiece, TranscriptWindow};
use crate::assist_plus::{self, FeatureToggles};

/// Whole-call limit for providers that answer in one piece.
const LIVE_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
/// Streaming (Ollama): the model must start answering within this time.
const LIVE_FIRST_TOKEN_TIMEOUT: Duration = Duration::from_secs(8);
/// Streaming (Ollama): the whole answer must arrive within this time.
const LIVE_STREAM_TOTAL_TIMEOUT: Duration = Duration::from_secs(20);
/// At most about 10 `live-assist-partial` events per second.
const PARTIAL_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// While the client of the meeting is unknown, the names already heard are
/// matched again at this pace. In-memory string matching only, no database.
const CLIENT_CHECK_INTERVAL: Duration = Duration::from_secs(20);
/// Tail of the conversation kept to find the client. Characters.
const HEARD_CHAR_LIMIT: usize = 4000;
const MAX_SPEAKERS: usize = 8;

/// Identifies one model call in `live-assist-partial`. Unique for the app run.
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// "Contexto em uso" of the recording in progress, for a panel that opens late.
static LAST_CONTEXT: Mutex<Option<ContextSummary>> = Mutex::new(None);
/// Set when the owner saves settings that change the context.
static CONTEXT_DIRTY: once_cell::sync::Lazy<tokio::sync::Notify> = once_cell::sync::Lazy::new(tokio::sync::Notify::new);

struct SessionState {
    window: TranscriptWindow,
    pause: PauseWatch,
    gate: InFlightGate,
    shown: ShownSuggestions,
    settings: LiveAssistSettings,
    toggles: FeatureToggles,
    /// Cached by `context_task`; read by every model call.
    context: LiveContext,
    /// Speaker labels seen in this recording, in order of appearance.
    speakers: Vec<String>,
    /// Tail of what was said, used only to find the client.
    heard: String,
    /// False when the live model is a cloud service.
    extras_allowed: bool,
    smart: SmartSettings,
    smart_loaded_at: Option<Instant>,
    /// Longer than `window`: feeds "O que eu perdi?" and the suggested answer.
    history: TalkHistory,
    usage: UsageMeter,
    /// Alert texts already shown, to keep the same one from repeating.
    alerts_shown: Vec<String>,
    /// Segments heard since the last model call started (economy mode groups them).
    unsent: usize,
    last_auto_answer: Option<Instant>,
}

impl SessionState {
    /// Economy mode as it applies now. Before the provider is known the
    /// context task has not run yet and the model counts as local.
    fn economy_now(&self) -> bool {
        economy_enabled(&self.smart.economy, !self.extras_allowed)
    }

    fn usage_snapshot(&self) -> UsageSnapshot {
        self.usage.snapshot(!self.extras_allowed, self.economy_now())
    }

    /// True when this alert is new; remembers it.
    fn note_alert(&mut self, kind: &str, text: &str) -> bool {
        let tagged = format!("{kind}: {text}");
        if self.alerts_shown.iter().any(|known| too_similar(known, &tagged)) {
            return false;
        }
        self.alerts_shown.push(tagged);
        if self.alerts_shown.len() > 30 {
            self.alerts_shown.remove(0);
        }
        true
    }

    /// Proactive alerts for one final segment. Local text work only.
    fn collect_alerts(&mut self, channel: &str, source: &str, text: &str) -> Vec<SmartAlert> {
        let from_other = channel == "system";
        let speaker = (source != "mic" && source != "system").then(|| source.to_string());
        let mut out = Vec::new();
        for signal in assist_plus::detect_signals(text, from_other) {
            let enabled = match signal.kind {
                "pergunta" => self.smart.question_alerts,
                "decisao" => self.smart.decision_alerts,
                "tarefa" => self.smart.task_alerts,
                _ => false,
            };
            if !enabled || !self.note_alert(signal.kind, &signal.text) {
                continue;
            }
            out.push(SmartAlert {
                id: Uuid::new_v4().to_string(),
                kind: signal.kind,
                text: signal.text,
                speaker: speaker.clone(),
                detail: None,
            });
        }
        if self.smart.contradiction_alerts && !self.context.notes.is_empty() {
            if let Some(found) = find_divergence(text, &self.context.notes) {
                let sentence: String = text.trim().chars().take(180).collect();
                if self.note_alert("divergencia", &sentence) {
                    out.push(SmartAlert {
                        id: Uuid::new_v4().to_string(),
                        kind: "divergencia",
                        text: sentence,
                        speaker: speaker.clone(),
                        detail: Some(format!("Na {}: {}", found.origin, found.remembered)),
                    });
                }
            }
        }
        out
    }

    /// Returns true when the label is new.
    fn note_speaker(&mut self, label: &str) -> bool {
        if label == "mic" || label == "system" || self.speakers.iter().any(|known| known == label) {
            return false;
        }
        if self.speakers.len() >= MAX_SPEAKERS {
            return false;
        }
        self.speakers.push(label.to_string());
        true
    }

    fn note_heard(&mut self, text: &str) {
        self.heard.push_str(text.trim());
        self.heard.push('\n');
        let excess = self.heard.chars().count().saturating_sub(HEARD_CHAR_LIMIT);
        if excess > 0 {
            self.heard = self.heard.chars().skip(excess).collect();
        }
    }
}

struct Session {
    state: Arc<Mutex<SessionState>>,
    cancel: CancellationToken,
    listener_id: tauri::EventId,
    pause_task: JoinHandle<()>,
    context_task: JoinHandle<()>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// Consumption of the recording in progress, for a panel that opens late.
static LAST_USAGE: Mutex<Option<UsageSnapshot>> = Mutex::new(None);
/// How long the settings of the proactive part are reused before the database is read again.
const SMART_SETTINGS_TTL: Duration = Duration::from_secs(2);
/// Whole-call limit for "O que eu perdi?" and "Sugerir resposta": the owner asked and waits.
const ON_DEMAND_TIMEOUT: Duration = Duration::from_secs(40);

/// Proactive alert, emitted as `live-assist-smart-alert`. The text is a
/// sentence of the transcript; the panel shows it, nothing is stored.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmartAlert {
    id: String,
    /// "pergunta", "decisao", "tarefa" or "divergencia".
    kind: &'static str,
    text: String,
    speaker: Option<String>,
    /// For "divergencia": what the earlier meeting said, with its origin.
    detail: Option<String>,
}

#[derive(Clone, Serialize)]
struct AssistItem {
    id: String,
    section: String,
    text: String,
    /// Where the suggestion came from. Absent when there is nothing to cite.
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<AssistOrigin>,
}

#[derive(Clone, Serialize)]
struct AssistUpdate {
    items: Vec<AssistItem>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssistPartial {
    request_id: u64,
    items: Vec<PartialItem>,
    done: bool,
}

enum CallError {
    Cancelled,
    Timeout,
    Failed,
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

/// Lets the context of the recording in progress be rebuilt after the owner
/// changed the profile, the glossary, the switch or the meeting context.
pub(crate) fn request_context_refresh() {
    CONTEXT_DIRTY.notify_one();
}

/// "Contexto em uso" of the recording in progress, if there is one.
pub(crate) fn current_context_summary() -> Option<ContextSummary> {
    LAST_CONTEXT.lock().ok().and_then(|guard| guard.clone())
}

fn publish_context<R: Runtime>(app: &AppHandle<R>, summary: ContextSummary) {
    if let Ok(mut slot) = LAST_CONTEXT.lock() {
        if slot.as_ref() == Some(&summary) {
            return;
        }
        *slot = Some(summary.clone());
    }
    let _ = app.emit("live-assist-context", summary);
}

/// Keeps `SessionState::context` current. The database is read when the
/// recording starts, when the client becomes known and when the owner saves
/// a setting; between those moments this task only sleeps.
async fn context_task<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    state: Arc<Mutex<SessionState>>,
    cancel: CancellationToken,
) {
    let mut known: Vec<String> = Vec::new();
    let mut client: Option<String> = None;
    let mut reload = true;
    loop {
        if reload {
            known = load_known_clients(&pool).await;
        }
        let (typed, heard) = {
            let Ok(guard) = state.lock() else { return };
            (guard.settings.meeting_context.clone(), guard.heard.clone())
        };
        let meeting_name = crate::audio::recording_commands::get_recording_meeting_name()
            .await
            .ok()
            .flatten();
        // What the owner typed can name another client; the conversation
        // alone never replaces a client that was already found.
        let declared = infer_client(&known, meeting_name.as_deref().unwrap_or(""), &typed, "");
        let found = declared
            .or_else(|| client.clone())
            .or_else(|| infer_client(&known, "", "", &heard));
        if reload || found != client {
            client = found;
            let settings = load_context_settings(&pool).await.unwrap_or_default();
            let live = load_settings(&pool).await.unwrap_or_default();
            let toggles = assist_plus::load_effective(&pool).await.unwrap_or_default();
            let memory_allowed = settings.use_past_meetings && toggles.meeting_memory;
            let extras_allowed = resolve_target(&load_hint(&pool).await, &live.model)
                .map(|target| !target.leaves_machine)
                .unwrap_or(true);
            let past = match &client {
                Some(name) => load_past_meetings(&pool, name).await.unwrap_or_default(),
                None => Vec::new(),
            };
            let mentioned = format!("{}\n{typed}\n{heard}", meeting_name.as_deref().unwrap_or(""));
            let context = LiveContext::assemble(
                settings,
                memory_allowed,
                meeting_name,
                client.clone(),
                &mentioned,
                &past,
                chrono::Utc::now(),
            );
            debug!(
                "Live assist context rebuilt; client_known={}, past_meetings={}, notes={}, glossary_terms={}, profile_filled={}, local_model={}",
                context.client.is_some(),
                context.past_meetings,
                context.notes.len(),
                context.settings.glossary_entries().len(),
                context.settings.profile_filled(),
                extras_allowed,
            );
            let summary = {
                let Ok(mut guard) = state.lock() else { return };
                guard.context = context;
                guard.extras_allowed = extras_allowed;
                guard.context.summary(&guard.speakers, extras_allowed)
            };
            if cancel.is_cancelled() {
                return;
            }
            publish_context(&app, summary);
        }
        reload = false;
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = CONTEXT_DIRTY.notified() => reload = true,
            // Nothing periodic once the client is known.
            _ = tokio::time::sleep(CLIENT_CHECK_INTERVAL), if client.is_none() => {}
        }
    }
}

pub fn on_recording_started<R: Runtime>(app: AppHandle<R>) {
    // A latency benchmark must not compete with a live meeting for the model.
    super::benchmark::cancel_running();
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
        context: LiveContext::default(),
        speakers: Vec::new(),
        heard: String::new(),
        extras_allowed: true,
        smart: SmartSettings::default(),
        smart_loaded_at: None,
        history: TalkHistory::default(),
        usage: UsageMeter::default(),
        alerts_shown: Vec::new(),
        unsent: 0,
        last_auto_answer: None,
    }));
    if let Ok(mut slot) = LAST_USAGE.lock() {
        *slot = None;
    }

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
    let context_pool = pool.clone();
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

    let context_app = app.clone();
    let context_state = state.clone();
    let session_state = state.clone();
    let context_cancel = cancel.clone();
    let context_task = tauri::async_runtime::spawn(async move {
        // The typed meeting context is part of what names the client.
        refresh_settings(&context_pool, &context_state).await;
        context_task(context_app, context_pool, context_state, context_cancel).await;
    });

    if let Ok(mut slot) = SESSION.lock() {
        *slot = Some(Session {
            state: session_state,
            cancel,
            listener_id,
            pause_task,
            context_task,
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
    session.context_task.abort();
    if let Ok(mut slot) = LAST_CONTEXT.lock() {
        *slot = None;
    }
    if let Ok(mut slot) = LAST_USAGE.lock() {
        *slot = None;
    }
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
    let toggles = assist_plus::load_effective(pool).await.unwrap_or_default();
    let smart_stale = state
        .lock()
        .map(|guard| guard.smart_loaded_at.map_or(true, |at| at.elapsed() >= SMART_SETTINGS_TTL))
        .unwrap_or(false);
    let smart = if smart_stale { load_smart_settings(pool).await.ok() } else { None };
    if let Ok(mut guard) = state.lock() {
        guard.gate.set_min_gap(frequency_gap(&settings.frequency));
        guard.settings = settings;
        guard.toggles = toggles;
        if let Some(smart) = smart {
            guard.smart = smart;
            guard.smart_loaded_at = Some(Instant::now());
        }
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

    let (job, new_speaker, alerts, auto_answers) = {
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
        guard.history.push(HistoryLine {
            at: update.audio_end_time,
            source: source.to_string(),
            text: update.text.clone(),
        });
        guard.unsent += 1;
        let alerts = guard.collect_alerts(channel, source, &update.text);
        let mut auto_answers = Vec::new();
        if guard.smart.auto_answer {
            for alert in alerts.iter().filter(|alert| alert.kind == "pergunta") {
                let waited = guard.last_auto_answer.map_or(true, |at| at.elapsed() >= ECONOMY_QUESTION_GAP);
                if waited {
                    guard.last_auto_answer = Some(Instant::now());
                    auto_answers.push(alert.text.clone());
                    break;
                }
            }
        }
        guard.note_heard(&update.text);
        let new_speaker = guard
            .note_speaker(source)
            .then(|| guard.context.summary(&guard.speakers, guard.extras_allowed));
        guard.pause.note_activity(Instant::now());
        let job = match (kind, hint) {
            (Some(kind), Some(hint)) => begin_job(&app, &state, &pool, &mut guard, kind, &hint),
            _ => None,
        };
        (job, new_speaker, alerts, auto_answers)
    };
    if let Some(summary) = new_speaker {
        publish_context(&app, summary);
    }
    for alert in alerts {
        debug!("Live assist smart alert; kind={}", alert.kind);
        let _ = app.emit("live-assist-smart-alert", alert);
    }
    for question in auto_answers {
        let (app, pool, state) = (app.clone(), pool.clone(), state.clone());
        tauri::async_runtime::spawn(async move {
            if let Err(error) = suggest_answer_inner(&app, &pool, &state, &question).await {
                debug!("Live assist automatic answer skipped; reason_chars={}", error.chars().count());
            }
        });
    }
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
    // Economy mode: no call per pause. What was heard since the last call is
    // grouped and sent once the group gap has passed.
    let (ready, economy, flush_due) = {
        let Ok(guard) = state.lock() else {
            return;
        };
        let economy = guard.economy_now();
        let flush_due = economy && guard.unsent > 0 && guard.gate.would_accept(Instant::now(), ECONOMY_GROUP_GAP);
        (guard.settings.enabled && !guard.window.is_empty(), economy, flush_due)
    };
    if !ready || (economy && !flush_due) {
        return;
    }
    let hint = load_hint(&pool).await;
    let job = {
        let Ok(mut guard) = state.lock() else {
            return;
        };
        if !economy && !guard.pause.take_pause(Instant::now(), PAUSE_THRESHOLD) {
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
    let economy = economy_enabled(&guard.smart.economy, target.leaves_machine);
    if economy && !economy_starts_immediately(kind) && kind != TriggerKind::Pause {
        // Sentence ends wait for the group flush (see `consider_pause`).
        guard.usage.record_grouped();
        return None;
    }
    let gap = if economy { economy_gap(kind) } else { frequency_gap(&guard.settings.frequency) };
    guard.gate.set_min_gap(gap);
    let RequestDecision::Start {
        token,
        cancelled_previous,
        generation,
    } = guard.gate.request(Instant::now())
    else {
        if economy {
            guard.usage.record_grouped();
        }
        return None;
    };
    guard.unsent = 0;
    debug!(
        "Live assist call; trigger={}, local={}, cancelled_previous={}, economy={}",
        describe_trigger(kind),
        !target.leaves_machine,
        cancelled_previous,
        economy,
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
            guard.context.clone(),
            guard.speakers.clone(),
        )
    };
    let (language, context, mut window, shown, internet, translation, live_context, speakers) = snapshot;
    if economy_enabled(&job.state.lock().map(|guard| guard.smart.economy.clone()).unwrap_or_default(), target.leaves_machine) {
        window.limit_recent_chars(ECONOMY_MAX_RECENT_CHARS);
    }
    // Context selection is in-memory text work on a few cached notes.
    let said_now = format!(
        "{}\n{}",
        window.earlier_summary(),
        window.recent().map(|piece| piece.text.as_str()).collect::<Vec<_>>().join(" ")
    );
    let rendered = render_context(
        &live_context,
        &speakers,
        &said_now,
        Budget::for_typed_context(context.chars().count()),
        // A cloud model gets nothing it did not get before this feature.
        !target.leaves_machine,
    );
    debug!(
        "Live assist context in prompt; chars={}, memory_notes={}, glossary_entries={}",
        rendered.text.chars().count(),
        rendered.memory_notes,
        rendered.glossary_entries,
    );
    // Glossary terms just said and the earlier note that matches the talk go
    // to the panel right away, before the model answers.
    let recent_talk = window.recent().map(|piece| piece.text.as_str()).collect::<Vec<_>>().join(" ");
    let mut direct = Vec::new();
    if let Ok(mut guard) = job.state.lock() {
        for item in direct_items(&live_context, &recent_talk) {
            if guard.shown.is_repeat("contexto", &item.text) {
                continue;
            }
            guard.shown.remember("contexto", &item.text);
            direct.push(AssistItem {
                id: Uuid::new_v4().to_string(),
                section: "contexto".to_string(),
                text: item.text,
                origin: Some(item.origin),
            });
        }
    }
    if !direct.is_empty() && !job.token.is_cancelled() {
        debug!("Live assist context items without the model; count={}", direct.len());
        let _ = job.app.emit("live-assist-update", AssistUpdate { items: direct });
    }
    let mut origins: Vec<SourceRef> = rendered.sources;
    if !context.trim().is_empty() {
        origins.push(SourceRef::new(AssistOrigin::simple(ORIGIN_AGENDA, "Pauta"), &context));
    }
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
            sources.push_str(&format!("- [{}] {line}\n", hit.source));
            let origin = AssistOrigin::internet(&hit.source, &hit.url);
            origins.push(SourceRef::new(origin.clone(), &hit.extract));
            let _ = job.app.emit(
                "live-assist-update",
                AssistUpdate {
                    items: vec![AssistItem {
                        id: Uuid::new_v4().to_string(),
                        section: "contexto".to_string(),
                        text: line,
                        origin: Some(origin),
                    }],
                },
            );
        }
    }
    if assist_plus::should_translate(&translation, &window.recent().map(|piece| piece.text.as_str()).collect::<Vec<_>>().join(" ")) {
        sources.push_str("Traduza para pt-BR a fala que não estiver em português e sugira a resposta no idioma da reunião.\n");
    }
    // Last, so it only wins when nothing else explains an item.
    origins.push(SourceRef::new(AssistOrigin::simple(ORIGIN_TALK, "Conversa"), &said_now));
    let prompt = user_prompt(&language, &context, &rendered.text, &window, &shown, &sources);
    let streaming = target.provider == LLMProvider::Ollama;
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst);
    let result = if streaming {
        stream_call(&job, &client, &prompt, request_id, &origins).await
    } else {
        plain_call(&job, &client, &prompt).await
    };
    let output_chars = result.as_ref().map(|content| content.chars().count()).unwrap_or(0);
    let input_chars = prompt.chars().count() + system_prompt().chars().count();
    record_usage(&job.app, &job.state, input_chars, output_chars);
    // Tells the panel to drop the in-progress items of this call. It is sent
    // on every exit of the streaming path, after the final update if any.
    let finish_partial = || {
        if streaming {
            let _ = job.app.emit(
                "live-assist-partial",
                AssistPartial {
                    request_id,
                    items: Vec::new(),
                    done: true,
                },
            );
        }
    };
    if job.token.is_cancelled() {
        finish_partial();
        return;
    }

    let note = match result {
        Ok(content) => match parse_assist_json(&content) {
            Some(parsed) => {
                publish(&job, parsed, &origins);
                None
            }
            None => {
                warn!("Live assist discarded an invalid model payload");
                Some("A resposta do modelo não estava no formato esperado.".to_string())
            }
        },
        Err(CallError::Cancelled) => None,
        Err(CallError::Failed) => {
            warn!("Live assist model call failed");
            Some("Não foi possível gerar uma sugestão agora.".to_string())
        }
        Err(CallError::Timeout) => {
            job.token.cancel();
            warn!("Live assist model call exceeded the live timeout");
            Some("A sugestão demorou demais e foi descartada.".to_string())
        }
    };
    finish_partial();

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

/// One POST, whole answer at the end. Used by every provider except Ollama.
async fn plain_call<R: Runtime>(job: &Job<R>, client: &Client, prompt: &str) -> Result<String, CallError> {
    let target = &job.target;
    let request = generate_summary(
        client,
        &target.provider,
        &target.model,
        &target.api_key,
        system_prompt(),
        prompt,
        target.ollama_endpoint.as_deref(),
        target.custom_openai_endpoint.as_deref(),
        target.max_tokens,
        target.temperature,
        target.top_p,
        None,
        Some(&job.token),
    );
    match tokio::time::timeout(LIVE_REQUEST_TIMEOUT, request).await {
        Ok(Ok(completion)) => Ok(completion.content),
        Ok(Err(error)) if error.to_ascii_lowercase().contains("cancel") => Err(CallError::Cancelled),
        Ok(Err(_)) => Err(CallError::Failed),
        Err(_) => Err(CallError::Timeout),
    }
}

/// Ollama: tokens arrive as they are generated and the items already readable
/// in the partial JSON go out as `live-assist-partial`.
async fn stream_call<R: Runtime>(
    job: &Job<R>,
    client: &Client,
    prompt: &str,
    request_id: u64,
    origins: &[SourceRef],
) -> Result<String, CallError> {
    let target = &job.target;
    let mut last_emit: Option<Instant> = None;
    let mut last_items: Vec<PartialItem> = Vec::new();
    let on_content = |content: &str| {
        if job.token.is_cancelled() {
            return;
        }
        let now = Instant::now();
        if last_emit.is_some_and(|at| now.saturating_duration_since(at) < PARTIAL_MIN_INTERVAL) {
            return;
        }
        let mut items = extract_partial_items(content);
        if let Ok(guard) = job.state.lock() {
            // Items the final filter would drop as repeats are not previewed.
            items.retain(|item| !guard.shown.is_repeat(section_for_kind(item.kind), &item.text));
        }
        for item in &mut items {
            // The tag appears once the model has written the item's `origem`.
            if item.claimed_origin.is_some() {
                item.origin = resolve_origin(
                    section_for_kind(item.kind),
                    &item.text,
                    item.claimed_origin.as_deref(),
                    origins,
                );
            }
        }
        if items == last_items {
            return;
        }
        last_emit = Some(now);
        last_items = items.clone();
        let _ = job.app.emit(
            "live-assist-partial",
            AssistPartial {
                request_id,
                items,
                done: false,
            },
        );
    };
    let outcome = stream_ollama_chat(
        client,
        target.ollama_endpoint.as_deref(),
        &target.model,
        system_prompt(),
        prompt,
        &job.token,
        StreamLimits {
            first_token: LIVE_FIRST_TOKEN_TIMEOUT,
            total: LIVE_STREAM_TOTAL_TIMEOUT,
        },
        on_content,
    )
    .await;
    match outcome {
        Ok(outcome) => Ok(outcome.content),
        Err(StreamError::Cancelled) => Err(CallError::Cancelled),
        Err(StreamError::FirstTokenTimeout) | Err(StreamError::TotalTimeout) => Err(CallError::Timeout),
        Err(StreamError::Failed(reason)) => {
            debug!("Live assist stream failed; reason={reason}");
            Err(CallError::Failed)
        }
    }
}

fn publish<R: Runtime>(job: &Job<R>, parsed: super::parse::AssistLists, origins: &[SourceRef]) {
    let mut items = Vec::new();
    let mut accepted = Vec::new();
    let claimed = parsed.origens;
    for (section, texts) in [
        ("perguntas", parsed.perguntas),
        ("respostas", parsed.respostas),
        ("contexto", parsed.contexto),
    ] {
        for (index, text) in texts.into_iter().enumerate() {
            let repeated = job
                .state
                .lock()
                .map(|guard| guard.shown.is_repeat(section, &text))
                .unwrap_or(true);
            if repeated {
                continue;
            }
            accepted.push((section.to_string(), text.clone()));
            let origin = resolve_origin(section, &text, claimed.get(section, index), origins);
            items.push(AssistItem {
                id: Uuid::new_v4().to_string(),
                section: section.to_string(),
                text,
                origin,
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

fn record_usage<R: Runtime>(app: &AppHandle<R>, state: &Mutex<SessionState>, input_chars: usize, output_chars: usize) {
    let snapshot = {
        let Ok(mut guard) = state.lock() else { return };
        guard.usage.record_call(input_chars, output_chars);
        guard.usage_snapshot()
    };
    publish_usage(app, snapshot);
}

fn publish_usage<R: Runtime>(app: &AppHandle<R>, snapshot: UsageSnapshot) {
    if let Ok(mut slot) = LAST_USAGE.lock() {
        *slot = Some(snapshot.clone());
    }
    let _ = app.emit("live-assist-usage", snapshot);
}

/// Consumption of the recording in progress; `None` when not recording.
pub(crate) fn current_usage() -> Option<UsageSnapshot> {
    LAST_USAGE.lock().ok().and_then(|guard| guard.clone())
}

fn current_session() -> Option<(Arc<Mutex<SessionState>>, CancellationToken)> {
    SESSION
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|session| (session.state.clone(), session.cancel.clone())))
}

fn pool_of<R: Runtime>(app: &AppHandle<R>) -> Result<SqlitePool, String> {
    app.try_state::<AppState>()
        .map(|state| state.db_manager.pool().clone())
        .ok_or_else(|| "Banco de dados indisponível".to_string())
}

const NO_SESSION: &str = "Comece uma gravação com a ajuda ao vivo ligada para usar isto.";

async fn resolve_live_target(pool: &SqlitePool, state: &Mutex<SessionState>) -> Result<LlmTarget, String> {
    let model = state.lock().map_err(|_| NO_SESSION.to_string())?.settings.model.clone();
    resolve_target(&load_hint(pool).await, &model).map_err(|_| "Não foi possível usar o provedor de IA configurado.".to_string())
}

/// One model call the owner asked for. Plain text answer, long timeout.
async fn on_demand_call(
    target: &LlmTarget,
    system: &str,
    user: &str,
    cancel: &CancellationToken,
) -> Result<String, String> {
    let client = Client::new();
    let request = generate_summary(
        &client,
        &target.provider,
        &target.model,
        &target.api_key,
        system,
        user,
        target.ollama_endpoint.as_deref(),
        target.custom_openai_endpoint.as_deref(),
        target.max_tokens,
        target.temperature,
        target.top_p,
        None,
        Some(cancel),
    );
    match tokio::time::timeout(ON_DEMAND_TIMEOUT, request).await {
        Ok(Ok(completion)) if !completion.content.trim().is_empty() => Ok(completion.content),
        Ok(Ok(_)) => Err("O modelo não devolveu texto. Tente de novo.".to_string()),
        Ok(Err(error)) if error.to_ascii_lowercase().contains("cancel") => Err("A gravação terminou antes da resposta.".to_string()),
        Ok(Err(error)) => Err(target
            .leaves_machine
            .then(|| provider_explanation(&error))
            .flatten()
            .unwrap_or_else(|| "Não foi possível falar com o modelo agora.".to_string())),
        Err(_) => Err("O modelo demorou demais. Tente de novo.".to_string()),
    }
}

/// The pt-BR explanation a cloud provider failure already carries ("Sem crédito…",
/// "Chave de API inválida…"), without the HTTP detail or the fallback-chain tail.
/// `None` for anything else, so raw transport errors never reach the screen.
fn provider_explanation(error: &str) -> Option<String> {
    let first = error.split(" | ").next()?;
    let text = first.split(" (HTTP ").next()?.trim();
    (first.contains(" (HTTP ") && !text.is_empty()).then(|| text.to_string())
}

/// "O que eu perdi?": summary of the last `minutes` of the recording.
pub(crate) async fn run_recap<R: Runtime>(app: &AppHandle<R>, minutes: u32) -> Result<RecapResult, String> {
    let (state, cancel) = current_session().ok_or_else(|| NO_SESSION.to_string())?;
    let pool = pool_of(app)?;
    let minutes = clamp_minutes(minutes);
    let target = resolve_live_target(&pool, &state).await?;
    let prompt = {
        let guard = state.lock().map_err(|_| NO_SESSION.to_string())?;
        if !guard.settings.enabled {
            return Err("Ligue a ajuda ao vivo para usar isto.".to_string());
        }
        let economy = economy_enabled(&guard.smart.economy, target.leaves_machine);
        build_recap_prompt(&guard.history, minutes, &guard.settings.language, economy)
    }
    .ok_or_else(|| "Ainda não há fala nesse período.".to_string())?;
    debug!("Live assist recap; minutes={minutes}, lines={}, local={}", prompt.lines, !target.leaves_machine);
    let answer = on_demand_call(&target, prompt.system, &prompt.user, &cancel).await?;
    let bullets = parse_bullets(&answer);
    record_usage(app, &state, prompt.input_chars, answer.chars().count());
    Ok(RecapResult {
        minutes,
        bullets,
        input_tokens: estimate_tokens(prompt.input_chars),
        output_tokens: estimate_tokens(answer.chars().count()),
        paid: target.leaves_machine,
    })
}

/// "Sugerir resposta": an answer to a question of the other side, from the
/// recent talk and the context in use. The text also goes to the panel as a
/// `respostas` item.
pub(crate) async fn suggest_answer<R: Runtime>(app: &AppHandle<R>, question: &str) -> Result<AnswerResult, String> {
    let (state, _) = current_session().ok_or_else(|| NO_SESSION.to_string())?;
    let pool = pool_of(app)?;
    suggest_answer_inner(app, &pool, &state, question).await
}

async fn suggest_answer_inner<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    state: &Arc<Mutex<SessionState>>,
    question: &str,
) -> Result<AnswerResult, String> {
    let cancel = current_session().map(|(_, cancel)| cancel).ok_or_else(|| NO_SESSION.to_string())?;
    let question: String = question.trim().chars().take(400).collect();
    if question.is_empty() {
        return Err("Não há pergunta para responder.".to_string());
    }
    let target = resolve_live_target(pool, state).await?;
    let prompt = {
        let guard = state.lock().map_err(|_| NO_SESSION.to_string())?;
        if !guard.settings.enabled {
            return Err("Ligue a ajuda ao vivo para usar isto.".to_string());
        }
        // Same rule as the live call: a cloud model gets no profile or earlier meetings.
        let rendered = render_context(
            &guard.context,
            &guard.speakers,
            &question,
            Budget::for_typed_context(guard.settings.meeting_context.chars().count()),
            !target.leaves_machine,
        );
        let mut context = guard.settings.meeting_context.clone();
        if !rendered.text.trim().is_empty() {
            context.push_str("\n\n");
            context.push_str(&rendered.text);
        }
        build_answer_prompt(&guard.history, &question, &guard.settings.language, &context)
    };
    debug!("Live assist suggested answer; local={}", !target.leaves_machine);
    let answer = on_demand_call(&target, answer_system_prompt(), &prompt.user, &cancel).await?;
    let text = answer.trim().to_string();
    record_usage(app, state, prompt.input_chars, text.chars().count());
    if let Ok(mut guard) = state.lock() {
        guard.shown.remember("respostas", &text);
    }
    let _ = app.emit(
        "live-assist-update",
        AssistUpdate {
            items: vec![AssistItem {
                id: Uuid::new_v4().to_string(),
                section: "respostas".to_string(),
                text: text.clone(),
                origin: None,
            }],
        },
    );
    Ok(AnswerResult {
        input_tokens: estimate_tokens(prompt.input_chars),
        output_tokens: estimate_tokens(text.chars().count()),
        paid: target.leaves_machine,
        text,
    })
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

#[cfg(test)]
mod provider_explanation_tests {
    use super::provider_explanation;

    #[test]
    fn keeps_the_friendly_text_of_a_provider_failure() {
        let error = "Sem crédito ou cota esgotada em Claude. Adicione saldo. (HTTP 402: credit balance is too low) | Alternativa OpenAI falhou: x";
        assert_eq!(
            provider_explanation(error).as_deref(),
            Some("Sem crédito ou cota esgotada em Claude. Adicione saldo.")
        );
    }

    #[test]
    fn hides_raw_transport_errors() {
        assert_eq!(provider_explanation("Failed to send request: connection refused"), None);
        assert_eq!(provider_explanation(""), None);
    }
}
