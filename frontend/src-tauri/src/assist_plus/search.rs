#[derive(Debug, Clone)]
pub struct Doc {
    pub id: String,
    pub title: String,
    pub when: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub title: String,
    pub when: String,
    pub excerpt: String,
    pub score: i32,
}

pub fn rank_snippets(question: &str, docs: &[Doc]) -> Vec<Snippet> {
    let tokens = tokens(question);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut ranked = Vec::new();
    for doc in docs {
        let title = doc.title.to_lowercase();
        let text = doc.text.to_lowercase();
        let mut score = 0;
        for token in &tokens {
            if title.contains(token) {
                score += 3;
            }
            score += text.matches(token.as_str()).take(5).count() as i32;
        }
        if score == 0 {
            continue;
        }
        ranked.push(Snippet {
            id: doc.id.clone(),
            title: doc.title.clone(),
            when: doc.when.clone(),
            excerpt: excerpt(&doc.text, &tokens),
            score,
        });
    }
    ranked.sort_by(|left, right| right.score.cmp(&left.score).then(left.when.cmp(&right.when)));
    ranked.truncate(5);
    ranked
}

pub fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(|word| word.trim().to_lowercase())
        .filter(|word| word.chars().count() >= 3 && !STOP.contains(&word.as_str()))
        .collect()
}

const STOP: &[&str] = &[
    "que", "como", "para", "com", "uma", "uns", "das", "dos", "por", "sobre", "falou", "cliente",
    "the", "and", "what", "sobre",
];

fn excerpt(text: &str, tokens: &[String]) -> String {
    let lower = text.to_lowercase();
    let start = tokens
        .iter()
        .filter_map(|token| lower.find(token))
        .min()
        .unwrap_or(0);
    let start = start.saturating_sub(40);
    let end = (start + 180).min(text.len());
    let start = floor_char(text, start);
    let end = floor_char(text, end);
    text[start..end].trim().to_string()
}

fn floor_char(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_the_meeting_that_mentions_the_price() {
        let docs = vec![
            Doc {
                id: "1".into(),
                title: "Cliente Aurora".into(),
                when: "2026-11-01".into(),
                text: "Falamos da equipe e do prazo.".into(),
            },
            Doc {
                id: "2".into(),
                title: "Cliente Aurora".into(),
                when: "2026-11-10".into(),
                text: "O cliente pediu desconto no preço da licença.".into(),
            },
        ];
        let ranked = rank_snippets("o que o cliente Aurora falou sobre preço?", &docs);
        assert_eq!(ranked[0].id, "2");
        assert!(ranked[0].excerpt.to_lowercase().contains("preço"));
        assert!(ranked[0].score > ranked.get(1).map(|item| item.score).unwrap_or(0));
    }
}
