use tauri::{AppHandle, Runtime};

use super::model::{AskResponse, AskStatus, HistoryTurn};
use super::service::{self, AskRequest};

/// JS: `invoke('ask_meetings', { question, useLlm, history, meetingId })`.
/// Searches the saved meetings locally; when `useLlm` is true and the summary
/// provider is usable, it also words an answer from the passages found.
#[tauri::command]
pub async fn ask_meetings<R: Runtime>(
    app: AppHandle<R>,
    question: String,
    use_llm: bool,
    history: Option<Vec<HistoryTurn>>,
    meeting_id: Option<String>,
) -> Result<AskResponse, String> {
    let pool = service::pool_from(&app)?;
    service::ask(
        &app,
        &pool,
        AskRequest {
            question,
            use_llm,
            history: history.unwrap_or_default(),
            meeting_id,
        },
    )
    .await
}

/// JS: `invoke('ask_status')`. Which model would word the answers, whether
/// that sends text to a cloud provider, and how much is indexed.
#[tauri::command]
pub async fn ask_status<R: Runtime>(app: AppHandle<R>) -> Result<AskStatus, String> {
    let pool = service::pool_from(&app)?;
    service::status(&pool).await
}

/// JS: `invoke('ask_rebuild_index')`. Rebuilds the search index from the saved
/// meetings; returns how many meetings were indexed.
#[tauri::command]
pub async fn ask_rebuild_index<R: Runtime>(app: AppHandle<R>) -> Result<usize, String> {
    let pool = service::pool_from(&app)?;
    service::rebuild(&pool).await
}
