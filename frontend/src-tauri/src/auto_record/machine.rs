//! Debounced meeting detector. A signal must hold before a recording starts,
//! and it must stay gone before that recording stops. A recording the user
//! started by hand is never stopped here.
//!
//! In "ask" mode the held signal does not start anything: the machine asks
//! first (`Action::Prompt`) and waits in `Phase::Prompting` for the answer.
//! A refused prompt ends in `Phase::Suppressed`, the same state a manual stop
//! of an automatic recording uses, so the same call does not ask again.

use std::time::{Duration, Instant};

use super::detect::MeetingApp;

pub const START_HOLD: Duration = Duration::from_secs(12);
pub const STOP_HOLD: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Arming { app: MeetingApp, since_ms: u64 },
    /// Waiting for the user to accept or refuse the recording ("ask" mode).
    Prompting { app: MeetingApp },
    Starting { app: MeetingApp, since_ms: u64 },
    Active { app: MeetingApp, owned: bool },
    Clearing { app: MeetingApp, since_ms: u64 },
    Stopping { since_ms: u64 },
    Suppressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start(MeetingApp),
    /// Ask the user whether this call should be recorded.
    Prompt(MeetingApp),
    Stop,
}

pub struct StepInput {
    pub now: Instant,
    pub origin: Instant,
    pub signal: Option<MeetingApp>,
    pub recording: bool,
    pub owned: bool,
    pub enabled: bool,
    pub user_hold: bool,
    /// "ask" mode: confirm with the user before starting.
    pub ask: bool,
    /// The user's answer to the open prompt, if one arrived since the last step.
    pub answer: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorMachine {
    phase: Phase,
}

impl Default for DetectorMachine {
    fn default() -> Self {
        Self { phase: Phase::Idle }
    }
}

impl DetectorMachine {
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn phase_name(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "idle",
            Phase::Arming { .. } => "arming",
            Phase::Prompting { .. } => "prompting",
            Phase::Starting { .. } => "starting",
            Phase::Active { .. } => "active",
            Phase::Clearing { .. } => "clearing",
            Phase::Stopping { .. } => "stopping",
            Phase::Suppressed => "suppressed",
        }
    }

    pub fn seconds_until_start(&self, now: Instant, origin: Instant) -> Option<u64> {
        let Phase::Arming { since_ms, .. } = self.phase else {
            return None;
        };
        let held = elapsed_ms(origin, now).saturating_sub(since_ms) / 1000;
        Some(START_HOLD.as_secs().saturating_sub(held))
    }

    pub fn step(&mut self, input: &StepInput) -> Option<Action> {
        let now_ms = elapsed_ms(input.origin, input.now);
        if input.user_hold {
            if input.signal.is_none() && !input.recording {
                self.phase = Phase::Idle;
            } else {
                self.phase = Phase::Suppressed;
            }
            return None;
        }

        if !input.enabled && !input.owned {
            self.phase = Phase::Idle;
            return None;
        }

        match self.phase.clone() {
            Phase::Idle => {
                if let Some(app) = input.signal {
                    if input.recording && !input.owned {
                        self.phase = Phase::Active { app, owned: false };
                    } else if !input.recording {
                        self.phase = Phase::Arming { app, since_ms: now_ms };
                    }
                }
                None
            }
            Phase::Arming { app, since_ms } => match input.signal {
                Some(next) if next == app => {
                    if input.recording {
                        self.phase = Phase::Active {
                            app,
                            owned: input.owned,
                        };
                        None
                    } else if now_ms.saturating_sub(since_ms) >= START_HOLD.as_millis() as u64 {
                        if input.ask {
                            self.phase = Phase::Prompting { app };
                            Some(Action::Prompt(app))
                        } else {
                            self.phase = Phase::Starting { app, since_ms: now_ms };
                            Some(Action::Start(app))
                        }
                    } else {
                        None
                    }
                }
                Some(next) => {
                    self.phase = Phase::Arming {
                        app: next,
                        since_ms: now_ms,
                    };
                    None
                }
                None => {
                    self.phase = Phase::Idle;
                    None
                }
            },
            Phase::Prompting { app } => {
                if input.signal.is_none() {
                    // The call ended before an answer.
                    self.phase = Phase::Idle;
                    return None;
                }
                if input.recording {
                    // The user started a recording by hand while the prompt was open.
                    self.phase = Phase::Active {
                        app,
                        owned: input.owned,
                    };
                    return None;
                }
                match input.answer {
                    Some(true) => {
                        self.phase = Phase::Starting { app, since_ms: now_ms };
                        Some(Action::Start(app))
                    }
                    Some(false) => {
                        self.phase = Phase::Suppressed;
                        None
                    }
                    None if !input.ask => {
                        // The mode changed to "auto" while the prompt was open.
                        self.phase = Phase::Starting { app, since_ms: now_ms };
                        Some(Action::Start(app))
                    }
                    None => None,
                }
            }
            Phase::Starting { app, since_ms } => {
                if input.recording && input.owned {
                    self.phase = Phase::Active { app, owned: true };
                } else if input.recording && !input.owned {
                    self.phase = Phase::Active { app, owned: false };
                } else if now_ms.saturating_sub(since_ms) >= START_HOLD.as_millis() as u64 {
                    self.phase = Phase::Arming { app, since_ms: now_ms };
                }
                None
            }
            Phase::Active { app, owned } => {
                if owned && !input.recording {
                    self.phase = Phase::Suppressed;
                    return None;
                }
                if !owned && input.signal.is_none() {
                    self.phase = Phase::Idle;
                    return None;
                }
                if owned && input.signal.is_none() {
                    self.phase = Phase::Clearing { app, since_ms: now_ms };
                }
                None
            }
            Phase::Clearing { app, since_ms } => {
                if !input.recording {
                    self.phase = Phase::Suppressed;
                    return None;
                }
                if input.signal.is_some() {
                    self.phase = Phase::Active { app, owned: true };
                    return None;
                }
                if now_ms.saturating_sub(since_ms) >= STOP_HOLD.as_millis() as u64 {
                    self.phase = Phase::Stopping { since_ms: now_ms };
                    return Some(Action::Stop);
                }
                None
            }
            Phase::Stopping { .. } => {
                if !input.recording {
                    self.phase = Phase::Idle;
                }
                None
            }
            Phase::Suppressed => {
                if input.signal.is_none() && !input.recording {
                    self.phase = Phase::Idle;
                }
                None
            }
        }
    }
}

fn elapsed_ms(origin: Instant, now: Instant) -> u64 {
    now.saturating_duration_since(origin).as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_at(
        origin: Instant,
        seconds: u64,
        signal: Option<MeetingApp>,
        recording: bool,
        owned: bool,
    ) -> StepInput {
        StepInput {
            now: origin + Duration::from_secs(seconds),
            origin,
            signal,
            recording,
            owned,
            enabled: true,
            user_hold: false,
            ask: false,
            answer: None,
        }
    }

    fn ask_at(origin: Instant, seconds: u64, signal: Option<MeetingApp>, answer: Option<bool>) -> StepInput {
        StepInput {
            ask: true,
            answer,
            ..input_at(origin, seconds, signal, false, false)
        }
    }

    fn prompting(origin: Instant) -> DetectorMachine {
        let mut machine = DetectorMachine::default();
        assert_eq!(machine.step(&ask_at(origin, 0, Some(MeetingApp::Teams), None)), None);
        assert_eq!(machine.step(&ask_at(origin, 11, Some(MeetingApp::Teams), None)), None);
        assert_eq!(
            machine.step(&ask_at(origin, 12, Some(MeetingApp::Teams), None)),
            Some(Action::Prompt(MeetingApp::Teams))
        );
        assert_eq!(machine.phase(), &Phase::Prompting { app: MeetingApp::Teams });
        assert_eq!(machine.phase_name(), "prompting");
        machine
    }

    #[test]
    fn ask_mode_prompts_once_and_waits_for_the_answer() {
        let origin = Instant::now();
        let mut machine = prompting(origin);
        // No answer yet: nothing starts and the prompt is not repeated.
        for seconds in [14, 40, 300] {
            assert_eq!(machine.step(&ask_at(origin, seconds, Some(MeetingApp::Teams), None)), None);
            assert_eq!(machine.phase(), &Phase::Prompting { app: MeetingApp::Teams });
        }
    }

    #[test]
    fn an_accepted_prompt_starts_a_recording_that_stops_like_an_automatic_one() {
        let origin = Instant::now();
        let mut machine = prompting(origin);
        assert_eq!(
            machine.step(&ask_at(origin, 15, Some(MeetingApp::Teams), Some(true))),
            Some(Action::Start(MeetingApp::Teams))
        );
        assert!(matches!(machine.phase(), Phase::Starting { .. }));
        let mut owned = ask_at(origin, 16, Some(MeetingApp::Teams), None);
        owned.recording = true;
        owned.owned = true;
        machine.step(&owned);
        assert_eq!(machine.phase(), &Phase::Active { app: MeetingApp::Teams, owned: true });

        let mut gone = ask_at(origin, 17, None, None);
        gone.recording = true;
        gone.owned = true;
        machine.step(&gone);
        let mut later = ask_at(origin, 37, None, None);
        later.recording = true;
        later.owned = true;
        assert_eq!(machine.step(&later), Some(Action::Stop));
    }

    #[test]
    fn a_refused_prompt_does_not_ask_again_during_the_same_call() {
        let origin = Instant::now();
        let mut machine = prompting(origin);
        assert_eq!(machine.step(&ask_at(origin, 15, Some(MeetingApp::Teams), Some(false))), None);
        assert_eq!(machine.phase(), &Phase::Suppressed);
        for seconds in [20, 60, 600] {
            assert_eq!(machine.step(&ask_at(origin, seconds, Some(MeetingApp::Teams), None)), None);
            assert_eq!(machine.phase(), &Phase::Suppressed);
        }
        // The call ends, then a new call asks again after the hold.
        machine.step(&ask_at(origin, 601, None, None));
        assert_eq!(machine.phase(), &Phase::Idle);
        machine.step(&ask_at(origin, 610, Some(MeetingApp::Teams), None));
        assert_eq!(
            machine.step(&ask_at(origin, 622, Some(MeetingApp::Teams), None)),
            Some(Action::Prompt(MeetingApp::Teams))
        );
    }

    #[test]
    fn the_call_ending_or_a_manual_recording_closes_the_prompt() {
        let origin = Instant::now();
        let mut machine = prompting(origin);
        assert_eq!(machine.step(&ask_at(origin, 14, None, None)), None);
        assert_eq!(machine.phase(), &Phase::Idle);
        // A late answer for the closed prompt starts nothing.
        assert_eq!(machine.step(&ask_at(origin, 15, None, Some(true))), None);
        assert_eq!(machine.phase(), &Phase::Idle);

        let mut machine = prompting(origin);
        let mut manual = ask_at(origin, 14, Some(MeetingApp::Teams), None);
        manual.recording = true;
        assert_eq!(machine.step(&manual), None);
        assert_eq!(machine.phase(), &Phase::Active { app: MeetingApp::Teams, owned: false });
    }

    #[test]
    fn switching_the_mode_resolves_an_open_prompt() {
        let origin = Instant::now();
        // ask -> auto: the recording starts without an answer.
        let mut machine = prompting(origin);
        assert_eq!(
            machine.step(&input_at(origin, 14, Some(MeetingApp::Teams), false, false)),
            Some(Action::Start(MeetingApp::Teams))
        );
        // ask -> off: back to idle, nothing starts.
        let mut machine = prompting(origin);
        let mut off = ask_at(origin, 14, Some(MeetingApp::Teams), None);
        off.enabled = false;
        assert_eq!(machine.step(&off), None);
        assert_eq!(machine.phase(), &Phase::Idle);
        // "auto" never prompts.
        let mut machine = DetectorMachine::default();
        machine.step(&input_at(origin, 0, Some(MeetingApp::Zoom), false, false));
        assert_eq!(
            machine.step(&input_at(origin, 12, Some(MeetingApp::Zoom), false, false)),
            Some(Action::Start(MeetingApp::Zoom))
        );
    }

    #[test]
    fn a_short_signal_does_not_start_and_a_held_signal_does() {
        let origin = Instant::now();
        let mut machine = DetectorMachine::default();
        assert_eq!(
            machine.step(&input_at(origin, 0, Some(MeetingApp::Teams), false, false)),
            None
        );
        assert_eq!(
            machine.step(&input_at(origin, 11, Some(MeetingApp::Teams), false, false)),
            None
        );
        assert_eq!(
            machine.step(&input_at(origin, 12, Some(MeetingApp::Teams), false, false)),
            Some(Action::Start(MeetingApp::Teams))
        );
        assert!(matches!(machine.phase(), Phase::Starting { .. }));
    }

    #[test]
    fn a_blip_returns_to_idle_and_a_manual_recording_is_not_stopped() {
        let origin = Instant::now();
        let mut machine = DetectorMachine::default();
        machine.step(&input_at(origin, 0, Some(MeetingApp::Zoom), false, false));
        assert_eq!(machine.step(&input_at(origin, 4, None, false, false)), None);
        assert_eq!(machine.phase(), &Phase::Idle);

        machine.step(&input_at(origin, 5, Some(MeetingApp::Meet), true, false));
        assert_eq!(machine.phase(), &Phase::Active { app: MeetingApp::Meet, owned: false });
        assert_eq!(machine.step(&input_at(origin, 40, None, true, false)), None);
        assert_eq!(machine.phase(), &Phase::Idle);
    }

    #[test]
    fn owned_recording_stops_only_after_the_signal_stays_gone() {
        let origin = Instant::now();
        let mut machine = DetectorMachine::default();
        machine.step(&input_at(origin, 0, Some(MeetingApp::Teams), false, false));
        assert_eq!(
            machine.step(&input_at(origin, 12, Some(MeetingApp::Teams), false, false)),
            Some(Action::Start(MeetingApp::Teams))
        );
        machine.step(&input_at(origin, 13, Some(MeetingApp::Teams), true, true));
        assert_eq!(machine.phase(), &Phase::Active { app: MeetingApp::Teams, owned: true });
        machine.step(&input_at(origin, 14, None, true, true));
        assert_eq!(machine.step(&input_at(origin, 33, None, true, true)), None);
        assert_eq!(machine.step(&input_at(origin, 34, None, true, true)), Some(Action::Stop));
    }

    #[test]
    fn the_signal_returning_cancels_the_stop_and_a_user_stop_does_not_restart() {
        let origin = Instant::now();
        let mut machine = DetectorMachine::default();
        machine.phase = Phase::Clearing {
            app: MeetingApp::Teams,
            since_ms: 0,
        };
        machine.step(&input_at(origin, 5, Some(MeetingApp::Teams), true, true));
        assert_eq!(machine.phase(), &Phase::Active { app: MeetingApp::Teams, owned: true });

        machine.step(&input_at(origin, 6, Some(MeetingApp::Teams), false, false));
        assert_eq!(machine.phase(), &Phase::Suppressed);
        assert_eq!(
            machine.step(&input_at(origin, 30, Some(MeetingApp::Teams), false, false)),
            None
        );
        assert_eq!(machine.phase(), &Phase::Suppressed);
        machine.step(&input_at(origin, 31, None, false, false));
        assert_eq!(machine.phase(), &Phase::Idle);
    }
}
