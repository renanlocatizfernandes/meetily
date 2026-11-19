//! Latency benchmark for the local Ollama models, on the same streaming path
//! the live suggestions use. One model at a time, never during a recording.

use std::sync::Mutex;
use std::time::Duration;

use log::{debug, info};
use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tokio_util::sync::CancellationToken;

use crate::audio::recording_commands;
use crate::state::AppState;

use super::prompt::{system_prompt, user_prompt};
use super::service::load_hint;
use super::stream::{stream_ollama_chat, StreamError, StreamLimits, StreamTimings};
use super::window::{TranscriptPiece, TranscriptWindow};

pub const PROGRESS_EVENT: &str = "live-assist-benchmark-progress";
/// Measured calls per model, after one warm-up call that loads the model.
const MEASURED_RUNS: usize = 2;
/// The warm-up includes loading the model into memory.
const WARMUP_LIMIT: Duration = Duration::from_secs(120);
const MEASURED_LIMIT: Duration = Duration::from_secs(60);
const MAX_MODELS: usize = 24;

const MSG_RECORDING: &str = "Não é possível medir a latência durante uma gravação. Pare a gravação e tente de novo.";
const MSG_CANCELLED: &str = "Medição cancelada.";
const MSG_INTERRUPTED: &str = "Medição interrompida porque uma gravação começou.";

static RUNNING: Mutex<Option<CancellationToken>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResult {
    pub model: String,
    pub first_token_ms: Option<u64>,
    pub total_ms: Option<u64>,
    pub ok: bool,
    pub error: Option<String>,
}

impl BenchmarkResult {
    fn failed(model: &str, error: &str) -> Self {
        Self {
            model: model.to_string(),
            first_token_ms: None,
            total_ms: None,
            ok: false,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkProgress {
    model: String,
    /// Zero-based position of the model that is starting now.
    index: usize,
    total: usize,
}

/// Clears the running slot when the command returns, whatever the path.
struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = RUNNING.lock() {
            *slot = None;
        }
    }
}

/// Stops a benchmark in progress. Returns whether one was running.
pub(crate) fn cancel_running() -> bool {
    let token = RUNNING.lock().ok().and_then(|slot| slot.clone());
    match token {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

pub(crate) fn median_ms(samples: &[Duration]) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut millis: Vec<u128> = samples.iter().map(Duration::as_millis).collect();
    millis.sort_unstable();
    let middle = millis.len() / 2;
    let median = if millis.len() % 2 == 0 {
        (millis[middle - 1] + millis[middle]) / 2
    } else {
        millis[middle]
    };
    Some(u64::try_from(median).unwrap_or(u64::MAX))
}

/// Trims, drops blanks and repeats, keeps the given order.
pub(crate) fn clean_models(models: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for model in models {
        let name: String = model.trim().chars().take(120).collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        if out.len() == MAX_MODELS {
            break;
        }
    }
    out
}

/// A short fixed meeting, shaped like a real live assist request.
pub(crate) fn benchmark_prompt() -> String {
    let mut window = TranscriptWindow::default();
    let lines = [
        ("Você", "Bom dia. Vamos revisar o cronograma da migração do sistema de pedidos."),
        ("Pessoa 1", "A homologação termina na sexta, mas o SLA do fornecedor ainda não foi assinado."),
        ("Você", "Entendi. O contrato prevê multa se a entrega atrasar?"),
        ("Pessoa 1", "Prevê. Qual é a data limite para a gente aprovar o orçamento extra?"),
    ];
    for (index, (source, text)) in lines.iter().enumerate() {
        window.push(TranscriptPiece {
            at: index as f64 * 6.0,
            source: (*source).to_string(),
            text: (*text).to_string(),
        });
    }
    user_prompt(
        "pt-BR",
        "Reunião semanal do projeto de migração do sistema de pedidos com o fornecedor.",
        // Same shape and size as the automatic context of a real call, so the
        // measured latency includes it.
        "Dados desta reunião:\nCliente: Fornecedor Exemplo | Projeto: Migração de pedidos | Quem fala: Você, Pessoa 1\n\nPerfil do dono:\nQuem sou: gerente do projeto\nEmpresa: empresa de varejo\n\nGlossário:\n- ERP: sistema de gestão integrado\n\nReuniões anteriores com Fornecedor Exemplo:\n- [reunião de 05/11] Pendência: aprovar o orçamento extra da migração\n- [reunião de 05/11] Decisão: entregar a primeira fase em dezembro",
        &window,
        &[],
        "",
    )
}

fn describe(error: &StreamError, interrupted: bool) -> String {
    match error {
        StreamError::Cancelled if interrupted => MSG_INTERRUPTED.to_string(),
        StreamError::Cancelled => MSG_CANCELLED.to_string(),
        StreamError::FirstTokenTimeout => "O modelo não começou a responder a tempo.".to_string(),
        StreamError::TotalTimeout => "O modelo demorou demais para terminar a resposta.".to_string(),
        StreamError::Failed(reason) if reason == "connect" => {
            "Não foi possível conectar ao Ollama. Confira se ele está aberto.".to_string()
        }
        StreamError::Failed(reason) if reason == "status 404" => {
            "O Ollama não encontrou este modelo.".to_string()
        }
        StreamError::Failed(_) => "O Ollama não respondeu a este modelo.".to_string(),
    }
}

async fn one_call(
    client: &Client,
    endpoint: Option<&str>,
    model: &str,
    prompt: &str,
    token: &CancellationToken,
    limit: Duration,
) -> Result<StreamTimings, StreamError> {
    stream_ollama_chat(
        client,
        endpoint,
        model,
        system_prompt(),
        prompt,
        token,
        StreamLimits {
            first_token: limit,
            total: limit,
        },
        |_| {},
    )
    .await
    .map(|outcome| outcome.timings)
}

async fn measure_model(
    client: &Client,
    endpoint: Option<&str>,
    model: &str,
    prompt: &str,
    token: &CancellationToken,
) -> Result<(u64, u64), StreamError> {
    one_call(client, endpoint, model, prompt, token, WARMUP_LIMIT).await?;
    let mut first = Vec::with_capacity(MEASURED_RUNS);
    let mut total = Vec::with_capacity(MEASURED_RUNS);
    for _ in 0..MEASURED_RUNS {
        if recording_commands::is_recording().await {
            token.cancel();
            return Err(StreamError::Cancelled);
        }
        let timings = one_call(client, endpoint, model, prompt, token, MEASURED_LIMIT).await?;
        first.push(timings.first_token);
        total.push(timings.total);
    }
    match (median_ms(&first), median_ms(&total)) {
        (Some(first), Some(total)) => Ok((first, total)),
        _ => Err(StreamError::Failed("empty".to_string())),
    }
}

/// Measures the installed (or the given) Ollama models one by one: a warm-up
/// call and then the median of two streamed calls. Results keep the order of
/// the models. A model that fails has `ok: false` and a message in `error`.
#[tauri::command]
pub async fn benchmark_live_assist_models(
    app: AppHandle<Wry>,
    models: Option<Vec<String>>,
) -> Result<Vec<BenchmarkResult>, String> {
    if recording_commands::is_recording().await {
        return Err(MSG_RECORDING.to_string());
    }
    let pool = app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone();

    let token = CancellationToken::new();
    {
        let mut slot = RUNNING
            .lock()
            .map_err(|_| "Não foi possível iniciar a medição.".to_string())?;
        if slot.is_some() {
            return Err("Já existe uma medição em andamento.".to_string());
        }
        *slot = Some(token.clone());
    }
    let _running = RunningGuard;

    let endpoint = load_hint(&pool)
        .await
        .ollama_endpoint
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let models = match models {
        Some(models) => clean_models(models),
        None => {
            let listed = crate::ollama::get_ollama_models(endpoint.clone())
                .await
                .map_err(|_| {
                    "Não foi possível listar os modelos do Ollama. Confira se ele está aberto e tem modelos instalados."
                        .to_string()
                })?;
            clean_models(listed.into_iter().map(|model| model.name).collect())
        }
    };
    if models.is_empty() {
        return Err("Nenhum modelo para medir.".to_string());
    }

    let client = Client::new();
    let prompt = benchmark_prompt();
    let total = models.len();
    let mut results = Vec::with_capacity(total);
    info!("Live assist benchmark started; models={total}");

    for (index, model) in models.iter().enumerate() {
        let recording = recording_commands::is_recording().await;
        if recording {
            token.cancel();
        }
        if token.is_cancelled() {
            let message = if recording || recording_commands::is_recording().await {
                MSG_INTERRUPTED
            } else {
                MSG_CANCELLED
            };
            results.push(BenchmarkResult::failed(model, message));
            continue;
        }
        let _ = app.emit(
            PROGRESS_EVENT,
            BenchmarkProgress {
                model: model.clone(),
                index,
                total,
            },
        );
        match measure_model(&client, endpoint.as_deref(), model, &prompt, &token).await {
            Ok((first_token_ms, total_ms)) => results.push(BenchmarkResult {
                model: model.clone(),
                first_token_ms: Some(first_token_ms),
                total_ms: Some(total_ms),
                ok: true,
                error: None,
            }),
            Err(error) => {
                let interrupted = recording_commands::is_recording().await;
                debug!("Live assist benchmark call failed; index={index}");
                results.push(BenchmarkResult::failed(model, &describe(&error, interrupted)));
            }
        }
    }
    info!(
        "Live assist benchmark finished; ok={}",
        results.iter().filter(|result| result.ok).count()
    );
    Ok(results)
}

/// Cancels the benchmark in progress. Returns false when none was running.
#[tauri::command]
pub async fn cancel_live_assist_benchmark() -> bool {
    cancel_running()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_two_is_the_mean_and_of_three_the_middle() {
        assert_eq!(median_ms(&[]), None);
        assert_eq!(median_ms(&[Duration::from_millis(400)]), Some(400));
        assert_eq!(
            median_ms(&[Duration::from_millis(900), Duration::from_millis(300)]),
            Some(600)
        );
        assert_eq!(
            median_ms(&[
                Duration::from_millis(900),
                Duration::from_millis(100),
                Duration::from_millis(300),
            ]),
            Some(300)
        );
    }

    #[test]
    fn model_names_are_trimmed_and_deduplicated() {
        let cleaned = clean_models(vec![
            " llama3.2:latest ".to_string(),
            String::new(),
            "qwen2.5:3b".to_string(),
            "llama3.2:latest".to_string(),
        ]);
        assert_eq!(cleaned, vec!["llama3.2:latest".to_string(), "qwen2.5:3b".to_string()]);
    }

    #[test]
    fn the_prompt_has_the_live_assist_shape() {
        let prompt = benchmark_prompt();
        assert!(prompt.contains("Idioma das sugestões: pt-BR"));
        assert!(prompt.contains("[Pessoa 1]"));
        assert!(prompt.contains("[Você]"));
        assert!(system_prompt().contains("\"respostas\""));
    }

    #[test]
    fn results_serialize_in_camel_case() {
        let json = serde_json::to_value(BenchmarkResult {
            model: "m".to_string(),
            first_token_ms: Some(120),
            total_ms: Some(900),
            ok: true,
            error: None,
        })
        .unwrap();
        assert_eq!(json["firstTokenMs"], 120);
        assert_eq!(json["totalMs"], 900);
        assert_eq!(json["ok"], true);
        assert!(json["error"].is_null());
        let failed = serde_json::to_value(BenchmarkResult::failed("m", MSG_CANCELLED)).unwrap();
        assert!(failed["firstTokenMs"].is_null());
        assert_eq!(failed["error"], MSG_CANCELLED);
    }

    /// Manual check against a real local Ollama:
    /// `cargo test --features metal --lib live_assist::benchmark -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs a local Ollama with the default model pulled"]
    async fn local_ollama_streams_partial_items() {
        use crate::live_assist::parse::parse_assist_json;
        use crate::live_assist::partial::extract_partial_items;

        let token = CancellationToken::new();
        let prompt = benchmark_prompt();
        let mut growth: Vec<usize> = Vec::new();
        let outcome = stream_ollama_chat(
            &Client::new(),
            None,
            crate::live_assist::target::DEFAULT_MODEL,
            system_prompt(),
            &prompt,
            &token,
            StreamLimits {
                first_token: WARMUP_LIMIT,
                total: WARMUP_LIMIT,
            },
            |content| growth.push(extract_partial_items(content).len()),
        )
        .await
        .expect("local Ollama answered");
        println!(
            "first_token_ms={} total_ms={} callbacks={} max_partial_items={}",
            outcome.timings.first_token.as_millis(),
            outcome.timings.total.as_millis(),
            growth.len(),
            growth.iter().max().copied().unwrap_or(0),
        );
        println!("content={}", outcome.content);
        assert!(growth.len() > 1, "the answer should arrive in more than one piece");
        // Small models do not always return the exact shape; the app discards
        // those answers. This check is about the transport, so it only reports.
        println!("valid_final_json={}", parse_assist_json(&outcome.content).is_some());

        let (first, total) = measure_model(
            &Client::new(),
            None,
            crate::live_assist::target::DEFAULT_MODEL,
            &prompt,
            &token,
        )
        .await
        .expect("benchmark of the default model");
        println!("median_first_token_ms={first} median_total_ms={total}");
        assert!(first <= total);
    }
}
