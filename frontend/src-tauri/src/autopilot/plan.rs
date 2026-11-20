//! Which provider/model the autopilot uses. Pure: the caller supplies what it
//! found on this machine.

use super::model::AutopilotStatus;

/// The summary provider configuration, as saved by the user.
#[derive(Debug, Clone, Default)]
pub struct Configured {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub ollama_endpoint: Option<String>,
    pub custom_endpoint: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}

/// A local model found on this machine, used when the paid provider has no key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalChoice {
    /// "ollama" or "builtin-ai".
    pub provider: String,
    pub model: String,
    pub ollama_endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub ollama_endpoint: Option<String>,
    pub custom_endpoint: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    /// The text leaves this machine (and costs credits).
    pub paid: bool,
    pub fallback: bool,
}

pub fn is_local_provider(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "ollama" | "builtin-ai" | "local-llama" | "localllama"
    )
}

pub fn is_paid(provider: &str) -> bool {
    !is_local_provider(provider)
}

pub fn has_credentials(configured: &Configured) -> bool {
    match configured.provider.trim().to_ascii_lowercase().as_str() {
        "custom-openai" => configured
            .custom_endpoint
            .as_deref()
            .map_or(false, |endpoint| !endpoint.trim().is_empty()),
        provider if is_local_provider(provider) => true,
        _ => !configured.api_key.trim().is_empty(),
    }
}

/// `Err` carries the discreet message shown to the user when nothing can run.
pub fn plan_target(configured: &Configured, local: Option<&LocalChoice>) -> Result<Plan, String> {
    let provider = configured.provider.trim().to_ascii_lowercase();
    if has_credentials(configured) {
        if configured.model.trim().is_empty() {
            return Err("Nenhum modelo de resumo configurado.".to_string());
        }
        return Ok(Plan {
            paid: is_paid(&provider),
            fallback: false,
            provider,
            model: configured.model.trim().to_string(),
            api_key: configured.api_key.clone(),
            ollama_endpoint: configured.ollama_endpoint.clone(),
            custom_endpoint: configured.custom_endpoint.clone(),
            max_tokens: configured.max_tokens,
            temperature: configured.temperature,
            top_p: configured.top_p,
        });
    }
    match local {
        Some(choice) => Ok(Plan {
            provider: choice.provider.clone(),
            model: choice.model.clone(),
            api_key: String::new(),
            ollama_endpoint: choice.ollama_endpoint.clone(),
            custom_endpoint: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            paid: false,
            fallback: true,
        }),
        None => Err("Piloto automático não rodou: o provedor de resumo está sem chave e não há modelo local disponível.".to_string()),
    }
}

pub fn describe(configured: &Configured, local: Option<&LocalChoice>) -> AutopilotStatus {
    let paid = is_paid(&configured.provider);
    let has = has_credentials(configured);
    let plan = plan_target(configured, local);
    let (will_use, provider, model, message) = match &plan {
        Ok(plan) if plan.fallback => (
            "local_fallback",
            Some(plan.provider.clone()),
            Some(plan.model.clone()),
            Some("O provedor de resumo está sem chave; o piloto usará o modelo local.".to_string()),
        ),
        Ok(plan) => ("configured", Some(plan.provider.clone()), Some(plan.model.clone()), None),
        Err(message) => ("none", None, None, Some(message.clone())),
    };
    AutopilotStatus {
        provider: configured.provider.clone(),
        model: configured.model.clone(),
        paid,
        has_credentials: has,
        will_use: will_use.to_string(),
        will_use_provider: provider,
        will_use_model: model,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(provider: &str, key: &str) -> Configured {
        Configured {
            provider: provider.to_string(),
            model: "m1".to_string(),
            api_key: key.to_string(),
            ..Configured::default()
        }
    }

    fn local() -> LocalChoice {
        LocalChoice {
            provider: "ollama".to_string(),
            model: "llama3.2:latest".to_string(),
            ollama_endpoint: None,
        }
    }

    #[test]
    fn local_provider_is_free_and_used_as_configured() {
        let plan = plan_target(&cfg("ollama", ""), None).unwrap();
        assert!(!plan.paid && !plan.fallback);
        assert_eq!(plan.model, "m1");
    }

    #[test]
    fn paid_provider_with_key_is_flagged_paid() {
        let plan = plan_target(&cfg("openai", "sk-x"), Some(&local())).unwrap();
        assert!(plan.paid && !plan.fallback);
        assert_eq!(plan.provider, "openai");
    }

    #[test]
    fn paid_provider_without_key_falls_back_to_local() {
        let plan = plan_target(&cfg("claude", "  "), Some(&local())).unwrap();
        assert!(!plan.paid && plan.fallback);
        assert_eq!(plan.provider, "ollama");
        assert_eq!(plan.model, "llama3.2:latest");
        assert!(plan.api_key.is_empty());
    }

    #[test]
    fn paid_provider_without_key_and_without_local_does_nothing() {
        assert!(plan_target(&cfg("groq", ""), None).is_err());
    }

    #[test]
    fn custom_openai_needs_an_endpoint_not_a_key() {
        let mut custom = cfg("custom-openai", "");
        assert!(plan_target(&custom, None).is_err());
        custom.custom_endpoint = Some("http://localhost:8080/v1".to_string());
        let plan = plan_target(&custom, None).unwrap();
        assert!(plan.paid);
    }

    #[test]
    fn empty_model_is_reported() {
        let mut config = cfg("ollama", "");
        config.model = " ".to_string();
        assert!(plan_target(&config, None).is_err());
    }

    #[test]
    fn describe_reports_cost_and_fallback() {
        let paid = describe(&cfg("openai", "k"), None);
        assert!(paid.paid && paid.will_use == "configured");
        let fallback = describe(&cfg("openai", ""), Some(&local()));
        assert!(fallback.paid && !fallback.has_credentials && fallback.will_use == "local_fallback");
        let none = describe(&cfg("openai", ""), None);
        assert_eq!(none.will_use, "none");
        assert!(none.message.is_some());
    }
}
