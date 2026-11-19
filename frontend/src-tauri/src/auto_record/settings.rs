use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

/// What happens when a meeting is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Never record by itself.
    Off,
    /// Ask before recording.
    Ask,
    /// Record without asking.
    Auto,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Ask => "ask",
            Mode::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Mode::Off),
            "ask" => Some(Mode::Ask),
            "auto" => Some(Mode::Auto),
            _ => None,
        }
    }
}

/// `enabled` is the older on/off switch and stays in the contract:
/// `enabled == false` is the same as `mode == "off"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoRecordSettings {
    pub enabled: bool,
    /// "off", "ask" or "auto".
    pub mode: String,
}

impl AutoRecordSettings {
    pub fn from_mode(mode: Mode) -> Self {
        Self {
            enabled: mode != Mode::Off,
            mode: mode.as_str().to_string(),
        }
    }

    pub fn mode(&self) -> Mode {
        Mode::parse(&self.mode).unwrap_or(if self.enabled { Mode::Ask } else { Mode::Off })
    }
}

impl Default for AutoRecordSettings {
    fn default() -> Self {
        Self::from_mode(Mode::Ask)
    }
}

/// The mode a save through the older `enabled` switch leads to: turning it
/// off is "off"; turning it on keeps the current mode, or "ask" if it was off.
pub fn mode_after_toggle(current: Mode, enabled: bool) -> Mode {
    match (enabled, current) {
        (false, _) => Mode::Off,
        (true, Mode::Off) => Mode::Ask,
        (true, mode) => mode,
    }
}

pub async fn load_settings(pool: &SqlitePool) -> Result<AutoRecordSettings, sqlx::Error> {
    let row = sqlx::query("SELECT enabled, mode FROM auto_record_settings WHERE id = '1'")
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(AutoRecordSettings::default());
    };
    let enabled: i64 = row.try_get("enabled").unwrap_or(1);
    let stored: String = row.try_get("mode").unwrap_or_default();
    let mode = match Mode::parse(&stored) {
        // The two columns are written together; `enabled = 0` wins if they ever disagree.
        Some(_) if enabled == 0 => Mode::Off,
        Some(mode) => mode,
        None if enabled != 0 => Mode::Ask,
        None => Mode::Off,
    };
    Ok(AutoRecordSettings::from_mode(mode))
}

pub async fn save_mode<'e, E>(executor: E, mode: Mode) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO auto_record_settings (id, enabled, mode) VALUES ('1', ?, ?)
         ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled, mode = excluded.mode",
    )
    .bind(if mode == Mode::Off { 0 } else { 1 })
    .bind(mode.as_str())
    .execute(executor)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_switch_maps_to_modes() {
        assert_eq!(mode_after_toggle(Mode::Auto, false), Mode::Off);
        assert_eq!(mode_after_toggle(Mode::Ask, false), Mode::Off);
        assert_eq!(mode_after_toggle(Mode::Off, true), Mode::Ask);
        assert_eq!(mode_after_toggle(Mode::Ask, true), Mode::Ask);
        assert_eq!(mode_after_toggle(Mode::Auto, true), Mode::Auto);
    }

    #[test]
    fn settings_keep_enabled_and_mode_consistent() {
        assert_eq!(AutoRecordSettings::default().mode, "ask");
        assert!(AutoRecordSettings::default().enabled);
        assert!(!AutoRecordSettings::from_mode(Mode::Off).enabled);
        assert!(AutoRecordSettings::from_mode(Mode::Auto).enabled);
        assert_eq!(Mode::parse(" AUTO "), Some(Mode::Auto));
        assert_eq!(Mode::parse("sempre"), None);
        let json = serde_json::to_value(AutoRecordSettings::from_mode(Mode::Auto)).unwrap();
        assert_eq!(json["enabled"], true);
        assert_eq!(json["mode"], "auto");
    }

    #[tokio::test]
    async fn migrations_move_existing_rows_to_ask_and_modes_round_trip() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        // The row created by the earlier migration (enabled = 1) is now "ask".
        let loaded = load_settings(&pool).await.unwrap();
        assert_eq!(loaded, AutoRecordSettings::from_mode(Mode::Ask));

        for mode in [Mode::Auto, Mode::Off, Mode::Ask] {
            save_mode(&pool, mode).await.unwrap();
            assert_eq!(load_settings(&pool).await.unwrap(), AutoRecordSettings::from_mode(mode));
        }

        // A row written by an older build, which only knows `enabled`.
        sqlx::query("UPDATE auto_record_settings SET enabled = 0 WHERE id = '1'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(load_settings(&pool).await.unwrap().mode, "off");
    }
}
