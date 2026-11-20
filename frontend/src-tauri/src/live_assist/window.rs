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

    /// Moves the oldest recent pieces into the earlier summary until the recent
    /// text fits `max_chars`. Used by economy mode to send less.
    pub fn limit_recent_chars(&mut self, max_chars: usize) {
        let mut total: usize = self.segments.iter().map(|piece| piece.text.chars().count()).sum();
        while total > max_chars && self.segments.len() > 1 {
            let old = self.segments.pop_front().expect("front exists");
            total = total.saturating_sub(old.text.chars().count());
            self.earlier.push_str(&format!("[{}] {}\n", old.source, old.text.trim()));
        }
        self.trim_earlier();
    }

    fn trim_earlier(&mut self) {
        if self.earlier.chars().count() > EARLIER_CHAR_LIMIT {
            let chars: Vec<char> = self.earlier.chars().collect();
            let start = chars.len() - EARLIER_CHAR_LIMIT;
            let tail: String = chars[start..].iter().collect();
            self.earlier = format!("Resumo anterior: {tail}");
        }
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
        self.trim_earlier();
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
    fn limiting_recent_text_keeps_the_newest_piece_and_rolls_the_rest() {
        let mut window = TranscriptWindow::default();
        for (index, word) in ["um", "dois", "tres"].iter().enumerate() {
            window.push(piece(index as f64, "system", &format!("{word} {}", "texto ".repeat(20))));
        }
        window.limit_recent_chars(150);
        assert_eq!(window.recent().count(), 1);
        assert!(window.earlier_summary().contains("um texto"));
        // A single oversized piece is never dropped.
        window.limit_recent_chars(1);
        assert_eq!(window.recent().count(), 1);
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
