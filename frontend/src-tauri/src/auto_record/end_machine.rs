//! Decides when a running recording has outlived its meeting.
//!
//! Two kinds of evidence, for any recording (started by hand or not):
//!
//! - **Signal**: a call was seen during the recording and has been gone for
//!   `SIGNAL_HOLD`. A recording the start detector owns is left to that
//!   detector (`machine.rs`), which already stops it on the same hold.
//! - **Silence**: no speech on the microphone or the system audio for the
//!   configured time. While a call is still on screen the limit is
//!   `SILENCE_FACTOR_IN_CALL` times longer (a lobby is quiet too).
//!
//! What happens next depends on the mode: stop, ask (`Prompting`, with a
//! deadline that stops), or nothing. The machine is pure: the service feeds
//! it one `EndInput` per tick and carries out the returned `EndAction`.

use std::time::Duration;

/// How long the call must stay gone. Same hold the start detector uses to stop.
pub const SIGNAL_HOLD: Duration = super::machine::STOP_HOLD;
/// After "keep recording", no new warning for this long.
pub const CONTINUE_SNOOZE: Duration = Duration::from_secs(10 * 60);
/// A stop that did not take effect is evaluated again after this long.
pub const STOP_RETRY: Duration = Duration::from_secs(30);
/// Silence limit multiplier while a call is still detected.
pub const SILENCE_FACTOR_IN_CALL: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndMode {
    Off,
    Ask,
    Auto,
}

/// The evidence that opened a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Signal,
    Silence,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            EndReason::Signal => "signal",
            EndReason::Silence => "silence",
        }
    }
}

/// Why the recording is being stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    Signal,
    Silence,
    /// The warning got no answer before its deadline.
    Timeout,
    /// The user chose "stop now" on the warning.
    User,
}

impl StopCause {
    pub fn as_str(self) -> &'static str {
        match self {
            StopCause::Signal => "signal",
            StopCause::Silence => "silence",
            StopCause::Timeout => "timeout",
            StopCause::User => "user",
        }
    }
}

/// Why an open warning was withdrawn without stopping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelCause {
    /// Speech came back (or the recording was paused) during a silence warning.
    Speech,
    /// A call is on screen again.
    Signal,
    /// The user chose "keep recording".
    Continued,
    /// The recording was stopped some other way while the warning was open.
    RecordingStopped,
    /// The mode was switched off while the warning was open.
    Disabled,
}

impl CancelCause {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelCause::Speech => "speech",
            CancelCause::Signal => "signal",
            CancelCause::Continued => "continued",
            CancelCause::RecordingStopped => "recordingStopped",
            CancelCause::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndAnswer {
    Stop,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndAction {
    /// Warn the user and wait for an answer.
    Prompt(EndReason),
    /// Withdraw the open warning.
    Cancel(CancelCause),
    /// Stop the recording through the normal stop path.
    Stop(StopCause),
}

#[derive(Debug, Clone)]
pub struct EndInput {
    /// Monotonic milliseconds, same clock as `last_speech_ms`.
    pub now_ms: u64,
    pub recording: bool,
    pub paused: bool,
    /// The start detector owns this recording and stops it when its call ends.
    pub owned: bool,
    /// A call is detected right now.
    pub signal: bool,
    /// Last speech on either source, if any was ever seen.
    pub last_speech_ms: Option<u64>,
    pub mode: EndMode,
    pub silence_limit_ms: u64,
    pub prompt_timeout_ms: u64,
    /// The user's answer to the open warning, if one arrived since the last step.
    pub answer: Option<EndAnswer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndPhase {
    /// Nothing is being recorded.
    Idle,
    Watching,
    Prompting {
        reason: EndReason,
        since_ms: u64,
        /// Whether a call was on screen when the warning opened.
        signal_at_open: bool,
    },
    Stopping { since_ms: u64 },
}

/// What is known about the recording in progress.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Session {
    /// Start of the current silence: the last speech, or the moment the
    /// recording started, resumed, or was told to continue.
    quiet_since_ms: u64,
    saw_call: bool,
    gone_since_ms: Option<u64>,
    /// The call's disappearance was already reported; wait for a new call.
    signal_spent: bool,
    snooze_until_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndMachine {
    phase: EndPhase,
    session: Session,
}

impl Default for EndMachine {
    fn default() -> Self {
        Self {
            phase: EndPhase::Idle,
            session: Session::default(),
        }
    }
}

impl EndMachine {
    #[cfg(test)]
    pub fn phase(&self) -> &EndPhase {
        &self.phase
    }

    pub fn is_idle(&self) -> bool {
        self.phase == EndPhase::Idle
    }

    pub fn step(&mut self, input: &EndInput) -> Option<EndAction> {
        let now = input.now_ms;

        if !input.recording {
            let was_prompting = matches!(self.phase, EndPhase::Prompting { .. });
            self.phase = EndPhase::Idle;
            return was_prompting.then_some(EndAction::Cancel(CancelCause::RecordingStopped));
        }

        if self.phase == EndPhase::Idle {
            self.session = Session {
                quiet_since_ms: now,
                ..Session::default()
            };
            self.phase = EndPhase::Watching;
        }

        self.observe(input);

        if input.mode == EndMode::Off {
            let was_prompting = matches!(self.phase, EndPhase::Prompting { .. });
            self.phase = EndPhase::Watching;
            return was_prompting.then_some(EndAction::Cancel(CancelCause::Disabled));
        }

        match self.phase.clone() {
            EndPhase::Idle => None,
            EndPhase::Watching => {
                let reason = self.evidence(input)?;
                if input.mode == EndMode::Auto {
                    Some(self.stop(now, cause_of(reason)))
                } else {
                    self.phase = EndPhase::Prompting {
                        reason,
                        since_ms: now,
                        signal_at_open: input.signal,
                    };
                    Some(EndAction::Prompt(reason))
                }
            }
            EndPhase::Prompting {
                reason,
                since_ms,
                signal_at_open,
            } => {
                match input.answer {
                    Some(EndAnswer::Stop) => return Some(self.stop(now, StopCause::User)),
                    Some(EndAnswer::Continue) => {
                        self.session.snooze_until_ms = now + CONTINUE_SNOOZE.as_millis() as u64;
                        self.session.signal_spent = true;
                        self.session.quiet_since_ms = now;
                        self.phase = EndPhase::Watching;
                        return Some(EndAction::Cancel(CancelCause::Continued));
                    }
                    None => {}
                }
                if input.signal && !signal_at_open {
                    // A call is back, or a new one started: the meeting goes on.
                    self.session.quiet_since_ms = now;
                    self.phase = EndPhase::Watching;
                    return Some(EndAction::Cancel(CancelCause::Signal));
                }
                if reason == EndReason::Silence && self.session.quiet_since_ms > since_ms {
                    self.phase = EndPhase::Watching;
                    return Some(EndAction::Cancel(CancelCause::Speech));
                }
                if input.mode == EndMode::Auto {
                    // The mode changed to "stop by itself" while the warning was open.
                    return Some(self.stop(now, cause_of(reason)));
                }
                if now.saturating_sub(since_ms) >= input.prompt_timeout_ms {
                    return Some(self.stop(now, StopCause::Timeout));
                }
                None
            }
            EndPhase::Stopping { since_ms } => {
                if now.saturating_sub(since_ms) >= STOP_RETRY.as_millis() as u64 {
                    // The stop did not take effect; look at the evidence again.
                    self.phase = EndPhase::Watching;
                }
                None
            }
        }
    }

    fn stop(&mut self, now: u64, cause: StopCause) -> EndAction {
        self.phase = EndPhase::Stopping { since_ms: now };
        EndAction::Stop(cause)
    }

    fn observe(&mut self, input: &EndInput) {
        let now = input.now_ms;
        let session = &mut self.session;
        if input.signal {
            session.saw_call = true;
            session.gone_since_ms = None;
            session.signal_spent = false;
        } else if session.saw_call && session.gone_since_ms.is_none() {
            session.gone_since_ms = Some(now);
        }
        if let Some(speech) = input.last_speech_ms {
            let speech = speech.min(now);
            if speech > session.quiet_since_ms {
                session.quiet_since_ms = speech;
            }
        }
        if input.paused {
            // A paused recording hears nothing; that is not a silent meeting.
            session.quiet_since_ms = now;
        }
    }

    fn evidence(&self, input: &EndInput) -> Option<EndReason> {
        let now = input.now_ms;
        let session = &self.session;
        if now < session.snooze_until_ms {
            return None;
        }
        let call_gone = session
            .gone_since_ms
            .is_some_and(|gone| now.saturating_sub(gone) >= SIGNAL_HOLD.as_millis() as u64);
        if call_gone && !session.signal_spent && !input.owned {
            return Some(EndReason::Signal);
        }
        let limit = if input.signal {
            input.silence_limit_ms.saturating_mul(SILENCE_FACTOR_IN_CALL)
        } else {
            input.silence_limit_ms
        };
        if !input.paused && now.saturating_sub(session.quiet_since_ms) >= limit {
            return Some(EndReason::Silence);
        }
        None
    }
}

fn cause_of(reason: EndReason) -> StopCause {
    match reason {
        EndReason::Signal => StopCause::Signal,
        EndReason::Silence => StopCause::Silence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SILENCE: u64 = 3 * 60 * 1000;
    const TIMEOUT: u64 = 60 * 1000;
    const HOLD: u64 = SIGNAL_HOLD.as_millis() as u64;

    /// A recording in progress, by hand, nobody speaking, no call, "ask" mode.
    fn at(now_ms: u64) -> EndInput {
        EndInput {
            now_ms,
            recording: true,
            paused: false,
            owned: false,
            signal: false,
            last_speech_ms: None,
            mode: EndMode::Ask,
            silence_limit_ms: SILENCE,
            prompt_timeout_ms: TIMEOUT,
            answer: None,
        }
    }

    /// Steps every 2 s from `from` to `to` with speech right up to each step.
    fn talk(machine: &mut EndMachine, from: u64, to: u64, signal: bool) {
        let mut now = from;
        while now <= to {
            let action = machine.step(&EndInput {
                signal,
                last_speech_ms: Some(now),
                ..at(now)
            });
            assert_eq!(action, None, "unexpected action at {now} ms");
            now += 2_000;
        }
    }

    #[test]
    fn call_gone_asks_about_a_manual_recording_and_the_deadline_stops_it() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 60_000, true);
        // The call disappears; people (or a video) are still audible.
        let gone = 62_000;
        assert_eq!(machine.step(&EndInput { last_speech_ms: Some(gone), ..at(gone) }), None);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(gone + HOLD - 2_000), ..at(gone + HOLD - 2_000) }),
            None
        );
        let asked = gone + HOLD;
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(asked), ..at(asked) }),
            Some(EndAction::Prompt(EndReason::Signal))
        );
        // Speech does not withdraw a warning about a call that ended.
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(asked + 30_000), ..at(asked + 30_000) }),
            None
        );
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(asked + TIMEOUT), ..at(asked + TIMEOUT) }),
            Some(EndAction::Stop(StopCause::Timeout))
        );
        assert!(matches!(machine.phase(), EndPhase::Stopping { .. }));
        assert_eq!(machine.step(&EndInput { recording: false, ..at(asked + TIMEOUT + 2_000) }), None);
        assert!(machine.is_idle());
    }

    #[test]
    fn call_gone_stops_a_manual_recording_in_auto_mode() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        assert_eq!(machine.step(&EndInput { mode: EndMode::Auto, last_speech_ms: Some(12_000), ..at(12_000) }), None);
        assert_eq!(
            machine.step(&EndInput { mode: EndMode::Auto, last_speech_ms: Some(12_000 + HOLD), ..at(12_000 + HOLD) }),
            Some(EndAction::Stop(StopCause::Signal))
        );
    }

    #[test]
    fn a_call_that_flickers_does_not_count_as_gone() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        talk(&mut machine, 12_000, 12_000 + HOLD - 2_000, false);
        talk(&mut machine, 12_000 + HOLD, 12_000 + HOLD + 4_000, true);
        // Gone again: the hold starts over.
        talk(&mut machine, 40_000, 40_000 + HOLD - 2_000, false);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(40_000 + HOLD), ..at(40_000 + HOLD) }),
            Some(EndAction::Prompt(EndReason::Signal))
        );
    }

    #[test]
    fn the_call_signal_is_left_to_the_start_detector_for_a_recording_it_owns() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        let mut now = 12_000;
        while now <= 12_000 + 3 * HOLD {
            assert_eq!(
                machine.step(&EndInput { owned: true, last_speech_ms: Some(now), ..at(now) }),
                None
            );
            now += 2_000;
        }
    }

    #[test]
    fn long_silence_asks_without_any_call() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 30_000, false);
        assert_eq!(machine.step(&EndInput { last_speech_ms: Some(30_000), ..at(30_000 + SILENCE - 2_000) }), None);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(30_000), ..at(30_000 + SILENCE) }),
            Some(EndAction::Prompt(EndReason::Silence))
        );
    }

    #[test]
    fn silence_is_measured_from_the_start_when_nobody_ever_spoke() {
        let mut machine = EndMachine::default();
        // A stamp from an earlier recording must not count against this one.
        assert_eq!(machine.step(&EndInput { last_speech_ms: Some(1_000), ..at(500_000) }), None);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(1_000), ..at(500_000 + SILENCE - 2_000) }),
            None
        );
        assert_eq!(
            machine.step(&EndInput { mode: EndMode::Auto, last_speech_ms: Some(1_000), ..at(500_000 + SILENCE) }),
            Some(EndAction::Stop(StopCause::Silence))
        );
    }

    #[test]
    fn silence_takes_longer_while_a_call_is_still_detected() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        assert_eq!(
            machine.step(&EndInput { signal: true, last_speech_ms: Some(10_000), ..at(10_000 + SILENCE) }),
            None
        );
        let stuck = 10_000 + SILENCE * SILENCE_FACTOR_IN_CALL;
        assert_eq!(
            machine.step(&EndInput { signal: true, last_speech_ms: Some(10_000), ..at(stuck) }),
            Some(EndAction::Prompt(EndReason::Silence))
        );
        // Also for a recording the start detector owns: its signal is stuck.
        let mut owned = EndMachine::default();
        assert_eq!(owned.step(&EndInput { owned: true, signal: true, ..at(0) }), None);
        assert_eq!(
            owned.step(&EndInput { owned: true, signal: true, ..at(SILENCE * SILENCE_FACTOR_IN_CALL) }),
            Some(EndAction::Prompt(EndReason::Silence))
        );
    }

    #[test]
    fn speech_coming_back_withdraws_the_silence_warning() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(machine.step(&at(SILENCE + 10_000)), None);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(SILENCE + 11_000), ..at(SILENCE + 12_000) }),
            Some(EndAction::Cancel(CancelCause::Speech))
        );
        assert_eq!(machine.phase(), &EndPhase::Watching);
        // The deadline of the withdrawn warning does nothing.
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(SILENCE + 11_000), ..at(SILENCE + TIMEOUT) }),
            None
        );
        // A new full silence is new evidence.
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(SILENCE + 11_000), ..at(2 * SILENCE + 11_000) }),
            Some(EndAction::Prompt(EndReason::Silence))
        );
    }

    #[test]
    fn a_call_showing_up_withdraws_the_warning() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(
            machine.step(&EndInput { signal: true, ..at(SILENCE + 2_000) }),
            Some(EndAction::Cancel(CancelCause::Signal))
        );

        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        talk(&mut machine, 12_000, 12_000 + HOLD - 2_000, false);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(12_000 + HOLD), ..at(12_000 + HOLD) }),
            Some(EndAction::Prompt(EndReason::Signal))
        );
        assert_eq!(
            machine.step(&EndInput { signal: true, last_speech_ms: Some(12_000 + HOLD), ..at(14_000 + HOLD) }),
            Some(EndAction::Cancel(CancelCause::Signal))
        );
    }

    #[test]
    fn continue_snoozes_and_only_new_evidence_asks_again() {
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        talk(&mut machine, 12_000, 12_000 + HOLD - 2_000, false);
        let asked = 12_000 + HOLD;
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(asked), ..at(asked) }),
            Some(EndAction::Prompt(EndReason::Signal))
        );
        let kept = asked + 4_000;
        assert_eq!(
            machine.step(&EndInput { answer: Some(EndAnswer::Continue), last_speech_ms: Some(asked), ..at(kept) }),
            Some(EndAction::Cancel(CancelCause::Continued))
        );
        // The same ended call never asks again, and silence waits for the snooze.
        let snooze = CONTINUE_SNOOZE.as_millis() as u64;
        let mut now = kept + 2_000;
        while now < kept + snooze {
            assert_eq!(machine.step(&EndInput { last_speech_ms: Some(asked), ..at(now) }), None);
            now += 2_000;
        }
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(asked), ..at(kept + snooze) }),
            Some(EndAction::Prompt(EndReason::Silence))
        );

        // With people still talking after the snooze, nothing asks again...
        let mut machine = EndMachine::default();
        talk(&mut machine, 0, 10_000, true);
        talk(&mut machine, 12_000, 12_000 + HOLD - 2_000, false);
        assert!(machine.step(&EndInput { last_speech_ms: Some(asked), ..at(asked) }).is_some());
        assert!(machine
            .step(&EndInput { answer: Some(EndAnswer::Continue), last_speech_ms: Some(asked), ..at(kept) })
            .is_some());
        talk(&mut machine, kept + 2_000, kept + snooze + 60_000, false);
        // ...until a new call comes and goes.
        let again = kept + snooze + 62_000;
        talk(&mut machine, again, again + 10_000, true);
        talk(&mut machine, again + 12_000, again + 10_000 + HOLD, false);
        assert_eq!(
            machine.step(&EndInput { last_speech_ms: Some(again + 12_000 + HOLD), ..at(again + 12_000 + HOLD) }),
            Some(EndAction::Prompt(EndReason::Signal))
        );
    }

    #[test]
    fn stop_now_stops_with_the_user_as_the_cause() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(
            machine.step(&EndInput { answer: Some(EndAnswer::Stop), ..at(SILENCE + 3_000) }),
            Some(EndAction::Stop(StopCause::User))
        );
    }

    #[test]
    fn no_answer_before_the_deadline_stops() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(machine.step(&at(SILENCE + TIMEOUT - 2_000)), None);
        assert_eq!(machine.step(&at(SILENCE + TIMEOUT)), Some(EndAction::Stop(StopCause::Timeout)));
        // One stop per decision, even if the recording takes a moment to end.
        assert_eq!(machine.step(&at(SILENCE + TIMEOUT + 2_000)), None);
    }

    #[test]
    fn off_mode_never_asks_nor_stops_and_closes_an_open_warning() {
        let mut machine = EndMachine::default();
        let mut now = 0;
        while now <= 10 * SILENCE {
            // A call for the first minute, then nothing at all.
            let input = EndInput { mode: EndMode::Off, signal: now < 60_000, ..at(now) };
            assert_eq!(machine.step(&input), None);
            now += 2_000;
        }

        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(
            machine.step(&EndInput { mode: EndMode::Off, ..at(SILENCE + 2_000) }),
            Some(EndAction::Cancel(CancelCause::Disabled))
        );
        assert_eq!(machine.step(&EndInput { mode: EndMode::Off, ..at(SILENCE + TIMEOUT) }), None);
    }

    #[test]
    fn a_manual_stop_during_the_warning_closes_it_and_the_next_recording_starts_clean() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(
            machine.step(&EndInput { recording: false, ..at(SILENCE + 4_000) }),
            Some(EndAction::Cancel(CancelCause::RecordingStopped))
        );
        assert!(machine.is_idle());
        assert_eq!(machine.step(&EndInput { recording: false, ..at(SILENCE + 6_000) }), None);
        // A new recording right away: the old silence is not held against it.
        assert_eq!(machine.step(&at(SILENCE + 8_000)), None);
        assert_eq!(machine.step(&at(2 * SILENCE + 6_000)), None);
        assert_eq!(
            machine.step(&at(2 * SILENCE + 8_000)),
            Some(EndAction::Prompt(EndReason::Silence))
        );
    }

    #[test]
    fn a_paused_recording_is_not_a_silent_meeting() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        let mut now = 2_000;
        while now <= 2 * SILENCE {
            assert_eq!(machine.step(&EndInput { paused: true, ..at(now) }), None);
            now += 2_000;
        }
        // Resumed: the silence counts from the resume.
        assert_eq!(machine.step(&at(2 * SILENCE + SILENCE - 2_000)), None);
        assert_eq!(
            machine.step(&at(2 * SILENCE + SILENCE)),
            Some(EndAction::Prompt(EndReason::Silence))
        );
    }

    #[test]
    fn switching_to_auto_during_the_warning_stops_and_a_failed_stop_is_retried() {
        let mut machine = EndMachine::default();
        assert_eq!(machine.step(&at(0)), None);
        assert_eq!(machine.step(&at(SILENCE)), Some(EndAction::Prompt(EndReason::Silence)));
        assert_eq!(
            machine.step(&EndInput { mode: EndMode::Auto, ..at(SILENCE + 2_000) }),
            Some(EndAction::Stop(StopCause::Silence))
        );
        let retry = STOP_RETRY.as_millis() as u64;
        assert_eq!(machine.step(&EndInput { mode: EndMode::Auto, ..at(SILENCE + retry) }), None);
        assert_eq!(machine.step(&EndInput { mode: EndMode::Auto, ..at(SILENCE + 2_000 + retry) }), None);
        assert_eq!(
            machine.step(&EndInput { mode: EndMode::Auto, ..at(SILENCE + 4_000 + retry) }),
            Some(EndAction::Stop(StopCause::Silence))
        );
    }
}
