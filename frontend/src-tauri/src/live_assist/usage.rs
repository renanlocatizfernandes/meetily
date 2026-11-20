//! Consumption of the live assistant in the recording in progress, and the
//! economy mode that groups model calls to spend fewer tokens.
//!
//! The providers do not report usage through `generate_summary`, so tokens are
//! an estimate: about one token per four characters of prompt and answer.

use std::time::Duration;

use serde::Serialize;

use super::trigger::TriggerKind;

/// Economy mode: a question may call the model after this gap; anything else
/// (sentence ends, pauses) is grouped into one call per `ECONOMY_GROUP_GAP`.
pub const ECONOMY_QUESTION_GAP: Duration = Duration::from_secs(20);
pub const ECONOMY_GROUP_GAP: Duration = Duration::from_secs(45);
/// Economy keeps this many characters of the recent talk in a call (the normal window is 75 s).
pub const ECONOMY_MAX_RECENT_CHARS: usize = 1800;

pub fn estimate_tokens(chars: usize) -> u64 {
    (chars as u64 + 3) / 4
}

/// "auto" is economy for a cloud model only; "on" and "off" are manual.
pub fn economy_enabled(mode: &str, leaves_machine: bool) -> bool {
    match mode.trim() {
        "on" => true,
        "off" => false,
        _ => leaves_machine,
    }
}

/// Minimum gap since the last accepted call, for a trigger in economy mode.
pub fn economy_gap(kind: TriggerKind) -> Duration {
    match kind {
        TriggerKind::Question => ECONOMY_QUESTION_GAP,
        TriggerKind::SystemSentenceEnd | TriggerKind::Pause => ECONOMY_GROUP_GAP,
    }
}

/// In economy mode only a question starts a call right away; the other
/// triggers wait for the group flush.
pub fn economy_starts_immediately(kind: TriggerKind) -> bool {
    kind == TriggerKind::Question
}

#[derive(Debug, Clone, Default)]
pub struct UsageMeter {
    calls: u64,
    input_chars: u64,
    output_chars: u64,
    /// Calls that economy mode held back or merged into a later one.
    grouped: u64,
}

impl UsageMeter {
    pub fn record_call(&mut self, input_chars: usize, output_chars: usize) {
        self.calls += 1;
        self.input_chars += input_chars as u64;
        self.output_chars += output_chars as u64;
    }

    pub fn record_grouped(&mut self) {
        self.grouped += 1;
    }

    pub fn snapshot(&self, paid: bool, economy: bool) -> UsageSnapshot {
        UsageSnapshot {
            calls: self.calls,
            input_tokens: estimate_tokens(self.input_chars as usize),
            output_tokens: estimate_tokens(self.output_chars as usize),
            grouped: self.grouped,
            paid,
            economy,
        }
    }
}

/// Shown in the panel. `paid` is true when the text goes to a cloud provider.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub grouped: u64,
    pub paid: bool,
    pub economy: bool,
}

/// Keeps only the tail of `text` within `max_chars`, cutting at a line start.
pub fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().skip(total - max_chars).collect();
    match cut.find('\n') {
        Some(index) if index + 1 < cut.len() => cut[index + 1..].to_string(),
        _ => cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn economy_auto_follows_the_provider() {
        assert!(economy_enabled("auto", true));
        assert!(!economy_enabled("auto", false));
        assert!(economy_enabled("on", false));
        assert!(!economy_enabled("off", true));
    }

    #[test]
    fn economy_waits_longer_for_everything_but_questions() {
        assert!(economy_starts_immediately(TriggerKind::Question));
        assert!(!economy_starts_immediately(TriggerKind::Pause));
        assert!(!economy_starts_immediately(TriggerKind::SystemSentenceEnd));
        assert!(economy_gap(TriggerKind::Question) < economy_gap(TriggerKind::Pause));
        assert_eq!(economy_gap(TriggerKind::SystemSentenceEnd), ECONOMY_GROUP_GAP);
    }

    #[test]
    fn meter_estimates_tokens_and_counts_grouped_calls() {
        let mut meter = UsageMeter::default();
        meter.record_call(4000, 400);
        meter.record_call(1, 1);
        meter.record_grouped();
        let snapshot = meter.snapshot(true, true);
        assert_eq!(snapshot.calls, 2);
        assert_eq!(snapshot.input_tokens, 1001);
        assert_eq!(snapshot.output_tokens, 101);
        assert_eq!(snapshot.grouped, 1);
        assert!(snapshot.paid && snapshot.economy);
        assert_eq!(estimate_tokens(0), 0);
    }

    #[test]
    fn tail_chars_cuts_at_a_line_start() {
        assert_eq!(tail_chars("abc", 10), "abc");
        let text = "linha um\nlinha dois\nlinha tres";
        let tail = tail_chars(text, 14);
        assert_eq!(tail, "linha tres");
        assert!(tail_chars("x".repeat(50).as_str(), 10).chars().count() <= 10);
    }
}
