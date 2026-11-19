//! Optional neural speaker embeddings, computed on this machine.
//!
//! Two models can be installed; one is in use at a time (`select_voice_model`).
//!
//! **CAM++** (`campplus-zh-en`, the default): speaker verification model
//! `speech_campplus_sv_zh_en_16k-common_advanced` from the 3D-Speaker project
//! (Alibaba DAMO), trained on about 200k Chinese and English speakers.
//! License: Apache-2.0 (model card at
//! <https://www.modelscope.cn/models/iic/speech_campplus_sv_zh_en_16k-common_advanced>,
//! code at <https://github.com/modelscope/3D-Speaker>). The ONNX export is the
//! one published for sherpa-onnx at
//! <https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models>.
//! Checked with onnxruntime on the downloaded file: input `x` float32
//! `[N, T, 80]` (mean-normalized fbank, see `speakers::fbank`), output
//! `embedding` float32 `[N, 192]`; metadata `sample_rate=16000`,
//! `normalize_samples=1` (samples in -1..1), `feature_normalize_type=global-mean`.
//!
//! **ReDimNet2-B6** (`redimnet2-b6`): the `vb2+vox2_v0` large-margin checkpoint
//! of <https://github.com/PalabraAI/redimnet2> (MIT), trained on VoxBlink2 and
//! VoxCeleb2, which cover many languages. The ONNX export is
//! <https://huggingface.co/soniqo/ReDimNet2-B6-ONNX-FP32> (MIT), pinned by
//! commit. Checked with onnxruntime: input `audio` float32 `[1, 96000]` (six
//! seconds of 16 kHz samples in -1..1; the mel front end is inside the graph),
//! output `embedding` float32 `[1, 192]`, unit length. On 20 Portuguese
//! speakers of Multilingual LibriSpeech it separated voices with more margin
//! than CAM++ (pair EER at 3 s: 0.47% against 0.59%; at 1.5 s: 3.6% against
//! 4.2%), at many times the CPU cost per turn. Thresholds are in `book.rs`.
//!
//! A file is downloaded once, when the user asks for it. Nothing else here
//! uses the network. While the file is missing, switched off, or failing, the
//! callers keep using the spectrum embedding.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use ndarray::{Array2, Array3};
use ort::execution_providers::CPUExecutionProvider;
use ort::inputs;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::embed::{Embedding, EmbeddingKind};
use super::fbank;

/// What the graph takes as input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frontend {
    /// `[1, frames, 80]` mean-normalized fbank computed here (`speakers::fbank`).
    Fbank,
    /// `[1, samples]` of 16 kHz audio in -1..1. Shorter turns are repeated to
    /// fill the window, longer ones are cropped to their center.
    Waveform { samples: usize },
}

#[derive(Debug, Clone, Copy)]
pub struct VoiceModelSpec {
    /// Stable id saved in the settings and used by the commands.
    pub id: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    pub input: &'static str,
    pub output: &'static str,
    pub dim: usize,
    pub kind: EmbeddingKind,
    pub frontend: Frontend,
    pub license: &'static str,
    /// One sentence in Portuguese for the settings screen.
    pub about: &'static str,
}

pub const DEFAULT_MODEL_ID: &str = "campplus-zh-en";

pub const VOICE_MODELS: [VoiceModelSpec; 2] = [
    VoiceModelSpec {
        id: DEFAULT_MODEL_ID,
        name: "CAM++ (3D-Speaker, zh-en 16 kHz)",
        file: "3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
        // SHA-256 of the release asset, computed on 2026-11-18 and again on 2026-11-19;
        // the release's own checksum.txt lists the same value.
        sha256: "aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2",
        bytes: 28_281_164,
        input: "x",
        output: "embedding",
        dim: 192,
        kind: EmbeddingKind::Neural,
        frontend: Frontend::Fbank,
        license: "Apache-2.0",
        about: "Leve e rápido. Treinado com falantes de chinês e inglês.",
    },
    VoiceModelSpec {
        id: "redimnet2-b6",
        name: "ReDimNet2-B6 (VoxBlink2 + VoxCeleb2)",
        file: "ReDimNet2B6.onnx",
        url: "https://huggingface.co/soniqo/ReDimNet2-B6-ONNX-FP32/resolve/e911e3f063899805f3d94ee5d1db53fff8e9f3e8/ReDimNet2B6.onnx",
        // Computed on the downloaded file on 2026-11-19; equal to the hash on the
        // repository's LFS page and to `model_sha256` in its config.json.
        sha256: "d960e9f713eaba712f44498e82221b6fc12fcd7eaacd367c9eac45d3a6ce59ec",
        bytes: 51_223_453,
        input: "audio",
        output: "embedding",
        dim: 192,
        kind: EmbeddingKind::NeuralRedimnet,
        frontend: Frontend::Waveform { samples: 96_000 },
        license: "MIT",
        about: "Separa vozes com mais margem em português. Treinado com falantes de muitos idiomas; usa mais processador.",
    },
];

/// Kept for callers that show the default model's name.
pub const MODEL_NAME: &str = VOICE_MODELS[0].name;

pub fn spec_by_id(id: &str) -> Option<&'static VoiceModelSpec> {
    VOICE_MODELS.iter().find(|spec| spec.id == id)
}

fn index_of(id: &str) -> Option<usize> {
    VOICE_MODELS.iter().position(|spec| spec.id == id)
}

/// Below this the turn gets no neural vector and the caller keeps today's
/// rule: match on the spectrum or inherit the previous person.
const MIN_SECONDS: f32 = 0.8;
/// Longer turns are cropped to their central part.
const MAX_SECONDS: f32 = 10.0;

const PROGRESS_EVENT: &str = "voice-model-download-progress";
const STATUS_EVENT: &str = "voice-model-status";
const STALLED_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelStatus {
    pub installed: bool,
    pub enabled: bool,
    pub downloading: bool,
    /// 0..1. 1 when installed, the download fraction while downloading.
    pub progress: f32,
    pub size_mb: f32,
    pub model_name: String,
    pub error: Option<String>,
    /// The model these fields are about: the one being downloaded, otherwise
    /// the one chosen for use.
    pub model_id: String,
    /// The model chosen for use.
    pub selected_model_id: String,
}

/// One row of `list_voice_models`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelOption {
    pub id: String,
    pub name: String,
    pub about: String,
    pub license: String,
    pub size_mb: f32,
    pub installed: bool,
    pub selected: bool,
    pub downloading: bool,
}

struct State {
    dir: Option<PathBuf>,
    enabled: bool,
    settings_loaded: bool,
    /// Index in `VOICE_MODELS` of the model chosen for use.
    selected: usize,
    /// The model the current recording session started with. A change of
    /// `selected` takes effect at the next `prepare`, so one meeting never
    /// mixes vectors of two models.
    session_model: usize,
    downloading: Option<usize>,
    progress: f32,
    error: Option<String>,
    cancel: Option<CancellationToken>,
    /// Inference failed in this recording session: spectrum until the next one.
    failed: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    dir: None,
    enabled: true,
    settings_loaded: false,
    selected: 0,
    session_model: 0,
    downloading: None,
    progress: 0.0,
    error: None,
    cancel: None,
    failed: false,
});

/// Created on first use and kept, with the index of the model it was built
/// from. Its lock is only taken on blocking threads.
static SESSION: Mutex<Option<(usize, Session)>> = Mutex::new(None);

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn session() -> std::sync::MutexGuard<'static, Option<(usize, Session)>> {
    SESSION.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn model_path(spec: &VoiceModelSpec) -> Option<PathBuf> {
    state().dir.as_ref().map(|dir| dir.join(spec.file))
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

fn is_installed(path: &Path, spec: &VoiceModelSpec) -> bool {
    std::fs::metadata(path).map(|meta| meta.is_file() && meta.len() == spec.bytes).unwrap_or(false)
}

fn size_mb(spec: &VoiceModelSpec) -> f32 {
    (spec.bytes as f32 / 100_000.0).round() / 10.0
}

fn status_of(state: &State, installed: bool) -> VoiceModelStatus {
    let focus = &VOICE_MODELS[state.downloading.unwrap_or(state.selected)];
    VoiceModelStatus {
        installed,
        enabled: state.enabled,
        downloading: state.downloading.is_some(),
        progress: if state.downloading.is_some() {
            state.progress.clamp(0.0, 1.0)
        } else if installed {
            1.0
        } else {
            0.0
        },
        size_mb: size_mb(focus),
        model_name: focus.name.to_string(),
        error: state.error.clone(),
        model_id: focus.id.to_string(),
        selected_model_id: VOICE_MODELS[state.selected].id.to_string(),
    }
}

fn status() -> VoiceModelStatus {
    let guard = state();
    let focus = &VOICE_MODELS[guard.downloading.unwrap_or(guard.selected)];
    let installed = guard.downloading.is_none()
        && guard.dir.as_ref().is_some_and(|dir| is_installed(&dir.join(focus.file), focus));
    status_of(&guard, installed)
}

fn options() -> Vec<VoiceModelOption> {
    let guard = state();
    VOICE_MODELS
        .iter()
        .enumerate()
        .map(|(index, spec)| VoiceModelOption {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            about: spec.about.to_string(),
            license: spec.license.to_string(),
            size_mb: size_mb(spec),
            installed: guard.dir.as_ref().is_some_and(|dir| is_installed(&dir.join(spec.file), spec)),
            selected: guard.selected == index,
            downloading: guard.downloading == Some(index),
        })
        .collect()
}

/// The known models with what is installed and chosen, and whether the
/// neural model is switched on. For other modules (`model_advisor`).
pub async fn overview<R: Runtime>(app: &AppHandle<R>) -> (Vec<VoiceModelOption>, bool) {
    resolve(app).await;
    let enabled = state().enabled;
    (options(), enabled)
}

/// Call once at the start of a recording session, before `embed`. Clears the
/// "failed" mark, so each session tries the model again, and fixes the model
/// this session uses.
pub async fn prepare<R: Runtime>(app: &AppHandle<R>) {
    resolve(app).await;
    let mut guard = state();
    guard.failed = false;
    guard.session_model = guard.selected;
}

/// Resolves the model folder (app data dir, next to the Whisper and Parakeet
/// models) and reads the saved switch and choice once.
async fn resolve<R: Runtime>(app: &AppHandle<R>) {
    let dir = app.path().app_data_dir().ok().map(|dir| dir.join("models").join("speaker"));
    let load_settings = {
        let mut guard = state();
        if dir.is_some() {
            guard.dir = dir;
        }
        !guard.settings_loaded
    };
    if !load_settings {
        return;
    }
    let Some(app_state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let pool = app_state.db_manager.pool();
    let Ok(row) = sqlx::query("SELECT * FROM voice_model_settings WHERE id = '1'").fetch_optional(pool).await else {
        return;
    };
    let enabled = row.as_ref().and_then(|row| row.try_get::<i64, _>("enabled").ok()).unwrap_or(1) != 0;
    // The column comes from a later migration; without it the default model stays.
    let selected = row
        .as_ref()
        .and_then(|row| row.try_get::<String, _>("model_id").ok())
        .and_then(|id| index_of(&id))
        .unwrap_or(0);
    let mut guard = state();
    if !guard.settings_loaded {
        guard.enabled = enabled;
        guard.selected = selected;
        guard.session_model = selected;
        guard.settings_loaded = true;
    }
}

/// Neural vector for one turn, or `None` when the model is not in use, the
/// turn is shorter than `MIN_SECONDS`, or inference failed (then the model
/// stays off until the next `prepare`). Inference runs on a blocking thread.
pub async fn embed(samples: &[f32], sample_rate: u32) -> Option<Embedding> {
    if sample_rate < 8_000 || (samples.len() as f32) < MIN_SECONDS * sample_rate as f32 {
        return None;
    }
    let (index, path) = {
        let guard = state();
        if !guard.enabled || guard.failed {
            return None;
        }
        let index = guard.session_model;
        (index, guard.dir.as_ref()?.join(VOICE_MODELS[index].file))
    };
    if !is_installed(&path, &VOICE_MODELS[index]) {
        return None;
    }
    let samples = crop_center(samples, (MAX_SECONDS * sample_rate as f32) as usize).to_vec();
    let result = tokio::task::spawn_blocking(move || embed_blocking(index, &path, &samples, sample_rate))
        .await
        .unwrap_or_else(|error| Err(format!("inference task ended early: {error}")));
    match result {
        Ok(embedding) => Some(embedding),
        Err(error) => {
            let first = {
                let mut guard = state();
                !std::mem::replace(&mut guard.failed, true)
            };
            if first {
                log::warn!("Voice model failed, using the spectrum for the rest of this session: {error}");
            }
            None
        }
    }
}

fn crop_center(samples: &[f32], max_len: usize) -> &[f32] {
    if samples.len() <= max_len {
        return samples;
    }
    let start = (samples.len() - max_len) / 2;
    &samples[start..start + max_len]
}

/// Exactly `len` samples: the center of a longer turn, or a shorter turn
/// repeated from its start.
fn fit_window(samples: &[f32], len: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; len];
    }
    if samples.len() >= len {
        return crop_center(samples, len).to_vec();
    }
    samples.iter().copied().cycle().take(len).collect()
}

fn embed_blocking(index: usize, path: &Path, samples: &[f32], sample_rate: u32) -> Result<Embedding, String> {
    let spec = &VOICE_MODELS[index];
    let resampled;
    let samples = if sample_rate == fbank::SAMPLE_RATE {
        samples
    } else {
        resampled = crate::audio::audio_processing::resample(samples, sample_rate, fbank::SAMPLE_RATE)
            .map_err(|error| format!("resample to 16 kHz: {error}"))?;
        &resampled
    };
    let input = match spec.frontend {
        Frontend::Fbank => {
            let features = fbank::fbank(samples).ok_or("no fbank frames")?;
            let frames = features.len() / fbank::MEL_BINS;
            Array3::from_shape_vec((1, frames, fbank::MEL_BINS), features)
                .map_err(|error| error.to_string())?
                .into_dyn()
        }
        Frontend::Waveform { samples: window } => Array2::from_shape_vec((1, window), fit_window(samples, window))
            .map_err(|error| error.to_string())?
            .into_dyn(),
    };

    let mut guard = session();
    if guard.as_ref().map(|(loaded, _)| *loaded) != Some(index) {
        *guard = None;
        *guard = Some((index, open_session(path)?));
    }
    let (_, session) = guard.as_mut().ok_or("no session")?;
    let outputs = session
        .run(inputs![spec.input => TensorRef::from_array_view(input.view()).map_err(|error| error.to_string())?])
        .map_err(|error| error.to_string())?;
    let output = outputs
        .get(spec.output)
        .ok_or("model output not found")?
        .try_extract_array::<f32>()
        .map_err(|error| error.to_string())?;
    let values: Vec<f32> = output.iter().copied().collect();
    if values.len() != spec.dim {
        return Err(format!("expected {} values, got {}", spec.dim, values.len()));
    }
    Embedding::neural_of(spec.kind, values).ok_or_else(|| "embedding is zero or not finite".to_string())
}

fn open_session(path: &Path) -> Result<Session, String> {
    crate::ensure_onnx_runtime_available().map_err(|error| error.to_string())?;
    // Two threads: the transcription model running at the same time needs the
    // other cores.
    Session::builder()
        .and_then(|builder| builder.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|builder| builder.with_execution_providers(vec![CPUExecutionProvider::default().build()]))
        .and_then(|builder| builder.with_intra_threads(2))
        .and_then(|builder| builder.commit_from_file(path))
        .map_err(|error| format!("load {}: {error}", path.display()))
}

#[derive(Debug)]
enum DownloadError {
    Cancelled,
    Failed(String),
}

/// Streams `url` to `<dest>.part`, checks size and SHA-256, then renames over
/// `dest`. The part file is removed on any other outcome.
async fn download_to(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(f32),
) -> Result<(), DownloadError> {
    let part = part_path(dest);
    let result = download_part(url, &part, expected_sha256, expected_bytes, cancel, &mut on_progress).await;
    let result = match result {
        Ok(()) => tokio::fs::rename(&part, dest).await.map_err(disk),
        Err(error) => Err(error),
    };
    if result.is_err() {
        let _ = tokio::fs::remove_file(&part).await;
    }
    result
}

fn network<E>(_: E) -> DownloadError {
    DownloadError::Failed("Não foi possível baixar o modelo de voz. Verifique a internet e tente de novo.".to_string())
}

fn disk<E>(_: E) -> DownloadError {
    DownloadError::Failed("Não foi possível gravar o modelo de voz no disco.".to_string())
}

fn corrupt() -> DownloadError {
    DownloadError::Failed("O arquivo baixado não confere com o modelo esperado. Tente de novo.".to_string())
}

async fn download_part(
    url: &str,
    part: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
    cancel: &CancellationToken,
    on_progress: &mut impl FnMut(f32),
) -> Result<(), DownloadError> {
    if let Some(parent) = part.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(disk)?;
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("Meetily/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(network)?;
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
        response = client.get(url).send() => response.and_then(|response| response.error_for_status()).map_err(network)?,
    };
    if response.content_length().is_some_and(|length| length != expected_bytes) {
        return Err(corrupt());
    }

    let mut file = tokio::fs::File::create(part).await.map_err(disk)?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
            chunk = tokio::time::timeout(STALLED_AFTER, stream.next()) => chunk.map_err(network)?,
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.map_err(network)?;
        received += chunk.len() as u64;
        if received > expected_bytes {
            return Err(corrupt());
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(disk)?;
        on_progress(received as f32 / expected_bytes.max(1) as f32);
    }
    file.flush().await.map_err(disk)?;
    file.sync_all().await.map_err(disk)?;
    drop(file);

    let digest = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    if received != expected_bytes || !digest.eq_ignore_ascii_case(expected_sha256) {
        return Err(corrupt());
    }
    if cancel.is_cancelled() {
        return Err(DownloadError::Cancelled);
    }
    Ok(())
}

fn emit_status<R: Runtime>(app: &AppHandle<R>) -> VoiceModelStatus {
    let current = status();
    let _ = app.emit(STATUS_EVENT, current.clone());
    current
}

/// The model a command is about: the given id, or the one chosen for use.
fn target(model_id: Option<&str>) -> Result<usize, String> {
    match model_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => index_of(id).ok_or_else(|| "Modelo de voz desconhecido.".to_string()),
        None => Ok(state().selected),
    }
}

#[tauri::command]
pub async fn get_voice_model_status<R: Runtime>(app: AppHandle<R>) -> Result<VoiceModelStatus, String> {
    resolve(&app).await;
    Ok(status())
}

/// The voice models this build knows, with what is installed and in use.
#[tauri::command]
pub async fn list_voice_models<R: Runtime>(app: AppHandle<R>) -> Result<Vec<VoiceModelOption>, String> {
    resolve(&app).await;
    Ok(options())
}

/// Resolves when the download ends. Cancelling is not an error: the returned
/// status then has `installed: false` and `error: null`. Without `model_id`
/// it downloads the model chosen for use, as before.
#[tauri::command]
pub async fn download_voice_model<R: Runtime>(
    app: AppHandle<R>,
    model_id: Option<String>,
) -> Result<VoiceModelStatus, String> {
    resolve(&app).await;
    let index = target(model_id.as_deref())?;
    let spec = &VOICE_MODELS[index];
    let path = model_path(spec).ok_or("A pasta de modelos não está disponível.")?;
    if is_installed(&path, spec) {
        return Ok(emit_status(&app));
    }
    let cancel = CancellationToken::new();
    {
        let mut guard = state();
        if guard.downloading.is_some() {
            return Err("O modelo de voz já está sendo baixado.".to_string());
        }
        guard.downloading = Some(index);
        guard.progress = 0.0;
        guard.error = None;
        guard.cancel = Some(cancel.clone());
    }
    let _ = app.emit(PROGRESS_EVENT, serde_json::json!({ "progress": 0.0, "modelId": spec.id }));

    let progress_app = app.clone();
    let mut last_sent = 0.0f32;
    let result = download_to(spec.url, &path, spec.sha256, spec.bytes, &cancel, move |progress| {
        let progress = progress.clamp(0.0, 1.0);
        state().progress = progress;
        if progress - last_sent >= 0.01 || (progress >= 1.0 && last_sent < 1.0) {
            last_sent = progress;
            let _ = progress_app.emit(PROGRESS_EVENT, serde_json::json!({ "progress": progress, "modelId": spec.id }));
        }
    })
    .await;

    {
        let mut guard = state();
        guard.downloading = None;
        guard.cancel = None;
        guard.progress = 0.0;
        match &result {
            Ok(()) => guard.failed = false,
            Err(DownloadError::Cancelled) => {}
            Err(DownloadError::Failed(message)) => guard.error = Some(message.clone()),
        }
    }
    match &result {
        Ok(()) => log::info!("Voice model {} downloaded to {}", spec.id, path.display()),
        Err(DownloadError::Cancelled) => log::info!("Voice model download cancelled"),
        Err(DownloadError::Failed(message)) => log::warn!("Voice model download failed: {message}"),
    }
    let current = emit_status(&app);
    match result {
        Err(DownloadError::Failed(message)) => Err(message),
        _ => Ok(current),
    }
}

#[tauri::command]
pub async fn cancel_voice_model_download() -> Result<VoiceModelStatus, String> {
    if let Some(cancel) = state().cancel.as_ref() {
        cancel.cancel();
    }
    Ok(status())
}

/// Without `model_id` it removes the model chosen for use, as before.
#[tauri::command]
pub async fn delete_voice_model<R: Runtime>(
    app: AppHandle<R>,
    model_id: Option<String>,
) -> Result<VoiceModelStatus, String> {
    resolve(&app).await;
    let index = target(model_id.as_deref())?;
    if state().downloading.is_some() {
        return Err("Cancele o download antes de apagar o modelo de voz.".to_string());
    }
    let path = model_path(&VOICE_MODELS[index]).ok_or("A pasta de modelos não está disponível.")?;
    let removed = tokio::task::spawn_blocking(move || {
        // Close the session first: a turn being embedded finishes before this.
        let mut guard = session();
        if guard.as_ref().is_some_and(|(loaded, _)| *loaded == index) {
            *guard = None;
        }
        drop(guard);
        let _ = std::fs::remove_file(part_path(&path));
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Não foi possível apagar o modelo de voz.".to_string()),
        }
    })
    .await
    .map_err(|_| "Não foi possível apagar o modelo de voz.".to_string())?;
    removed?;
    state().error = None;
    Ok(emit_status(&app))
}

#[tauri::command]
pub async fn set_voice_model_enabled<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<VoiceModelStatus, String> {
    resolve(&app).await;
    if let Some(app_state) = app.try_state::<crate::state::AppState>() {
        sqlx::query(
            "INSERT INTO voice_model_settings (id, enabled) VALUES ('1', ?)
             ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled",
        )
        .bind(i64::from(enabled))
        .execute(app_state.db_manager.pool())
        .await
        .map_err(|_| "Não foi possível salvar a preferência do modelo de voz.".to_string())?;
    }
    {
        let mut guard = state();
        guard.enabled = enabled;
        guard.settings_loaded = true;
    }
    Ok(emit_status(&app))
}

/// Chooses the installed model to use from the next recording on. A recording
/// in progress keeps the model it started with. Voices saved under a name
/// with the other model are matched by the simple method until renamed again.
#[tauri::command]
pub async fn select_voice_model<R: Runtime>(app: AppHandle<R>, model_id: String) -> Result<VoiceModelStatus, String> {
    resolve(&app).await;
    let index = target(Some(&model_id))?;
    let spec = &VOICE_MODELS[index];
    let installed = model_path(spec).is_some_and(|path| is_installed(&path, spec));
    if !installed {
        return Err("Baixe o modelo de voz antes de usar.".to_string());
    }
    if let Some(app_state) = app.try_state::<crate::state::AppState>() {
        sqlx::query(
            "INSERT INTO voice_model_settings (id, model_id) VALUES ('1', ?)
             ON CONFLICT(id) DO UPDATE SET model_id = excluded.model_id",
        )
        .bind(spec.id)
        .execute(app_state.db_manager.pool())
        .await
        .map_err(|_| "Não foi possível salvar o modelo de voz escolhido.".to_string())?;
    }
    {
        let mut guard = state();
        guard.selected = index;
        guard.settings_loaded = true;
        guard.error = None;
    }
    Ok(emit_status(&app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn blank_state() -> State {
        State {
            dir: None,
            enabled: true,
            settings_loaded: false,
            selected: 0,
            session_model: 0,
            downloading: None,
            progress: 0.0,
            error: None,
            cancel: None,
            failed: false,
        }
    }

    #[test]
    fn the_status_reaches_the_frontend_in_camel_case() {
        let mut state = blank_state();
        state.downloading = Some(0);
        state.progress = 0.25;
        let json = serde_json::to_value(status_of(&state, false)).unwrap();
        let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        // The first seven are the original contract; the two ids were added later.
        assert_eq!(
            keys,
            [
                "downloading",
                "enabled",
                "error",
                "installed",
                "modelId",
                "modelName",
                "progress",
                "selectedModelId",
                "sizeMb"
            ]
        );
        assert_eq!(json["modelId"], DEFAULT_MODEL_ID);
        assert_eq!(json["selectedModelId"], DEFAULT_MODEL_ID);
        assert_eq!(json["installed"], false);
        assert_eq!(json["enabled"], true);
        assert_eq!(json["downloading"], true);
        assert_eq!(json["progress"], 0.25);
        assert!((json["sizeMb"].as_f64().unwrap() - 28.3).abs() < 1.0e-4);
        assert_eq!(json["modelName"], MODEL_NAME);
        assert!(json["error"].is_null());
    }

    #[test]
    fn progress_is_one_when_installed_and_zero_when_idle() {
        let mut state = blank_state();
        state.progress = 0.4;
        assert_eq!(status_of(&state, false).progress, 0.0);
        assert_eq!(status_of(&state, true).progress, 1.0);
        state.enabled = false;
        state.error = Some("x".into());
        let status = status_of(&state, true);
        assert!(!status.enabled && status.installed);
        assert_eq!(status.error.as_deref(), Some("x"));
    }

    #[test]
    fn the_status_is_about_the_model_being_downloaded() {
        let mut state = blank_state();
        state.downloading = Some(1);
        let status = status_of(&state, false);
        assert_eq!(status.model_id, "redimnet2-b6");
        assert_eq!(status.selected_model_id, DEFAULT_MODEL_ID);
        assert_eq!(status.model_name, VOICE_MODELS[1].name);
        assert!((status.size_mb - 51.2).abs() < 1.0e-4);
    }

    #[test]
    fn the_catalog_is_pinned_and_has_one_model_per_kind() {
        assert_eq!(VOICE_MODELS[0].id, DEFAULT_MODEL_ID);
        assert_eq!(VOICE_MODELS[0].kind, EmbeddingKind::Neural);
        for (index, spec) in VOICE_MODELS.iter().enumerate() {
            assert_eq!(spec.sha256.len(), 64);
            assert!(spec.sha256.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
            assert!(spec.url.starts_with("https://") && spec.url.ends_with(spec.file));
            assert!(spec.bytes > 1_000_000);
            assert!(spec.kind.is_neural());
            assert!(!spec.about.contains('\u{2014}'));
            assert_eq!(spec_by_id(spec.id).map(|found| found.file), Some(spec.file));
            for other in &VOICE_MODELS[..index] {
                assert_ne!(other.id, spec.id);
                assert_ne!(other.file, spec.file);
                assert_ne!(other.kind, spec.kind);
            }
        }
        assert!(spec_by_id("other").is_none());
        // The Hugging Face link names a commit, not a branch.
        assert!(VOICE_MODELS[1].url.contains("/resolve/e911e3f063899805f3d94ee5d1db53fff8e9f3e8/"));
    }

    #[test]
    fn a_short_turn_is_repeated_and_a_long_one_cropped_to_fill_the_window() {
        assert_eq!(fit_window(&[1.0, 2.0, 3.0], 7), [1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0]);
        let long: Vec<f32> = (0..10).map(|index| index as f32).collect();
        assert_eq!(fit_window(&long, 4), [3.0, 4.0, 5.0, 6.0]);
        assert_eq!(fit_window(&long, 10), long);
        assert_eq!(fit_window(&[], 3), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_long_turn_is_cropped_to_its_center() {
        let samples: Vec<f32> = (0..100).map(|index| index as f32).collect();
        assert_eq!(crop_center(&samples, 200).len(), 100);
        let cropped = crop_center(&samples, 10);
        assert_eq!(cropped.len(), 10);
        assert_eq!(cropped[0], 45.0);
        assert_eq!(cropped[9], 54.0);
    }

    #[test]
    fn the_part_file_sits_next_to_the_model() {
        assert_eq!(part_path(Path::new("/m/speaker/a.onnx")), PathBuf::from("/m/speaker/a.onnx.part"));
    }

    #[tokio::test]
    async fn without_a_model_folder_there_is_no_neural_vector() {
        // Nothing in this test binary calls `prepare`, so the folder is unset.
        let samples = vec![0.1f32; 32_000];
        assert!(embed(&samples, 16_000).await.is_none());
        assert!(embed(&samples[..8_000], 16_000).await.is_none());
    }

    #[tokio::test]
    async fn a_broken_model_file_turns_the_model_off_until_the_next_session() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(VOICE_MODELS[0].file), vec![0u8; VOICE_MODELS[0].bytes as usize]).unwrap();
        {
            let mut guard = state();
            guard.dir = Some(folder.path().to_path_buf());
            guard.enabled = true;
            guard.failed = false;
            guard.session_model = 0;
        }
        let samples = crate::speakers::embed::tone(200.0, 2.0, 16_000);
        assert!(embed(&samples, 16_000).await.is_none());
        let failed = state().failed;
        // Marked as failed: the next turn does not try to load the file again.
        assert!(embed(&samples, 16_000).await.is_none());
        {
            let mut guard = state();
            guard.dir = None;
            guard.failed = false;
        }
        assert!(failed);
    }

    /// Serves `body` once over HTTP on a local port. `stall_after` stops
    /// sending after that many bytes and keeps the connection open.
    async fn serve_once(body: Vec<u8>, stall_after: Option<usize>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    return;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            socket.write_all(head.as_bytes()).await.unwrap();
            match stall_after {
                Some(count) => {
                    socket.write_all(&body[..count]).await.unwrap();
                    socket.flush().await.unwrap();
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                None => {
                    socket.write_all(&body).await.unwrap();
                    let _ = socket.shutdown().await;
                }
            }
        });
        format!("http://{address}/model.onnx")
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn body() -> Vec<u8> {
        (0..200_000u32).map(|index| (index % 251) as u8).collect()
    }

    #[tokio::test]
    async fn a_download_with_the_right_hash_lands_under_the_final_name() {
        let folder = tempfile::tempdir().unwrap();
        let dest = folder.path().join("speaker").join("model.onnx");
        let body = body();
        let url = serve_once(body.clone(), None).await;
        let mut last = 0.0f32;
        download_to(&url, &dest, &sha256_hex(&body), body.len() as u64, &CancellationToken::new(), |progress| {
            assert!(progress >= last);
            last = progress;
        })
        .await
        .unwrap();
        assert_eq!(last, 1.0);
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn a_download_with_the_wrong_hash_leaves_no_file() {
        let folder = tempfile::tempdir().unwrap();
        let dest = folder.path().join("model.onnx");
        let body = body();
        let url = serve_once(body.clone(), None).await;
        let wrong = sha256_hex(b"something else");
        let result = download_to(&url, &dest, &wrong, body.len() as u64, &CancellationToken::new(), |_| {}).await;
        assert!(matches!(result, Err(DownloadError::Failed(_))));
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn a_download_of_another_size_is_refused_before_writing() {
        let folder = tempfile::tempdir().unwrap();
        let dest = folder.path().join("model.onnx");
        let body = body();
        let url = serve_once(body.clone(), None).await;
        let result = download_to(&url, &dest, &sha256_hex(&body), body.len() as u64 + 1, &CancellationToken::new(), |_| {}).await;
        assert!(matches!(result, Err(DownloadError::Failed(_))));
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn cancelling_a_stalled_download_removes_the_part_file() {
        let folder = tempfile::tempdir().unwrap();
        let dest = folder.path().join("model.onnx");
        let body = body();
        let url = serve_once(body.clone(), Some(50_000)).await;
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            trigger.cancel();
        });
        let result = download_to(&url, &dest, &sha256_hex(&body), body.len() as u64, &cancel, |_| {}).await;
        assert!(matches!(result, Err(DownloadError::Cancelled)));
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    fn read_wav_16k_mono(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).unwrap();
        let mut offset = 12;
        while offset + 8 <= bytes.len() {
            let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            if &bytes[offset..offset + 4] == b"data" {
                let end = (offset + 8 + size).min(bytes.len());
                return bytes[offset + 8..end]
                    .chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32_768.0)
                    .collect();
            }
            offset += 8 + size + (size & 1);
        }
        panic!("no data chunk in {}", path.display());
    }

    /// Runs the real model. Not part of the normal test run: it needs the
    /// model file and speech recordings, which are not in the repository.
    ///
    /// ```text
    /// MEETILY_VOICE_MODEL=/path/to/<file of one of VOICE_MODELS> \
    /// MEETILY_VOICE_WAVS=/folder/with/<speaker>_<n>.wav   (16 kHz mono PCM16, 2+ speakers, 2+ files each) \
    /// MEETILY_VOICE_REF=/optional/reference.json          ({"file.wav": [192 floats]} from another implementation) \
    /// cargo test --features metal --lib speakers::neural::tests::real_model -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn real_model_separates_speakers_and_agrees_with_the_reference() {
        let model = PathBuf::from(std::env::var("MEETILY_VOICE_MODEL").expect("set MEETILY_VOICE_MODEL"));
        let wavs = PathBuf::from(std::env::var("MEETILY_VOICE_WAVS").expect("set MEETILY_VOICE_WAVS"));
        let index = VOICE_MODELS
            .iter()
            .position(|spec| model.file_name().is_some_and(|name| name == spec.file))
            .expect("the file name must be the one of a model in VOICE_MODELS");
        let spec = &VOICE_MODELS[index];
        assert_eq!(std::fs::metadata(&model).unwrap().len(), spec.bytes);
        assert_eq!(sha256_hex(&std::fs::read(&model).unwrap()), spec.sha256);
        let reference: Option<std::collections::HashMap<String, Vec<f32>>> = std::env::var("MEETILY_VOICE_REF")
            .ok()
            .map(|path| serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap());

        let mut files: Vec<PathBuf> = std::fs::read_dir(&wavs)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "wav"))
            .collect();
        files.sort();
        assert!(files.len() >= 4);

        let mut voices = Vec::new();
        let mut slowest = Duration::ZERO;
        let mut total = Duration::ZERO;
        let mut short_total = Duration::ZERO;
        let mut worst_agreement = 1.0f32;
        for (file_index, file) in files.iter().enumerate() {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let samples = read_wav_16k_mono(file);
            let samples = crop_center(&samples, (MAX_SECONDS * 16_000.0) as usize);
            let started = std::time::Instant::now();
            let embedding = embed_blocking(index, &model, samples, 16_000).unwrap();
            if file_index > 0 {
                slowest = slowest.max(started.elapsed());
                total += started.elapsed();
                let short = crop_center(samples, 48_000);
                let started = std::time::Instant::now();
                embed_blocking(index, &model, short, 16_000).unwrap();
                short_total += started.elapsed();
            }
            assert_eq!(embedding.values.len(), spec.dim);
            assert_eq!(embedding.kind, spec.kind);
            if let Some(expected) = reference.as_ref().and_then(|reference| reference.get(&name)) {
                let expected = Embedding::neural_of(spec.kind, expected.clone()).unwrap();
                let agreement = embedding.similarity(&expected).unwrap();
                // Exactly 1.000 when the turn has a multiple of 200 frames. Otherwise
                // onnxruntime versions differ in how the model's ceil-mode average
                // pooling treats the last partial window (0.986 to 0.995 between the
                // bundled 1.22 and Python's 1.32 on the same features).
                assert!(agreement > 0.98, "{name}: cosine with the reference is {agreement}");
                worst_agreement = worst_agreement.min(agreement);
            }
            // 48 kHz input must land on (nearly) the same vector after resampling.
            if file_index == 0 {
                let upsampled = crate::audio::audio_processing::resample(samples, 16_000, 48_000).unwrap();
                let again = embed_blocking(index, &model, &upsampled, 48_000).unwrap();
                let agreement = embedding.similarity(&again).unwrap();
                println!("16 kHz vs 48 kHz input: {agreement:.3}");
                assert!(agreement > 0.9);
            }
            voices.push((name.split('_').next().unwrap().to_string(), embedding));
        }

        let (mut same_min, mut other_max) = (1.0f32, -1.0f32);
        for (left_index, (left_speaker, left)) in voices.iter().enumerate() {
            for (right_speaker, right) in &voices[..left_index] {
                let score = left.similarity(right).unwrap();
                if left_speaker == right_speaker {
                    same_min = same_min.min(score);
                } else {
                    other_max = other_max.max(score);
                }
            }
        }
        // The path the transcription worker uses, and the clustering on real vectors.
        {
            {
                let mut guard = state();
                guard.dir = model.parent().map(Path::to_path_buf);
                guard.enabled = true;
                guard.failed = false;
                guard.selected = index;
                guard.session_model = index;
            }
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let mut book = crate::speakers::SpeakerBook::default();
            let mut spectrum_book = crate::speakers::SpeakerBook::default();
            let mut spectrum_people = std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            let mut people = std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            for round in 0..3 {
                for file in files.iter().filter(|file| file.to_string_lossy().ends_with(&format!("_{}.wav", round + 1))) {
                    let samples = read_wav_16k_mono(file);
                    let neural = runtime.block_on(embed(&samples, 16_000));
                    assert!(neural.as_ref().is_some_and(|embedding| embedding.kind == spec.kind));
                    let assignment = book
                        .assign_with(crate::speakers::Channel::System, &samples, 16_000, neural)
                        .unwrap();
                    let speaker = file.file_name().unwrap().to_string_lossy().split('_').next().unwrap().to_string();
                    people.entry(speaker.clone()).or_default().insert(assignment.key);
                    let baseline = spectrum_book.assign(crate::speakers::Channel::System, &samples, 16_000).unwrap();
                    spectrum_people.entry(speaker).or_default().insert(baseline.key);
                }
            }
            {
                let mut guard = state();
                guard.dir = None;
                guard.selected = 0;
                guard.session_model = 0;
            }
            let split = people.values().filter(|keys| keys.len() > 1).count();
            let keys: std::collections::BTreeSet<&String> = people.values().flatten().collect();
            println!("{} speakers, {} keys, {split} speakers split in two or more", people.len(), keys.len());
            println!("speaker -> keys with the model: {people:?}");
            println!("speaker -> keys with the spectrum only: {spectrum_people:?}");
        }

        let timed = (files.len() - 1).max(1) as u32;
        println!("average per turn: {:?} (10 s), {:?} (3 s)", total / timed, short_total / timed);
        println!(
            "{} files, same speaker min {same_min:.3}, other speaker max {other_max:.3}, slowest turn {slowest:?}, worst agreement with the reference {worst_agreement:.4}",
            files.len()
        );
        assert!(same_min > other_max);
        // Single turns against single turns; the clustering compares against a
        // running average, which scores higher.
        assert!(same_min >= crate::speakers::book::neural_rule(spec.kind).map(|rule| rule.loose).unwrap_or(1.0));
    }

    /// Downloads the real files (28 MB and 51 MB) and checks the pinned hashes.
    /// `cargo test --features metal --lib speakers::neural::tests::real_download -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn real_download_matches_the_pinned_hash() {
        let folder = tempfile::tempdir().unwrap();
        for spec in &VOICE_MODELS {
            let dest = folder.path().join(spec.file);
            download_to(spec.url, &dest, spec.sha256, spec.bytes, &CancellationToken::new(), |_| {})
                .await
                .unwrap();
            assert!(is_installed(&dest, spec));
        }
    }
}
