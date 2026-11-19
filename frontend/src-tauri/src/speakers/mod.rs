//! Local speaker labels for sequential turns.
//!
//! Each turn gets a normalized log-magnitude spectrum (`speakers::embed`) and,
//! when the optional model is installed and switched on, a neural speaker
//! vector (`speakers::neural`). `speakers::book` clusters turns per audio
//! channel in one kind at a time. Neither can separate two people talking at
//! the same time in one mix.

mod alias;
mod book;
mod embed;
mod fbank;
pub mod neural;
mod profiles;

pub use alias::{clear_aliases, rename};
pub use book::{Assignment, Channel, SpeakerBook};
pub use embed::{Embedding, EmbeddingKind};
pub use neural::{embed as neural_embedding, prepare as prepare_voice_model, VoiceModelStatus};
pub use profiles::{embeddings_for, remember_embeddings, set_profiles, suggest_name, VoiceProfile};

use sqlx::Row;
use tauri::Manager;

/// Keeps the person's current voice vectors for a later rename and, while the
/// label is still the default "Pessoa N", gives it the name of a saved voice.
pub fn apply_profiles(assignment: &mut Assignment) {
    if assignment.embeddings.is_empty() {
        return;
    }
    remember_embeddings(&assignment.key, assignment.embeddings.clone());
    if assignment.label.starts_with("Pessoa") {
        if let Some(name) = suggest_name(&assignment.embeddings) {
            rename(assignment.key.clone(), name.clone());
            assignment.label = name;
        }
    }
}

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
    let embeddings = embeddings_for(&speaker_key);
    if !embeddings.is_empty() {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            let _ = save_voice_profile(state.db_manager.pool(), &display_name, &embeddings).await;
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

/// Saves the vectors under `name`. A kind that is not in `embeddings` keeps
/// what the row already had, so renaming with the model off does not erase
/// the neural vector saved earlier (and the other way round).
async fn save_voice_profile(pool: &sqlx::SqlitePool, name: &str, embeddings: &[Embedding]) -> Result<(), sqlx::Error> {
    if !voice_profiles_enabled(pool).await {
        return Ok(());
    }
    let existing = sqlx::query("SELECT embedding FROM voice_profiles WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?
        .and_then(|row| row.try_get::<String, _>("embedding").ok())
        .map(|text| embed::parse_stored(&text))
        .unwrap_or_default();
    let Some(payload) = embed::to_stored(&merge_kinds(existing, embeddings)) else {
        return Ok(());
    };
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
    set_profiles(read_voice_profiles(pool).await?);
    Ok(())
}

fn merge_kinds(mut saved: Vec<Embedding>, fresh: &[Embedding]) -> Vec<Embedding> {
    for embedding in fresh {
        saved.retain(|existing| existing.kind != embedding.kind);
        saved.push(embedding.clone());
    }
    saved
}

async fn voice_profiles_enabled(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query("SELECT voice_profiles FROM feature_toggles WHERE id = '1'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<i64, _>("voice_profiles").ok())
        .unwrap_or(1)
        != 0
}

async fn read_voice_profiles(pool: &sqlx::SqlitePool) -> Result<Vec<VoiceProfile>, sqlx::Error> {
    let rows = sqlx::query("SELECT name, embedding, updated_at FROM voice_profiles")
        .fetch_all(pool)
        .await?;
    let mut profiles = Vec::new();
    for row in rows {
        let Ok(name) = row.try_get::<String, _>("name") else { continue };
        let Ok(embedding) = row.try_get::<String, _>("embedding") else { continue };
        let updated_at: String = row.try_get("updated_at").unwrap_or_default();
        let embeddings = embed::parse_stored(&embedding);
        if embeddings.is_empty() {
            continue;
        }
        let updated_ms = chrono::DateTime::parse_from_rfc3339(&updated_at)
            .map(|time| time.timestamp_millis())
            .unwrap_or(0);
        profiles.push(VoiceProfile { name, embeddings, updated_ms });
    }
    Ok(profiles)
}

pub async fn load_voice_profiles(pool: &sqlx::SqlitePool) {
    if !voice_profiles_enabled(pool).await {
        set_profiles(Vec::new());
        return;
    }
    if let Ok(profiles) = read_voice_profiles(pool).await {
        set_profiles(profiles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_one_kind_keeps_the_other_kind_already_saved_for_the_name() {
        let old_neural = Embedding::neural(vec![1.0, 0.0]).unwrap();
        let old_spectrum = Embedding::spectrum(vec![0.0, 1.0]);
        let new_spectrum = Embedding::spectrum(vec![0.6, 0.8]);
        let merged = merge_kinds(vec![old_neural.clone(), old_spectrum], &[new_spectrum.clone()]);
        assert_eq!(merged, vec![old_neural.clone(), new_spectrum.clone()]);
        let text = embed::to_stored(&merged).unwrap();
        let read = embed::parse_stored(&text);
        assert!(read.contains(&old_neural) && read.contains(&new_spectrum));
    }

    #[tokio::test]
    async fn profiles_survive_the_database_in_both_formats() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE voice_profiles (name TEXT PRIMARY KEY NOT NULL, embedding TEXT NOT NULL, updated_at TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        // A row written by the build without the model.
        sqlx::query("INSERT INTO voice_profiles VALUES ('Ana', '[0.6,0.8]', '2026-11-01T10:00:00+00:00')")
            .execute(&pool)
            .await
            .unwrap();
        let neural = Embedding::neural(vec![0.0, 3.0, 4.0]).unwrap();
        save_voice_profile(&pool, "Ana", &[neural.clone()]).await.unwrap();
        save_voice_profile(&pool, "Bia", &[Embedding::spectrum(vec![1.0, 0.0])]).await.unwrap();

        let profiles = read_voice_profiles(&pool).await.unwrap();
        let ana = profiles.iter().find(|profile| profile.name == "Ana").unwrap();
        assert!(ana.embeddings.contains(&neural));
        assert!(ana.embeddings.contains(&Embedding::spectrum(vec![0.6, 0.8])));
        let bia: String = sqlx::query("SELECT embedding FROM voice_profiles WHERE name = 'Bia'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("embedding");
        assert_eq!(bia, "[1.0,0.0]");
        set_profiles(Vec::new());
    }
}
