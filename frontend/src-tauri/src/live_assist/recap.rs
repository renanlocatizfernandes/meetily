//! "O que eu perdi?" and "Sugerir resposta": prompts and the longer talk
//! history they read. The 75 s window of the live assistant is too short for a
//! recap, so the session keeps its own bounded history.

use std::collections::VecDeque;

use serde::Serialize;

use super::usage::tail_chars;

/// History kept in memory for the recording in progress.
const HISTORY_MAX_SECONDS: f64 = 3600.0;
const HISTORY_MAX_CHARS: usize = 60_000;
pub const RECAP_MIN_MINUTES: u32 = 1;
pub const RECAP_MAX_MINUTES: u32 = 30;
/// What one recap call may read. Economy mode reads less.
pub const RECAP_MAX_INPUT_CHARS: usize = 14_000;
pub const RECAP_ECONOMY_INPUT_CHARS: usize = 6_000;
/// Lines of the talk that ground a suggested answer.
const ANSWER_CONTEXT_LINES: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryLine {
    pub at: f64,
    pub source: String,
    pub text: String,
}

#[derive(Debug, Default, Clone)]
pub struct TalkHistory {
    lines: VecDeque<HistoryLine>,
    chars: usize,
}

impl TalkHistory {
    pub fn push(&mut self, line: HistoryLine) {
        self.chars += line.text.chars().count();
        let latest = line.at;
        self.lines.push_back(line);
        while let Some(front) = self.lines.front() {
            if latest - front.at > HISTORY_MAX_SECONDS || self.chars > HISTORY_MAX_CHARS {
                let old = self.lines.pop_front().expect("front exists");
                self.chars = self.chars.saturating_sub(old.text.chars().count());
            } else {
                break;
            }
        }
    }

    pub fn latest_at(&self) -> Option<f64> {
        self.lines.back().map(|line| line.at)
    }

    /// Lines of the last `minutes`, rendered `[Quem] texto`, oldest first.
    pub fn render_last_minutes(&self, minutes: u32) -> (String, usize) {
        let Some(latest) = self.latest_at() else {
            return (String::new(), 0);
        };
        let from = latest - f64::from(minutes) * 60.0;
        self.render(self.lines.iter().filter(|line| line.at >= from))
    }

    pub fn render_last_lines(&self, count: usize) -> String {
        let skip = self.lines.len().saturating_sub(count);
        self.render(self.lines.iter().skip(skip)).0
    }

    fn render<'a>(&self, lines: impl Iterator<Item = &'a HistoryLine>) -> (String, usize) {
        let mut out = String::new();
        let mut count = 0;
        for line in lines {
            out.push_str(&format!("[{}] {}\n", speaker_name(&line.source), line.text.trim()));
            count += 1;
        }
        (out, count)
    }
}

/// The owner is "Você" in a recap, whatever the label the transcript used.
pub fn speaker_name(source: &str) -> &str {
    match source {
        "mic" => "Você",
        "system" => "Participantes",
        other => other,
    }
}

pub fn clamp_minutes(minutes: u32) -> u32 {
    minutes.clamp(RECAP_MIN_MINUTES, RECAP_MAX_MINUTES)
}

pub fn recap_system_prompt() -> &'static str {
    "Você ajuda quem entrou atrasado ou se distraiu numa reunião. Resuma o trecho recebido no idioma pedido, em no máximo 6 tópicos curtos, um por linha, começando com \"- \". Priorize decisões, pendências, números, datas e perguntas feitas ao dono (rótulo Você). Cite quem falou quando importar. Não invente nada que não esteja no trecho, não use markdown além dos tópicos e não escreva introdução nem conclusão. Se o trecho não tiver conteúdo útil, responda apenas: - Nada relevante nesse período."
}

pub fn answer_system_prompt() -> &'static str {
    "Você escreve a resposta que o dono da reunião (rótulo Você) pode dar agora a uma pergunta feita por outra pessoa. Responda em no máximo 3 frases curtas, na voz do dono, no idioma pedido, usando só o contexto recebido e o trecho recente. Se o contexto não bastar, diga o que falta confirmar em vez de inventar. Sem markdown, sem introdução."
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecapPrompt {
    pub system: &'static str,
    pub user: String,
    /// Lines of talk the prompt covers.
    pub lines: usize,
    pub input_chars: usize,
}

/// `None` when nothing was said in the period.
pub fn build_recap_prompt(history: &TalkHistory, minutes: u32, language: &str, economy: bool) -> Option<RecapPrompt> {
    let minutes = clamp_minutes(minutes);
    let (talk, lines) = history.render_last_minutes(minutes);
    if lines == 0 {
        return None;
    }
    let limit = if economy { RECAP_ECONOMY_INPUT_CHARS } else { RECAP_MAX_INPUT_CHARS };
    let talk = tail_chars(&talk, limit);
    let language = if language.trim().is_empty() { "pt-BR" } else { language.trim() };
    let user = format!("Idioma do resumo: {language}\nPeríodo: últimos {minutes} minutos da reunião.\n\nTrecho:\n{talk}");
    let input_chars = user.chars().count() + recap_system_prompt().chars().count();
    Some(RecapPrompt { system: recap_system_prompt(), user, lines, input_chars })
}

pub fn build_answer_prompt(history: &TalkHistory, question: &str, language: &str, context: &str) -> RecapPrompt {
    let language = if language.trim().is_empty() { "pt-BR" } else { language.trim() };
    let talk = history.render_last_lines(ANSWER_CONTEXT_LINES);
    let context = if context.trim().is_empty() { "(nenhum)".to_string() } else { context.trim().to_string() };
    let user = format!(
        "Idioma da resposta: {language}\n\nContexto do dono:\n{context}\n\nTrecho recente:\n{talk}\nPergunta a responder:\n{}",
        question.trim()
    );
    let input_chars = user.chars().count() + answer_system_prompt().chars().count();
    RecapPrompt { system: answer_system_prompt(), user, lines: ANSWER_CONTEXT_LINES, input_chars }
}

/// Bullet lines of a recap answer, cleaned. A model that ignored the format
/// still gets its text split into lines.
pub fn parse_bullets(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|line| line.trim().trim_start_matches(['-', '*', '•']).trim().to_string())
        .filter(|line| !line.is_empty())
        .take(8)
        .collect()
}

/// Reply of `live_assist_recap`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecapResult {
    pub minutes: u32,
    pub bullets: Vec<String>,
    /// Estimated tokens this one call used.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub paid: bool,
}

/// Reply of `live_assist_suggest_answer`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnswerResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub paid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(at: f64, source: &str, text: &str) -> HistoryLine {
        HistoryLine { at, source: source.into(), text: text.into() }
    }

    fn history() -> TalkHistory {
        let mut history = TalkHistory::default();
        history.push(line(0.0, "system", "Abertura sem importância"));
        history.push(line(200.0, "mic", "Eu posso enviar a proposta"));
        history.push(line(290.0, "Ana", "Qual o prazo da proposta?"));
        history
    }

    #[test]
    fn recap_covers_only_the_asked_minutes_and_names_the_owner() {
        let prompt = build_recap_prompt(&history(), 2, "pt-BR", false).unwrap();
        assert_eq!(prompt.lines, 2);
        assert!(prompt.user.contains("[Você] Eu posso enviar a proposta"));
        assert!(prompt.user.contains("[Ana] Qual o prazo"));
        assert!(!prompt.user.contains("Abertura"));
        let all = build_recap_prompt(&history(), 10, "pt-BR", false).unwrap();
        assert_eq!(all.lines, 3);
        assert!(all.user.contains("[Participantes] Abertura"));
    }

    #[test]
    fn empty_history_gives_no_prompt_and_minutes_are_clamped() {
        assert!(build_recap_prompt(&TalkHistory::default(), 5, "pt-BR", false).is_none());
        assert_eq!(clamp_minutes(0), RECAP_MIN_MINUTES);
        assert_eq!(clamp_minutes(999), RECAP_MAX_MINUTES);
    }

    #[test]
    fn economy_reads_less_of_a_long_talk() {
        let mut history = TalkHistory::default();
        for index in 0..400 {
            history.push(line(f64::from(index), "system", &"palavra ".repeat(10)));
        }
        let normal = build_recap_prompt(&history, 30, "pt-BR", false).unwrap();
        let economy = build_recap_prompt(&history, 30, "pt-BR", true).unwrap();
        assert!(economy.input_chars < normal.input_chars);
        assert!(economy.user.chars().count() <= RECAP_ECONOMY_INPUT_CHARS + 200);
    }

    #[test]
    fn history_drops_old_lines_by_age_and_size() {
        let mut history = TalkHistory::default();
        history.push(line(0.0, "mic", "velha"));
        history.push(line(4000.0, "mic", "nova"));
        assert_eq!(history.render_last_minutes(120).1, 1);
        let mut big = TalkHistory::default();
        for index in 0..100 {
            big.push(line(f64::from(index), "mic", &"x".repeat(1000)));
        }
        assert!(big.render_last_lines(1000).chars().count() <= HISTORY_MAX_CHARS + 2000);
    }

    #[test]
    fn answer_prompt_carries_question_context_and_recent_talk() {
        let prompt = build_answer_prompt(&history(), " Qual o prazo? ", "pt-BR", "Empresa: Aurora");
        assert!(prompt.user.contains("Empresa: Aurora"));
        assert!(prompt.user.trim_end().ends_with("Qual o prazo?"));
        assert!(prompt.user.contains("[Ana]"));
        assert!(build_answer_prompt(&history(), "q", "", "").user.contains("(nenhum)"));
    }

    #[test]
    fn prompts_stay_short_and_without_dash_punctuation() {
        for text in [recap_system_prompt(), answer_system_prompt()] {
            assert!(text.chars().count() <= 800, "{}", text.chars().count());
            assert!(!text.contains('—') && !text.contains('–'));
        }
    }

    #[test]
    fn bullets_are_cleaned_and_capped() {
        let bullets = parse_bullets("- Decidiu migrar\n* Prazo sexta\n\n• Ana pediu a proposta\ntexto solto");
        assert_eq!(bullets, vec!["Decidiu migrar", "Prazo sexta", "Ana pediu a proposta", "texto solto"]);
        assert_eq!(parse_bullets(&"- a\n".repeat(20)).len(), 8);
        assert!(parse_bullets("  \n ").is_empty());
    }
}
