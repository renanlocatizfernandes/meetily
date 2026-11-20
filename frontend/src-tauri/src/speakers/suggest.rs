//! Suggests who is who from what people say ("meu nome é Ana", "obrigada,
//! Maria"). Everything is a suggestion the user confirms: nothing here renames
//! a speaker, and nothing logs transcript content.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use sqlx::Row;
use tauri::Manager;

use crate::database::repositories::transcript::TranscriptsRepository;

/// One transcript segment in order, as the suggester needs it.
#[derive(Debug, Clone)]
pub struct Turn {
    pub speaker_key: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpeakerNameSuggestion {
    pub speaker_key: String,
    /// Current label, e.g. "Pessoa 2".
    pub label: String,
    pub name: String,
    /// 0.0 to 1.0.
    pub confidence: f32,
    /// Short explanation in pt-BR for the confirmation prompt.
    pub reason: String,
}

const NAME: &str = r"(\p{Lu}[\p{L}]{1,19})";
const MIN_CONFIDENCE: f32 = 0.4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Evidence {
    Introduced,
    Thanked,
    Addressed,
}

impl Evidence {
    fn reason(self) -> &'static str {
        match self {
            Evidence::Introduced => "Disse o próprio nome",
            Evidence::Thanked => "Foi agradecida(o) pelo nome logo depois de falar",
            Evidence::Addressed => "Foi chamada(o) pelo nome antes de responder",
        }
    }
}

static INTRODUCED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i:meu nome (?:completo )?(?:é|e)|me chamo|aqui (?:é|e) (?:o|a)|aqui quem fala (?:é|e)|meu nome:)\s+{NAME}"
    ))
    .unwrap()
});
static INTRODUCED_WEAK: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"(?i:\bsou (?:o|a))\s+{NAME}")).unwrap());
static THANKED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"(?i:obrigad[oa]|valeu|agrade[cç]o)[,\s]+{NAME}")).unwrap()
});
static ADDRESSED_AFTER_GREETING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i:\boi|\bol[aá]|bom dia|boa tarde|boa noite|\bfala|\be a[ií]|vamos l[aá]|tudo bem)[,\s]+{NAME}"
    ))
    .unwrap()
});
static ADDRESSED_QUESTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?:^|[.!?]\s+){NAME},\s+(?i:o que|como|voc[eê]|pode|poderia|qual|quer|tem|consegue|me conta|vamos)"
    ))
    .unwrap()
});

/// Capitalized words that are not names.
const NOT_NAMES: &[&str] = &[
    "bom", "boa", "bem", "oi", "olá", "ola", "obrigado", "obrigada", "então", "entao", "tudo",
    "você", "voce", "pessoal", "gente", "sim", "não", "nao", "ok", "eu", "ele", "ela", "senhor",
    "senhora", "doutor", "doutora", "dr", "dra", "time", "equipe", "galera", "todos", "todas",
    "claro", "certo", "pois", "mas", "agora", "hoje", "ontem", "amanhã", "amanha", "aqui", "lá",
    "la", "bora", "valeu", "legal", "ótimo", "otimo", "perfeito", "desculpa", "desculpe", "por",
    "para", "com", "que", "como", "quando", "onde", "porque", "sobre", "também", "tambem",
];

fn plausible_name(word: &str) -> bool {
    !NOT_NAMES.contains(&word.to_lowercase().as_str())
}

fn names_in<'a>(re: &'a Regex, text: &'a str) -> impl Iterator<Item = String> + 'a {
    re.captures_iter(text)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .filter(|name| plausible_name(name))
}

/// Compares speaker keys; a missing neighbour (no different speaker around)
/// yields no vote.
fn previous_other(turns: &[Turn], index: usize) -> Option<&Turn> {
    turns[..index].iter().rev().find(|t| t.speaker_key != turns[index].speaker_key)
}

fn next_other(turns: &[Turn], index: usize) -> Option<&Turn> {
    turns[index + 1..].iter().find(|t| t.speaker_key != turns[index].speaker_key)
}

/// Suggests names for speakers whose label is still the default "Pessoa N".
/// `already_named` holds the keys the user has already renamed.
pub fn suggest(turns: &[Turn], already_named: &dyn Fn(&str) -> bool) -> Vec<SpeakerNameSuggestion> {
    let eligible = |turn: &Turn| turn.label.starts_with("Pessoa") && !already_named(&turn.speaker_key);

    // (speaker_key, name) -> (confidence so far, strongest evidence, strongest weight)
    let mut votes: HashMap<(String, String), (f32, Evidence, f32)> = HashMap::new();
    let mut vote = |turn: &Turn, name: String, weight: f32, evidence: Evidence| {
        if !eligible(turn) {
            return;
        }
        let entry = votes
            .entry((turn.speaker_key.clone(), name))
            .or_insert((0.0, evidence, 0.0));
        entry.0 = 1.0 - (1.0 - entry.0) * (1.0 - weight);
        if weight > entry.2 {
            entry.1 = evidence;
            entry.2 = weight;
        }
    };

    for (i, turn) in turns.iter().enumerate() {
        for name in names_in(&INTRODUCED, &turn.text) {
            vote(turn, name, 0.9, Evidence::Introduced);
        }
        for name in names_in(&INTRODUCED_WEAK, &turn.text) {
            vote(turn, name, 0.55, Evidence::Introduced);
        }
        for name in names_in(&THANKED, &turn.text) {
            if let Some(other) = previous_other(turns, i) {
                vote(other, name, 0.45, Evidence::Thanked);
            }
        }
        for name in names_in(&ADDRESSED_AFTER_GREETING, &turn.text)
            .chain(names_in(&ADDRESSED_QUESTION, &turn.text))
        {
            if let Some(other) = next_other(turns, i) {
                vote(other, name, 0.45, Evidence::Addressed);
            }
        }
    }

    // Best name per speaker, reduced when a second name competes closely.
    let mut per_speaker: HashMap<&str, Vec<(&str, f32, Evidence)>> = HashMap::new();
    for ((key, name), (confidence, evidence, _)) in &votes {
        per_speaker.entry(key).or_default().push((name, *confidence, *evidence));
    }
    let mut chosen: Vec<(String, String, f32, Evidence)> = Vec::new();
    for (key, mut names) in per_speaker {
        names.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let (name, mut confidence, evidence) = names[0];
        if let Some(second) = names.get(1) {
            if confidence - second.1 < 0.15 {
                confidence -= 0.2;
            }
        }
        if confidence >= MIN_CONFIDENCE {
            chosen.push((key.to_string(), name.to_string(), confidence, evidence));
        }
    }

    // One name belongs to one speaker: the most confident wins.
    chosen.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    let mut used_names: Vec<String> = Vec::new();
    let mut result = Vec::new();
    for (key, name, confidence, evidence) in chosen {
        let lowered = name.to_lowercase();
        if used_names.contains(&lowered) {
            continue;
        }
        used_names.push(lowered);
        let label = turns
            .iter()
            .find(|t| t.speaker_key == key)
            .map(|t| t.label.clone())
            .unwrap_or_default();
        result.push(SpeakerNameSuggestion {
            speaker_key: key,
            label,
            name,
            confidence: (confidence.min(0.95) * 100.0).round() / 100.0,
            reason: evidence.reason().to_string(),
        });
    }
    result.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.speaker_key.cmp(&b.speaker_key)));
    result
}

/// Reads a saved meeting and suggests names for its unnamed speakers.
#[tauri::command]
pub async fn api_suggest_speaker_names(
    app: tauri::AppHandle<tauri::Wry>,
    meeting_id: String,
) -> Result<Vec<SpeakerNameSuggestion>, String> {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return Ok(Vec::new());
    };
    let pool = state.db_manager.pool();
    let rows = sqlx::query(
        "SELECT transcript, speaker, speaker_key FROM transcripts
         WHERE meeting_id = ? AND speaker_key IS NOT NULL
         ORDER BY audio_start_time IS NULL, audio_start_time, rowid",
    )
    .bind(&meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "Não foi possível ler a transcrição.".to_string())?;
    let aliases = TranscriptsRepository::speaker_aliases(pool, &meeting_id)
        .await
        .unwrap_or_default();
    let turns: Vec<Turn> = rows
        .iter()
        .filter_map(|row| {
            let text: String = row.try_get("transcript").ok()?;
            let speaker_key: String = row.try_get("speaker_key").ok()?;
            let label: String = row.try_get::<Option<String>, _>("speaker").ok().flatten().unwrap_or_default();
            Some(Turn { speaker_key, label, text })
        })
        .collect();
    // Matching is pure CPU over the whole transcript: keep it off the async workers.
    tokio::task::spawn_blocking(move || suggest(&turns, &|key| aliases.contains_key(key)))
        .await
        .map_err(|_| "Não foi possível analisar a transcrição.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(key: &str, label: &str, text: &str) -> Turn {
        Turn { speaker_key: key.into(), label: label.into(), text: text.into() }
    }
    fn none_named(_: &str) -> bool {
        false
    }

    #[test]
    fn self_introduction_names_the_speaker() {
        let turns = [
            turn("a", "Pessoa 1", "Bom dia a todos, meu nome é Carla e vou apresentar."),
            turn("b", "Pessoa 2", "Bom dia."),
        ];
        let out = suggest(&turns, &none_named);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].speaker_key, "a");
        assert_eq!(out[0].name, "Carla");
        assert!(out[0].confidence >= 0.85);
    }

    #[test]
    fn thanks_name_the_previous_speaker() {
        let turns = [
            turn("a", "Pessoa 1", "O relatório está pronto e enviado."),
            turn("b", "Pessoa 2", "Obrigado, Maria. Vamos ao próximo item."),
        ];
        let out = suggest(&turns, &none_named);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].speaker_key, "a");
        assert_eq!(out[0].name, "Maria");
    }

    #[test]
    fn question_to_a_name_goes_to_the_next_speaker() {
        let turns = [
            turn("a", "Pessoa 1", "Pedro, o que você acha do prazo?"),
            turn("b", "Pessoa 2", "Acho apertado."),
        ];
        let out = suggest(&turns, &none_named);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].speaker_key, "b");
        assert_eq!(out[0].name, "Pedro");
    }

    #[test]
    fn repeated_evidence_raises_confidence() {
        let one = [
            turn("a", "Pessoa 1", "Pronto."),
            turn("b", "Pessoa 2", "Obrigado, Maria."),
        ];
        let two = [
            turn("a", "Pessoa 1", "Pronto."),
            turn("b", "Pessoa 2", "Obrigado, Maria."),
            turn("a", "Pessoa 1", "De nada."),
            turn("b", "Pessoa 2", "Valeu, Maria."),
        ];
        assert!(suggest(&two, &none_named)[0].confidence > suggest(&one, &none_named)[0].confidence);
    }

    #[test]
    fn already_named_or_custom_labels_get_no_suggestion() {
        let turns = [
            turn("a", "Ana", "Meu nome é Carla."),
            turn("b", "Pessoa 2", "Meu nome é Davi."),
            turn("c", "Você", "Meu nome é Rui."),
        ];
        let out = suggest(&turns, &|key| key == "b");
        assert!(out.is_empty());
    }

    #[test]
    fn common_capitalized_words_are_not_names() {
        let turns = [
            turn("a", "Pessoa 1", "Olá, Pessoal. Obrigado, Gente."),
            turn("b", "Pessoa 2", "Bom dia, Então vamos."),
        ];
        assert!(suggest(&turns, &none_named).is_empty());
    }

    #[test]
    fn a_name_is_suggested_for_one_speaker_only() {
        let turns = [
            turn("a", "Pessoa 1", "Meu nome é Ana."),
            turn("b", "Pessoa 2", "Obrigado, Ana."),
        ];
        let out = suggest(&turns, &none_named);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].speaker_key, "a");
    }

    #[test]
    fn conflicting_names_lower_confidence() {
        let clean = [turn("a", "Pessoa 1", "Oi."), turn("b", "Pessoa 2", "Obrigado, Lia.")];
        let mixed = [
            turn("a", "Pessoa 1", "Oi."),
            turn("b", "Pessoa 2", "Obrigado, Lia."),
            turn("c", "Pessoa 3", "Valeu, Bia."),
            turn("a", "Pessoa 1", "Tchau."),
        ];
        let c = suggest(&clean, &none_named);
        let m = suggest(&mixed, &none_named);
        assert!(c[0].confidence >= 0.4);
        assert!(m.iter().all(|s| s.confidence >= 0.4));
    }

    #[test]
    fn lowercase_words_after_thanks_do_not_match() {
        let turns = [
            turn("a", "Pessoa 1", "Falei tudo."),
            turn("b", "Pessoa 2", "Obrigado a todos, valeu pela ajuda."),
        ];
        assert!(suggest(&turns, &none_named).is_empty());
    }
}
