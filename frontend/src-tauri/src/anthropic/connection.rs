//! "Testar conexão" for the cloud providers (Anthropic, OpenAI, OpenRouter).
//!
//! Makes one cheap authenticated GET (model list / key info). It spends no
//! tokens, and neither the key nor any response body is logged.

use crate::summary::llm_client::friendly_http_message;
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub provider: String,
    pub status: Option<u16>,
    pub latency_ms: u64,
    /// pt-BR message ready to show in the UI.
    pub message: String,
}

fn provider_request(
    client: &reqwest::Client,
    provider: &str,
    api_key: &str,
) -> Result<(&'static str, reqwest::RequestBuilder), String> {
    match provider {
        "claude" | "anthropic" => Ok((
            "Anthropic",
            client
                .get("https://api.anthropic.com/v1/models?limit=1")
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01"),
        )),
        "openai" => Ok((
            "OpenAI",
            client
                .get("https://api.openai.com/v1/models")
                .bearer_auth(api_key),
        )),
        // /auth/key validates the key itself (the model list is public).
        "openrouter" => Ok((
            "OpenRouter",
            client
                .get("https://openrouter.ai/api/v1/auth/key")
                .bearer_auth(api_key),
        )),
        other => Err(format!("Provedor sem teste de conexão: {other}")),
    }
}

/// Tests the API key for `provider` ("claude" | "openai" | "openrouter").
#[tauri::command]
pub async fn test_llm_provider_connection(
    provider: String,
    api_key: Option<String>,
) -> Result<ConnectionTestResult, String> {
    let key = api_key.unwrap_or_default();
    let key = key.trim();
    if key.is_empty() {
        return Ok(ConnectionTestResult {
            ok: false,
            provider,
            status: None,
            latency_ms: 0,
            message: "Informe a chave de API antes de testar.".to_string(),
        });
    }

    let client = reqwest::Client::new();
    let (label, request) = provider_request(&client, &provider.to_ascii_lowercase(), key)?;
    let started = Instant::now();
    let result = request.timeout(Duration::from_secs(15)).send().await;
    let latency_ms = started.elapsed().as_millis() as u64;

    let response = match result {
        Ok(r) => r,
        Err(e) => {
            let message = if e.is_timeout() {
                format!("{label} não respondeu a tempo. Verifique sua internet.")
            } else {
                format!("Não foi possível alcançar {label}. Verifique sua internet.")
            };
            return Ok(ConnectionTestResult { ok: false, provider, status: None, latency_ms, message });
        }
    };

    let status = response.status();
    if status.is_success() {
        // The key may have changed: drop stale model caches.
        super::anthropic::clear_cache();
        crate::openai::openai::clear_cache();
        return Ok(ConnectionTestResult {
            ok: true,
            provider,
            status: Some(status.as_u16()),
            latency_ms,
            message: format!("Conexão com {label} funcionando ({latency_ms} ms)."),
        });
    }

    let body = response.text().await.unwrap_or_default();
    log::warn!("{label} connection test failed with status {status}");
    Ok(ConnectionTestResult {
        ok: false,
        provider,
        status: Some(status.as_u16()),
        latency_ms,
        message: friendly_http_message(label, status.as_u16(), &body),
    })
}
