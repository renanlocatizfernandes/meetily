use crate::summary::LLMProvider;

pub const DEFAULT_MODEL: &str = "llama3.2:latest";

#[derive(Debug, Clone)]
pub struct SummaryHint {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub ollama_endpoint: Option<String>,
    pub custom_endpoint: Option<String>,
    pub custom_api_key: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}

impl Default for SummaryHint {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: DEFAULT_MODEL.to_string(),
            api_key: String::new(),
            ollama_endpoint: None,
            custom_endpoint: None,
            custom_api_key: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmTarget {
    pub provider: LLMProvider,
    pub model: String,
    pub api_key: String,
    pub ollama_endpoint: Option<String>,
    pub custom_openai_endpoint: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub leaves_machine: bool,
}

pub fn provider_is_cloud(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "openai" | "claude" | "groq" | "openrouter" | "custom-openai"
    )
}

/// Live Assist stays on local Ollama unless the saved summary provider is a cloud one.
/// The default Ollama tag is not forwarded to a cloud provider; that call uses the
/// summary model instead, until the live-assist model field is set to something else.
pub fn resolve_target(hint: &SummaryHint, live_model: &str) -> Result<LlmTarget, String> {
    let cloud = provider_is_cloud(&hint.provider);
    let live = live_model.trim();
    let model = if live.is_empty() || (cloud && live == DEFAULT_MODEL) {
        let summary_model = hint.model.trim();
        if summary_model.is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            summary_model.to_string()
        }
    } else {
        live.to_string()
    };

    if cloud {
        let provider = LLMProvider::from_str(&hint.provider)?;
        let (api_key, custom_openai_endpoint, max_tokens, temperature, top_p) =
            if provider == LLMProvider::CustomOpenAI {
                (
                    hint.custom_api_key.clone().unwrap_or_default(),
                    hint.custom_endpoint.clone(),
                    hint.max_tokens,
                    hint.temperature,
                    hint.top_p,
                )
            } else {
                (hint.api_key.clone(), None, None, None, None)
            };
        return Ok(LlmTarget {
            provider,
            model,
            api_key,
            ollama_endpoint: None,
            custom_openai_endpoint,
            max_tokens,
            temperature,
            top_p,
            leaves_machine: true,
        });
    }

    Ok(LlmTarget {
        provider: LLMProvider::Ollama,
        model,
        api_key: String::new(),
        ollama_endpoint: hint.ollama_endpoint.clone(),
        custom_openai_endpoint: None,
        max_tokens: None,
        temperature: None,
        top_p: None,
        leaves_machine: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_local_ollama() {
        let target = resolve_target(&SummaryHint::default(), "").unwrap();
        assert_eq!(target.provider, LLMProvider::Ollama);
        assert_eq!(target.model, DEFAULT_MODEL);
        assert!(!target.leaves_machine);
    }

    #[test]
    fn live_model_overrides_ollama() {
        let target = resolve_target(&SummaryHint::default(), "qwen2.5:3b").unwrap();
        assert_eq!(target.model, "qwen2.5:3b");
        assert!(!target.leaves_machine);
    }

    #[test]
    fn cloud_summary_provider_is_used_with_its_own_model_by_default() {
        let hint = SummaryHint {
            provider: "openai".to_string(),
            model: "gpt-4.1-mini".to_string(),
            api_key: "test-key".to_string(),
            ..SummaryHint::default()
        };
        let target = resolve_target(&hint, DEFAULT_MODEL).unwrap();
        assert_eq!(target.provider, LLMProvider::OpenAI);
        assert_eq!(target.model, "gpt-4.1-mini");
        assert!(target.leaves_machine);
        assert_eq!(target.api_key, "test-key");
    }
}
