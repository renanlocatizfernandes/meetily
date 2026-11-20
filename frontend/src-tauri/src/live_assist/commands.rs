use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

use super::context::{ContextSettings, ContextSummary};
use super::context_store;
use super::recap::{AnswerResult, RecapResult};
use super::settings_store::{self, LiveAssistSettings};
use super::smart_settings::{self, SmartSettings};
use super::usage::UsageSnapshot;

/// Switches of the proactive part (alerts, economy mode).
/// JS: `invoke('get_live_assist_smart_settings')`
#[tauri::command]
pub async fn get_live_assist_smart_settings<R: Runtime>(app: AppHandle<R>) -> Result<SmartSettings, String> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();
    smart_settings::load_smart_settings(&pool)
        .await
        .map_err(|_| "Não foi possível ler as configurações da ajuda proativa.".to_string())
}

/// JS: `invoke('save_live_assist_smart_settings', { settings })`
#[tauri::command]
pub async fn save_live_assist_smart_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: SmartSettings,
) -> Result<SmartSettings, String> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();
    let settings = settings.normalized();
    smart_settings::save_smart_settings(&pool, &settings)
        .await
        .map_err(|_| "Não foi possível salvar as configurações da ajuda proativa.".to_string())?;
    Ok(settings)
}

/// "O que eu perdi?": summary of the last `minutes` of the recording in progress.
/// Calls the configured model once; with a cloud provider that spends tokens.
/// JS: `invoke('live_assist_recap', { minutes: 5 })`
#[tauri::command]
pub async fn live_assist_recap<R: Runtime>(app: AppHandle<R>, minutes: u32) -> Result<RecapResult, String> {
    super::service::run_recap(&app, minutes).await
}

/// Writes an answer to a question the other side asked. Calls the model once.
/// JS: `invoke('live_assist_suggest_answer', { question })`
#[tauri::command]
pub async fn live_assist_suggest_answer<R: Runtime>(
    app: AppHandle<R>,
    question: String,
) -> Result<AnswerResult, String> {
    super::service::suggest_answer(&app, &question).await
}

/// Estimated consumption of the recording in progress; `null` when not recording.
/// The same payload is emitted as `live-assist-usage` after every model call.
#[tauri::command]
pub async fn get_live_assist_usage() -> Result<Option<UsageSnapshot>, String> {
    Ok(super::service::current_usage())
}

#[tauri::command]
pub async fn get_live_assist_settings<R: Runtime>(
    app: AppHandle<R>,
) -> Result<LiveAssistSettings, String> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();
    settings_store::load_settings(&pool)
        .await
        .map_err(|_| "Não foi possível ler as configurações da ajuda ao vivo.".to_string())
}

#[tauri::command]
pub async fn save_live_assist_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: LiveAssistSettings,
) -> Result<LiveAssistSettings, String> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();
    let settings = settings.normalized();
    settings_store::save_settings(&pool, &settings)
        .await
        .map_err(|_| "Não foi possível salvar as configurações da ajuda ao vivo.".to_string())?;
    // The typed meeting context can name the client of a recording in progress.
    super::service::request_context_refresh();
    Ok(settings)
}

/// Owner profile, glossary and the switch for earlier meetings.
/// JS: `invoke('get_live_assist_context_settings')`
#[tauri::command]
pub async fn get_live_assist_context_settings<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ContextSettings, String> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();
    context_store::load_context_settings(&pool)
        .await
        .map_err(|_| "Não foi possível ler o perfil da ajuda ao vivo.".to_string())
}

/// JS: `invoke('save_live_assist_context_settings', { settings })`
#[tauri::command]
pub async fn save_live_assist_context_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: ContextSettings,
) -> Result<ContextSettings, String> {
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();
    let settings = settings.normalized();
    context_store::save_context_settings(&pool, &settings)
        .await
        .map_err(|_| "Não foi possível salvar o perfil da ajuda ao vivo.".to_string())?;
    super::service::request_context_refresh();
    Ok(settings)
}

/// "Contexto em uso" of the recording in progress; `null` when not recording.
/// The same payload is emitted as `live-assist-context` whenever it changes.
#[tauri::command]
pub async fn get_live_assist_context() -> Result<Option<ContextSummary>, String> {
    Ok(super::service::current_context_summary())
}

fn live_assist_url() -> WebviewUrl {
    // Dev server routes the Next page. The production export writes live-assist.html.
    if cfg!(debug_assertions) {
        WebviewUrl::App("live-assist".into())
    } else {
        WebviewUrl::App("live-assist.html".into())
    }
}

#[tauri::command]
pub async fn open_live_assist_window<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("live-assist") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        window
            .set_always_on_top(true)
            .map_err(|_| "Não foi possível manter a janela visível.".to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "live-assist", live_assist_url())
        .title("Ajuda ao vivo")
        .inner_size(400.0, 740.0)
        .min_inner_size(320.0, 420.0)
        .resizable(true)
        .always_on_top(true)
        .focused(true)
        .build()
        .map_err(|_| "Não foi possível abrir a janela da ajuda ao vivo.".to_string())?;
    Ok(())
}
