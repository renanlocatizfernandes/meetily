#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub who: String,
    pub what: String,
    pub deadline: Option<String>,
}

pub fn parse_action_items(text: &str) -> Vec<ActionItem> {
    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '•']).trim();
        if line.chars().count() < 6 {
            continue;
        }
        let Some(item) = parse_line(line) else { continue };
        if items.iter().any(|existing: &ActionItem| existing.what == item.what) {
            continue;
        }
        items.push(item);
        if items.len() == 12 {
            break;
        }
    }
    items
}

fn parse_line(line: &str) -> Option<ActionItem> {
    let lower = line.to_lowercase();
    let actionable = lower.contains(" vai ")
        || lower.starts_with("vai ")
        || lower.contains(" precisa ")
        || lower.contains(" ficou de ")
        || lower.contains(" até ")
        || lower.contains(" ate ")
        || lower.contains("todo:")
        || lower.starts_with("todo ")
        || lower.contains("ação:")
        || lower.contains("acao:")
        || lower.contains(':');
    if !actionable {
        return None;
    }
    let (who, rest) = if let Some((name, after)) = line.split_once(':') {
        let name = name.trim();
        if name.chars().count() <= 40 && !name.to_lowercase().starts_with("http") {
            (name.to_string(), after.trim().to_string())
        } else {
            (String::new(), line.to_string())
        }
    } else if let Some(index) = lower.find(" vai ") {
        (line[..index].trim().to_string(), line[index + 5..].trim().to_string())
    } else {
        (String::new(), line.to_string())
    };
    if rest.is_empty() {
        return None;
    }
    let deadline = extract_deadline(&rest);
    Some(ActionItem {
        who: if who.is_empty() { "A definir".to_string() } else { who },
        what: rest.chars().take(160).collect(),
        deadline,
    })
}

fn extract_deadline(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for marker in ["até ", "ate ", "prazo "] {
        if let Some(index) = lower.find(marker) {
            let rest = text[index + marker.len()..].trim();
            let end = rest.find([',', '.']).unwrap_or(rest.len());
            let deadline = rest[..end].trim();
            if !deadline.is_empty() {
                return Some(deadline.chars().take(40).collect());
            }
        }
    }
    None
}

pub fn to_ics(items: &[ActionItem], stamp: &str) -> String {
    let mut body = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Meetily Live//PT\r\n");
    for (index, item) in items.iter().enumerate() {
        body.push_str("BEGIN:VTODO\r\n");
        body.push_str(&format!("UID:meetily-{stamp}-{index}@local\r\n"));
        body.push_str(&format!("DTSTAMP:{stamp}\r\n"));
        body.push_str(&format!("SUMMARY:{}\r\n", escape(&item.what)));
        body.push_str(&format!(
            "DESCRIPTION:Responsável: {}\r\n",
            escape(&item.who)
        ));
        if item.deadline.is_some() {
            body.push_str("STATUS:NEEDS-ACTION\r\n");
        }
        body.push_str("END:VTODO\r\n");
    }
    body.push_str("END:VCALENDAR\r\n");
    body
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', " ")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_who_what_and_deadline() {
        let text = "Ana vai enviar a proposta até sexta.\nJoão: revisar o contrato até 20/11\nBom dia a todos";
        let items = parse_action_items(text);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].who, "Ana");
        assert_eq!(items[0].deadline.as_deref(), Some("sexta"));
        assert_eq!(items[1].who, "João");
        assert_eq!(items[1].deadline.as_deref(), Some("20/11"));
    }

    #[test]
    fn ics_is_a_calendar_of_todos() {
        let ics = to_ics(
            &[ActionItem {
                who: "Ana".to_string(),
                what: "enviar a proposta".to_string(),
                deadline: Some("sexta".to_string()),
            }],
            "20261119T010000Z",
        );
        assert!(ics.contains("BEGIN:VTODO"));
        assert!(ics.contains("SUMMARY:enviar a proposta"));
        assert!(ics.contains("END:VCALENDAR"));
    }
}
