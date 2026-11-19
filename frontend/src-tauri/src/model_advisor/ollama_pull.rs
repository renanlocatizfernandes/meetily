//! Downloads an Ollama model through the local Ollama HTTP API (`/api/pull`),
//! with progress and cancel. Nothing here talks to a paid service: Ollama
//! fetches the files from its public registry and checks their digests.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio_util::sync::CancellationToken;

use super::catalog::{find, Role};

pub const PROGRESS_EVENT: &str = "model-advisor-ollama-progress";
pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
/// No data for this long ends the download with an error.
const STALLED_AFTER: Duration = Duration::from_secs(120);

static ACTIVE: Mutex<Option<HashMap<String, CancellationToken>>> = Mutex::new(None);

fn active() -> std::sync::MutexGuard<'static, Option<HashMap<String, CancellationToken>>> {
    ACTIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn is_pulling(model: &str) -> bool {
    active().as_ref().is_some_and(|map| map.contains_key(model))
}

/// Payload of `model-advisor-ollama-progress`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub model_name: String,
    /// 0 to 100.
    pub progress: u8,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    /// "downloading", "verifying", "done", "cancelled" or "error".
    pub phase: String,
}

/// What the command resolves to.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcome {
    pub model_name: String,
    pub installed: bool,
    pub cancelled: bool,
}

/// The endpoint to use, or an error in Portuguese. Only `http(s)://` URLs.
pub fn endpoint_or_default(endpoint: Option<&str>) -> Result<String, String> {
    let endpoint = endpoint.map(str::trim).filter(|value| !value.is_empty()).unwrap_or(DEFAULT_ENDPOINT);
    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        return Err("O endereço do Ollama precisa começar com http:// ou https://.".to_string());
    }
    Ok(endpoint.trim_end_matches('/').to_string())
}

/// Adds up the layers of a pull. Ollama reports each layer by digest, with
/// its own total, and repeats lines; the sum never goes back.
#[derive(Default)]
pub struct PullMeter {
    layers: HashMap<String, (u64, u64)>,
    best: u64,
}

#[derive(Debug, PartialEq)]
pub enum PullLine {
    /// Bytes done so far and the phase to show.
    Progress { completed: u64, phase: &'static str },
    Success,
    Failed(String),
    Ignored,
}

impl PullMeter {
    pub fn read(&mut self, line: &str) -> PullLine {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            return PullLine::Ignored;
        };
        if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
            return PullLine::Failed(error.to_string());
        }
        let status = value.get("status").and_then(|status| status.as_str()).unwrap_or_default();
        if status == "success" {
            return PullLine::Success;
        }
        if let (Some(digest), Some(total)) = (
            value.get("digest").and_then(|digest| digest.as_str()),
            value.get("total").and_then(|total| total.as_u64()),
        ) {
            let completed = value.get("completed").and_then(|completed| completed.as_u64()).unwrap_or(0).min(total);
            let layer = self.layers.entry(digest.to_string()).or_insert((0, total));
            layer.0 = layer.0.max(completed);
            layer.1 = total;
        }
        let done: u64 = self.layers.values().map(|(completed, _)| *completed).sum();
        self.best = self.best.max(done);
        let phase = if status.starts_with("pulling") || status.is_empty() { "downloading" } else { "verifying" };
        PullLine::Progress { completed: self.best, phase }
    }
}

fn percent(completed: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    // 100 is only sent with "done".
    ((completed.min(total) as f64 / total as f64) * 100.0).floor().min(99.0) as u8
}

enum PullError {
    Cancelled,
    Failed(String),
}

/// `emit` receives the bytes done, the percentage and the phase.
async fn pull(
    endpoint: &str,
    model: &str,
    total_bytes: u64,
    cancel: &CancellationToken,
    emit: impl Fn(u64, u8, &str),
) -> Result<(), PullError> {
    emit(0, 0, "downloading");

    // No overall timeout: a large model takes as long as the connection needs.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| PullError::Failed(error.to_string()))?;
    let request = client
        .post(format!("{endpoint}/api/pull"))
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send();
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(PullError::Cancelled),
        response = request => response.map_err(|error| {
            if error.is_connect() {
                PullError::Failed("Não foi possível falar com o Ollama. Abra o Ollama e tente de novo.".to_string())
            } else {
                PullError::Failed("O Ollama não respondeu ao pedido de download.".to_string())
            }
        })?,
    };
    if !response.status().is_success() {
        return Err(PullError::Failed(format!("O Ollama recusou o download (HTTP {}).", response.status().as_u16())));
    }

    let mut stream = response.bytes_stream();
    let mut meter = PullMeter::default();
    let mut pending = Vec::<u8>::new();
    let mut last_sent = (0u8, "downloading");
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Err(PullError::Cancelled),
            chunk = tokio::time::timeout(STALLED_AFTER, stream.next()) => chunk,
        };
        let chunk = match chunk {
            Err(_) => return Err(PullError::Failed("O download parou de receber dados. Tente de novo: o Ollama continua de onde parou.".to_string())),
            Ok(None) => break,
            Ok(Some(Err(_))) => return Err(PullError::Failed("A conexão com o Ollama caiu durante o download. Tente de novo: o Ollama continua de onde parou.".to_string())),
            Ok(Some(Ok(chunk))) => chunk,
        };
        pending.extend_from_slice(&chunk);
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            match meter.read(&String::from_utf8_lossy(&line)) {
                PullLine::Success => return Ok(()),
                PullLine::Failed(message) => {
                    log::warn!("Ollama pull of {model} failed: {message}");
                    return Err(PullError::Failed("O Ollama não conseguiu baixar este modelo. Atualize o Ollama e tente de novo.".to_string()));
                }
                PullLine::Progress { completed, phase } => {
                    let now = (percent(completed, total_bytes), phase);
                    if now != last_sent {
                        last_sent = now;
                        emit(completed, now.0, phase);
                    }
                }
                PullLine::Ignored => {}
            }
        }
    }
    // The stream ended without "success".
    match meter.read(&String::from_utf8_lossy(&pending)) {
        PullLine::Success => Ok(()),
        _ => Err(PullError::Failed("O Ollama encerrou o download antes do fim. Tente de novo.".to_string())),
    }
}

/// Pulls one of the catalog's Ollama models. Resolves when the pull ends;
/// cancelling is not an error (`cancelled: true`). The model is not put in
/// use: the caller does that after the user's click.
#[tauri::command]
pub async fn model_advisor_pull_ollama_model<R: Runtime>(
    app: AppHandle<R>,
    model_name: String,
    endpoint: Option<String>,
) -> Result<PullOutcome, String> {
    let model = find(Role::Assistant, model_name.trim()).ok_or("Este modelo não está na lista de modelos recomendados.")?;
    let endpoint = endpoint_or_default(endpoint.as_deref())?;
    let cancel = CancellationToken::new();
    {
        let mut guard = active();
        let map = guard.get_or_insert_with(HashMap::new);
        if map.contains_key(model.id) {
            return Err("Este modelo já está sendo baixado.".to_string());
        }
        map.insert(model.id.to_string(), cancel.clone());
    }
    let progress_app = app.clone();
    let result = pull(&endpoint, model.id, model.download_bytes, &cancel, |completed, progress, phase| {
        let _ = progress_app.emit(
            PROGRESS_EVENT,
            PullProgress {
                model_name: model.id.to_string(),
                progress,
                completed_bytes: completed,
                total_bytes: model.download_bytes,
                phase: phase.to_string(),
            },
        );
    })
    .await;
    if let Some(map) = active().as_mut() {
        map.remove(model.id);
    }
    let finish = |progress: u8, phase: &str| {
        let _ = app.emit(
            PROGRESS_EVENT,
            PullProgress {
                model_name: model.id.to_string(),
                progress,
                completed_bytes: if progress == 100 { model.download_bytes } else { 0 },
                total_bytes: model.download_bytes,
                phase: phase.to_string(),
            },
        );
    };
    match result {
        Ok(()) => {
            log::info!("Ollama model {} pulled", model.id);
            finish(100, "done");
            Ok(PullOutcome { model_name: model.id.to_string(), installed: true, cancelled: false })
        }
        Err(PullError::Cancelled) => {
            log::info!("Ollama pull of {} cancelled", model.id);
            finish(0, "cancelled");
            Ok(PullOutcome { model_name: model.id.to_string(), installed: false, cancelled: true })
        }
        Err(PullError::Failed(message)) => {
            finish(0, "error");
            Err(message)
        }
    }
}

/// `true` when a download of that model was running. What Ollama already
/// received stays in its own storage and is reused by the next attempt.
#[tauri::command]
pub async fn model_advisor_cancel_ollama_pull(model_name: String) -> Result<bool, String> {
    let token = active().as_ref().and_then(|map| map.get(model_name.trim()).cloned());
    match token {
        Some(token) => {
            token.cancel();
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_are_added_up_and_the_sum_never_goes_back() {
        let mut meter = PullMeter::default();
        assert_eq!(meter.read(r#"{"status":"pulling manifest"}"#), PullLine::Progress { completed: 0, phase: "downloading" });
        assert_eq!(
            meter.read(r#"{"status":"pulling aaa","digest":"sha256:aaa","total":1000,"completed":400}"#),
            PullLine::Progress { completed: 400, phase: "downloading" }
        );
        assert_eq!(
            meter.read(r#"{"status":"pulling bbb","digest":"sha256:bbb","total":200,"completed":200}"#),
            PullLine::Progress { completed: 600, phase: "downloading" }
        );
        // A repeated, older line of the first layer does not lower the sum.
        assert_eq!(
            meter.read(r#"{"status":"pulling aaa","digest":"sha256:aaa","total":1000,"completed":300}"#),
            PullLine::Progress { completed: 600, phase: "downloading" }
        );
        // A layer without "completed" yet, and one that reports more than its total.
        assert_eq!(
            meter.read(r#"{"status":"pulling ccc","digest":"sha256:ccc","total":50}"#),
            PullLine::Progress { completed: 600, phase: "downloading" }
        );
        assert_eq!(
            meter.read(r#"{"status":"pulling ccc","digest":"sha256:ccc","total":50,"completed":80}"#),
            PullLine::Progress { completed: 650, phase: "downloading" }
        );
        assert_eq!(meter.read(r#"{"status":"verifying sha256 digest"}"#), PullLine::Progress { completed: 650, phase: "verifying" });
        assert_eq!(meter.read(r#"{"status":"success"}"#), PullLine::Success);
    }

    #[test]
    fn errors_and_noise_are_told_apart() {
        let mut meter = PullMeter::default();
        assert_eq!(meter.read(r#"{"error":"pull model manifest: file does not exist"}"#), PullLine::Failed("pull model manifest: file does not exist".to_string()));
        assert_eq!(meter.read("not json"), PullLine::Ignored);
        assert_eq!(meter.read(""), PullLine::Ignored);
    }

    #[test]
    fn one_hundred_percent_is_kept_for_the_end() {
        assert_eq!(percent(0, 1_000), 0);
        assert_eq!(percent(500, 1_000), 50);
        assert_eq!(percent(1_000, 1_000), 99);
        assert_eq!(percent(5_000, 1_000), 99);
        assert_eq!(percent(10, 0), 0);
    }

    #[test]
    fn only_http_endpoints_are_accepted() {
        assert_eq!(endpoint_or_default(None).unwrap(), DEFAULT_ENDPOINT);
        assert_eq!(endpoint_or_default(Some("  ")).unwrap(), DEFAULT_ENDPOINT);
        assert_eq!(endpoint_or_default(Some("http://127.0.0.1:11434/")).unwrap(), "http://127.0.0.1:11434");
        assert!(endpoint_or_default(Some("file:///etc")).is_err());
    }

    /// Talks to the Ollama of this machine. Not part of the normal test run.
    /// With a model that is already installed nothing is downloaded: Ollama
    /// only checks the manifest. A name that does not exist must fail.
    ///
    /// ```text
    /// MEETILY_OLLAMA_INSTALLED=llama3.2:latest \
    /// cargo test --features metal --lib model_advisor::ollama_pull::tests::real_ollama -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore]
    async fn real_ollama_reports_progress_and_errors() {
        let model = std::env::var("MEETILY_OLLAMA_INSTALLED").expect("set MEETILY_OLLAMA_INSTALLED");
        let seen = std::sync::Mutex::new(Vec::new());
        let result = pull(DEFAULT_ENDPOINT, &model, 2_019_392_628, &CancellationToken::new(), |completed, progress, phase| {
            seen.lock().unwrap().push((completed, progress, phase.to_string()));
        })
        .await;
        let seen = seen.into_inner().unwrap();
        println!("{seen:?}");
        assert!(result.is_ok());
        assert_eq!(seen.first().map(|event| event.1), Some(0));
        assert!(seen.iter().any(|event| event.1 == 99));
        assert!(seen.windows(2).all(|pair| pair[0].0 <= pair[1].0));

        let missing = pull(DEFAULT_ENDPOINT, "meetily-does-not-exist:1b", 1, &CancellationToken::new(), |_, _, _| {}).await;
        assert!(matches!(missing, Err(PullError::Failed(_))));

        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = pull(DEFAULT_ENDPOINT, &model, 1, &cancel, |_, _, _| {}).await;
        assert!(matches!(cancelled, Err(PullError::Cancelled)));

        let offline = pull("http://127.0.0.1:9", &model, 1, &CancellationToken::new(), |_, _, _| {}).await;
        assert!(matches!(offline, Err(PullError::Failed(message)) if message.contains("Ollama")));
    }

    #[tokio::test]
    async fn cancelling_a_model_that_is_not_downloading_is_harmless() {
        assert_eq!(model_advisor_cancel_ollama_pull("gemma4:e4b".to_string()).await, Ok(false));
        assert!(!is_pulling("gemma4:e4b"));
    }
}
