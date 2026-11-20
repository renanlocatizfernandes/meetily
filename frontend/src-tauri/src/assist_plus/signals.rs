//! Local, model-free detectors for the proactive live assistant: a question
//! asked to the owner, a decision and a task. Plain text matching on one
//! transcript segment; nothing here calls the network or a model.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    /// "pergunta", "decisao" or "tarefa".
    pub kind: &'static str,
    pub text: String,
}

const MAX_SIGNALS: usize = 3;
const MAX_TEXT_CHARS: usize = 200;
/// Tag questions that ask nothing ("né?", "certo?").
const TAG_QUESTIONS: &[&str] = &[
    "ne", "certo", "ok", "ta", "tá", "entendeu", "entendi", "beleza", "ne?", "viu", "sim", "ouviu", "tudo bem",
    "tudo certo", "pode ser", "combinado", "right", "okay", "yes",
];

const SMALL_TALK_STARTS: &[&str] = &["tudo bem", "tudo certo", "tudo joia", "tudo ótimo", "como vai", "como vão", "bom dia", "boa tarde", "boa noite", "how are you"];

const QUESTION_STARTS: &[&str] = &[
    "qual", "quais", "quando", "onde", "quem", "quanto", "quantos", "quantas", "como", "por que", "por quê", "porque",
    "voce consegue", "você consegue", "voce pode", "você pode", "voce tem", "você tem", "vocês conseguem",
    "vocês têm", "da pra", "dá pra", "dá para", "da para", "podemos", "pode", "podem", "conseguem", "tem como",
    "what", "when", "where", "who", "how", "can you", "could you", "do you", "will you",
];

const DECISION_CUES: &[&str] = &[
    "ficou decidido",
    "ficou definido",
    "ficou combinado",
    "fica combinado",
    "fica decidido",
    "decidimos",
    "definimos",
    "combinamos",
    "fechamos",
    "fechado entao",
    "fechado então",
    "esta aprovado",
    "está aprovado",
    "foi aprovado",
    "vamos seguir com",
    "vamos com a opcao",
    "vamos com a opção",
    "vamos adotar",
    "a decisao e",
    "a decisão é",
    "we decided",
    "we agreed",
    "it's decided",
];

const TASK_CUES: &[&str] = &[
    "preciso que voce",
    "preciso que você",
    "precisamos que voce",
    "precisamos que você",
    "fica responsavel",
    "fica responsável",
    "fica a seu cargo",
    "fica com voce",
    "fica com você",
    "voce fica de",
    "você fica de",
    "voce ficou de",
    "você ficou de",
    "proximo passo",
    "próximo passo",
    "proximos passos",
    "próximos passos",
    "nao esquece de",
    "não esquece de",
    "nao esqueca de",
    "não esqueça de",
    "anota ai",
    "anota aí",
    "item de acao",
    "item de ação",
    "next step",
    "action item",
    "can you take",
];

/// Detects signals in one final segment. `from_other` is true for the other
/// participants (system audio); questions and tasks only matter when someone
/// else addressed the owner, while a decision matters whoever said it.
pub fn detect_signals(text: &str, from_other: bool) -> Vec<Signal> {
    let mut found: Vec<Signal> = Vec::new();
    for sentence in split_sentences(text) {
        let trimmed = sentence.trim();
        if trimmed.chars().count() < 10 {
            continue;
        }
        let lower = trimmed.to_lowercase();
        let kind = if has_cue(&lower, DECISION_CUES) {
            Some("decisao")
        } else if from_other && has_cue(&lower, TASK_CUES) {
            Some("tarefa")
        } else if from_other && is_real_question(trimmed, &lower) {
            Some("pergunta")
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        let text: String = trimmed.chars().take(MAX_TEXT_CHARS).collect();
        if found.iter().any(|signal| signal.text == text) {
            continue;
        }
        found.push(Signal { kind, text });
        if found.len() == MAX_SIGNALS {
            break;
        }
    }
    found
}

/// Splits on sentence ends and keeps the final `?` so the question mark is
/// still there for the question test.
fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '…' | '\n') {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn has_cue(lower: &str, cues: &[&str]) -> bool {
    cues.iter().any(|cue| lower.contains(cue))
}

fn is_real_question(original: &str, lower: &str) -> bool {
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .collect();
    if words.len() < 4 {
        return false;
    }
    if SMALL_TALK_STARTS.iter().any(|start| starts_with_phrase(lower, start)) {
        return false;
    }
    let body = lower.trim_end_matches(|c: char| !c.is_alphanumeric()).trim();
    if TAG_QUESTIONS.iter().any(|tag| body.ends_with(&format!(" {tag}")) && words.len() < 5) {
        return false;
    }
    if original.trim_end().ends_with('?') {
        return true;
    }
    // The transcriber often drops the question mark: an interrogative opening
    // with enough words still counts.
    words.len() >= 5 && QUESTION_STARTS.iter().any(|start| starts_with_phrase(lower, start))
}

fn starts_with_phrase(lower: &str, phrase: &str) -> bool {
    let trimmed = lower.trim_start();
    trimmed.starts_with(phrase)
        && trimmed[phrase.len()..].chars().next().map_or(true, |c| !c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str, from_other: bool) -> Vec<&'static str> {
        detect_signals(text, from_other).into_iter().map(|signal| signal.kind).collect()
    }

    #[test]
    fn finds_a_real_question_to_the_owner() {
        assert_eq!(kinds("Você consegue entregar a proposta até sexta?", true), vec!["pergunta"]);
        assert_eq!(kinds("Qual é o orçamento previsto para o projeto", true), vec!["pergunta"]);
    }

    #[test]
    fn ignores_tag_questions_short_ones_and_the_owner_asking() {
        assert!(kinds("Né?", true).is_empty());
        assert!(kinds("Isso fica claro, certo?", true).is_empty());
        assert!(kinds("Tudo bem por aí?", true).is_empty());
        assert!(kinds("Qual é o prazo do projeto?", false).is_empty());
    }

    #[test]
    fn finds_decisions_from_anyone_and_tasks_only_from_others() {
        assert_eq!(kinds("Ficou decidido que vamos migrar em dezembro.", true), vec!["decisao"]);
        assert_eq!(kinds("Ficou decidido que vamos migrar em dezembro.", false), vec!["decisao"]);
        assert_eq!(kinds("Preciso que você revise o contrato amanhã.", true), vec!["tarefa"]);
        assert!(kinds("Preciso que você revise o contrato amanhã.", false).is_empty());
    }

    #[test]
    fn limits_count_and_length_and_skips_small_talk() {
        let text = "Bom dia pessoal. Ficou decidido A. Ficou decidido B. Ficou decidido C. Ficou decidido D.";
        assert_eq!(detect_signals(text, true).len(), MAX_SIGNALS);
        let long = format!("Ficou decidido {}", "x".repeat(400));
        assert_eq!(detect_signals(&long, true)[0].text.chars().count(), MAX_TEXT_CHARS);
        assert!(detect_signals("Bom dia, tudo bem?", true).is_empty());
    }
}
