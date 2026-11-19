//! Configurable global shortcut for the live assist panel.
//!
//! The shortcut does what the fixed one did: it emits `live-assist-toggle`.
//! The accelerator is stored in `live_assist_settings.shortcut` and the
//! registration result is kept in managed state so the settings screen can
//! show it.

use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use log::warn;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+L";
pub const STATUS_EVENT: &str = "live-assist-shortcut-status";
const MAX_SHORTCUT_CHARS: usize = 80;
/// How long startup waits for the database before using the default shortcut.
const STARTUP_DB_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub shortcut: String,
    pub registered: bool,
    pub error: Option<String>,
}

impl Default for ShortcutStatus {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            registered: false,
            error: None,
        }
    }
}

/// Managed state. `status` is what the commands report. `change` serializes
/// startup registration and `set_live_assist_shortcut` calls.
#[derive(Default)]
pub struct LiveAssistShortcutState {
    status: Mutex<ShortcutStatus>,
    change: tokio::sync::Mutex<()>,
}

impl LiveAssistShortcutState {
    fn current(&self) -> ShortcutStatus {
        self.status.lock().map(|status| status.clone()).unwrap_or_default()
    }

    fn store(&self, status: &ShortcutStatus) {
        if let Ok(mut slot) = self.status.lock() {
            *slot = status.clone();
        }
    }
}

/// Checks the accelerator text. The error is ready to show to the user.
pub fn parse_shortcut(raw: &str) -> Result<(String, Shortcut), String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("Informe um atalho, por exemplo CommandOrControl+Shift+L.".to_string());
    }
    if text.chars().count() > MAX_SHORTCUT_CHARS {
        return Err("O atalho é longo demais.".to_string());
    }
    let shortcut = Shortcut::from_str(text).map_err(|_| {
        "Atalho inválido. Use modificadores e uma tecla separados por +, por exemplo CommandOrControl+Shift+L."
            .to_string()
    })?;
    if shortcut.mods.is_empty() {
        return Err(
            "O atalho precisa de pelo menos um modificador (Ctrl, Cmd, Alt ou Shift) além da tecla.".to_string(),
        );
    }
    Ok((text.to_string(), shortcut))
}

fn registration_error(shortcut: &str) -> String {
    format!(
        "Não foi possível registrar o atalho {shortcut}. Ele pode estar em uso por outro aplicativo ou pelo sistema."
    )
}

fn register(app: &AppHandle<Wry>, shortcut: Shortcut) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = app.emit("live-assist-toggle", ());
            }
        })
        .map_err(|error| error.to_string())
}

pub async fn load_shortcut(pool: &SqlitePool) -> Result<String, sqlx::Error> {
    let row = sqlx::query("SELECT shortcut FROM live_assist_settings WHERE id = '1'")
        .fetch_optional(pool)
        .await?;
    let stored = row
        .and_then(|row| row.try_get::<String, _>("shortcut").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(stored.unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()))
}

pub async fn save_shortcut(pool: &SqlitePool, shortcut: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO live_assist_settings (id, shortcut) VALUES ('1', ?)
         ON CONFLICT(id) DO UPDATE SET shortcut = excluded.shortcut",
    )
    .bind(shortcut)
    .execute(pool)
    .await?;
    Ok(())
}

fn pool(app: &AppHandle<Wry>) -> Option<SqlitePool> {
    app.try_state::<AppState>()
        .map(|state| state.db_manager.pool().clone())
}

/// Called once from the Tauri setup hook. Registers the saved shortcut (or
/// the default while the database is not ready) without blocking startup.
pub fn init(app: &AppHandle<Wry>) {
    app.manage(LiveAssistShortcutState::default());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<LiveAssistShortcutState>();
        let _change = state.change.lock().await;

        let waited = tokio::time::Instant::now();
        let mut pool_ready = pool(&app);
        while pool_ready.is_none() && waited.elapsed() < STARTUP_DB_WAIT {
            tokio::time::sleep(Duration::from_millis(100)).await;
            pool_ready = pool(&app);
        }
        let stored = match &pool_ready {
            Some(pool) => load_shortcut(pool).await.unwrap_or_else(|error| {
                warn!("Live assist shortcut was not read ({error})");
                DEFAULT_SHORTCUT.to_string()
            }),
            None => DEFAULT_SHORTCUT.to_string(),
        };

        let status = match parse_shortcut(&stored) {
            Ok((text, shortcut)) => match register(&app, shortcut) {
                Ok(()) => ShortcutStatus {
                    shortcut: text,
                    registered: true,
                    error: None,
                },
                Err(error) => {
                    warn!("Live assist shortcut was not registered: {error}");
                    ShortcutStatus {
                        error: Some(registration_error(&text)),
                        shortcut: text,
                        registered: false,
                    }
                }
            },
            Err(error) => {
                warn!("Saved live assist shortcut is not valid");
                ShortcutStatus {
                    shortcut: stored,
                    registered: false,
                    error: Some(error),
                }
            }
        };
        state.store(&status);
        if !status.registered {
            let _ = app.emit(STATUS_EVENT, &status);
        }
    });
}

#[tauri::command]
pub async fn get_live_assist_shortcut(app: AppHandle<Wry>) -> Result<ShortcutStatus, String> {
    let state = app
        .try_state::<LiveAssistShortcutState>()
        .ok_or_else(|| "O atalho da ajuda ao vivo ainda não está disponível.".to_string())?;
    Ok(state.current())
}

/// Swaps the global shortcut. The new one is saved only after it registers.
/// On failure the previous shortcut is registered again and the returned
/// status describes it, with `error` explaining what went wrong.
#[tauri::command]
pub async fn set_live_assist_shortcut(app: AppHandle<Wry>, shortcut: String) -> Result<ShortcutStatus, String> {
    let state = app
        .try_state::<LiveAssistShortcutState>()
        .ok_or_else(|| "O atalho da ajuda ao vivo ainda não está disponível.".to_string())?;
    let _change = state.change.lock().await;
    let previous = state.current();

    let (text, parsed) = match parse_shortcut(&shortcut) {
        Ok(value) => value,
        Err(error) => {
            return Ok(ShortcutStatus {
                error: Some(error),
                ..previous
            })
        }
    };
    let Some(pool) = pool(&app) else {
        return Ok(ShortcutStatus {
            error: Some("Banco de dados indisponível. O atalho não foi alterado.".to_string()),
            ..previous
        });
    };

    let previous_parsed = parse_shortcut(&previous.shortcut).ok().map(|(_, shortcut)| shortcut);
    let same_keys = previous.registered && previous_parsed == Some(parsed);
    if !same_keys {
        if let Some(old) = previous_parsed {
            if previous.registered {
                if let Err(error) = app.global_shortcut().unregister(old) {
                    warn!("Previous live assist shortcut was not released: {error}");
                }
            }
        }
        if let Err(error) = register(&app, parsed) {
            warn!("New live assist shortcut was not registered: {error}");
            let restored = restore(&app, &previous, previous_parsed);
            state.store(&restored);
            return Ok(ShortcutStatus {
                error: Some(registration_error(&text)),
                ..restored
            });
        }
    }

    if let Err(error) = save_shortcut(&pool, &text).await {
        warn!("Live assist shortcut was not saved ({error})");
        if !same_keys {
            let _ = app.global_shortcut().unregister(parsed);
            let restored = restore(&app, &previous, previous_parsed);
            state.store(&restored);
            return Ok(ShortcutStatus {
                error: Some("Não foi possível salvar o atalho. O anterior foi mantido.".to_string()),
                ..restored
            });
        }
        return Ok(ShortcutStatus {
            error: Some("Não foi possível salvar o atalho.".to_string()),
            ..previous
        });
    }

    let status = ShortcutStatus {
        shortcut: text,
        registered: true,
        error: None,
    };
    state.store(&status);
    Ok(status)
}

/// Registers the previous shortcut again after a failed change.
fn restore(app: &AppHandle<Wry>, previous: &ShortcutStatus, parsed: Option<Shortcut>) -> ShortcutStatus {
    let registered = match parsed {
        Some(shortcut) if previous.registered => match register(app, shortcut) {
            Ok(()) => true,
            Err(error) => {
                warn!("Previous live assist shortcut was not restored: {error}");
                false
            }
        },
        _ => false,
    };
    ShortcutStatus {
        shortcut: previous.shortcut.clone(),
        registered,
        error: if registered || !previous.registered {
            previous.error.clone()
        } else {
            Some(registration_error(&previous.shortcut))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_and_common_accelerators_parse() {
        let (text, _) = parse_shortcut(DEFAULT_SHORTCUT).expect("default");
        assert_eq!(text, DEFAULT_SHORTCUT);
        assert!(parse_shortcut("  Alt+Shift+K ").is_ok());
        assert_eq!(parse_shortcut("  Alt+Shift+K ").unwrap().0, "Alt+Shift+K");
        assert!(parse_shortcut("CmdOrCtrl+Alt+F9").is_ok());
        assert_eq!(
            parse_shortcut("ctrl+shift+l").unwrap().1,
            parse_shortcut("Control+Shift+L").unwrap().1
        );
    }

    #[test]
    fn invalid_accelerators_return_a_message_in_portuguese() {
        for raw in ["", "   ", "Ctrl+", "Ctrl+Shift+NaoExiste", "++", "Ctrl+Shift"] {
            let error = parse_shortcut(raw).expect_err(raw);
            assert!(error.contains("atalho") || error.contains("Atalho"), "{raw}: {error}");
        }
        let error = parse_shortcut("L").expect_err("no modifier");
        assert!(error.contains("modificador"));
        assert!(parse_shortcut(&"A+".repeat(60)).is_err());
    }

    #[test]
    fn status_serializes_in_camel_case() {
        let json = serde_json::to_value(ShortcutStatus::default()).unwrap();
        assert_eq!(json["shortcut"], DEFAULT_SHORTCUT);
        assert_eq!(json["registered"], false);
        assert!(json["error"].is_null());
    }

    #[tokio::test]
    async fn the_shortcut_column_defaults_and_round_trips() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        assert_eq!(load_shortcut(&pool).await.unwrap(), DEFAULT_SHORTCUT);
        save_shortcut(&pool, "Alt+Shift+K").await.unwrap();
        assert_eq!(load_shortcut(&pool).await.unwrap(), "Alt+Shift+K");
        // Saving the other live assist settings leaves the shortcut alone.
        let settings = crate::live_assist::settings_store::load_settings(&pool).await.unwrap();
        crate::live_assist::settings_store::save_settings(&pool, &settings).await.unwrap();
        assert_eq!(load_shortcut(&pool).await.unwrap(), "Alt+Shift+K");
    }
}
