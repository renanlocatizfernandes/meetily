//! Database access for meeting organization.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::database::repositories::transcript::TranscriptsRepository;

use super::model::{
    FacetCount, MeetingFacets, MeetingOrg, OrganizationSettings, SOURCE_DEFAULT, SOURCE_NONE,
};

const MEETING_SELECT: &str = "SELECT m.id, m.title, m.created_at, m.client, m.project, m.category, m.tags, m.subject,
        m.title_source, m.classification_source, m.pinned, m.organized_at, t.duration
     FROM meetings m
     LEFT JOIN (
        SELECT meeting_id, MAX(audio_end_time) AS duration FROM transcripts GROUP BY meeting_id
     ) t ON t.meeting_id = m.id";

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

fn parse_string_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub async fn load_settings(pool: &SqlitePool) -> Result<OrganizationSettings, sqlx::Error> {
    let row = sqlx::query(
        "SELECT auto_organize, title_template, known_clients, categories FROM organization_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(OrganizationSettings::default());
    };
    let defaults = OrganizationSettings::default();
    let auto_organize: i64 = row.try_get("auto_organize").unwrap_or(1);
    let title_template: String = row.try_get("title_template").unwrap_or(defaults.title_template);
    let known_clients: String = row.try_get("known_clients").unwrap_or_default();
    let categories: String = row.try_get("categories").unwrap_or_default();
    Ok(OrganizationSettings {
        auto_organize: auto_organize != 0,
        title_template,
        known_clients: parse_string_list(&known_clients),
        categories: parse_string_list(&categories),
    }
    .normalized())
}

pub async fn save_settings(
    pool: &SqlitePool,
    settings: &OrganizationSettings,
) -> Result<OrganizationSettings, sqlx::Error> {
    let settings = settings.clone().normalized();
    sqlx::query(
        "INSERT INTO organization_settings (id, auto_organize, title_template, known_clients, categories)
         VALUES ('1', ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            auto_organize = excluded.auto_organize,
            title_template = excluded.title_template,
            known_clients = excluded.known_clients,
            categories = excluded.categories",
    )
    .bind(if settings.auto_organize { 1 } else { 0 })
    .bind(&settings.title_template)
    .bind(serde_json::to_string(&settings.known_clients).unwrap_or_else(|_| "[]".to_string()))
    .bind(serde_json::to_string(&settings.categories).unwrap_or_else(|_| "[]".to_string()))
    .execute(pool)
    .await?;
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Meetings
// ---------------------------------------------------------------------------

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
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

fn meeting_from_row(row: &SqliteRow) -> Result<MeetingOrg, sqlx::Error> {
    let created = created_from_row(row);
    let tags: Option<String> = row.try_get("tags")?;
    let pinned: i64 = row.try_get("pinned")?;
    let title_source: Option<String> = row.try_get("title_source")?;
    let classification_source: Option<String> = row.try_get("classification_source")?;
    Ok(MeetingOrg {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        created_at: created.to_rfc3339(),
        client: non_empty(row.try_get("client")?),
        project: non_empty(row.try_get("project")?),
        category: non_empty(row.try_get("category")?),
        tags: tags.as_deref().map(parse_string_list).unwrap_or_default(),
        title_source: non_empty(title_source).unwrap_or_else(|| SOURCE_DEFAULT.to_string()),
        classification_source: non_empty(classification_source).unwrap_or_else(|| SOURCE_NONE.to_string()),
        pinned: pinned != 0,
        duration_seconds: row
            .try_get::<Option<f64>, _>("duration")
            .ok()
            .flatten()
            .filter(|value| value.is_finite() && *value > 0.0),
        subject: non_empty(row.try_get("subject")?),
        organized_at: non_empty(row.try_get("organized_at")?),
        created,
    })
}

pub async fn list_meetings(pool: &SqlitePool) -> Result<Vec<MeetingOrg>, sqlx::Error> {
    let rows = sqlx::query(&format!("{MEETING_SELECT} ORDER BY m.created_at DESC"))
        .fetch_all(pool)
        .await?;
    rows.iter().map(meeting_from_row).collect()
}

pub async fn get_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Option<MeetingOrg>, sqlx::Error> {
    let row = sqlx::query(&format!("{MEETING_SELECT} WHERE m.id = ?"))
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(meeting_from_row).transpose()
}

/// Saved transcript as plain text, one line per segment. The speaker label is
/// written when it changes.
pub async fn transcript_text(pool: &SqlitePool, meeting_id: &str) -> Result<String, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT transcript, speaker, speaker_key FROM transcripts
         WHERE meeting_id = ?
         ORDER BY audio_start_time IS NULL, audio_start_time, rowid",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let aliases: HashMap<String, String> = TranscriptsRepository::speaker_aliases(pool, meeting_id)
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
        let speaker = non_empty(TranscriptsRepository::display_speaker(&aliases, speaker, speaker_key));
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

/// Markdown of the saved summary, when one finished.
pub async fn summary_markdown(pool: &SqlitePool, meeting_id: &str) -> Result<Option<String>, sqlx::Error> {
    let result: Option<Option<String>> = sqlx::query_scalar(
        "SELECT result FROM summary_processes WHERE meeting_id = ? AND status = 'completed'",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;
    Ok(result.flatten().and_then(|raw| summary_markdown_from_result(&raw)))
}

pub fn summary_markdown_from_result(raw: &str) -> Option<String> {
    let text = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(map)) => map.get("markdown")?.as_str()?.to_string(),
        Ok(serde_json::Value::String(text)) => text,
        Ok(_) => return None,
        Err(_) => raw.to_string(),
    };
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn tags_json(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        None
    } else {
        serde_json::to_string(tags).ok()
    }
}

#[derive(Debug, Clone)]
pub struct AutoClassification {
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
}

/// Result of an automatic organization. The `CASE` guards repeat the "manual
/// wins unless forced" rule inside the UPDATE, so a rename typed while the
/// model was answering is not overwritten.
#[derive(Debug, Clone)]
pub struct AutoWrite {
    pub subject: String,
    pub title: Option<String>,
    pub classification: Option<AutoClassification>,
    pub force: bool,
    /// False when the answer came from the fallback heuristic: the meeting
    /// then still counts as "not organized" for a later batch run.
    pub mark_organized: bool,
}

pub async fn write_auto(pool: &SqlitePool, meeting_id: &str, write: &AutoWrite) -> Result<bool, sqlx::Error> {
    let now = Utc::now();
    let classification = write.classification.as_ref();
    let result = sqlx::query(
        "UPDATE meetings SET
            subject = ?1,
            organized_at = CASE WHEN ?2 IS NOT NULL THEN ?2 ELSE organized_at END,
            updated_at = ?3,
            title = CASE WHEN ?4 = 1 AND (?5 = 1 OR title_source != 'manual') THEN ?6 ELSE title END,
            title_source = CASE WHEN ?4 = 1 AND (?5 = 1 OR title_source != 'manual') THEN 'auto' ELSE title_source END,
            client = CASE WHEN ?7 = 1 AND (?5 = 1 OR classification_source != 'manual') THEN ?8 ELSE client END,
            project = CASE WHEN ?7 = 1 AND (?5 = 1 OR classification_source != 'manual') THEN ?9 ELSE project END,
            category = CASE WHEN ?7 = 1 AND (?5 = 1 OR classification_source != 'manual') THEN ?10 ELSE category END,
            tags = CASE WHEN ?7 = 1 AND (?5 = 1 OR classification_source != 'manual') THEN ?11 ELSE tags END,
            classification_source = CASE WHEN ?7 = 1 AND (?5 = 1 OR classification_source != 'manual') THEN 'auto' ELSE classification_source END
         WHERE id = ?12",
    )
    .bind(&write.subject)
    .bind(write.mark_organized.then(|| now.to_rfc3339()))
    .bind(now)
    .bind(if write.title.is_some() { 1 } else { 0 })
    .bind(if write.force { 1 } else { 0 })
    .bind(write.title.as_deref().unwrap_or_default())
    .bind(if classification.is_some() { 1 } else { 0 })
    .bind(classification.and_then(|c| c.client.as_deref()))
    .bind(classification.and_then(|c| c.project.as_deref()))
    .bind(classification.and_then(|c| c.category.as_deref()))
    .bind(classification.and_then(|c| tags_json(&c.tags)))
    .bind(meeting_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(false);
    }
    sync_chunk_name(pool, meeting_id).await;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct ManualWrite {
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub classification_source: String,
    pub pinned: bool,
    /// Applied only while the title is still automatic.
    pub title: Option<String>,
}

pub async fn write_manual(pool: &SqlitePool, meeting_id: &str, write: &ManualWrite) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE meetings SET
            client = ?1,
            project = ?2,
            category = ?3,
            tags = ?4,
            classification_source = ?5,
            pinned = ?6,
            updated_at = ?7,
            title = CASE WHEN ?8 IS NOT NULL AND title_source = 'auto' THEN ?8 ELSE title END
         WHERE id = ?9",
    )
    .bind(write.client.as_deref())
    .bind(write.project.as_deref())
    .bind(write.category.as_deref())
    .bind(tags_json(&write.tags))
    .bind(&write.classification_source)
    .bind(if write.pinned { 1 } else { 0 })
    .bind(Utc::now())
    .bind(write.title.as_deref())
    .bind(meeting_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(false);
    }
    if write.title.is_some() {
        sync_chunk_name(pool, meeting_id).await;
    }
    Ok(true)
}

/// `transcript_chunks.meeting_name` mirrors the title, as the summary flow does.
async fn sync_chunk_name(pool: &SqlitePool, meeting_id: &str) {
    let synced = sqlx::query(
        "UPDATE transcript_chunks SET meeting_name = (SELECT title FROM meetings WHERE id = ?1) WHERE meeting_id = ?1",
    )
    .bind(meeting_id)
    .execute(pool)
    .await;
    if let Err(error) = synced {
        log::warn!("Organize: could not mirror the title of {}: {}", meeting_id, error);
    }
}

// ---------------------------------------------------------------------------
// Facets
// ---------------------------------------------------------------------------

async fn column_facet(pool: &SqlitePool, column: &str) -> Result<Vec<FacetCount>, sqlx::Error> {
    // `column` is one of three fixed names below, never user input.
    let rows = sqlx::query(&format!(
        "SELECT TRIM({column}) AS name, COUNT(*) AS count FROM meetings
         WHERE {column} IS NOT NULL AND TRIM({column}) != ''
         GROUP BY TRIM({column})
         ORDER BY count DESC, name COLLATE NOCASE ASC"
    ))
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(FacetCount {
                name: row.try_get("name")?,
                count: row.try_get("count")?,
            })
        })
        .collect()
}

pub fn count_tags(rows: &[String]) -> Vec<FacetCount> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for raw in rows {
        let mut seen: Vec<String> = Vec::new();
        for tag in parse_string_list(raw) {
            let tag = tag.trim().to_lowercase();
            if tag.is_empty() || seen.contains(&tag) {
                continue;
            }
            seen.push(tag.clone());
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    let mut facets: Vec<FacetCount> = counts
        .into_iter()
        .map(|(name, count)| FacetCount { name, count })
        .collect();
    facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    facets
}

pub async fn facets(pool: &SqlitePool) -> Result<MeetingFacets, sqlx::Error> {
    let tag_rows: Vec<String> =
        sqlx::query_scalar("SELECT tags FROM meetings WHERE tags IS NOT NULL AND tags != ''")
            .fetch_all(pool)
            .await?;
    Ok(MeetingFacets {
        clients: column_facet(pool, "client").await?,
        projects: column_facet(pool, "project").await?,
        categories: column_facet(pool, "category").await?,
        tags: count_tags(&tag_rows),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::database::repositories::meeting::MeetingsRepository;
    use crate::organize::model::{SOURCE_AUTO, SOURCE_MANUAL};
    use sqlx::sqlite::SqlitePoolOptions;

    /// In-memory database with every real migration applied.
    pub(crate) async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    pub(crate) async fn seed_meeting(pool: &SqlitePool, id: &str, title: &str, created: DateTime<Utc>) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(title)
            .bind(created)
            .bind(created)
            .execute(pool)
            .await
            .unwrap();
    }

    pub(crate) async fn seed_segment(pool: &SqlitePool, meeting_id: &str, n: u32, text: &str, speaker: Option<&str>) {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, speaker)
             VALUES (?, ?, ?, '00:00', ?, ?, ?)",
        )
        .bind(format!("{meeting_id}-t{n}"))
        .bind(meeting_id)
        .bind(text)
        .bind(f64::from(n) * 10.0)
        .bind(f64::from(n) * 10.0 + 9.5)
        .bind(speaker)
        .execute(pool)
        .await
        .unwrap();
    }

    fn at(day: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap()
            .and_utc()
    }

    #[tokio::test]
    async fn migration_defaults_and_existing_queries_still_work() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Meeting 18/11", at(18)).await;

        let settings = load_settings(&pool).await.unwrap();
        assert_eq!(settings, OrganizationSettings::default());

        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title_source, SOURCE_DEFAULT);
        assert_eq!(meeting.classification_source, SOURCE_NONE);
        assert!(!meeting.pinned);
        assert!(meeting.tags.is_empty());
        assert_eq!(meeting.client, None);
        assert_eq!(meeting.duration_seconds, None);
        assert_eq!(meeting.created, at(18));

        // `SELECT *` into the old model keeps working with the new columns.
        let legacy = MeetingsRepository::get_meetings(&pool).await.unwrap();
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].title, "Meeting 18/11");
        assert!(get_meeting(&pool, "nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn settings_round_trip() {
        let pool = test_pool().await;
        let saved = save_settings(
            &pool,
            &OrganizationSettings {
                auto_organize: false,
                title_template: "{data} · {assunto}".into(),
                known_clients: vec!["Boreal".into(), "BOREAL".into(), "Cedro".into()],
                categories: vec!["Cliente".into(), "Outros".into()],
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.known_clients, vec!["Boreal", "Cedro"]);
        assert_eq!(load_settings(&pool).await.unwrap(), saved);
    }

    #[tokio::test]
    async fn list_is_newest_first_with_duration_and_transcript_labels() {
        let pool = test_pool().await;
        seed_meeting(&pool, "old", "Antiga", at(10)).await;
        seed_meeting(&pool, "new", "Nova", at(18)).await;
        seed_segment(&pool, "new", 0, "bom dia", Some("Você")).await;
        seed_segment(&pool, "new", 1, "vamos começar", Some("Você")).await;
        seed_segment(&pool, "new", 2, "pode ser", Some("Participantes")).await;
        seed_segment(&pool, "new", 3, "  ", None).await;
        seed_segment(&pool, "new", 4, "sem rótulo", None).await;

        let list = list_meetings(&pool).await.unwrap();
        assert_eq!(list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["new", "old"]);
        assert_eq!(list[0].duration_seconds, Some(49.5));
        assert_eq!(list[1].duration_seconds, None);

        let text = transcript_text(&pool, "new").await.unwrap();
        assert_eq!(text, "Você: bom dia\nvamos começar\nParticipantes: pode ser\nsem rótulo");
    }

    #[tokio::test]
    async fn summary_markdown_only_from_completed_processes() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "x", at(18)).await;
        assert_eq!(summary_markdown(&pool, "m1").await.unwrap(), None);
        sqlx::query(
            "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result)
             VALUES ('m1', 'processing', ?, ?, ?)",
        )
        .bind(at(18))
        .bind(at(18))
        .bind(r###"{"markdown": "## Decisões\n- ok"}"###)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(summary_markdown(&pool, "m1").await.unwrap(), None);
        sqlx::query("UPDATE summary_processes SET status = 'completed'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            summary_markdown(&pool, "m1").await.unwrap().as_deref(),
            Some("## Decisões\n- ok")
        );
        assert_eq!(summary_markdown_from_result("{\"other\": 1}"), None);
        assert_eq!(summary_markdown_from_result("texto solto").as_deref(), Some("texto solto"));
        assert_eq!(summary_markdown_from_result("{\"markdown\": \"  \"}"), None);
    }

    fn auto(title: &str, client: Option<&str>, force: bool) -> AutoWrite {
        AutoWrite {
            subject: "Revisão de custos".into(),
            title: Some(title.into()),
            classification: Some(AutoClassification {
                client: client.map(str::to_string),
                project: None,
                category: Some("Cliente".into()),
                tags: vec!["custos".into(), "oci".into()],
            }),
            force,
            mark_organized: true,
        }
    }

    #[tokio::test]
    async fn auto_write_respects_manual_title_and_classification() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Meeting", at(18)).await;

        assert!(write_auto(&pool, "m1", &auto("Boreal · Revisão de custos · 18/11/2026", Some("Boreal"), false)).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Boreal · Revisão de custos · 18/11/2026");
        assert_eq!(meeting.title_source, SOURCE_AUTO);
        assert_eq!(meeting.classification_source, SOURCE_AUTO);
        assert_eq!(meeting.client.as_deref(), Some("Boreal"));
        assert_eq!(meeting.tags, vec!["custos", "oci"]);
        assert_eq!(meeting.subject.as_deref(), Some("Revisão de custos"));
        assert!(meeting.organized_at.is_some());

        // The user renames: the existing save path marks the title as manual.
        assert!(MeetingsRepository::update_meeting_title(&pool, "m1", "Meu título").await.unwrap());
        assert!(write_auto(&pool, "m1", &auto("Outro título", Some("Cedro"), false)).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Meu título");
        assert_eq!(meeting.title_source, SOURCE_MANUAL);
        assert_eq!(meeting.client.as_deref(), Some("Cedro"));

        // Manual classification is kept too, until forced.
        sqlx::query("UPDATE meetings SET classification_source = 'manual', client = 'Lume'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(write_auto(&pool, "m1", &auto("Outro título", Some("Cedro"), false)).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.client.as_deref(), Some("Lume"));
        assert_eq!(meeting.classification_source, SOURCE_MANUAL);

        assert!(write_auto(&pool, "m1", &auto("Forçado", None, true)).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Forçado");
        assert_eq!(meeting.title_source, SOURCE_AUTO);
        assert_eq!(meeting.classification_source, SOURCE_AUTO);
        assert_eq!(meeting.client, None);

        // A heuristic result does not touch `organized_at`.
        sqlx::query("UPDATE meetings SET organized_at = NULL").execute(&pool).await.unwrap();
        let mut heuristic = auto("Heurístico", None, true);
        heuristic.mark_organized = false;
        assert!(write_auto(&pool, "m1", &heuristic).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Heurístico");
        assert_eq!(meeting.organized_at, None);

        assert!(!write_auto(&pool, "missing", &auto("x", None, true)).await.unwrap());
    }

    #[tokio::test]
    async fn summary_rename_only_replaces_placeholder_titles() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Meeting", at(18)).await;
        assert!(MeetingsRepository::update_meeting_name(&pool, "m1", "Nome do resumo").await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Nome do resumo");
        assert_eq!(meeting.title_source, SOURCE_DEFAULT);

        write_auto(&pool, "m1", &auto("Automático", None, false)).await.unwrap();
        assert!(MeetingsRepository::update_meeting_name(&pool, "m1", "Outro nome").await.unwrap());
        assert_eq!(get_meeting(&pool, "m1").await.unwrap().unwrap().title, "Automático");

        MeetingsRepository::update_meeting_title(&pool, "m1", "Digitado").await.unwrap();
        assert!(MeetingsRepository::update_meeting_name(&pool, "m1", "Outro nome").await.unwrap());
        assert_eq!(get_meeting(&pool, "m1").await.unwrap().unwrap().title, "Digitado");
        assert!(!MeetingsRepository::update_meeting_name(&pool, "missing", "x").await.unwrap());
    }

    #[tokio::test]
    async fn manual_write_rerenders_only_automatic_titles() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Meeting", at(18)).await;
        let manual = |title: &str| ManualWrite {
            client: Some("Cedro".into()),
            project: Some("ETL".into()),
            category: None,
            tags: vec![],
            classification_source: SOURCE_MANUAL.into(),
            pinned: true,
            title: Some(title.into()),
        };
        // Placeholder title: classification changes, title stays.
        assert!(write_manual(&pool, "m1", &manual("Cedro · X")).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Meeting");
        assert_eq!(meeting.client.as_deref(), Some("Cedro"));
        assert_eq!(meeting.project.as_deref(), Some("ETL"));
        assert!(meeting.pinned);
        assert!(meeting.tags.is_empty());
        assert_eq!(meeting.classification_source, SOURCE_MANUAL);

        sqlx::query("UPDATE meetings SET title_source = 'auto'").execute(&pool).await.unwrap();
        assert!(write_manual(&pool, "m1", &manual("Cedro · X")).await.unwrap());
        let meeting = get_meeting(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(meeting.title, "Cedro · X");
        assert_eq!(meeting.title_source, SOURCE_AUTO);
        assert!(!write_manual(&pool, "missing", &manual("x")).await.unwrap());
    }

    #[tokio::test]
    async fn facets_count_clients_projects_categories_and_tags() {
        let pool = test_pool().await;
        for (id, client, category, tags) in [
            ("a", Some("Boreal"), Some("Cliente"), Some(r#"["custos","oci"]"#)),
            ("b", Some("Boreal"), Some("Cliente"), Some(r#"["custos","Custos"]"#)),
            ("c", Some("Cedro"), Some("Suporte"), None),
            ("d", None, Some("Interna"), Some("não é json")),
            ("e", Some("  "), None, Some("[]")),
        ] {
            seed_meeting(&pool, id, id, at(18)).await;
            sqlx::query("UPDATE meetings SET client = ?, category = ?, tags = ?, project = ? WHERE id = ?")
                .bind(client)
                .bind(category)
                .bind(tags)
                .bind(if id == "a" { Some("FinOps") } else { None })
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let facets = facets(&pool).await.unwrap();
        let pairs = |list: &[FacetCount]| list.iter().map(|f| (f.name.clone(), f.count)).collect::<Vec<_>>();
        assert_eq!(pairs(&facets.clients), vec![("Boreal".to_string(), 2), ("Cedro".to_string(), 1)]);
        assert_eq!(pairs(&facets.projects), vec![("FinOps".to_string(), 1)]);
        assert_eq!(
            pairs(&facets.categories),
            vec![("Cliente".to_string(), 2), ("Interna".to_string(), 1), ("Suporte".to_string(), 1)]
        );
        assert_eq!(pairs(&facets.tags), vec![("custos".to_string(), 2), ("oci".to_string(), 1)]);
    }
}
