use tauri::{AppHandle, Runtime};

use super::model::{MeetingFacets, MeetingOrg, OrganizationSettings, OrganizeAllResult};
use super::service::{self, ManualEdit, OrganizeOptions, Outcome};
use super::store;

fn read_error(error: sqlx::Error) -> String {
    log::error!("Organize: database error: {}", error);
    "Não foi possível ler as conversas.".to_string()
}

#[tauri::command]
pub async fn list_meetings_organized<R: Runtime>(app: AppHandle<R>) -> Result<Vec<MeetingOrg>, String> {
    let pool = service::pool_from(&app)?;
    store::list_meetings(&pool).await.map_err(read_error)
}

/// JS: `invoke('organize_meeting', { meetingId, force })`
#[tauri::command]
pub async fn organize_meeting<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    force: bool,
) -> Result<MeetingOrg, String> {
    if service::recording_active().await {
        return Err("Não é possível organizar uma conversa durante uma gravação.".to_string());
    }
    let pool = service::pool_from(&app)?;
    let options = OrganizeOptions {
        force,
        cancel: None,
        defer_while_recording: true,
    };
    match service::organize(&app, &pool, &meeting_id, options).await? {
        Outcome::Organized(meeting) | Outcome::Unchanged(meeting) => Ok(meeting),
        Outcome::TooShort(_) => Err("A conversa tem pouco texto para ser classificada.".to_string()),
        Outcome::Deferred => Err("Não é possível organizar uma conversa durante uma gravação.".to_string()),
    }
}

/// JS: `invoke('update_meeting_classification', { meetingId, client, project, category, tags, pinned })`.
/// `null` leaves a field as it is; `""` (or `[]` for tags) clears it.
#[tauri::command]
pub async fn update_meeting_classification<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    client: Option<String>,
    project: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    pinned: Option<bool>,
) -> Result<MeetingOrg, String> {
    let pool = service::pool_from(&app)?;
    let edit = ManualEdit {
        client,
        project,
        category,
        tags,
        pinned,
    };
    service::update_classification(&app, &pool, &meeting_id, edit).await
}

#[tauri::command]
pub async fn list_meeting_facets<R: Runtime>(app: AppHandle<R>) -> Result<MeetingFacets, String> {
    let pool = service::pool_from(&app)?;
    store::facets(&pool).await.map_err(read_error)
}

#[tauri::command]
pub async fn get_organization_settings<R: Runtime>(app: AppHandle<R>) -> Result<OrganizationSettings, String> {
    let pool = service::pool_from(&app)?;
    store::load_settings(&pool)
        .await
        .map_err(|_| "Não foi possível ler as configurações de organização.".to_string())
}

/// JS: `invoke('save_organization_settings', { settings })`. Returns the
/// settings as saved (trimmed, without duplicates).
#[tauri::command]
pub async fn save_organization_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: OrganizationSettings,
) -> Result<OrganizationSettings, String> {
    let pool = service::pool_from(&app)?;
    store::save_settings(&pool, &settings)
        .await
        .map_err(|_| "Não foi possível salvar as configurações de organização.".to_string())
}

/// JS: `invoke('organize_all_meetings', { onlyUnorganized })`
#[tauri::command]
pub async fn organize_all_meetings<R: Runtime>(
    app: AppHandle<R>,
    only_unorganized: bool,
) -> Result<OrganizeAllResult, String> {
    let pool = service::pool_from(&app)?;
    service::organize_all(&app, &pool, only_unorganized).await
}

/// Returns true when a batch run was in progress and got the cancel signal.
#[tauri::command]
pub async fn cancel_organize_all() -> Result<bool, String> {
    Ok(service::cancel_all())
}
