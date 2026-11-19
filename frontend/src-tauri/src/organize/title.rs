//! Title rendering from the user's template.

use chrono::NaiveDateTime;

use super::text::{clamp_chars, squash};

pub const DEFAULT_TEMPLATE: &str = "{cliente} · {assunto} · {data}";
const FALLBACK_TEMPLATE: &str = "{assunto} · {data}";
const MAX_TITLE_CHARS: usize = 140;
const PLACEHOLDERS: [&str; 5] = ["cliente", "projeto", "assunto", "data", "hora"];

#[derive(Debug, Clone, Copy)]
pub struct TitleParts<'a> {
    pub client: Option<&'a str>,
    pub project: Option<&'a str>,
    pub subject: &'a str,
    /// Takes the client's place when there is no client.
    pub category: Option<&'a str>,
    /// Meeting start, already in local time.
    pub when: NaiveDateTime,
}

pub fn template_has_placeholder(template: &str) -> bool {
    tokenize(template).iter().any(|(_, name)| name.is_some())
}

/// Splits the template into `(literal before, placeholder)` pairs. The last
/// pair has no placeholder and carries the trailing literal.
fn tokenize(template: &str) -> Vec<(String, Option<&'static str>)> {
    let mut pairs = Vec::new();
    let mut literal = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let known = after.find('}').and_then(|close| {
            let name = after[..close].trim().to_lowercase();
            PLACEHOLDERS
                .iter()
                .find(|candidate| **candidate == name)
                .map(|candidate| (*candidate, close))
        });
        match known {
            Some((name, close)) => {
                literal.push_str(&rest[..open]);
                pairs.push((std::mem::take(&mut literal), Some(name)));
                rest = &after[close + 1..];
            }
            None => {
                literal.push_str(&rest[..open + 1]);
                rest = after;
            }
        }
    }
    literal.push_str(rest);
    pairs.push((literal, None));
    pairs
}

fn value_for(name: &str, parts: &TitleParts<'_>) -> String {
    let clean = |value: Option<&str>| value.map(squash).unwrap_or_default();
    match name {
        "cliente" => {
            let client = clean(parts.client);
            if client.is_empty() {
                clean(parts.category)
            } else {
                client
            }
        }
        "projeto" => clean(parts.project),
        "assunto" => squash(parts.subject),
        "data" => parts.when.format("%d/%m/%Y").to_string(),
        "hora" => parts.when.format("%H:%M").to_string(),
        _ => String::new(),
    }
}

fn render_once(template: &str, parts: &TitleParts<'_>) -> String {
    let pairs = tokenize(template);
    let placeholder_count = pairs.iter().filter(|(_, name)| name.is_some()).count();
    let mut out = String::new();
    let mut kept_any = false;
    let mut last_kept = false;
    let mut index = 0;
    for (literal, name) in &pairs {
        let Some(name) = name else {
            // Trailing literal: only when the last placeholder was kept, or
            // when the template has no placeholder at all.
            if last_kept || placeholder_count == 0 {
                out.push_str(literal);
            }
            break;
        };
        let value = value_for(name, parts);
        if value.is_empty() {
            last_kept = false;
        } else {
            // The literal before a kept value is a prefix (first placeholder)
            // or the separator from the previous kept value. A separator that
            // belonged to a dropped value is dropped with it.
            if kept_any || index == 0 {
                out.push_str(literal);
            }
            out.push_str(&value);
            kept_any = true;
            last_kept = true;
        }
        index += 1;
    }
    squash(&out)
}

/// Renders the title. Empty placeholders disappear together with the
/// separator next to them, so there is never a dangling " · ".
pub fn render_title(template: &str, parts: &TitleParts<'_>) -> String {
    let template = if template.trim().is_empty() { DEFAULT_TEMPLATE } else { template };
    let mut title = render_once(template, parts);
    if title.is_empty() {
        title = render_once(FALLBACK_TEMPLATE, parts);
    }
    clamp_chars(&title, MAX_TITLE_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn when() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(14, 5, 0)
            .unwrap()
    }

    fn parts<'a>(client: Option<&'a str>, project: Option<&'a str>, subject: &'a str, category: Option<&'a str>) -> TitleParts<'a> {
        TitleParts { client, project, subject, category, when: when() }
    }

    #[test]
    fn default_template_with_client() {
        let title = render_title(
            DEFAULT_TEMPLATE,
            &parts(Some("Boreal"), None, "Revisão de custos OCI", Some("Cliente")),
        );
        assert_eq!(title, "Boreal · Revisão de custos OCI · 18/11/2026");
    }

    #[test]
    fn category_takes_the_place_of_a_missing_client() {
        let title = render_title(
            DEFAULT_TEMPLATE,
            &parts(None, None, "Planejamento do trimestre", Some("Interna")),
        );
        assert_eq!(title, "Interna · Planejamento do trimestre · 18/11/2026");
    }

    #[test]
    fn empty_placeholders_take_their_separator_with_them() {
        let none = parts(None, None, "Ajuste de backup", None);
        assert_eq!(render_title(DEFAULT_TEMPLATE, &none), "Ajuste de backup · 18/11/2026");
        assert_eq!(
            render_title("{cliente} · {projeto} · {assunto} · {data}", &parts(Some("Lume"), None, "Carga do DW", None)),
            "Lume · Carga do DW · 18/11/2026"
        );
        assert_eq!(
            render_title("{data} - {assunto} - {projeto}", &none),
            "18/11/2026 - Ajuste de backup"
        );
        assert_eq!(
            render_title("[{cliente}] {assunto}", &none),
            "Ajuste de backup"
        );
        assert_eq!(
            render_title("[{cliente}] {assunto}", &parts(Some("Cedro"), None, "ETL", None)),
            "[Cedro] ETL"
        );
    }

    #[test]
    fn hour_project_and_unknown_placeholders() {
        let title = render_title(
            "{data} {hora} | {projeto} | {assunto} {x}",
            &parts(Some("Boreal"), Some("FinOps"), "Fechamento mensal", None),
        );
        assert_eq!(title, "18/11/2026 14:05 | FinOps | Fechamento mensal {x}");
    }

    #[test]
    fn blank_or_placeholder_free_templates() {
        let some = parts(Some("Boreal"), None, "Custos", None);
        assert_eq!(render_title("   ", &some), "Boreal · Custos · 18/11/2026");
        assert_eq!(render_title("Reunião", &some), "Reunião");
        assert!(!template_has_placeholder("Reunião {nada}"));
        assert!(template_has_placeholder("x {Assunto}"));
        // Only empty placeholders: falls back to subject and date.
        assert_eq!(
            render_title("{projeto}", &parts(None, None, "Custos", None)),
            "Custos · 18/11/2026"
        );
    }

    #[test]
    fn long_titles_are_clamped() {
        let subject = "palavra ".repeat(60);
        let title = render_title(DEFAULT_TEMPLATE, &parts(None, None, &subject, None));
        assert!(title.chars().count() <= 140);
    }
}
