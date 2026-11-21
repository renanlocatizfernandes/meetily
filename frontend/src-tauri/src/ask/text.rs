//! Question analysis: search terms, FTS5 query, intent and period hints.
//! Pure functions, no database and no model.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};

use crate::organize::fold;

const STOPWORDS: &[&str] = &[
    "a", "o", "as", "os", "um", "uma", "uns", "umas", "de", "do", "da", "dos", "das", "em", "no", "na", "nos", "nas",
    "por", "para", "pra", "com", "sem", "sobre", "entre", "ate", "e", "ou", "mas", "que", "qual", "quais", "quem",
    "quando", "onde", "como", "quanto", "quantos", "quanta", "quantas", "porque", "foi", "foram", "era", "eram", "ser",
    "sao", "esta", "estao", "tem", "tinha", "tive", "teve", "temos", "ficou", "ficaram", "fica", "tinham", "ha", "isso",
    "isto", "esse", "essa", "esses", "essas", "este", "esta", "estes", "estas", "aquele", "aquela", "meu", "minha",
    "meus", "minhas", "seu", "sua", "seus", "suas", "nosso", "nossa", "nossos", "nossas", "me", "te", "se", "nos",
    "lhe", "eu", "voce", "ele", "ela", "eles", "elas", "nos", "ja", "mais", "menos", "muito", "pouco", "todo",
    "toda", "todos", "todas", "algum", "alguma", "alguns", "algumas", "ao", "aos", "pelo", "pela", "pelos", "pelas",
    "num", "numa", "dele", "dela", "deles", "delas", "falamos", "falou", "falaram", "disse", "disseram", "conversamos",
    "reuniao", "reunioes", "ultima", "ultimo", "ultimas", "ultimos", "nas", "hoje", "ontem", "anteontem", "semana",
    "passada", "passado", "mes", "ano", "recente", "recentes", "dia", "dias", "decidimos", "decidiu", "decidiram",
    "combinamos", "combinou", "combinado", "ficamos", "acordamos", "alguem", "algo", "tudo", "nada", "ainda",
    "depois", "antes", "agora", "so", "tambem", "pode", "podem", "preciso", "precisa", "fazer", "fiz", "fez",
    "the", "of", "and", "is", "to",
];

/// Words that signal the user asks for tasks and follow-ups.
const TASK_WORDS: &[&str] = &[
    "tarefa", "tarefas", "pendencia", "pendencias", "acao", "acoes", "responsavel", "prazo", "prazos", "proximos",
    "passos", "encaminhamento", "encaminhamentos", "todo", "afazeres",
];

/// Words that signal the user asks about decisions.
const DECISION_WORDS: &[&str] = &["decidimos", "decidiu", "decisao", "decisoes", "combinamos", "acordamos", "definimos"];

/// Extra terms added to the search when the question is about tasks.
const TASK_EXPANSION: &[&str] = &["acoes", "tarefas", "proximos passos", "responsavel", "prazo"];
const DECISION_EXPANSION: &[&str] = &["decisao", "decidido", "definido", "aprovado"];

/// Splits folded text in alphanumeric words.
pub fn words(folded: &str) -> Vec<String> {
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Cuts the common Portuguese suffixes so "reuniões" and "reunião", or
/// "contratos" and "contrato", share a root. Conservative: keeps at least 4 letters.
pub fn stem(word: &str) -> String {
    const SUFFIXES: &[&str] = &[
        "mente", "coes", "oes", "ando", "endo", "indo", "ados", "adas", "idos", "idas", "amos", "emos", "imos", "ado",
        "ada", "ido", "ida", "ar", "er", "ir", "es", "s",
    ];
    let count = word.chars().count();
    for suffix in SUFFIXES {
        if let Some(root) = word.strip_suffix(suffix) {
            if root.chars().count() >= 4 && count > 4 {
                return root.to_string();
            }
        }
    }
    word.to_string()
}

fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}

/// Search terms of a question: folded, without stopwords, deduplicated, stemmed.
pub fn search_terms(question: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in words(&fold(question)) {
        let numeric = word.chars().all(|c| c.is_ascii_digit());
        if is_stopword(&word) || (word.chars().count() < 3 && !numeric) {
            continue;
        }
        let stemmed = stem(&word);
        if !out.contains(&stemmed) {
            out.push(stemmed);
        }
        if out.len() >= 12 {
            break;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Intent {
    pub tasks: bool,
    pub decisions: bool,
}

pub fn detect_intent(question: &str) -> Intent {
    let ws = words(&fold(question));
    Intent {
        tasks: ws.iter().any(|w| TASK_WORDS.contains(&w.as_str())),
        decisions: ws.iter().any(|w| DECISION_WORDS.contains(&w.as_str())),
    }
}

/// Terms to search for: the question terms plus the intent expansion.
pub fn expanded_terms(question: &str) -> Vec<String> {
    let mut terms = search_terms(question);
    let intent = detect_intent(question);
    let mut extra: Vec<&str> = Vec::new();
    if intent.tasks {
        extra.extend(TASK_EXPANSION);
    }
    if intent.decisions {
        extra.extend(DECISION_EXPANSION);
    }
    for phrase in extra {
        for word in words(phrase) {
            let stemmed = stem(&word);
            if !terms.contains(&stemmed) {
                terms.push(stemmed);
            }
        }
    }
    terms
}

/// FTS5 MATCH expression: every term as a quoted prefix, joined by OR.
/// Terms only contain alphanumerics (quotes cannot appear).
pub fn fts_query(terms: &[String]) -> Option<String> {
    let parts: Vec<String> = terms
        .iter()
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric()))
        .map(|t| format!("\"{}\"*", t))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" OR "))
    }
}

/// An inclusive start and an exclusive end, in UTC.
#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub label: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

fn local_midnight(date: NaiveDate) -> DateTime<Utc> {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap_or_default())
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap_or_default()))
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    let back = date.weekday().num_days_from_monday() as i64;
    date - Duration::days(back)
}

/// Period words understood in the question ("ontem", "semana passada"...).
pub fn detect_period(question: &str, today: NaiveDate) -> Option<Period> {
    // Whole words only: "contemplam" contains "ontem" and must not mean yesterday.
    let padded = format!(" {} ", words(&fold(question)).join(" "));
    let has = |needle: &str| padded.contains(&format!(" {needle} "));
    let day_range = |label: &str, first: NaiveDate, last_inclusive: NaiveDate| Period {
        label: label.to_string(),
        from: local_midnight(first),
        to: local_midnight(last_inclusive + Duration::days(1)),
    };
    if has("anteontem") {
        let d = today - Duration::days(2);
        return Some(day_range("anteontem", d, d));
    }
    if has("ontem") {
        let d = today - Duration::days(1);
        return Some(day_range("ontem", d, d));
    }
    if has("hoje") {
        return Some(day_range("hoje", today, today));
    }
    if has("semana passada") || has("semana anterior") {
        let monday = monday_of(today) - Duration::days(7);
        return Some(day_range("semana passada", monday, monday + Duration::days(6)));
    }
    if has("esta semana") || has("essa semana") || has("nesta semana") || has("na semana") {
        let monday = monday_of(today);
        return Some(day_range("esta semana", monday, today));
    }
    if has("mes passado") {
        let first_this = today.with_day(1)?;
        let last_prev = first_this - Duration::days(1);
        let first_prev = last_prev.with_day(1)?;
        return Some(day_range("mês passado", first_prev, last_prev));
    }
    if has("este mes") || has("esse mes") || has("neste mes") {
        return Some(day_range("este mês", today.with_day(1)?, today));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_drop_stopwords_and_stem() {
        let terms = search_terms("O que decidimos com o cliente Acme sobre os contratos?");
        assert_eq!(terms, vec!["cliente", "acme", "contrato"]);
    }

    #[test]
    fn accents_are_folded() {
        assert_eq!(search_terms("Reunião de orçamento"), search_terms("reuniao de orcamento"));
        assert!(search_terms("orçamento").contains(&"orcamento".to_string()));
    }

    #[test]
    fn plural_and_singular_share_a_root() {
        assert_eq!(search_terms("contratos"), search_terms("contrato"));
        assert_eq!(stem("tarefa"), "tarefa");
        assert_eq!(stem("sol"), "sol");
    }

    #[test]
    fn numbers_are_kept() {
        assert!(search_terms("proposta de 2026").contains(&"2026".to_string()));
    }

    #[test]
    fn intent_adds_expansion_terms() {
        let intent = detect_intent("Quais tarefas estão comigo?");
        assert!(intent.tasks && !intent.decisions);
        let terms = expanded_terms("Quais tarefas estão comigo?");
        assert!(terms.contains(&"prazo".to_string()));
        assert!(terms.contains(&"passo".to_string()));
        assert!(detect_intent("o que decidimos?").decisions);
    }

    #[test]
    fn fts_query_is_quoted_prefix_or() {
        let query = fts_query(&["acme".into(), "contrat".into()]).unwrap();
        assert_eq!(query, "\"acme\"* OR \"contrat\"*");
        assert!(fts_query(&[]).is_none());
        assert!(fts_query(&["a\"b".into()]).is_none());
    }

    #[test]
    fn period_words() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(); // sexta
        let p = detect_period("o que combinamos ontem?", today).unwrap();
        assert_eq!(p.label, "ontem");
        assert_eq!(p.to - p.from, Duration::days(1));
        let week = detect_period("na semana passada", today).unwrap();
        assert_eq!(week.to - week.from, Duration::days(7));
        assert!(week.to < local_midnight(monday_of(today)) + Duration::hours(1));
        let month = detect_period("mês passado", today).unwrap();
        assert_eq!(month.label, "mês passado");
        assert!(detect_period("sobre o contrato", today).is_none());
        assert!(detect_period("Quais contratos contemplam suporte?", today).is_none());
        assert!(detect_period("um tema contemporâneo", today).is_none());
        assert!(detect_period("O que vimos hoje?", today).is_some());
    }
}
