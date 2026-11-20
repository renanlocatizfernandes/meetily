//! Optional connection to Jev, TypeSafe AI's "System One" decision model
//! (docs.typesafe.ai/api), or any server with the same contract (for example
//! a local Kev server). Off by default: nothing here touches the network
//! unless the person turns it on or presses "Testar conexão".
//!
//! Jev returns typed answers with probabilities (yes/no, choice, score)
//! instead of text, which makes it a cheap filter in front of the paid LLMs.
//! Features call [`decide_if_enabled`]; the settings card uses the commands.

pub mod client;
pub mod settings;

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

pub use client::{Answer, DecideRequest, DecideResponse, JevClient, JevError, Question};
use settings::{PublicSettings, SettingsUpdate, StoredSettings, STORE_FILE, STORE_KEY};

const TEST_TIMEOUT: Duration = Duration::from_secs(15);
const DECIDE_TIMEOUT: Duration = Duration::from_secs(20);

pub fn load<R: Runtime>(app: &AppHandle<R>) -> StoredSettings {
    let Ok(store) = app.store(STORE_FILE) else {
        return StoredSettings::default();
    };
    store
        .get(STORE_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn save<R: Runtime>(app: &AppHandle<R>, settings: &StoredSettings) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|_| "Não foi possível abrir as configurações do Jev.".to_string())?;
    let previous = store.get(STORE_KEY);
    let value = serde_json::to_value(settings).map_err(|_| "Configuração inválida.".to_string())?;
    store.set(STORE_KEY, value);
    if store.save().is_err() {
        match previous {
            Some(old) => store.set(STORE_KEY, old),
            None => {
                store.delete(STORE_KEY);
            }
        }
        return Err("Não foi possível salvar as configurações do Jev.".to_string());
    }
    Ok(())
}

fn client_for(settings: &StoredSettings, timeout: Duration) -> Result<JevClient, JevError> {
    JevClient::new(&settings.base_url, settings.api_key.clone(), timeout)
}

/// Entry point for features: returns `JevError::Disabled` without any network
/// call when the connection is off, so callers fall back to their local path.
pub async fn decide_if_enabled<R: Runtime>(
    app: &AppHandle<R>,
    mut request: DecideRequest,
) -> Result<DecideResponse, JevError> {
    let settings = load(app);
    if !settings.enabled {
        return Err(JevError::Disabled);
    }
    if request.model.is_empty() {
        request.model = settings.model.clone();
    }
    client_for(&settings, DECIDE_TIMEOUT)?.decide(&request).await
}

#[tauri::command]
pub async fn jev_get_settings<R: Runtime>(app: AppHandle<R>) -> Result<PublicSettings, String> {
    Ok(load(&app).to_public())
}

#[tauri::command]
pub async fn jev_save_settings<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
    base_url: String,
    model: String,
    api_key: Option<String>,
    clear_key: Option<bool>,
) -> Result<PublicSettings, String> {
    let current = load(&app);
    let next = settings::apply_update(
        &current,
        SettingsUpdate {
            enabled,
            base_url,
            model,
            api_key,
            clear_key: clear_key.unwrap_or(false),
        },
    )?;
    save(&app, &next)?;
    log::info!("Jev settings saved (enabled: {}, has key: {})", next.enabled, next.has_key());
    Ok(next.to_public())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    /// Models the server listed (may be empty on compatible servers).
    pub models: Vec<String>,
    /// Whether the configured model is in that list; `None` when the list is empty.
    pub model_listed: Option<bool>,
    /// True only when a real System One call succeeded with the key.
    pub key_verified: bool,
    pub input_tokens: Option<u64>,
    pub latency_ms: u64,
}

impl TestResult {
    fn failed(error: &JevError, models: Vec<String>, started: Instant) -> Self {
        Self {
            ok: false,
            message: error.user_message(),
            models,
            model_listed: None,
            key_verified: false,
            input_tokens: None,
            latency_ms: started.elapsed().as_millis() as u64,
        }
    }
}

/// Tests the saved settings. Works while the connection is off so the person
/// can check before turning it on. Step 1 lists models (free). Step 2, only
/// with `verify_key`, sends a one-question call that costs a few input tokens.
#[tauri::command]
pub async fn jev_test_connection<R: Runtime>(
    app: AppHandle<R>,
    verify_key: Option<bool>,
) -> Result<TestResult, String> {
    let settings = load(&app);
    Ok(run_test(&settings, verify_key.unwrap_or(false)).await)
}

pub async fn run_test(settings: &StoredSettings, verify_key: bool) -> TestResult {
    let started = Instant::now();
    if verify_key && !settings.has_key() {
        return TestResult::failed(&JevError::MissingKey, Vec::new(), started);
    }
    let client = match client_for(settings, TEST_TIMEOUT) {
        Ok(client) => client,
        Err(error) => return TestResult::failed(&error, Vec::new(), started),
    };
    let models = match client.list_models().await {
        Ok(models) => models,
        Err(error) => {
            log::warn!("Jev connection test failed at /v1/models: {error:?}");
            return TestResult::failed(&error, Vec::new(), started);
        }
    };
    let model_listed = (!models.is_empty()).then(|| models.iter().any(|m| m == &settings.model));

    if !verify_key {
        let message = match model_listed {
            Some(true) => format!("Servidor respondeu e o modelo {} está disponível. A chave só é conferida em \"Validar chave\".", settings.model),
            Some(false) => format!("Servidor respondeu, mas o modelo {} não está na lista.", settings.model),
            None => "Servidor respondeu. A chave só é conferida em \"Validar chave\".".to_string(),
        };
        return TestResult {
            ok: model_listed != Some(false),
            message,
            models,
            model_listed,
            key_verified: false,
            input_tokens: None,
            latency_ms: started.elapsed().as_millis() as u64,
        };
    }

    match client.decide(&client::ping_request(&settings.model)).await {
        Ok(response) if client::ping_ok(&response) => TestResult {
            ok: true,
            message: format!("Conexão funcionando: chave aceita e o modelo {} respondeu.", settings.model),
            models,
            model_listed,
            key_verified: true,
            input_tokens: response.usage.map(|u| u.input_tokens),
            latency_ms: started.elapsed().as_millis() as u64,
        },
        Ok(_) => TestResult::failed(&JevError::BadResponse, models, started),
        Err(error) => {
            log::warn!("Jev connection test failed at /v1/systemone: {error:?}");
            TestResult::failed(&error, models, started)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mock(responses: Vec<(u16, &'static str)>) -> String {
        client::mock::serve(responses).await.0
    }

    fn settings_for(base: String, key: Option<&str>) -> StoredSettings {
        StoredSettings {
            enabled: false,
            base_url: base,
            model: "jev-latest".into(),
            api_key: key.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn jev_test_lists_models_without_spending() {
        let base = mock(vec![(200, r#"{"data":[{"id":"jev-latest"}]}"#)]).await;
        let result = run_test(&settings_for(base, None), false).await;
        assert!(result.ok, "{result:?}");
        assert_eq!(result.model_listed, Some(true));
        assert!(!result.key_verified);
    }

    #[tokio::test]
    async fn jev_test_flags_a_missing_model() {
        let base = mock(vec![(200, r#"{"data":[{"id":"jev-preview"}]}"#)]).await;
        let result = run_test(&settings_for(base, None), false).await;
        assert!(!result.ok);
        assert_eq!(result.model_listed, Some(false));
    }

    #[tokio::test]
    async fn jev_verify_key_needs_a_key() {
        let result = run_test(&settings_for("http://127.0.0.1:9".into(), None), true).await;
        assert!(!result.ok);
        assert_eq!(result.message, JevError::MissingKey.user_message());
    }

    #[tokio::test]
    async fn jev_verify_key_success() {
        let base = mock(vec![
            (200, r#"{"data":[{"id":"jev-latest"}]}"#),
            (200, r#"{"answers":{"ping":{"type":"noul","noul":0.99}},"usage":{"input_tokens":18,"output_tokens":1}}"#),
        ])
        .await;
        let result = run_test(&settings_for(base, Some("test-key")), true).await;
        assert!(result.ok, "{result:?}");
        assert!(result.key_verified);
        assert_eq!(result.input_tokens, Some(18));
    }

    #[tokio::test]
    async fn jev_verify_key_reports_rejection() {
        let base = mock(vec![(200, "[]"), (401, r#"{"detail":"bad key"}"#)]).await;
        let result = run_test(&settings_for(base, Some("bad")), true).await;
        assert!(!result.ok);
        assert!(!result.key_verified);
        assert_eq!(result.message, JevError::Unauthorized.user_message());
    }
}
