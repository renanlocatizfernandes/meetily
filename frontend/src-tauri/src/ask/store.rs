//! Local passage index: a plain table plus an FTS5 index when SQLite has it.
//!
//! The tables are created at run time (not by a migration) so a SQLite build
//! without FTS5 can never block the app's migrations: the search then falls
//! back to a LIKE scan over the same table.
//!
//! The index only copies text that is already saved in this database.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use tokio::sync::Mutex;

use crate::database::repositories::transcript::TranscriptsRepository;
use crate::organize::summary_markdown_from_result;

use super::chunk::{summary_passages, transcript_passages, NewPassage, Segment};
use super::model::{Candidate, MeetingMeta};
use super::text::fts_query;

/// Serializes index maintenance so two questions never rebuild at once.
static SYNC_LOCK: Mutex<()> = Mutex::const_new(());

const SCHEMA: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS ask_passages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        source TEXT NOT NULL,
        start_sec REAL,
        text TEXT NOT NULL,
        folded TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_ask_passages_meeting ON ask_passages(meeting_id)",
    "CREATE TABLE IF NOT EXISTS ask_index_meta (
        meeting_id TEXT PRIMARY KEY NOT NULL,
        fingerprint TEXT NOT NULL,
        passages INTEGER NOT NULL,
        indexed_at TEXT NOT NULL
    )",
];

const FTS_SCHEMA: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS ask_fts USING fts5(
    folded, content='ask_passages', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
)";

/// Creates the tables. Returns whether the FTS5 index is available.
pub async fn ensure_schema(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    for statement in SCHEMA {
        sqlx::query(statement).execute(pool).await?;
    }
    if sqlx::query(FTS_SCHEMA).execute(pool).await.is_err() {
        log::info!("Ask: FTS5 not available, using the basic search");
        return Ok(false);
    }
    let present: Option<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'ask_fts'")
            .fetch_optional(pool)
            .await?;
    Ok(present.is_some())
}

/// Turns a stored folded text into the indexed form (already folded by the caller).
pub fn folded_text(text: &str) -> String {
    crate::organize::fold(text)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncReport {
    pub indexed_meetings: usize,
    pub changed_meetings: usize,
}

struct MeetingState {
    id: String,
    fingerprint: String,
}

async fn current_fingerprints(pool: &SqlitePool) -> Result<Vec<MeetingState>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT m.id AS id,
            (SELECT COUNT(*) || ':' || COALESCE(SUM(LENGTH(t.transcript)), 0)
               FROM transcripts t WHERE t.meeting_id = m.id) AS tfp,
            (SELECT COALESCE(s.updated_at, '') || ':' || LENGTH(COALESCE(s.result, ''))
               FROM summary_processes s WHERE s.meeting_id = m.id AND s.status = 'completed') AS sfp,
            (SELECT COALESCE(group_concat(a.display_name, '|'), '')
               FROM speaker_aliases a WHERE a.meeting_id = m.id) AS afp
         FROM meetings m",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        let tfp: Option<String> = row.try_get("tfp").unwrap_or(None);
        let sfp: Option<String> = row.try_get("sfp").unwrap_or(None);
        let afp: Option<String> = row.try_get("afp").unwrap_or(None);
        out.push(MeetingState {
            id,
            fingerprint: format!(
                "v1;{};{};{}",
                tfp.unwrap_or_default(),
                sfp.unwrap_or_default(),
                afp.unwrap_or_default()
            ),
        });
    }
    Ok(out)
}

async fn load_segments(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<Segment>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT transcript, speaker, speaker_key, audio_start_time FROM transcripts
         WHERE meeting_id = ?
         ORDER BY audio_start_time IS NULL, audio_start_time, rowid",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let aliases = TranscriptsRepository::speaker_aliases(pool, meeting_id)
        .await
        .unwrap_or_default();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let text: String = row.try_get("transcript")?;
        let speaker: Option<String> = row.try_get("speaker").unwrap_or(None);
        let speaker_key: Option<String> = row.try_get("speaker_key").unwrap_or(None);
        out.push(Segment {
            start_sec: row.try_get::<Option<f64>, _>("audio_start_time").unwrap_or(None),
            speaker: TranscriptsRepository::display_speaker(&aliases, speaker, speaker_key),
            text,
        });
    }
    Ok(out)
}

async fn load_summary(pool: &SqlitePool, meeting_id: &str) -> Result<Option<String>, sqlx::Error> {
    let result: Option<Option<String>> =
        sqlx::query_scalar("SELECT result FROM summary_processes WHERE meeting_id = ? AND status = 'completed'")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await?;
    Ok(result.flatten().and_then(|raw| summary_markdown_from_result(&raw)))
}

async fn remove_meeting(pool: &SqlitePool, meeting_id: &str, fts: bool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    if fts {
        sqlx::query(
            "INSERT INTO ask_fts(ask_fts, rowid, folded)
             SELECT 'delete', id, folded FROM ask_passages WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM ask_passages WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM ask_index_meta WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

async fn write_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
    fingerprint: &str,
    passages: &[NewPassage],
    fts: bool,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    if fts {
        sqlx::query(
            "INSERT INTO ask_fts(ask_fts, rowid, folded)
             SELECT 'delete', id, folded FROM ask_passages WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM ask_passages WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    for passage in passages {
        let folded = folded_text(&passage.text);
        let id = sqlx::query("INSERT INTO ask_passages (meeting_id, source, start_sec, text, folded) VALUES (?, ?, ?, ?, ?)")
            .bind(meeting_id)
            .bind(passage.source)
            .bind(passage.start_sec)
            .bind(&passage.text)
            .bind(&folded)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
        if fts {
            sqlx::query("INSERT INTO ask_fts(rowid, folded) VALUES (?, ?)")
                .bind(id)
                .bind(&folded)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query(
        "INSERT INTO ask_index_meta (meeting_id, fingerprint, passages, indexed_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(meeting_id) DO UPDATE SET
            fingerprint = excluded.fingerprint,
            passages = excluded.passages,
            indexed_at = excluded.indexed_at",
    )
    .bind(meeting_id)
    .bind(fingerprint)
    .bind(passages.len() as i64)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Brings the index up to date: re-reads only meetings whose saved text
/// changed, and drops meetings that no longer exist.
pub async fn sync_index(pool: &SqlitePool, fts: bool, force: bool) -> Result<SyncReport, sqlx::Error> {
    let _guard = SYNC_LOCK.lock().await;
    let current = current_fingerprints(pool).await?;
    let indexed: HashMap<String, String> = sqlx::query("SELECT meeting_id, fingerprint FROM ask_index_meta")
        .fetch_all(pool)
        .await?
        .iter()
        .filter_map(|row| Some((row.try_get::<String, _>("meeting_id").ok()?, row.try_get::<String, _>("fingerprint").ok()?)))
        .collect();

    let live: HashSet<&str> = current.iter().map(|m| m.id.as_str()).collect();
    for orphan in indexed.keys().filter(|id| !live.contains(id.as_str())) {
        remove_meeting(pool, orphan, fts).await?;
    }
    // Passages left without a meta row (an interrupted older run) are removed too.
    let strays: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT meeting_id FROM ask_passages WHERE meeting_id NOT IN (SELECT meeting_id FROM ask_index_meta)",
    )
    .fetch_all(pool)
    .await?;
    for stray in strays {
        remove_meeting(pool, &stray, fts).await?;
    }

    let mut changed = 0usize;
    for meeting in &current {
        if !force && indexed.get(&meeting.id) == Some(&meeting.fingerprint) {
            continue;
        }
        let segments = load_segments(pool, &meeting.id).await?;
        let mut passages = transcript_passages(&segments);
        if let Some(summary) = load_summary(pool, &meeting.id).await? {
            passages.extend(summary_passages(&summary));
        }
        write_meeting(pool, &meeting.id, &meeting.fingerprint, &passages, fts).await?;
        changed += 1;
        // Lets other tasks run between meetings on a first, large indexing.
        tokio::task::yield_now().await;
    }
    Ok(SyncReport {
        indexed_meetings: current.len(),
        changed_meetings: changed,
    })
}

pub async fn indexed_meeting_count(pool: &SqlitePool) -> usize {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ask_index_meta")
        .fetch_one(pool)
        .await
        .map(|n| n.max(0) as usize)
        .unwrap_or(0)
}

pub async fn meeting_count(pool: &SqlitePool) -> usize {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM meetings")
        .fetch_one(pool)
        .await
        .map(|n| n.max(0) as usize)
        .unwrap_or(0)
}

fn candidate_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Candidate, sqlx::Error> {
    Ok(Candidate {
        id: row.try_get("id")?,
        meeting_id: row.try_get("meeting_id")?,
        source: row.try_get("source")?,
        start_sec: row.try_get::<Option<f64>, _>("start_sec").unwrap_or(None),
        text: row.try_get("text")?,
    })
}

/// Passages that contain any of the terms, best text matches first.
pub async fn search_passages(
    pool: &SqlitePool,
    fts: bool,
    terms: &[String],
    limit: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    if fts {
        let Some(query) = fts_query(terms) else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            "SELECT p.id, p.meeting_id, p.source, p.start_sec, p.text
             FROM ask_fts JOIN ask_passages p ON p.id = ask_fts.rowid
             WHERE ask_fts MATCH ?
             ORDER BY bm25(ask_fts)
             LIMIT ?",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        return rows.iter().map(candidate_from_row).collect();
    }
    // Basic search: terms are alphanumeric only, so no LIKE wildcards can appear.
    let clauses = vec!["folded LIKE ?"; terms.len()].join(" OR ");
    let sql = format!(
        "SELECT id, meeting_id, source, start_sec, text FROM ask_passages WHERE {clauses} ORDER BY id DESC LIMIT ?"
    );
    let mut query = sqlx::query(&sql);
    for term in terms {
        query = query.bind(format!("%{term}%"));
    }
    let rows = query.bind(limit).fetch_all(pool).await?;
    rows.iter().map(candidate_from_row).collect()
}

/// A few passages of each given meeting (summary first), for meetings the
/// question matches by metadata (client, project, tags) rather than by text.
pub async fn passages_of_meetings(
    pool: &SqlitePool,
    meeting_ids: &[String],
    per_meeting: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
    let mut out = Vec::new();
    for id in meeting_ids {
        let rows = sqlx::query(
            "SELECT id, meeting_id, source, start_sec, text FROM ask_passages
             WHERE meeting_id = ?
             ORDER BY CASE source WHEN 'summary' THEN 0 ELSE 1 END, id
             LIMIT ?",
        )
        .bind(id)
        .bind(per_meeting)
        .fetch_all(pool)
        .await?;
        for row in &rows {
            out.push(candidate_from_row(row)?);
        }
    }
    Ok(out)
}

pub async fn load_metas(pool: &SqlitePool) -> Result<HashMap<String, MeetingMeta>, sqlx::Error> {
    let meetings = crate::organize::list_organized_meetings(pool).await?;
    Ok(meetings
        .into_iter()
        .map(|m| {
            (
                m.id.clone(),
                MeetingMeta {
                    id: m.id,
                    title: m.title,
                    client: m.client,
                    project: m.project,
                    category: m.category,
                    tags: m.tags,
                    created_at: m.created,
                },
            )
        })
        .collect())
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
        for ddl in [
            "CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
            "CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, transcript TEXT NOT NULL, timestamp TEXT NOT NULL,
                summary TEXT, action_items TEXT, key_points TEXT, audio_start_time REAL, audio_end_time REAL, duration REAL,
                speaker TEXT, speaker_key TEXT)",
            "CREATE TABLE summary_processes (meeting_id TEXT PRIMARY KEY, status TEXT NOT NULL, created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL, error TEXT, result TEXT)",
            "CREATE TABLE speaker_aliases (meeting_id TEXT NOT NULL, speaker_key TEXT NOT NULL, display_name TEXT NOT NULL)",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn add_meeting(pool: &SqlitePool, id: &str, lines: &[(&str, f64)], summary: Option<&str>) {
        sqlx::query("INSERT INTO meetings VALUES (?, ?, '2026-11-10T12:00:00Z', '2026-11-10T12:00:00Z')")
            .bind(id)
            .bind(format!("Reunião {id}"))
            .execute(pool)
            .await
            .unwrap();
        for (i, (text, start)) in lines.iter().enumerate() {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time) VALUES (?, ?, ?, 'x', ?)")
                .bind(format!("{id}-{i}"))
                .bind(id)
                .bind(*text)
                .bind(*start)
                .execute(pool)
                .await
                .unwrap();
        }
        if let Some(summary) = summary {
            sqlx::query("INSERT INTO summary_processes VALUES (?, 'completed', 'x', '2026-11-10', NULL, ?)")
                .bind(id)
                .bind(serde_json::json!({ "markdown": summary }).to_string())
                .execute(pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn indexes_and_finds_with_fts_when_available() {
        let pool = test_pool().await;
        let fts = ensure_schema(&pool).await.unwrap();
        add_meeting(
            &pool,
            "m1",
            &[("Fechamos o contrato com a Acme por doze meses.", 12.0), ("Ana envia a proposta amanhã.", 40.0)],
            Some("## Decisões\n\n- Contrato Acme fechado"),
        )
        .await;
        add_meeting(&pool, "m2", &[("Falamos de férias e do almoço.", 3.0)], None).await;
        let report = sync_index(&pool, fts, false).await.unwrap();
        assert_eq!(report.indexed_meetings, 2);
        assert_eq!(report.changed_meetings, 2);

        let terms = vec!["contrato".to_string(), "acme".to_string()];
        let found = search_passages(&pool, fts, &terms, 20).await.unwrap();
        assert!(!found.is_empty());
        assert!(found.iter().all(|c| c.meeting_id == "m1"));
        assert!(found.iter().any(|c| c.source == "summary"));
        let transcript = found.iter().find(|c| c.source == "transcript").unwrap();
        assert_eq!(transcript.start_sec, Some(12.0));

        // Accents are folded on both sides.
        let accent = search_passages(&pool, fts, &["amanha".to_string()], 20).await.unwrap();
        assert_eq!(accent.len(), 1);
    }

    #[tokio::test]
    async fn basic_search_works_without_fts() {
        let pool = test_pool().await;
        let _ = ensure_schema(&pool).await.unwrap();
        add_meeting(&pool, "m1", &[("O orçamento anual foi aprovado.", 1.0)], None).await;
        sync_index(&pool, false, false).await.unwrap();
        let found = search_passages(&pool, false, &["orcamento".to_string()], 20).await.unwrap();
        assert_eq!(found.len(), 1);
        assert!(search_passages(&pool, false, &[], 20).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_changed_meetings_are_reindexed_and_orphans_removed() {
        let pool = test_pool().await;
        let fts = ensure_schema(&pool).await.unwrap();
        add_meeting(&pool, "m1", &[("primeiro texto sobre preço", 1.0)], None).await;
        add_meeting(&pool, "m2", &[("segundo texto sobre prazo", 1.0)], None).await;
        sync_index(&pool, fts, false).await.unwrap();

        let again = sync_index(&pool, fts, false).await.unwrap();
        assert_eq!(again.changed_meetings, 0);

        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time) VALUES ('n', 'm1', 'novo trecho sobre garantia', 'x', 9)")
            .execute(&pool)
            .await
            .unwrap();
        let after_edit = sync_index(&pool, fts, false).await.unwrap();
        assert_eq!(after_edit.changed_meetings, 1);
        assert_eq!(search_passages(&pool, fts, &["garantia".to_string()], 10).await.unwrap().len(), 1);

        sqlx::query("DELETE FROM meetings WHERE id = 'm2'").execute(&pool).await.unwrap();
        let after_delete = sync_index(&pool, fts, false).await.unwrap();
        assert_eq!(after_delete.indexed_meetings, 1);
        assert!(search_passages(&pool, fts, &["segundo".to_string()], 10).await.unwrap().is_empty());
        assert_eq!(indexed_meeting_count(&pool).await, 1);

        let forced = sync_index(&pool, fts, true).await.unwrap();
        assert_eq!(forced.changed_meetings, 1);
    }

    #[tokio::test]
    async fn metadata_passages_prefer_the_summary() {
        let pool = test_pool().await;
        let fts = ensure_schema(&pool).await.unwrap();
        add_meeting(&pool, "m1", &[("conversa solta", 1.0)], Some("- Resumo importante")).await;
        sync_index(&pool, fts, false).await.unwrap();
        let passages = passages_of_meetings(&pool, &["m1".to_string()], 1).await.unwrap();
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].source, "summary");
    }
}
