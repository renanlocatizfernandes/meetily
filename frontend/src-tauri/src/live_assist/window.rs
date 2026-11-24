use std::collections::VecDeque;

/// Recent talk kept after a trim.
pub const WINDOW_SECONDS: f64 = 75.0;
/// The recent talk is trimmed back to `WINDOW_SECONDS` only when it spans this
/// much. Between two trims the window only grows at the end, so consecutive
/// prompts share their beginning and the model server reuses what it already
/// read. Trimming on every push made each prompt differ from the previous one
/// near the top, and Ollama 0.40 then kept a new cache entry per call:
/// `llama3.2` went from 0.6 GiB to 9 GiB of memory in four minutes of calls.
/// With trims in batches the same load was at 3.2 GiB after nine minutes: the
/// cache still grows a little at each trim, so the trims are kept far apart.
const TRIM_AT_SECONDS: f64 = 180.0;
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
        let span = |window: &Self| window.segments.front().map_or(0.0, |piece| latest - piece.at);
        if span(self) <= TRIM_AT_SECONDS {
            return;
        }
        while span(self) > WINDOW_SECONDS {
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
    fn a_trim_keeps_the_last_75_seconds_and_rolls_the_rest() {
        let mut window = TranscriptWindow::default();
        window.push(piece(0.0, "mic", "Abertura da reunião"));
        window.push(piece(70.0, "system", "O cliente pediu o relatório"));
        window.push(piece(100.0, "system", "E perguntou do prazo."));
        // Nothing leaves the window before it spans three minutes.
        assert_eq!(window.recent().count(), 3);
        assert!(window.earlier_summary().is_empty());
        window.push(piece(190.0, "system", "Ficou para sexta."));
        window.push(piece(200.0, "mic", "Combinado."));

        let recent: Vec<_> = window.recent().map(|piece| piece.at).collect();
        assert_eq!(recent, vec![190.0, 200.0]);
        assert!(window.earlier_summary().contains("Abertura da reunião"));
        assert!(window.earlier_summary().contains("[mic]"));
        assert!(window.earlier_summary().contains("O cliente pediu o relatório"));
    }

    #[test]
    fn between_two_trims_the_window_only_grows_at_the_end() {
        let mut window = TranscriptWindow::default();
        let mut previous: Vec<f64> = Vec::new();
        let mut trims = 0;
        for step in 0..60 {
            window.push(piece(step as f64 * 10.0, "system", "fala"));
            let now: Vec<f64> = window.recent().map(|piece| piece.at).collect();
            if now.len() == previous.len() + 1 && now.starts_with(&previous) {
                assert_eq!(window.earlier_summary().is_empty(), trims == 0);
            } else {
                trims += 1;
                // A trim leaves the last 75 seconds.
                assert!(now.last().unwrap() - now.first().unwrap() <= WINDOW_SECONDS);
            }
            assert!(now.last().unwrap() - now.first().unwrap() <= TRIM_AT_SECONDS);
            previous = now;
        }
        // Ten minutes of talk: a trim about every two minutes, not one per push.
        assert!((3..=5).contains(&trims), "{trims}");
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
        window.push(piece(200.0, "mic", "agora"));
        assert!(window.earlier_summary().chars().count() <= 500 + "Resumo anterior: ".chars().count());
        assert!(window.earlier_summary().starts_with("Resumo anterior:"));
        assert_eq!(window.recent().count(), 1);
    }
}
