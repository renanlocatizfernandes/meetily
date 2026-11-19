#[derive(Debug, Clone)]
pub struct Turn {
    pub speaker: String,
    pub seconds: f64,
    pub text: String,
    pub at: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TalkShare {
    pub speaker: String,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scoreboard {
    pub talk: Vec<TalkShare>,
    pub unanswered: Vec<String>,
    pub coverage: f64,
    pub covered: Vec<String>,
    pub missing: Vec<String>,
}

pub fn score(turns: &[Turn], agenda: &str) -> Scoreboard {
    let mut talk: Vec<TalkShare> = Vec::new();
    for turn in turns {
        let name = if turn.speaker.trim().is_empty() {
            "Sem nome".to_string()
        } else {
            turn.speaker.clone()
        };
        if let Some(slot) = talk.iter_mut().find(|item| item.speaker == name) {
            slot.seconds += turn.seconds.max(0.0);
        } else {
            talk.push(TalkShare {
                speaker: name,
                seconds: turn.seconds.max(0.0),
            });
        }
    }
    talk.sort_by(|left, right| right.seconds.partial_cmp(&left.seconds).unwrap_or(std::cmp::Ordering::Equal));

    let mut unanswered = Vec::new();
    for (index, turn) in turns.iter().enumerate() {
        if !is_question(&turn.text) || is_owner(&turn.speaker) {
            continue;
        }
        let answered = turns[index + 1..].iter().any(|next| {
            next.at >= turn.at && next.at - turn.at <= 20.0 && is_owner(&next.speaker) && !next.text.trim().is_empty()
        });
        if !answered {
            unanswered.push(turn.text.trim().chars().take(160).collect());
        }
    }

    let agenda_terms = agenda_terms(agenda);
    let blob = turns
        .iter()
        .map(|turn| turn.text.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    let mut covered = Vec::new();
    let mut missing = Vec::new();
    for term in &agenda_terms {
        if blob.contains(term) {
            covered.push(term.clone());
        } else {
            missing.push(term.clone());
        }
    }
    let coverage = if agenda_terms.is_empty() {
        0.0
    } else {
        covered.len() as f64 / agenda_terms.len() as f64
    };
    Scoreboard {
        talk,
        unanswered,
        coverage,
        covered,
        missing,
    }
}

fn is_owner(speaker: &str) -> bool {
    let speaker = speaker.trim();
    speaker.eq_ignore_ascii_case("você")
        || speaker.eq_ignore_ascii_case("voce")
        || speaker.eq_ignore_ascii_case("mic")
}

fn is_question(text: &str) -> bool {
    let lower = text.to_lowercase();
    text.contains('?')
        || ["quem", "qual", "quando", "onde", "por que", "porque", "como", "quanto"]
            .iter()
            .any(|word| lower.contains(word))
}

fn agenda_terms(agenda: &str) -> Vec<String> {
    agenda
        .split(|c: char| !c.is_alphanumeric())
        .map(|word| word.trim().to_lowercase())
        .filter(|word| word.chars().count() >= 4)
        .filter(|word| {
            ![
                "reunião", "reuniao", "pauta", "sobre", "cliente", "para", "com", "essa", "este",
            ]
            .contains(&word.as_str())
        })
        .fold(Vec::new(), |mut acc, word| {
            if !acc.contains(&word) {
                acc.push(word);
            }
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_talk_time_and_flags_an_unanswered_question() {
        let turns = vec![
            Turn { speaker: "Você".into(), seconds: 10.0, text: "Vamos ver o preço.".into(), at: 0.0 },
            Turn { speaker: "Ana".into(), seconds: 30.0, text: "Qual é o desconto?".into(), at: 12.0 },
            Turn { speaker: "Ana".into(), seconds: 5.0, text: "E o prazo?".into(), at: 40.0 },
            Turn { speaker: "Você".into(), seconds: 8.0, text: "O prazo é sexta.".into(), at: 46.0 },
        ];
        let board = score(&turns, "preço e implantação");
        assert_eq!(board.talk[0].speaker, "Ana");
        assert!((board.talk[0].seconds - 35.0).abs() < 0.01);
        assert_eq!(board.unanswered, vec!["Qual é o desconto?".to_string()]);
        assert!(board.covered.iter().any(|term| term == "preço"));
        assert!(board.missing.iter().any(|term| term == "implantação"));
        assert!((board.coverage - 0.5).abs() < 0.01);
    }
}
