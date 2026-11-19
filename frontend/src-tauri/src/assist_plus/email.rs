pub struct FollowupDraft {
    pub subject: String,
    pub body: String,
}

pub fn draft_followup(title: &str, summary: &str) -> FollowupDraft {
    let title = if title.trim().is_empty() {
        "nossa reunião".to_string()
    } else {
        title.trim().chars().take(80).collect()
    };
    let body_text = summary.trim();
    let body = if body_text.is_empty() {
        "Olá,\n\nObrigado pela conversa. Segue um resumo em branco porque a reunião ainda não tem resumo salvo.\n\nAbraço".to_string()
    } else {
        format!("Olá,\n\nObrigado pela conversa. Segue o que ficou combinado:\n\n{body_text}\n\nSe algo estiver diferente do que você entendeu, me avise.\n\nAbraço")
    };
    FollowupDraft {
        subject: format!("Acompanhamento: {title}"),
        body,
    }
}

pub fn mailto(draft: &FollowupDraft) -> String {
    format!(
        "mailto:?subject={}&body={}",
        encode(&draft.subject),
        encode(&clip(&draft.body, 1500))
    )
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_uses_the_summary_and_does_not_send() {
        let draft = draft_followup("Aurora", "Ficou combinado o envio da proposta.");
        assert!(draft.subject.contains("Aurora"));
        assert!(draft.body.contains("envio da proposta"));
        let link = mailto(&draft);
        assert!(link.starts_with("mailto:?"));
        assert!(!link.contains("send"));
    }
}
