//! Live Assist: trigger-based suggestions from the live transcript.
//!
//! Audio never enters this module. It only sees transcript text that the
//! existing transcription worker already produced on this machine.

pub mod benchmark;
pub mod commands;
mod context;
mod context_store;
mod contradiction;
mod debounce;
mod parse;
mod partial;
mod prompt;
mod recap;
mod service;
pub(crate) mod settings_store;
mod smart_settings;
pub mod shortcut;
mod similarity;
mod stream;
mod target;
mod trigger;
mod usage;
mod window;

pub(crate) use service::load_hint as load_summary_hint;
pub(crate) use target::resolve_target;
pub(crate) use target::DEFAULT_MODEL as DEFAULT_LIVE_MODEL;

pub use commands::{
    get_live_assist_smart_settings, get_live_assist_usage, live_assist_recap, live_assist_suggest_answer,
    save_live_assist_smart_settings,
    get_live_assist_context, get_live_assist_context_settings, get_live_assist_settings, open_live_assist_window,
    save_live_assist_context_settings, save_live_assist_settings,
};
pub use service::{on_recording_started, on_recording_stopped};
