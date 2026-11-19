pub const MAX_ITEMS: usize = 2;
pub const MAX_WORDS: usize = 25;
/// The origin the model wrote for an item is a short label, not a sentence.
pub const MAX_ORIGIN_CHARS: usize = 60;

/// Origin labels as the model wrote them, parallel to the three lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimedOrigins {
    pub perguntas: Vec<Option<String>>,
    pub respostas: Vec<Option<String>>,
    pub contexto: Vec<Option<String>>,
}

impl ClaimedOrigins {
    pub fn get(&self, section: &str, index: usize) -> Option<&str> {
        let list = match section {
            "perguntas" => &self.perguntas,
            "respostas" => &self.respostas,
            "contexto" => &self.contexto,
            _ => return None,
        };
        list.get(index).and_then(|origin| origin.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistLists {
    pub perguntas: Vec<String>,
    pub respostas: Vec<String>,
    pub contexto: Vec<String>,
    /// Optional `origem` of each item. Plain string items have none.
    pub origens: ClaimedOrigins,
}

/// Trims an origin label and drops an empty one.
pub fn clean_origin(raw: &str) -> Option<String> {
    let label: String = raw.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_ORIGIN_CHARS).collect();
    let label = label.trim().to_string();
    (!label.is_empty()).then_some(label)
}

pub fn limit_words(text: &str, max_words: usize) -> String {
    let mut out = String::new();
    for (index, word) in text.split_whitespace().enumerate() {
        if index >= max_words {
            break;
        }
        if index > 0 {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

fn string_item(value: &serde_json::Value) -> Option<(String, Option<String>)> {
    let (text, origin) = match value {
        serde_json::Value::String(text) => (text.clone(), None),
        serde_json::Value::Object(object) => (
            object
                .get("text")
                .or_else(|| object.get("texto"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string(),
            object
                .get("origem")
                .or_else(|| object.get("origin"))
                .or_else(|| object.get("fonte"))
                .and_then(serde_json::Value::as_str)
                .and_then(clean_origin),
        ),
        _ => return None,
    };
    let limited = limit_words(text.trim(), MAX_WORDS);
    if limited.is_empty() {
        None
    } else {
        Some((limited, origin))
    }
}

fn string_list(value: &serde_json::Value) -> Option<(Vec<String>, Vec<Option<String>>)> {
    let array = value.as_array()?;
    let mut items = Vec::new();
    let mut origins = Vec::new();
    for entry in array {
        if let Some((text, origin)) = string_item(entry) {
            items.push(text);
            origins.push(origin);
        }
        if items.len() == MAX_ITEMS {
            break;
        }
    }
    Some((items, origins))
}

/// Parses the strict JSON object. Invalid JSON is discarded (`None`).
pub fn parse_assist_json(raw: &str) -> Option<AssistLists> {
    let cleaned = raw.trim().trim_start_matches("```json").trim_start_matches("```");
    let cleaned = cleaned.trim().trim_end_matches("```").trim();
    let start = cleaned.find('{')?;
    let end = cleaned.rfind('}')?;
    if end <= start {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&cleaned[start..=end]).ok()?;
    let object = value.as_object()?;
    let empty = serde_json::Value::Array(Vec::new());
    let (perguntas, perguntas_origem) = string_list(object.get("perguntas").unwrap_or(&empty))?;
    let (respostas, respostas_origem) = string_list(object.get("respostas").unwrap_or(&empty))?;
    let (contexto, contexto_origem) = string_list(object.get("contexto").unwrap_or(&empty))?;
    Some(AssistLists {
        perguntas,
        respostas,
        contexto,
        origens: ClaimedOrigins {
            perguntas: perguntas_origem,
            respostas: respostas_origem,
            contexto: contexto_origem,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_json_and_enforces_limits() {
        let raw = r#"Aqui está:
```json
{"perguntas":["Uma pergunta curta?","Segunda?","Terceira que deve cair"],"respostas":[],"contexto":["Um termo com palavras demais um dois tres quatro cinco seis sete oito nove dez onze doze treze quatorze quinze dezesseis dezessete dezoito dezenove vinte vinteum vintedois vintetres vintequatro vintecinco vinteséis extra"],"extra":true}
```
"#;
        let parsed = parse_assist_json(raw).expect("json");
        assert_eq!(parsed.perguntas, vec!["Uma pergunta curta?", "Segunda?"]);
        assert!(parsed.respostas.is_empty());
        assert_eq!(parsed.contexto.len(), 1);
        assert_eq!(parsed.contexto[0].split_whitespace().count(), MAX_WORDS);
        assert!(!parsed.contexto[0].contains("extra"));
    }

    #[test]
    fn discards_invalid_output() {
        assert!(parse_assist_json("sem json").is_none());
        assert!(parse_assist_json("{\"perguntas\":\"nao e lista\",\"respostas\":[],\"contexto\":[]}").is_none());
        assert!(parse_assist_json("").is_none());
    }

    #[test]
    fn object_items_carry_an_optional_origin() {
        let raw = r#"{"perguntas":["Qual é o prazo?"],"respostas":[{"texto":"Para a Ana: sexta."}],"contexto":[{"texto":"SLA é o acordo de nível de serviço.","origem":"  glossário "},{"text":"Ficou pendente a proposta.","origin":"reunião de 12/11"}]}"#;
        let parsed = parse_assist_json(raw).expect("json");
        assert_eq!(parsed.perguntas, vec!["Qual é o prazo?"]);
        assert_eq!(parsed.respostas, vec!["Para a Ana: sexta."]);
        assert_eq!(parsed.contexto.len(), 2);
        assert_eq!(parsed.origens.get("perguntas", 0), None);
        assert_eq!(parsed.origens.get("respostas", 0), None);
        assert_eq!(parsed.origens.get("contexto", 0), Some("glossário"));
        assert_eq!(parsed.origens.get("contexto", 1), Some("reunião de 12/11"));
        assert_eq!(parsed.origens.get("contexto", 2), None);
        assert_eq!(parsed.origens.get("outra", 0), None);
        // A non-string or empty origin is ignored, the item stays.
        let odd = parse_assist_json(r#"{"contexto":[{"texto":"Um termo.","origem":7},{"texto":"Outro termo.","origem":"   "}]}"#).unwrap();
        assert_eq!(odd.contexto.len(), 2);
        assert_eq!(odd.origens.contexto, vec![None, None]);
        assert_eq!(clean_origin(&"x".repeat(200)).unwrap().chars().count(), MAX_ORIGIN_CHARS);
    }

    #[test]
    fn accepts_an_empty_valid_object() {
        let parsed = parse_assist_json("{\"perguntas\":[],\"respostas\":[],\"contexto\":[]}").unwrap();
        assert!(parsed.perguntas.is_empty());
    }
}
