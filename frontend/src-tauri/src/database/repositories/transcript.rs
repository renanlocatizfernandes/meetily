use std::collections::HashMap;

use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, Row, SqlitePool};
use log::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting with id: {}", meeting_id);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker, speaker_key)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(segment.speaker.as_deref())
            .bind(segment.speaker_key.as_deref())
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!("Failed to save transcript segment for meeting {}", meeting_id);
                transaction.rollback().await?;
                return Err(e);
            }
            if let (Some(key), Some(name)) = (segment.speaker_key.as_deref(), segment.speaker.as_deref()) {
                let key = key.trim();
                let name = name.trim();
                if !key.is_empty() && !name.is_empty() {
                    let alias = sqlx::query(
                        "INSERT INTO speaker_aliases (meeting_id, speaker_key, display_name) VALUES (?, ?, ?)
                         ON CONFLICT(meeting_id, speaker_key) DO UPDATE SET display_name = excluded.display_name",
                    )
                    .bind(&meeting_id)
                    .bind(key)
                    .bind(name)
                    .execute(&mut *transaction)
                    .await;
                    if let Err(e) = alias {
                        error!("Failed to save speaker name for meeting {}", meeting_id);
                        transaction.rollback().await?;
                        return Err(e);
                    }
                }
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    pub async fn speaker_aliases(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<HashMap<String, String>, SqlxError> {
        let rows = sqlx::query(
            "SELECT speaker_key, display_name FROM speaker_aliases WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
        let mut aliases = HashMap::new();
        for row in rows {
            let key: String = row.try_get("speaker_key")?;
            let name: String = row.try_get("display_name")?;
            aliases.insert(key, name);
        }
        Ok(aliases)
    }

    pub fn display_speaker(
        aliases: &HashMap<String, String>,
        speaker: Option<String>,
        speaker_key: Option<String>,
    ) -> Option<String> {
        speaker_key
            .as_ref()
            .and_then(|key| aliases.get(key).cloned())
            .or(speaker)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}
