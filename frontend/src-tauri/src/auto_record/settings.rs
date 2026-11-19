use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoRecordSettings {
    pub enabled: bool,
}

impl Default for AutoRecordSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub async fn load_settings(pool: &SqlitePool) -> Result<AutoRecordSettings, sqlx::Error> {
    let row = sqlx::query("SELECT enabled FROM auto_record_settings WHERE id = '1'")
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(AutoRecordSettings::default());
    };
    let enabled: i64 = row.try_get("enabled").unwrap_or(1);
    Ok(AutoRecordSettings { enabled: enabled != 0 })
}

pub async fn save_settings(pool: &SqlitePool, settings: &AutoRecordSettings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO auto_record_settings (id, enabled) VALUES ('1', ?)
         ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled",
    )
    .bind(if settings.enabled { 1 } else { 0 })
    .execute(pool)
    .await?;
    Ok(())
}
