//! Orchestration: waits for the recording to be over, picks the provider,
//! runs the enabled steps and records what happened.
//!
//! Never blocks the stop flow (it runs in its own task, after the transcript is
//! saved), never runs during a recording, never sends anything, and never logs
//! meeting content.

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

use super::model::{
    AutopilotSettings, Wanted, STATE_DONE, STATE_FAILED, STATE_OFF, STATE_SKIPPED, STATUS_DONE, STATUS_FAILED,
    STATUS_PARTIAL, STATUS_SKIPPED, STEP_EMAIL, STEP_SUMMARY, STEP_TASKS, STEP_TITLE_TAGS,
};
use super::parse::parse_extraction;
use super::plan::{plan_target, Configured, LocalChoice, Plan};
use super::prompt::{build_user_prompt, RETRY_SUFFIX, SYSTEM_PROMPT};
use super::store;

pub const EVENT_PROGRESS: &str = "autopilot-progress";
pub const EVENT_FINISHED: &str = "autopilot-finished";

const AUTO_START_DELAY: Duration = Duration::from_secs(3);
/// The meeting page may start its own automatic summary right after the stop.
const SUMMARY_GRACE: Duration = Duration::from_secs(15);
const RECORDING_POLL: Duration = Duration::from_secs(5);
const RECORDING_WAIT_LIMIT: Duration = Duration::from_secs(6 * 60 * 60);
const SUMMARY_POLL: Duration = Duration::from_secs(2);
const SUMMARY_WAIT_LIMIT: Duration = Duration::from_secs(30 * 60);
const LLM_TIMEOUT: Duration = Duration::from_secs(240);
const MIN_TRANSCRIPT_WORDS: usize = 40;

static RUNNING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
/// One run at a time: the local model is shared with summaries and live assist.
static RUN_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

struct RunningGuard(String);

impl RunningGuard {
    fn acquire(meeting_id: &str) -> Option<Self> {
        let mut running = RUNNING.lock().ok()?;
        if running.insert(meeting_id.to_string()) {
            Some(Self(meeting_id.to_string()))
        } else {
            None
        }
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = RUNNING.lock() {
            running.remove(&self.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Engine: everything that talks to the outside, so the pipeline can be tested
// ---------------------------------------------------------------------------

pub(super) trait Engine {
    /// Runs one completion on the planned provider.
    fn complete<'a>(&'a self, plan: &'a Plan, system: &'a str, user: &'a str)
        -> impl Future<Output = Result<String, String>> + Send + 'a;
    /// Makes sure the summary exists (reusing one that already finished) and
    /// returns its markdown.
    fn summarize<'a>(
        &'a self,
        plan: &'a Plan,
        meeting_id: &'a str,
        transcript: &'a str,
        template_id: &'a str,
    ) -> impl Future<Output = Result<String, String>> + Send + 'a;
    fn progress(&self, meeting_id: &str, step: &str, state: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    Skipped(String),
    Finished(String),
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Pure decision of the final status from the per-step states.
pub(super) fn overall_status(steps: &BTreeMap<String, String>) -> &'static str {
    let enabled: Vec<&String> = steps.values().filter(|state| state.as_str() != STATE_OFF).collect();
    if enabled.is_empty() {
        return STATUS_SKIPPED;
    }
    let done = enabled.iter().filter(|state| state.as_str() == STATE_DONE).count();
    if done == enabled.len() {
        STATUS_DONE
    } else if done == 0 {
        STATUS_FAILED
    } else {
        STATUS_PARTIAL
    }
}

fn initial_steps(settings: &AutopilotSettings) -> BTreeMap<String, String> {
    let state = |on: bool| if on { STATE_SKIPPED } else { STATE_OFF };
    BTreeMap::from([
        (STEP_SUMMARY.to_string(), state(settings.step_summary).to_string()),
        (STEP_TASKS.to_string(), state(settings.step_tasks).to_string()),
        (STEP_TITLE_TAGS.to_string(), state(settings.step_title_tags).to_string()),
        (STEP_EMAIL.to_string(), state(settings.step_email).to_string()),
    ])
}

async fn ask_extraction<E: Engine>(
    engine: &E,
    plan: &Plan,
    prompt: &str,
    wanted: &Wanted,
) -> Result<super::model::Extraction, String> {
    // A retry costs credits, so only local models get one.
    let attempts = if plan.paid { 1 } else { 2 };
    let mut last_error = "resposta inválida".to_string();
    for attempt in 0..attempts {
        let user = if attempt == 0 {
            prompt.to_string()
        } else {
            format!("{prompt}{RETRY_SUFFIX}")
        };
        match engine.complete(plan, SYSTEM_PROMPT, &user).await {
            Ok(raw) => match parse_extraction(&raw, wanted) {
                Ok(extraction) => return Ok(extraction),
                Err(reason) => {
                    log::warn!("Autopilot: invalid model answer (attempt {}): {}", attempt + 1, reason);
                    last_error = reason;
                }
            },
            Err(error) => {
                // Error bodies of local servers may echo the prompt; log only the size.
                log::warn!("Autopilot: model call failed ({} chars of error)", error.chars().count());
                return Err("falha ao chamar o modelo".to_string());
            }
        }
    }
    Err(last_error)
}

pub(super) async fn pipeline<E: Engine>(
    engine: &E,
    pool: &SqlitePool,
    meeting_id: &str,
    settings: &AutopilotSettings,
    configured: &Configured,
    local: Option<&LocalChoice>,
    today: &str,
) -> Result<Outcome, String> {
    let db = |e: sqlx::Error| {
        log::error!("Autopilot: database error: {}", e);
        "Não foi possível acessar o banco de dados.".to_string()
    };
    store::begin_run(pool, meeting_id).await.map_err(db)?;
    let mut steps = initial_steps(settings);

    let transcript = store::transcript_text(pool, meeting_id).await.map_err(db)?;
    if word_count(&transcript) < MIN_TRANSCRIPT_WORDS {
        let note = "Conversa curta demais para o piloto automático.";
        store::finish(pool, meeting_id, STATUS_SKIPPED, Some(note), &steps).await.map_err(db)?;
        return Ok(Outcome::Skipped(note.to_string()));
    }

    let plan = match plan_target(configured, local) {
        Ok(plan) => plan,
        Err(note) => {
            store::finish(pool, meeting_id, STATUS_SKIPPED, Some(&note), &steps).await.map_err(db)?;
            return Ok(Outcome::Skipped(note));
        }
    };
    store::set_model(pool, meeting_id, &plan.provider, &plan.model, plan.paid).await.map_err(db)?;

    let mut notes: Vec<&str> = Vec::new();
    if plan.fallback {
        notes.push("Provedor sem chave: usei o modelo local.");
    }

    // 1. Summary
    let mut summary: Option<String> = None;
    if settings.step_summary {
        match engine.summarize(&plan, meeting_id, &transcript, &settings.template_id).await {
            Ok(markdown) => {
                steps.insert(STEP_SUMMARY.to_string(), STATE_DONE.to_string());
                summary = Some(markdown).filter(|m| !m.trim().is_empty());
            }
            Err(error) => {
                log::warn!("Autopilot: summary step failed ({} chars of error)", error.chars().count());
                steps.insert(STEP_SUMMARY.to_string(), STATE_FAILED.to_string());
                notes.push("O resumo não foi gerado.");
            }
        }
        engine.progress(meeting_id, STEP_SUMMARY, &steps[STEP_SUMMARY]);
        store::set_steps(pool, meeting_id, &steps).await.map_err(db)?;
    } else if let Ok(Some((status, markdown))) = store::summary_state(pool, meeting_id).await {
        if status == "completed" {
            summary = markdown;
        }
    }

    // 2. Extraction (tasks, decisions, pending, title, tags, e-mail) in one call
    let wanted = settings.wanted();
    if wanted.any() {
        let prompt = build_user_prompt(&transcript, summary.as_deref(), &wanted, today);
        match ask_extraction(engine, &plan, &prompt, &wanted).await {
            Ok(extraction) => {
                store::save_extraction(pool, meeting_id, &extraction).await.map_err(db)?;
                if wanted.tasks {
                    steps.insert(STEP_TASKS.to_string(), STATE_DONE.to_string());
                }
                if wanted.title_tags {
                    let mut applied_any = false;
                    if let Some(title) = extraction.title.as_deref() {
                        applied_any |= store::apply_title(pool, meeting_id, title, false).await.map_err(db)?;
                    }
                    if !extraction.tags.is_empty() {
                        applied_any |= store::apply_tags(pool, meeting_id, &extraction.tags, false).await.map_err(db)?;
                    }
                    let produced = extraction.title.is_some() || !extraction.tags.is_empty();
                    steps.insert(
                        STEP_TITLE_TAGS.to_string(),
                        if produced { STATE_DONE } else { STATE_FAILED }.to_string(),
                    );
                    let _ = applied_any;
                }
                if wanted.email {
                    let produced = extraction.email_body.is_some();
                    steps.insert(STEP_EMAIL.to_string(), if produced { STATE_DONE } else { STATE_FAILED }.to_string());
                }
            }
            Err(error) => {
                log::warn!("Autopilot: extraction failed: {}", error);
                for (on, step) in [(wanted.tasks, STEP_TASKS), (wanted.title_tags, STEP_TITLE_TAGS), (wanted.email, STEP_EMAIL)] {
                    if on {
                        steps.insert(step.to_string(), STATE_FAILED.to_string());
                    }
                }
                notes.push("O modelo não devolveu uma resposta utilizável para as demais etapas.");
            }
        }
        for step in [STEP_TASKS, STEP_TITLE_TAGS, STEP_EMAIL] {
            if steps[step] != STATE_OFF {
                engine.progress(meeting_id, step, &steps[step]);
            }
        }
    }

    let status = overall_status(&steps);
    let note = if notes.is_empty() { None } else { Some(notes.join(" ")) };
    store::finish(pool, meeting_id, status, note.as_deref(), &steps).await.map_err(db)?;
    Ok(Outcome::Finished(status.to_string()))
}

// ---------------------------------------------------------------------------
// Tauri engine
// ---------------------------------------------------------------------------

struct TauriEngine<R: Runtime> {
    app: AppHandle<R>,
    pool: SqlitePool,
    summary_grace: Duration,
}

impl<R: Runtime> TauriEngine<R> {
    async fn wait_summary(&self, meeting_id: &str) -> Result<String, String> {
        let started = std::time::Instant::now();
        loop {
            match store::summary_state(&self.pool, meeting_id).await {
                Ok(Some((status, markdown))) => match status.as_str() {
                    "completed" => return Ok(markdown.unwrap_or_default()),
                    "failed" => return Err("o resumo falhou".to_string()),
                    "cancelled" => return Err("o resumo foi cancelado".to_string()),
                    _ => {}
                },
                Ok(None) => return Err("o resumo não foi iniciado".to_string()),
                Err(_) => return Err("não foi possível ler o resumo".to_string()),
            }
            if started.elapsed() > SUMMARY_WAIT_LIMIT {
                return Err("o resumo demorou demais".to_string());
            }
            tokio::time::sleep(SUMMARY_POLL).await;
        }
    }
}

impl<R: Runtime> Engine for TauriEngine<R> {
    fn complete<'a>(&'a self, plan: &'a Plan, system: &'a str, user: &'a str)
        -> impl Future<Output = Result<String, String>> + Send + 'a {
        async move {
            let provider = crate::summary::LLMProvider::from_str(&plan.provider)?;
            let client = reqwest::Client::new();
            let app_data = self.app.path().app_data_dir().ok();
            let request = crate::summary::llm_client::generate_summary(
                &client,
                &provider,
                &plan.model,
                &plan.api_key,
                system,
                user,
                plan.ollama_endpoint.as_deref(),
                plan.custom_endpoint.as_deref(),
                plan.max_tokens,
                plan.temperature,
                plan.top_p,
                app_data.as_ref(),
                None,
            );
            match tokio::time::timeout(LLM_TIMEOUT, request).await {
                Ok(Ok(completion)) => Ok(completion.content),
                Ok(Err(error)) => Err(error),
                Err(_) => Err("tempo esgotado".to_string()),
            }
        }
    }

    fn summarize<'a>(
        &'a self,
        plan: &'a Plan,
        meeting_id: &'a str,
        transcript: &'a str,
        template_id: &'a str,
    ) -> impl Future<Output = Result<String, String>> + Send + 'a {
        async move {
            if !self.summary_grace.is_zero() {
                tokio::time::sleep(self.summary_grace).await;
            }
            match store::summary_state(&self.pool, meeting_id).await {
                Ok(Some((status, markdown))) if status == "completed" && markdown.is_some() => {
                    return Ok(markdown.unwrap_or_default());
                }
                // Someone else (the meeting page) is already generating it.
                Ok(Some((status, _))) if !matches!(status.as_str(), "failed" | "cancelled" | "completed") => {
                    return self.wait_summary(meeting_id).await;
                }
                _ => {}
            }
            crate::summary::commands::api_process_transcript(
                self.app.clone(),
                self.app.state::<AppState>(),
                transcript.to_string(),
                plan.provider.clone(),
                plan.model.clone(),
                Some(meeting_id.to_string()),
                None,
                None,
                None,
                Some(template_id.to_string()),
                None,
                None,
            )
            .await?;
            self.wait_summary(meeting_id).await
        }
    }

    fn progress(&self, meeting_id: &str, step: &str, state: &str) {
        let _ = self.app.emit(
            EVENT_PROGRESS,
            json!({ "meetingId": meeting_id, "step": step, "state": state }),
        );
    }
}

// ---------------------------------------------------------------------------
// Local model discovery
// ---------------------------------------------------------------------------

/// Chooses an Ollama chat model: the preferred one when installed, otherwise
/// the first that is not an embedding model.
pub(super) fn pick_ollama_model(names: &[String], preferred: &str) -> Option<String> {
    let preferred = preferred.trim();
    if !preferred.is_empty() {
        if let Some(found) = names.iter().find(|name| name.as_str() == preferred) {
            return Some(found.clone());
        }
    }
    names
        .iter()
        .find(|name| {
            let lower = name.to_lowercase();
            !lower.contains("embed") && !lower.contains("rerank")
        })
        .cloned()
}

pub(super) async fn find_local<R: Runtime>(app: &AppHandle<R>, configured: &Configured) -> Option<LocalChoice> {
    let endpoint = configured.ollama_endpoint.clone();
    if let Ok(models) = crate::ollama::get_ollama_models(endpoint.clone()).await {
        let names: Vec<String> = models.into_iter().map(|m| m.name).collect();
        if let Some(model) = pick_ollama_model(&names, &configured.model) {
            return Some(LocalChoice {
                provider: "ollama".to_string(),
                model,
                ollama_endpoint: endpoint,
            });
        }
    }
    let state = app.try_state::<crate::summary::summary_engine::ModelManagerState>()?;
    match crate::summary::summary_engine::commands::builtin_ai_get_available_summary_model(app.clone(), state).await {
        Ok(Some(model)) if !model.trim().is_empty() => Some(LocalChoice {
            provider: "builtin-ai".to_string(),
            model,
            ollama_endpoint: None,
        }),
        _ => None,
    }
}

pub(super) async fn load_configured(pool: &SqlitePool) -> Configured {
    let hint = crate::live_assist::load_summary_hint(pool).await;
    Configured {
        provider: hint.provider,
        model: hint.model,
        // For custom-openai the key lives in the custom config and is optional.
        api_key: if hint.custom_endpoint.is_some() {
            hint.custom_api_key.unwrap_or_default()
        } else {
            hint.api_key
        },
        ollama_endpoint: hint.ollama_endpoint,
        custom_endpoint: hint.custom_endpoint,
        max_tokens: hint.max_tokens,
        temperature: hint.temperature,
        top_p: hint.top_p,
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

pub(super) fn pool_from<R: Runtime>(app: &AppHandle<R>) -> Result<SqlitePool, String> {
    Ok(app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone())
}

async fn wait_for_recording_end() -> Result<(), String> {
    let started = std::time::Instant::now();
    while crate::audio::recording_commands::is_recording().await {
        if started.elapsed() > RECORDING_WAIT_LIMIT {
            return Err("uma gravação ficou ativa por tempo demais".to_string());
        }
        tokio::time::sleep(RECORDING_POLL).await;
    }
    Ok(())
}

async fn run<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, forced: bool) -> Result<(), String> {
    let pool = pool_from(app)?;
    let settings = store::load_settings(&pool).await.map_err(|e| e.to_string())?;
    if (!settings.enabled && !forced) || !settings.any_step() {
        return Ok(());
    }
    let Some(_guard) = RunningGuard::acquire(meeting_id) else {
        return Ok(());
    };
    if !forced {
        // The transcript is saved while the stop flow is still finishing.
        tokio::time::sleep(AUTO_START_DELAY).await;
    }
    wait_for_recording_end().await?;
    let _turn = RUN_LOCK.lock().await;
    // The user may have turned it off, or deleted the meeting, while waiting.
    let settings = store::load_settings(&pool).await.map_err(|e| e.to_string())?;
    if (!settings.enabled && !forced) || !settings.any_step() {
        return Ok(());
    }
    if !store::meeting_exists(&pool, meeting_id).await.map_err(|e| e.to_string())? {
        return Ok(());
    }

    let configured = load_configured(&pool).await;
    // Only look for a local model when the configured one cannot be used.
    let local = if super::plan::has_credentials(&configured) {
        None
    } else {
        find_local(app, &configured).await
    };
    let engine = TauriEngine {
        app: app.clone(),
        pool: pool.clone(),
        summary_grace: if forced { Duration::ZERO } else { SUMMARY_GRACE },
    };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let outcome = pipeline(&engine, &pool, meeting_id, &settings, &configured, local.as_ref(), &today).await;
    let status = match &outcome {
        Ok(Outcome::Skipped(_)) => STATUS_SKIPPED.to_string(),
        Ok(Outcome::Finished(status)) => status.clone(),
        Err(_) => STATUS_FAILED.to_string(),
    };
    if outcome.is_err() {
        let _ = store::finish(&pool, meeting_id, STATUS_FAILED, Some("Erro interno do piloto automático."), &BTreeMap::new()).await;
    }
    let _ = app.emit(EVENT_FINISHED, json!({ "meetingId": meeting_id, "status": status }));
    outcome.map(|_| ())
}

/// Schedules the automatic run after a recording was saved. No-op when the
/// autopilot is off. Never blocks the caller.
pub fn schedule<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    spawn(app, meeting_id, false);
}

pub(super) fn spawn<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, forced: bool) {
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run(&app, &meeting_id, forced).await {
            log::warn!("Autopilot: run for {} did not finish: {}", meeting_id, error);
        }
    });
}

pub(super) fn is_running(meeting_id: &str) -> bool {
    RUNNING.lock().map(|running| running.contains(meeting_id)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeEngine {
        answers: Mutex<Vec<Result<String, String>>>,
        summary: Result<String, String>,
        complete_calls: AtomicUsize,
        summarize_calls: AtomicUsize,
        events: Mutex<Vec<(String, String)>>,
        prompts: Mutex<Vec<String>>,
    }

    impl FakeEngine {
        fn new(answers: Vec<Result<String, String>>, summary: Result<String, String>) -> Self {
            Self {
                answers: Mutex::new(answers),
                summary,
                complete_calls: AtomicUsize::new(0),
                summarize_calls: AtomicUsize::new(0),
                events: Mutex::new(Vec::new()),
                prompts: Mutex::new(Vec::new()),
            }
        }
    }

    impl Engine for FakeEngine {
        fn complete<'a>(&'a self, _plan: &'a Plan, _system: &'a str, user: &'a str)
            -> impl Future<Output = Result<String, String>> + Send + 'a {
            async move {
                self.complete_calls.fetch_add(1, Ordering::SeqCst);
                self.prompts.lock().unwrap().push(user.to_string());
                let mut answers = self.answers.lock().unwrap();
                if answers.is_empty() {
                    Err("sem resposta".to_string())
                } else {
                    answers.remove(0)
                }
            }
        }

        fn summarize<'a>(&'a self, _plan: &'a Plan, _meeting_id: &'a str, _transcript: &'a str, _template_id: &'a str)
            -> impl Future<Output = Result<String, String>> + Send + 'a {
            async move {
                self.summarize_calls.fetch_add(1, Ordering::SeqCst);
                self.summary.clone()
            }
        }

        fn progress(&self, _meeting_id: &str, step: &str, state: &str) {
            self.events.lock().unwrap().push((step.to_string(), state.to_string()));
        }
    }

    const GOOD: &str = r#"{"tarefas":[{"texto":"Enviar proposta","responsavel":"Ana","prazo":"2026-12-01"}],"decisoes":["Adotar plano B"],"pendencias":["Definir orcamento"],"titulo":"Revisao do plano","etiquetas":["plano"],"email":{"assunto":"Resumo","corpo":"Ola, segue o resumo."}}"#;

    async fn pool_with_meeting(words: usize) -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Reuniao 2026', '2026-11-20T10:00:00Z', '2026-11-20T10:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        let text = vec!["palavra"; words].join(" ");
        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES ('t1', 'm1', ?, '2026-11-20T10:00:00Z')")
            .bind(text)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn configured(provider: &str, key: &str) -> Configured {
        Configured {
            provider: provider.to_string(),
            model: "m1".to_string(),
            api_key: key.to_string(),
            ..Configured::default()
        }
    }

    fn all_on() -> AutopilotSettings {
        AutopilotSettings {
            enabled: true,
            ..AutopilotSettings::default()
        }
    }

    #[tokio::test]
    async fn runs_every_step_and_applies_title_and_tags() {
        let pool = pool_with_meeting(80).await;
        let engine = FakeEngine::new(vec![Ok(GOOD.to_string())], Ok("# Resumo sintetico".to_string()));
        let outcome = pipeline(&engine, &pool, "m1", &all_on(), &configured("ollama", ""), None, "2026-11-20").await.unwrap();
        assert_eq!(outcome, Outcome::Finished(STATUS_DONE.to_string()));
        assert_eq!(engine.summarize_calls.load(Ordering::SeqCst), 1);
        // One extraction call for tasks + title/tags + e-mail together.
        assert_eq!(engine.complete_calls.load(Ordering::SeqCst), 1);
        assert!(engine.prompts.lock().unwrap()[0].contains("# Resumo sintetico"));
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.status, STATUS_DONE);
        assert_eq!(result.tasks[0].owner.as_deref(), Some("Ana"));
        assert_eq!(result.decisions, vec!["Adotar plano B"]);
        assert_eq!(result.pending, vec!["Definir orcamento"]);
        assert_eq!(result.email_body.as_deref(), Some("Ola, segue o resumo."));
        assert!(result.title_applied && result.tags_applied);
        assert!(!result.leaves_machine);
        let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(title, "Revisao do plano");
        let tags: String = sqlx::query_scalar("SELECT tags FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(tags, "[\"plano\"]");
        let events = engine.events.lock().unwrap();
        assert!(events.iter().any(|(step, state)| step == STEP_SUMMARY && state == STATE_DONE));
    }

    #[tokio::test]
    async fn manual_title_is_kept_but_suggestion_is_stored() {
        let pool = pool_with_meeting(80).await;
        sqlx::query("UPDATE meetings SET title = 'Meu titulo', title_source = 'manual' WHERE id = 'm1'").execute(&pool).await.unwrap();
        let engine = FakeEngine::new(vec![Ok(GOOD.to_string())], Ok("r".to_string()));
        pipeline(&engine, &pool, "m1", &all_on(), &configured("ollama", ""), None, "2026-11-20").await.unwrap();
        let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(title, "Meu titulo");
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.suggested_title.as_deref(), Some("Revisao do plano"));
        assert!(!result.title_applied);
    }

    #[tokio::test]
    async fn each_step_has_its_own_switch() {
        let pool = pool_with_meeting(80).await;
        let engine = FakeEngine::new(
            vec![Ok(r#"{"tarefas":[{"texto":"A"}],"decisoes":[],"pendencias":[]}"#.to_string())],
            Ok("r".to_string()),
        );
        let settings = AutopilotSettings {
            enabled: true,
            step_summary: false,
            step_tasks: true,
            step_title_tags: false,
            step_email: false,
            ..AutopilotSettings::default()
        };
        let outcome = pipeline(&engine, &pool, "m1", &settings, &configured("ollama", ""), None, "2026-11-20").await.unwrap();
        assert_eq!(outcome, Outcome::Finished(STATUS_DONE.to_string()));
        assert_eq!(engine.summarize_calls.load(Ordering::SeqCst), 0);
        let prompt = engine.prompts.lock().unwrap()[0].clone();
        assert!(prompt.contains("\"tarefas\"") && !prompt.contains("\"email\"") && !prompt.contains("\"titulo\""));
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.steps[STEP_SUMMARY], STATE_OFF);
        assert_eq!(result.steps[STEP_EMAIL], STATE_OFF);
        assert!(result.email_body.is_none() && result.suggested_title.is_none());
        let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(title, "Reuniao 2026");
    }

    #[tokio::test]
    async fn no_key_and_no_local_model_does_nothing_and_says_so() {
        let pool = pool_with_meeting(80).await;
        let engine = FakeEngine::new(vec![], Ok("r".to_string()));
        let outcome = pipeline(&engine, &pool, "m1", &all_on(), &configured("openai", ""), None, "2026-11-20").await.unwrap();
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_eq!(engine.summarize_calls.load(Ordering::SeqCst), 0);
        assert_eq!(engine.complete_calls.load(Ordering::SeqCst), 0);
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.status, STATUS_SKIPPED);
        assert!(result.note.unwrap().contains("sem chave"));
    }

    #[tokio::test]
    async fn no_key_uses_the_local_model_when_there_is_one() {
        let pool = pool_with_meeting(80).await;
        let engine = FakeEngine::new(vec![Ok(GOOD.to_string())], Ok("r".to_string()));
        let local = LocalChoice {
            provider: "ollama".to_string(),
            model: "llama3.2:latest".to_string(),
            ollama_endpoint: None,
        };
        let outcome = pipeline(&engine, &pool, "m1", &all_on(), &configured("openai", ""), Some(&local), "2026-11-20").await.unwrap();
        assert_eq!(outcome, Outcome::Finished(STATUS_DONE.to_string()));
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.provider.as_deref(), Some("ollama"));
        assert!(!result.leaves_machine);
        assert!(result.note.unwrap().contains("modelo local"));
    }

    #[tokio::test]
    async fn short_conversation_is_skipped() {
        let pool = pool_with_meeting(10).await;
        let engine = FakeEngine::new(vec![], Ok("r".to_string()));
        let outcome = pipeline(&engine, &pool, "m1", &all_on(), &configured("ollama", ""), None, "2026-11-20").await.unwrap();
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_eq!(engine.complete_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invalid_answers_retry_locally_but_not_on_paid_providers() {
        let pool = pool_with_meeting(80).await;
        let local = FakeEngine::new(vec![Ok("lixo".to_string()), Ok("mais lixo".to_string())], Ok("r".to_string()));
        let outcome = pipeline(&local, &pool, "m1", &all_on(), &configured("ollama", ""), None, "2026-11-20").await.unwrap();
        assert_eq!(local.complete_calls.load(Ordering::SeqCst), 2);
        // The summary worked, the rest failed: partial.
        assert_eq!(outcome, Outcome::Finished(STATUS_PARTIAL.to_string()));

        let paid = FakeEngine::new(vec![Ok("lixo".to_string())], Ok("r".to_string()));
        let outcome = pipeline(&paid, &pool, "m1", &all_on(), &configured("openai", "sk-x"), None, "2026-11-20").await.unwrap();
        assert_eq!(paid.complete_calls.load(Ordering::SeqCst), 1);
        assert_eq!(outcome, Outcome::Finished(STATUS_PARTIAL.to_string()));
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert!(result.leaves_machine);
    }

    #[tokio::test]
    async fn summary_failure_does_not_stop_the_other_steps() {
        let pool = pool_with_meeting(80).await;
        let engine = FakeEngine::new(vec![Ok(GOOD.to_string())], Err("falhou".to_string()));
        let outcome = pipeline(&engine, &pool, "m1", &all_on(), &configured("ollama", ""), None, "2026-11-20").await.unwrap();
        assert_eq!(outcome, Outcome::Finished(STATUS_PARTIAL.to_string()));
        let result = store::get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.steps[STEP_SUMMARY], STATE_FAILED);
        assert_eq!(result.steps[STEP_TASKS], STATE_DONE);
    }

    #[test]
    fn overall_status_rules() {
        let steps = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        assert_eq!(overall_status(&steps(&[("a", "done"), ("b", "off")])), STATUS_DONE);
        assert_eq!(overall_status(&steps(&[("a", "done"), ("b", "failed")])), STATUS_PARTIAL);
        assert_eq!(overall_status(&steps(&[("a", "failed"), ("b", "failed")])), STATUS_FAILED);
        assert_eq!(overall_status(&steps(&[("a", "off")])), STATUS_SKIPPED);
    }

    #[test]
    fn picks_a_chat_model_not_an_embedding_one() {
        let names: Vec<String> = ["nomic-embed-text:latest", "qwen3:8b", "llama3.2:latest"].iter().map(|s| s.to_string()).collect();
        assert_eq!(pick_ollama_model(&names, "llama3.2:latest").as_deref(), Some("llama3.2:latest"));
        assert_eq!(pick_ollama_model(&names, "gpt-4o").as_deref(), Some("qwen3:8b"));
        assert_eq!(pick_ollama_model(&["bge-embed".to_string()], ""), None);
    }

    #[test]
    fn a_meeting_runs_once_at_a_time() {
        let first = RunningGuard::acquire("dup-test").unwrap();
        assert!(RunningGuard::acquire("dup-test").is_none());
        assert!(is_running("dup-test"));
        drop(first);
        assert!(RunningGuard::acquire("dup-test").is_some());
    }
}
