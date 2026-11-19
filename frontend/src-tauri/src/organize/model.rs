//! Types exchanged with the frontend (camelCase JSON).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::classify::{clean_name_list, MAX_KNOWN_CLIENTS};
use super::title::{template_has_placeholder, DEFAULT_TEMPLATE};

pub const SOURCE_DEFAULT: &str = "default";
pub const SOURCE_AUTO: &str = "auto";
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_NONE: &str = "none";

pub const DEFAULT_CATEGORIES: [&str; 7] = [
    "Cliente",
    "Comercial",
    "Interna",
    "Suporte",
    "Planejamento",
    "Pessoal",
    "Outros",
];

const MAX_CATEGORIES: usize = 20;
const MAX_CATEGORY_CHARS: usize = 30;
const MAX_CLIENT_CHARS: usize = 60;
const MAX_TEMPLATE_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingOrg {
    pub id: String,
    pub title: String,
    /// RFC 3339.
    pub created_at: String,
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    /// "default" | "auto" | "manual"
    pub title_source: String,
    /// "none" | "auto" | "manual"
    pub classification_source: String,
    pub pinned: bool,
    pub duration_seconds: Option<f64>,
    /// Subject found by the classifier; the part of an automatic title that
    /// is neither client nor date.
    pub subject: Option<String>,
    /// RFC 3339 of the last automatic organization, if any.
    pub organized_at: Option<String>,
    #[serde(skip)]
    pub created: DateTime<Utc>,
}

impl MeetingOrg {
    pub fn title_is_manual(&self) -> bool {
        self.title_source == SOURCE_MANUAL
    }

    pub fn classification_is_manual(&self) -> bool {
        self.classification_source == SOURCE_MANUAL
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationSettings {
    pub auto_organize: bool,
    pub title_template: String,
    pub known_clients: Vec<String>,
    pub categories: Vec<String>,
}

impl Default for OrganizationSettings {
    fn default() -> Self {
        Self {
            auto_organize: true,
            title_template: DEFAULT_TEMPLATE.to_string(),
            known_clients: Vec::new(),
            categories: DEFAULT_CATEGORIES.iter().map(|value| value.to_string()).collect(),
        }
    }
}

impl OrganizationSettings {
    pub fn normalized(mut self) -> Self {
        let template: String = self.title_template.trim().chars().take(MAX_TEMPLATE_CHARS).collect();
        self.title_template = if template.is_empty() || !template_has_placeholder(&template) {
            DEFAULT_TEMPLATE.to_string()
        } else {
            template
        };
        self.known_clients = clean_name_list(&self.known_clients, MAX_KNOWN_CLIENTS, MAX_CLIENT_CHARS);
        self.categories = clean_name_list(&self.categories, MAX_CATEGORIES, MAX_CATEGORY_CHARS);
        if self.categories.is_empty() {
            self.categories = Self::default().categories;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacetCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct MeetingFacets {
    pub clients: Vec<FacetCount>,
    pub projects: Vec<FacetCount>,
    pub categories: Vec<FacetCount>,
    pub tags: Vec<FacetCount>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeAllResult {
    pub processed: u32,
    pub failed: u32,
    /// Left as they were: too little text, or title and classification are manual.
    pub skipped: u32,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeProgress {
    pub meeting_id: String,
    /// 1-based position of the meeting being processed.
    pub index: u32,
    pub total: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_normalized() {
        let settings = OrganizationSettings {
            auto_organize: false,
            title_template: "  sem campos  ".to_string(),
            known_clients: vec![" Boreal ".into(), "boreal".into(), "".into(), "Cedro".into()],
            categories: vec!["  ".into()],
        }
        .normalized();
        assert!(!settings.auto_organize);
        assert_eq!(settings.title_template, DEFAULT_TEMPLATE);
        assert_eq!(settings.known_clients, vec!["Boreal", "Cedro"]);
        assert_eq!(settings.categories.len(), 7);

        let kept = OrganizationSettings {
            title_template: " {data} {assunto} ".to_string(),
            categories: vec!["Cliente".into(), "cliente".into(), "Jurídico".into()],
            ..OrganizationSettings::default()
        }
        .normalized();
        assert_eq!(kept.title_template, "{data} {assunto}");
        assert_eq!(kept.categories, vec!["Cliente", "Jurídico"]);
    }

    #[test]
    fn settings_json_is_camel_case() {
        let json = serde_json::to_value(OrganizationSettings::default()).unwrap();
        assert_eq!(json["autoOrganize"], true);
        assert_eq!(json["titleTemplate"], DEFAULT_TEMPLATE);
        assert!(json["knownClients"].as_array().unwrap().is_empty());
        assert_eq!(json["categories"][6], "Outros");
    }
}
