#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commitment {
    pub kind: &'static str,
    pub text: String,
}

pub fn detect_commitments(text: &str) -> Vec<Commitment> {
    let mut found = Vec::new();
    for sentence in text.split(|c| matches!(c, '.' | '!' | '?' | '\n')) {
        let sentence = sentence.trim();
        if sentence.chars().count() < 8 {
            continue;
        }
        let lower = sentence.to_lowercase();
        let kind = if has_money(&lower) {
            "valor"
        } else if has_deadline(&lower) {
            "prazo"
        } else if has_deliverable(&lower) {
            "entrega"
        } else {
            continue;
        };
        found.push(Commitment {
            kind,
            text: sentence.chars().take(180).collect(),
        });
        if found.len() == 3 {
            break;
        }
    }
    found
}

fn has_money(text: &str) -> bool {
    text.contains("r$")
        || text.contains("us$")
        || text.contains("reais")
        || text.contains("dólar")
        || text.contains("dolar")
}

fn has_deadline(text: &str) -> bool {
    text.contains("até ")
        || text.contains("ate ")
        || text.contains("prazo")
        || text.contains("deadline")
        || text.contains("amanhã")
        || text.contains("amanha")
        || text.contains("semana que vem")
        || text.contains("dia ")
}

fn has_deliverable(text: &str) -> bool {
    [
        "entreg",
        "enviar",
        "envia",
        "mando",
        "mandamos",
        "fechado",
        "combinado",
        "proposta",
        "contrato",
    ]
    .iter()
    .any(|word| text.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_a_price_a_date_and_a_deliverable() {
        let text = "Fica em R$ 12 mil. Envio a proposta até sexta. O contrato segue depois.";
        let found = detect_commitments(text);
        assert!(found.iter().any(|item| item.kind == "valor" && item.text.contains("R$")));
        assert!(found.iter().any(|item| item.kind == "prazo"));
        assert!(found.iter().any(|item| item.kind == "entrega"));
    }

    #[test]
    fn ignores_small_talk() {
        assert!(detect_commitments("Bom dia, tudo bem por aí?").is_empty());
    }
}
