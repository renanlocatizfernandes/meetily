//! Pure classification logic: prompt, tolerant JSON parsing, validation and
//! the deterministic fallback used when the model does not answer properly.

use std::collections::HashMap;

use serde_json::Value;

use super::text::{capitalize_first, clamp_chars, clamp_words, count_term, fold, squash};

pub const DEFAULT_CATEGORY: &str = "Outros";
pub const FALLBACK_SUBJECT: &str = "Conversa";
pub const MIN_TRANSCRIPT_WORDS: usize = 40;

const HEAD_CHARS: usize = 6000;
const TAIL_CHARS: usize = 1500;
const SUMMARY_CHARS: usize = 2500;
const CONTEXT_CHARS: usize = 600;
const MAX_NAME_CHARS: usize = 60;
const MAX_SUBJECT_WORDS: usize = 8;
const MAX_SUBJECT_CHARS: usize = 80;
const MAX_TAGS: usize = 4;
const MAX_TAG_CHARS: usize = 24;
const MAX_TAG_WORDS: usize = 3;
pub const MAX_KNOWN_CLIENTS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub client: Option<String>,
    pub project: Option<String>,
    pub subject: String,
    pub category: String,
    pub tags: Vec<String>,
}

/// What the validator needs to know about the meeting and the settings.
#[derive(Debug, Clone, Copy)]
pub struct Rules<'a> {
    pub known_clients: &'a [String],
    pub categories: &'a [String],
    /// Folded transcript + summary + context. A client name that is neither
    /// in the known list nor in this text is treated as invented.
    pub evidence: &'a str,
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

pub const SYSTEM_PROMPT: &str = "Você organiza conversas gravadas de um consultor que atende vários clientes. Responda somente com um objeto JSON válido, sem texto antes ou depois e sem bloco de código.";

pub const RETRY_SUFFIX: &str = "\n\nA resposta anterior não era um objeto JSON válido nesse formato. Responda apenas com o objeto JSON, com o campo \"assunto\" preenchido.";

/// First ~6000 characters plus the last ~1500 of the transcript.
pub fn build_excerpt(transcript: &str) -> String {
    let total = transcript.chars().count();
    if total <= HEAD_CHARS + TAIL_CHARS {
        return transcript.trim().to_string();
    }
    let head: String = transcript.chars().take(HEAD_CHARS).collect();
    let tail: String = transcript.chars().skip(total - TAIL_CHARS).collect();
    format!("{}\n[...]\n{}", head.trim_end(), tail.trim_start())
}

pub struct PromptInput<'a> {
    pub transcript: &'a str,
    pub summary: Option<&'a str>,
    pub context: Option<&'a str>,
    /// "18/11/2026 14:05", local time.
    pub date_label: &'a str,
    pub known_clients: &'a [String],
    pub categories: &'a [String],
}

pub fn build_user_prompt(input: &PromptInput<'_>) -> String {
    let clients = if input.known_clients.is_empty() {
        "nenhum cadastrado".to_string()
    } else {
        input.known_clients.join("; ")
    };
    let categories = input.categories.join("; ");
    let mut prompt = String::new();
    prompt.push_str("Classifique a conversa abaixo.\n\n");
    prompt.push_str(&format!("Data da conversa: {}\n", input.date_label));
    prompt.push_str(&format!("Clientes conhecidos: {}\n", clients));
    prompt.push_str(&format!("Categorias permitidas: {}\n", categories));
    if let Some(context) = input.context.map(str::trim).filter(|value| !value.is_empty()) {
        prompt.push_str(&format!(
            "Contexto informado pelo usuário: {}\n",
            clamp_chars(&squash(context), CONTEXT_CHARS)
        ));
    }
    if let Some(summary) = input.summary.map(str::trim).filter(|value| !value.is_empty()) {
        prompt.push_str(&format!("\nResumo salvo:\n{}\n", clamp_chars(summary, SUMMARY_CHARS)));
    }
    prompt.push_str(&format!("\nTranscrição (trechos):\n{}\n", build_excerpt(input.transcript)));
    prompt.push_str(
        "\nResponda com este JSON e nada mais:\n\
{\"cliente\": string|null, \"projeto\": string|null, \"assunto\": string, \"categoria\": string, \"tags\": string[]}\n\n\
Regras:\n\
- \"cliente\": a empresa ou pessoa com quem a conversa acontece. Se bater com um nome da lista de clientes conhecidos, use exatamente a grafia da lista. Use null se a conversa for interna ou se não estiver claro. Nunca invente um nome que não aparece no texto.\n\
- \"projeto\": nome do projeto ou da frente de trabalho citada, ou null.\n\
- \"assunto\": de 3 a 8 palavras, objetivo, dizendo do que a conversa tratou. Não comece com \"Reunião sobre\", \"Conversa sobre\" nem parecido. Sem data e sem o nome do cliente.\n\
- \"categoria\": exatamente uma das categorias permitidas.\n\
- \"tags\": no máximo 4 palavras-chave curtas, em minúsculas.\n\
- Escreva em português do Brasil.\n",
    );
    prompt
}

// ---------------------------------------------------------------------------
// Parsing and validation
// ---------------------------------------------------------------------------

/// Returns the first balanced `{...}` in the text, ignoring braces inside
/// JSON strings. Code fences and any chatter around the object are skipped.
pub fn extract_json_object(raw: &str) -> Option<&str> {
    let mut search_from = 0;
    while let Some(offset) = raw[search_from..].find('{') {
        let start = search_from + offset;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (index, c) in raw[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let candidate = &raw[start..start + index + 1];
                        if serde_json::from_str::<Value>(candidate).is_ok() {
                            return Some(candidate);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
        search_from = start + 1;
    }
    None
}

const EMPTY_WORDS: &[&str] = &[
    "", "null", "none", "nenhum", "nenhuma", "n/a", "na", "-", "--", "desconhecido", "desconhecida",
    "indefinido", "indefinida", "nao informado", "nao identificado", "nao ha", "nao se aplica",
    "sem cliente", "sem projeto", "interno", "interna", "nao esta claro",
];

const GENERIC_CLIENTS: &[&str] = &[
    "cliente", "o cliente", "empresa", "a empresa", "equipe", "time", "consultor", "usuario",
    "participantes", "voce",
];

fn optional_text(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(text) => squash(text),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    let text = text
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '“' || c == '”')
        .trim()
        .to_string();
    if EMPTY_WORDS.contains(&fold(&text).as_str()) {
        None
    } else {
        Some(text)
    }
}

/// Same spelling as the known list when the name matches one, ignoring case
/// and accents.
pub fn match_known_client(name: &str, known: &[String]) -> Option<String> {
    let wanted = fold(&squash(name));
    if wanted.is_empty() {
        return None;
    }
    known
        .iter()
        .find(|candidate| fold(&squash(candidate)) == wanted)
        .map(|candidate| squash(candidate))
}

/// The known client mentioned most often in the (folded) text.
pub fn find_known_client_in_text(folded_text: &str, known: &[String]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for candidate in known {
        let name = squash(candidate);
        let needle = fold(&name);
        if needle.chars().count() < 2 {
            continue;
        }
        let count = count_term(folded_text, &needle);
        if count > 0 && best.as_ref().map_or(true, |(top, _)| count > *top) {
            best = Some((count, name));
        }
    }
    best.map(|(_, name)| name)
}

/// Adds a client to the known list, unless a case-insensitive match is
/// already there (the first spelling stays). Returns the new list only when
/// it changed.
pub fn merge_known_client(known: &[String], name: &str) -> Option<Vec<String>> {
    let name = clamp_chars(&squash(name), MAX_NAME_CHARS);
    if name.is_empty() || known.len() >= MAX_KNOWN_CLIENTS || match_known_client(&name, known).is_some() {
        return None;
    }
    let mut list = known.to_vec();
    list.push(name);
    Some(list)
}

/// Trims, dedupes (case-insensitive, first spelling wins) and caps a list.
pub fn clean_name_list(values: &[String], max_items: usize, max_chars: usize) -> Vec<String> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for value in values {
        let name = clamp_chars(&squash(value), max_chars);
        let key = fold(&name);
        if name.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(name);
        if out.len() >= max_items {
            break;
        }
    }
    out
}

pub fn default_category(categories: &[String]) -> String {
    let wanted = fold(DEFAULT_CATEGORY);
    categories
        .iter()
        .find(|candidate| fold(candidate) == wanted)
        .or_else(|| categories.last())
        .cloned()
        .unwrap_or_else(|| DEFAULT_CATEGORY.to_string())
}

pub fn normalize_category(raw: Option<&str>, categories: &[String]) -> String {
    let wanted = raw.map(|value| fold(&squash(value))).unwrap_or_default();
    if wanted.is_empty() {
        return default_category(categories);
    }
    categories
        .iter()
        .find(|candidate| fold(&squash(candidate)) == wanted)
        .cloned()
        .unwrap_or_else(|| default_category(categories))
}

pub fn normalize_client(raw: Option<String>, rules: &Rules<'_>) -> Option<String> {
    let name = clamp_chars(&squash(&raw?), MAX_NAME_CHARS);
    if let Some(known) = match_known_client(&name, rules.known_clients) {
        return Some(known);
    }
    let folded = fold(&name);
    if folded.chars().count() < 2
        || GENERIC_CLIENTS.contains(&folded.as_str())
        || rules.categories.iter().any(|category| fold(category) == folded)
    {
        return None;
    }
    // "Never invent": the name has to be somewhere in what was said.
    if count_term(rules.evidence, &folded) == 0 {
        return None;
    }
    Some(name)
}

const FILLER_PREFIXES: &[&[&str]] = &[
    &["reuniao", "sobre"],
    &["reuniao", "de"],
    &["reuniao", "do"],
    &["reuniao", "da"],
    &["reuniao", "para"],
    &["reuniao", "com"],
    &["reuniao", "a", "respeito", "de"],
    &["conversa", "sobre"],
    &["conversa", "de"],
    &["conversa", "a", "respeito", "de"],
    &["call", "sobre"],
    &["call", "de"],
    &["discussao", "sobre"],
    &["discussao", "de"],
    &["encontro", "sobre"],
    &["alinhamento", "sobre"],
    &["bate-papo", "sobre"],
    &["meeting", "about"],
    &["meeting", "on"],
    &["assunto:"],
    &["tema:"],
    &["sobre"],
];

const GENERIC_SUBJECTS: &[&str] = &[
    "reuniao", "conversa", "call", "assunto", "tema", "discussao", "encontro", "meeting",
    "sem assunto", "geral",
];

/// Removes filler ("Reunião sobre…"), trailing punctuation and extra words.
/// `None` when nothing usable is left.
pub fn normalize_subject(raw: &str) -> Option<String> {
    let cleaned = squash(raw);
    let cleaned = cleaned
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '“' || c == '”' || c == '*' || c == '#')
        .trim();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    loop {
        let folded: Vec<String> = words.iter().map(|word| fold(word)).collect();
        let hit = FILLER_PREFIXES.iter().find(|prefix| {
            folded.len() > prefix.len()
                && prefix.iter().zip(folded.iter()).all(|(want, got)| *want == got.as_str())
        });
        match hit {
            Some(prefix) => {
                words.drain(..prefix.len());
            }
            None => break,
        }
    }
    let folded: Vec<String> = words.iter().map(|word| fold(word)).collect();
    if FILLER_PREFIXES
        .iter()
        .any(|prefix| prefix.len() == folded.len() && prefix.iter().zip(folded.iter()).all(|(want, got)| *want == got.as_str()))
    {
        return None;
    }
    let subject = clamp_chars(&clamp_words(&words.join(" "), MAX_SUBJECT_WORDS), MAX_SUBJECT_CHARS);
    let subject = subject
        .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '-' | '·'))
        .trim()
        .to_string();
    let folded = fold(&subject);
    if subject.chars().filter(|c| c.is_alphanumeric()).count() < 3
        || GENERIC_SUBJECTS.contains(&folded.as_str())
        || EMPTY_WORDS.contains(&folded.as_str())
    {
        return None;
    }
    Some(capitalize_first(&subject))
}

pub fn normalize_tags(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let tag = squash(value.trim().trim_start_matches('#')).to_lowercase();
        let tag = tag.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | '"' | '\'')).trim().to_string();
        if tag.is_empty()
            || tag.chars().count() > MAX_TAG_CHARS
            || tag.split_whitespace().count() > MAX_TAG_WORDS
            || out.iter().any(|existing| fold(existing) == fold(&tag))
        {
            continue;
        }
        out.push(tag);
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

fn tags_from_value(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(text)) => text.split([',', ';']).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Parses and validates the model's answer. An error means "ask again".
pub fn parse_classification(raw: &str, rules: &Rules<'_>) -> Result<Classification, String> {
    let json = extract_json_object(raw).ok_or_else(|| "sem objeto JSON".to_string())?;
    let value: Value = serde_json::from_str(json).map_err(|_| "JSON inválido".to_string())?;
    let object = value.as_object().ok_or_else(|| "JSON não é objeto".to_string())?;
    let fields: HashMap<String, &Value> = object.iter().map(|(key, value)| (fold(key), value)).collect();
    let pick = |names: &[&str]| names.iter().find_map(|name| fields.get(*name).copied());

    let subject = optional_text(pick(&["assunto", "subject", "titulo", "title"]))
        .and_then(|text| normalize_subject(&text))
        .ok_or_else(|| "assunto vazio".to_string())?;
    let client = normalize_client(optional_text(pick(&["cliente", "client"])), rules);
    let project = optional_text(pick(&["projeto", "project"]))
        .map(|text| clamp_chars(&text, MAX_NAME_CHARS))
        .filter(|text| !text.is_empty());
    let category = normalize_category(
        pick(&["categoria", "category"]).and_then(Value::as_str),
        rules.categories,
    );
    let tags = normalize_tags(&tags_from_value(pick(&["tags", "etiquetas"])));

    Ok(Classification { client, project, subject, category, tags })
}

// ---------------------------------------------------------------------------
// Deterministic fallback
// ---------------------------------------------------------------------------

const STOPWORDS: &[&str] = &[
    "para", "como", "mais", "mas", "isso", "isto", "essa", "esse", "esta", "este", "aqui", "aquilo",
    "aquele", "aquela", "muito", "muita", "muitos", "pouco", "tambem", "entao", "porque", "quando",
    "onde", "qual", "quais", "quem", "sobre", "entre", "depois", "antes", "ainda", "agora", "hoje",
    "ontem", "amanha", "esta", "estao", "estava", "estamos", "estou", "seria", "sera", "fazer", "faz",
    "feito", "fazendo", "pode", "podem", "podemos", "posso", "precisa", "precisamos", "preciso",
    "temos", "tenho", "tinha", "tem", "ter", "vamos", "vai", "voce", "voces", "gente", "coisa",
    "coisas", "acho", "tipo", "assim", "bem", "bom", "boa", "sim", "nao", "nos", "eles", "elas",
    "dele", "dela", "deles", "meu", "minha", "seu", "sua", "nosso", "nossa", "numa", "num", "uma",
    "umas", "uns", "com", "sem", "por", "pelo", "pela", "pelos", "pelas", "dos", "das", "nas", "nos",
    "aos", "que", "ser", "foi", "era", "sao", "ja", "la", "so", "ate", "cada", "todo", "toda",
    "todos", "todas", "outro", "outra", "outros", "outras", "mesmo", "mesma", "talvez", "certo",
    "claro", "beleza", "legal", "tudo", "nada", "algo", "alguma", "algum", "ok", "okay", "ne", "ta",
    "pra", "pro", "dai", "olha", "enfim", "exatamente", "realmente", "basicamente", "obrigado",
    "obrigada", "falar", "falou", "falando", "dizer", "disse", "ver", "vendo", "ficar", "ficou",
    "quero", "queria", "sabe", "saber", "sei", "the", "and", "that", "this", "with", "for", "you",
    "have", "are", "was", "but", "not", "from", "they", "what", "your", "will", "would", "there",
    "their", "about", "just", "like", "resumo", "reuniao", "conversa", "participantes", "decisoes",
    "pontos", "proximos", "passos", "acoes", "itens", "summary", "meeting", "action", "items",
    "notes", "sistema", "microfone",
];

fn frequent_words(text: &str, exclude: &[String], take: usize) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, (usize, String)> = HashMap::new();
    for raw in text.split(|c: char| !c.is_alphanumeric() && c != '-') {
        let word = raw.trim_matches('-');
        if word.chars().count() < 4 || word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let key = fold(word);
        if STOPWORDS.contains(&key.as_str()) || exclude.contains(&key) {
            continue;
        }
        let entry = counts.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            (0, word.to_lowercase())
        });
        entry.0 += 1;
    }
    let mut ranked: Vec<(usize, usize, String)> = order
        .iter()
        .enumerate()
        .map(|(position, key)| {
            let (count, spelling) = &counts[key];
            (*count, position, spelling.clone())
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let has_repeats = ranked.first().map_or(false, |top| top.0 >= 2);
    let mut chosen: Vec<(usize, String)> = ranked
        .into_iter()
        .filter(|(count, _, _)| !has_repeats || *count >= 2)
        .take(take)
        .map(|(_, position, spelling)| (position, spelling))
        .collect();
    // Back to the order in which the words first showed up.
    chosen.sort_by_key(|(position, _)| *position);
    chosen.into_iter().map(|(_, spelling)| spelling).collect()
}

fn first_summary_heading(summary: &str) -> Option<String> {
    summary
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .filter_map(|line| normalize_subject(line.trim_start_matches('#')))
        .next()
}

fn join_words(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} e {}", init.join(", "), last),
    }
}

/// Used when the model is unavailable or keeps answering garbage: a known
/// client mentioned in the text, the most frequent meaningful words as the
/// subject (or the first summary heading), category "Outros".
pub fn heuristic_classification(transcript: &str, summary: Option<&str>, rules: &Rules<'_>) -> Classification {
    let client = find_known_client_in_text(rules.evidence, rules.known_clients);
    let exclude: Vec<String> = client
        .as_deref()
        .map(|name| fold(name).split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let source = match summary.map(str::trim).filter(|value| !value.is_empty()) {
        Some(summary) => format!("{}\n{}", summary, build_excerpt(transcript)),
        None => build_excerpt(transcript),
    };
    let words = frequent_words(&source, &exclude, 3);
    let subject = normalize_subject(&join_words(&words))
        .or_else(|| summary.and_then(first_summary_heading))
        .unwrap_or_else(|| FALLBACK_SUBJECT.to_string());
    let tags = normalize_tags(&words);
    Classification {
        client,
        project: None,
        subject,
        category: default_category(rules.categories),
        tags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn categories() -> Vec<String> {
        ["Cliente", "Comercial", "Interna", "Suporte", "Planejamento", "Pessoal", "Outros"]
            .iter()
            .map(|value| value.to_string())
            .collect()
    }

    fn known() -> Vec<String> {
        vec!["Boreal Sul".to_string(), "Cedro".to_string(), "Nortec".to_string()]
    }

    #[test]
    fn extracts_json_from_fences_and_chatter() {
        let raw = "Claro! Aqui está:\n```json\n{\"assunto\": \"Custos {OCI}\", \"tags\": []}\n```\nEspero ter ajudado.";
        assert_eq!(
            extract_json_object(raw),
            Some("{\"assunto\": \"Custos {OCI}\", \"tags\": []}")
        );
        assert_eq!(extract_json_object("nada aqui"), None);
        assert_eq!(extract_json_object("{\"assunto\": \"cortado"), None);
        // A broken first object does not hide a valid one after it.
        assert_eq!(extract_json_object("{oops} {\"a\": 1}"), Some("{\"a\": 1}"));
        assert_eq!(
            extract_json_object("{\"a\": {\"b\": \"}\\\"\"}} resto"),
            Some("{\"a\": {\"b\": \"}\\\"\"}}")
        );
    }

    #[test]
    fn parses_and_validates_a_good_answer() {
        let categories = categories();
        let known = known();
        let evidence = fold("Falamos com o pessoal da boreal sul sobre a fatura da OCI.");
        let rules = Rules { known_clients: &known, categories: &categories, evidence: &evidence };
        let raw = r##"{"cliente": "boreal sul", "projeto": "FinOps", "assunto": "Reunião sobre revisão de custos OCI.", "categoria": "cliente", "tags": ["OCI", "#Custos", "oci", "uma tag comprida demais para valer", "fatura", "extra", "sobra"]}"##;
        let parsed = parse_classification(raw, &rules).unwrap();
        assert_eq!(parsed.client.as_deref(), Some("Boreal Sul"));
        assert_eq!(parsed.project.as_deref(), Some("FinOps"));
        assert_eq!(parsed.subject, "Revisão de custos OCI");
        assert_eq!(parsed.category, "Cliente");
        assert_eq!(parsed.tags, vec!["oci", "custos", "fatura", "extra"]);
    }

    #[test]
    fn invented_or_empty_clients_become_null() {
        let categories = categories();
        let known = known();
        let evidence = fold("Conversa interna do time sobre o trimestre com a Acme Ltda.");
        let rules = Rules { known_clients: &known, categories: &categories, evidence: &evidence };

        let invented = parse_classification(
            r#"{"cliente": "Petrobras", "assunto": "Planejamento do trimestre", "categoria": "Interna", "tags": "metas; okr"}"#,
            &rules,
        )
        .unwrap();
        assert_eq!(invented.client, None);
        assert_eq!(invented.category, "Interna");
        assert_eq!(invented.tags, vec!["metas", "okr"]);

        for empty in ["null", "\"null\"", "\"\"", "\"Nenhum\"", "\"interna\"", "\"Cliente\"", "42.5", "[]"] {
            let raw = format!(r#"{{"cliente": {empty}, "assunto": "Planejamento do trimestre"}}"#);
            assert_eq!(parse_classification(&raw, &rules).unwrap().client, None, "{empty}");
        }

        // New client that really is in the text is accepted as written.
        let seen = parse_classification(
            r#"{"client": "Acme Ltda", "subject": "Proposta comercial", "category": "Vendas"}"#,
            &rules,
        )
        .unwrap();
        assert_eq!(seen.client.as_deref(), Some("Acme Ltda"));
        assert_eq!(seen.category, "Outros");
        assert!(seen.tags.is_empty());
    }

    #[test]
    fn invalid_answers_are_errors() {
        let categories = categories();
        let rules = Rules { known_clients: &[], categories: &categories, evidence: "" };
        assert!(parse_classification("não sei", &rules).is_err());
        assert!(parse_classification("[1, 2]", &rules).is_err());
        assert!(parse_classification(r#"{"cliente": "X"}"#, &rules).is_err());
        assert!(parse_classification(r#"{"assunto": ""}"#, &rules).is_err());
        assert!(parse_classification(r#"{"assunto": "Reunião"}"#, &rules).is_err());
        assert!(parse_classification(r#"{"assunto": null}"#, &rules).is_err());
    }

    #[test]
    fn subject_is_cleaned_and_clamped() {
        assert_eq!(
            normalize_subject("  \"Conversa sobre migração do banco para a nuvem\" ").as_deref(),
            Some("Migração do banco para a nuvem")
        );
        assert_eq!(
            normalize_subject("Reunião de Reunião sobre alinhamento semanal").as_deref(),
            Some("Alinhamento semanal")
        );
        assert_eq!(
            normalize_subject("um dois três quatro cinco seis sete oito nove dez").as_deref(),
            Some("Um dois três quatro cinco seis sete oito")
        );
        assert_eq!(normalize_subject("Reunião sobre"), None);
        assert_eq!(normalize_subject("..."), None);
        assert!(normalize_subject(&"x".repeat(300)).unwrap().chars().count() <= 80);
    }

    #[test]
    fn category_matching_ignores_case_and_accents() {
        let mut list = categories();
        assert_eq!(normalize_category(Some("planejamento"), &list), "Planejamento");
        assert_eq!(normalize_category(Some("Financeiro"), &list), "Outros");
        assert_eq!(normalize_category(None, &list), "Outros");
        list.retain(|value| value != "Outros");
        assert_eq!(normalize_category(Some("Financeiro"), &list), "Pessoal");
        assert_eq!(normalize_category(Some("x"), &[]), "Outros");
    }

    #[test]
    fn known_client_matching() {
        let known = known();
        assert_eq!(match_known_client("  CEDRO ", &known).as_deref(), Some("Cedro"));
        assert_eq!(match_known_client("Cedrosa", &known), None);
        let text = fold("A Cedro ligou. Depois a Nortec, e de novo a nortec pediu o relatório. Cedrosa não.");
        assert_eq!(find_known_client_in_text(&text, &known).as_deref(), Some("Nortec"));
        assert_eq!(find_known_client_in_text("nenhum nome aqui", &known), None);
    }

    #[test]
    fn known_clients_merge_keeps_first_spelling() {
        let known = known();
        assert_eq!(merge_known_client(&known, "cedro"), None);
        assert_eq!(merge_known_client(&known, "  "), None);
        let merged = merge_known_client(&known, " Lume  Alimentos ").unwrap();
        assert_eq!(merged.last().map(String::as_str), Some("Lume Alimentos"));
        assert_eq!(merged.len(), 4);
        assert_eq!(
            clean_name_list(&["A".into(), " a ".into(), "".into(), "B".into(), "C".into()], 2, 10),
            vec!["A", "B"]
        );
    }

    #[test]
    fn excerpt_keeps_head_and_tail() {
        let short = "fala curta";
        assert_eq!(build_excerpt(short), short);
        let long = format!("{}{}{}", "a".repeat(6000), "b".repeat(5000), "ç".repeat(1500));
        let excerpt = build_excerpt(&long);
        assert!(excerpt.starts_with("aaaa"));
        assert!(excerpt.ends_with("çççç"));
        assert!(excerpt.contains("\n[...]\n"));
        assert!(!excerpt.contains('b'));
    }

    #[test]
    fn prompt_carries_the_inputs() {
        let categories = categories();
        let known = known();
        let prompt = build_user_prompt(&PromptInput {
            transcript: "Você: bom dia\nParticipantes: vamos ver a fatura",
            summary: Some("## Decisões\n- Reduzir o shape"),
            context: Some("Pauta: custos de novembro"),
            date_label: "18/11/2026 14:05",
            known_clients: &known,
            categories: &categories,
        });
        assert!(prompt.contains("Data da conversa: 18/11/2026 14:05"));
        assert!(prompt.contains("Clientes conhecidos: Boreal Sul; Cedro; Nortec"));
        assert!(prompt.contains("Categorias permitidas: Cliente; Comercial"));
        assert!(prompt.contains("Contexto informado pelo usuário: Pauta: custos de novembro"));
        assert!(prompt.contains("Resumo salvo:\n## Decisões"));
        assert!(prompt.contains("Participantes: vamos ver a fatura"));
        assert!(prompt.contains("\"assunto\": string"));

        let bare = build_user_prompt(&PromptInput {
            transcript: "texto",
            summary: None,
            context: Some("  "),
            date_label: "18/11/2026 14:05",
            known_clients: &[],
            categories: &categories,
        });
        assert!(bare.contains("Clientes conhecidos: nenhum cadastrado"));
        assert!(!bare.contains("Contexto informado"));
        assert!(!bare.contains("Resumo salvo"));
    }

    #[test]
    fn heuristic_uses_known_client_and_frequent_words() {
        let categories = categories();
        let known = known();
        let transcript = "Você: bom dia, pessoal da Cedro. Participantes: precisamos revisar a migração do banco. \
A migração está atrasada porque o backup falhou. Você: então o backup roda hoje e a migração amanhã. \
Participantes: o banco da Cedro precisa do backup antes.";
        let evidence = fold(transcript);
        let rules = Rules { known_clients: &known, categories: &categories, evidence: &evidence };
        let result = heuristic_classification(transcript, None, &rules);
        assert_eq!(result.client.as_deref(), Some("Cedro"));
        assert_eq!(result.subject, "Migração, banco e backup");
        assert_eq!(result.category, "Outros");
        assert_eq!(result.tags, vec!["migração", "banco", "backup"]);
        assert_eq!(result.project, None);
    }

    #[test]
    fn heuristic_falls_back_to_heading_then_to_a_fixed_subject() {
        let categories = categories();
        let rules = Rules { known_clients: &[], categories: &categories, evidence: "" };
        let from_heading = heuristic_classification("sim não ok", Some("## Revisão do contrato anual\nok"), &rules);
        // The summary words win when there are any meaningful ones.
        assert_eq!(from_heading.subject, "Revisão, contrato e anual");
        let heading_only = heuristic_classification("sim não ok", Some("## Uma boa\nok"), &rules);
        assert_eq!(heading_only.subject, "Uma boa");
        let nothing = heuristic_classification("sim não ok", None, &rules);
        assert_eq!(nothing.subject, "Conversa");
        assert_eq!(nothing.client, None);
        assert!(nothing.tags.is_empty());
    }
}
