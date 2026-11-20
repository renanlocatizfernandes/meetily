//! Splits a meeting into searchable passages.

pub const SOURCE_TRANSCRIPT: &str = "transcript";
pub const SOURCE_SUMMARY: &str = "summary";

/// Target size of one passage, in characters.
const PASSAGE_TARGET: usize = 700;
/// A passage is closed once it reaches this size even in the middle of a block.
const PASSAGE_HARD_MAX: usize = 1100;

#[derive(Debug, Clone, PartialEq)]
pub struct NewPassage {
    pub source: &'static str,
    pub start_sec: Option<f64>,
    pub text: String,
}

/// One saved transcript segment.
#[derive(Debug, Clone)]
pub struct Segment {
    pub start_sec: Option<f64>,
    pub speaker: Option<String>,
    pub text: String,
}

fn clamp_to_boundary(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

/// Groups consecutive segments into passages of roughly `PASSAGE_TARGET`
/// characters. The speaker label is written when it changes inside a passage
/// and at the start of each one.
pub fn transcript_passages(segments: &[Segment]) -> Vec<NewPassage> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut start: Option<f64> = None;
    let mut started = false;
    let mut last_speaker: Option<String> = None;

    let flush = |text: &mut String, start: &mut Option<f64>, started: &mut bool, out: &mut Vec<NewPassage>| {
        let body = text.trim();
        if !body.is_empty() {
            out.push(NewPassage {
                source: SOURCE_TRANSCRIPT,
                start_sec: *start,
                text: body.to_string(),
            });
        }
        text.clear();
        *start = None;
        *started = false;
    };

    for segment in segments {
        let line = segment.text.trim();
        if line.is_empty() {
            continue;
        }
        let line = clamp_to_boundary(line, PASSAGE_HARD_MAX);
        if started && text.chars().count() + line.chars().count() > PASSAGE_TARGET {
            flush(&mut text, &mut start, &mut started, &mut out);
        }
        if !started {
            started = true;
            start = segment.start_sec.filter(|s| s.is_finite() && *s >= 0.0);
            // A new passage always names the speaker again.
            last_speaker = None;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        if let Some(speaker) = segment.speaker.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if last_speaker.as_deref() != Some(speaker) {
                text.push_str(speaker);
                text.push_str(": ");
                last_speaker = Some(speaker.to_string());
            }
        }
        text.push_str(&line);
    }
    flush(&mut text, &mut start, &mut started, &mut out);
    out
}

/// Splits the summary markdown on blank lines and merges small blocks, keeping
/// each heading with the block that follows it.
pub fn summary_passages(markdown: &str) -> Vec<NewPassage> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in markdown.lines() {
        if line.trim().is_empty() {
            if !current.trim().is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            current.clear();
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line.trim_end());
        }
    }
    if !current.trim().is_empty() {
        blocks.push(current);
    }

    let mut out = Vec::new();
    let mut acc = String::new();
    let mut push = |acc: &mut String, out: &mut Vec<NewPassage>| {
        let body = acc.trim();
        if !body.is_empty() {
            out.push(NewPassage {
                source: SOURCE_SUMMARY,
                start_sec: None,
                text: body.to_string(),
            });
        }
        acc.clear();
    };
    for block in blocks {
        let heading_only = block.lines().all(|l| l.trim_start().starts_with('#'));
        let size = block.chars().count();
        if !acc.is_empty() && !heading_only && acc.chars().count() + size > PASSAGE_TARGET {
            push(&mut acc, &mut out);
        }
        if !acc.is_empty() {
            acc.push_str("\n\n");
        }
        acc.push_str(&clamp_to_boundary(&block, PASSAGE_HARD_MAX * 2));
        // Closes only when the block is not a bare heading, so the heading
        // stays with the text that follows.
        if !heading_only && acc.chars().count() >= PASSAGE_TARGET {
            push(&mut acc, &mut out);
        }
    }
    push(&mut acc, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, speaker: Option<&str>, text: &str) -> Segment {
        Segment {
            start_sec: Some(start),
            speaker: speaker.map(str::to_string),
            text: text.to_string(),
        }
    }

    #[test]
    fn groups_segments_and_keeps_first_start() {
        let long = "palavra ".repeat(40); // 320 chars
        let segments = vec![
            seg(5.0, Some("Ana"), &long),
            seg(25.0, Some("Ana"), &long),
            seg(45.0, Some("Beto"), &long),
        ];
        let passages = transcript_passages(&segments);
        assert_eq!(passages.len(), 2);
        assert_eq!(passages[0].start_sec, Some(5.0));
        assert_eq!(passages[1].start_sec, Some(45.0));
        // Same speaker twice: label written once. New passage repeats it.
        assert_eq!(passages[0].text.matches("Ana:").count(), 1);
        assert!(passages[1].text.starts_with("Beto: "));
    }

    #[test]
    fn skips_blank_segments_and_handles_missing_times() {
        let passages = transcript_passages(&[
            Segment { start_sec: None, speaker: None, text: "  ".into() },
            Segment { start_sec: None, speaker: None, text: "olá".into() },
        ]);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].start_sec, None);
        assert!(transcript_passages(&[]).is_empty());
    }

    #[test]
    fn summary_blocks_keep_headings_with_text() {
        let md = "# Decisões\n\n- Fechar contrato com a Acme\n- Prazo em 15 dias\n\n## Próximos passos\n\n- Ana envia proposta";
        let passages = summary_passages(md);
        assert_eq!(passages.len(), 1);
        assert!(passages[0].text.contains("Decisões"));
        assert!(passages[0].text.contains("Ana envia proposta"));
        assert_eq!(passages[0].source, SOURCE_SUMMARY);
        assert!(summary_passages("  \n\n ").is_empty());
    }

    #[test]
    fn huge_blocks_are_clamped() {
        let md = "x".repeat(10_000);
        let passages = summary_passages(&md);
        assert_eq!(passages.len(), 1);
        assert!(passages[0].text.chars().count() <= PASSAGE_HARD_MAX * 2);
    }
}
