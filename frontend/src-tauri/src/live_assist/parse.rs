pub const MAX_ITEMS: usize = 2;
pub const MAX_WORDS: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistLists {
    pub perguntas: Vec<String>,
    pub respostas: Vec<String>,
    pub contexto: Vec<String>,
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

fn string_item(value: &serde_json::Value) -> Option<String> {
    let text = match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("texto"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => return None,
    };
    let limited = limit_words(text.trim(), MAX_WORDS);
    if limited.is_empty() {
        None
    } else {
        Some(limited)
    }
}

fn string_list(value: &serde_json::Value) -> Option<Vec<String>> {
    let array = value.as_array()?;
    let mut items = Vec::new();
    for entry in array {
        if let Some(text) = string_item(entry) {
            items.push(text);
        }
        if items.len() == MAX_ITEMS {
            break;
        }
    }
    Some(items)
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
    Some(AssistLists {
        perguntas: string_list(object.get("perguntas").unwrap_or(&empty))?,
        respostas: string_list(object.get("respostas").unwrap_or(&empty))?,
        contexto: string_list(object.get("contexto").unwrap_or(&empty))?,
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
    fn accepts_an_empty_valid_object() {
        let parsed = parse_assist_json("{\"perguntas\":[],\"respostas\":[],\"contexto\":[]}").unwrap();
        assert!(parsed.perguntas.is_empty());
    }
}
