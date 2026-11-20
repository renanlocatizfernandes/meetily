use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tauri::command;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    pub context_length: Option<u32>,
    /// USD per token, as a decimal string (OpenRouter's format).
    pub prompt_price: Option<String>,
    pub completion_price: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterApiModel {
    id: String,
    name: Option<String>,
    context_length: Option<u32>,
    #[serde(default)]
    top_provider: Option<TopProvider>,
    #[serde(default)]
    pricing: Option<Pricing>,
}

#[derive(Debug, Deserialize, Default)]
struct TopProvider {
    context_length: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct Pricing {
    prompt: Option<String>,
    completion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    data: Vec<OpenRouterApiModel>,
}

struct CacheEntry {
    models: Vec<OpenRouterModel>,
    fetched_at: Instant,
}

static MODELS_CACHE: RwLock<Option<CacheEntry>> = RwLock::new(None);
const CACHE_TTL: Duration = Duration::from_secs(600);

/// Used when the network is unavailable. Best-effort IDs; the live list
/// replaces them as soon as it can be fetched.
const FALLBACK_MODELS: &[(&str, &str)] = &[
    ("anthropic/claude-opus-5.5", "Anthropic: Claude Opus 5.5"),
    ("anthropic/claude-sonnet-5.5", "Anthropic: Claude Sonnet 5.5"),
    ("anthropic/claude-haiku-5.5", "Anthropic: Claude Haiku 5.5"),
    ("openai/gpt-4o", "OpenAI: GPT-4o"),
    ("openai/gpt-4o-mini", "OpenAI: GPT-4o mini"),
];

fn fallback_models() -> Vec<OpenRouterModel> {
    FALLBACK_MODELS
        .iter()
        .map(|(id, name)| OpenRouterModel {
            id: id.to_string(),
            name: name.to_string(),
            context_length: None,
            prompt_price: None,
            completion_price: None,
        })
        .collect()
}

fn convert(api: OpenRouterResponse) -> Vec<OpenRouterModel> {
    api.data
        .into_iter()
        .map(|m| OpenRouterModel {
            id: m.id,
            name: m.name.unwrap_or_else(|| "Unknown".to_string()),
            context_length: m
                .top_provider
                .as_ref()
                .and_then(|tp| tp.context_length)
                .or(m.context_length),
            prompt_price: m.pricing.as_ref().and_then(|p| p.prompt.clone()),
            completion_price: m.pricing.as_ref().and_then(|p| p.completion.clone()),
        })
        .collect()
}

/// Lists OpenRouter models with context and pricing. Cached for 10 minutes;
/// falls back to an embedded list when the request fails and nothing is cached.
#[command]
pub async fn get_openrouter_models(
    force_refresh: Option<bool>,
) -> Result<Vec<OpenRouterModel>, String> {
    if !force_refresh.unwrap_or(false) {
        if let Ok(cache) = MODELS_CACHE.read() {
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < CACHE_TTL {
                    return Ok(entry.models.clone());
                }
            }
        }
    }

    let fetched = async {
        let response = reqwest::Client::new()
            .get("https://openrouter.ai/api/v1/models")
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("Failed to make HTTP request: {}", e))?;
        if !response.status().is_success() {
            return Err(format!("HTTP request failed with status: {}", response.status()));
        }
        response
            .json::<OpenRouterResponse>()
            .await
            .map_err(|e| format!("Failed to parse JSON response: {}", e))
    }
    .await;

    match fetched {
        Ok(api) => {
            let models = convert(api);
            if models.is_empty() {
                return Ok(stale_or_fallback());
            }
            if let Ok(mut cache) = MODELS_CACHE.write() {
                *cache = Some(CacheEntry { models: models.clone(), fetched_at: Instant::now() });
            }
            Ok(models)
        }
        Err(e) => {
            log::warn!("OpenRouter model list unavailable: {e}. Using cached/default list.");
            Ok(stale_or_fallback())
        }
    }
}

fn stale_or_fallback() -> Vec<OpenRouterModel> {
    MODELS_CACHE
        .read()
        .ok()
        .and_then(|c| c.as_ref().map(|e| e.models.clone()))
        .unwrap_or_else(fallback_models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_pricing_and_prefers_top_provider_context() {
        let api: OpenRouterResponse = serde_json::from_value(serde_json::json!({
            "data": [{
                "id": "vendor/model", "name": "Model", "context_length": 8000,
                "top_provider": {"context_length": 16000},
                "pricing": {"prompt": "0.000001", "completion": "0.000002"}
            }, {"id": "vendor/bare"}]
        }))
        .unwrap();
        let models = convert(api);
        assert_eq!(models[0].context_length, Some(16000));
        assert_eq!(models[0].prompt_price.as_deref(), Some("0.000001"));
        assert_eq!(models[1].name, "Unknown");
        assert!(!fallback_models().is_empty());
    }
}
