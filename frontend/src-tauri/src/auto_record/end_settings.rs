use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::end_machine::EndMode;

pub const DEFAULT_SILENCE_MINUTES: u32 = 3;
pub const MIN_SILENCE_MINUTES: u32 = 1;
pub const MAX_SILENCE_MINUTES: u32 = 60;
pub const DEFAULT_PROMPT_SECONDS: u32 = 60;
pub const MIN_PROMPT_SECONDS: u32 = 15;
pub const MAX_PROMPT_SECONDS: u32 = 600;

impl EndMode {
    pub fn as_str(self) -> &'static str {
        match self {
            EndMode::Off => "off",
            EndMode::Ask => "ask",
            EndMode::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(EndMode::Off),
            "ask" => Some(EndMode::Ask),
            "auto" => Some(EndMode::Auto),
            _ => None,
        }
    }
}

/// What to do when the meeting seems to be over.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingEndSettings {
    /// "off", "ask" or "auto".
    pub mode: String,
    pub silence_minutes: u32,
    pub prompt_seconds: u32,
}

impl MeetingEndSettings {
    pub fn new(mode: EndMode, silence_minutes: u32, prompt_seconds: u32) -> Self {
        Self {
            mode: mode.as_str().to_string(),
            silence_minutes: silence_minutes.clamp(MIN_SILENCE_MINUTES, MAX_SILENCE_MINUTES),
            prompt_seconds: prompt_seconds.clamp(MIN_PROMPT_SECONDS, MAX_PROMPT_SECONDS),
        }
    }

    pub fn mode(&self) -> EndMode {
        EndMode::parse(&self.mode).unwrap_or(EndMode::Ask)
    }
}

impl Default for MeetingEndSettings {
    fn default() -> Self {
        Self::new(EndMode::Ask, DEFAULT_SILENCE_MINUTES, DEFAULT_PROMPT_SECONDS)
    }
}

pub async fn load(pool: &SqlitePool) -> Result<MeetingEndSettings, sqlx::Error> {
    let row = sqlx::query("SELECT mode, silence_minutes, prompt_seconds FROM meeting_end_settings WHERE id = '1'")
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(MeetingEndSettings::default());
    };
    let mode: String = row.try_get("mode").unwrap_or_default();
    let silence: i64 = row.try_get("silence_minutes").unwrap_or(DEFAULT_SILENCE_MINUTES as i64);
    let prompt: i64 = row.try_get("prompt_seconds").unwrap_or(DEFAULT_PROMPT_SECONDS as i64);
    Ok(MeetingEndSettings::new(
        EndMode::parse(&mode).unwrap_or(EndMode::Ask),
        u32::try_from(silence).unwrap_or(DEFAULT_SILENCE_MINUTES),
        u32::try_from(prompt).unwrap_or(DEFAULT_PROMPT_SECONDS),
    ))
}

pub async fn save(pool: &SqlitePool, settings: &MeetingEndSettings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO meeting_end_settings (id, mode, silence_minutes, prompt_seconds) VALUES ('1', ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET mode = excluded.mode,
             silence_minutes = excluded.silence_minutes, prompt_seconds = excluded.prompt_seconds",
    )
    .bind(&settings.mode)
    .bind(settings.silence_minutes as i64)
    .bind(settings.prompt_seconds as i64)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_clamped_and_serialized_in_camel_case() {
        let settings = MeetingEndSettings::new(EndMode::Auto, 0, 5);
        assert_eq!(settings.silence_minutes, MIN_SILENCE_MINUTES);
        assert_eq!(settings.prompt_seconds, MIN_PROMPT_SECONDS);
        let settings = MeetingEndSettings::new(EndMode::Auto, 999, 9_999);
        assert_eq!(settings.silence_minutes, MAX_SILENCE_MINUTES);
        assert_eq!(settings.prompt_seconds, MAX_PROMPT_SECONDS);
        let json = serde_json::to_value(MeetingEndSettings::default()).unwrap();
        assert_eq!(json["mode"], "ask");
        assert_eq!(json["silenceMinutes"], 3);
        assert_eq!(json["promptSeconds"], 60);
        assert_eq!(EndMode::parse(" Auto "), Some(EndMode::Auto));
        assert_eq!(EndMode::parse("sempre"), None);
    }

    #[tokio::test]
    async fn the_migration_defaults_to_ask_and_settings_round_trip() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        assert_eq!(load(&pool).await.unwrap(), MeetingEndSettings::default());

        for mode in [EndMode::Auto, EndMode::Off, EndMode::Ask] {
            let settings = MeetingEndSettings::new(mode, 7, 90);
            save(&pool, &settings).await.unwrap();
            assert_eq!(load(&pool).await.unwrap(), settings);
        }

        // A value written by hand outside the limits is read back inside them.
        sqlx::query("UPDATE meeting_end_settings SET mode = 'x', silence_minutes = -4, prompt_seconds = 0")
            .execute(&pool)
            .await
            .unwrap();
        let loaded = load(&pool).await.unwrap();
        assert_eq!(loaded.mode, "ask");
        assert_eq!(loaded.silence_minutes, DEFAULT_SILENCE_MINUTES);
        assert_eq!(loaded.prompt_seconds, MIN_PROMPT_SECONDS);
    }
}
