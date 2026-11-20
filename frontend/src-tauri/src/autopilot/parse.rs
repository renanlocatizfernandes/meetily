//! Reads the model's JSON answer defensively. Nothing here logs content.

use serde_json::Value;

use super::model::{Extraction, TaskItem, Wanted};

const MAX_ITEMS: usize = 30;
const MAX_ITEM_CHARS: usize = 240;
const MAX_OWNER_CHARS: usize = 60;
const MAX_DUE_CHARS: usize = 40;
const MAX_TITLE_CHARS: usize = 90;
const MAX_TAGS: usize = 8;
const MAX_TAG_CHARS: usize = 24;
const MAX_SUBJECT_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 4000;

pub fn clamp_chars(text: &str, max: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    squashed.chars().take(max).collect::<String>().trim().to_string()
}

fn strip_reasoning(raw: &str) -> String {
    let mut text = raw.to_string();
    while let (Some(start), Some(end)) = (text.find("<think>"), text.find("</think>")) {
        if end < start {
            break;
        }
        text.replace_range(start..end + "</think>".len(), "");
    }
    text
}

/// Finds the outermost JSON object in a reply that may carry code fences or prose.
fn json_object(raw: &str) -> Option<Value> {
    let text = strip_reasoning(raw);
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    match serde_json::from_str::<Value>(&text[start..=end]) {
        Ok(value @ Value::Object(_)) => Some(value),
        _ => None,
    }
}

fn is_blank_marker(text: &str) -> bool {
    matches!(
        text.trim().to_lowercase().as_str(),
        "" | "null" | "none" | "nenhum" | "nenhuma" | "n/a" | "-" | "—" | "nao informado" | "não informado" | "sem prazo"
    )
}

fn optional_text(value: Option<&Value>, max: usize) -> Option<String> {
    let text = clamp_chars(value?.as_str()?, max);
    if is_blank_marker(&text) {
        None
    } else {
        Some(text)
    }
}

fn dedupe_push(list: &mut Vec<String>, item: String) {
    let key = item.to_lowercase();
    if !list.iter().any(|existing| existing.to_lowercase() == key) {
        list.push(item);
    }
}

fn string_list(value: Option<&Value>, max_items: usize, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let Some(Value::Array(items)) = value else {
        return out;
    };
    for item in items {
        let text = match item {
            Value::String(text) => clamp_chars(text, max_chars),
            Value::Object(map) => map
                .get("texto")
                .or_else(|| map.get("text"))
                .and_then(Value::as_str)
                .map(|text| clamp_chars(text, max_chars))
                .unwrap_or_default(),
            _ => String::new(),
        };
        if !is_blank_marker(&text) {
            dedupe_push(&mut out, text);
        }
        if out.len() >= max_items {
            break;
        }
    }
    out
}

fn task_list(value: Option<&Value>) -> Vec<TaskItem> {
    let mut out: Vec<TaskItem> = Vec::new();
    let Some(Value::Array(items)) = value else {
        return out;
    };
    for item in items {
        let task = match item {
            Value::String(text) => TaskItem {
                text: clamp_chars(text, MAX_ITEM_CHARS),
                ..TaskItem::default()
            },
            Value::Object(map) => TaskItem {
                text: map
                    .get("texto")
                    .or_else(|| map.get("tarefa"))
                    .or_else(|| map.get("text"))
                    .and_then(Value::as_str)
                    .map(|text| clamp_chars(text, MAX_ITEM_CHARS))
                    .unwrap_or_default(),
                owner: optional_text(
                    map.get("responsavel").or_else(|| map.get("responsável")).or_else(|| map.get("owner")),
                    MAX_OWNER_CHARS,
                ),
                due: optional_text(map.get("prazo").or_else(|| map.get("due")), MAX_DUE_CHARS),
            },
            _ => continue,
        };
        if is_blank_marker(&task.text) {
            continue;
        }
        let key = task.text.to_lowercase();
        if out.iter().any(|existing| existing.text.to_lowercase() == key) {
            continue;
        }
        out.push(task);
        if out.len() >= MAX_ITEMS {
            break;
        }
    }
    out
}

fn tag_list(value: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    for tag in string_list(value, MAX_TAGS * 2, MAX_TAG_CHARS) {
        let tag = tag.trim_start_matches('#').trim().to_string();
        if !tag.is_empty() {
            dedupe_push(&mut out, tag);
        }
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

fn body_text(value: Option<&Value>) -> Option<String> {
    let raw = value?.as_str()?;
    // Keeps line breaks (an e-mail needs paragraphs); only trims and caps.
    let lines: Vec<String> = raw
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();
    let joined = lines.join("\n");
    let capped: String = joined.trim().chars().take(MAX_BODY_CHARS).collect();
    if is_blank_marker(&capped) {
        None
    } else {
        Some(capped)
    }
}

/// Parses the answer for the groups that were asked for. Groups that were not
/// asked for are ignored even if the model returned them.
pub fn parse_extraction(raw: &str, wanted: &Wanted) -> Result<Extraction, String> {
    let value = json_object(raw).ok_or_else(|| "resposta sem JSON".to_string())?;
    let mut extraction = Extraction::default();
    let mut found_any = false;

    if wanted.tasks {
        extraction.tasks = task_list(value.get("tarefas"));
        extraction.decisions = string_list(value.get("decisoes").or_else(|| value.get("decisões")), MAX_ITEMS, MAX_ITEM_CHARS);
        extraction.pending = string_list(value.get("pendencias").or_else(|| value.get("pendências")), MAX_ITEMS, MAX_ITEM_CHARS);
        found_any |= value.get("tarefas").is_some()
            || value.get("decisoes").is_some()
            || value.get("decisões").is_some()
            || value.get("pendencias").is_some()
            || value.get("pendências").is_some();
    }
    if wanted.title_tags {
        extraction.title = optional_text(value.get("titulo").or_else(|| value.get("título")), MAX_TITLE_CHARS);
        extraction.tags = tag_list(value.get("etiquetas"));
        found_any |= value.get("titulo").is_some() || value.get("título").is_some() || value.get("etiquetas").is_some();
    }
    if wanted.email {
        let email = value.get("email");
        extraction.email_subject = optional_text(email.and_then(|e| e.get("assunto")), MAX_SUBJECT_CHARS);
        extraction.email_body = body_text(email.and_then(|e| e.get("corpo")));
        found_any |= email.is_some();
    }

    if !found_any {
        return Err("resposta sem os campos esperados".to_string());
    }
    Ok(extraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Wanted = Wanted {
        tasks: true,
        title_tags: true,
        email: true,
    };

    #[test]
    fn parses_fenced_json_with_prose() {
        let raw = "Claro!\n```json\n{\"tarefas\":[{\"texto\":\"Enviar proposta\",\"responsavel\":\"Ana\",\"prazo\":\"2026-12-01\"},{\"texto\":\"Revisar contrato\",\"responsavel\":null,\"prazo\":\"nenhum\"}],\"decisoes\":[\"Adotar plano B\"],\"pendencias\":[\"Definir orcamento\"],\"titulo\":\"Revisao do plano\",\"etiquetas\":[\"#plano\",\"Orcamento\",\"plano\"],\"email\":{\"assunto\":\"Resumo da reuniao\",\"corpo\":\"Ola,\\n\\nSegue o resumo.\\n\\nAbraco\"}}\n```";
        let out = parse_extraction(raw, &ALL).unwrap();
        assert_eq!(out.tasks.len(), 2);
        assert_eq!(out.tasks[0].owner.as_deref(), Some("Ana"));
        assert_eq!(out.tasks[0].due.as_deref(), Some("2026-12-01"));
        assert_eq!(out.tasks[1].owner, None);
        assert_eq!(out.tasks[1].due, None);
        assert_eq!(out.decisions, vec!["Adotar plano B"]);
        assert_eq!(out.pending, vec!["Definir orcamento"]);
        assert_eq!(out.title.as_deref(), Some("Revisao do plano"));
        assert_eq!(out.tags, vec!["plano", "Orcamento"]);
        assert_eq!(out.email_subject.as_deref(), Some("Resumo da reuniao"));
        assert!(out.email_body.unwrap().contains("\n\nSegue o resumo."));
    }

    #[test]
    fn ignores_groups_that_were_not_requested() {
        let raw = r#"{"tarefas":[{"texto":"A"}],"titulo":"T","email":{"assunto":"S","corpo":"B"}}"#;
        let wanted = Wanted {
            tasks: true,
            title_tags: false,
            email: false,
        };
        let out = parse_extraction(raw, &wanted).unwrap();
        assert_eq!(out.tasks.len(), 1);
        assert_eq!(out.title, None);
        assert_eq!(out.email_subject, None);
    }

    #[test]
    fn rejects_text_without_json_or_fields() {
        assert!(parse_extraction("nao sei", &ALL).is_err());
        assert!(parse_extraction("{\"outra\":1}", &ALL).is_err());
        assert!(parse_extraction("{\"tarefas\":[]}", &Wanted { tasks: false, title_tags: true, email: false }).is_err());
    }

    #[test]
    fn empty_lists_are_a_valid_answer() {
        let out = parse_extraction(r#"{"tarefas":[],"decisoes":[],"pendencias":[]}"#, &Wanted { tasks: true, ..Wanted::default() }).unwrap();
        assert!(out.tasks.is_empty() && out.decisions.is_empty() && out.pending.is_empty());
    }

    #[test]
    fn strips_reasoning_and_caps_lengths() {
        let long = "x".repeat(500);
        let raw = format!("<think>{{\"tarefas\":[\"lixo\"]}}</think>{{\"tarefas\":[\"{long}\"],\"titulo\":\"{long}\"}}");
        let out = parse_extraction(&raw, &ALL).unwrap();
        assert_eq!(out.tasks[0].text.chars().count(), MAX_ITEM_CHARS);
        assert_eq!(out.title.unwrap().chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn dedupes_and_caps_item_counts() {
        let items: Vec<String> = (0..50).map(|i| format!("\"item {i}\"")).collect();
        let raw = format!("{{\"decisoes\":[{}]}}", items.join(","));
        let out = parse_extraction(&raw, &Wanted { tasks: true, ..Wanted::default() }).unwrap();
        assert_eq!(out.decisions.len(), MAX_ITEMS);
    }
}
