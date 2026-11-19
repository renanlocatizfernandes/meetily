//! Local speaker labels for sequential turns.
//!
//! The embedding is a normalized log-magnitude spectrum (`speakers::embed`),
//! clustered per audio channel. It does not download a model and it cannot
//! separate two people talking at the same time in one mix.

mod alias;
mod book;
mod embed;
mod profiles;

pub use alias::{clear_aliases, rename};
pub use book::{Assignment, Channel, SpeakerBook};
pub use profiles::{embedding_for, remember_embedding, set_profiles, suggest_name, VoiceProfile};

use sqlx::Row;
use tauri::Manager;

#[tauri::command]
pub async fn rename_speaker(
    app: tauri::AppHandle<tauri::Wry>,
    meeting_id: Option<String>,
    speaker_key: String,
    display_name: String,
) -> Result<(), String> {
    let display_name = display_name.trim().chars().take(40).collect::<String>();
    if speaker_key.trim().is_empty() || display_name.is_empty() {
        return Err("Informe um nome para esta pessoa.".to_string());
    }
    rename(speaker_key.clone(), display_name.clone());
    if let Some(embedding) = embedding_for(&speaker_key) {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            let _ = save_voice_profile(state.db_manager.pool(), &display_name, &embedding).await;
        }
    }
    let Some(meeting_id) = meeting_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return Ok(());
    };
    let pool = state.db_manager.pool();
    sqlx::query(
        "INSERT INTO speaker_aliases (meeting_id, speaker_key, display_name) VALUES (?, ?, ?)
         ON CONFLICT(meeting_id, speaker_key) DO UPDATE SET display_name = excluded.display_name",
    )
    .bind(&meeting_id)
    .bind(&speaker_key)
    .bind(&display_name)
    .execute(pool)
    .await
    .map_err(|_| "Não foi possível salvar o nome.".to_string())?;
    sqlx::query("UPDATE transcripts SET speaker = ? WHERE meeting_id = ? AND speaker_key = ?")
        .bind(&display_name)
        .bind(&meeting_id)
        .bind(&speaker_key)
        .execute(pool)
        .await
        .map_err(|_| "Não foi possível atualizar a transcrição.".to_string())?;
    Ok(())
}

async fn save_voice_profile(pool: &sqlx::SqlitePool, name: &str, embedding: &[f32]) -> Result<(), sqlx::Error> {
    let enabled = sqlx::query("SELECT voice_profiles FROM feature_toggles WHERE id = '1'")
        .fetch_optional(pool)
        .await?
        .and_then(|row| row.try_get::<i64, _>("voice_profiles").ok())
        .unwrap_or(1);
    if enabled == 0 {
        return Ok(());
    }
    let payload = serde_json::to_string(embedding).unwrap_or_else(|_| "[]".to_string());
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO voice_profiles (name, embedding, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET embedding = excluded.embedding, updated_at = excluded.updated_at",
    )
    .bind(name)
    .bind(payload)
    .bind(now)
    .execute(pool)
    .await?;
    let mut profiles = Vec::new();
    let rows = sqlx::query("SELECT name, embedding, updated_at FROM voice_profiles")
        .fetch_all(pool)
        .await?;
    for row in rows {
        let name: String = row.try_get("name")?;
        let embedding: String = row.try_get("embedding")?;
        let updated_at: String = row.try_get("updated_at")?;
        let parsed: Vec<f32> = serde_json::from_str(&embedding).unwrap_or_default();
        if parsed.is_empty() {
            continue;
        }
        let updated_ms = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| time.timestamp_millis())
            .unwrap_or(0);
        profiles.push(VoiceProfile { name, embedding: parsed, updated_ms });
    }
    set_profiles(profiles);
    Ok(())
}

pub async fn load_voice_profiles(pool: &sqlx::SqlitePool) {
    let enabled = sqlx::query("SELECT voice_profiles FROM feature_toggles WHERE id = '1'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<i64, _>("voice_profiles").ok())
        .unwrap_or(1);
    if enabled == 0 {
        set_profiles(Vec::new());
        return;
    }
    let Ok(rows) = sqlx::query("SELECT name, embedding, updated_at FROM voice_profiles").fetch_all(pool).await else {
        return;
    };
    let mut profiles = Vec::new();
    for row in rows {
        let Ok(name) = row.try_get::<String, _>("name") else { continue };
        let Ok(embedding) = row.try_get::<String, _>("embedding") else { continue };
        let updated_at: String = row.try_get("updated_at").unwrap_or_default();
        let parsed: Vec<f32> = serde_json::from_str(&embedding).unwrap_or_default();
        if parsed.is_empty() {
            continue;
        }
        let updated_ms = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| time.timestamp_millis())
            .unwrap_or(0);
        profiles.push(VoiceProfile { name, embedding: parsed, updated_ms });
    }
    set_profiles(profiles);
}
