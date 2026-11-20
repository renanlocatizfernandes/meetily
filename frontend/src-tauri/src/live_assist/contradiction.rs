//! Possible contradiction between what was just said and the notes of earlier
//! meetings already loaded in memory. Local text work: a note and a sentence
//! about the same subject (shared content words) that carry figures (numbers or
//! weekdays) with nothing in common. It says "possível divergência", never a
//! verdict, and it costs no tokens.

use std::collections::BTreeSet;

use super::context::{content_tokens, MemoryNote};

/// Sentences shorter than this carry too little to compare.
const MIN_WORDS: usize = 6;
/// Content words shared with the note, figures aside.
const MIN_SHARED_WORDS: usize = 2;
const WEEKDAYS: &[&str] = &[
    "segunda", "terca", "quarta", "quinta", "sexta", "sabado", "domingo", "monday", "tuesday", "wednesday",
    "thursday", "friday", "saturday", "sunday",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// "reunião de 12/11".
    pub origin: String,
    pub remembered: String,
}

fn fold_text(text: &str) -> String {
    text.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Numbers (with "mil" scaling and thousand separators removed) and weekdays.
pub fn figures(text: &str) -> BTreeSet<String> {
    let folded = fold_text(text);
    let mut out = BTreeSet::new();
    let words: Vec<&str> = folded.split_whitespace().collect();
    for (index, raw) in words.iter().enumerate() {
        let word = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != ',' && c != '.');
        let plain = word.trim_matches(|c: char| !c.is_alphanumeric());
        if WEEKDAYS.contains(&plain.trim_end_matches("-feira")) {
            out.insert(plain.trim_end_matches("-feira").to_string());
            continue;
        }
        if !word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // "12/11" and "10h30" stay as one token each.
        if word.contains('/') || word.chars().any(|c| c.is_alphabetic()) {
            out.insert(word.to_string());
            continue;
        }
        // Thousand separators: "12.000" -> "12000"; a decimal comma stays.
        let digits: String = word.replace('.', "");
        let digits = digits.trim_matches(',').to_string();
        if digits.is_empty() {
            continue;
        }
        let scaled = match words.get(index + 1).map(|next| next.trim_matches(|c: char| !c.is_alphanumeric())) {
            Some("mil") => digits.parse::<f64>().ok().map(|value| format!("{}", (value * 1000.0).round() as i64)),
            Some("milhao") | Some("milhoes") => {
                digits.replace(',', ".").parse::<f64>().ok().map(|value| format!("{}", (value * 1_000_000.0).round() as i64))
            }
            _ => None,
        };
        out.insert(scaled.unwrap_or(digits));
    }
    out
}

fn words_without_figures(text: &str) -> std::collections::HashSet<String> {
    content_tokens(text)
        .into_iter()
        .filter(|word| !word.chars().all(|c| c.is_ascii_digit()) && !WEEKDAYS.contains(&word.as_str()))
        .collect()
}

/// The note that most plausibly disagrees with the sentence, if any.
pub fn find_divergence(sentence: &str, notes: &[MemoryNote]) -> Option<Divergence> {
    if sentence.split_whitespace().count() < MIN_WORDS {
        return None;
    }
    let said_figures = figures(sentence);
    if said_figures.is_empty() {
        return None;
    }
    let said_words = words_without_figures(sentence);
    let mut best: Option<(usize, &MemoryNote)> = None;
    for note in notes {
        let note_figures = figures(&note.text);
        if note_figures.is_empty() || !note_figures.is_disjoint(&said_figures) {
            continue;
        }
        let shared = said_words.intersection(&words_without_figures(&note.text)).count();
        if shared < MIN_SHARED_WORDS {
            continue;
        }
        // Same kind of figure only: a date against a date, a number against a number.
        let said_has_weekday = said_figures.iter().any(|f| WEEKDAYS.contains(&f.as_str()));
        let note_has_weekday = note_figures.iter().any(|f| WEEKDAYS.contains(&f.as_str()));
        let said_has_number = said_figures.iter().any(|f| f.chars().next().is_some_and(|c| c.is_ascii_digit()));
        let note_has_number = note_figures.iter().any(|f| f.chars().next().is_some_and(|c| c.is_ascii_digit()));
        if !((said_has_weekday && note_has_weekday) || (said_has_number && note_has_number)) {
            continue;
        }
        if best.map_or(true, |(score, _)| shared > score) {
            best = Some((shared, note));
        }
    }
    best.map(|(_, note)| Divergence {
        origin: note.origin_label(),
        remembered: note.text.chars().take(160).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_assist::context::NoteKind;

    fn note(text: &str) -> MemoryNote {
        MemoryNote {
            meeting_id: "m1".into(),
            meeting_title: "Renovação Aurora".into(),
            date_label: "12/11".into(),
            kind: NoteKind::Decisao,
            text: text.into(),
            recency: 0,
        }
    }

    #[test]
    fn figures_scale_and_fold() {
        assert!(figures("Fica em R$ 12 mil no total").contains("12000"));
        assert!(figures("O valor é 12.000 reais").contains("12000"));
        assert!(figures("Entrego na Sexta-feira").contains("sexta"));
        assert!(figures("Reunião dia 12/11 às 10h30").contains("12/11"));
        assert!(figures("sem numero nenhum").is_empty());
    }

    #[test]
    fn flags_a_different_price_for_the_same_subject() {
        let notes = vec![note("Orçamento da proposta de renovação fechado em R$ 12 mil")];
        let found = find_divergence("Então o orçamento da proposta de renovação ficou em 15 mil reais", &notes).unwrap();
        assert_eq!(found.origin, "reunião de 12/11");
        assert!(found.remembered.contains("12 mil"));
    }

    #[test]
    fn flags_a_different_weekday() {
        let notes = vec![note("Entrega do relatório mensal combinada para sexta")];
        assert!(find_divergence("Vamos entregar o relatório mensal na segunda sem falta", &notes).is_some());
    }

    #[test]
    fn stays_quiet_when_figures_match_or_subjects_differ() {
        let notes = vec![note("Orçamento da proposta de renovação fechado em R$ 12 mil")];
        assert!(find_divergence("O orçamento da proposta de renovação é de 12 mil mesmo", &notes).is_none());
        assert!(find_divergence("A reunião de equipe do marketing começa às 15 horas hoje", &notes).is_none());
        assert!(find_divergence("Curto demais 15", &notes).is_none());
        assert!(find_divergence("Orçamento da proposta de renovação sem números aqui", &notes).is_none());
    }

    #[test]
    fn a_date_is_not_compared_with_a_price() {
        let notes = vec![note("Orçamento da proposta de renovação fechado em R$ 12 mil")];
        assert!(find_divergence("A proposta de renovação do orçamento sai na sexta", &notes).is_none());
    }
}
