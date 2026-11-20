//! Meeting organization: objective titles ("cliente · assunto · data"),
//! automatic classification (client, project, category, tags) and manual
//! reclassification.
//!
//! Only transcript text and summaries already saved on this machine are read.
//! The classification request goes to the provider configured for summaries
//! (local Ollama by default), never to a new destination.

mod classify;
pub mod commands;
mod model;
mod service;
mod store;
mod text;
mod title;

pub use model::{MeetingFacets, MeetingOrg, OrganizationSettings, OrganizeAllResult};
pub(crate) use service::complete_text;
pub use service::{schedule_auto_organize, AutoTrigger};

// Read-only helpers reused by the live assist context (same matching rules).
pub(crate) use classify::find_known_client_in_text;
pub(crate) use store::{
    list_meetings as list_organized_meetings, load_settings as load_organization_settings,
    summary_markdown_from_result,
};
pub(crate) use text::fold;
