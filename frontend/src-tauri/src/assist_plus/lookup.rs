use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;

const USER_AGENT: &str = "MeetilyLive/0.5 (local desktop meeting notes)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupHit {
    pub term: String,
    pub extract: String,
    pub url: String,
    pub source: String,
}

pub struct LookupCache {
    entries: HashMap<String, (Instant, Option<LookupHit>)>,
}

impl LookupCache {
    pub fn empty() -> Self {
        Self { entries: HashMap::new() }
    }
}

impl LookupCache {
    pub fn get(&self, term: &str) -> Option<Option<LookupHit>> {
        let key = term.trim().to_lowercase();
        let (at, hit) = self.entries.get(&key)?;
        if at.elapsed() > Duration::from_secs(60 * 60) {
            return None;
        }
        Some(hit.clone())
    }

    pub fn insert(&mut self, term: &str, hit: Option<LookupHit>) {
        self.entries
            .insert(term.trim().to_lowercase(), (Instant::now(), hit));
    }
}

pub fn wikipedia_summary_url(language: &str, title: &str) -> String {
    let lang = if language == "pt" { "pt" } else { "en" };
    format!(
        "https://{lang}.wikipedia.org/api/rest_v1/page/summary/{title}",
        title = encode_title(title)
    )
}

pub fn duckduckgo_url(term: &str) -> String {
    format!(
        "https://api.duckduckgo.com/?q={q}&format=json&no_html=1&skip_disambig=1&t=meetilylive",
        q = encode_query(term)
    )
}

pub fn parse_wikipedia(term: &str, body: &str) -> Option<LookupHit> {
    let page: WikiSummary = serde_json::from_str(body).ok()?;
    let extract = page.extract.unwrap_or_default();
    if extract.trim().is_empty() || page.type_field.as_deref() == Some("disambiguation") {
        return None;
    }
    let url = page
        .content_urls
        .as_ref()
        .and_then(|urls| urls.desktop.as_ref())
        .map(|desktop| desktop.page.clone())
        .unwrap_or_default();
    if url.is_empty() {
        return None;
    }
    Some(LookupHit {
        term: term.to_string(),
        extract: extract.trim().to_string(),
        url,
        source: "Wikipédia".to_string(),
    })
}

pub fn parse_duckduckgo(term: &str, body: &str) -> Option<LookupHit> {
    let answer: DuckAnswer = serde_json::from_str(body).ok()?;
    let extract = answer.abstract_text.unwrap_or_default();
    let url = answer.abstract_url.unwrap_or_default();
    if extract.trim().is_empty() || url.trim().is_empty() {
        return None;
    }
    Some(LookupHit {
        term: term.to_string(),
        extract: extract.trim().to_string(),
        url,
        source: "DuckDuckGo".to_string(),
    })
}

pub fn contexto_line(hit: &LookupHit) -> String {
    let mut words = String::new();
    for (index, word) in hit.extract.split_whitespace().enumerate() {
        if index == 22 {
            break;
        }
        if index > 0 {
            words.push(' ');
        }
        words.push_str(word);
    }
    format!("{words} {}", hit.url)
}

pub async fn fetch_term(client: &reqwest::Client, term: &str) -> Option<LookupHit> {
    for language in ["pt", "en"] {
        if let Some(hit) = fetch_json(client, &wikipedia_summary_url(language, term)).await {
            if let Some(parsed) = parse_wikipedia(term, &hit) {
                return Some(parsed);
            }
        }
    }
    let body = fetch_json(client, &duckduckgo_url(term)).await?;
    parse_duckduckgo(term, &body)
}

async fn fetch_json(client: &reqwest::Client, url: &str) -> Option<String> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().await.ok()
}

fn encode_title(title: &str) -> String {
    encode_query(&title.trim().replace(' ', "_"))
}

fn encode_query(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Deserialize)]
struct WikiSummary {
    #[serde(rename = "type")]
    type_field: Option<String>,
    extract: Option<String>,
    content_urls: Option<WikiUrls>,
}

#[derive(Deserialize)]
struct WikiUrls {
    desktop: Option<WikiDesktop>,
}

#[derive(Deserialize)]
struct WikiDesktop {
    page: String,
}

#[derive(Deserialize)]
struct DuckAnswer {
    #[serde(rename = "AbstractText")]
    abstract_text: Option<String>,
    #[serde(rename = "AbstractURL")]
    abstract_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_wikipedia_summary_shape() {
        let body = r#"{"type":"standard","title":"LGPD","extract":"Lei brasileira de proteção de dados.","content_urls":{"desktop":{"page":"https://pt.wikipedia.org/wiki/LGPD"}}}"#;
        let hit = parse_wikipedia("LGPD", body).unwrap();
        assert_eq!(hit.url, "https://pt.wikipedia.org/wiki/LGPD");
        assert!(hit.extract.contains("proteção"));
    }

    #[test]
    fn parses_the_duckduckgo_instant_answer_shape() {
        let body = r#"{"AbstractText":"Uma lei de dados.","AbstractURL":"https://pt.wikipedia.org/wiki/LGPD","Heading":"LGPD"}"#;
        let hit = parse_duckduckgo("LGPD", body).unwrap();
        assert_eq!(hit.source, "DuckDuckGo");
        assert!(contexto_line(&hit).contains("https://pt.wikipedia.org/wiki/LGPD"));
    }

    #[test]
    fn empty_abstracts_are_not_hits() {
        assert!(parse_duckduckgo("x", r#"{"AbstractText":"","AbstractURL":""}"#).is_none());
        assert!(parse_wikipedia("x", r#"{"type":"disambiguation","extract":"veja","content_urls":{"desktop":{"page":"https://example"}}}"#).is_none());
    }

    #[test]
    fn cache_returns_a_recent_miss() {
        let mut cache = LookupCache::empty();
        cache.insert("lgpd", None);
        assert_eq!(cache.get("LGPD"), Some(None));
        assert_eq!(cache.get("outro"), None);
    }
}
