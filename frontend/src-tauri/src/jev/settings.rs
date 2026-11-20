//! Jev connection settings: where the System One endpoint lives, which model
//! to ask and the API key. Stored in its own `jev.json` (tauri-plugin-store),
//! off by default. The key never leaves this module towards the frontend.

use reqwest::Url;
use serde::{Deserialize, Serialize};

pub const STORE_FILE: &str = "jev.json";
pub const STORE_KEY: &str = "settings";
/// TypeSafe's hosted System One API.
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
/// Alias TypeSafe documents for general use.
pub const DEFAULT_MODEL: &str = "jev-latest";
const MAX_MODEL_LEN: usize = 100;
const MAX_KEY_LEN: usize = 512;

/// What is written to `jev.json`.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredSettings {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            api_key: None,
        }
    }
}

// Hand-written so the key can never end up in a log line.
impl std::fmt::Debug for StoredSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSettings")
            .field("enabled", &self.enabled)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl StoredSettings {
    pub fn has_key(&self) -> bool {
        self.api_key.as_deref().is_some_and(|k| !k.is_empty())
    }

    pub fn to_public(&self) -> PublicSettings {
        PublicSettings {
            enabled: self.enabled,
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            has_key: self.has_key(),
            default_base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
        }
    }
}

/// What the settings card sees: everything except the key itself.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
    pub default_base_url: String,
    pub default_model: String,
}

/// A save from the settings card.
#[derive(Debug, Clone, Default)]
pub struct SettingsUpdate {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    /// `None` or blank keeps the stored key.
    pub api_key: Option<String>,
    pub clear_key: bool,
}

/// Accepts `https://` anywhere and plain `http://` only on this machine
/// (a local System One server such as Kev), so the key never travels in clear text.
/// Trailing `/v1`, `/v1/systemone` or `/v1/models` pasted from the docs are dropped.
pub fn normalize_base_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let raw = if trimmed.is_empty() { DEFAULT_BASE_URL } else { trimmed };
    let url = Url::parse(raw).map_err(|_| "Endereço inválido. Use algo como https://api.typesafe.ai".to_string())?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() {
        return Err("Endereço sem servidor.".to_string());
    }
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1");
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        "http" => return Err("Use https://. Endereço http:// só é aceito neste computador (localhost).".to_string()),
        _ => return Err("O endereço precisa começar com https://".to_string()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Não coloque usuário ou senha no endereço; use o campo da chave.".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("O endereço não pode ter parâmetros (?…) nem âncora (#…).".to_string());
    }
    let mut text = url.as_str().trim_end_matches('/').to_string();
    for suffix in ["/v1/systemone", "/v1/models", "/v1"] {
        if let Some(stripped) = text.strip_suffix(suffix) {
            text = stripped.trim_end_matches('/').to_string();
            break;
        }
    }
    Ok(text)
}

pub fn normalize_model(input: &str) -> Result<String, String> {
    let model = input.trim();
    if model.is_empty() {
        return Ok(DEFAULT_MODEL.to_string());
    }
    let valid = model.len() <= MAX_MODEL_LEN
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '/'));
    if valid {
        Ok(model.to_string())
    } else {
        Err("Nome de modelo inválido. Exemplo: jev-latest".to_string())
    }
}

/// Applies a save on top of what is stored. Validation errors come back in pt-BR.
pub fn apply_update(current: &StoredSettings, update: SettingsUpdate) -> Result<StoredSettings, String> {
    let base_url = normalize_base_url(&update.base_url)?;
    let model = normalize_model(&update.model)?;
    let api_key = if update.clear_key {
        None
    } else {
        match update.api_key.as_deref().map(str::trim) {
            Some(key) if !key.is_empty() => {
                if key.len() > MAX_KEY_LEN || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
                    return Err("Chave inválida: confira se colou só a chave, sem espaços.".to_string());
                }
                Some(key.to_string())
            }
            _ => current.api_key.clone(),
        }
    };
    Ok(StoredSettings {
        enabled: update.enabled,
        base_url,
        model,
        api_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jev_defaults_are_off_and_point_to_typesafe() {
        let s = StoredSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.base_url, DEFAULT_BASE_URL);
        assert_eq!(s.model, DEFAULT_MODEL);
        assert!(!s.has_key());
    }

    #[test]
    fn jev_old_or_partial_store_falls_back_to_defaults() {
        let s: StoredSettings = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.base_url, DEFAULT_BASE_URL);
        assert_eq!(s.model, DEFAULT_MODEL);
    }

    #[test]
    fn jev_base_url_rules() {
        assert_eq!(normalize_base_url("").unwrap(), DEFAULT_BASE_URL);
        assert_eq!(normalize_base_url(" https://api.typesafe.ai/ ").unwrap(), DEFAULT_BASE_URL);
        assert_eq!(normalize_base_url("https://api.typesafe.ai/v1/systemone").unwrap(), DEFAULT_BASE_URL);
        assert_eq!(normalize_base_url("https://gw.example.com/api/v1").unwrap(), "https://gw.example.com/api");
        assert_eq!(normalize_base_url("http://localhost:8000").unwrap(), "http://localhost:8000");
        assert_eq!(normalize_base_url("http://127.0.0.1:8000/v1").unwrap(), "http://127.0.0.1:8000");
        assert!(normalize_base_url("http://api.typesafe.ai").is_err());
        assert!(normalize_base_url("ftp://api.typesafe.ai").is_err());
        assert!(normalize_base_url("https://user:pass@api.typesafe.ai").is_err());
        assert!(normalize_base_url("https://api.typesafe.ai/?key=1").is_err());
        assert!(normalize_base_url("não é url").is_err());
    }

    #[test]
    fn jev_model_rules() {
        assert_eq!(normalize_model("  ").unwrap(), DEFAULT_MODEL);
        assert_eq!(normalize_model("jev-1.14.0").unwrap(), "jev-1.14.0");
        assert_eq!(normalize_model("typesafe-ai/jev").unwrap(), "typesafe-ai/jev");
        assert!(normalize_model("jev latest").is_err());
        assert!(normalize_model(&"a".repeat(101)).is_err());
    }

    #[test]
    fn jev_update_keeps_replaces_or_clears_the_key() {
        let current = StoredSettings {
            api_key: Some("old".into()),
            ..StoredSettings::default()
        };
        let base = SettingsUpdate {
            enabled: true,
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
            ..SettingsUpdate::default()
        };
        let kept = apply_update(&current, base.clone()).unwrap();
        assert_eq!(kept.api_key.as_deref(), Some("old"));
        assert!(kept.enabled);

        let blank = apply_update(&current, SettingsUpdate { api_key: Some("   ".into()), ..base.clone() }).unwrap();
        assert_eq!(blank.api_key.as_deref(), Some("old"));

        let replaced = apply_update(&current, SettingsUpdate { api_key: Some(" new ".into()), ..base.clone() }).unwrap();
        assert_eq!(replaced.api_key.as_deref(), Some("new"));

        let cleared = apply_update(&current, SettingsUpdate { clear_key: true, ..base.clone() }).unwrap();
        assert!(cleared.api_key.is_none());

        assert!(apply_update(&current, SettingsUpdate { api_key: Some("a b".into()), ..base }).is_err());
    }

    #[test]
    fn jev_key_is_never_exposed() {
        let s = StoredSettings {
            api_key: Some("sk-secret-value".into()),
            ..StoredSettings::default()
        };
        assert!(!format!("{s:?}").contains("sk-secret-value"));
        let public = serde_json::to_string(&s.to_public()).unwrap();
        assert!(!public.contains("sk-secret-value"));
        assert!(public.contains("\"hasKey\":true"));
    }
}
