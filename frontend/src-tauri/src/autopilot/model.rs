use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_TEMPLATE: &str = "standard_meeting";

pub const STEP_SUMMARY: &str = "summary";
pub const STEP_TASKS: &str = "tasks";
pub const STEP_TITLE_TAGS: &str = "title_tags";
pub const STEP_EMAIL: &str = "email";

pub const STATE_DONE: &str = "done";
pub const STATE_FAILED: &str = "failed";
pub const STATE_SKIPPED: &str = "skipped";
pub const STATE_OFF: &str = "off";

pub const STATUS_RUNNING: &str = "running";
pub const STATUS_DONE: &str = "done";
pub const STATUS_PARTIAL: &str = "partial";
pub const STATUS_SKIPPED: &str = "skipped";
pub const STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutopilotSettings {
    pub enabled: bool,
    pub step_summary: bool,
    pub step_tasks: bool,
    pub step_title_tags: bool,
    pub step_email: bool,
    pub template_id: String,
}

impl Default for AutopilotSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            step_summary: true,
            step_tasks: true,
            step_title_tags: true,
            step_email: true,
            template_id: DEFAULT_TEMPLATE.to_string(),
        }
    }
}

impl AutopilotSettings {
    pub fn normalized(mut self) -> Self {
        let id = self.template_id.trim();
        let valid = !id.is_empty()
            && id.len() <= 64
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        self.template_id = if valid { id.to_string() } else { DEFAULT_TEMPLATE.to_string() };
        self
    }

    pub fn wanted(&self) -> Wanted {
        Wanted {
            tasks: self.step_tasks,
            title_tags: self.step_title_tags,
            email: self.step_email,
        }
    }

    pub fn any_step(&self) -> bool {
        self.step_summary || self.wanted().any()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutopilotResult {
    pub meeting_id: String,
    pub status: String,
    pub note: Option<String>,
    pub steps: BTreeMap<String, String>,
    pub tasks: Vec<TaskItem>,
    pub decisions: Vec<String>,
    pub pending: Vec<String>,
    pub suggested_title: Option<String>,
    pub title_applied: bool,
    pub suggested_tags: Vec<String>,
    pub tags_applied: bool,
    pub email_subject: Option<String>,
    pub email_body: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub leaves_machine: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// What the settings screen needs to warn about cost and privacy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutopilotStatus {
    pub provider: String,
    pub model: String,
    /// Configured provider bills per use (anything that is not local).
    pub paid: bool,
    pub has_credentials: bool,
    /// "configured" | "local_fallback" | "none"
    pub will_use: String,
    pub will_use_provider: Option<String>,
    pub will_use_model: Option<String>,
    pub message: Option<String>,
}

/// Fields the extraction call is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wanted {
    pub tasks: bool,
    pub title_tags: bool,
    pub email: bool,
}

impl Wanted {
    pub fn any(&self) -> bool {
        self.tasks || self.title_tags || self.email
    }
}

/// Parsed and sanitized model answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extraction {
    pub tasks: Vec<TaskItem>,
    pub decisions: Vec<String>,
    pub pending: Vec<String>,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub email_subject: Option<String>,
    pub email_body: Option<String>,
}
