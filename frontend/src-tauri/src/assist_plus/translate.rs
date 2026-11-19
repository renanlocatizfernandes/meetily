#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingLanguage {
    Portuguese,
    English,
    Spanish,
}

pub fn detect_language(text: &str) -> Option<MeetingLanguage> {
    let lower = format!(" {} ", text.to_lowercase());
    let pt = count(&lower, &[" não ", " nao ", " você ", " voce ", " para ", " reunião ", " obrigado ", " pra "]);
    let en = count(&lower, &[" the ", " what ", " please ", " price ", " with ", " this ", " that ", " would "]);
    let es = count(&lower, &[" usted ", " también ", " tambien ", " gracias ", " precio ", " reunión ", " mañana ", " manana ", " el ", " la "]);
    let best = [
        (pt, MeetingLanguage::Portuguese),
        (en, MeetingLanguage::English),
        (es, MeetingLanguage::Spanish),
    ]
    .into_iter()
    .max_by_key(|(score, _)| *score)?;
    if best.0 < 2 {
        None
    } else {
        Some(best.1)
    }
}

pub fn should_translate(mode: &str, text: &str) -> bool {
    let language = detect_language(text);
    match mode {
        "on" => !matches!(language, Some(MeetingLanguage::Portuguese) | None),
        "auto" => matches!(language, Some(MeetingLanguage::English | MeetingLanguage::Spanish)),
        _ => false,
    }
}

fn count(text: &str, words: &[&str]) -> i32 {
    words.iter().filter(|word| text.contains(*word)).count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_mode_translates_english_and_spanish_only() {
        assert!(should_translate("auto", "What is the price with this discount please?"));
        assert!(should_translate("auto", "Usted también quiere el precio mañana, gracias."));
        assert!(!should_translate("auto", "Você não precisa decidir isso para a reunião agora."));
        assert!(!should_translate("off", "What is the price with this discount please?"));
    }
}
