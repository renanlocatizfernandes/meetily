use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use log::{info, warn};

const REQUEST_TIMEOUT_DURATION: Duration = Duration::from_secs(300);

async fn await_or_cancel<T>(
    operation: impl Future<Output = T>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<T, String> {
    let Some(token) = cancellation_token else {
        return Ok(operation.await);
    };

    tokio::select! {
        biased;
        _ = token.cancelled() => Err("Summary generation was cancelled".to_string()),
        result = operation => Ok(result),
    }
}

// Generic structure for OpenAI-compatible API chat messages
#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// Generic structure for OpenAI-compatible API chat requests
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<&'static str>,
}

/// Build OpenAI-compat JSON body.
pub fn build_openai_compat_chat_body(
    provider: &LLMProvider,
    model_name: &str,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
) -> serde_json::Value {
    let (max_tokens_val, temperature_val, top_p_val) = if *provider == LLMProvider::CustomOpenAI {
        (max_tokens, temperature, top_p)
    } else {
        (None, None, None)
    };

    serde_json::json!(ChatRequest {
        model: model_name.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }
        ],
        max_tokens: max_tokens_val,
        temperature: temperature_val,
        top_p: top_p_val,
        reasoning_effort: (*provider == LLMProvider::Ollama).then_some("none"),
    })
}

// Generic structure for OpenAI-compatible API chat responses
#[derive(Deserialize, Debug)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: MessageContent,
}

#[derive(Deserialize, Debug)]
pub struct MessageContent {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

// Claude-specific request structure
#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: serde_json::Value,
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<serde_json::Value>,
}

/// Minimum user-prompt size (chars) before we mark it as a cache breakpoint.
/// Prefixes below the model's minimum are silently not cached, so this only
/// avoids pointless markers on tiny prompts.
const CLAUDE_CACHE_MIN_CHARS: usize = 4000;

/// Whether `model` is a Claude 5.x model, which accepts `output_config.effort`.
/// Older models reject the field, so it is never sent to them.
pub(crate) fn claude_supports_effort(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    ["opus", "sonnet", "haiku", "fable", "mythos"]
        .iter()
        .any(|family| m.starts_with(&format!("claude-{family}-5")))
}

/// Build the Anthropic Messages API body with prompt caching.
///
/// The system prompt and (for long transcripts) the user content carry an
/// ephemeral `cache_control` breakpoint, so chunked summaries that reuse the
/// same prefix are billed at the cache-read rate.
pub(crate) fn build_claude_body(
    model_name: &str,
    system_prompt: &str,
    user_prompt: &str,
    effort: Option<&str>,
) -> ClaudeRequest {
    let mut user_block = serde_json::json!({"type": "text", "text": user_prompt});
    if user_prompt.len() >= CLAUDE_CACHE_MIN_CHARS {
        user_block["cache_control"] = serde_json::json!({"type": "ephemeral"});
    }
    ClaudeRequest {
        model: model_name.to_string(),
        // Shared budget: thinking tokens count against it as well.
        max_tokens: 16000,
        system: serde_json::json!([{
            "type": "text",
            "text": system_prompt,
            "cache_control": {"type": "ephemeral"}
        }]),
        messages: vec![serde_json::json!({"role": "user", "content": [user_block]})],
        output_config: effort
            .filter(|_| claude_supports_effort(model_name))
            .map(|e| serde_json::json!({"effort": e})),
    }
}

// Claude-specific response structure
#[derive(Deserialize, Debug)]
pub struct ClaudeChatResponse {
    pub content: Vec<ClaudeChatContent>,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeChatContent {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
}

impl ClaudeChatResponse {
    fn completion(&self) -> Option<LlmCompletion> {
        let content = self
            .content
            .iter()
            .find(|block| block.block_type == "text")
            .and_then(|block| block.text.as_deref())?
            .trim()
            .to_string();
        Some(LlmCompletion {
            content,
            reasoning_stripped: self.content.iter().any(|block| {
                matches!(block.block_type.as_str(), "thinking" | "redacted_thinking")
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlmCompletion {
    pub content: String,
    pub reasoning_stripped: bool,
}

impl ChatResponse {
    fn completion(&self) -> Result<LlmCompletion, String> {
        let message = &self
            .choices
            .first()
            .ok_or("No content in LLM response")?
            .message;
        Ok(LlmCompletion {
            content: message
                .content
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string(),
            reasoning_stripped: [message.reasoning.as_deref(), message.reasoning_content.as_deref()]
                .into_iter()
                .flatten()
                .any(|reasoning| !reasoning.trim().is_empty()),
        })
    }
}

pub(crate) fn ollama_rejects_reasoning_effort(status: reqwest::StatusCode, body: &str) -> bool {
    if !matches!(status.as_u16(), 400 | 422) {
        return false;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    let matches_field = |value: Option<&serde_json::Value>| {
        value
            .and_then(serde_json::Value::as_str)
            .is_some_and(|field| field.eq_ignore_ascii_case("reasoning_effort"))
    };
    if matches_field(value.get("param"))
        || matches_field(value.get("field"))
        || matches_field(value.get("error").and_then(|error| error.get("param")))
        || matches_field(value.get("error").and_then(|error| error.get("field")))
    {
        return true;
    }

    let matches_message = [
        value.as_str(),
        value.get("error").and_then(serde_json::Value::as_str),
        value.get("message").and_then(serde_json::Value::as_str),
        value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str),
    ]
    .into_iter()
    .flatten()
    .any(|message| {
        let message = message.to_ascii_lowercase();
        message.contains("reasoning_effort")
            || (message.contains("think value")
                && message.contains("none")
                && (message.contains("not supported") || message.contains("invalid")))
    });
    matches_message
}
/// LLM Provider enumeration for multi-provider support
#[derive(Debug, Clone, PartialEq)]
pub enum LLMProvider {
    OpenAI,
    Claude,
    Groq,
    Ollama,
    OpenRouter,
    BuiltInAI,
    CustomOpenAI,
}

impl LLMProvider {
    /// Parse provider from string (case-insensitive)
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "claude" => Ok(Self::Claude),
            "groq" => Ok(Self::Groq),
            "ollama" => Ok(Self::Ollama),
            "openrouter" => Ok(Self::OpenRouter),
            "builtin-ai" | "local-llama" | "localllama" => Ok(Self::BuiltInAI),
            "custom-openai" => Ok(Self::CustomOpenAI),
            _ => Err(format!("Unsupported LLM provider: {}", s)),
        }
    }
}

/// Public entry point: runs the primary provider and, only when the user has
/// configured a fallback chain (off by default), tries each fallback entry in
/// order when the primary fails. Cancellation never triggers a fallback.
pub(crate) async fn generate_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    ollama_endpoint: Option<&str>,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<LlmCompletion, String> {
    let primary = generate_summary_once(
        client, provider, model_name, api_key, system_prompt, user_prompt,
        ollama_endpoint, custom_openai_endpoint, max_tokens, temperature, top_p,
        app_data_dir, cancellation_token,
    )
    .await;
    let mut last_error = match primary {
        Ok(done) => return Ok(done),
        Err(e) => e,
    };
    if cancellation_token.is_some_and(|t| t.is_cancelled()) {
        return Err(last_error);
    }

    for entry in fallback_chain_snapshot() {
        let Ok(fb_provider) = LLMProvider::from_str(&entry.provider) else { continue };
        if &fb_provider == provider && entry.model == model_name {
            continue;
        }
        if fb_provider == LLMProvider::BuiltInAI && app_data_dir.is_none() {
            continue;
        }
        warn!(
            "Primary LLM provider {} failed; falling back to {}",
            provider_name(provider),
            provider_name(&fb_provider)
        );
        match generate_summary_once(
            client, &fb_provider, &entry.model, &entry.api_key, system_prompt, user_prompt,
            ollama_endpoint, custom_openai_endpoint, max_tokens, temperature, top_p,
            app_data_dir, cancellation_token,
        )
        .await
        {
            Ok(done) => return Ok(done),
            Err(e) => {
                if cancellation_token.is_some_and(|t| t.is_cancelled()) {
                    return Err(e);
                }
                last_error = format!("{last_error} | Alternativa {} falhou: {e}", provider_name(&fb_provider));
            }
        }
    }
    Err(last_error)
}

/// Generates a summary using the specified LLM provider
///
/// # Arguments
/// * `client` - Reqwest HTTP client (reused for performance)
/// * `provider` - The LLM provider to use
/// * `model_name` - The specific model to use (e.g., "gpt-4", "claude-3-opus")
/// * `api_key` - API key for the provider (not needed for Ollama)
/// * `system_prompt` - System instructions for the LLM
/// * `user_prompt` - User query/content to process
/// * `ollama_endpoint` - Optional custom Ollama endpoint (defaults to localhost:11434)
/// * `custom_openai_endpoint` - Optional custom OpenAI-compatible endpoint
/// * `max_tokens` - Optional max tokens (for CustomOpenAI provider)
/// * `temperature` - Optional temperature (for CustomOpenAI provider)
/// * `top_p` - Optional top_p (for CustomOpenAI provider)
/// * `app_data_dir` - Optional app data directory (for BuiltInAI provider)
/// * `cancellation_token` - Optional token to cancel the request
///
/// The generated visible content and whether private reasoning was removed.
async fn generate_summary_once(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    ollama_endpoint: Option<&str>,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<LlmCompletion, String> {
    // Check if cancelled before starting
    if let Some(token) = cancellation_token {
        if token.is_cancelled() {
            return Err("Summary generation was cancelled".to_string());
        }
    }

    // Handle BuiltInAI provider separately (uses local sidecar, no HTTP API)
    if provider == &LLMProvider::BuiltInAI {
        let app_data_dir = app_data_dir
            .ok_or_else(|| "app_data_dir is required for BuiltInAI provider".to_string())?;

        return crate::summary::summary_engine::generate_with_builtin(
            app_data_dir,
            model_name,
            system_prompt,
            user_prompt,
            cancellation_token,
        )
        .await
        .map(|content| LlmCompletion {
            content,
            reasoning_stripped: false,
        })
        .map_err(|e| e.to_string());
    }

    let (api_url, mut headers) = match provider {
        LLMProvider::OpenAI => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Groq => (
            "https://api.groq.com/openai/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::OpenRouter => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Ollama => {
            let host = ollama_endpoint
                .map(|s| s.to_string())
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            (
                format!("{}/v1/chat/completions", host),
                header::HeaderMap::new(),
            )
        }
        LLMProvider::CustomOpenAI => {
            let endpoint = custom_openai_endpoint
                .ok_or_else(|| "Custom OpenAI endpoint not configured".to_string())?;
            (
                format!("{}/chat/completions", endpoint.trim_end_matches('/')),
                header::HeaderMap::new(),
            )
        }
        LLMProvider::Claude => {
            let mut header_map = header::HeaderMap::new();
            header_map.insert(
                "x-api-key",
                api_key
                    .parse()
                    .map_err(|_| "Invalid API key format".to_string())?,
            );
            header_map.insert(
                "anthropic-version",
                "2023-06-01"
                    .parse()
                    .map_err(|_| "Invalid anthropic version".to_string())?,
            );
            ("https://api.anthropic.com/v1/messages".to_string(), header_map)
        }
        LLMProvider::BuiltInAI => {
            // This case is handled earlier with early returns
            unreachable!("BuiltInAI is handled before this match statement")
        }
    };

    // Add authorization header for non-Claude providers
    if provider != &LLMProvider::Claude {
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", api_key)
                .parse()
                .map_err(|_| "Invalid authorization header".to_string())?,
        );
    }
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .map_err(|_| "Invalid content type".to_string())?,
    );

    let effort = current_reasoning_effort();

    // Build request body based on provider
    let request_body = if provider != &LLMProvider::Claude {
        build_openai_compat_chat_body(
            provider,
            model_name,
            system_prompt,
            user_prompt,
            max_tokens,
            temperature,
            top_p,
        )
    } else {
        serde_json::json!(build_claude_body(
            model_name,
            system_prompt,
            user_prompt,
            effort.as_deref()
        ))
    };
    // Reasoning effort is optional and off ("auto") by default.
    let mut request_body = request_body;
    let mut effort_added = provider == &LLMProvider::Claude
        && request_body.get("output_config").is_some();
    if let Some(e) = effort.as_deref() {
        // OpenAI-style APIs only know low/medium/high.
        let e = if matches!(e, "xhigh" | "max") { "high" } else { e };
        match provider {
            LLMProvider::OpenAI if openai_supports_reasoning(model_name) => {
                request_body["reasoning_effort"] = serde_json::json!(e);
                effort_added = true;
            }
            LLMProvider::OpenRouter => {
                request_body["reasoning"] = serde_json::json!({"effort": e});
                effort_added = true;
            }
            _ => {}
        }
    }
    let retry_policy = if is_cloud_provider(provider) { MAX_ATTEMPTS } else { 1 };

    info!("🐞 LLM Request to {}: model={}", provider_name(provider), model_name);

    // Send request with timeout, cancellation and (cloud providers) retry/backoff
    let response = send_with_retry(
        client,
        &api_url,
        &headers,
        &request_body,
        retry_policy,
        cancellation_token,
    )
    .await?;

    let response = if response.status().is_success() {
        response
    } else {
        let status = response.status();
        let error_body = await_or_cancel(response.text(), cancellation_token)
            .await?
            .unwrap_or_else(|error| format!("Failed to read LLM error response body: {error}"));
        let rejects_effort = if provider == &LLMProvider::Ollama {
            ollama_rejects_reasoning_effort(status, &error_body)
        } else {
            effort_added && cloud_rejects_reasoning(status, &error_body)
        };
        if !rejects_effort {
            warn!("{}", llm_http_failure_log_message(provider, status, error_body.len()));
            return Err(format_llm_error(provider, status, &error_body));
        }

        warn!("{} rejected the reasoning parameter; retrying once without it", provider_name(provider));
        let mut retry_body = request_body;
        {
            let obj = retry_body
                .as_object_mut()
                .ok_or_else(|| "Failed to prepare compatibility retry".to_string())?;
            obj.remove("reasoning_effort");
            obj.remove("reasoning");
            obj.remove("output_config");
        }
        let retry_future = client
            .post(api_url)
            .headers(headers)
            .json(&retry_body)
            .timeout(REQUEST_TIMEOUT_DURATION)
            .send();
        await_or_cancel(retry_future, cancellation_token)
            .await?
            .map_err(|e| {
                if e.is_timeout() {
                    format!(
                        "LLM retry request timed out after {} seconds",
                        REQUEST_TIMEOUT_DURATION.as_secs()
                    )
                } else {
                    format!("Failed to send retry request to LLM: {}", e)
                }
            })?
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_body = await_or_cancel(response.text(), cancellation_token)
            .await?
            .unwrap_or_else(|error| format!("Failed to read LLM error response body: {error}"));
        warn!("{}", llm_http_failure_log_message(provider, status, error_body.len()));
        return Err(format_llm_error(provider, status, &error_body));
    }

    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = await_or_cancel(
            response.json::<ClaudeChatResponse>(),
            cancellation_token,
        )
        .await?
        .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from Claude");

        let completion = chat_response
            .completion()
            .ok_or("No text content in LLM response")?;
        Ok(completion)
    } else {
        let chat_response = await_or_cancel(
            response.json::<ChatResponse>(),
            cancellation_token,
        )
        .await?
        .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from {}", provider_name(provider));
        chat_response.completion()
    }
}

// ---------------------------------------------------------------------------
// Retry/backoff, friendly errors, reasoning effort and fallback chain
// ---------------------------------------------------------------------------

/// Attempts (including the first) for cloud providers.
const MAX_ATTEMPTS: u32 = 3;
const MAX_BACKOFF: Duration = Duration::from_secs(20);

/// Cloud providers get retry/backoff and friendly error messages; local and
/// user-hosted endpoints keep the raw behaviour.
pub(crate) fn is_cloud_provider(provider: &LLMProvider) -> bool {
    matches!(
        provider,
        LLMProvider::OpenAI | LLMProvider::Claude | LLMProvider::OpenRouter | LLMProvider::Groq
    )
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504 | 529)
}

fn backoff_delay(attempt: u32, retry_after: Option<&header::HeaderValue>) -> Duration {
    let from_header = retry_after
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    from_header
        .unwrap_or_else(|| Duration::from_secs(1u64 << attempt.min(4)))
        .min(MAX_BACKOFF)
}

/// POST with timeout, cancellation and retry/backoff on 429/5xx/529 and
/// connection errors. Returns the last response (success or not) so callers
/// keep their existing error handling. `attempts == 1` disables retries.
async fn send_with_retry(
    client: &Client,
    url: &str,
    headers: &header::HeaderMap,
    body: &serde_json::Value,
    attempts: u32,
    cancellation_token: Option<&CancellationToken>,
) -> Result<reqwest::Response, String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let future = client
            .post(url)
            .headers(headers.clone())
            .json(body)
            .timeout(REQUEST_TIMEOUT_DURATION)
            .send();
        let result = await_or_cancel(future, cancellation_token).await?;
        let delay = match &result {
            Ok(resp) if is_retryable_status(resp.status()) && attempt < attempts => {
                Some(backoff_delay(attempt, resp.headers().get(header::RETRY_AFTER)))
            }
            Err(e) if e.is_connect() && attempt < attempts => Some(backoff_delay(attempt, None)),
            _ => None,
        };
        let Some(delay) = delay else {
            return result.map_err(|e| {
                if e.is_timeout() {
                    format!(
                        "LLM request timed out after {} seconds",
                        REQUEST_TIMEOUT_DURATION.as_secs()
                    )
                } else {
                    format!("Failed to send request to LLM: {}", e)
                }
            });
        };
        warn!("LLM request attempt {attempt}/{attempts} failed transiently; retrying in {delay:?}");
        await_or_cancel(tokio::time::sleep(delay), cancellation_token).await?;
    }
}

fn extract_api_error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("error")
        .and_then(|e| e.get("message").or(Some(e)))
        .and_then(|m| m.as_str())
        .or_else(|| value.get("message").and_then(|m| m.as_str()))
        .map(|m| m.chars().take(300).collect())
}

/// User-facing (pt-BR) explanation of a provider HTTP failure.
pub(crate) fn friendly_http_message(label: &str, status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    let no_credit = status == 402
        || lower.contains("credit balance")
        || lower.contains("insufficient_quota")
        || lower.contains("insufficient credits")
        || lower.contains("exceeded your current quota");
    let text = if no_credit {
        format!("Sem crédito ou cota esgotada em {label}. Adicione saldo na conta do provedor ou troque de modelo.")
    } else {
        match status {
            401 | 403 => format!("Chave de API inválida ou sem permissão em {label}. Confira a chave nas configurações."),
            404 => format!("Modelo não encontrado em {label}. Escolha outro modelo na lista."),
            413 => format!("A transcrição é grande demais para o modelo escolhido em {label}. Use um modelo com contexto maior."),
            429 => format!("Limite de uso de {label} atingido. Aguarde um pouco e tente de novo."),
            500..=599 => format!("O serviço {label} está instável ou sobrecarregado. Tente de novo em instantes."),
            _ => format!("{label} recusou a requisição."),
        }
    };
    match extract_api_error_message(body) {
        Some(detail) if !matches!(status, 401 | 403) => format!("{text} (HTTP {status}: {detail})"),
        _ => format!("{text} (HTTP {status})"),
    }
}

fn format_llm_error(provider: &LLMProvider, status: reqwest::StatusCode, body: &str) -> String {
    if is_cloud_provider(provider) {
        friendly_http_message(provider_name(provider), status.as_u16(), body)
    } else {
        format!("LLM API request failed with status {}: {}", status, body)
    }
}

fn openai_supports_reasoning(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    let b = m.as_bytes();
    m.starts_with("gpt-5") || (b.len() > 1 && b[0] == b'o' && b[1].is_ascii_digit())
}

fn cloud_rejects_reasoning(status: reqwest::StatusCode, body: &str) -> bool {
    if !matches!(status.as_u16(), 400 | 422) {
        return false;
    }
    let lower = body.to_ascii_lowercase();
    lower.contains("reasoning") || lower.contains("effort") || lower.contains("output_config")
}

/// Fallback entry; deliberately no `Debug` so the key can never be logged.
#[derive(Clone, Deserialize)]
pub struct FallbackEntry {
    pub provider: String,
    pub model: String,
    #[serde(default, rename = "apiKey")]
    pub api_key: String,
}

static FALLBACK_CHAIN: std::sync::RwLock<Vec<FallbackEntry>> = std::sync::RwLock::new(Vec::new());
static REASONING_EFFORT: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

fn fallback_chain_snapshot() -> Vec<FallbackEntry> {
    FALLBACK_CHAIN.read().map(|c| c.clone()).unwrap_or_default()
}

fn current_reasoning_effort() -> Option<String> {
    REASONING_EFFORT.read().ok().and_then(|e| e.clone())
}

/// Sets the optional fallback chain (empty = disabled, the default). Held in
/// memory only; the frontend pushes it from its saved settings and keys.
#[tauri::command]
pub fn set_llm_fallback_chain(entries: Vec<FallbackEntry>) -> Result<usize, String> {
    let valid: Vec<FallbackEntry> = entries
        .into_iter()
        .filter(|e| LLMProvider::from_str(&e.provider).is_ok() && !e.model.trim().is_empty())
        .collect();
    let n = valid.len();
    *FALLBACK_CHAIN.write().map_err(|e| e.to_string())? = valid;
    info!("LLM fallback chain set ({} entries)", n);
    Ok(n)
}

/// Sets the reasoning effort: "auto" (default, sends nothing), "low",
/// "medium", "high", "xhigh" or "max". Applied only to models that accept it.
#[tauri::command]
pub fn set_llm_reasoning_effort(effort: String) -> Result<(), String> {
    let value = match effort.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => None,
        e @ ("low" | "medium" | "high" | "xhigh" | "max") => Some(e.to_string()),
        other => return Err(format!("Esforço de raciocínio inválido: {other}")),
    };
    *REASONING_EFFORT.write().map_err(|e| e.to_string())? = value;
    Ok(())
}

fn llm_http_failure_log_message(
    provider: &LLMProvider,
    status: reqwest::StatusCode,
    response_bytes: usize,
) -> String {
    format!(
        "LLM API request failed; provider={}, status={}, response_bytes={}",
        provider_name(provider),
        status,
        response_bytes
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{cell::Cell, task::Poll};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
        time::{timeout, Duration},
    };

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];

        loop {
            let bytes_read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(bytes_read, 0, "connection closed before completing request");
            request.extend_from_slice(&buffer[..bytes_read]);

            let Some(headers_end) = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| position + 4)
            else {
                continue;
            };
            let content_length = std::str::from_utf8(&request[..headers_end])
                .unwrap()
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .expect("request should have Content-Length");
            if request.len() >= headers_end + content_length {
                return request;
            }
        }
    }

    fn request_json(request: &[u8]) -> serde_json::Value {
        let headers_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
            .expect("request should contain complete headers");
        serde_json::from_slice(&request[headers_end..]).expect("request body should be valid JSON")
    }

    #[test]
    fn claude_body_has_cache_breakpoints_and_gated_effort() {
        let long = "x".repeat(CLAUDE_CACHE_MIN_CHARS);
        let body = serde_json::to_value(build_claude_body(
            "claude-opus-5-5", "sys", &long, Some("low"),
        ))
        .unwrap();
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["messages"][0]["content"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["output_config"]["effort"], "low");

        let short = serde_json::to_value(build_claude_body(
            "claude-sonnet-4-5-20250929", "sys", "oi", Some("low"),
        ))
        .unwrap();
        assert!(short["messages"][0]["content"][0].get("cache_control").is_none());
        assert!(short.get("output_config").is_none(), "old models reject effort");
        assert!(short.get("temperature").is_none());
    }

    #[test]
    fn friendly_errors_are_ptbr_and_leak_nothing_secret() {
        let m = friendly_http_message("OpenAI", 401, r#"{"error":{"message":"Incorrect API key sk-secret"}}"#);
        assert!(m.contains("Chave de API inválida"));
        assert!(!m.contains("sk-secret"), "401 detail is dropped");
        assert!(friendly_http_message("Claude", 400, r#"{"error":{"message":"Your credit balance is too low"}}"#).contains("Sem crédito"));
        assert!(friendly_http_message("OpenAI", 429, r#"{"error":{"code":"insufficient_quota"}}"#).contains("Sem crédito"));
        assert!(friendly_http_message("Claude", 429, "").contains("Limite de uso"));
        assert!(friendly_http_message("Claude", 529, "").contains("sobrecarregado"));
    }

    #[test]
    fn retry_policy_and_reasoning_helpers() {
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(reqwest::StatusCode::from_u16(529).unwrap()));
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
        let ra = header::HeaderValue::from_static("120");
        assert_eq!(backoff_delay(1, Some(&ra)), MAX_BACKOFF);
        assert_eq!(backoff_delay(1, None), Duration::from_secs(2));
        assert!(openai_supports_reasoning("o4-mini") && openai_supports_reasoning("gpt-5-mini"));
        assert!(!openai_supports_reasoning("gpt-4o"));
        assert!(cloud_rejects_reasoning(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"Unsupported parameter: reasoning_effort"}}"#
        ));
        assert!(is_cloud_provider(&LLMProvider::Claude) && !is_cloud_provider(&LLMProvider::Ollama));
    }

    #[tokio::test]
    async fn fallback_chain_is_off_by_default_and_filters_invalid_entries() {
        assert!(fallback_chain_snapshot().is_empty());
        let n = set_llm_fallback_chain(vec![
            FallbackEntry { provider: "nope".into(), model: "m".into(), api_key: String::new() },
            FallbackEntry { provider: "ollama".into(), model: "llama".into(), api_key: String::new() },
        ])
        .unwrap();
        assert_eq!(n, 1);
        set_llm_fallback_chain(vec![]).unwrap();
        assert!(set_llm_reasoning_effort("bogus".into()).is_err());
        set_llm_reasoning_effort("auto".into()).unwrap();
        assert!(current_reasoning_effort().is_none());
    }

    #[test]
    fn only_ollama_disables_reasoning_and_custom_sampling_is_preserved() {
        for provider in [
            LLMProvider::OpenAI,
            LLMProvider::Groq,
            LLMProvider::OpenRouter,
            LLMProvider::CustomOpenAI,
        ] {
            assert!(build_openai_compat_chat_body(&provider, "model", "sys", "user", None, None, None)
                .get("reasoning_effort")
                .is_none());
        }
        let ollama = build_openai_compat_chat_body(
            &LLMProvider::Ollama, "model", "sys", "user", None, None, None,
        );
        assert_eq!(ollama["reasoning_effort"], "none");
        let custom = build_openai_compat_chat_body(
            &LLMProvider::CustomOpenAI, "model", "sys", "user", Some(12), Some(0.3), Some(0.8),
        );
        assert_eq!(custom["max_tokens"], 12);
        assert_eq!(custom["temperature"].as_f64(), Some(0.3_f32 as f64));
        assert_eq!(custom["top_p"].as_f64(), Some(0.8_f32 as f64));
    }

    #[test]
    fn compatible_reasoning_is_separate_from_visible_content() {
        for message in [
            json!({"reasoning_content": "private"}),
            json!({"content": null, "reasoning_content": "private"}),
        ] {
            let response: ChatResponse = serde_json::from_value(json!({
                "choices": [{"message": message}]
            }))
            .unwrap();
            assert_eq!(
                response.completion().unwrap(),
                LlmCompletion {
                    content: String::new(),
                    reasoning_stripped: true,
                }
            );
        }
    }

    #[test]
    fn legacy_ollama_rejection_requires_compatible_error() {
        assert!(ollama_rejects_reasoning_effort(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"param":"reasoning_effort"}}"#,
        ));
        assert!(ollama_rejects_reasoning_effort(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            r#"{"message":"Unsupported parameter 'ReAsOnInG_EfFoRt'."}"#,
        ));
        assert!(ollama_rejects_reasoning_effort(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"think value \"none\" is not supported for this model"}"#,
        ));
        assert!(!ollama_rejects_reasoning_effort(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"message":"invalid model"}"#,
        ));
        assert!(!ollama_rejects_reasoning_effort(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"param":"reasoning_effort"}}"#,
        ));
        assert!(!ollama_rejects_reasoning_effort(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":"think value \"none\" is not supported"}"#,
        ));
    }

    #[tokio::test]
    async fn custom_provider_failure_keeps_response_body_out_of_log_message() {
        let response_body = b"REMOTE_BODY_SECRET";
        let safe_message = llm_http_failure_log_message(
            &LLMProvider::CustomOpenAI,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            response_body.len(),
        );
        assert_eq!(
            safe_message,
            "LLM API request failed; provider=Custom OpenAI, status=500 Internal Server Error, response_bytes=18"
        );
        assert!(!safe_message.contains("REMOTE_BODY_SECRET"));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _request = read_http_request(&mut stream).await;
            let headers = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(response_body).await.unwrap();
            stream.flush().await.unwrap();
        });

        let endpoint = format!("http://{address}");
        let error = generate_summary(
            &Client::new(),
            &LLMProvider::CustomOpenAI,
            "model",
            "",
            "system",
            "user",
            None,
            Some(&endpoint),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect_err("custom provider failure should preserve its response body");
        server.await.unwrap();

        assert!(error.contains("REMOTE_BODY_SECRET"));
    }

    #[tokio::test]
    async fn cancellation_prevents_compatibility_io_from_being_polled() {
        let cancellation_token = CancellationToken::new();
        cancellation_token.cancel();
        let polled = Cell::new(false);

        let result = await_or_cancel(
            std::future::poll_fn(|_| {
                polled.set(true);
                Poll::<()>::Pending
            }),
            Some(&cancellation_token),
        )
        .await;

        assert_eq!(result, Err("Summary generation was cancelled".to_string()));
        assert!(!polled.get());
    }

    #[tokio::test]
    async fn cancellation_during_failed_legacy_ollama_retry_body_returns_promptly() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (retry_headers_sent, mut retry_headers_ready) = oneshot::channel();
        let (release_retry_body, retry_body_released) = oneshot::channel();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert_eq!(body["reasoning_effort"], "none");

            let rejection_body = br#"{"error":{"param":"reasoning_effort"}}"#;
            let rejection_headers = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                rejection_body.len()
            );
            stream.write_all(rejection_headers.as_bytes()).await.unwrap();
            stream.write_all(rejection_body).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert!(body.get("reasoning_effort").is_none());

            stream
                .write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            stream.flush().await.unwrap();
            let _ = retry_headers_sent.send(());
            let _ = retry_body_released.await;
            let _ = stream.write_all(b"fail").await;
        });

        let client = Client::new();
        let cancellation_token = CancellationToken::new();
        let endpoint = format!("http://{address}");
        let generation = generate_summary(
            &client,
            &LLMProvider::Ollama,
            "model",
            "",
            "system",
            "user",
            Some(&endpoint),
            None,
            None,
            None,
            None,
            None,
            Some(&cancellation_token),
        );
        tokio::pin!(generation);

        tokio::select! {
            biased;
            result = &mut generation => panic!("generation ended before retry headers: {result:?}"),
            _ = &mut retry_headers_ready => {}
        }

        cancellation_token.cancel();
        let completion = timeout(Duration::from_secs(1), &mut generation).await;
        let _ = release_retry_body.send(());
        let released_completion = if completion.is_err() {
            Some(timeout(Duration::from_secs(1), &mut generation).await)
        } else {
            None
        };
        server.await.unwrap();

        let result = completion.unwrap_or_else(|_| {
            released_completion
                .expect("generation should be awaited after releasing the retry body")
                .expect("generation should finish after releasing the retry body")
        });
        assert_eq!(result, Err("Summary generation was cancelled".to_string()));
    }

    #[tokio::test]
    async fn cancellation_during_successful_legacy_ollama_retry_body_returns_promptly() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (retry_headers_sent, mut retry_headers_ready) = oneshot::channel();
        let (release_retry_body, retry_body_released) = oneshot::channel();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert_eq!(body["reasoning_effort"], "none");

            let rejection_body = br#"{"error":{"param":"reasoning_effort"}}"#;
            let rejection_headers = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                rejection_body.len()
            );
            stream.write_all(rejection_headers.as_bytes()).await.unwrap();
            stream.write_all(rejection_body).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert!(body.get("reasoning_effort").is_none());

            let completion_body =
                br#"{"choices":[{"message":{"content":"Meeting summary."}}]}"#;
            let completion_headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                completion_body.len()
            );
            stream.write_all(completion_headers.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = retry_headers_sent.send(());
            let _ = retry_body_released.await;
            let _ = stream.write_all(completion_body).await;
        });

        let client = Client::new();
        let cancellation_token = CancellationToken::new();
        let endpoint = format!("http://{address}");
        let generation = generate_summary(
            &client,
            &LLMProvider::Ollama,
            "model",
            "",
            "system",
            "user",
            Some(&endpoint),
            None,
            None,
            None,
            None,
            None,
            Some(&cancellation_token),
        );
        tokio::pin!(generation);

        tokio::select! {
            biased;
            result = &mut generation => panic!("generation ended before retry headers: {result:?}"),
            _ = &mut retry_headers_ready => {}
        }

        cancellation_token.cancel();
        let completion = timeout(Duration::from_secs(1), &mut generation).await;
        let _ = release_retry_body.send(());
        if completion.is_err() {
            let _ = timeout(Duration::from_secs(1), &mut generation).await;
        }
        server.await.unwrap();

        let result =
            completion.expect("generation should observe cancellation before retry body release");
        assert_eq!(result, Err("Summary generation was cancelled".to_string()));
    }

    #[tokio::test]
    async fn legacy_ollama_retry_without_reasoning_effort_returns_completion() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert_eq!(body["reasoning_effort"], "none");

            let rejection_body = br#"{"error":{"param":"reasoning_effort"}}"#;
            let rejection_headers = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                rejection_body.len()
            );
            stream.write_all(rejection_headers.as_bytes()).await.unwrap();
            stream.write_all(rejection_body).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert!(body.get("reasoning_effort").is_none());

            let completion_body =
                br#"{"choices":[{"message":{"content":"Meeting summary."}}]}"#;
            let completion_headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                completion_body.len()
            );
            stream.write_all(completion_headers.as_bytes()).await.unwrap();
            stream.write_all(completion_body).await.unwrap();
            stream.flush().await.unwrap();
        });

        let client = Client::new();
        let cancellation_token = CancellationToken::new();
        let endpoint = format!("http://{address}");
        let completion = timeout(
            Duration::from_secs(1),
            generate_summary(
                &client,
                &LLMProvider::Ollama,
                "model",
                "",
                "system",
                "user",
                Some(&endpoint),
                None,
                None,
                None,
                None,
                None,
                Some(&cancellation_token),
            ),
        )
        .await;
        server.await.unwrap();

        assert_eq!(
            completion
                .expect("generation should finish after the compatibility retry")
                .unwrap(),
            LlmCompletion {
                content: "Meeting summary.".to_string(),
                reasoning_stripped: false,
            }
        );
    }

    #[test]
    fn claude_response_uses_exact_block_types() {
        let response: ClaudeChatResponse = serde_json::from_value(json!({
            "content": [
                {"type": "analysis", "text": "not visible"},
                {"type": "thinking", "thinking": "private"},
                {"type": "text", "text": "Meeting summary."}
            ]
        }))
        .unwrap();
        assert_eq!(
            response.completion(),
            Some(LlmCompletion {
                content: "Meeting summary.".to_string(),
                reasoning_stripped: true,
            })
        );
    }
}

/// Helper function to get provider name for logging
fn provider_name(provider: &LLMProvider) -> &str {
    match provider {
        LLMProvider::OpenAI => "OpenAI",
        LLMProvider::Claude => "Claude",
        LLMProvider::Groq => "Groq",
        LLMProvider::Ollama => "Ollama",
        LLMProvider::BuiltInAI => "Built-in AI",
        LLMProvider::OpenRouter => "OpenRouter",
        LLMProvider::CustomOpenAI => "Custom OpenAI",
    }
}
