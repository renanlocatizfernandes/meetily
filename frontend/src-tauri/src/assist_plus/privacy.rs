//! One-time privacy choice: how automatic recording behaves and whether the
//! live assist may look terms up on the internet.
//!
//! Until the choice is made (`feature_toggles.privacy_onboarding_done = 0`),
//! automatic recording behaves as "off" and the internet lookup as disabled,
//! whatever the stored values say. Every existing install is asked once.

use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager};

use crate::auto_record::settings::{self as auto_settings, Mode};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyOnboarding {
    pub done: bool,
    /// "off", "ask" or "auto".
    pub auto_record_mode: String,
    pub internet_lookup: bool,
}

pub async fn is_done(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT privacy_onboarding_done FROM feature_toggles WHERE id = '1'")
        .fetch_optional(pool)
        .await?;
    Ok(row
        .and_then(|row| row.try_get::<i64, _>("privacy_onboarding_done").ok())
        .is_some_and(|value| value != 0))
}

/// After the choice: the stored values. Before it: the suggestion to show in
/// the form, which is the stored recording mode ("ask" unless it was turned
/// off) and the internet lookup off.
pub async fn read(pool: &SqlitePool) -> Result<PrivacyOnboarding, sqlx::Error> {
    let done = is_done(pool).await?;
    let mode = auto_settings::load_settings(pool).await?.mode();
    let internet_lookup = done && super::toggles::load(pool).await?.internet_search;
    Ok(PrivacyOnboarding {
        done,
        auto_record_mode: mode.as_str().to_string(),
        internet_lookup,
    })
}

/// Stores both choices and marks the onboarding as done, in one transaction.
pub async fn complete(pool: &SqlitePool, mode: Mode, internet_lookup: bool) -> Result<PrivacyOnboarding, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    auto_settings::save_mode(&mut *transaction, mode).await?;
    sqlx::query(
        "INSERT INTO feature_toggles (id, internet_search, privacy_onboarding_done) VALUES ('1', ?, 1)
         ON CONFLICT(id) DO UPDATE SET
            internet_search = excluded.internet_search,
            privacy_onboarding_done = 1",
    )
    .bind(if internet_lookup { 1 } else { 0 })
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    read(pool).await
}

fn pool(app: &AppHandle) -> Result<SqlitePool, String> {
    app.try_state::<AppState>()
        .map(|state| state.db_manager.pool().clone())
        .ok_or_else(|| "Banco de dados indisponível".to_string())
}

#[tauri::command]
pub async fn get_privacy_onboarding(app: AppHandle) -> Result<PrivacyOnboarding, String> {
    read(&pool(&app)?)
        .await
        .map_err(|_| "Não foi possível ler a escolha de privacidade.".to_string())
}

#[tauri::command]
pub async fn complete_privacy_onboarding(
    app: AppHandle,
    auto_record_mode: String,
    internet_lookup: bool,
) -> Result<PrivacyOnboarding, String> {
    let mode = Mode::parse(&auto_record_mode)
        .ok_or_else(|| "Modo de gravação automática inválido. Use off, ask ou auto.".to_string())?;
    complete(&pool(&app)?, mode, internet_lookup)
        .await
        .map_err(|_| "Não foi possível salvar a escolha de privacidade.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assist_plus::toggles;

    async fn migrated_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn an_existing_install_is_asked_once_and_stays_private_until_then() {
        let pool = migrated_pool().await;
        // The earlier migrations left internet_search = 1 and recording enabled.
        assert!(toggles::load(&pool).await.unwrap().internet_search);
        assert!(!is_done(&pool).await.unwrap());
        assert!(!toggles::load_effective(&pool).await.unwrap().internet_search);
        assert_eq!(
            read(&pool).await.unwrap(),
            PrivacyOnboarding {
                done: false,
                auto_record_mode: "ask".to_string(),
                internet_lookup: false,
            }
        );
    }

    #[tokio::test]
    async fn completing_stores_both_choices_and_keeps_the_other_toggles() {
        let pool = migrated_pool().await;
        let mut stored = toggles::load(&pool).await.unwrap();
        stored.scoreboard = false;
        toggles::save(&pool, &stored).await.unwrap();

        let result = complete(&pool, Mode::Auto, false).await.unwrap();
        assert_eq!(
            result,
            PrivacyOnboarding {
                done: true,
                auto_record_mode: "auto".to_string(),
                internet_lookup: false,
            }
        );
        let effective = toggles::load_effective(&pool).await.unwrap();
        assert!(!effective.internet_search);
        assert!(!effective.scoreboard);
        assert!(effective.auto_tasks);

        let result = complete(&pool, Mode::Off, true).await.unwrap();
        assert!(result.done && result.internet_lookup);
        assert_eq!(result.auto_record_mode, "off");
        assert!(toggles::load_effective(&pool).await.unwrap().internet_search);
        // Saving the toggles later does not reopen the onboarding.
        toggles::save(&pool, &toggles::load(&pool).await.unwrap()).await.unwrap();
        assert!(is_done(&pool).await.unwrap());
    }
}
