//! Tolerant reader for the live assist JSON while it is still being streamed.
//!
//! The model answers `{"perguntas":[],"respostas":[],"contexto":[]}`. This
//! module reads whatever prefix of that object has arrived and returns the
//! string items that are already visible, including one that is still being
//! written. It never fails: text it cannot read is skipped.

use std::iter::Peekable;
use std::str::Chars;

use serde::Serialize;

use super::context::AssistOrigin;
use super::parse::{clean_origin, limit_words, MAX_ITEMS, MAX_WORDS};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialItem {
    /// "pergunta", "resposta" or "contexto".
    pub kind: &'static str,
    pub text: String,
    /// Where the item came from, once the model wrote its `origem` and the
    /// service checked it. Absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<AssistOrigin>,
    /// `origem` exactly as the model wrote it. Not sent to the panel.
    #[serde(skip)]
    pub claimed_origin: Option<String>,
}

/// Maps an object key to the item kind and to the section name used by the
/// final `live-assist-update` event.
fn kind_for_key(key: &str) -> Option<&'static str> {
    match key.trim().to_ascii_lowercase().as_str() {
        "perguntas" => Some("pergunta"),
        "respostas" => Some("resposta"),
        "contexto" => Some("contexto"),
        _ => None,
    }
}

pub fn section_for_kind(kind: &str) -> &'static str {
    match kind {
        "pergunta" => "perguntas",
        "resposta" => "respostas",
        _ => "contexto",
    }
}

struct Scanner<'a> {
    chars: Peekable<Chars<'a>>,
    items: Vec<PartialItem>,
}

impl Scanner<'_> {
    fn skip_whitespace(&mut self) {
        while self.chars.peek().is_some_and(|ch| ch.is_whitespace()) {
            self.chars.next();
        }
    }

    /// Reads a JSON string after its opening quote. Returns the decoded text
    /// and whether the closing quote was seen. A dangling escape at the end of
    /// the input is dropped.
    fn read_string(&mut self) -> (String, bool) {
        let mut out = String::new();
        loop {
            let Some(ch) = self.chars.next() else {
                return (out, false);
            };
            match ch {
                '"' => return (out, true),
                '\\' => {
                    let Some(escaped) = self.chars.next() else {
                        return (out, false);
                    };
                    match escaped {
                        'n' | 'r' | 't' => out.push(' '),
                        'b' | 'f' => {}
                        'u' => {
                            let Some(unit) = self.read_hex4() else {
                                return (out, false);
                            };
                            match self.decode_unit(unit) {
                                Some(Some(decoded)) => out.push(decoded),
                                Some(None) => {}
                                None => return (out, false),
                            }
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn read_hex4(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = self.chars.peek().and_then(|ch| ch.to_digit(16))?;
            self.chars.next();
            value = value * 16 + digit;
        }
        Some(value)
    }

    /// `None` means the input ended inside a surrogate pair. `Some(None)` is
    /// an invalid unit that is skipped.
    fn decode_unit(&mut self, unit: u32) -> Option<Option<char>> {
        if !(0xD800..=0xDBFF).contains(&unit) {
            return Some(char::from_u32(unit));
        }
        // High surrogate: the low half must follow as another \uXXXX.
        match self.chars.peek() {
            None => return None,
            Some('\\') => {}
            Some(_) => return Some(None),
        }
        let mut ahead = self.chars.clone();
        ahead.next();
        match ahead.peek() {
            None => return None,
            Some('u') => {}
            Some(_) => return Some(None),
        }
        self.chars.next();
        self.chars.next();
        let low = self.read_hex4()?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return Some(None);
        }
        Some(char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)))
    }

    fn push(&mut self, kind: Option<&'static str>, text: &str, claimed_origin: Option<String>) {
        let Some(kind) = kind else { return };
        if self.items.iter().filter(|item| item.kind == kind).count() >= MAX_ITEMS {
            return;
        }
        let limited = limit_words(text.trim(), MAX_WORDS);
        if !limited.is_empty() {
            self.items.push(PartialItem {
                kind,
                text: limited,
                origin: None,
                claimed_origin,
            });
        }
    }

    /// Reads the top-level object. Returns when the input ends.
    fn read_root(&mut self) {
        while let Some(ch) = self.chars.next() {
            if ch == '{' {
                break;
            }
        }
        loop {
            self.skip_whitespace();
            match self.chars.next() {
                None | Some('}') => return,
                Some('"') => {
                    let (key, complete) = self.read_string();
                    if !complete {
                        return;
                    }
                    self.skip_whitespace();
                    if self.chars.peek() != Some(&':') {
                        continue;
                    }
                    self.chars.next();
                    self.skip_whitespace();
                    let kind = kind_for_key(&key);
                    if self.chars.peek() == Some(&'[') {
                        self.chars.next();
                        if !self.read_array(kind) {
                            return;
                        }
                    } else if !self.skip_value() {
                        return;
                    }
                }
                Some(_) => {}
            }
        }
    }

    /// Reads an array of items after its `[`. Returns false when the input
    /// ended before the closing `]`.
    fn read_array(&mut self, kind: Option<&'static str>) -> bool {
        loop {
            self.skip_whitespace();
            match self.chars.next() {
                None => return false,
                Some(']') => return true,
                Some('"') => {
                    let (text, complete) = self.read_string();
                    self.push(kind, &text, None);
                    if !complete {
                        return false;
                    }
                }
                Some('{') => {
                    let (text, origin, complete) = self.read_item_object();
                    if let Some(text) = text {
                        self.push(kind, &text, origin);
                    }
                    if !complete {
                        return false;
                    }
                }
                Some('[') => {
                    if !self.read_array(None) {
                        return false;
                    }
                }
                Some(_) => {}
            }
        }
    }

    /// Items may be objects: `{"texto":"...","origem":"..."}` (also `text`,
    /// `origin`, `fonte`). Reads such an object after its `{` and returns the
    /// text, the origin label once it is complete, and whether the object closed.
    fn read_item_object(&mut self) -> (Option<String>, Option<String>, bool) {
        let mut found = None;
        let mut origin = None;
        loop {
            self.skip_whitespace();
            match self.chars.next() {
                None => return (found, origin, false),
                Some('}') => return (found, origin, true),
                Some('"') => {
                    let (key, complete) = self.read_string();
                    if !complete {
                        return (found, origin, false);
                    }
                    self.skip_whitespace();
                    if self.chars.peek() != Some(&':') {
                        continue;
                    }
                    self.chars.next();
                    self.skip_whitespace();
                    let is_text = matches!(key.trim(), "text" | "texto");
                    let is_origin = matches!(key.trim(), "origem" | "origin" | "fonte");
                    if (is_text || is_origin) && self.chars.peek() == Some(&'"') {
                        self.chars.next();
                        let (value, complete) = self.read_string();
                        if is_text {
                            found = Some(value);
                        } else if complete {
                            // A half-written label would flicker in the panel.
                            origin = clean_origin(&value);
                        }
                        if !complete {
                            return (found, origin, false);
                        }
                    } else if !self.skip_value() {
                        return (found, origin, false);
                    }
                }
                Some(_) => {}
            }
        }
    }

    /// Skips one value that is not an item list. Returns false when the input
    /// ended inside it.
    fn skip_value(&mut self) -> bool {
        match self.chars.peek().copied() {
            None => false,
            Some('"') => {
                self.chars.next();
                self.read_string().1
            }
            Some('{') | Some('[') => {
                let mut depth = 0usize;
                loop {
                    match self.chars.next() {
                        None => return false,
                        Some('"') => {
                            if !self.read_string().1 {
                                return false;
                            }
                        }
                        Some('{') | Some('[') => depth += 1,
                        Some('}') | Some(']') => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                return true;
                            }
                        }
                        Some(_) => {}
                    }
                }
            }
            Some(_) => {
                // Number, true, false or null: stop before the separator.
                while let Some(ch) = self.chars.peek() {
                    if matches!(ch, ',' | '}' | ']') {
                        break;
                    }
                    self.chars.next();
                }
                true
            }
        }
    }
}

/// Returns the items visible so far in a partial answer, in the order
/// perguntas, respostas, contexto as the model wrote them. At most
/// `MAX_ITEMS` per kind and `MAX_WORDS` words per item, like the final parse.
pub fn extract_partial_items(raw: &str) -> Vec<PartialItem> {
    let mut scanner = Scanner {
        chars: raw.chars().peekable(),
        items: Vec::new(),
    };
    scanner.read_root();
    scanner.items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(raw: &str) -> Vec<(&'static str, String)> {
        extract_partial_items(raw)
            .into_iter()
            .map(|item| (item.kind, item.text))
            .collect()
    }

    #[test]
    fn nothing_is_visible_before_the_first_string() {
        assert!(pairs("").is_empty());
        assert!(pairs("```json\n").is_empty());
        assert!(pairs("{").is_empty());
        assert!(pairs("{\"pergun").is_empty());
        assert!(pairs("{\"perguntas\"").is_empty());
        assert!(pairs("{\"perguntas\":").is_empty());
        assert!(pairs("{\"perguntas\":[").is_empty());
        assert!(pairs("{\"perguntas\":[\"").is_empty());
    }

    #[test]
    fn a_truncated_string_is_returned_as_typed_so_far() {
        assert_eq!(
            pairs("{\"perguntas\":[\"Qual é o pra"),
            vec![("pergunta", "Qual é o pra".to_string())]
        );
        assert_eq!(
            pairs("{\"perguntas\":[\"Qual é o prazo?\",\"E o orç"),
            vec![
                ("pergunta", "Qual é o prazo?".to_string()),
                ("pergunta", "E o orç".to_string()),
            ]
        );
    }

    #[test]
    fn a_complete_object_gives_every_section_in_order() {
        let raw = r#"{"perguntas":["Qual é o prazo?"],"respostas":["Para a Ana: até sexta."],"contexto":["SLA: acordo de nível de serviço."]}"#;
        assert_eq!(
            pairs(raw),
            vec![
                ("pergunta", "Qual é o prazo?".to_string()),
                ("resposta", "Para a Ana: até sexta.".to_string()),
                ("contexto", "SLA: acordo de nível de serviço.".to_string()),
            ]
        );
    }

    #[test]
    fn missing_and_empty_sections_are_fine() {
        assert_eq!(
            pairs("{\"respostas\":[],\"contexto\":[\"Kubernetes orquestra contêin"),
            vec![("contexto", "Kubernetes orquestra contêin".to_string())]
        );
        assert_eq!(
            pairs("Aqui está:\n```json\n{ \"contexto\" : [ \"Um termo\" ] }\n```"),
            vec![("contexto", "Um termo".to_string())]
        );
        assert!(pairs("{\"perguntas\":[],\"respostas\":[],\"contexto\":[]}").is_empty());
    }

    #[test]
    fn escapes_are_decoded_and_a_dangling_escape_is_dropped() {
        assert_eq!(
            pairs(r#"{"perguntas":["Ele disse \"sim\" ou não?"]}"#),
            vec![("pergunta", "Ele disse \"sim\" ou não?".to_string())]
        );
        // Input cut right after the backslash, inside \u and inside a pair.
        assert_eq!(pairs("{\"perguntas\":[\"Custo\\"), vec![("pergunta", "Custo".to_string())]);
        assert_eq!(pairs("{\"perguntas\":[\"Custo\\u00"), vec![("pergunta", "Custo".to_string())]);
        assert_eq!(pairs("{\"perguntas\":[\"Ok \\ud83d"), vec![("pergunta", "Ok".to_string())]);
        assert_eq!(
            pairs("{\"perguntas\":[\"Ok \\ud83d\\ude00 fim"),
            vec![("pergunta", "Ok 😀 fim".to_string())]
        );
        assert_eq!(
            pairs("{\"contexto\":[\"linha\\num\\tdois\"]}"),
            vec![("contexto", "linha um dois".to_string())]
        );
    }

    #[test]
    fn a_quote_inside_a_string_does_not_close_the_list() {
        let raw = r#"{"perguntas":["O termo \"]\" aparece?"],"respostas":["Sim"#;
        assert_eq!(
            pairs(raw),
            vec![
                ("pergunta", "O termo \"]\" aparece?".to_string()),
                ("resposta", "Sim".to_string()),
            ]
        );
    }

    #[test]
    fn limits_match_the_final_parse() {
        let long = (1..=40).map(|n| format!("p{n}")).collect::<Vec<_>>().join(" ");
        let raw = format!("{{\"perguntas\":[\"um\",\"dois\",\"tres\"],\"contexto\":[\"{long}");
        let items = pairs(&raw);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], ("pergunta", "um".to_string()));
        assert_eq!(items[1], ("pergunta", "dois".to_string()));
        assert_eq!(items[2].0, "contexto");
        assert_eq!(items[2].1.split_whitespace().count(), MAX_WORDS);
    }

    #[test]
    fn unknown_keys_scalars_and_object_items_are_tolerated() {
        let raw = r#"{"nota":"ignorar","n":3,"extra":{"perguntas":["não vale"]},"perguntas":[{"text":"Pergunta em objeto?"},null,7,{"texto":"Outra em andam"#;
        assert_eq!(
            pairs(raw),
            vec![
                ("pergunta", "Pergunta em objeto?".to_string()),
                ("pergunta", "Outra em andam".to_string()),
            ]
        );
        // A section that is not a list is skipped, the next one still reads.
        assert_eq!(
            pairs(r#"{"perguntas":"não é lista","respostas":["Vale"]}"#),
            vec![("resposta", "Vale".to_string())]
        );
    }

    #[test]
    fn object_items_expose_the_origin_once_it_is_complete() {
        let raw = r#"{"contexto":[{"texto":"SLA é o acordo de nível de serviço.","origem":"glossário"},{"origem":"reunião de 12/11","texto":"Ficou pendente a propos"#;
        let items = extract_partial_items(raw);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].claimed_origin.as_deref(), Some("glossário"));
        assert_eq!(items[1].text, "Ficou pendente a propos");
        assert_eq!(items[1].claimed_origin.as_deref(), Some("reunião de 12/11"));
        assert!(items.iter().all(|item| item.origin.is_none()));

        // A label still being written is not shown.
        let cut = extract_partial_items(r#"{"contexto":[{"texto":"Um termo.","origem":"reuni"#);
        assert_eq!(cut.len(), 1);
        assert_eq!(cut[0].claimed_origin, None);

        // The claimed label is internal; the event only carries `origin` when set.
        let json = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(json, serde_json::json!({"kind":"contexto","text":"SLA é o acordo de nível de serviço."}));
    }

    #[test]
    fn every_prefix_of_a_valid_answer_is_safe_and_grows() {
        let raw = r#"{"perguntas":["Qual é o prazo \"final\"?","E o custo?"],"respostas":["Até sexta."],"contexto":["SLA é o acordo."]}"#;
        let mut last = 0;
        let chars: Vec<char> = raw.chars().collect();
        for end in 0..=chars.len() {
            let prefix: String = chars[..end].iter().collect();
            let items = extract_partial_items(&prefix);
            assert!(items.len() >= last, "items shrank at {end}");
            last = items.len();
        }
        assert_eq!(last, 4);
    }
}
