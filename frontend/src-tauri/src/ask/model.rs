//! Types exchanged with the frontend (camelCase JSON) and between layers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A stored passage (a slice of a transcript or of a summary).
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub meeting_id: String,
    pub source: String,
    pub start_sec: Option<f64>,
    pub text: String,
}

/// What the app already knows about a meeting (title, classification, date).
#[derive(Debug, Clone)]
pub struct MeetingMeta {
    pub id: String,
    pub title: String,
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// A previous turn of the conversation, used only to word the answer.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryTurn {
    pub question: String,
    pub answer: String,
}

/// One citation: where a piece of the answer came from.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    /// Number used in the answer text, as `[n]`.
    pub n: usize,
    pub meeting_id: String,
    pub meeting_title: String,
    pub client: Option<String>,
    /// RFC 3339.
    pub meeting_date: String,
    /// "transcript" or "summary".
    pub source: String,
    pub start_seconds: Option<f64>,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AskResponse {
    pub citations: Vec<Citation>,
    /// Present when the model wrote an answer.
    pub answer: Option<String>,
    /// "generated" | "skipped" (user chose search only) | "unavailable" (model
    /// not configured or failed) | "no_results"
    pub answer_status: String,
    /// Short reason in pt-BR when the answer could not be generated.
    pub answer_note: Option<String>,
    pub leaves_machine: bool,
    pub provider: String,
    pub estimated_tokens: u32,
    /// "fts" or "basic"
    pub search_mode: String,
    pub period_label: Option<String>,
    pub client_filter: Option<String>,
    pub indexed_meetings: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskStatus {
    /// A model is ready to word answers (a local model is assumed available).
    pub llm_configured: bool,
    pub provider: String,
    pub model: String,
    /// The passages found would be sent to a cloud provider.
    pub leaves_machine: bool,
    pub indexed_meetings: usize,
    pub total_meetings: usize,
    pub search_mode: String,
}
