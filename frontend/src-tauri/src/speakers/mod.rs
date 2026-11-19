//! Local speaker labels for sequential turns.
//!
//! The embedding is a normalized log-magnitude spectrum (`speakers::embed`),
//! clustered per audio channel. It does not download a model and it cannot
//! separate two people talking at the same time in one mix.

mod alias;
mod book;
mod embed;

pub use alias::{clear_aliases, rename};
pub use book::{Assignment, Channel, SpeakerBook};

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
