//! Database access for the autopilot.

use std::collections::BTreeMap;

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::database::repositories::transcript::TranscriptsRepository;

use super::model::{AutopilotResult, AutopilotSettings, Extraction, TaskItem, STATUS_RUNNING};

const MAX_TAGS_PER_MEETING: usize = 12;

fn flag(row: &sqlx::sqlite::SqliteRow, column: &str) -> bool {
    row.try_get::<i64, _>(column).map(|v| v != 0).unwrap_or(false)
}

pub async fn load_settings(pool: &SqlitePool) -> Result<AutopilotSettings, sqlx::Error> {
    let row = sqlx::query(
        "SELECT enabled, step_summary, step_tasks, step_title_tags, step_email, template_id
         FROM autopilot_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(AutopilotSettings::default());
    };
    Ok(AutopilotSettings {
        enabled: flag(&row, "enabled"),
        step_summary: flag(&row, "step_summary"),
        step_tasks: flag(&row, "step_tasks"),
        step_title_tags: flag(&row, "step_title_tags"),
        step_email: flag(&row, "step_email"),
        template_id: row.try_get("template_id").unwrap_or_default(),
    }
    .normalized())
}

pub async fn save_settings(pool: &SqlitePool, settings: &AutopilotSettings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO autopilot_settings (id, enabled, step_summary, step_tasks, step_title_tags, step_email, template_id)
         VALUES ('1', ?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            enabled = excluded.enabled,
            step_summary = excluded.step_summary,
            step_tasks = excluded.step_tasks,
            step_title_tags = excluded.step_title_tags,
            step_email = excluded.step_email,
            template_id = excluded.template_id",
    )
    .bind(settings.enabled as i64)
    .bind(settings.step_summary as i64)
    .bind(settings.step_tasks as i64)
    .bind(settings.step_title_tags as i64)
    .bind(settings.step_email as i64)
    .bind(&settings.template_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn list<T: serde::de::DeserializeOwned>(raw: &str) -> Vec<T> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[]".to_string())
}

pub async fn get_result(pool: &SqlitePool, meeting_id: &str) -> Result<Option<AutopilotResult>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM autopilot_results WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| AutopilotResult {
        meeting_id: row.try_get("meeting_id").unwrap_or_default(),
        status: row.try_get("status").unwrap_or_default(),
        note: row.try_get("note").unwrap_or(None),
        steps: serde_json::from_str::<BTreeMap<String, String>>(&row.try_get::<String, _>("steps").unwrap_or_default())
            .unwrap_or_default(),
        tasks: list::<TaskItem>(&row.try_get::<String, _>("tasks").unwrap_or_default()),
        decisions: list(&row.try_get::<String, _>("decisions").unwrap_or_default()),
        pending: list(&row.try_get::<String, _>("pending").unwrap_or_default()),
        suggested_title: row.try_get("suggested_title").unwrap_or(None),
        title_applied: flag(&row, "title_applied"),
        suggested_tags: list(&row.try_get::<String, _>("suggested_tags").unwrap_or_default()),
        tags_applied: flag(&row, "tags_applied"),
        email_subject: row.try_get("email_subject").unwrap_or(None),
        email_body: row.try_get("email_body").unwrap_or(None),
        provider: row.try_get("provider").unwrap_or(None),
        model: row.try_get("model").unwrap_or(None),
        leaves_machine: flag(&row, "leaves_machine"),
        created_at: row.try_get("created_at").unwrap_or_default(),
        updated_at: row.try_get("updated_at").unwrap_or_default(),
    }))
}

/// Starts (or restarts) the record for a meeting, clearing the previous run.
pub async fn begin_run(pool: &SqlitePool, meeting_id: &str) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO autopilot_results (meeting_id, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(meeting_id) DO UPDATE SET
            status = ?2, note = NULL, steps = '{}', tasks = '[]', decisions = '[]', pending = '[]',
            suggested_title = NULL, title_applied = 0, suggested_tags = '[]', tags_applied = 0,
            email_subject = NULL, email_body = NULL, provider = NULL, model = NULL,
            leaves_machine = 0, created_at = ?3, updated_at = ?3",
    )
    .bind(meeting_id)
    .bind(STATUS_RUNNING)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_steps(pool: &SqlitePool, meeting_id: &str, steps: &BTreeMap<String, String>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE autopilot_results SET steps = ?, updated_at = ? WHERE meeting_id = ?")
        .bind(json(steps))
        .bind(Utc::now().to_rfc3339())
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_model(pool: &SqlitePool, meeting_id: &str, provider: &str, model: &str, leaves_machine: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE autopilot_results SET provider = ?, model = ?, leaves_machine = ?, updated_at = ? WHERE meeting_id = ?")
        .bind(provider)
        .bind(model)
        .bind(leaves_machine as i64)
        .bind(Utc::now().to_rfc3339())
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish(pool: &SqlitePool, meeting_id: &str, status: &str, note: Option<&str>, steps: &BTreeMap<String, String>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE autopilot_results SET status = ?, note = ?, steps = ?, updated_at = ? WHERE meeting_id = ?")
        .bind(status)
        .bind(note)
        .bind(json(steps))
        .bind(Utc::now().to_rfc3339())
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn save_extraction(pool: &SqlitePool, meeting_id: &str, extraction: &Extraction) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE autopilot_results SET tasks = ?, decisions = ?, pending = ?, suggested_title = ?,
            suggested_tags = ?, email_subject = ?, email_body = ?, updated_at = ?
         WHERE meeting_id = ?",
    )
    .bind(json(&extraction.tasks))
    .bind(json(&extraction.decisions))
    .bind(json(&extraction.pending))
    .bind(extraction.title.as_deref())
    .bind(json(&extraction.tags))
    .bind(extraction.email_subject.as_deref())
    .bind(extraction.email_body.as_deref())
    .bind(Utc::now().to_rfc3339())
    .bind(meeting_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub fn merge_tags(existing: &[String], suggested: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in existing.iter().chain(suggested.iter()) {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        let key = tag.to_lowercase();
        if !out.iter().any(|t| t.to_lowercase() == key) {
            out.push(tag.to_string());
        }
        if out.len() >= MAX_TAGS_PER_MEETING {
            break;
        }
    }
    out
}

/// Applies the suggested title when it is still the placeholder from the
/// recording (`title_source = 'default'`) or when `force` (a user click).
/// Returns whether it was written.
pub async fn apply_title(pool: &SqlitePool, meeting_id: &str, title: &str, force: bool) -> Result<bool, sqlx::Error> {
    let source = if force { "manual" } else { "auto" };
    let result = sqlx::query(
        "UPDATE meetings SET title = ?1, title_source = ?2, updated_at = ?3
         WHERE id = ?4 AND (?5 = 1 OR title_source = 'default')",
    )
    .bind(title)
    .bind(source)
    .bind(Utc::now())
    .bind(meeting_id)
    .bind(force as i64)
    .execute(pool)
    .await?;
    let applied = result.rows_affected() > 0;
    if applied {
        mark_applied(pool, meeting_id, "title_applied").await?;
    }
    Ok(applied)
}

/// Adds the suggested tags to the meeting's own, unless the classification was
/// typed by the user (and `force` is off). Returns whether it was written.
pub async fn apply_tags(pool: &SqlitePool, meeting_id: &str, tags: &[String], force: bool) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT tags, classification_source FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let source: String = row.try_get("classification_source").unwrap_or_default();
    if source == "manual" && !force {
        return Ok(false);
    }
    let existing: Vec<String> = row
        .try_get::<Option<String>, _>("tags")
        .unwrap_or(None)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let merged = merge_tags(&existing, tags);
    if merged.is_empty() {
        return Ok(false);
    }
    // Keeps the classification source as it is; only 'none' becomes 'auto'.
    sqlx::query(
        "UPDATE meetings SET tags = ?1, updated_at = ?2,
            classification_source = CASE WHEN classification_source = 'none' THEN 'auto' ELSE classification_source END
         WHERE id = ?3",
    )
    .bind(json(&merged))
    .bind(Utc::now())
    .bind(meeting_id)
    .execute(pool)
    .await?;
    mark_applied(pool, meeting_id, "tags_applied").await?;
    Ok(true)
}

async fn mark_applied(pool: &SqlitePool, meeting_id: &str, column: &str) -> Result<(), sqlx::Error> {
    let column = match column {
        "title_applied" => "title_applied",
        "tags_applied" => "tags_applied",
        _ => return Ok(()),
    };
    sqlx::query(&format!("UPDATE autopilot_results SET {column} = 1 WHERE meeting_id = ?"))
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn meeting_exists(pool: &SqlitePool, meeting_id: &str) -> Result<bool, sqlx::Error> {
    let found: Option<String> = sqlx::query_scalar("SELECT id FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

pub async fn transcript_text(pool: &SqlitePool, meeting_id: &str) -> Result<String, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT transcript, speaker, speaker_key FROM transcripts
         WHERE meeting_id = ?
         ORDER BY audio_start_time IS NULL, audio_start_time, rowid",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let aliases = TranscriptsRepository::speaker_aliases(pool, meeting_id)
        .await
        .unwrap_or_default();
    let mut text = String::new();
    let mut last_speaker: Option<String> = None;
    for row in rows {
        let segment: String = row.try_get("transcript")?;
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let speaker: Option<String> = row.try_get("speaker").unwrap_or(None);
        let speaker_key: Option<String> = row.try_get("speaker_key").unwrap_or(None);
        let speaker = TranscriptsRepository::display_speaker(&aliases, speaker, speaker_key)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if !text.is_empty() {
            text.push('\n');
        }
        if speaker.is_some() && speaker != last_speaker {
            text.push_str(speaker.as_deref().unwrap_or_default());
            text.push_str(": ");
        }
        if speaker.is_some() {
            last_speaker = speaker;
        }
        text.push_str(segment);
    }
    Ok(text)
}

/// Status of the summary process and, when finished, its markdown.
pub async fn summary_state(pool: &SqlitePool, meeting_id: &str) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
    let row = sqlx::query("SELECT status, result FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| {
        let status: String = row.try_get("status").unwrap_or_default();
        let result: Option<String> = row.try_get("result").unwrap_or(None);
        let markdown = result.and_then(|raw| crate::organize::summary_markdown_from_result(&raw));
        (status.to_lowercase(), markdown)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn meeting(pool: &SqlitePool, id: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'Reuniao 2026', '2026-11-20T10:00:00Z', '2026-11-20T10:00:00Z')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn settings_default_to_off_and_roundtrip() {
        let pool = pool().await;
        let defaults = load_settings(&pool).await.unwrap();
        assert!(!defaults.enabled);
        assert!(defaults.step_summary && defaults.step_tasks && defaults.step_title_tags && defaults.step_email);
        let changed = AutopilotSettings {
            enabled: true,
            step_email: false,
            template_id: "retrospective".to_string(),
            ..defaults
        };
        save_settings(&pool, &changed).await.unwrap();
        assert_eq!(load_settings(&pool).await.unwrap(), changed);
    }

    #[tokio::test]
    async fn result_roundtrip_and_restart_clears_previous_run() {
        let pool = pool().await;
        meeting(&pool, "m1").await;
        begin_run(&pool, "m1").await.unwrap();
        let extraction = Extraction {
            tasks: vec![TaskItem {
                text: "Enviar proposta".to_string(),
                owner: Some("Ana".to_string()),
                due: None,
            }],
            decisions: vec!["Plano B".to_string()],
            pending: vec!["Orcamento".to_string()],
            title: Some("Plano".to_string()),
            tags: vec!["plano".to_string()],
            email_subject: Some("Resumo".to_string()),
            email_body: Some("Ola".to_string()),
        };
        save_extraction(&pool, "m1", &extraction).await.unwrap();
        let result = get_result(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(result.status, STATUS_RUNNING);
        assert_eq!(result.tasks[0].owner.as_deref(), Some("Ana"));
        assert_eq!(result.decisions, vec!["Plano B"]);
        assert_eq!(result.email_body.as_deref(), Some("Ola"));
        begin_run(&pool, "m1").await.unwrap();
        let cleared = get_result(&pool, "m1").await.unwrap().unwrap();
        assert!(cleared.tasks.is_empty() && cleared.suggested_title.is_none());
    }

    #[tokio::test]
    async fn title_is_applied_only_while_default_unless_forced() {
        let pool = pool().await;
        meeting(&pool, "m1").await;
        begin_run(&pool, "m1").await.unwrap();
        assert!(apply_title(&pool, "m1", "Titulo auto", false).await.unwrap());
        assert!(get_result(&pool, "m1").await.unwrap().unwrap().title_applied);
        // Now 'auto': a second automatic run must not overwrite it.
        assert!(!apply_title(&pool, "m1", "Outro", false).await.unwrap());
        let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(title, "Titulo auto");
        // A user click forces it and marks it manual.
        assert!(apply_title(&pool, "m1", "Do usuario", true).await.unwrap());
        let source: String = sqlx::query_scalar("SELECT title_source FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(source, "manual");
        assert!(!apply_title(&pool, "m1", "Auto de novo", false).await.unwrap());
    }

    #[tokio::test]
    async fn tags_merge_and_respect_manual_classification() {
        let pool = pool().await;
        meeting(&pool, "m1").await;
        sqlx::query("UPDATE meetings SET tags = '[\"vendas\"]', classification_source = 'manual' WHERE id = 'm1'")
            .execute(&pool)
            .await
            .unwrap();
        begin_run(&pool, "m1").await.unwrap();
        let suggested = vec!["Vendas".to_string(), "plano".to_string()];
        assert!(!apply_tags(&pool, "m1", &suggested, false).await.unwrap());
        assert!(apply_tags(&pool, "m1", &suggested, true).await.unwrap());
        let tags: String = sqlx::query_scalar("SELECT tags FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(tags, "[\"vendas\",\"plano\"]");
        let source: String = sqlx::query_scalar("SELECT classification_source FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(source, "manual");
    }

    #[test]
    fn merge_tags_dedupes_and_caps() {
        let existing: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        let suggested: Vec<String> = vec!["T1".into(), "novo1".into(), "novo2".into(), "novo3".into()];
        let merged = merge_tags(&existing, &suggested);
        assert_eq!(merged.len(), MAX_TAGS_PER_MEETING);
        assert_eq!(merged.iter().filter(|t| t.to_lowercase() == "t1").count(), 1);
    }
}
