use std::collections::{HashSet, VecDeque};

const MAX_REMEMBERED: usize = 30;
const SIMILARITY: f64 = 0.8;

fn normalize(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if ch.is_whitespace() {
            out.push(' ');
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokens(text: &str) -> HashSet<String> {
    normalize(text)
        .split_whitespace()
        .filter(|word| word.chars().count() > 1 || word.chars().all(|ch| ch.is_numeric()))
        .map(str::to_string)
        .collect()
}

pub fn too_similar(left: &str, right: &str) -> bool {
    let a = normalize(left);
    let b = normalize(right);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let ta = tokens(left);
    let tb = tokens(right);
    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    let intersection = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    if union == 0.0 {
        return false;
    }
    intersection / union >= SIMILARITY
}

#[derive(Debug, Default, Clone)]
pub struct ShownSuggestions {
    perguntas: VecDeque<String>,
    respostas: VecDeque<String>,
    contexto: VecDeque<String>,
}

impl ShownSuggestions {
    fn bucket(&mut self, section: &str) -> Option<&mut VecDeque<String>> {
        match section {
            "perguntas" => Some(&mut self.perguntas),
            "respostas" => Some(&mut self.respostas),
            "contexto" => Some(&mut self.contexto),
            _ => None,
        }
    }

    pub fn remember(&mut self, section: &str, text: &str) {
        let Some(bucket) = self.bucket(section) else {
            return;
        };
        bucket.push_front(text.to_string());
        while bucket.len() > MAX_REMEMBERED {
            bucket.pop_back();
        }
    }

    pub fn is_repeat(&self, section: &str, text: &str) -> bool {
        let bucket = match section {
            "perguntas" => &self.perguntas,
            "respostas" => &self.respostas,
            "contexto" => &self.contexto,
            _ => return true,
        };
        bucket.iter().any(|previous| too_similar(previous, text))
    }

    pub fn lines(&self) -> Vec<String> {
        self.perguntas
            .iter()
            .chain(self.respostas.iter())
            .chain(self.contexto.iter())
            .take(12)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_and_near_copies_are_suppressed() {
        let mut shown = ShownSuggestions::default();
        shown.remember("perguntas", "Qual é o prazo do projeto?");
        assert!(shown.is_repeat("perguntas", "qual e o prazo do projeto?"));
        assert!(!shown.is_repeat("respostas", "Qual é o prazo do projeto?"));
        assert!(!shown.is_repeat("perguntas", "Quem aprova o orçamento?"));
    }
}
