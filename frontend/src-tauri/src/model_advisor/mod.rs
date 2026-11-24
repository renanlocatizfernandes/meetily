//! "Modelos recomendados para este Mac": looks at the machine at run time
//! and suggests, for each function (speech, voices, assistant), the best
//! model whose set fits in memory together. Nothing here downloads or
//! switches a model by itself; the settings screen does that on a click,
//! through the commands of each engine.

pub mod catalog;
pub mod hardware;
pub mod ollama_pull;
pub mod recommend;

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::database::repositories::setting::SettingsRepository;

use catalog::{CatalogModel, Engine, Role, CATALOG};
use hardware::Hardware;
use recommend::{Fit, Machine, Tier};

/// A catalog model as the settings screen shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub id: String,
    pub role: Role,
    pub engine: Engine,
    pub name: String,
    pub about: String,
    pub download_bytes: u64,
    /// Estimated memory in use on this machine.
    pub memory_mb: u32,
    pub license: String,
    pub license_note: Option<String>,
    pub source: String,
    pub installed: bool,
    /// In use now for this function (for the assistant: in the summaries).
    pub active: bool,
    pub downloading: bool,
    pub fit: Fit,
}

/// The model in use now for a function, which may be outside the catalog.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentModel {
    /// "localWhisper", "parakeet", "voice", "ollama", or a cloud provider.
    pub provider: String,
    pub model: String,
    /// The catalog name when the model is in the catalog, otherwise `model`.
    pub name: String,
    pub in_catalog: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoleReport {
    pub role: Role,
    /// For the assistant this is the model of the summaries; the live help
    /// keeps its own setting (`get_live_assist_settings`).
    pub current: Option<CurrentModel>,
    pub recommended: Option<ModelView>,
    /// One sentence in Portuguese.
    pub reason: String,
    /// The other catalog models of the function, the ones that fit first.
    pub others: Vec<ModelView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OllamaState {
    pub endpoint: String,
    /// The local Ollama answered.
    pub running: bool,
    pub installed_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorReport {
    pub hardware: Hardware,
    /// Meeting language used for the suggestion ("pt", "en", "auto"…).
    pub language: String,
    pub tier: Tier,
    /// Memory this machine can give to models, in MB.
    pub budget_mb: u32,
    /// Estimated memory of the three suggested models together, in MB.
    pub total_memory_mb: u32,
    /// A voice model is installed, chosen and switched on.
    pub voice_model_enabled: bool,
    pub ollama: OllamaState,
    /// Ollama tag suggested for the live help, which runs during the meeting
    /// next to the speech and voice models. The assistant role is the model
    /// of the summaries, which run after it.
    pub live_help_model: String,
    pub roles: Vec<RoleReport>,
}

/// What is on this machine, gathered before the pure part runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Installed {
    pub whisper: Vec<String>,
    pub parakeet: Vec<String>,
    pub voices: Vec<String>,
    pub ollama: Vec<String>,
    pub downloading: Vec<String>,
    /// (provider, model) of the transcription setting.
    pub speech_current: Option<(String, String)>,
    /// Chosen voice model, when it is installed and switched on.
    pub voice_current: Option<String>,
    /// (provider, model) of the summary setting.
    pub assistant_current: Option<(String, String)>,
}

impl Installed {
    fn has(&self, model: &CatalogModel) -> bool {
        let list = match model.engine {
            Engine::Whisper => &self.whisper,
            Engine::Parakeet => &self.parakeet,
            Engine::Voice => &self.voices,
            Engine::Ollama => &self.ollama,
        };
        list.iter().any(|name| name == model.id)
    }

    fn current(&self, role: Role) -> Option<(String, String)> {
        match role {
            Role::Speech => self.speech_current.clone(),
            Role::Voices => self.voice_current.clone().map(|id| ("voice".to_string(), id)),
            Role::Assistant => self.assistant_current.clone(),
        }
    }
}

fn provider_of(engine: Engine) -> &'static str {
    match engine {
        Engine::Whisper => "localWhisper",
        Engine::Parakeet => "parakeet",
        Engine::Voice => "voice",
        Engine::Ollama => "ollama",
    }
}

/// The report, from facts already gathered. Pure.
pub fn build_report(hardware: Hardware, language: &str, installed: &Installed, ollama: OllamaState) -> AdvisorReport {
    let machine = Machine {
        ram_gb: hardware.ram_gb,
        apple_silicon: hardware.apple_silicon,
        free_disk_bytes: hardware.free_disk_bytes,
    };
    let on_disk: Vec<&str> = CATALOG.iter().filter(|model| installed.has(model)).map(|model| model.id).collect();
    let recommendation = recommend::recommend(&machine, language, &on_disk);
    let view = |model: &CatalogModel, memory_mb: u32, fit: Fit| {
        let current = installed.current(model.role);
        ModelView {
            id: model.id.to_string(),
            role: model.role,
            engine: model.engine,
            name: model.name.to_string(),
            about: model.about.to_string(),
            download_bytes: model.download_bytes,
            memory_mb,
            license: model.license.to_string(),
            license_note: model.license_note.map(str::to_string),
            source: model.source.to_string(),
            installed: installed.has(model),
            active: current.is_some_and(|(provider, id)| provider == provider_of(model.engine) && id == model.id),
            downloading: installed.downloading.iter().any(|name| name == model.id),
            fit,
        }
    };
    let roles = recommendation
        .picks
        .iter()
        .map(|pick| RoleReport {
            role: pick.role,
            current: installed.current(pick.role).map(|(provider, model)| {
                let known = CATALOG
                    .iter()
                    .find(|entry| entry.role == pick.role && entry.id == model && provider_of(entry.engine) == provider);
                CurrentModel {
                    name: known.map(|entry| entry.name.to_string()).unwrap_or_else(|| model.clone()),
                    in_catalog: known.is_some(),
                    provider,
                    model,
                }
            }),
            recommended: pick.model.map(|model| view(model, pick.memory_mb, Fit::Fits)),
            reason: pick.reason.clone(),
            others: pick.others.iter().map(|other| view(other.model, other.memory_mb, other.fit)).collect(),
        })
        .collect();
    AdvisorReport {
        hardware,
        language: language.to_string(),
        tier: recommendation.tier,
        budget_mb: recommendation.budget_mb,
        total_memory_mb: recommendation.total_memory_mb,
        voice_model_enabled: installed.voice_current.is_some(),
        ollama,
        live_help_model: catalog::LIVE_HELP_MODEL.to_string(),
        roles,
    }
}

/// Names of the models the Ollama at `endpoint` has, or `None` when it does
/// not answer. A short wait: the settings screen should not hang on it.
async fn ollama_models(endpoint: &str) -> Option<Vec<String>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().ok()?;
    let response = client.get(format!("{endpoint}/api/tags")).send().await.ok()?;
    let body: serde_json::Value = response.json().await.ok()?;
    Some(
        body.get("models")?
            .as_array()?
            .iter()
            .filter_map(|model| model.get("name").and_then(|name| name.as_str()).map(str::to_string))
            .collect(),
    )
}

/// The machine, what it has installed and in use, and the suggestion for
/// each function. Read-only: it never downloads or switches anything.
#[tauri::command]
pub async fn model_advisor_get_report<R: Runtime>(
    app: AppHandle<R>,
    ollama_endpoint: Option<String>,
    language: Option<String>,
) -> Result<AdvisorReport, String> {
    let models_dir = app.path().app_data_dir().ok().map(|dir| dir.join("models"));
    let disk_probe = models_dir.clone().or_else(|| app.path().app_data_dir().ok());
    let hardware = tokio::task::spawn_blocking(move || {
        // The folder may not exist yet: look at the closest one that does.
        let existing = disk_probe.as_deref().and_then(|path| path.ancestors().find(|ancestor| ancestor.exists()));
        hardware::detect(existing)
    })
    .await
    .map_err(|_| "Não foi possível ler as informações deste computador.".to_string())?;

    let language = language
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(crate::get_language_preference_internal)
        .unwrap_or_else(|| "auto".to_string());

    let mut installed = Installed::default();
    if let Ok(models) = crate::whisper_engine::commands::whisper_get_available_models().await {
        for model in models {
            match model.status {
                crate::whisper_engine::whisper_engine::ModelStatus::Available => installed.whisper.push(model.name),
                crate::whisper_engine::whisper_engine::ModelStatus::Downloading { .. } => installed.downloading.push(model.name),
                _ => {}
            }
        }
    }
    if crate::parakeet_engine::commands::parakeet_init().await.is_ok() {
        if let Ok(models) = crate::parakeet_engine::commands::parakeet_get_available_models().await {
            for model in models {
                match model.status {
                    crate::parakeet_engine::ModelStatus::Available => installed.parakeet.push(model.name),
                    crate::parakeet_engine::ModelStatus::Downloading { .. } => installed.downloading.push(model.name),
                    _ => {}
                }
            }
        }
    }
    let (voices, voice_enabled) = crate::speakers::neural::overview(&app).await;
    for voice in &voices {
        if voice.installed {
            installed.voices.push(voice.id.clone());
        }
        if voice.downloading {
            installed.downloading.push(voice.id.clone());
        }
        if voice.selected && voice.installed && voice_enabled {
            installed.voice_current = Some(voice.id.clone());
        }
    }

    let mut saved_endpoint = None;
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        let pool = state.db_manager.pool();
        if let Ok(Some(config)) = SettingsRepository::get_transcript_config(pool).await {
            installed.speech_current = Some((config.provider, config.model));
        }
        if let Ok(Some(config)) = SettingsRepository::get_model_config(pool).await {
            saved_endpoint = config.ollama_endpoint.clone();
            installed.assistant_current = Some((config.provider, config.model));
        }
    }
    let endpoint = ollama_pull::endpoint_or_default(ollama_endpoint.as_deref().or(saved_endpoint.as_deref()))
        .unwrap_or_else(|_| ollama_pull::DEFAULT_ENDPOINT.to_string());
    let ollama_installed = ollama_models(&endpoint).await;
    let ollama = OllamaState {
        endpoint,
        running: ollama_installed.is_some(),
        installed_models: ollama_installed.clone().unwrap_or_default(),
    };
    installed.ollama = ollama_installed.unwrap_or_default();
    for model in CATALOG.iter().filter(|model| model.role == Role::Assistant) {
        if ollama_pull::is_pulling(model.id) {
            installed.downloading.push(model.id.to_string());
        }
    }

    Ok(build_report(hardware, &language, &installed, ollama))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware(ram_gb: u32) -> Hardware {
        Hardware {
            ram_gb,
            total_memory_bytes: ram_gb as u64 * 1024 * 1024 * 1024,
            chip: "Apple M2".to_string(),
            apple_silicon: true,
            cpu_cores: 8,
            free_disk_bytes: Some(100_000_000_000),
        }
    }

    fn ollama(installed: &[&str]) -> OllamaState {
        OllamaState {
            endpoint: ollama_pull::DEFAULT_ENDPOINT.to_string(),
            running: true,
            installed_models: installed.iter().map(|name| name.to_string()).collect(),
        }
    }

    #[test]
    fn the_report_marks_what_is_installed_and_in_use() {
        let installed = Installed {
            parakeet: vec!["parakeet-tdt-0.6b-v3-int8".to_string()],
            voices: vec!["campplus-zh-en".to_string()],
            ollama: vec!["llama3.2:latest".to_string()],
            downloading: vec!["redimnet2-b6".to_string()],
            speech_current: Some(("parakeet".to_string(), "parakeet-tdt-0.6b-v3-int8".to_string())),
            voice_current: Some("campplus-zh-en".to_string()),
            assistant_current: Some(("claude".to_string(), "claude-sonnet".to_string())),
            ..Installed::default()
        };
        let report = build_report(hardware(16), "pt-BR", &installed, ollama(&["llama3.2:latest"]));
        assert_eq!(report.roles.len(), 3);
        assert!(report.voice_model_enabled);

        let speech = &report.roles[0];
        let recommended = speech.recommended.as_ref().unwrap();
        assert_eq!(recommended.id, "parakeet-tdt-0.6b-v3-int8");
        assert!(recommended.installed && recommended.active);
        assert_eq!(speech.current.as_ref().unwrap().name, "Parakeet v3 multilíngue int8");
        assert!(speech.current.as_ref().unwrap().in_catalog);

        let voices = &report.roles[1];
        let recommended = voices.recommended.as_ref().unwrap();
        assert_eq!(recommended.id, "redimnet2-b6");
        assert!(!recommended.installed && !recommended.active && recommended.downloading);
        assert!(voices.others.iter().any(|other| other.id == "campplus-zh-en" && other.active));

        let assistant = &report.roles[2];
        let current = assistant.current.as_ref().unwrap();
        assert_eq!((current.provider.as_str(), current.in_catalog), ("claude", false));
        assert_eq!(current.name, "claude-sonnet");
        assert!(!assistant.recommended.as_ref().unwrap().active);
        assert!(assistant.others.iter().any(|other| other.id == "llama3.2:latest" && other.installed));
    }

    #[test]
    fn a_whisper_model_and_a_parakeet_model_with_the_same_name_are_not_confused() {
        let installed = Installed {
            speech_current: Some(("localWhisper".to_string(), "parakeet-tdt-0.6b-v3-int8".to_string())),
            ..Installed::default()
        };
        let report = build_report(hardware(8), "pt", &installed, ollama(&[]));
        assert!(!report.roles[0].recommended.as_ref().unwrap().active);
        assert!(!report.roles[0].current.as_ref().unwrap().in_catalog);
        assert!(!report.voice_model_enabled);
    }

    #[test]
    fn the_report_reaches_the_frontend_in_camel_case() {
        let report = build_report(hardware(8), "pt", &Installed::default(), ollama(&[]));
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["hardware"]["ramGb"], 8);
        assert_eq!(json["hardware"]["appleSilicon"], true);
        assert_eq!(json["tier"], "light");
        assert_eq!(json["liveHelpModel"], "llama3.2:latest");
        assert_eq!(json["budgetMb"], 4_500);
        assert_eq!(json["ollama"]["installedModels"].as_array().unwrap().len(), 0);
        assert_eq!(json["roles"][0]["role"], "speech");
        assert_eq!(json["roles"][1]["role"], "voices");
        assert_eq!(json["roles"][2]["role"], "assistant");
        let recommended = &json["roles"][2]["recommended"];
        assert_eq!(recommended["id"], "llama3.2:latest");
        assert_eq!(recommended["engine"], "ollama");
        assert_eq!(recommended["fit"], "fits");
        assert!(recommended["downloadBytes"].as_u64().unwrap() > 2_000_000_000);
        assert!(json["roles"][0]["others"].as_array().unwrap().iter().any(|other| other["fit"] == "tooLittleMemory"));
    }
}
