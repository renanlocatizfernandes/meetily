use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use regex::Regex;

static QUESTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(\?|\b(quem|qual|quais|quando|onde|por\s*qu[eê]|porque|como|quanto|quantos|quantas|who|what|when|where|why|how)\b)",
    )
    .expect("question regex")
});

pub const PAUSE_THRESHOLD: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    SystemSentenceEnd,
    Question,
    Pause,
}

pub fn ends_sentence(text: &str) -> bool {
    let trimmed = text.trim().trim_end_matches(|c: char| {
        matches!(c, '"' | '\'' | '”' | '’' | ')' | ']' | '»')
    });
    trimmed.ends_with(['.', '!', '?', '…'])
}

pub fn is_question(text: &str) -> bool {
    QUESTION_RE.is_match(text)
}

/// Immediate triggers for one final transcript segment.
/// A pause is detected separately from the wall clock.
pub fn classify_segment(source: &str, text: &str) -> Option<TriggerKind> {
    if is_question(text) {
        return Some(TriggerKind::Question);
    }
    if source == "system" && ends_sentence(text) {
        return Some(TriggerKind::SystemSentenceEnd);
    }
    None
}

/// Fires once per quiet stretch, then waits for new activity.
#[derive(Debug, Default)]
pub struct PauseWatch {
    last_activity: Option<Instant>,
    armed: bool,
}

impl PauseWatch {
    pub fn note_activity(&mut self, now: Instant) {
        self.last_activity = Some(now);
        self.armed = true;
    }

    pub fn take_pause(&mut self, now: Instant, threshold: Duration) -> bool {
        let Some(last) = self.last_activity else {
            return false;
        };
        if self.armed && now.saturating_duration_since(last) > threshold {
            self.armed = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_sentence_and_questions_trigger() {
        assert_eq!(
            classify_segment("system", "Vamos revisar o orçamento."),
            Some(TriggerKind::SystemSentenceEnd)
        );
        assert_eq!(
            classify_segment("system", "Qual é o prazo?"),
            Some(TriggerKind::Question)
        );
        assert_eq!(
            classify_segment("mic", "Como vocês medem isso"),
            Some(TriggerKind::Question)
        );
        assert_eq!(classify_segment("mic", "Entendi o ponto."), None);
        assert_eq!(classify_segment("system", "uh"), None);
    }

    #[test]
    fn pause_fires_once_until_the_next_segment() {
        let mut watch = PauseWatch::default();
        let start = Instant::now();
        assert!(!watch.take_pause(start, PAUSE_THRESHOLD));
        watch.note_activity(start);
        assert!(!watch.take_pause(start + Duration::from_millis(500), PAUSE_THRESHOLD));
        assert!(watch.take_pause(start + Duration::from_millis(1600), PAUSE_THRESHOLD));
        assert!(!watch.take_pause(start + Duration::from_millis(4000), PAUSE_THRESHOLD));
        watch.note_activity(start + Duration::from_millis(4100));
        assert!(watch.take_pause(start + Duration::from_millis(6000), PAUSE_THRESHOLD));
    }
}
