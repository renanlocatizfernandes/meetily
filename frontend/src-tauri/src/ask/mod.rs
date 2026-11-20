//! "Pergunte às suas reuniões": questions answered from every saved meeting,
//! with citations that point to the meeting and the passage they came from.
//!
//! Order of work: (1) a local index of passages (SQLite FTS5 when available),
//! (2) ranking in Rust, (3) optionally the provider already configured for
//! summaries words the answer from the passages found. Without a usable model
//! the passages alone are returned.
//!
//! Only text already saved on this machine is read. A cloud provider receives
//! the selected passages (and nothing else) and only when the user asks for an
//! answer; the response says so.

mod chunk;
pub mod commands;
mod model;
mod rank;
mod service;
mod store;
mod text;

pub use model::{AskResponse, AskStatus, Citation};
