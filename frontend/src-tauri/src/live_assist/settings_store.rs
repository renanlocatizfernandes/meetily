use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::debounce::normalize_frequency;
use super::target::DEFAULT_MODEL;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveAssistSettings {
    pub enabled: bool,
    pub model: String,
    pub language: String,
    pub frequency: String,
    pub meeting_context: String,
}

impl Default for LiveAssistSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            model: DEFAULT_MODEL.to_string(),
            language: "pt-BR".to_string(),
            frequency: "media".to_string(),
            meeting_context: String::new(),
        }
    }
}

impl LiveAssistSettings {
    pub fn normalized(mut self) -> Self {
        self.model = {
            let trimmed = self.model.trim();
            if trimmed.is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                trimmed.chars().take(120).collect()
            }
        };
        self.language = {
            let trimmed = self.language.trim();
            if trimmed.is_empty() {
                "pt-BR".to_string()
            } else {
                trimmed.chars().take(32).collect()
            }
        };
        self.frequency = normalize_frequency(&self.frequency).to_string();
        self.meeting_context = self.meeting_context.trim().chars().take(2000).collect();
        self
    }
}

pub async fn load_settings(pool: &SqlitePool) -> Result<LiveAssistSettings, sqlx::Error> {
    let row = sqlx::query(
        "SELECT enabled, model, language, frequency, meeting_context FROM live_assist_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(LiveAssistSettings::default());
    };

    let enabled: i64 = row.try_get("enabled").unwrap_or(0);
    let model: String = row.try_get("model").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let language: String = row.try_get("language").unwrap_or_else(|_| "pt-BR".to_string());
    let frequency: String = row.try_get("frequency").unwrap_or_else(|_| "media".to_string());
    let meeting_context: String = row.try_get("meeting_context").unwrap_or_default();

    Ok(LiveAssistSettings {
        enabled: enabled != 0,
        model,
        language,
        frequency,
        meeting_context,
    }
    .normalized())
}

pub async fn save_settings(pool: &SqlitePool, settings: &LiveAssistSettings) -> Result<(), sqlx::Error> {
    let settings = settings.clone().normalized();
    sqlx::query(
        "INSERT INTO live_assist_settings (id, enabled, model, language, frequency, meeting_context)
         VALUES ('1', ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            enabled = excluded.enabled,
            model = excluded.model,
            language = excluded.language,
            frequency = excluded.frequency,
            meeting_context = excluded.meeting_context",
    )
    .bind(if settings.enabled { 1 } else { 0 })
    .bind(&settings.model)
    .bind(&settings.language)
    .bind(&settings.frequency)
    .bind(&settings.meeting_context)
    .execute(pool)
    .await?;
    Ok(())
}
