use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use super::trigger::TriggerKind;

/// Minimum quiet gap between accepted model calls.
pub fn frequency_gap(frequency: &str) -> Duration {
    match normalize_frequency(frequency) {
        "baixa" => Duration::from_secs(8),
        "alta" => Duration::from_secs(2),
        _ => Duration::from_secs(4),
    }
}

pub fn normalize_frequency(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "baixa" | "low" => "baixa",
        "alta" | "high" => "alta",
        "media" | "média" | "medio" | "medium" => "media",
        _ => "media",
    }
}

#[derive(Debug)]
pub enum RequestDecision {
    Debounced,
    Start {
        token: CancellationToken,
        cancelled_previous: bool,
        generation: u64,
    },
}

/// Debounces triggers and cancels the previous in-flight call when a new one is accepted.
pub struct InFlightGate {
    min_gap: Duration,
    last_accept: Option<Instant>,
    current: Option<CancellationToken>,
    generation: u64,
    parent: CancellationToken,
}

impl InFlightGate {
    pub fn new(parent: CancellationToken, min_gap: Duration) -> Self {
        Self {
            min_gap,
            last_accept: None,
            current: None,
            generation: 0,
            parent,
        }
    }

    pub fn set_min_gap(&mut self, min_gap: Duration) {
        self.min_gap = min_gap;
    }

    pub fn request(&mut self, now: Instant) -> RequestDecision {
        if let Some(last) = self.last_accept {
            if now.saturating_duration_since(last) < self.min_gap {
                return RequestDecision::Debounced;
            }
        }

        let mut cancelled_previous = false;
        if let Some(previous) = self.current.take() {
            if !previous.is_cancelled() {
                previous.cancel();
                cancelled_previous = true;
            }
        }

        let token = self.parent.child_token();
        self.current = Some(token.clone());
        self.last_accept = Some(now);
        self.generation = self.generation.saturating_add(1);
        RequestDecision::Start {
            token,
            cancelled_previous,
            generation: self.generation,
        }
    }

    /// True when `request` with this gap would start a call now. Changes nothing.
    pub fn would_accept(&self, now: Instant, gap: Duration) -> bool {
        self.last_accept.map_or(true, |last| now.saturating_duration_since(last) >= gap)
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation && generation != 0
    }
}

pub fn describe_trigger(kind: TriggerKind) -> &'static str {
    match kind {
        TriggerKind::SystemSentenceEnd => "system-sentence",
        TriggerKind::Question => "question",
        TriggerKind::Pause => "pause",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_names_map_to_gaps() {
        assert_eq!(frequency_gap("baixa"), Duration::from_secs(8));
        assert_eq!(frequency_gap("Média"), Duration::from_secs(4));
        assert_eq!(frequency_gap("alta"), Duration::from_secs(2));
        assert_eq!(frequency_gap("nope"), Duration::from_secs(4));
    }

    #[test]
    fn debounce_holds_the_gap_and_cancel_replaces_the_previous_call() {
        let parent = CancellationToken::new();
        let mut gate = InFlightGate::new(parent, Duration::from_millis(1500));
        let start = Instant::now();

        let RequestDecision::Start { token: first, cancelled_previous, .. } = gate.request(start) else {
            panic!("first trigger should start");
        };
        assert!(!cancelled_previous);
        assert!(!first.is_cancelled());

        assert!(matches!(
            gate.request(start + Duration::from_millis(500)),
            RequestDecision::Debounced
        ));
        assert!(!first.is_cancelled());

        let RequestDecision::Start {
            token: second,
            cancelled_previous,
            ..
        } = gate.request(start + Duration::from_millis(1500))
        else {
            panic!("trigger after the gap should start");
        };
        assert!(cancelled_previous);
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }

    #[test]
    fn cancelling_the_session_cancels_the_child_call() {
        let parent = CancellationToken::new();
        let mut gate = InFlightGate::new(parent.clone(), Duration::from_secs(1));
        let RequestDecision::Start { token, .. } = gate.request(Instant::now()) else {
            panic!("expected start");
        };
        parent.cancel();
        assert!(token.is_cancelled());
    }
}
