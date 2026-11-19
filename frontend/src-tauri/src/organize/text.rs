//! Small text helpers shared by the classifier and the title renderer.

/// Lowercases and removes the accents used in Portuguese and Spanish, so
/// "Revisão" and "revisao" compare equal.
pub fn fold(input: &str) -> String {
    input
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// Trims and collapses every run of whitespace into one space.
pub fn squash(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clamp_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    input.chars().take(max).collect::<String>().trim_end().to_string()
}

pub fn clamp_words(input: &str, max: usize) -> String {
    input.split_whitespace().take(max).collect::<Vec<_>>().join(" ")
}

pub fn word_count(input: &str) -> usize {
    input.split_whitespace().count()
}

pub fn capitalize_first(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Counts whole-term occurrences of `needle` in `haystack`. Both must already
/// be folded. A match has to start and end at a non-alphanumeric boundary, so
/// "vale" does not match inside "equivalente".
pub fn count_term(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut from = 0;
    while let Some(found) = haystack[from..].find(needle) {
        let start = from + found;
        let end = start + needle.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .map_or(true, |c| !c.is_alphanumeric());
        let after_ok = haystack[end..]
            .chars()
            .next()
            .map_or(true, |c| !c.is_alphanumeric());
        if before_ok && after_ok {
            count += 1;
        }
        from = end;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_removes_accents_and_case() {
        assert_eq!(fold("Revisão de CUSTOS – Ação"), "revisao de custos – acao");
    }

    #[test]
    fn count_term_respects_word_boundaries() {
        let text = fold("A Vale pediu. Equivalente não conta, mas vale, sim. Boreal-Sul");
        assert_eq!(count_term(&text, "vale"), 2);
        assert_eq!(count_term(&text, "boreal"), 1);
        assert_eq!(count_term(&text, ""), 0);
    }

    #[test]
    fn clamp_helpers() {
        assert_eq!(clamp_chars("abcdef", 3), "abc");
        assert_eq!(clamp_chars("ação", 10), "ação");
        assert_eq!(clamp_words("um dois  três quatro", 3), "um dois três");
        assert_eq!(capitalize_first("étapa final"), "Étapa final");
        assert_eq!(squash("  a \n b\t c "), "a b c");
    }
}
