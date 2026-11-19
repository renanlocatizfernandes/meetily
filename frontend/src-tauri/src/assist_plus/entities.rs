/// Terms from the other participants that are worth a lookup.
/// Common function words and the meeting context are skipped.

const SKIP: &[&str] = &[
    "você", "voce", "pessoas", "participantes", "microsoft", "teams", "zoom", "meet",
    "google", "olá", "ola", "obrigado", "obrigada", "bom", "boa", "dia", "tarde", "noite",
    "sim", "não", "nao", "ok", "okay", "the", "and", "you", "this", "that", "with",
];

pub fn unknown_terms(text: &str, known: &str) -> Vec<String> {
    let known_norm = known.to_lowercase();
    let mut found = Vec::new();
    push_unique(&mut found, quoted(text));
    push_unique(&mut found, acronyms(text));
    push_unique(&mut found, names(text));
    found
        .into_iter()
        .filter(|term| {
            let norm = term.to_lowercase();
            norm.chars().count() >= 2
                && !SKIP.contains(&norm.as_str())
                && !known_norm.contains(&norm)
        })
        .take(3)
        .collect()
}

fn push_unique(into: &mut Vec<String>, items: Vec<String>) {
    for item in items {
        if !into.iter().any(|existing| existing.eq_ignore_ascii_case(&item)) {
            into.push(item);
        }
    }
}

fn quoted(text: &str) -> Vec<String> {
    let mut items = Vec::new();
    for quote in ['"', '“', '«'] {
        let close = match quote {
            '"' => '"',
            '“' => '”',
            _ => '»',
        };
        let mut rest = text;
        while let Some(start) = rest.find(quote) {
            rest = &rest[start + quote.len_utf8()..];
            let Some(end) = rest.find(close) else { break };
            let term = rest[..end].trim();
            if (2..=40).contains(&term.chars().count()) {
                items.push(term.to_string());
            }
            rest = &rest[end + close.len_utf8()..];
        }
    }
    items
}

fn acronyms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| {
            let chars: Vec<char> = word.chars().collect();
            (2..=6).contains(&chars.len()) && chars.iter().all(|c| c.is_ascii_uppercase())
        })
        .map(str::to_string)
        .collect()
}

fn names(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut items = Vec::new();
    let mut index = 0;
    while index < words.len() {
        if !capitalized(words[index]) {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < words.len() && capitalized(words[end]) {
            end += 1;
        }
        if end - index >= 2 || words[index].chars().count() >= 4 {
            let name = words[index..end].join(" ");
            let name = name.trim_matches(|c: char| !c.is_alphanumeric() && c != '-').to_string();
            if !name.is_empty() {
                items.push(name);
            }
        }
        index = end;
    }
    items
}

fn capitalized(word: &str) -> bool {
    let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => chars.next().is_some_and(|second| second.is_lowercase() || second.is_uppercase()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_company_an_acronym_and_a_quoted_term() {
        let text = "A Aurora Digital citou o LGPD e o produto \"Ledger X\" na proposta.";
        let terms = unknown_terms(text, "reunião semanal");
        assert!(terms.iter().any(|term| term == "Aurora Digital"));
        assert!(terms.iter().any(|term| term == "LGPD"));
        assert!(terms.iter().any(|term| term == "Ledger X"));
    }

    #[test]
    fn skips_terms_already_in_the_meeting_context() {
        let terms = unknown_terms("A Aurora Digital perguntou do prazo.", "Cliente Aurora Digital");
        assert!(terms.iter().all(|term| !term.eq_ignore_ascii_case("Aurora Digital")));
    }
}
