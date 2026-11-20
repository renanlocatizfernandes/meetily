use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SmartSettings {
    /// A question the other side asked the owner. Local text matching.
    pub question_alerts: bool,
    pub decision_alerts: bool,
    pub task_alerts: bool,
    /// "Possível divergência" with notes of earlier meetings. Local.
    pub contradiction_alerts: bool,
    /// Writes an answer to a detected question by itself. Calls the model, so
    /// it starts off; with a paid provider it spends tokens.
    pub auto_answer: bool,
    /// "auto" turns the economy mode on only for a cloud model; "on" and "off" are manual.
    pub economy: String,
}

impl Default for SmartSettings {
    fn default() -> Self {
        Self {
            question_alerts: true,
            decision_alerts: true,
            task_alerts: true,
            contradiction_alerts: true,
            auto_answer: false,
            economy: "auto".to_string(),
        }
    }
}

impl SmartSettings {
    pub fn normalized(mut self) -> Self {
        self.economy = match self.economy.trim() {
            "on" => "on".to_string(),
            "off" => "off".to_string(),
            _ => "auto".to_string(),
        };
        self
    }
}

fn flag(row: &sqlx::sqlite::SqliteRow, name: &str, default: i64) -> bool {
    row.try_get::<i64, _>(name).unwrap_or(default) != 0
}

pub async fn load_smart_settings(pool: &SqlitePool) -> Result<SmartSettings, sqlx::Error> {
    let row = sqlx::query(
        "SELECT question_alerts, decision_alerts, task_alerts, contradiction_alerts, auto_answer, economy FROM live_assist_smart_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(SmartSettings::default());
    };
    Ok(SmartSettings {
        question_alerts: flag(&row, "question_alerts", 1),
        decision_alerts: flag(&row, "decision_alerts", 1),
        task_alerts: flag(&row, "task_alerts", 1),
        contradiction_alerts: flag(&row, "contradiction_alerts", 1),
        auto_answer: flag(&row, "auto_answer", 0),
        economy: row.try_get("economy").unwrap_or_else(|_| "auto".to_string()),
    }
    .normalized())
}

pub async fn save_smart_settings(pool: &SqlitePool, settings: &SmartSettings) -> Result<(), sqlx::Error> {
    let settings = settings.clone().normalized();
    sqlx::query(
        "INSERT INTO live_assist_smart_settings (id, question_alerts, decision_alerts, task_alerts, contradiction_alerts, auto_answer, economy)
         VALUES ('1', ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            question_alerts = excluded.question_alerts,
            decision_alerts = excluded.decision_alerts,
            task_alerts = excluded.task_alerts,
            contradiction_alerts = excluded.contradiction_alerts,
            auto_answer = excluded.auto_answer,
            economy = excluded.economy",
    )
    .bind(i64::from(settings.question_alerts))
    .bind(i64::from(settings.decision_alerts))
    .bind(i64::from(settings.task_alerts))
    .bind(i64::from(settings.contradiction_alerts))
    .bind(i64::from(settings.auto_answer))
    .bind(&settings.economy)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_model_switch_off_and_normalize_economy() {
        let defaults = SmartSettings::default();
        assert!(!defaults.auto_answer);
        assert!(defaults.question_alerts && defaults.contradiction_alerts);
        let odd = SmartSettings { economy: " whatever ".into(), ..SmartSettings::default() }.normalized();
        assert_eq!(odd.economy, "auto");
        let on = SmartSettings { economy: "on".into(), ..SmartSettings::default() }.normalized();
        assert_eq!(on.economy, "on");
    }

    #[tokio::test]
    async fn round_trips_through_the_database() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        assert_eq!(load_smart_settings(&pool).await.unwrap(), SmartSettings::default());
        let changed = SmartSettings { auto_answer: true, task_alerts: false, economy: "on".into(), ..SmartSettings::default() };
        save_smart_settings(&pool, &changed).await.unwrap();
        assert_eq!(load_smart_settings(&pool).await.unwrap(), changed);
    }
}
