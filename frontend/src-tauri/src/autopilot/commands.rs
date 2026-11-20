use tauri::{AppHandle, Runtime};

use super::model::{AutopilotResult, AutopilotSettings, AutopilotStatus};
use super::{plan, service, store};

fn db_error(error: sqlx::Error) -> String {
    log::error!("Autopilot: database error: {}", error);
    "Não foi possível acessar o banco de dados.".to_string()
}

#[tauri::command]
pub async fn get_autopilot_settings<R: Runtime>(app: AppHandle<R>) -> Result<AutopilotSettings, String> {
    let pool = service::pool_from(&app)?;
    store::load_settings(&pool).await.map_err(db_error)
}

#[tauri::command]
pub async fn save_autopilot_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: AutopilotSettings,
) -> Result<AutopilotSettings, String> {
    let pool = service::pool_from(&app)?;
    let settings = settings.normalized();
    store::save_settings(&pool, &settings).await.map_err(db_error)?;
    Ok(settings)
}

/// Which provider the autopilot would use right now, and whether it costs credits.
#[tauri::command]
pub async fn get_autopilot_status<R: Runtime>(app: AppHandle<R>) -> Result<AutopilotStatus, String> {
    let pool = service::pool_from(&app)?;
    let configured = service::load_configured(&pool).await;
    let local = if plan::has_credentials(&configured) {
        None
    } else {
        service::find_local(&app, &configured).await
    };
    Ok(plan::describe(&configured, local.as_ref()))
}

#[tauri::command]
pub async fn get_autopilot_result<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
) -> Result<Option<AutopilotResult>, String> {
    let pool = service::pool_from(&app)?;
    store::get_result(&pool, &meeting_id).await.map_err(db_error)
}

/// Runs the enabled steps for one saved meeting now (even if the autopilot is
/// off, because the user asked). Returns false when it is already running.
#[tauri::command]
pub async fn run_autopilot_for_meeting<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<bool, String> {
    let pool = service::pool_from(&app)?;
    if !store::meeting_exists(&pool, &meeting_id).await.map_err(db_error)? {
        return Err("Conversa não encontrada.".to_string());
    }
    if service::is_running(&meeting_id) {
        return Ok(false);
    }
    service::spawn(&app, &meeting_id, true);
    Ok(true)
}

/// Applies a stored suggestion on the user's request (`kind`: "title" or "tags").
#[tauri::command]
pub async fn apply_autopilot_suggestion<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    kind: String,
) -> Result<bool, String> {
    let pool = service::pool_from(&app)?;
    let Some(result) = store::get_result(&pool, &meeting_id).await.map_err(db_error)? else {
        return Err("Nada para aplicar nesta conversa.".to_string());
    };
    match kind.as_str() {
        "title" => match result.suggested_title.as_deref() {
            Some(title) => store::apply_title(&pool, &meeting_id, title, true).await.map_err(db_error),
            None => Err("Não há título sugerido.".to_string()),
        },
        "tags" if !result.suggested_tags.is_empty() => {
            store::apply_tags(&pool, &meeting_id, &result.suggested_tags, true).await.map_err(db_error)
        }
        "tags" => Err("Não há etiquetas sugeridas.".to_string()),
        _ => Err("Sugestão desconhecida.".to_string()),
    }
}
