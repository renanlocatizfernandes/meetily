use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

use super::settings_store::{self, LiveAssistSettings};

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
    Ok(settings)
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
