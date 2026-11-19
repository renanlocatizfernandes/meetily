//! SQLite access for the live assist context. Called when a recording
//! starts, when the client of the meeting becomes known and when the owner
//! saves the context settings. Never per transcript segment.

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::organize::{fold, load_organization_settings, summary_markdown_from_result};

use super::context::{ContextSettings, PastMeeting};

/// Earlier meetings read for one client. Only the latest ones matter.
pub const PAST_MEETINGS_LIMIT: usize = 4;
/// A saved summary is cut before parsing; decisions and pending items of the
/// templates sit well inside this.
const SUMMARY_CHARS: usize = 6000;
const KNOWN_CLIENTS_LIMIT: i64 = 300;

pub async fn load_context_settings(pool: &SqlitePool) -> Result<ContextSettings, sqlx::Error> {
    let row = sqlx::query(
        "SELECT profile_enabled, about, company, offerings, glossary, use_past_meetings
         FROM live_assist_context_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(ContextSettings::default());
    };
    Ok(ContextSettings {
        profile_enabled: row.try_get::<i64, _>("profile_enabled").unwrap_or(1) != 0,
        about: row.try_get("about").unwrap_or_default(),
        company: row.try_get("company").unwrap_or_default(),
        offerings: row.try_get("offerings").unwrap_or_default(),
        glossary: row.try_get("glossary").unwrap_or_default(),
        use_past_meetings: row.try_get::<i64, _>("use_past_meetings").unwrap_or(1) != 0,
    }
    .normalized())
}

pub async fn save_context_settings(pool: &SqlitePool, settings: &ContextSettings) -> Result<(), sqlx::Error> {
    let settings = settings.clone().normalized();
    sqlx::query(
        "INSERT INTO live_assist_context_settings (id, profile_enabled, about, company, offerings, glossary, use_past_meetings)
         VALUES ('1', ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            profile_enabled = excluded.profile_enabled,
            about = excluded.about,
            company = excluded.company,
            offerings = excluded.offerings,
            glossary = excluded.glossary,
            use_past_meetings = excluded.use_past_meetings",
    )
    .bind(i64::from(settings.profile_enabled))
    .bind(&settings.about)
    .bind(&settings.company)
    .bind(&settings.offerings)
    .bind(&settings.glossary)
    .bind(i64::from(settings.use_past_meetings))
    .execute(pool)
    .await?;
    Ok(())
}

/// Client names the app already knows: the list in the organization settings
/// plus the clients of saved meetings.
pub async fn load_known_clients(pool: &SqlitePool) -> Vec<String> {
    let mut known = load_organization_settings(pool)
        .await
        .map(|settings| settings.known_clients)
        .unwrap_or_default();
    let saved: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT TRIM(client) FROM meetings
         WHERE client IS NOT NULL AND TRIM(client) != '' LIMIT ?",
    )
    .bind(KNOWN_CLIENTS_LIMIT)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for name in saved {
        if !known.iter().any(|existing| fold(existing) == fold(&name)) {
            known.push(name);
        }
    }
    known
}

fn created_from_row(row: &SqliteRow) -> DateTime<Utc> {
    if let Ok(value) = row.try_get::<DateTime<Utc>, _>("created_at") {
        return value;
    }
    if let Ok(value) = row.try_get::<NaiveDateTime, _>("created_at") {
        return DateTime::<Utc>::from_naive_utc_and_offset(value, Utc);
    }
    DateTime::<Utc>::UNIX_EPOCH
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
}

/// The latest saved meetings of `client`, newest first, with their summary.
pub async fn load_past_meetings(pool: &SqlitePool, client: &str) -> Result<Vec<PastMeeting>, sqlx::Error> {
    // SQLite's lower() only folds ASCII, so spellings are matched here.
    let wanted = fold(client.trim());
    let spellings: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT client FROM meetings WHERE client IS NOT NULL AND TRIM(client) != ''",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter(|name: &String| fold(name.trim()) == wanted)
    .take(8)
    .collect();
    if spellings.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; spellings.len()].join(", ");
    let sql = format!(
        "SELECT m.id, m.title, m.created_at, m.project, m.category, m.tags, m.subject,
                (SELECT substr(sp.result, 1, {SUMMARY_CHARS}) FROM summary_processes sp
                  WHERE sp.meeting_id = m.id AND sp.status = 'completed') AS summary
         FROM meetings m
         WHERE m.client IN ({placeholders})
         ORDER BY m.created_at DESC
         LIMIT {PAST_MEETINGS_LIMIT}"
    );
    let mut query = sqlx::query(&sql);
    for spelling in &spellings {
        query = query.bind(spelling);
    }
    let rows = query.fetch_all(pool).await?;
    let mut meetings = Vec::with_capacity(rows.len());
    for row in rows {
        let tags: Option<String> = row.try_get("tags").unwrap_or(None);
        let summary: Option<String> = row.try_get("summary").unwrap_or(None);
        meetings.push(PastMeeting {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            created: created_from_row(&row),
            project: non_empty(row.try_get("project").unwrap_or(None)),
            category: non_empty(row.try_get("category").unwrap_or(None)),
            tags: tags
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                .unwrap_or_default(),
            subject: non_empty(row.try_get("subject").unwrap_or(None)),
            summary_markdown: summary.as_deref().and_then(summary_markdown_from_result),
        });
    }
    Ok(meetings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    fn at(day: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap()
            .and_utc()
    }

    async fn seed(pool: &SqlitePool, id: &str, day: u32, client: Option<&str>, summary: Option<&str>) {
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, client, project, category, tags, subject)
             VALUES (?, ?, ?, ?, ?, 'Portal', 'Cliente', '[\"contrato\"]', 'Renovação')",
        )
        .bind(id)
        .bind(format!("Reunião {id}"))
        .bind(at(day))
        .bind(at(day))
        .bind(client)
        .execute(pool)
        .await
        .unwrap();
        if let Some(summary) = summary {
            sqlx::query(
                "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result)
                 VALUES (?, 'completed', '2026-11-01', '2026-11-01', ?)",
            )
            .bind(id)
            .bind(serde_json::json!({ "markdown": summary }).to_string())
            .execute(pool)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn settings_default_and_round_trip() {
        let pool = test_pool().await;
        assert_eq!(load_context_settings(&pool).await.unwrap(), ContextSettings::default());

        let saved = ContextSettings {
            profile_enabled: false,
            about: "  Renan  ".to_string(),
            company: "Tech Fernandes".to_string(),
            offerings: "Apps".to_string(),
            glossary: "SLA: acordo de nível de serviço".to_string(),
            use_past_meetings: false,
        };
        save_context_settings(&pool, &saved).await.unwrap();
        let loaded = load_context_settings(&pool).await.unwrap();
        assert_eq!(loaded, saved.normalized());
        assert_eq!(loaded.about, "Renan");
        // The per-meeting text of the other table is untouched.
        let live = crate::live_assist::settings_store::load_settings(&pool).await.unwrap();
        assert_eq!(live.meeting_context, "");
    }

    #[tokio::test]
    async fn past_meetings_are_the_latest_of_the_same_client() {
        let pool = test_pool().await;
        seed(&pool, "m1", 1, Some("Aurora"), Some("## Decisões\n- Fechar o contrato anual em dezembro")).await;
        seed(&pool, "m2", 5, Some("aurora "), None).await;
        seed(&pool, "m3", 8, Some("Boreal"), Some("## Decisões\n- Outro cliente, não entra")).await;
        seed(&pool, "m4", 9, None, None).await;
        for (n, day) in (10..=14).enumerate() {
            seed(&pool, &format!("n{n}"), day, Some("Aurora"), None).await;
        }

        let past = load_past_meetings(&pool, "AURORA").await.unwrap();
        assert_eq!(past.len(), PAST_MEETINGS_LIMIT);
        assert_eq!(past[0].id, "n4");
        assert!(past.iter().all(|meeting| meeting.id != "m3" && meeting.id != "m4"));
        assert_eq!(past[0].project.as_deref(), Some("Portal"));
        assert_eq!(past[0].tags, vec!["contrato"]);
        assert_eq!(past[0].created, at(14));

        let boreal = load_past_meetings(&pool, "Boreal").await.unwrap();
        assert_eq!(boreal.len(), 1);
        assert!(boreal[0].summary_markdown.as_deref().unwrap().contains("Outro cliente"));
        assert!(load_past_meetings(&pool, "Ninguém").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn known_clients_join_settings_and_saved_meetings() {
        let pool = test_pool().await;
        seed(&pool, "m1", 1, Some("Aurora"), None).await;
        seed(&pool, "m2", 2, Some("aurora"), None).await;
        seed(&pool, "m3", 3, Some("Cedro"), None).await;
        sqlx::query("UPDATE organization_settings SET known_clients = '[\"Boreal\",\"AURORA\"]' WHERE id = '1'")
            .execute(&pool)
            .await
            .unwrap();
        let known = load_known_clients(&pool).await;
        assert_eq!(known, vec!["Boreal", "AURORA", "Cedro"]);
    }
}
