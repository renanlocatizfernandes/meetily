//! Debounced meeting detector. A signal must hold before a recording starts,
//! and it must stay gone before that recording stops. A recording the user
//! started by hand is never stopped here.

use std::time::{Duration, Instant};

use super::detect::MeetingApp;

pub const START_HOLD: Duration = Duration::from_secs(12);
pub const STOP_HOLD: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Arming { app: MeetingApp, since_ms: u64 },
    Starting { app: MeetingApp, since_ms: u64 },
    Active { app: MeetingApp, owned: bool },
    Clearing { app: MeetingApp, since_ms: u64 },
    Stopping { since_ms: u64 },
    Suppressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start(MeetingApp),
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
                        self.phase = Phase::Starting { app, since_ms: now_ms };
                        Some(Action::Start(app))
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
        }
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
