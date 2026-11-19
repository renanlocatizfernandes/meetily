//! What the live assist prompt is anchored on, besides the transcript:
//! the meeting (client, project, who is speaking), the owner's profile and
//! glossary, and a few notes from earlier meetings of the same client.
//!
//! Everything here is pure text selection with fixed character budgets, so a
//! small local model (llama3.2 3B) is not flooded. Nothing in this module
//! talks to a model, the network or the database, and nothing is logged.

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::organize::{find_known_client_in_text, fold};

/// The free-text meeting context plus the automatic context share this many
/// characters. The typed text always wins; the automatic part shrinks.
/// The numbers are small on purpose: the target is a 3B model on a fanless
/// 8 GB machine, where every extra prompt token costs latency.
const PROMPT_CONTEXT_CHARS: usize = 2000;
/// Ceiling of the automatic block (about 250 tokens in Portuguese).
pub const AUTO_CONTEXT_MAX_CHARS: usize = 900;
pub const AUTO_CONTEXT_MIN_CHARS: usize = 400;

/// Titles of the four blocks, with a 60 character client name.
const HEADER_OVERHEAD_CHARS: usize = 180;

const MAX_ABOUT_CHARS: usize = 300;
const MAX_COMPANY_CHARS: usize = 300;
const MAX_OFFERINGS_CHARS: usize = 600;
pub const MAX_GLOSSARY_CHARS: usize = 4000;
const MAX_GLOSSARY_ENTRIES_IN_PROMPT: usize = 4;
const MAX_MEMORY_NOTES_IN_PROMPT: usize = 3;
const MAX_NOTES_PER_MEETING_IN_PROMPT: usize = 2;
const MAX_NOTE_CHARS: usize = 120;
const MAX_SPEAKERS: usize = 8;
const MAX_TAGS: usize = 4;

// ---------------------------------------------------------------------------
// Settings (profile, glossary, switch)
// ---------------------------------------------------------------------------

/// Persistent profile of the owner and the switch for earlier meetings.
/// Separate from the per-meeting free text in `LiveAssistSettings`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ContextSettings {
    pub profile_enabled: bool,
    /// Who the owner is (name, role).
    pub about: String,
    pub company: String,
    /// Products and services.
    pub offerings: String,
    /// One entry per line: `TERMO: explicação`.
    pub glossary: String,
    pub use_past_meetings: bool,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            profile_enabled: true,
            about: String::new(),
            company: String::new(),
            offerings: String::new(),
            glossary: String::new(),
            use_past_meetings: true,
        }
    }
}

fn clamp(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect::<String>().trim_end().to_string()
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl ContextSettings {
    pub fn normalized(mut self) -> Self {
        self.about = clamp(&self.about, MAX_ABOUT_CHARS);
        self.company = clamp(&self.company, MAX_COMPANY_CHARS);
        self.offerings = clamp(&self.offerings, MAX_OFFERINGS_CHARS);
        self.glossary = clamp(&self.glossary, MAX_GLOSSARY_CHARS);
        self
    }

    pub fn profile_filled(&self) -> bool {
        !(self.about.is_empty() && self.company.is_empty() && self.offerings.is_empty())
    }

    pub fn glossary_entries(&self) -> Vec<GlossaryEntry> {
        parse_glossary(&self.glossary)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossaryEntry {
    pub term: String,
    pub definition: String,
}

/// `TERMO: explicação`, `TERMO = explicação` or `TERMO - explicação`, one per line.
pub fn parse_glossary(raw: &str) -> Vec<GlossaryEntry> {
    let mut entries: Vec<GlossaryEntry> = Vec::new();
    for line in raw.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '•']).trim();
        let split = line
            .split_once(':')
            .or_else(|| line.split_once('='))
            .or_else(|| line.split_once(" - "));
        let Some((term, definition)) = split else { continue };
        let term = one_line(term);
        let definition = one_line(definition);
        if term.is_empty() || definition.is_empty() || term.chars().count() > 60 {
            continue;
        }
        if entries.iter().any(|entry| fold(&entry.term) == fold(&term)) {
            continue;
        }
        entries.push(GlossaryEntry { term, definition });
    }
    entries
}

// ---------------------------------------------------------------------------
// Earlier meetings
// ---------------------------------------------------------------------------

/// A saved meeting of the same client, as read from SQLite.
#[derive(Debug, Clone)]
pub struct PastMeeting {
    pub id: String,
    pub title: String,
    pub created: DateTime<Utc>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub subject: Option<String>,
    pub summary_markdown: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Pendencia,
    Decisao,
    Resumo,
    Topico,
    Assunto,
}

impl NoteKind {
    fn label(self) -> &'static str {
        match self {
            NoteKind::Pendencia => "Pendência",
            NoteKind::Decisao => "Decisão",
            NoteKind::Resumo => "Resumo",
            NoteKind::Topico => "Ponto",
            NoteKind::Assunto => "Assunto",
        }
    }

    fn weight(self) -> usize {
        match self {
            NoteKind::Pendencia | NoteKind::Decisao => 3,
            NoteKind::Resumo => 2,
            NoteKind::Topico | NoteKind::Assunto => 1,
        }
    }

    fn per_meeting_cap(self) -> usize {
        match self {
            NoteKind::Pendencia | NoteKind::Decisao => 3,
            NoteKind::Resumo | NoteKind::Topico => 2,
            NoteKind::Assunto => 1,
        }
    }
}

/// One fact from an earlier meeting, short enough for the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryNote {
    pub meeting_id: String,
    pub meeting_title: String,
    /// "12/11", or "12/11/2025" for another year.
    pub date_label: String,
    pub kind: NoteKind,
    pub text: String,
    /// 0 is the most recent meeting of the client.
    pub recency: usize,
}

impl MemoryNote {
    pub fn origin_label(&self) -> String {
        format!("reunião de {}", self.date_label)
    }

    fn line(&self) -> String {
        format!("- [{}] {}: {}", self.origin_label(), self.kind.label(), self.text)
    }
}

pub fn date_label(created: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let local = created.with_timezone(&Local);
    if local.year() == now.with_timezone(&Local).year() {
        local.format("%d/%m").to_string()
    } else {
        local.format("%d/%m/%Y").to_string()
    }
}

fn heading_kind(heading: &str) -> NoteKind {
    let folded = fold(heading);
    // Word prefixes, so "avaliação" is not read as "ação".
    let has = |needles: &[&str]| {
        needles.iter().any(|needle| {
            if needle.contains(' ') {
                folded.contains(needle)
            } else {
                folded.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(needle))
            }
        })
    };
    if has(&["action", "acao", "acoes", "pendenc", "proxim", "next step", "tarefa", "follow", "encaminhamento", "compromisso"]) {
        NoteKind::Pendencia
    } else if has(&["decis"]) {
        NoteKind::Decisao
    } else if has(&["resumo", "summary", "overview", "visao geral", "sintese"]) {
        NoteKind::Resumo
    } else {
        NoteKind::Topico
    }
}

fn strip_markdown(text: &str) -> String {
    let mut out = text.replace("**", "").replace("__", "").replace('`', "");
    for marker in ["[ ]", "[x]", "[X]"] {
        out = out.replace(marker, "");
    }
    one_line(&out)
}

fn is_placeholder(text: &str) -> bool {
    let folded = fold(text);
    let folded = folded.trim_matches(|c: char| !c.is_alphanumeric());
    text.split_whitespace().count() < 3
        || ["none", "n/a", "nenhum", "nenhuma", "nao houve", "sem pendencias", "not discussed", "nao discutido"]
            .iter()
            .any(|placeholder| folded.starts_with(placeholder))
}

fn clamp_note(text: &str) -> String {
    if text.chars().count() <= MAX_NOTE_CHARS {
        return text.to_string();
    }
    let cut: String = text.chars().take(MAX_NOTE_CHARS).collect();
    let cut = match cut.rfind(' ') {
        Some(space) if space > MAX_NOTE_CHARS / 2 => &cut[..space],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

fn bullet_text(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("+ ")) {
        return Some(rest);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 2 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(rest);
        }
    }
    None
}

/// Reads decisions, pending items and summary lines out of a saved summary.
/// Headings choose the kind; bullets, table rows and summary paragraphs are
/// the items.
pub fn notes_from_summary(markdown: &str) -> Vec<(NoteKind, String)> {
    let mut notes: Vec<(NoteKind, String)> = Vec::new();
    let mut kind = NoteKind::Topico;
    let mut table_rows = 0usize;
    for raw in markdown.lines() {
        let line = raw.trim();
        if line.is_empty() {
            table_rows = 0;
            continue;
        }
        if line.starts_with('#') {
            kind = heading_kind(line.trim_start_matches('#'));
            table_rows = 0;
            continue;
        }
        let bold_only = line.starts_with("**") && line.ends_with("**") && line.matches("**").count() == 2;
        if bold_only {
            kind = heading_kind(line);
            table_rows = 0;
            continue;
        }
        let item = if line.starts_with('|') {
            table_rows += 1;
            let cells: Vec<String> = line
                .trim_matches('|')
                .split('|')
                .map(strip_markdown)
                .filter(|cell| !cell.is_empty())
                .collect();
            let separator = cells.iter().all(|cell| cell.chars().all(|c| matches!(c, '-' | ':' | ' ')));
            // The first row of a table is its header.
            if table_rows == 1 || separator {
                continue;
            }
            cells.join(" · ")
        } else {
            table_rows = 0;
            match bullet_text(line) {
                Some(rest) => strip_markdown(rest),
                None if kind == NoteKind::Resumo => strip_markdown(line),
                None => continue,
            }
        };
        if is_placeholder(&item) {
            continue;
        }
        if notes.iter().filter(|(existing, _)| *existing == kind).count() >= kind.per_meeting_cap() {
            continue;
        }
        notes.push((kind, clamp_note(&item)));
    }
    notes
}

fn default_title(title: &str) -> bool {
    let folded = fold(title);
    ["meeting ", "reuniao ", "gravacao "]
        .iter()
        .any(|prefix| folded.strip_prefix(prefix).is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit())))
}

/// Notes of one earlier meeting. Without a summary, only its subject is kept.
pub fn notes_from_meeting(meeting: &PastMeeting, recency: usize, now: DateTime<Utc>) -> Vec<MemoryNote> {
    let label = date_label(meeting.created, now);
    let note = |kind: NoteKind, text: String| MemoryNote {
        meeting_id: meeting.id.clone(),
        meeting_title: meeting.title.clone(),
        date_label: label.clone(),
        kind,
        text,
        recency,
    };
    let mut notes: Vec<MemoryNote> = meeting
        .summary_markdown
        .as_deref()
        .map(notes_from_summary)
        .unwrap_or_default()
        .into_iter()
        .map(|(kind, text)| note(kind, text))
        .collect();
    if notes.is_empty() {
        let subject = meeting
            .subject
            .clone()
            .filter(|subject| !subject.trim().is_empty())
            .or_else(|| (!default_title(&meeting.title)).then(|| meeting.title.clone()));
        if let Some(subject) = subject {
            notes.push(note(NoteKind::Assunto, clamp_note(&one_line(&subject))));
        }
    }
    notes
}

// ---------------------------------------------------------------------------
// Relevance
// ---------------------------------------------------------------------------

const STOP_WORDS: &[&str] = &[
    "para", "com", "que", "uma", "uns", "umas", "dos", "das", "por", "pela", "pelo", "sobre", "como", "mais", "muito",
    "isso", "esse", "essa", "este", "esta", "esta", "ele", "ela", "eles", "elas", "nao", "sim", "tem", "ter", "foi",
    "ser", "sao", "vai", "vamos", "aqui", "entao", "tambem", "quando", "onde", "qual", "quais", "quem", "the", "and",
    "for", "with", "that", "this", "from", "are", "was", "voce", "nos", "seu", "sua", "meu", "minha", "reuniao",
    "ate", "sem", "nas", "nos", "num", "numa", "aos", "the",
];

/// Folded content words of a text.
pub fn content_tokens(text: &str) -> HashSet<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| {
            let length = word.chars().count();
            (length >= 3 || (length == 2 && word.chars().all(|c| c.is_ascii_digit()))) && !STOP_WORDS.contains(word)
        })
        .map(str::to_string)
        .collect()
}

fn overlap(left: &HashSet<String>, right: &HashSet<String>) -> usize {
    left.intersection(right).count()
}

fn mentions(folded_text: &str, term: &str) -> bool {
    let needle = fold(term);
    let mut from = 0;
    while let Some(found) = folded_text[from..].find(&needle) {
        let start = from + found;
        let end = start + needle.len();
        let before = folded_text[..start].chars().next_back().map_or(true, |c| !c.is_alphanumeric());
        let after = folded_text[end..].chars().next().map_or(true, |c| !c.is_alphanumeric());
        if before && after {
            return true;
        }
        from = end;
    }
    false
}

/// Picks the few notes worth the prompt: the ones that share words with what
/// is being said now, then pending items and decisions of the latest
/// meetings. Never more than `budget_chars` in total.
pub fn select_memory<'a>(notes: &'a [MemoryNote], focus: &str, budget_chars: usize) -> Vec<&'a MemoryNote> {
    let focus_tokens = content_tokens(focus);
    let mut ranked: Vec<(usize, usize, &MemoryNote)> = notes
        .iter()
        .enumerate()
        .map(|(index, note)| {
            let shared = overlap(&content_tokens(&note.text), &focus_tokens);
            let score = shared * 4 + note.kind.weight() + 3usize.saturating_sub(note.recency);
            (score, index, note)
        })
        .collect();
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then(left.2.recency.cmp(&right.2.recency)).then(left.1.cmp(&right.1)));

    let mut chosen: Vec<(usize, &MemoryNote)> = Vec::new();
    let mut used = 0usize;
    for (_, index, note) in ranked {
        if chosen.len() >= MAX_MEMORY_NOTES_IN_PROMPT {
            break;
        }
        let from_same = chosen.iter().filter(|(_, picked)| picked.meeting_id == note.meeting_id).count();
        if from_same >= MAX_NOTES_PER_MEETING_IN_PROMPT {
            continue;
        }
        let cost = note.line().chars().count() + 1;
        if used + cost > budget_chars {
            continue;
        }
        used += cost;
        chosen.push((index, note));
    }
    // Reading order: latest meeting first, then as written in the summary.
    chosen.sort_by(|left, right| left.1.recency.cmp(&right.1.recency).then(left.0.cmp(&right.0)));
    chosen.into_iter().map(|(_, note)| note).collect()
}

/// Glossary entries for the prompt: only terms that were just said. The
/// rest of the glossary would cost prompt tokens without helping.
pub fn select_glossary<'a>(entries: &'a [GlossaryEntry], focus: &str, budget_chars: usize) -> Vec<&'a GlossaryEntry> {
    let folded = fold(focus);
    let mut chosen = Vec::new();
    let mut used = 0usize;
    for entry in entries.iter().filter(|entry| mentions(&folded, &entry.term)) {
        if chosen.len() >= MAX_GLOSSARY_ENTRIES_IN_PROMPT {
            break;
        }
        let cost = entry.term.chars().count() + entry.definition.chars().count() + 5;
        if used + cost > budget_chars {
            continue;
        }
        used += cost;
        chosen.push(entry);
    }
    chosen
}

// ---------------------------------------------------------------------------
// The context of one recording
// ---------------------------------------------------------------------------

/// Finds the client of the meeting being recorded. The saved meeting row does
/// not exist yet, so the name comes from the known clients: one mention in the
/// meeting name or in the typed context is enough, the conversation needs two.
pub fn infer_client(known: &[String], meeting_name: &str, typed_context: &str, heard: &str) -> Option<String> {
    if known.is_empty() {
        return None;
    }
    let declared = fold(&format!("{meeting_name}\n{typed_context}"));
    if let Some(client) = find_known_client_in_text(&declared, known) {
        return Some(client);
    }
    let heard = fold(heard);
    let client = find_known_client_in_text(&heard, known)?;
    let needle = fold(&client);
    let mut count = 0;
    let mut rest = heard.as_str();
    while let Some(found) = rest.find(&needle) {
        count += 1;
        rest = &rest[found + needle.len()..];
    }
    (count >= 2).then_some(client)
}

fn most_common(values: impl Iterator<Item = String>, limit: usize) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for value in values {
        let value = one_line(&value);
        if value.is_empty() {
            continue;
        }
        match counts.iter_mut().find(|(name, _)| fold(name) == fold(&value)) {
            Some(entry) => entry.1 += 1,
            None => counts.push((value, 1)),
        }
    }
    counts.sort_by(|left, right| right.1.cmp(&left.1));
    counts.into_iter().take(limit).map(|(name, _)| name).collect()
}

/// Cached per recording and refreshed on an interval, never per segment.
#[derive(Debug, Clone, Default)]
pub struct LiveContext {
    pub meeting_name: Option<String>,
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    /// Earlier meetings of the client that were read.
    pub past_meetings: usize,
    pub notes: Vec<MemoryNote>,
    pub settings: ContextSettings,
    /// False when the "Memória" feature or the switch is off.
    pub memory_allowed: bool,
}

impl LiveContext {
    /// Builds the cached context from what the database returned.
    pub fn assemble(
        settings: ContextSettings,
        memory_allowed: bool,
        meeting_name: Option<String>,
        client: Option<String>,
        mentioned: &str,
        past: &[PastMeeting],
        now: DateTime<Utc>,
    ) -> Self {
        let meeting_name = meeting_name
            .map(|name| one_line(&name))
            .filter(|name| !name.is_empty() && !default_title(name));
        let mut context = LiveContext {
            meeting_name,
            client,
            settings,
            memory_allowed,
            ..LiveContext::default()
        };
        if context.client.is_none() {
            return context;
        }
        let folded = fold(mentioned);
        let projects = most_common(past.iter().filter_map(|meeting| meeting.project.clone()), 8);
        context.project = projects
            .iter()
            .find(|project| mentions(&folded, project))
            .cloned()
            .or_else(|| (projects.len() == 1).then(|| projects[0].clone()));
        context.category = most_common(past.iter().filter_map(|meeting| meeting.category.clone()), 1)
            .into_iter()
            .next();
        context.tags = most_common(past.iter().flat_map(|meeting| meeting.tags.clone()), MAX_TAGS);
        if memory_allowed {
            context.past_meetings = past.len();
            context.notes = past
                .iter()
                .enumerate()
                .flat_map(|(recency, meeting)| notes_from_meeting(meeting, recency, now))
                .collect();
        }
        context
    }

    pub fn summary(&self, speakers: &[String], extras_allowed: bool) -> ContextSummary {
        ContextSummary {
            meeting_name: self.meeting_name.clone(),
            client: self.client.clone(),
            project: self.project.clone(),
            category: self.category.clone(),
            tags: self.tags.clone(),
            speakers: speakers.iter().take(MAX_SPEAKERS).cloned().collect(),
            past_meetings: self.past_meetings,
            memory_notes: self.notes.len(),
            use_past_meetings: self.memory_allowed,
            profile_enabled: self.settings.profile_enabled,
            profile_filled: self.settings.profile_filled(),
            glossary_terms: self.settings.glossary_entries().len(),
            local_only_blocked: !extras_allowed,
        }
    }
}

/// "Contexto em uso", shown in the panel. Counts and names only, no notes.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSummary {
    pub meeting_name: Option<String>,
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub speakers: Vec<String>,
    pub past_meetings: usize,
    pub memory_notes: usize,
    pub use_past_meetings: bool,
    pub profile_enabled: bool,
    pub profile_filled: bool,
    pub glossary_terms: usize,
    /// True when the model is a cloud service: the profile and the earlier
    /// meetings stay out of the prompt so nothing new leaves the machine.
    pub local_only_blocked: bool,
}

// ---------------------------------------------------------------------------
// Origins
// ---------------------------------------------------------------------------

pub const ORIGIN_MEETING: &str = "reuniao";
pub const ORIGIN_PROFILE: &str = "perfil";
pub const ORIGIN_GLOSSARY: &str = "glossario";
pub const ORIGIN_INTERNET: &str = "internet";
pub const ORIGIN_AGENDA: &str = "pauta";
pub const ORIGIN_TALK: &str = "conversa";
pub const ORIGIN_MODEL: &str = "modelo";

/// Where one suggestion came from. Optional field of every live assist item.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssistOrigin {
    /// "reuniao" | "perfil" | "glossario" | "internet" | "pauta" | "conversa" | "modelo"
    pub kind: String,
    /// Short text for the tag, for example "Reunião de 12/11".
    pub label: String,
    /// Meeting title, glossary term or link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_id: Option<String>,
}

impl AssistOrigin {
    pub fn simple(kind: &str, label: &str) -> Self {
        Self {
            kind: kind.to_string(),
            label: label.to_string(),
            detail: None,
            meeting_id: None,
        }
    }

    pub fn internet(source: &str, url: &str) -> Self {
        Self {
            kind: ORIGIN_INTERNET.to_string(),
            label: source.to_string(),
            detail: Some(url.to_string()),
            meeting_id: None,
        }
    }
}

/// One piece of text that went into the prompt, with the origin it stands for.
#[derive(Debug, Clone)]
pub struct SourceRef {
    pub origin: AssistOrigin,
    tokens: HashSet<String>,
    /// Folded term that settles the match on its own (glossary).
    key_term: Option<String>,
}

impl SourceRef {
    pub fn new(origin: AssistOrigin, text: &str) -> Self {
        Self {
            origin,
            tokens: content_tokens(text),
            key_term: None,
        }
    }
}

fn capitalized(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn claimed_kind(claim: &str) -> Option<&'static str> {
    let folded = fold(claim);
    let has = |needles: &[&str]| needles.iter().any(|needle| folded.contains(needle));
    if has(&["gloss"]) {
        Some(ORIGIN_GLOSSARY)
    } else if has(&["perfil"]) {
        Some(ORIGIN_PROFILE)
    } else if has(&["wiki", "duckduck", "internet", "http", "fonte"]) {
        Some(ORIGIN_INTERNET)
    } else if has(&["pauta", "contexto da reuni"]) {
        Some(ORIGIN_AGENDA)
    } else if has(&["reuni", "anterior", "memoria"]) || folded.chars().filter(|c| *c == '/').count() >= 1 {
        Some(ORIGIN_MEETING)
    } else if has(&["conversa", "trecho", "transcri", "fala"]) {
        Some(ORIGIN_TALK)
    } else {
        None
    }
}

fn best_match<'a>(candidates: impl Iterator<Item = &'a SourceRef>, folded_text: &str, tokens: &HashSet<String>, minimum: usize) -> Option<&'a SourceRef> {
    let mut best: Option<(usize, &SourceRef)> = None;
    for source in candidates {
        let mut score = overlap(&source.tokens, tokens);
        if source.key_term.as_deref().is_some_and(|term| mentions(folded_text, term)) {
            score += 3;
        }
        if score >= minimum && best.map_or(true, |(top, _)| score > top) {
            best = Some((score, source));
        }
    }
    best.map(|(_, source)| source)
}

/// Decides the origin tag of one item. The model's own claim is only kept
/// when that source really was in the prompt and shares words with the item;
/// otherwise the origin is found by comparing the item with every source.
/// A context item that matches nothing is marked as coming from the model.
pub fn resolve_origin(section: &str, text: &str, claimed: Option<&str>, sources: &[SourceRef]) -> Option<AssistOrigin> {
    let folded = fold(text);
    let tokens = content_tokens(text);
    let grounded = |source: &&SourceRef| source.origin.kind != ORIGIN_TALK;

    if let Some(claim) = claimed.map(str::trim).filter(|claim| !claim.is_empty()) {
        if let Some(kind) = claimed_kind(claim) {
            let folded_claim = fold(claim);
            let of_kind: Vec<&SourceRef> = sources.iter().filter(|source| source.origin.kind == kind).collect();
            // "reunião de 12/11": prefer the meeting with that date.
            let dated: Vec<&SourceRef> = of_kind
                .iter()
                .copied()
                .filter(|source| {
                    let date = source.origin.label.rsplit(' ').next().unwrap_or_default();
                    kind == ORIGIN_MEETING && !date.is_empty() && folded_claim.contains(date)
                })
                .collect();
            let pool = if dated.is_empty() { of_kind } else { dated };
            if let Some(source) = best_match(pool.into_iter(), &folded, &tokens, 1) {
                return Some(source.origin.clone());
            }
        }
    }
    // "Na reunião de 12/11 ficou pendente...": the date written in the text
    // points to that meeting.
    if folded.contains("reuni") {
        let dated = sources.iter().filter(|source| {
            let date = source.origin.label.rsplit(' ').next().unwrap_or_default();
            source.origin.kind == ORIGIN_MEETING && !date.is_empty() && folded.contains(date)
        });
        if let Some(source) = best_match(dated, &folded, &tokens, 1) {
            return Some(source.origin.clone());
        }
    }
    if let Some(source) = best_match(sources.iter().filter(grounded), &folded, &tokens, 2) {
        return Some(source.origin.clone());
    }
    if section != "contexto" {
        return None;
    }
    if best_match(sources.iter().filter(|source| source.origin.kind == ORIGIN_TALK), &folded, &tokens, 2).is_some() {
        return Some(AssistOrigin::simple(ORIGIN_TALK, "Conversa"));
    }
    Some(AssistOrigin::simple(ORIGIN_MODEL, "Sem fonte local"))
}

// ---------------------------------------------------------------------------
// Context found without the model
// ---------------------------------------------------------------------------

const MAX_DIRECT_GLOSSARY: usize = 2;
const MAX_DIRECT_DEFINITION_CHARS: usize = 160;
/// Content words a note must share with what was just said.
const DIRECT_NOTE_MIN_OVERLAP: usize = 2;

/// A context item that needs no model call: the text is the owner's own
/// glossary entry or a note of an earlier meeting, word for word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectItem {
    pub text: String,
    pub origin: AssistOrigin,
}

/// Glossary terms that were just said and the one earlier note that best
/// matches the recent talk. Runs on cached data, so it is instant and works
/// the same with a small model, a slow machine or a cloud provider (nothing
/// here is sent anywhere).
pub fn direct_items(context: &LiveContext, recent_talk: &str) -> Vec<DirectItem> {
    let mut items = Vec::new();
    let folded = fold(recent_talk);
    if context.settings.profile_enabled {
        for entry in context
            .settings
            .glossary_entries()
            .into_iter()
            .filter(|entry| mentions(&folded, &entry.term))
            .take(MAX_DIRECT_GLOSSARY)
        {
            let definition: String = entry.definition.chars().take(MAX_DIRECT_DEFINITION_CHARS).collect();
            items.push(DirectItem {
                text: format!("{}: {}", entry.term, definition.trim_end()),
                origin: AssistOrigin {
                    kind: ORIGIN_GLOSSARY.to_string(),
                    label: "Glossário".to_string(),
                    detail: Some(entry.term),
                    meeting_id: None,
                },
            });
        }
    }
    if context.memory_allowed {
        let talk = content_tokens(recent_talk);
        let best = context
            .notes
            .iter()
            .filter(|note| note.kind != NoteKind::Assunto)
            .map(|note| (overlap(&content_tokens(&note.text), &talk), note))
            .filter(|(shared, _)| *shared >= DIRECT_NOTE_MIN_OVERLAP)
            // Most shared words; then the latest meeting.
            .max_by(|left, right| left.0.cmp(&right.0).then(right.1.recency.cmp(&left.1.recency)));
        if let Some((_, note)) = best {
            items.push(DirectItem {
                text: format!("{} de {}: {}", note.kind.label(), note.date_label, note.text),
                origin: AssistOrigin {
                    kind: ORIGIN_MEETING.to_string(),
                    label: capitalized(&note.origin_label()),
                    detail: Some(note.meeting_title.clone()),
                    meeting_id: Some(note.meeting_id.clone()),
                },
            });
        }
    }
    items
}

// ---------------------------------------------------------------------------
// Prompt block
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub meeting: usize,
    pub profile: usize,
    pub glossary: usize,
    pub memory: usize,
}

impl Budget {
    /// Automatic context left after the typed meeting context took its share.
    pub fn for_typed_context(typed_chars: usize) -> Self {
        let total = PROMPT_CONTEXT_CHARS
            .saturating_sub(typed_chars)
            .clamp(AUTO_CONTEXT_MIN_CHARS, AUTO_CONTEXT_MAX_CHARS);
        Self::with_total(total)
    }

    pub fn with_total(total: usize) -> Self {
        // Block titles and separators are paid first.
        let usable = total.saturating_sub(HEADER_OVERHEAD_CHARS);
        Self {
            meeting: usable * 16 / 100,
            profile: usable * 24 / 100,
            glossary: usable * 20 / 100,
            memory: usable * 40 / 100,
        }
    }

    #[cfg(test)]
    pub fn total(self) -> usize {
        self.meeting + self.profile + self.glossary + self.memory
    }
}

#[derive(Debug, Clone, Default)]
pub struct RenderedContext {
    /// Block for the user prompt. Empty when there is nothing to add.
    pub text: String,
    pub sources: Vec<SourceRef>,
    pub memory_notes: usize,
    pub glossary_entries: usize,
}

fn push_field(line: &mut String, name: &str, value: &str, budget: usize) {
    let value = one_line(value);
    if value.is_empty() {
        return;
    }
    let addition = if line.is_empty() {
        format!("{name}: {value}")
    } else {
        format!(" | {name}: {value}")
    };
    if line.chars().count() + addition.chars().count() <= budget {
        line.push_str(&addition);
    }
}

fn profile_line(name: &str, value: &str, left: &mut usize) -> Option<String> {
    let value = one_line(value);
    if value.is_empty() {
        return None;
    }
    let prefix = format!("{name}: ");
    let room = left.checked_sub(prefix.chars().count() + 1)?;
    if room < 20 {
        return None;
    }
    let value = if value.chars().count() > room {
        format!("{}…", value.chars().take(room.saturating_sub(1)).collect::<String>().trim_end())
    } else {
        value
    };
    let line = format!("{prefix}{value}");
    *left = left.saturating_sub(line.chars().count() + 1);
    Some(line)
}

/// Builds the automatic context block for one model call.
///
/// `focus` is what is being said now; it decides which glossary entries and
/// which notes get in. `include_extras` is false for a cloud model: then the
/// profile, the glossary and the earlier meetings stay out.
pub fn render_context(context: &LiveContext, speakers: &[String], focus: &str, budget: Budget, include_extras: bool) -> RenderedContext {
    let mut rendered = RenderedContext::default();
    let mut blocks: Vec<String> = Vec::new();

    // 1. This meeting.
    let mut meeting = String::new();
    if let Some(client) = &context.client {
        push_field(&mut meeting, "Cliente", client, budget.meeting);
    }
    if let Some(project) = &context.project {
        push_field(&mut meeting, "Projeto", project, budget.meeting);
    }
    let speakers: Vec<&str> = speakers.iter().take(MAX_SPEAKERS).map(String::as_str).collect();
    push_field(&mut meeting, "Quem fala", &speakers.join(", "), budget.meeting);
    if let Some(name) = &context.meeting_name {
        push_field(&mut meeting, "Assunto", name, budget.meeting);
    }
    // Usual category and tags of this client's earlier meetings.
    if let Some(category) = &context.category {
        push_field(&mut meeting, "Categoria", category, budget.meeting);
    }
    push_field(&mut meeting, "Etiquetas", &context.tags.join(", "), budget.meeting);
    if !meeting.is_empty() {
        blocks.push(format!("Dados desta reunião:\n{meeting}"));
    }

    if include_extras && context.settings.profile_enabled {
        // 2. Owner profile.
        let mut left = budget.profile;
        let lines: Vec<String> = [
            ("Quem sou", &context.settings.about),
            ("Empresa", &context.settings.company),
            ("Produtos e serviços", &context.settings.offerings),
        ]
        .into_iter()
        .filter_map(|(name, value)| profile_line(name, value, &mut left))
        .collect();
        if !lines.is_empty() {
            let body = lines.join("\n");
            rendered
                .sources
                .push(SourceRef::new(AssistOrigin::simple(ORIGIN_PROFILE, "Perfil"), &body));
            blocks.push(format!("Perfil do dono:\n{body}"));
        }

        // 3. Glossary.
        let entries = context.settings.glossary_entries();
        let chosen = select_glossary(&entries, focus, budget.glossary);
        if !chosen.is_empty() {
            let mut body = String::new();
            for entry in &chosen {
                body.push_str(&format!("- {}: {}\n", entry.term, entry.definition));
                let mut source = SourceRef::new(
                    AssistOrigin {
                        kind: ORIGIN_GLOSSARY.to_string(),
                        label: "Glossário".to_string(),
                        detail: Some(entry.term.clone()),
                        meeting_id: None,
                    },
                    &format!("{} {}", entry.term, entry.definition),
                );
                source.key_term = Some(fold(&entry.term));
                rendered.sources.push(source);
            }
            rendered.glossary_entries = chosen.len();
            blocks.push(format!("Glossário:\n{}", body.trim_end()));
        }
    }

    // 4. Earlier meetings of the same client.
    if include_extras && context.memory_allowed {
        let chosen = select_memory(&context.notes, focus, budget.memory);
        if !chosen.is_empty() {
            let mut body = String::new();
            for note in &chosen {
                body.push_str(&note.line());
                body.push('\n');
                rendered.sources.push(SourceRef::new(
                    AssistOrigin {
                        kind: ORIGIN_MEETING.to_string(),
                        label: capitalized(&note.origin_label()),
                        detail: Some(note.meeting_title.clone()),
                        meeting_id: Some(note.meeting_id.clone()),
                    },
                    &note.text,
                ));
            }
            rendered.memory_notes = chosen.len();
            let client = context.client.as_deref().unwrap_or("o mesmo cliente");
            blocks.push(format!("Reuniões anteriores com {client}:\n{}", body.trim_end()));
        }
    }

    rendered.text = blocks.join("\n\n");
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap()
            .and_utc()
    }

    fn meeting(id: &str, day: u32, summary: Option<&str>) -> PastMeeting {
        PastMeeting {
            id: id.to_string(),
            title: format!("Aurora · Reunião {id}"),
            created: at(day),
            project: Some("Portal".to_string()),
            category: Some("Cliente".to_string()),
            tags: vec!["contrato".to_string()],
            subject: Some("Renovação do contrato".to_string()),
            summary_markdown: summary.map(str::to_string),
        }
    }

    const SUMMARY: &str = "# Aurora\n\n## Resumo\nA Aurora quer renovar o contrato do Portal por mais doze meses.\n\n## Key Decisions\n- Manter o **SLA** de 99,9% no novo contrato\n- N/A\n\n## Action Items\n| **Owner** | **Task** | **Due** |\n| --- | --- | --- |\n| Renan | Enviar a proposta revisada com desconto de 8% | sexta |\n| Ana | Validar o orçamento com o financeiro | 20/11 |\n\n## Outros\n- Falaram do evento de dezembro na sede\n";

    fn settings() -> ContextSettings {
        ContextSettings {
            about: "Renan, consultor de dados".to_string(),
            company: "Tech Fernandes, consultoria de software".to_string(),
            offerings: "Painéis FinOps, apps móveis e automação de atendimento".to_string(),
            glossary: "SLA: acordo de nível de serviço\nFinOps = gestão de custo de nuvem\nlinha sem separador\nRPO - ponto de recuperação aceitável\nsla: repetido".to_string(),
            ..ContextSettings::default()
        }
    }

    #[test]
    fn glossary_lines_are_parsed_and_deduplicated() {
        let entries = settings().glossary_entries();
        let terms: Vec<&str> = entries.iter().map(|entry| entry.term.as_str()).collect();
        assert_eq!(terms, vec!["SLA", "FinOps", "RPO"]);
        assert_eq!(entries[2].definition, "ponto de recuperação aceitável");
    }

    #[test]
    fn settings_are_clamped_and_camel_case() {
        let long = ContextSettings {
            glossary: "x".repeat(MAX_GLOSSARY_CHARS + 50),
            about: format!("  {}  ", "a".repeat(400)),
            ..ContextSettings::default()
        }
        .normalized();
        assert_eq!(long.glossary.chars().count(), MAX_GLOSSARY_CHARS);
        assert_eq!(long.about.chars().count(), 300);
        let json = serde_json::to_value(ContextSettings::default()).unwrap();
        assert_eq!(json["profileEnabled"], true);
        assert_eq!(json["usePastMeetings"], true);
        // Older callers may omit fields.
        let partial: ContextSettings = serde_json::from_str(r#"{"about":"Eu"}"#).unwrap();
        assert!(partial.profile_enabled && partial.use_past_meetings);
    }

    #[test]
    fn summary_is_split_into_decisions_pending_items_and_summary() {
        let notes = notes_from_summary(SUMMARY);
        let of = |kind: NoteKind| -> Vec<&str> {
            notes.iter().filter(|(k, _)| *k == kind).map(|(_, text)| text.as_str()).collect()
        };
        assert_eq!(of(NoteKind::Resumo), vec!["A Aurora quer renovar o contrato do Portal por mais doze meses."]);
        assert_eq!(of(NoteKind::Decisao), vec!["Manter o SLA de 99,9% no novo contrato"]);
        assert_eq!(
            of(NoteKind::Pendencia),
            vec![
                "Renan · Enviar a proposta revisada com desconto de 8% · sexta",
                "Ana · Validar o orçamento com o financeiro · 20/11",
            ]
        );
        assert_eq!(of(NoteKind::Topico), vec!["Falaram do evento de dezembro na sede"]);
    }

    #[test]
    fn long_notes_are_cut_at_a_word() {
        let long = format!("- {}", "palavra ".repeat(60));
        let notes = notes_from_summary(&format!("## Decisões\n{long}"));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].1.chars().count() <= MAX_NOTE_CHARS + 1);
        assert!(notes[0].1.ends_with('…'));
    }

    #[test]
    fn a_meeting_without_summary_keeps_only_its_subject() {
        let notes = notes_from_meeting(&meeting("m1", 12, None), 0, at(19));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].kind, NoteKind::Assunto);
        assert_eq!(notes[0].text, "Renovação do contrato");
        assert_eq!(notes[0].date_label, "12/11");
        assert_eq!(notes[0].origin_label(), "reunião de 12/11");

        let mut bare = meeting("m2", 12, None);
        bare.subject = None;
        bare.title = "Meeting 2026-11-12_10-00-00".to_string();
        assert!(notes_from_meeting(&bare, 0, at(19)).is_empty());
    }

    #[test]
    fn another_year_shows_in_the_date_label() {
        let old = chrono::NaiveDate::from_ymd_opt(2025, 3, 4).unwrap().and_hms_opt(15, 0, 0).unwrap().and_utc();
        assert_eq!(date_label(old, at(19)), "04/03/2025");
    }

    fn all_notes() -> Vec<MemoryNote> {
        let past = [meeting("m2", 12, Some(SUMMARY)), meeting("m1", 3, Some("## Decisões\n- Migrar o banco de dados para a nuvem em janeiro\n\n## Pendências\n- Levantar o custo mensal de armazenamento\n"))];
        past.iter()
            .enumerate()
            .flat_map(|(recency, meeting)| notes_from_meeting(meeting, recency, at(19)))
            .collect()
    }

    #[test]
    fn memory_selection_prefers_what_is_being_said_now() {
        let notes = all_notes();
        let chosen = select_memory(&notes, "E a migração do banco de dados para a nuvem, como ficou?", 520);
        assert!(chosen.len() <= MAX_MEMORY_NOTES_IN_PROMPT);
        assert!(chosen.iter().any(|note| note.text.contains("Migrar o banco")));
        // Without any overlap the latest pending items and decisions come first.
        let idle = select_memory(&notes, "bom dia a todos", 520);
        assert_eq!(idle.len(), MAX_MEMORY_NOTES_IN_PROMPT);
        assert_eq!(idle[0].recency, 0);
        assert!(idle.iter().all(|note| matches!(note.kind, NoteKind::Pendencia | NoteKind::Decisao)));
    }

    #[test]
    fn memory_selection_respects_the_budget() {
        let notes = all_notes();
        for budget in [0usize, 40, 120, 250, 520] {
            let chosen = select_memory(&notes, "proposta contrato banco", budget);
            let used: usize = chosen.iter().map(|note| note.line().chars().count() + 1).sum();
            assert!(used <= budget, "budget {budget} used {used}");
            assert!(chosen.len() <= MAX_MEMORY_NOTES_IN_PROMPT);
        }
        assert!(select_memory(&notes, "proposta", 0).is_empty());
        assert!(select_memory(&[], "proposta", 500).is_empty());
    }

    #[test]
    fn one_meeting_cannot_take_every_slot() {
        let notes = all_notes();
        let chosen = select_memory(&notes, "contrato proposta orçamento SLA Aurora Portal", 2000);
        let from_latest = chosen.iter().filter(|note| note.meeting_id == "m2").count();
        assert!(from_latest <= MAX_NOTES_PER_MEETING_IN_PROMPT);
    }

    #[test]
    fn glossary_selection_keeps_only_heard_terms() {
        let entries = settings().glossary_entries();
        let chosen = select_glossary(&entries, "qual é o rpo que vocês garantem?", 60);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].term, "RPO");
        assert!(select_glossary(&entries, "nada", 1000).is_empty());
        assert_eq!(select_glossary(&entries, "o SLA e o FinOps e o RPO", 1000).len(), 3);
        // Not inside another word.
        assert!(select_glossary(&entries, "o reembolso do translado", 1000).is_empty());
        assert!(select_glossary(&entries, "rpo", 5).is_empty());
    }

    #[test]
    fn client_is_inferred_from_name_context_or_repeated_mentions() {
        let known = vec!["Aurora".to_string(), "Boreal".to_string()];
        assert_eq!(infer_client(&known, "Aurora - alinhamento", "", ""), Some("Aurora".to_string()));
        assert_eq!(infer_client(&known, "Meeting 1", "Pauta: contrato da boreal", ""), Some("Boreal".to_string()));
        assert_eq!(infer_client(&known, "", "", "falei com a Aurora ontem"), None);
        assert_eq!(
            infer_client(&known, "", "", "a Aurora pediu prazo. Depois a Aurora confirma."),
            Some("Aurora".to_string())
        );
        assert_eq!(infer_client(&[], "Aurora", "", ""), None);
    }

    fn live_context() -> LiveContext {
        LiveContext::assemble(
            settings(),
            true,
            Some("Aurora · renovação".to_string()),
            Some("Aurora".to_string()),
            "vamos falar do portal",
            &[meeting("m2", 12, Some(SUMMARY)), meeting("m1", 3, None)],
            at(19),
        )
    }

    #[test]
    fn assembled_context_carries_the_client_data() {
        let context = live_context();
        assert_eq!(context.project.as_deref(), Some("Portal"));
        assert_eq!(context.category.as_deref(), Some("Cliente"));
        assert_eq!(context.tags, vec!["contrato"]);
        assert_eq!(context.past_meetings, 2);
        assert!(context.notes.len() >= 5);
        let summary = context.summary(&["Você".to_string(), "Ana".to_string()], true);
        assert_eq!(summary.client.as_deref(), Some("Aurora"));
        assert_eq!(summary.glossary_terms, 3);
        assert!(summary.profile_filled && summary.use_past_meetings && !summary.local_only_blocked);
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["pastMeetings"], 2);
        assert_eq!(json["localOnlyBlocked"], false);
        assert!(json.get("memoryNotes").is_some());
    }

    #[test]
    fn memory_switch_off_reads_no_notes() {
        let context = LiveContext::assemble(
            settings(),
            false,
            None,
            Some("Aurora".to_string()),
            "",
            &[meeting("m2", 12, Some(SUMMARY))],
            at(19),
        );
        assert_eq!(context.past_meetings, 0);
        assert!(context.notes.is_empty());
        let rendered = render_context(&context, &[], "proposta", Budget::with_total(AUTO_CONTEXT_MAX_CHARS), true);
        assert!(!rendered.text.contains("Reuniões anteriores"));
        assert!(rendered.text.contains("Perfil do dono"));
    }

    #[test]
    fn rendered_block_has_every_part_and_cites_dates() {
        let context = live_context();
        let speakers = vec!["Você".to_string(), "Ana".to_string()];
        let rendered = render_context(
            &context,
            &speakers,
            "a Ana perguntou do SLA e da proposta revisada",
            Budget::with_total(AUTO_CONTEXT_MAX_CHARS),
            true,
        );
        let text = &rendered.text;
        assert!(text.contains("Cliente: Aurora"));
        assert!(text.contains("Projeto: Portal"));
        assert!(text.contains("Quem fala: Você, Ana"));
        assert!(text.contains("Perfil do dono:\nQuem sou: Renan"));
        assert!(text.contains("- SLA: acordo de nível de serviço"));
        assert!(text.contains("Reuniões anteriores com Aurora"));
        assert!(text.contains("[reunião de 12/11] Pendência: Renan · Enviar a proposta revisada"));
        assert!(rendered.memory_notes >= 1 && rendered.glossary_entries >= 1);
        assert!(rendered.sources.iter().any(|source| source.origin.kind == ORIGIN_MEETING));
    }

    #[test]
    fn rendered_block_never_exceeds_the_budget() {
        let mut context = live_context();
        context.settings.offerings = "serviço ".repeat(80);
        context.settings.about = "a".repeat(300);
        context.settings.glossary = (0..60).map(|n| format!("T{n}: definição número {n} bem comprida para ocupar espaço")).collect::<Vec<_>>().join("\n");
        let speakers: Vec<String> = (0..20).map(|n| format!("Pessoa {n}")).collect();
        for typed in [0usize, 800, 1500, 2000] {
            let budget = Budget::for_typed_context(typed);
            let total = PROMPT_CONTEXT_CHARS.saturating_sub(typed).clamp(AUTO_CONTEXT_MIN_CHARS, AUTO_CONTEXT_MAX_CHARS);
            assert!(budget.total() <= total);
            let rendered = render_context(&context, &speakers, "proposta SLA contrato T7 T9", budget, true);
            assert!(
                rendered.text.chars().count() <= total,
                "typed {typed}: {} > {total}",
                rendered.text.chars().count()
            );
        }
        assert!(Budget::for_typed_context(0).total() <= AUTO_CONTEXT_MAX_CHARS);
        assert!(Budget::for_typed_context(2000).total() <= AUTO_CONTEXT_MIN_CHARS);
    }

    #[test]
    fn heard_terms_and_matching_notes_become_items_without_the_model() {
        let context = live_context();
        let items = direct_items(&context, "Ana: qual é o SLA? E a proposta revisada com desconto, sai quando?");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "SLA: acordo de nível de serviço");
        assert_eq!(items[0].origin.kind, ORIGIN_GLOSSARY);
        assert_eq!(items[0].origin.detail.as_deref(), Some("SLA"));
        assert_eq!(items[1].text, "Pendência de 12/11: Renan · Enviar a proposta revisada com desconto de 8% · sexta");
        assert_eq!(items[1].origin.label, "Reunião de 12/11");
        assert_eq!(items[1].origin.meeting_id.as_deref(), Some("m2"));

        // One shared word is not enough for a note, and small talk gives nothing.
        assert!(direct_items(&context, "vamos falar da proposta").is_empty());
        assert!(direct_items(&context, "bom dia, tudo bem?").is_empty());
        // At most two glossary terms per call.
        let many = direct_items(&context, "SLA, FinOps e RPO");
        assert_eq!(many.len(), 2);
    }

    #[test]
    fn switches_turn_the_direct_items_off() {
        let mut context = live_context();
        context.settings.profile_enabled = false;
        context.memory_allowed = false;
        assert!(direct_items(&context, "qual é o SLA da proposta revisada com desconto?").is_empty());
        assert!(direct_items(&LiveContext::default(), "SLA proposta revisada").is_empty());
    }

    #[test]
    fn a_cloud_model_gets_no_profile_and_no_earlier_meetings() {
        let rendered = render_context(&live_context(), &[], "proposta SLA", Budget::with_total(AUTO_CONTEXT_MAX_CHARS), false);
        assert!(rendered.text.contains("Cliente: Aurora"));
        assert!(!rendered.text.contains("Perfil"));
        assert!(!rendered.text.contains("Glossário"));
        assert!(!rendered.text.contains("Reuniões anteriores"));
        assert!(rendered.sources.is_empty());
    }

    #[test]
    fn nothing_to_say_renders_an_empty_block() {
        let rendered = render_context(&LiveContext::default(), &[], "oi", Budget::with_total(AUTO_CONTEXT_MAX_CHARS), true);
        assert!(rendered.text.is_empty());
        assert!(rendered.sources.is_empty());
    }

    fn sources() -> Vec<SourceRef> {
        let mut sources = render_context(
            &live_context(),
            &[],
            "a Ana perguntou do SLA e da proposta revisada",
            Budget::with_total(AUTO_CONTEXT_MAX_CHARS),
            true,
        )
        .sources;
        sources.push(SourceRef::new(
            AssistOrigin::internet("Wikipédia", "https://pt.wikipedia.org/wiki/LGPD"),
            "Lei Geral de Proteção de Dados Pessoais, lei brasileira de 2018",
        ));
        sources.push(SourceRef::new(
            AssistOrigin::simple(ORIGIN_TALK, "Conversa"),
            "a Ana perguntou do SLA e da proposta revisada e citou o Kanban do time",
        ));
        sources
    }

    #[test]
    fn a_valid_claim_is_kept_and_points_to_the_meeting() {
        let origin = resolve_origin(
            "contexto",
            "Ficou pendente enviar a proposta revisada com desconto de 8%.",
            Some("reunião de 12/11"),
            &sources(),
        )
        .unwrap();
        assert_eq!(origin.kind, ORIGIN_MEETING);
        assert_eq!(origin.label, "Reunião de 12/11");
        assert_eq!(origin.meeting_id.as_deref(), Some("m2"));
        assert_eq!(origin.detail.as_deref(), Some("Aurora · Reunião m2"));
    }

    #[test]
    fn a_false_claim_is_replaced_by_what_the_text_matches() {
        // Claims the profile, but the words are the glossary entry.
        let origin = resolve_origin("contexto", "SLA: acordo de nível de serviço.", Some("perfil"), &sources()).unwrap();
        assert_eq!(origin.kind, ORIGIN_GLOSSARY);
        assert_eq!(origin.detail.as_deref(), Some("SLA"));
        // Claims a meeting that was not in the prompt.
        let origin = resolve_origin("contexto", "LGPD é a lei brasileira de proteção de dados.", Some("reunião de 01/01"), &sources()).unwrap();
        assert_eq!(origin.kind, ORIGIN_INTERNET);
        assert_eq!(origin.label, "Wikipédia");
    }

    #[test]
    fn missing_claims_are_inferred_or_marked_as_unsourced() {
        let sources = sources();
        let origin = resolve_origin("contexto", "A Tech Fernandes faz painéis FinOps e apps móveis.", None, &sources).unwrap();
        assert!(origin.kind == ORIGIN_PROFILE || origin.kind == ORIGIN_GLOSSARY);
        let talk = resolve_origin("contexto", "Kanban do time foi citado pela Ana.", None, &sources).unwrap();
        assert_eq!(talk.kind, ORIGIN_TALK);
        let model = resolve_origin("contexto", "Kubernetes orquestra contêineres.", Some("conversa"), &sources).unwrap();
        assert_eq!(model.kind, ORIGIN_MODEL);
        assert_eq!(model.label, "Sem fonte local");
        // The date written in the sentence is enough to point to the meeting.
        let dated = resolve_origin("contexto", "Na reunião de 12/11 falaram da proposta.", None, &sources).unwrap();
        assert_eq!(dated.kind, ORIGIN_MEETING);
        assert_eq!(dated.meeting_id.as_deref(), Some("m2"));
        // Questions and answers only get a tag when a real source backs them.
        assert_eq!(resolve_origin("perguntas", "Qual é o prazo de entrega?", None, &sources), None);
        let answer = resolve_origin("respostas", "Para a Ana: a proposta revisada com desconto de 8% sai sexta.", None, &sources).unwrap();
        assert_eq!(answer.kind, ORIGIN_MEETING);
    }

    #[test]
    fn origin_json_skips_empty_fields() {
        let json = serde_json::to_value(AssistOrigin::simple(ORIGIN_PROFILE, "Perfil")).unwrap();
        assert_eq!(json, serde_json::json!({"kind":"perfil","label":"Perfil"}));
        let json = serde_json::to_value(AssistOrigin {
            kind: ORIGIN_MEETING.to_string(),
            label: "Reunião de 12/11".to_string(),
            detail: Some("Aurora".to_string()),
            meeting_id: Some("m2".to_string()),
        })
        .unwrap();
        assert_eq!(json["meetingId"], "m2");
    }
}
