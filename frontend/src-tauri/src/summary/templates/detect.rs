//! Local meeting-type detection from a transcript.
//!
//! A keyword score per meeting type (pt-BR and a few English terms). Nothing
//! leaves the machine and nothing here logs transcript content. The result is
//! only a suggestion: the user can always pick another template.

use serde::Serialize;

/// One meeting type: the template it maps to and the weighted phrases that
/// point to it. Phrases are lowercase and accent-free; a trailing `*` matches
/// any word starting with the stem.
struct Profile {
    template_id: &'static str,
    phrases: &'static [(&'static str, u32)],
}

const PROFILES: &[Profile] = &[
    Profile {
        template_id: "daily_standup",
        phrases: &[
            ("daily", 4), ("standup", 4), ("stand-up", 4), ("stand up", 3),
            ("ontem eu", 3), ("ontem", 1), ("hoje eu", 2), ("hoje vou", 2),
            ("o que fiz", 3), ("vou fazer", 2), ("bloqueio*", 3),
            ("impedimento*", 3), ("blocker*", 3), ("sprint", 2), ("yesterday", 1),
            ("sem bloqueios", 3), ("tudo certo por aqui", 2),
        ],
    },
    Profile {
        template_id: "one_on_one",
        phrases: &[
            ("um_a_um", 5), ("one on one", 5), ("um a um", 4), ("feedback*", 3),
            ("carreira", 3), ("como voce esta", 3), ("como vc esta", 3),
            ("te sentindo", 3), ("se sentindo", 2), ("crescimento", 2),
            ("promocao", 3), ("desenvolvimento", 1), ("motivad*", 2),
            ("meu gestor", 2), ("minha lider", 2), ("meu lider", 2),
            ("expectativa*", 1), ("cansad*", 1), ("sobrecarreg*", 2),
        ],
    },
    Profile {
        template_id: "reuniao_vendas",
        phrases: &[
            ("proposta*", 3), ("orcamento*", 3), ("preco*", 3), ("desconto*", 3),
            ("cliente*", 1), ("contrato*", 2), ("fechar", 2), ("fechamos", 2),
            ("concorrente*", 3), ("investimento", 2), ("demonstracao", 3),
            ("decisor*", 4), ("pipeline", 3), ("pricing", 3), ("licenca*", 2),
            ("mensalidade", 2), ("plano*", 1), ("prospect*", 3), ("lead*", 2),
            ("comercial", 2), ("negociacao", 3), ("objecao", 3), ("objecoes", 3),
        ],
    },
    Profile {
        template_id: "entrevista_selecao",
        phrases: &[
            ("vaga*", 4), ("curriculo*", 4), ("candidat*", 4), ("entrevista*", 3),
            ("pretensao salarial", 5), ("conte sobre voce", 4), ("fale sobre voce", 4),
            ("ultimo emprego", 4), ("pontos fortes", 3), ("recrutamento", 4),
            ("processo seletivo", 5), ("rh", 2), ("experiencia anterior", 3),
            ("onde voce se ve", 4), ("por que voce saiu", 4), ("contratacao", 2),
            ("recrutador*", 4),
        ],
    },
    Profile {
        template_id: "aula_treinamento",
        phrases: &[
            ("aula*", 4), ("aluno*", 4), ("alunas", 4), ("exercicio*", 3),
            ("professor*", 3), ("treinamento*", 3), ("slide*", 2), ("modulo*", 2),
            ("turma", 3), ("licao", 3), ("prova", 2), ("conteudo de hoje", 4),
            ("vamos ver", 1), ("tarefa de casa", 4), ("material de estudo", 3),
            ("palestra*", 3), ("curso*", 2), ("conceito*", 2), ("explicar", 1),
        ],
    },
    Profile {
        template_id: "consulta_atendimento",
        phrases: &[
            ("sintoma*", 4), ("paciente*", 4), ("consulta*", 3), ("remedio*", 3),
            ("medicamento*", 3), ("exame*", 3), ("queixa*", 3), ("receita", 2),
            ("tratamento*", 3), ("diagnostico*", 3), ("sessao", 2), ("terapia", 3),
            ("terapeuta*", 3), ("medic*", 1), ("doutor*", 2), ("doutora", 2),
            ("dor de", 2), ("sinto dor", 3), ("pressao", 1), ("retorno em", 2),
            ("anamnese", 4), ("atendimento", 1),
        ],
    },
    Profile {
        template_id: "retrospective",
        phrases: &[
            ("retrospectiva*", 5), ("retro", 3), ("o que deu certo", 5),
            ("o que melhorar", 5), ("o que nao deu certo", 5), ("went well", 4),
            ("licoes aprendidas", 4), ("pontos a melhorar", 4), ("kudos", 3),
        ],
    },
    Profile {
        template_id: "project_sync",
        phrases: &[
            ("status do projeto", 4), ("andamento", 2), ("marco*", 2),
            ("cronograma", 3), ("entrega*", 1), ("roadmap", 3), ("escopo", 2),
            ("stakeholder*", 3), ("risco*", 2), ("prazo*", 1), ("projeto*", 1),
        ],
    },
];

const FALLBACK_TEMPLATE: &str = "standard_meeting";
/// Below this score the transcript has too few clues to switch templates.
const MIN_SCORE: u32 = 6;
/// Each phrase counts at most this many times, so one repeated word does not decide.
const MAX_HITS_PER_PHRASE: u32 = 3;
/// A type must beat the runner-up by this share of its own score.
const MIN_MARGIN_RATIO: f32 = 0.25;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Candidate {
    pub template_id: String,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Detection {
    /// Suggested template, always one `is_available` accepted.
    pub template_id: String,
    /// 0.0 to 1.0. Zero means "no clear type": the fallback was used.
    pub confidence: f32,
    /// True when a type stood out; false when `template_id` is the fallback.
    pub detected: bool,
    pub candidates: Vec<Candidate>,
}

/// Lowercase, drop accents and punctuation, and join words with one space,
/// padded at both ends so whole-word matching is a plain substring search.
fn normalize(text: &str) -> String {
    let lowered = text.to_lowercase().replace("1:1", " um_a_um ");
    let mut out = String::with_capacity(lowered.len() + 2);
    out.push(' ');
    let mut last_space = true;
    for ch in lowered.chars() {
        let mapped = match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() || c == '_' || c == '-' => c,
            _ => ' ',
        };
        if mapped == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(mapped);
            last_space = false;
        }
    }
    if !last_space {
        out.push(' ');
    }
    out
}

fn count_phrase(haystack: &str, phrase: &str) -> u32 {
    let needle = match phrase.strip_suffix('*') {
        Some(stem) => format!(" {stem}"),
        None => format!(" {phrase} "),
    };
    haystack.matches(needle.as_str()).count() as u32
}

fn score(haystack: &str, profile: &Profile) -> u32 {
    profile
        .phrases
        .iter()
        .map(|(phrase, weight)| count_phrase(haystack, phrase).min(MAX_HITS_PER_PHRASE) * weight)
        .sum()
}

/// Picks the template that best fits `transcript`. Only ids accepted by
/// `is_available` can be returned; the fallback is returned (with
/// `detected == false`) when nothing stands out.
pub fn detect(transcript: &str, is_available: impl Fn(&str) -> bool) -> Detection {
    let haystack = normalize(transcript);
    let mut candidates: Vec<Candidate> = PROFILES
        .iter()
        .filter(|profile| is_available(profile.template_id))
        .map(|profile| Candidate {
            template_id: profile.template_id.to_string(),
            score: score(&haystack, profile),
        })
        .filter(|candidate| candidate.score > 0)
        .collect();
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.template_id.cmp(&b.template_id)));

    if let Some(top) = candidates.first() {
        let second = candidates.get(1).map(|c| c.score).unwrap_or(0);
        let margin = (top.score - second) as f32 / top.score as f32;
        if top.score >= MIN_SCORE && margin >= MIN_MARGIN_RATIO {
            let strength = (top.score as f32 / 20.0).min(1.0);
            return Detection {
                template_id: top.template_id.clone(),
                confidence: (strength * 0.5 + margin * 0.5).clamp(0.0, 1.0),
                detected: true,
                candidates,
            };
        }
    }
    Detection {
        template_id: FALLBACK_TEMPLATE.to_string(),
        confidence: 0.0,
        detected: false,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(_: &str) -> bool {
        true
    }

    #[test]
    fn detects_daily() {
        let text = "Bom dia, daily de hoje. Ontem eu terminei a tela de login. Hoje eu vou \
                    revisar o PR. Sem bloqueios. Ontem eu fiquei no deploy, tenho um bloqueio no ambiente.";
        let result = detect(text, all);
        assert!(result.detected);
        assert_eq!(result.template_id, "daily_standup");
    }

    #[test]
    fn detects_one_on_one() {
        let text = "Oi, nosso 1:1 de hoje. Como você está se sentindo no time? Queria te dar um \
                    feedback sobre sua carreira e falar de promoção. Feedback também para mim.";
        assert_eq!(detect(text, all).template_id, "one_on_one");
    }

    #[test]
    fn detects_sales() {
        let text = "Nossa proposta tem desconto no preço da licença. O decisor é o diretor, o \
                    concorrente já mandou orçamento. Vamos fechar o contrato esta semana?";
        assert_eq!(detect(text, all).template_id, "reuniao_vendas");
    }

    #[test]
    fn detects_interview() {
        let text = "Obrigada por vir. Conte sobre você e seu último emprego. Vi seu currículo para \
                    a vaga. Qual sua pretensão salarial? O candidato segue no processo seletivo.";
        assert_eq!(detect(text, all).template_id, "entrevista_selecao");
    }

    #[test]
    fn detects_class() {
        let text = "Pessoal, na aula de hoje, alunos, vamos fazer um exercício do módulo 3. \
                    Dúvidas da turma? Tarefa de casa: slides 10 a 20.";
        assert_eq!(detect(text, all).template_id, "aula_treinamento");
    }

    #[test]
    fn detects_consultation() {
        let text = "Qual é a sua queixa principal? Sinto dor de cabeça, esse sintoma começou ontem. \
                    Vou pedir um exame e trocar o medicamento. Retorno em duas semanas, paciente estável.";
        assert_eq!(detect(text, all).template_id, "consulta_atendimento");
    }

    #[test]
    fn weak_clues_fall_back_to_standard() {
        let result = detect("Vamos conversar sobre o almoço de sexta e a decoração da sala.", all);
        assert!(!result.detected);
        assert_eq!(result.template_id, "standard_meeting");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn repeated_word_alone_does_not_decide() {
        let result = detect("cliente cliente cliente cliente cliente cliente", all);
        assert!(!result.detected);
    }

    #[test]
    fn unavailable_templates_are_never_suggested() {
        let text = "Nossa proposta tem desconto no preço. O decisor e o concorrente. Fechar contrato.";
        let result = detect(text, |id| id != "reuniao_vendas");
        assert_ne!(result.template_id, "reuniao_vendas");
    }

    #[test]
    fn accents_and_case_are_ignored() {
        assert!(normalize("ORÇAMENTO Preço!").contains(" orcamento preco "));
    }

    #[test]
    fn empty_text_falls_back() {
        assert!(!detect("", all).detected);
    }
}
