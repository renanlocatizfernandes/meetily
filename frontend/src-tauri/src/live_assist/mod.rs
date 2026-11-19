//! Live Assist: trigger-based suggestions from the live transcript.
//!
//! Audio never enters this module. It only sees transcript text that the
//! existing transcription worker already produced on this machine.

pub mod commands;
mod debounce;
mod parse;
mod prompt;
mod service;
pub(crate) mod settings_store;
mod similarity;
mod target;
mod trigger;
mod window;

pub(crate) use service::load_hint as load_summary_hint;
pub(crate) use target::resolve_target;

pub use commands::{get_live_assist_settings, open_live_assist_window, save_live_assist_settings};
pub use service::{on_recording_started, on_recording_stopped};
