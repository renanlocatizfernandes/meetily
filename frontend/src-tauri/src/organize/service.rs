//! Orchestration: reads a saved meeting, asks the configured summary model
//! for a classification, writes title and classification, emits events.
//!
//! Nothing here runs while a recording is active, and only transcript text
//! already saved on this machine is used. The text goes to the same provider
//! the user configured for summaries (local Ollama by default).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

use super::classify::{
    build_user_prompt, clean_name_list, find_known_client_in_text, heuristic_classification, match_known_client,
    merge_known_client,
    parse_classification, Classification, PromptInput, Rules, MIN_TRANSCRIPT_WORDS, RETRY_SUFFIX, SYSTEM_PROMPT,
};
use super::model::{
    MeetingOrg, OrganizationSettings, OrganizeAllResult, OrganizeProgress, SOURCE_AUTO, SOURCE_MANUAL,
};
use super::store::{self, AutoClassification, AutoWrite, ManualWrite};
use super::text::{clamp_chars, fold, squash, word_count};
use super::title::{render_title, TitleParts};

pub const EVENT_ORGANIZED: &str = "meeting-organized";
pub const EVENT_PROGRESS: &str = "meeting-organize-progress";

const LLM_TIMEOUT: Duration = Duration::from_secs(120);
const RECORDING_POLL: Duration = Duration::from_secs(5);
const RECORDING_WAIT_LIMIT: Duration = Duration::from_secs(6 * 60 * 60);
const AUTO_START_DELAY: Duration = Duration::from_secs(3);
/// The live-assist "Contexto da reunião" is one global field. It is only
/// trusted for a meeting recorded recently.
const CONTEXT_MAX_AGE_HOURS: i64 = 12;
const MAX_FIELD_CHARS: usize = 60;
const MAX_MANUAL_TAGS: usize = 12;
const MAX_TAG_CHARS: usize = 24;

/// One classification at a time: the local model is shared with summaries
/// and live assist, and two runs for the same meeting must not interleave.
static ORGANIZE_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
static BATCH_RUNNING: AtomicBool = AtomicBool::new(false);
static BATCH_TOKEN: LazyLock<Mutex<Option<CancellationToken>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Organized(MeetingOrg),
    /// Title and classification are both manual and `force` was not set.
    Unchanged(MeetingOrg),
    /// Fewer than ~40 words of transcript.
    TooShort(MeetingOrg),
    /// A recording was active; nothing was done.
    Deferred,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OrganizeOptions<'a> {
    pub force: bool,
    pub cancel: Option<&'a CancellationToken>,
    /// Give up (with `Outcome::Deferred`) instead of running during a recording.
    pub defer_while_recording: bool,
}

pub fn pool_from<R: Runtime>(app: &AppHandle<R>) -> Result<SqlitePool, String> {
    Ok(app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone())
}

pub async fn recording_active() -> bool {
    crate::audio::recording_commands::is_recording().await
}

pub fn local_time(created: DateTime<Utc>) -> NaiveDateTime {
    created.with_timezone(&Local).naive_local()
}

fn emit_organized<R: Runtime>(app: &AppHandle<R>, meeting: &MeetingOrg) {
    if let Err(error) = app.emit(EVENT_ORGANIZED, meeting) {
        log::warn!("Organize: could not emit {}: {}", EVENT_ORGANIZED, error);
    }
}

// ---------------------------------------------------------------------------
// Pure planning
// ---------------------------------------------------------------------------

/// Decides what an automatic run may write, given what is manual.
pub fn plan_auto_write(
    current: &MeetingOrg,
    classification: &Classification,
    settings: &OrganizationSettings,
    force: bool,
    when: NaiveDateTime,
    from_model: bool,
) -> AutoWrite {
    let classification_locked = current.classification_is_manual() && !force;
    let title_locked = current.title_is_manual() && !force;
    let (client, project, category) = if classification_locked {
        (current.client.clone(), current.project.clone(), current.category.clone())
    } else {
        (
            classification.client.clone(),
            classification.project.clone(),
            Some(classification.category.clone()),
        )
    };
    let title = (!title_locked).then(|| {
        render_title(
            &settings.title_template,
            &TitleParts {
                client: client.as_deref(),
                project: project.as_deref(),
                subject: &classification.subject,
                category: category.as_deref(),
                when,
            },
        )
    });
    AutoWrite {
        subject: classification.subject.clone(),
        title,
        classification: (!classification_locked).then(|| AutoClassification {
            client,
            project,
            category,
            tags: classification.tags.clone(),
        }),
        force,
        mark_organized: from_model,
    }
}

/// Manual edit from the UI. `None` leaves a field alone, an empty string (or
/// an empty tag list) clears it.
#[derive(Debug, Clone, Default)]
pub struct ManualEdit {
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub pinned: Option<bool>,
}

fn edited_field(edit: Option<&String>, current: &Option<String>) -> Option<String> {
    match edit {
        None => current.clone(),
        Some(value) => Some(clamp_chars(&squash(value), MAX_FIELD_CHARS)).filter(|text| !text.is_empty()),
    }
}

pub fn plan_manual_write(
    current: &MeetingOrg,
    edit: &ManualEdit,
    settings: &OrganizationSettings,
    when: NaiveDateTime,
) -> ManualWrite {
    let client = edited_field(edit.client.as_ref(), &current.client)
        .map(|name| match_known_client(&name, &settings.known_clients).unwrap_or(name));
    let project = edited_field(edit.project.as_ref(), &current.project);
    let category = edited_field(edit.category.as_ref(), &current.category).map(|name| {
        settings
            .categories
            .iter()
            .find(|candidate| fold(candidate) == fold(&name))
            .cloned()
            .unwrap_or(name)
    });
    let tags = match &edit.tags {
        None => current.tags.clone(),
        Some(tags) => {
            let lowered: Vec<String> = tags
                .iter()
                .map(|tag| tag.trim().trim_start_matches('#').to_lowercase())
                .collect();
            clean_name_list(&lowered, MAX_MANUAL_TAGS, MAX_TAG_CHARS)
        }
    };
    let classified = edit.client.is_some() || edit.project.is_some() || edit.category.is_some() || edit.tags.is_some();
    let title_fields_changed =
        client != current.client || project != current.project || category != current.category;
    let title = match (&current.subject, current.title_source == SOURCE_AUTO && title_fields_changed) {
        (Some(subject), true) => Some(render_title(
            &settings.title_template,
            &TitleParts {
                client: client.as_deref(),
                project: project.as_deref(),
                subject,
                category: category.as_deref(),
                when,
            },
        )),
        _ => None,
    };
    ManualWrite {
        client,
        project,
        category,
        tags,
        classification_source: if classified {
            SOURCE_MANUAL.to_string()
        } else {
            current.classification_source.clone()
        },
        pinned: edit.pinned.unwrap_or(current.pinned),
        title,
    }
}

// ---------------------------------------------------------------------------
// Model call
// ---------------------------------------------------------------------------

async fn ask_model<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    user_prompt: &str,
    cancel: Option<&CancellationToken>,
) -> Result<String, String> {
    complete_text(app, pool, SYSTEM_PROMPT, user_prompt, LLM_TIMEOUT, cancel)
        .await
        .map(|completion| completion.0)
}

/// Runs one prompt on the provider configured for summaries (same routing as
/// the classifier). Returns the text and whether it left this machine.
pub(crate) async fn complete_text<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    system_prompt: &str,
    user_prompt: &str,
    timeout: Duration,
    cancel: Option<&CancellationToken>,
) -> Result<(String, bool), String> {
    let hint = crate::live_assist::load_summary_hint(pool).await;
    let provider = hint.provider.trim().to_ascii_lowercase();
    let uses_summary_model = matches!(
        provider.as_str(),
        "ollama" | "openai" | "claude" | "groq" | "openrouter" | "custom-openai"
    );
    // Same rule as live assist: a cloud summary provider is used as configured;
    // anything else goes to local Ollama. With Ollama as the summary provider
    // the summary model is used; with the built-in model, the live-assist one.
    let model = if uses_summary_model {
        String::new()
    } else {
        crate::live_assist::settings_store::load_settings(pool)
            .await
            .map(|settings| settings.model)
            .unwrap_or_default()
    };
    let target = crate::live_assist::resolve_target(&hint, &model)?;
    let client = reqwest::Client::new();
    let app_data = app.path().app_data_dir().ok();
    let request = crate::summary::llm_client::generate_summary(
        &client,
        &target.provider,
        &target.model,
        &target.api_key,
        system_prompt,
        user_prompt,
        target.ollama_endpoint.as_deref(),
        target.custom_openai_endpoint.as_deref(),
        target.max_tokens,
        target.temperature,
        target.top_p,
        app_data.as_ref(),
        cancel,
    );
    match tokio::time::timeout(timeout, request).await {
        Ok(Ok(completion)) => Ok((completion.content, target.leaves_machine)),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("tempo esgotado".to_string()),
    }
}

/// Model first (one retry on an invalid answer), heuristic after that.
/// The boolean says whether the model produced the result. `Err` only when
/// the run was cancelled.
async fn classify<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    input: &PromptInput<'_>,
    rules: &Rules<'_>,
    cancel: Option<&CancellationToken>,
) -> Result<(Classification, bool), String> {
    let cancelled = || cancel.map_or(false, CancellationToken::is_cancelled);
    let base_prompt = build_user_prompt(input);
    for attempt in 0..2 {
        if cancelled() {
            return Err("Organização cancelada.".to_string());
        }
        let prompt = if attempt == 0 {
            base_prompt.clone()
        } else {
            format!("{base_prompt}{RETRY_SUFFIX}")
        };
        match ask_model(app, pool, &prompt, cancel).await {
            Ok(raw) => match parse_classification(&raw, rules) {
                Ok(classification) => return Ok((classification, true)),
                // Only the reason is logged, never the model's text.
                Err(reason) => log::warn!(
                    "Organize: invalid model answer for {} (attempt {}): {}",
                    meeting_id,
                    attempt + 1,
                    reason
                ),
            },
            Err(error) => {
                if cancelled() {
                    return Err("Organização cancelada.".to_string());
                }
                log::warn!(
                    "Organize: model call failed for {}: {}",
                    meeting_id,
                    clamp_chars(&error, 160)
                );
                break;
            }
        }
    }
    log::info!("Organize: using the heuristic fallback for {}", meeting_id);
    Ok((heuristic_classification(input.transcript, input.summary, rules), false))
}

async fn live_assist_context(pool: &SqlitePool, created: DateTime<Utc>) -> Option<String> {
    let age = Utc::now().signed_duration_since(created);
    if age.num_hours() >= CONTEXT_MAX_AGE_HOURS || age.num_seconds() < 0 {
        return None;
    }
    crate::live_assist::settings_store::load_settings(pool)
        .await
        .ok()
        .map(|settings| settings.meeting_context.trim().to_string())
        .filter(|context| !context.is_empty())
}

// ---------------------------------------------------------------------------
// Organize one meeting
// ---------------------------------------------------------------------------

fn db_error(error: sqlx::Error) -> String {
    log::error!("Organize: database error: {}", error);
    "Não foi possível acessar o banco de dados.".to_string()
}

async fn load_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<MeetingOrg, String> {
    store::get_meeting(pool, meeting_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "Conversa não encontrada.".to_string())
}

pub async fn organize<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    options: OrganizeOptions<'_>,
) -> Result<Outcome, String> {
    let _guard = ORGANIZE_LOCK.lock().await;
    if options.defer_while_recording && recording_active().await {
        return Ok(Outcome::Deferred);
    }
    let current = load_meeting(pool, meeting_id).await?;
    if !options.force && current.title_is_manual() && current.classification_is_manual() {
        return Ok(Outcome::Unchanged(current));
    }

    let transcript = store::transcript_text(pool, meeting_id).await.map_err(db_error)?;
    if word_count(&transcript) < MIN_TRANSCRIPT_WORDS {
        return Ok(Outcome::TooShort(current));
    }
    let summary = store::summary_markdown(pool, meeting_id).await.map_err(db_error)?;
    let settings = store::load_settings(pool).await.map_err(db_error)?;
    let context = live_assist_context(pool, current.created).await;
    // Um título que ainda é o de origem (nome da reunião no Teams, por
    // exemplo) ajuda a achar o cliente.
    let context = if current.title_source == "default" && !crate::workspace::is_generic_title(&current.title) {
        let line = format!("Nome da reunião: {}", current.title.trim());
        Some(match context {
            Some(context) => format!("{line}\n{context}"),
            None => line,
        })
    } else {
        context
    };
    let when = local_time(current.created);
    let date_label = when.format("%d/%m/%Y %H:%M").to_string();

    let evidence = fold(&format!(
        "{}\n{}\n{}",
        transcript,
        summary.as_deref().unwrap_or_default(),
        context.as_deref().unwrap_or_default()
    ));
    let rules = Rules {
        known_clients: &settings.known_clients,
        categories: &settings.categories,
        evidence: &evidence,
    };
    let input = PromptInput {
        transcript: &transcript,
        summary: summary.as_deref(),
        context: context.as_deref(),
        date_label: &date_label,
        known_clients: &settings.known_clients,
        categories: &settings.categories,
    };
    let (mut classification, from_model) = classify(app, pool, meeting_id, &input, &rules, options.cancel).await?;
    // Apelidos viram o nome oficial; o domínio de e-mail dos participantes
    // vale mais que o texto; sem cliente, um apelido dito na conversa serve.
    let client_rules = crate::workspace::clients::load_rules(pool).await.unwrap_or_default();
    if let Some(official) = classification
        .client
        .as_deref()
        .and_then(|name| crate::workspace::clients::canonical_client(&client_rules, name))
    {
        classification.client = Some(official);
    }
    if let Ok(Some(by_domain)) = crate::workspace::clients::client_from_participants(pool, meeting_id).await {
        classification.client = Some(by_domain);
    } else if classification.client.is_none() {
        let aliases = crate::workspace::clients::alias_index(&client_rules);
        let names: Vec<String> = aliases.iter().map(|(alias, _)| alias.clone()).collect();
        if let Some(found) = find_known_client_in_text(&evidence, &names) {
            classification.client = aliases
                .iter()
                .find(|(alias, _)| fold(alias) == fold(&found))
                .map(|(_, official)| official.clone());
        }
    }

    // The model can take a while: read the row again so a rename or a manual
    // classification made meanwhile is respected.
    let current = load_meeting(pool, meeting_id).await?;
    if !options.force && current.title_is_manual() && current.classification_is_manual() {
        return Ok(Outcome::Unchanged(current));
    }
    let write = plan_auto_write(&current, &classification, &settings, options.force, when, from_model);
    if !store::write_auto(pool, meeting_id, &write).await.map_err(db_error)? {
        return Err("Conversa não encontrada.".to_string());
    }
    if let Some(client) = write.classification.as_ref().and_then(|c| c.client.as_deref()) {
        remember_client(pool, client).await;
    }

    let updated = load_meeting(pool, meeting_id).await?;
    log::info!(
        "Organize: meeting {} organized (model answer: {}, title source: {}, classification source: {})",
        meeting_id,
        from_model,
        updated.title_source,
        updated.classification_source
    );
    emit_organized(app, &updated);
    Ok(Outcome::Organized(updated))
}

/// Adds a client to `known_clients` when it is new. Reads the settings again
/// so a concurrent save from the settings screen is not lost.
async fn remember_client(pool: &SqlitePool, client: &str) {
    let Ok(mut settings) = store::load_settings(pool).await else {
        return;
    };
    if let Some(list) = merge_known_client(&settings.known_clients, client) {
        settings.known_clients = list;
        if let Err(error) = store::save_settings(pool, &settings).await {
            log::warn!("Organize: could not save the known clients list: {}", error);
        }
    }
}

pub async fn update_classification<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    edit: ManualEdit,
) -> Result<MeetingOrg, String> {
    let current = load_meeting(pool, meeting_id).await?;
    let settings = store::load_settings(pool).await.map_err(db_error)?;
    let write = plan_manual_write(&current, &edit, &settings, local_time(current.created));
    if !store::write_manual(pool, meeting_id, &write).await.map_err(db_error)? {
        return Err("Conversa não encontrada.".to_string());
    }
    if edit.client.is_some() {
        if let Some(client) = write.client.as_deref() {
            remember_client(pool, client).await;
        }
    }
    let updated = load_meeting(pool, meeting_id).await?;
    emit_organized(app, &updated);
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Automatic trigger
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoTrigger {
    /// A recording or an imported file was saved to the database.
    TranscriptSaved,
    /// A summary finished; the title can now use it.
    SummaryCompleted,
}

/// Schedules `organize(meeting, force = false)` in the background when
/// "organizar automaticamente" is on. Never blocks the caller, never runs
/// during a recording, and only logs its errors.
pub fn schedule_auto_organize<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, trigger: AutoTrigger) {
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_auto(&app, &meeting_id, trigger).await {
            log::warn!("Organize: automatic run for {} did not finish: {}", meeting_id, error);
        }
    });
}

async fn auto_enabled(pool: &SqlitePool) -> bool {
    store::load_settings(pool)
        .await
        .map(|settings| settings.auto_organize)
        .unwrap_or(false)
}

async fn run_auto<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, trigger: AutoTrigger) -> Result<(), String> {
    let pool = pool_from(app)?;
    if !auto_enabled(&pool).await {
        return Ok(());
    }
    // The transcript is saved while the stop flow is still finishing.
    tokio::time::sleep(AUTO_START_DELAY).await;
    let started = std::time::Instant::now();
    loop {
        while recording_active().await {
            if started.elapsed() > RECORDING_WAIT_LIMIT {
                return Err("uma gravação ficou ativa por tempo demais".to_string());
            }
            tokio::time::sleep(RECORDING_POLL).await;
        }
        if !auto_enabled(&pool).await {
            return Ok(());
        }
        let Some(meeting) = store::get_meeting(&pool, meeting_id).await.map_err(db_error)? else {
            return Ok(());
        };
        if trigger == AutoTrigger::SummaryCompleted && meeting.title_is_manual() {
            return Ok(());
        }
        let options = OrganizeOptions {
            force: false,
            cancel: None,
            defer_while_recording: true,
        };
        match organize(app, &pool, meeting_id, options).await? {
            Outcome::Deferred => continue,
            Outcome::TooShort(_) => {
                log::info!("Organize: {} has too little text, title kept", meeting_id);
                return Ok(());
            }
            Outcome::Organized(_) | Outcome::Unchanged(_) => return Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Batch
// ---------------------------------------------------------------------------

struct BatchGuard;

impl Drop for BatchGuard {
    fn drop(&mut self) {
        if let Ok(mut token) = BATCH_TOKEN.lock() {
            *token = None;
        }
        BATCH_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub fn cancel_all() -> bool {
    match BATCH_TOKEN.lock() {
        Ok(token) => match token.as_ref() {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        },
        Err(_) => false,
    }
}

/// Which meetings a batch run looks at.
pub fn batch_candidates(meetings: Vec<MeetingOrg>, only_unorganized: bool) -> Vec<MeetingOrg> {
    meetings
        .into_iter()
        .filter(|meeting| !(meeting.title_is_manual() && meeting.classification_is_manual()))
        .filter(|meeting| !only_unorganized || meeting.organized_at.is_none())
        .collect()
}

pub async fn organize_all<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    only_unorganized: bool,
) -> Result<OrganizeAllResult, String> {
    if recording_active().await {
        return Err("Não é possível organizar as conversas durante uma gravação.".to_string());
    }
    if BATCH_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A organização das conversas já está em andamento.".to_string());
    }
    let _guard = BatchGuard;
    let token = CancellationToken::new();
    if let Ok(mut slot) = BATCH_TOKEN.lock() {
        *slot = Some(token.clone());
    }

    // Oldest first, so the sidebar fills in chronological order.
    let mut meetings = batch_candidates(store::list_meetings(pool).await.map_err(db_error)?, only_unorganized);
    meetings.reverse();
    let total = u32::try_from(meetings.len()).unwrap_or(u32::MAX);
    let mut result = OrganizeAllResult::default();

    for (position, meeting) in meetings.iter().enumerate() {
        if token.is_cancelled() || recording_active().await {
            result.cancelled = true;
            break;
        }
        let progress = OrganizeProgress {
            meeting_id: meeting.id.clone(),
            index: u32::try_from(position + 1).unwrap_or(u32::MAX),
            total,
        };
        if let Err(error) = app.emit(EVENT_PROGRESS, &progress) {
            log::warn!("Organize: could not emit {}: {}", EVENT_PROGRESS, error);
        }
        let options = OrganizeOptions {
            force: false,
            cancel: Some(&token),
            defer_while_recording: true,
        };
        match organize(app, pool, &meeting.id, options).await {
            Ok(Outcome::Organized(_)) => result.processed += 1,
            Ok(Outcome::Unchanged(_)) | Ok(Outcome::TooShort(_)) => result.skipped += 1,
            Ok(Outcome::Deferred) => {
                result.cancelled = true;
                break;
            }
            Err(_) if token.is_cancelled() => {
                result.cancelled = true;
                break;
            }
            Err(error) => {
                log::warn!("Organize: batch could not organize {}: {}", meeting.id, error);
                result.failed += 1;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organize::model::{SOURCE_DEFAULT, SOURCE_NONE};
    use chrono::NaiveDate;

    fn when() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    fn meeting() -> MeetingOrg {
        MeetingOrg {
            id: "m1".into(),
            title: "Meeting 18/11".into(),
            created_at: "2026-11-18T12:30:00+00:00".into(),
            client: None,
            project: None,
            category: None,
            tags: vec![],
            title_source: SOURCE_DEFAULT.into(),
            classification_source: SOURCE_NONE.into(),
            pinned: false,
            duration_seconds: None,
            subject: None,
            organized_at: None,
            created: DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    fn classification() -> Classification {
        Classification {
            client: Some("Boreal".into()),
            project: Some("FinOps".into()),
            subject: "Revisão de custos OCI".into(),
            category: "Cliente".into(),
            tags: vec!["custos".into()],
        }
    }

    #[test]
    fn auto_plan_writes_title_and_classification() {
        let write = plan_auto_write(&meeting(), &classification(), &OrganizationSettings::default(), false, when(), true);
        assert_eq!(write.title.as_deref(), Some("Boreal · Revisão de custos OCI · 18/11/2026"));
        let class = write.classification.unwrap();
        assert_eq!(class.client.as_deref(), Some("Boreal"));
        assert_eq!(class.category.as_deref(), Some("Cliente"));
        assert!(write.mark_organized);
    }

    #[test]
    fn auto_plan_keeps_manual_parts_unless_forced() {
        let mut current = meeting();
        current.title_source = SOURCE_MANUAL.into();
        let write = plan_auto_write(&current, &classification(), &OrganizationSettings::default(), false, when(), true);
        assert_eq!(write.title, None);
        assert!(write.classification.is_some());

        let mut current = meeting();
        current.classification_source = SOURCE_MANUAL.into();
        current.client = Some("Cedro".into());
        current.category = Some("Suporte".into());
        let write = plan_auto_write(&current, &classification(), &OrganizationSettings::default(), false, when(), false);
        // The title uses the client the user chose, with the new subject.
        assert_eq!(write.title.as_deref(), Some("Cedro · Revisão de custos OCI · 18/11/2026"));
        assert!(write.classification.is_none());
        assert!(!write.mark_organized);

        current.title_source = SOURCE_MANUAL.into();
        let forced = plan_auto_write(&current, &classification(), &OrganizationSettings::default(), true, when(), true);
        assert_eq!(forced.title.as_deref(), Some("Boreal · Revisão de custos OCI · 18/11/2026"));
        assert_eq!(forced.classification.unwrap().client.as_deref(), Some("Boreal"));
        assert!(forced.force);
    }

    #[test]
    fn auto_plan_uses_category_when_there_is_no_client() {
        let mut internal = classification();
        internal.client = None;
        internal.project = None;
        internal.category = "Interna".into();
        internal.subject = "Planejamento do trimestre".into();
        let write = plan_auto_write(&meeting(), &internal, &OrganizationSettings::default(), false, when(), true);
        assert_eq!(write.title.as_deref(), Some("Interna · Planejamento do trimestre · 18/11/2026"));
    }

    #[test]
    fn manual_plan_null_keeps_empty_clears() {
        let mut current = meeting();
        current.client = Some("Boreal".into());
        current.project = Some("FinOps".into());
        current.category = Some("Cliente".into());
        current.tags = vec!["custos".into()];
        current.classification_source = SOURCE_AUTO.into();
        let settings = OrganizationSettings {
            known_clients: vec!["Cedro Logística".into()],
            ..OrganizationSettings::default()
        };

        // Only pinning: nothing else moves and the source is not manual.
        let pin = plan_manual_write(&current, &ManualEdit { pinned: Some(true), ..ManualEdit::default() }, &settings, when());
        assert!(pin.pinned);
        assert_eq!(pin.client.as_deref(), Some("Boreal"));
        assert_eq!(pin.classification_source, SOURCE_AUTO);
        assert_eq!(pin.title, None);

        let edit = ManualEdit {
            client: Some(" cedro  logística ".into()),
            project: Some("".into()),
            category: Some("suporte".into()),
            tags: Some(vec!["#ETL".into(), "etl".into(), " Carga ".into()]),
            pinned: None,
        };
        let write = plan_manual_write(&current, &edit, &settings, when());
        assert_eq!(write.client.as_deref(), Some("Cedro Logística"));
        assert_eq!(write.project, None);
        assert_eq!(write.category.as_deref(), Some("Suporte"));
        assert_eq!(write.tags, vec!["etl", "carga"]);
        assert_eq!(write.classification_source, SOURCE_MANUAL);
        assert!(!write.pinned);
        // Title is a placeholder (not automatic): it is not rendered again.
        assert_eq!(write.title, None);

        let cleared = plan_manual_write(&current, &ManualEdit { tags: Some(vec![]), ..ManualEdit::default() }, &settings, when());
        assert!(cleared.tags.is_empty());
        assert_eq!(cleared.classification_source, SOURCE_MANUAL);
    }

    #[test]
    fn manual_plan_rerenders_an_automatic_title() {
        let mut current = meeting();
        current.title_source = SOURCE_AUTO.into();
        current.subject = Some("Revisão de custos OCI".into());
        current.client = Some("Boreal".into());
        current.category = Some("Cliente".into());
        let settings = OrganizationSettings::default();

        let edit = ManualEdit { client: Some("Nortec".into()), ..ManualEdit::default() };
        let write = plan_manual_write(&current, &edit, &settings, when());
        assert_eq!(write.title.as_deref(), Some("Nortec · Revisão de custos OCI · 18/11/2026"));

        // Clearing the client puts the category in its place.
        let edit = ManualEdit { client: Some("".into()), ..ManualEdit::default() };
        let write = plan_manual_write(&current, &edit, &settings, when());
        assert_eq!(write.title.as_deref(), Some("Cliente · Revisão de custos OCI · 18/11/2026"));

        // Tags alone do not change the title.
        let edit = ManualEdit { tags: Some(vec!["x".into()]), ..ManualEdit::default() };
        assert_eq!(plan_manual_write(&current, &edit, &settings, when()).title, None);

        current.title_source = SOURCE_MANUAL.into();
        let edit = ManualEdit { client: Some("Nortec".into()), ..ManualEdit::default() };
        assert_eq!(plan_manual_write(&current, &edit, &settings, when()).title, None);
    }

    #[test]
    fn batch_candidates_skip_manual_and_already_organized() {
        let mut manual = meeting();
        manual.id = "manual".into();
        manual.title_source = SOURCE_MANUAL.into();
        manual.classification_source = SOURCE_MANUAL.into();
        let mut done = meeting();
        done.id = "done".into();
        done.organized_at = Some("2026-11-18T12:00:00+00:00".into());
        let fresh = meeting();

        let all = batch_candidates(vec![manual.clone(), done.clone(), fresh.clone()], false);
        assert_eq!(all.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["done", "m1"]);
        let pending = batch_candidates(vec![manual, done, fresh], true);
        assert_eq!(pending.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["m1"]);
    }
}
