//! Ranking of the passages found by the text search. Pure and deterministic.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::organize::fold;

use super::chunk::SOURCE_SUMMARY;
use super::model::{Candidate, MeetingMeta};
use super::text::{words, Intent};

pub const MAX_PER_MEETING: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    /// Index in the candidate slice.
    pub index: usize,
    pub score: f64,
}

fn term_matches(word: &str, term: &str) -> bool {
    word.starts_with(term)
}

/// How many terms the meeting metadata mentions (title, client, project, category, tags).
pub fn metadata_hits(meta: &MeetingMeta, terms: &[String]) -> usize {
    let mut haystack = String::new();
    for part in [Some(&meta.title), meta.client.as_ref(), meta.project.as_ref(), meta.category.as_ref()]
        .into_iter()
        .flatten()
    {
        haystack.push_str(&fold(part));
        haystack.push(' ');
    }
    for tag in &meta.tags {
        haystack.push_str(&fold(tag));
        haystack.push(' ');
    }
    let meta_words = words(&haystack);
    terms
        .iter()
        .filter(|term| meta_words.iter().any(|w| term_matches(w, term)))
        .count()
}

/// Scores every candidate for the question terms and returns them best first,
/// at most `MAX_PER_MEETING` per meeting and `limit` in total. Candidates that
/// match no term and whose meeting metadata does not match either are dropped.
pub fn rank(
    candidates: &[Candidate],
    metas: &HashMap<String, MeetingMeta>,
    terms: &[String],
    intent: Intent,
    now: DateTime<Utc>,
    limit: usize,
) -> Vec<Scored> {
    if candidates.is_empty() || terms.is_empty() {
        return Vec::new();
    }
    let n_docs = candidates.len() as f64;
    let doc_words: Vec<Vec<String>> = candidates.iter().map(|c| words(&fold(&c.text))).collect();

    // Document frequency of each term inside the candidate set.
    let df: Vec<f64> = terms
        .iter()
        .map(|term| {
            doc_words
                .iter()
                .filter(|ws| ws.iter().any(|w| term_matches(w, term)))
                .count() as f64
        })
        .collect();
    let avg_len = (doc_words.iter().map(Vec::len).sum::<usize>() as f64 / n_docs).max(1.0);

    let oldest = candidates
        .iter()
        .filter_map(|c| metas.get(&c.meeting_id))
        .map(|m| m.created_at)
        .min()
        .unwrap_or(now);

    let mut scored: Vec<Scored> = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let ws = &doc_words[index];
        let len = ws.len().max(1) as f64;
        let mut score = 0.0;
        let mut matched = 0usize;
        for (t, term) in terms.iter().enumerate() {
            let tf = ws.iter().filter(|w| term_matches(w, term)).count() as f64;
            if tf == 0.0 {
                continue;
            }
            matched += 1;
            let idf = ((n_docs - df[t] + 0.5) / (df[t] + 0.5) + 1.0).ln();
            // BM25 term weight with k1 = 1.2 and b = 0.75.
            score += idf * (tf * 2.2) / (tf + 1.2 * (0.25 + 0.75 * len / avg_len));
        }
        let meta_hits = metas.get(&candidate.meeting_id).map_or(0, |m| metadata_hits(m, terms));
        if matched == 0 && meta_hits == 0 {
            continue;
        }
        // Passages that cover more of the question win over repeated single hits.
        let coverage = matched as f64 / terms.len() as f64;
        score *= 0.6 + 0.8 * coverage;
        // The meeting itself is about what was asked (client, project, tags).
        score += meta_hits as f64 * 0.8;
        if candidate.source == SOURCE_SUMMARY {
            score *= if intent.tasks || intent.decisions { 1.35 } else { 1.1 };
        }
        // Mild preference for recent meetings: up to +10%.
        if let Some(meta) = metas.get(&candidate.meeting_id) {
            let span = (now - oldest).num_seconds().max(1) as f64;
            let age = (now - meta.created_at).num_seconds().max(0) as f64;
            score *= 1.0 + 0.1 * (1.0 - (age / span).clamp(0.0, 1.0));
        }
        if score > 0.0 {
            scored.push(Scored { index, score });
        }
    }
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.index.cmp(&b.index))
    });

    let mut per_meeting: HashMap<&str, usize> = HashMap::new();
    let mut seen_text: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for item in scored {
        let candidate = &candidates[item.index];
        let count = per_meeting.entry(candidate.meeting_id.as_str()).or_insert(0);
        if *count >= MAX_PER_MEETING {
            continue;
        }
        // Drops a passage that repeats one already picked.
        let key: String = fold(&candidate.text)
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(80)
            .collect();
        if !seen_text.insert(key) {
            continue;
        }
        *count += 1;
        out.push(item);
        if out.len() >= limit {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cand(id: i64, meeting: &str, source: &str, text: &str) -> Candidate {
        Candidate {
            id,
            meeting_id: meeting.into(),
            source: source.into(),
            start_sec: Some(0.0),
            text: text.into(),
        }
    }

    fn meta(id: &str, client: Option<&str>, day: u32) -> MeetingMeta {
        MeetingMeta {
            id: id.into(),
            title: format!("Reunião {id}"),
            client: client.map(str::to_string),
            project: None,
            category: None,
            tags: vec![],
            created_at: Utc.with_ymd_and_hms(2026, 11, day, 12, 0, 0).unwrap(),
        }
    }

    fn metas(items: Vec<MeetingMeta>) -> HashMap<String, MeetingMeta> {
        items.into_iter().map(|m| (m.id.clone(), m)).collect()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap()
    }

    fn terms(list: &[&str]) -> Vec<String> {
        list.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn more_covered_terms_rank_first() {
        let cands = vec![
            cand(1, "a", "transcript", "Falamos do prazo da entrega e nada mais."),
            cand(2, "b", "transcript", "O contrato da Acme tem prazo de quinze dias para assinatura."),
        ];
        let m = metas(vec![meta("a", None, 10), meta("b", None, 10)]);
        let out = rank(&cands, &m, &terms(&["contrato", "acme", "prazo"]), Intent::default(), now(), 5);
        assert_eq!(out[0].index, 1);
    }

    #[test]
    fn accents_and_prefix_match() {
        let cands = vec![cand(1, "a", "transcript", "Definimos o orçamento anual")];
        let m = metas(vec![meta("a", None, 10)]);
        let out = rank(&cands, &m, &terms(&["orcamento"]), Intent::default(), now(), 5);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn unrelated_passages_are_dropped() {
        let cands = vec![cand(1, "a", "transcript", "Bom dia a todos")];
        let m = metas(vec![meta("a", None, 10)]);
        assert!(rank(&cands, &m, &terms(&["acme"]), Intent::default(), now(), 5).is_empty());
        assert!(rank(&[], &m, &terms(&["acme"]), Intent::default(), now(), 5).is_empty());
    }

    #[test]
    fn client_in_metadata_pulls_the_meeting_in() {
        let cands = vec![cand(1, "a", "transcript", "Ficou combinado revisar o escopo na sexta.")];
        let m = metas(vec![meta("a", Some("Acme"), 10)]);
        let out = rank(&cands, &m, &terms(&["acme", "escopo"]), Intent::default(), now(), 5);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn caps_per_meeting_and_total() {
        let cands: Vec<Candidate> = (0..6)
            .map(|i| cand(i, "a", "transcript", &format!("preço do plano variante {i}")))
            .chain(std::iter::once(cand(9, "b", "transcript", "preço do plano em outra reunião")))
            .collect();
        let m = metas(vec![meta("a", None, 10), meta("b", None, 11)]);
        let out = rank(&cands, &m, &terms(&["preco"]), Intent::default(), now(), 10);
        assert_eq!(out.iter().filter(|s| cands[s.index].meeting_id == "a").count(), MAX_PER_MEETING);
        assert_eq!(out.len(), MAX_PER_MEETING + 1);
        let limited = rank(&cands, &m, &terms(&["preco"]), Intent::default(), now(), 2);
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn summary_wins_for_task_questions() {
        let cands = vec![
            cand(1, "a", "transcript", "tarefa de enviar a proposta ficou com Ana ontem"),
            cand(2, "b", "summary", "tarefa de enviar a proposta ficou com Beto hoje"),
        ];
        let m = metas(vec![meta("a", None, 10), meta("b", None, 10)]);
        let intent = Intent { tasks: true, decisions: false };
        let out = rank(&cands, &m, &terms(&["tarefa", "proposta"]), intent, now(), 5);
        assert_eq!(out[0].index, 1);
    }
}
