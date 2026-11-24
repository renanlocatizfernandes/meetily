//! Models the advisor knows about, by function.
//!
//! Every entry was checked on 2026-11-19 against the page it names: the file
//! exists, the size is the one served, and the license is the one stated
//! there. Directly downloaded files carry the pinned revision and SHA-256.
//! Ollama models are pulled by Ollama itself, which checks its own digests;
//! their size is the sum of the layers in the registry manifest for the tag.
//!
//! Memory figures are estimates of the resident memory while the model is in
//! use, not measurements on every machine. Measured on an Apple M5 Pro: the
//! Parakeet int8 models peaked at 1.5 GB and the FP32 one at 3.0 GB while
//! transcribing; `llama3.2:latest` took 2.56 GB in Ollama 0.40 with a context
//! of 4,096 tokens, 3.12 GB with 8,192 and 4.08 GB with 16,384 (the context
//! Ollama chose by itself on that 24 GB machine). The other Ollama figures
//! come from `ollama_memory_mb`, fitted to those three points, and were not
//! measured. Whisper and voice figures are the weights plus working buffers.
//!
//! Those Ollama figures hold for a few calls, which is how summaries use a
//! model. The live help calls the model every few seconds with a prompt that
//! changes, and Ollama 0.40.16 keeps a prompt cache that grows with every
//! call. Measured on the same M5 Pro (24 GB) on 2026-11-23, with prompts of
//! about 1,700 tokens and a context of 4,096:
//! - `gemma4:12b`, `gemma4:e4b` and `qwen3.5:9b` run on Ollama's MLX runner on
//!   Apple Silicon. Each started near its weights (7.5 to 9 GiB) and reached
//!   15 GiB held, 16 GiB at the peak, in five to eight minutes, whatever the
//!   context asked for.
//! - `llama3.2:latest` (GGUF, llama.cpp runner) went from 0.6 GiB to 9 GiB of
//!   process memory in four minutes with the prompt the live help sent then
//!   (a window that slid on every call) and stayed there. With the talk only
//!   growing at the end between trims, as the live help sends it now, the same
//!   load was at 3.2 GiB after nine minutes (144 calls).
//! That is why the live help has its own suggestion (`LIVE_HELP_MODEL`) and
//! the assistant models below are suggested for the summaries only.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Speech to text.
    Speech,
    /// Who is speaking (speaker embeddings).
    Voices,
    /// Organization, live help and summaries (local LLM).
    Assistant,
}

pub const ROLES: [Role; 3] = [Role::Speech, Role::Voices, Role::Assistant];

/// The model suggested for the live help on every machine: the lightest one
/// of the catalog, served by the llama.cpp runner (see the module note).
pub const LIVE_HELP_MODEL: &str = "llama3.2:latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// whisper.cpp, ggml files (`whisper_engine`).
    Whisper,
    /// ONNX Runtime, onnx-asr layout (`parakeet_engine`).
    Parakeet,
    /// ONNX Runtime speaker embedding (`speakers::neural`).
    Voice,
    /// Pulled and served by the local Ollama.
    Ollama,
}

/// Which languages a model is good for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Coverage {
    /// Fine-tuned for Brazilian Portuguese. Other languages are not guaranteed.
    PortugueseTuned,
    /// The 25 European languages of Parakeet TDT v3.
    European,
    /// Broad multilingual training.
    Multilingual,
    /// Trained on Chinese and English speakers; works on other languages with less margin.
    ChineseEnglish,
}

/// Languages of `nvidia/parakeet-tdt-0.6b-v3` (model card).
pub const PARAKEET_V3_LANGUAGES: [&str; 25] = [
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt", "pl", "pt", "ro",
    "sk", "sl", "es", "sv", "ru", "uk",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFile {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatalogModel {
    /// The name the engine uses: Whisper or Parakeet model name, voice model
    /// id, or Ollama tag.
    pub id: &'static str,
    pub role: Role,
    pub engine: Engine,
    pub name: &'static str,
    /// One sentence in Portuguese about what the model is.
    pub about: &'static str,
    /// Why it is the pick when it is: a Portuguese clause that follows the
    /// model name, without the final period.
    pub why: &'static str,
    pub download_bytes: u64,
    pub memory_mb: u32,
    /// Machines with less total memory than this are never offered the model,
    /// even when the sum would fit: it would leave no room for the meeting app.
    pub min_ram_gb: u32,
    pub coverage: Coverage,
    /// Higher is better, compared only inside the same role, for Portuguese.
    pub rank_pt: u8,
    /// The same for other languages. 0 means "do not recommend".
    pub rank_other: u8,
    /// Needs the GPU of Apple Silicon to keep up with a live meeting.
    pub needs_apple_gpu: bool,
    pub license: &'static str,
    /// Restrictions worth showing, when the license is not a plain permissive one.
    pub license_note: Option<&'static str>,
    pub source: &'static str,
    /// Commit of the repository the files are pinned to, when downloaded directly.
    pub revision: Option<&'static str>,
    pub files: &'static [PinnedFile],
}

const HF_WHISPER: &str = "https://huggingface.co/ggerganov/whisper.cpp";
const WHISPER_REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
const HF_TAGARELA: &str = "https://huggingface.co/alefiury/parakeet-tdt-0.6b-v3-ptBR-TAGARELA-onnx";
const TAGARELA_REVISION: &str = "6235416f71c11eeaf81c534c9a78b1ebd38ecdb9";
const HF_PARAKEET_V3: &str = "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx";
const PARAKEET_V3_REVISION: &str = "8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce";

const NEMO128: PinnedFile = PinnedFile {
    name: "nemo128.onnx",
    bytes: 139_764,
    sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f",
};
const PARAKEET_V3_VOCAB: PinnedFile = PinnedFile {
    name: "vocab.txt",
    bytes: 93_939,
    sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
};

pub const CATALOG: &[CatalogModel] = &[
    // ---- Speech ---------------------------------------------------------
    CatalogModel {
        id: "parakeet-tdt-0.6b-v3-ptbr-tagarela-int8",
        role: Role::Speech,
        engine: Engine::Parakeet,
        name: "Parakeet v3 pt-BR (TAGARELA) int8",
        about: "Parakeet v3 ajustado para português do Brasil com podcasts brasileiros, versão compacta.",
        why: "é ajustado para fala espontânea em português do Brasil, segundo os autores; no nosso teste com fala lida empatou com o Parakeet v3 original e escreveu pior alguns termos em inglês",
        download_bytes: 672_858_975,
        memory_mb: 1_500,
        min_ram_gb: 8,
        coverage: Coverage::PortugueseTuned,
        rank_pt: 80,
        rank_other: 0,
        needs_apple_gpu: false,
        license: "CC-BY-4.0",
        license_note: Some("Uso comercial permitido com atribuição."),
        source: HF_TAGARELA,
        revision: Some(TAGARELA_REVISION),
        files: &[
            PinnedFile {
                name: "encoder-model.int8.onnx",
                bytes: 654_315_802,
                sha256: "5569fdd4b0362eeb9e95078286eb16401d1963bb9e11f0a3ab61fe5fd4baf6dd",
            },
            PinnedFile {
                name: "decoder_joint-model.int8.onnx",
                bytes: 18_309_470,
                sha256: "f898366fa233e2e9a1471d31ae4b8423500f6d2a8471798c7a48a23d648a3e0f",
            },
            NEMO128,
            PARAKEET_V3_VOCAB,
        ],
    },
    CatalogModel {
        id: "parakeet-tdt-0.6b-v3-ptbr-tagarela-fp32",
        role: Role::Speech,
        engine: Engine::Parakeet,
        name: "Parakeet v3 pt-BR (TAGARELA) precisão total",
        about: "O mesmo modelo ajustado para português do Brasil, sem quantização: é a versão dos números publicados.",
        why: "é a versão sem quantização do modelo ajustado para o Brasil; no nosso teste não foi mais precisa que a compacta e usa o dobro de memória",
        download_bytes: 2_550_017_710,
        memory_mb: 3_000,
        min_ram_gb: 24,
        coverage: Coverage::PortugueseTuned,
        rank_pt: 70,
        rank_other: 0,
        needs_apple_gpu: false,
        license: "CC-BY-4.0",
        license_note: Some("Uso comercial permitido com atribuição."),
        source: HF_TAGARELA,
        revision: Some(TAGARELA_REVISION),
        files: &[
            PinnedFile {
                name: "encoder-model.onnx",
                bytes: 41_842_954,
                sha256: "5e6470dc3c6f0bcb5f99dbf3d64285ac52a0e991b3bddcf634e34bbe60d9d6f0",
            },
            PinnedFile {
                name: "encoder-model.onnx.data",
                bytes: 2_435_420_160,
                sha256: "21bee798dceaaff37a31ef2b56910eaed90b71ee2be143d8bda3b7be3f549dd7",
            },
            PinnedFile {
                name: "decoder_joint-model.onnx",
                bytes: 72_520_893,
                sha256: "7ffb7866c27dbd2d983ec82e2780dbb2669f451d33ea0491ac7a81fce5f84ba9",
            },
            NEMO128,
            PARAKEET_V3_VOCAB,
        ],
    },
    CatalogModel {
        id: "parakeet-tdt-0.6b-v3-int8",
        role: Role::Speech,
        engine: Engine::Parakeet,
        name: "Parakeet v3 multilíngue int8",
        about: "Parakeet v3 original da NVIDIA, para 25 idiomas europeus.",
        why: "transcreve português e inglês na mesma reunião, roda rápido só com o processador e empatou com a versão ajustada para o Brasil no nosso teste com fala lida",
        download_bytes: 670_619_706,
        memory_mb: 1_500,
        min_ram_gb: 8,
        coverage: Coverage::European,
        rank_pt: 85,
        rank_other: 80,
        needs_apple_gpu: false,
        license: "CC-BY-4.0",
        license_note: Some("Uso comercial permitido com atribuição."),
        source: HF_PARAKEET_V3,
        revision: Some(PARAKEET_V3_REVISION),
        files: &[
            PinnedFile {
                name: "encoder-model.int8.onnx",
                bytes: 652_183_999,
                sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
            },
            PinnedFile {
                name: "decoder_joint-model.int8.onnx",
                bytes: 18_202_004,
                sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
            },
            NEMO128,
            PARAKEET_V3_VOCAB,
        ],
    },
    CatalogModel {
        id: "large-v3-turbo-q5_0",
        role: Role::Speech,
        engine: Engine::Whisper,
        name: "Whisper large-v3-turbo q5",
        about: "Whisper multilíngue em versão compacta, para quase cem idiomas.",
        why: "cobre quase cem idiomas e é o Whisper mais leve com boa precisão",
        download_bytes: 574_041_195,
        memory_mb: 1_200,
        min_ram_gb: 8,
        coverage: Coverage::Multilingual,
        rank_pt: 50,
        rank_other: 60,
        needs_apple_gpu: true,
        license: "MIT",
        license_note: None,
        source: HF_WHISPER,
        revision: Some(WHISPER_REVISION),
        files: &[PinnedFile {
            name: "ggml-large-v3-turbo-q5_0.bin",
            bytes: 574_041_195,
            sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        }],
    },
    CatalogModel {
        id: "large-v3-turbo-q8_0",
        role: Role::Speech,
        engine: Engine::Whisper,
        name: "Whisper large-v3-turbo q8",
        about: "Whisper multilíngue com quantização leve: quase a precisão total em metade do tamanho.",
        why: "cobre quase cem idiomas com precisão próxima do Whisper turbo completo em metade do tamanho",
        download_bytes: 874_188_075,
        memory_mb: 1_700,
        min_ram_gb: 16,
        coverage: Coverage::Multilingual,
        rank_pt: 55,
        rank_other: 70,
        needs_apple_gpu: true,
        license: "MIT",
        license_note: None,
        source: HF_WHISPER,
        revision: Some(WHISPER_REVISION),
        files: &[PinnedFile {
            name: "ggml-large-v3-turbo-q8_0.bin",
            bytes: 874_188_075,
            sha256: "317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1",
        }],
    },
    CatalogModel {
        id: "large-v3",
        role: Role::Speech,
        engine: Engine::Whisper,
        name: "Whisper large-v3",
        about: "O Whisper mais preciso, mais lento que o turbo.",
        why: "é o Whisper mais preciso e esta máquina tem memória para ele",
        download_bytes: 3_095_033_483,
        memory_mb: 4_200,
        min_ram_gb: 24,
        coverage: Coverage::Multilingual,
        rank_pt: 58,
        rank_other: 75,
        needs_apple_gpu: true,
        license: "MIT",
        license_note: None,
        source: HF_WHISPER,
        revision: Some(WHISPER_REVISION),
        files: &[PinnedFile {
            name: "ggml-large-v3.bin",
            bytes: 3_095_033_483,
            sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        }],
    },
    // ---- Voices ---------------------------------------------------------
    CatalogModel {
        id: "campplus-zh-en",
        role: Role::Voices,
        engine: Engine::Voice,
        name: "CAM++ (3D-Speaker)",
        about: "Modelo de vozes leve, treinado com falantes de chinês e inglês.",
        why: "separa bem as vozes gastando pouco processador, o que importa em uma máquina sem ventoinha ou com pouca memória",
        download_bytes: 28_281_164,
        memory_mb: 150,
        min_ram_gb: 8,
        coverage: Coverage::ChineseEnglish,
        rank_pt: 50,
        rank_other: 50,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models",
        revision: None,
        files: &[PinnedFile {
            name: "3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
            bytes: 28_281_164,
            sha256: "aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2",
        }],
    },
    CatalogModel {
        id: "redimnet2-b6",
        role: Role::Voices,
        engine: Engine::Voice,
        name: "ReDimNet2-B6",
        about: "Modelo de vozes de 2026, treinado com falantes de muitos idiomas (VoxBlink2 e VoxCeleb2).",
        why: "separou vozes em português com mais margem que o modelo atual no nosso teste com 20 falantes e esta máquina tem folga para o custo maior de processador",
        download_bytes: 51_223_453,
        memory_mb: 350,
        // About half a second of two cores per turn on an M5 Pro (measured with
        // the app's own inference path), against 40 ms for CAM++: too much next
        // to transcription on a fanless 8 GB machine.
        min_ram_gb: 16,
        coverage: Coverage::Multilingual,
        rank_pt: 80,
        rank_other: 80,
        needs_apple_gpu: false,
        license: "MIT",
        license_note: None,
        source: "https://huggingface.co/soniqo/ReDimNet2-B6-ONNX-FP32",
        revision: Some("e911e3f063899805f3d94ee5d1db53fff8e9f3e8"),
        files: &[PinnedFile {
            name: "ReDimNet2B6.onnx",
            bytes: 51_223_453,
            sha256: "d960e9f713eaba712f44498e82221b6fc12fcd7eaacd367c9eac45d3a6ce59ec",
        }],
    },
    // ---- Assistant (Ollama) --------------------------------------------
    CatalogModel {
        id: "llama3.2:latest",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Llama 3.2 3B",
        about: "Modelo pequeno e rápido da Meta, com suporte oficial a português.",
        why: "é o modelo mais leve da lista, responde rápido durante a reunião e deixa memória para a transcrição",
        download_bytes: 2_019_392_628,
        memory_mb: ollama_memory_mb(2_019_392_628, 4_096),
        min_ram_gb: 8,
        coverage: Coverage::Multilingual,
        rank_pt: 40,
        rank_other: 40,
        needs_apple_gpu: false,
        license: "Llama 3.2 Community License",
        license_note: Some("Uso comercial permitido, com política de uso aceitável da Meta."),
        source: "https://ollama.com/library/llama3.2",
        revision: None,
        files: &[],
    },
    CatalogModel {
        id: "granite4.2:3b",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Granite 4.2 3B",
        about: "Modelo pequeno da IBM para classificação, extração e resumo, com português entre os idiomas oficiais.",
        why: "é pequeno e voltado a classificar, extrair e resumir, com português entre os idiomas oficiais",
        download_bytes: 2_244_023_548,
        memory_mb: ollama_memory_mb(2_244_023_548, 4_096),
        min_ram_gb: 8,
        coverage: Coverage::Multilingual,
        rank_pt: 38,
        rank_other: 38,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://ollama.com/library/granite4.2",
        revision: None,
        files: &[],
    },
    CatalogModel {
        id: "qwen3.5:4b",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Qwen 3.5 4B",
        about: "Modelo compacto da Alibaba, com 201 idiomas e dialetos.",
        why: "é mais capaz que os modelos de 3 bilhões de parâmetros e ainda cabe ao lado da transcrição",
        download_bytes: 3_324_173_757,
        memory_mb: ollama_memory_mb(3_324_173_757, 4_096),
        min_ram_gb: 12,
        coverage: Coverage::Multilingual,
        rank_pt: 50,
        rank_other: 50,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://ollama.com/library/qwen3.5",
        revision: None,
        files: &[],
    },
    CatalogModel {
        id: "qwen3.5:9b",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Qwen 3.5 9B",
        about: "Modelo médio da Alibaba, com 201 idiomas e dialetos.",
        why: "tem boa qualidade em muitos idiomas e cabe ao lado da transcrição nesta máquina",
        download_bytes: 6_550_825_373,
        memory_mb: ollama_memory_mb(6_550_825_373, 4_096),
        min_ram_gb: 16,
        coverage: Coverage::Multilingual,
        rank_pt: 65,
        rank_other: 65,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://ollama.com/library/qwen3.5",
        revision: None,
        files: &[],
    },
    CatalogModel {
        id: "gemma4:e4b",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Gemma 4 E4B",
        about: "Modelo do Google feito para notebooks, com 128 mil tokens de contexto.",
        why: "foi feito para notebooks, escreve bem em português e cabe ao lado da transcrição nesta máquina",
        download_bytes: 6_583_656_264,
        memory_mb: ollama_memory_mb(6_583_656_264, 4_096),
        min_ram_gb: 16,
        coverage: Coverage::Multilingual,
        rank_pt: 70,
        rank_other: 70,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://ollama.com/library/gemma4",
        revision: None,
        files: &[],
    },
    CatalogModel {
        id: "gemma4:12b",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Gemma 4 12B",
        about: "Modelo médio do Google, com 256 mil tokens de contexto.",
        why: "é o melhor modelo que cabe ao lado da transcrição nesta máquina, com contexto longo para reuniões extensas",
        download_bytes: 8_021_618_699,
        memory_mb: ollama_memory_mb(8_021_618_699, 4_096),
        min_ram_gb: 24,
        coverage: Coverage::Multilingual,
        rank_pt: 80,
        rank_other: 80,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://ollama.com/library/gemma4",
        revision: None,
        files: &[],
    },
    CatalogModel {
        id: "gemma4:26b",
        role: Role::Assistant,
        engine: Engine::Ollama,
        name: "Gemma 4 26B",
        about: "Modelo grande do Google, para máquinas com muita memória.",
        why: "é o modelo mais capaz da lista e esta máquina tem memória para rodá-lo junto com a transcrição",
        download_bytes: 18_731_025_387,
        memory_mb: ollama_memory_mb(18_731_025_387, 4_096),
        min_ram_gb: 48,
        coverage: Coverage::Multilingual,
        rank_pt: 90,
        rank_other: 90,
        needs_apple_gpu: false,
        license: "Apache-2.0",
        license_note: None,
        source: "https://ollama.com/library/gemma4",
        revision: None,
        files: &[],
    },
];

/// The context Ollama is assumed to give a model when the app does not ask
/// for one: 4,096 tokens below 24 GB of memory, 16,384 from there on (what
/// Ollama 0.40 chose on a 24 GB Mac).
pub const fn ollama_context_tokens(ram_gb: u32) -> u32 {
    if ram_gb < 24 {
        4_096
    } else {
        16_384
    }
}

const fn isqrt(value: u64) -> u64 {
    let mut root = 0;
    while (root + 1) * (root + 1) <= value {
        root += 1;
    }
    root
}

/// Weights plus 5%, plus the context: 140 MB per 1,024 tokens for a 2 GB
/// model, growing with the square root of the model size.
pub const fn ollama_memory_mb(download_bytes: u64, context_tokens: u32) -> u32 {
    let megabytes = download_bytes / 1_000_000;
    let per_1024_tokens = 140 * isqrt(megabytes * 10_000 / 2_000) / 100;
    (megabytes * 105 / 100 + per_1024_tokens * context_tokens as u64 / 1_024) as u32
}

impl CatalogModel {
    /// Estimated memory in use on a machine with `ram_gb` of memory.
    pub fn memory_mb_on(&self, ram_gb: u32) -> u32 {
        match self.engine {
            Engine::Ollama => ollama_memory_mb(self.download_bytes, ollama_context_tokens(ram_gb)),
            _ => self.memory_mb,
        }
    }
}

pub fn find(role: Role, id: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|model| model.role == role && model.id == id)
}

pub fn of_role(role: Role) -> impl Iterator<Item = &'static CatalogModel> {
    CATALOG.iter().filter(move |model| model.role == role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_inside_each_role_and_every_role_has_a_model_for_8_gb() {
        for role in ROLES {
            let ids: Vec<&str> = of_role(role).map(|model| model.id).collect();
            assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len(), "{role:?}");
            assert!(of_role(role).any(|model| model.min_ram_gb <= 8 && model.rank_pt > 0), "{role:?}");
        }
    }

    #[test]
    fn the_live_help_model_is_the_lightest_assistant_and_the_default_of_the_live_help() {
        let live = find(Role::Assistant, LIVE_HELP_MODEL).expect("in the catalog");
        assert!(of_role(Role::Assistant).all(|model| model.download_bytes >= live.download_bytes));
        assert_eq!(live.min_ram_gb, 8);
        assert_eq!(LIVE_HELP_MODEL, crate::live_assist::DEFAULT_LIVE_MODEL);
    }

    #[test]
    fn pinned_files_have_a_sha256_and_add_up_to_the_download_size() {
        for model in CATALOG {
            assert!(model.download_bytes > 0 && model.memory_mb > 0, "{}", model.id);
            assert!(model.source.starts_with("https://"), "{}", model.id);
            assert!(!model.license.is_empty() && !model.about.contains('—'), "{}", model.id);
            assert!(!model.why.is_empty() && !model.why.contains('—') && !model.why.ends_with('.'), "{}", model.id);
            if model.engine == Engine::Ollama {
                assert!(model.files.is_empty() && model.id.contains(':'), "{}", model.id);
                assert_eq!(model.memory_mb, ollama_memory_mb(model.download_bytes, 4_096));
                assert!(model.memory_mb_on(24) > model.memory_mb_on(16));
                continue;
            }
            assert!(!model.files.is_empty(), "{}", model.id);
            let total: u64 = model.files.iter().map(|file| file.bytes).sum();
            assert_eq!(total, model.download_bytes, "{}", model.id);
            for file in model.files {
                assert_eq!(file.sha256.len(), 64, "{} {}", model.id, file.name);
                assert!(file.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            }
            if let Some(revision) = model.revision {
                assert_eq!(revision.len(), 40, "{}", model.id);
            }
        }
    }

    #[test]
    fn the_ollama_estimate_follows_the_measured_points() {
        // llama3.2:latest measured at 2,561 / 3,119 / 4,075 MB.
        let bytes = 2_019_392_628;
        for (context, measured) in [(4_096, 2_561.0), (8_192, 3_119.0), (16_384, 4_075.0)] {
            let estimate = ollama_memory_mb(bytes, context) as f64;
            assert!(estimate >= measured && estimate <= measured * 1.12, "{context}: {estimate}");
        }
    }

    #[test]
    fn parakeet_entries_match_the_engine_catalog_file_by_file() {
        let engine = crate::parakeet_engine::parakeet_engine::catalog_sources();
        for model in CATALOG.iter().filter(|model| model.engine == Engine::Parakeet) {
            let (_, base_url, files) = engine
                .iter()
                .find(|(name, _, _)| *name == model.id)
                .unwrap_or_else(|| panic!("{} is not in the Parakeet engine", model.id));
            let revision = model.revision.expect("pinned");
            assert!(base_url.ends_with(&format!("/resolve/{revision}")), "{}", model.id);
            assert!(base_url.starts_with(model.source), "{}", model.id);
            let pinned: Vec<(&str, u64)> = model.files.iter().map(|file| (file.name, file.bytes)).collect();
            assert_eq!(&pinned, files, "{}", model.id);
        }
    }

    #[test]
    fn whisper_entries_match_the_engine_catalog() {
        for model in CATALOG.iter().filter(|model| model.engine == Engine::Whisper) {
            let (_, filename, size_mb, ..) = crate::config::WHISPER_MODEL_CATALOG
                .iter()
                .find(|(name, ..)| *name == model.id)
                .unwrap_or_else(|| panic!("{} is not in the Whisper catalog", model.id));
            assert_eq!(model.files.len(), 1);
            assert_eq!(model.files[0].name, *filename);
            // The engine treats a file under 90% of `size_mb` MiB as corrupted.
            let mib = model.download_bytes / (1024 * 1024);
            assert!(mib >= (*size_mb as u64) * 9 / 10 && mib <= *size_mb as u64 + 1, "{}: {mib} MiB vs {size_mb}", model.id);
        }
    }

    #[test]
    fn voice_entries_match_the_speaker_module() {
        for model in CATALOG.iter().filter(|model| model.engine == Engine::Voice) {
            let spec = crate::speakers::neural::spec_by_id(model.id)
                .unwrap_or_else(|| panic!("{} is not a voice model", model.id));
            assert_eq!(model.files.len(), 1);
            assert_eq!(model.files[0].bytes, spec.bytes, "{}", model.id);
            assert_eq!(model.files[0].sha256, spec.sha256, "{}", model.id);
            assert!(spec.url.ends_with(model.files[0].name), "{}", model.id);
            if let Some(revision) = model.revision {
                assert!(spec.url.contains(revision), "{}", model.id);
            }
        }
        assert_eq!(
            CATALOG.iter().filter(|model| model.engine == Engine::Voice).count(),
            crate::speakers::neural::VOICE_MODELS.len()
        );
    }
}
