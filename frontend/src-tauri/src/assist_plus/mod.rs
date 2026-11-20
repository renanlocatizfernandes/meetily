//! Extra meeting tools. Lookup uses the public Wikipedia REST summary and the
//! DuckDuckGo Instant Answer API. Only the search term leaves the machine, and
//! only when "Pesquisa na internet" is on.

mod actions;
mod calendar;
pub mod commands;
mod commitments;
mod email;
mod entities;
mod lookup;
pub mod privacy;
mod scoreboard;
mod signals;
mod search;
mod toggles;
mod translate;

pub use commands::{
    draft_followup, export_action_ics, get_feature_toggles, meeting_scoreboard, prepare_from_calendar,
    prepare_from_ics, recall_meetings, save_feature_toggles, search_meetings,
};
pub use toggles::{load, load_effective, FeatureToggles};

pub use commitments::detect_commitments;
pub use signals::{detect_signals, Signal};
pub use entities::unknown_terms;
pub use lookup::{contexto_line, LookupHit};
pub use translate::should_translate;

pub async fn lookup_terms(pool: &sqlx::SqlitePool, terms: &[String]) -> Vec<LookupHit> {
    let mut hits = Vec::new();
    for term in terms.iter().take(2) {
        if let Some(hit) = commands::cached_or_fetch(pool, term).await {
            hits.push(hit);
        }
    }
    hits
}
