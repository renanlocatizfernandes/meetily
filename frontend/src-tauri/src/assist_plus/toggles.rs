use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureToggles {
    pub internet_search: bool,
    pub auto_tasks: bool,
    pub meeting_memory: bool,
    pub calendar_prep: bool,
    pub voice_profiles: bool,
    pub commitment_alerts: bool,
    pub followup_email: bool,
    pub meeting_search: bool,
    pub scoreboard: bool,
    /// "auto" liga a tradução só em inglês ou espanhol. "on" e "off" são manuais.
    pub translation: String,
}

impl Default for FeatureToggles {
    fn default() -> Self {
        Self {
            internet_search: true,
            auto_tasks: true,
            meeting_memory: true,
            calendar_prep: true,
            voice_profiles: true,
            commitment_alerts: true,
            followup_email: true,
            meeting_search: true,
            scoreboard: true,
            translation: "auto".to_string(),
        }
    }
}

impl FeatureToggles {
    pub fn normalized(mut self) -> Self {
        self.translation = match self.translation.trim() {
            "on" => "on".to_string(),
            "off" => "off".to_string(),
            _ => "auto".to_string(),
        };
        self
    }
}

pub async fn load(pool: &SqlitePool) -> Result<FeatureToggles, sqlx::Error> {
    let row = sqlx::query(
        "SELECT internet_search, auto_tasks, meeting_memory, calendar_prep, voice_profiles, commitment_alerts, followup_email, meeting_search, scoreboard, translation FROM feature_toggles WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(FeatureToggles::default());
    };
    Ok(FeatureToggles {
        internet_search: flag(&row, "internet_search"),
        auto_tasks: flag(&row, "auto_tasks"),
        meeting_memory: flag(&row, "meeting_memory"),
        calendar_prep: flag(&row, "calendar_prep"),
        voice_profiles: flag(&row, "voice_profiles"),
        commitment_alerts: flag(&row, "commitment_alerts"),
        followup_email: flag(&row, "followup_email"),
        meeting_search: flag(&row, "meeting_search"),
        scoreboard: flag(&row, "scoreboard"),
        translation: row.try_get("translation").unwrap_or_else(|_| "auto".to_string()),
    }
    .normalized())
}

fn flag(row: &sqlx::sqlite::SqliteRow, name: &str) -> bool {
    row.try_get::<i64, _>(name).unwrap_or(1) != 0
}

pub async fn save(pool: &SqlitePool, toggles: &FeatureToggles) -> Result<(), sqlx::Error> {
    let toggles = toggles.clone().normalized();
    sqlx::query(
        "INSERT INTO feature_toggles (
            id, internet_search, auto_tasks, meeting_memory, calendar_prep, voice_profiles,
            commitment_alerts, followup_email, meeting_search, scoreboard, translation
         ) VALUES ('1', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            internet_search = excluded.internet_search,
            auto_tasks = excluded.auto_tasks,
            meeting_memory = excluded.meeting_memory,
            calendar_prep = excluded.calendar_prep,
            voice_profiles = excluded.voice_profiles,
            commitment_alerts = excluded.commitment_alerts,
            followup_email = excluded.followup_email,
            meeting_search = excluded.meeting_search,
            scoreboard = excluded.scoreboard,
            translation = excluded.translation",
    )
    .bind(bit(toggles.internet_search))
    .bind(bit(toggles.auto_tasks))
    .bind(bit(toggles.meeting_memory))
    .bind(bit(toggles.calendar_prep))
    .bind(bit(toggles.voice_profiles))
    .bind(bit(toggles.commitment_alerts))
    .bind(bit(toggles.followup_email))
    .bind(bit(toggles.meeting_search))
    .bind(bit(toggles.scoreboard))
    .bind(&toggles.translation)
    .execute(pool)
    .await?;
    Ok(())
}

fn bit(value: bool) -> i64 {
    if value { 1 } else { 0 }
}
