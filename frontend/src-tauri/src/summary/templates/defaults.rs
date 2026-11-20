/// Embedded default templates using compile-time inclusion
///
/// These templates are bundled into the binary and serve as fallbacks
/// when custom templates are not available.

/// Daily standup template for engineering/product teams
pub const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");

/// Standard meeting notes template
pub const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");

/// Pt-BR template: one_on_one
pub const ONE_ON_ONE: &str = include_str!("../../../templates/one_on_one.json");

/// Pt-BR template: entrevista_selecao
pub const ENTREVISTA_SELECAO: &str = include_str!("../../../templates/entrevista_selecao.json");

/// Pt-BR template: aula_treinamento
pub const AULA_TREINAMENTO: &str = include_str!("../../../templates/aula_treinamento.json");

/// Pt-BR template: consulta_atendimento
pub const CONSULTA_ATENDIMENTO: &str = include_str!("../../../templates/consulta_atendimento.json");

/// Pt-BR template: reuniao_vendas
pub const REUNIAO_VENDAS: &str = include_str!("../../../templates/reuniao_vendas.json");

/// Registry of all built-in templates
///
/// Maps template identifiers to their embedded JSON content
pub fn get_builtin_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("daily_standup", DAILY_STANDUP),
        ("standard_meeting", STANDARD_MEETING),
        ("one_on_one", ONE_ON_ONE),
        ("entrevista_selecao", ENTREVISTA_SELECAO),
        ("aula_treinamento", AULA_TREINAMENTO),
        ("consulta_atendimento", CONSULTA_ATENDIMENTO),
        ("reuniao_vendas", REUNIAO_VENDAS),
    ]
}

/// Get a built-in template by identifier
///
/// # Arguments
/// * `id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// The template JSON content if found, None otherwise
pub fn get_builtin_template(id: &str) -> Option<&'static str> {
    match id {
        "daily_standup" => Some(DAILY_STANDUP),
        "standard_meeting" => Some(STANDARD_MEETING),
        "one_on_one" => Some(ONE_ON_ONE),
        "entrevista_selecao" => Some(ENTREVISTA_SELECAO),
        "aula_treinamento" => Some(AULA_TREINAMENTO),
        "consulta_atendimento" => Some(CONSULTA_ATENDIMENTO),
        "reuniao_vendas" => Some(REUNIAO_VENDAS),
        _ => None,
    }
}

/// List all built-in template identifiers
pub fn list_builtin_template_ids() -> Vec<&'static str> {
    vec![
        "daily_standup",
        "standard_meeting",
        "one_on_one",
        "entrevista_selecao",
        "aula_treinamento",
        "consulta_atendimento",
        "reuniao_vendas",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_templates_valid_json() {
        for (id, content) in get_builtin_templates() {
            let result = serde_json::from_str::<serde_json::Value>(content);
            assert!(
                result.is_ok(),
                "Built-in template '{}' contains invalid JSON: {:?}",
                id,
                result.err()
            );
        }
    }

    #[test]
    fn test_get_builtin_template() {
        assert!(get_builtin_template("daily_standup").is_some());
        assert!(get_builtin_template("standard_meeting").is_some());
        assert!(get_builtin_template("nonexistent").is_none());
    }
}
