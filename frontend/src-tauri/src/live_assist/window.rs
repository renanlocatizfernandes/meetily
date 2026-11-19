use std::collections::VecDeque;

pub const WINDOW_SECONDS: f64 = 75.0;
const EARLIER_CHAR_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptPiece {
    pub at: f64,
    pub source: String,
    pub text: String,
}

#[derive(Debug, Default, Clone)]
pub struct TranscriptWindow {
    segments: VecDeque<TranscriptPiece>,
    earlier: String,
}

impl TranscriptWindow {
    pub fn push(&mut self, piece: TranscriptPiece) {
        self.segments.push_back(piece);
        self.evict();
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty() && self.earlier.trim().is_empty()
    }

    pub fn earlier_summary(&self) -> &str {
        &self.earlier
    }

    pub fn recent(&self) -> impl Iterator<Item = &TranscriptPiece> {
        self.segments.iter()
    }

    fn evict(&mut self) {
        let Some(latest) = self.segments.back().map(|piece| piece.at) else {
            return;
        };
        while self
            .segments
            .front()
            .is_some_and(|piece| latest - piece.at > WINDOW_SECONDS)
        {
            let old = self.segments.pop_front().expect("front exists");
            self.earlier
                .push_str(&format!("[{}] {}\n", old.source, old.text.trim()));
        }
        if self.earlier.chars().count() > EARLIER_CHAR_LIMIT {
            let chars: Vec<char> = self.earlier.chars().collect();
            let start = chars.len() - EARLIER_CHAR_LIMIT;
            let tail: String = chars[start..].iter().collect();
            self.earlier = format!("Resumo anterior: {tail}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(at: f64, source: &str, text: &str) -> TranscriptPiece {
        TranscriptPiece {
            at,
            source: source.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn keeps_the_last_75_seconds_and_rolls_the_rest() {
        let mut window = TranscriptWindow::default();
        window.push(piece(0.0, "mic", "Abertura da reunião"));
        window.push(piece(70.0, "system", "O cliente pediu o relatório"));
        window.push(piece(80.0, "system", "E perguntou do prazo."));

        let recent: Vec<_> = window.recent().map(|piece| piece.at).collect();
        assert_eq!(recent, vec![70.0, 80.0]);
        assert!(window.earlier_summary().contains("Abertura da reunião"));
        assert!(window.earlier_summary().contains("[mic]"));
    }

    #[test]
    fn earlier_summary_stays_bounded() {
        let mut window = TranscriptWindow::default();
        let long = "palavra ".repeat(80);
        window.push(piece(0.0, "system", &long));
        window.push(piece(100.0, "mic", "agora"));
        assert!(window.earlier_summary().chars().count() <= 500 + "Resumo anterior: ".chars().count());
        assert!(window.earlier_summary().starts_with("Resumo anterior:"));
        assert_eq!(window.recent().count(), 1);
    }
}
