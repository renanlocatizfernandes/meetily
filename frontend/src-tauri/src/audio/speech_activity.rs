//! "Last speech" stamps for the microphone and the system audio.
//!
//! The live pipeline already runs one VAD per source. Each processed window
//! stores a monotonic stamp here when that VAD is inside speech or has just
//! closed a segment. It is one atomic store: no allocation, no lock, no log.
//! Readers (the meeting-end detector) only compare stamps; nothing here
//! touches the audio that goes to the recording or to transcription.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use once_cell::sync::Lazy;

use super::recording_state::DeviceType;

static ORIGIN: Lazy<Instant> = Lazy::new(Instant::now);
/// Milliseconds since `ORIGIN`, plus one so that zero means "never".
static MICROPHONE: AtomicU64 = AtomicU64::new(0);
static SYSTEM: AtomicU64 = AtomicU64::new(0);

/// Monotonic clock shared by the stamps and their readers.
pub fn now_ms() -> u64 {
    ORIGIN.elapsed().as_millis() as u64
}

/// Records that `source` carried speech just now.
#[inline]
pub fn mark(source: DeviceType) {
    let stamp = now_ms() + 1;
    match source {
        DeviceType::Microphone => MICROPHONE.store(stamp, Ordering::Relaxed),
        DeviceType::System => SYSTEM.store(stamp, Ordering::Relaxed),
    }
}

/// The most recent speech on either source, on the `now_ms` clock.
pub fn last_speech_ms() -> Option<u64> {
    latest(
        MICROPHONE.load(Ordering::Relaxed),
        SYSTEM.load(Ordering::Relaxed),
    )
}

fn latest(microphone: u64, system: u64) -> Option<u64> {
    microphone.max(system).checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_means_never_and_the_newest_source_wins() {
        assert_eq!(latest(0, 0), None);
        assert_eq!(latest(1, 0), Some(0));
        assert_eq!(latest(501, 0), Some(500));
        assert_eq!(latest(501, 901), Some(900));
        assert_eq!(latest(0, 41), Some(40));
    }

    #[test]
    fn a_mark_from_either_source_is_visible_and_not_in_the_future() {
        let before = now_ms();
        mark(DeviceType::System);
        let stamp = last_speech_ms().expect("a stamp after a mark");
        assert!(stamp >= before);
        assert!(stamp <= now_ms());

        let before = now_ms();
        mark(DeviceType::Microphone);
        let stamp = last_speech_ms().expect("a stamp after a mark");
        assert!(stamp >= before);
        assert!(stamp <= now_ms());
    }
}
