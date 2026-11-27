//! Quem estava na reunião. Os nomes vêm da agenda do Mac (que enxerga contas
//! do Outlook/Teams já sincronizadas), de arquivos que o próprio Teams gera
//! (transcrição .vtt e relatório de presença .csv), de um .ics ou do que
//! você digita. Com um .vtt, as falas gravadas são casadas com os nomes.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Runtime};

use crate::organize::fold;

use super::{db_error, emit, load_settings, new_id, now, pool_from, EVENT_PARTICIPANTS};

const MAX_NAME_CHARS: usize = 80;
const MAX_PARTICIPANTS: usize = 80;
/// Provedores de e-mail pessoais: o domínio não identifica uma empresa.
const PERSONAL_DOMAINS: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "outlook.com.br", "hotmail.com", "hotmail.com.br", "live.com",
    "msn.com", "yahoo.com", "yahoo.com.br", "icloud.com", "me.com", "uol.com.br", "bol.com.br", "terra.com.br",
    "proton.me", "protonmail.com",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: String,
    pub meeting_id: String,
    pub name: String,
    pub email: Option<String>,
    pub organization: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Person {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub speaker_key: String,
    pub start: f64,
    pub text: String,
}

/// Nome proposto para uma voz da gravação.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerMatch {
    pub speaker_key: String,
    pub name: String,
    /// Fração das falas dessa voz que caíram nesse nome (0 a 1).
    pub share: f64,
    pub segments: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// vtt | csv | ics
    pub kind: String,
    pub added: usize,
    pub participants: Vec<Participant>,
    pub matches: Vec<SpeakerMatch>,
    /// As falas foram alinhadas pelo texto (true) ou só pelo relógio (false).
    pub aligned_by_text: bool,
}

// ---------------------------------------------------------------------------
// Texto
// ---------------------------------------------------------------------------

pub fn clean_name(raw: &str) -> Option<String> {
    let name = raw.replace(['"', '\u{feff}'], "");
    // "Souza, Ana (ACME)" → "Ana Souza"; sufixos entre parênteses saem.
    let name = name.split('(').next().unwrap_or("").trim();
    let name = match name.split_once(',') {
        Some((last, first)) if !first.trim().is_empty() && !last.contains('@') => format!("{} {}", first.trim(), last.trim()),
        _ => name.to_string(),
    };
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().take(MAX_NAME_CHARS).collect();
    (name.chars().filter(|c| c.is_alphabetic()).count() >= 2).then_some(name)
}

pub fn clean_email(raw: &str) -> Option<String> {
    let email = raw.trim().trim_start_matches("mailto:").trim_matches(['<', '>', '"', ' ']).to_lowercase();
    let (user, domain) = email.split_once('@')?;
    (!user.is_empty() && domain.contains('.') && !email.contains(char::is_whitespace)).then_some(email)
}

/// Domínio que identifica uma empresa, ou `None` para e-mail pessoal.
pub fn company_domain(email: &str) -> Option<String> {
    let domain = email.rsplit_once('@')?.1.trim().to_lowercase();
    (!domain.is_empty() && !PERSONAL_DOMAINS.contains(&domain.as_str())).then_some(domain)
}

fn name_from_email(email: &str) -> Option<String> {
    let user = email.split('@').next()?;
    let name = user
        .split(['.', '_', '-'])
        .filter(|part| !part.is_empty() && !part.chars().all(|c| c.is_ascii_digit()))
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    clean_name(&name)
}

fn push_person(people: &mut Vec<Person>, name: Option<String>, email: Option<String>) {
    let Some(name) = name.or_else(|| email.as_deref().and_then(name_from_email)) else { return };
    let key = fold(&name);
    if let Some(existing) = people.iter_mut().find(|person| fold(&person.name) == key) {
        if existing.email.is_none() {
            existing.email = email;
        }
        return;
    }
    if people.len() < MAX_PARTICIPANTS {
        people.push(Person { name, email });
    }
}

pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |bytes: &[u8], big: bool| -> String {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| if big { u16::from_be_bytes([pair[0], pair[1]]) } else { u16::from_le_bytes([pair[0], pair[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).to_string(),
        // Relatório do Teams sem BOM: UTF-16 tem um zero a cada dois bytes.
        _ if bytes.len() > 8 && bytes.iter().skip(1).step_by(2).take(32).filter(|b| **b == 0).count() > 24 => utf16(bytes, false),
        _ => String::from_utf8_lossy(bytes).to_string(),
    }
}

// ---------------------------------------------------------------------------
// Arquivos
// ---------------------------------------------------------------------------

fn parse_clock(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let seconds: f64 = parts.last()?.replace(',', ".").parse().ok()?;
    let minutes: f64 = if parts.len() >= 2 { parts[parts.len() - 2].parse().ok()? } else { 0.0 };
    let hours: f64 = if parts.len() >= 3 { parts[parts.len() - 3].parse().ok()? } else { 0.0 };
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Transcrição WebVTT do Teams: `<v Nome>fala</v>` em cada bloco.
pub fn parse_vtt(text: &str) -> Vec<Cue> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut timing: Option<(f64, f64)> = None;
    let mut speaker: Option<String> = None;
    let mut body = String::new();
    let mut flush = |timing: &mut Option<(f64, f64)>, speaker: &mut Option<String>, body: &mut String| {
        if let (Some((start, end)), Some(name)) = (timing.take(), speaker.take()) {
            let text = strip_tags(body);
            if !text.is_empty() {
                cues.push(Cue { start, end, speaker: name, text });
            }
        }
        body.clear();
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some((from, to)) = line.split_once("-->") {
            flush(&mut timing, &mut speaker, &mut body);
            let to = to.trim().split_whitespace().next().unwrap_or("");
            if let (Some(start), Some(end)) = (parse_clock(from), parse_clock(to)) {
                timing = Some((start, end));
            }
            continue;
        }
        if line.is_empty() {
            flush(&mut timing, &mut speaker, &mut body);
            continue;
        }
        if timing.is_none() {
            continue;
        }
        if speaker.is_none() {
            if let Some(rest) = line.strip_prefix("<v ") {
                if let Some((name, _)) = rest.split_once('>') {
                    speaker = clean_name(name);
                }
            } else if let Some((name, _)) = line.split_once(':') {
                // "Ana Souza: fala" (exportações de outras ferramentas).
                if name.split_whitespace().count() <= 4 && !name.chars().any(|c| c.is_ascii_digit()) {
                    speaker = clean_name(name);
                    if speaker.is_some() {
                        body.push_str(line.split_once(':').map(|(_, rest)| rest).unwrap_or(""));
                        body.push(' ');
                        continue;
                    }
                }
            }
        }
        body.push_str(line);
        body.push(' ');
    }
    flush(&mut timing, &mut speaker, &mut body);
    cues
}

fn split_cells(line: &str, delimiter: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c == delimiter && !quoted => cells.push(std::mem::take(&mut cell).trim().to_string()),
            c => cell.push(c),
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

/// Relatório de presença do Teams (.csv, separado por tabulação, em português
/// ou inglês) ou qualquer planilha com colunas de nome e e-mail.
pub fn parse_attendance(text: &str) -> Vec<Person> {
    let mut people = Vec::new();
    let mut columns: Option<(char, usize, Option<usize>)> = None;
    for line in text.lines() {
        let line = line.trim_matches(['\r', '\u{feff}']);
        if line.trim().is_empty() {
            columns = None;
            continue;
        }
        if let Some((delimiter, name_index, email_index)) = columns {
            let cells = split_cells(line, delimiter);
            let name = cells.get(name_index).and_then(|cell| clean_name(cell));
            let email = email_index.and_then(|index| cells.get(index)).and_then(|cell| clean_email(cell));
            if name.is_some() || email.is_some() {
                push_person(&mut people, name, email);
            }
            continue;
        }
        let delimiter = ['\t', ';', ','].into_iter().max_by_key(|d| line.matches(*d).count()).unwrap_or('\t');
        let header: Vec<String> = split_cells(line, delimiter).iter().map(|cell| fold(cell)).collect();
        let name_index = header.iter().position(|cell| {
            matches!(cell.as_str(), "name" | "nome" | "full name" | "nome completo" | "participante" | "participant")
        });
        if let Some(name_index) = name_index {
            let email_index = header
                .iter()
                .position(|cell| cell.contains("mail"))
                .or_else(|| header.iter().position(|cell| cell.contains("upn")));
            columns = Some((delimiter, name_index, email_index));
        }
    }
    people
}

/// Convidados de um .ics, com nome e e-mail.
pub fn parse_ics_people(ics: &str) -> Vec<Person> {
    let unfolded = ics.replace("\r\n ", "").replace("\n ", "").replace("\r\n\t", "").replace("\n\t", "");
    let mut people = Vec::new();
    for line in unfolded.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("ATTENDEE").or_else(|| line.strip_prefix("ORGANIZER")) else { continue };
        let name = rest
            .split(';')
            .find_map(|part| part.strip_prefix("CN="))
            .map(|cn| cn.split(":mailto").next().unwrap_or(cn).split(":MAILTO").next().unwrap_or(cn))
            .and_then(|cn| if cn.contains('@') { None } else { clean_name(cn) });
        let email = rest.to_lowercase().rsplit("mailto:").next().filter(|_| rest.to_lowercase().contains("mailto:")).and_then(clean_email);
        push_person(&mut people, name, email);
    }
    people
}

// ---------------------------------------------------------------------------
// Casar falas com nomes
// ---------------------------------------------------------------------------

fn word_set(text: &str) -> HashSet<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .map(str::to_string)
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let common = a.intersection(b).count() as f64;
    common / (a.len() + b.len()) as f64 * 2.0
}

/// Diferença entre o relógio do arquivo e o da gravação, achada pelas falas
/// que aparecem nos dois. `None` quando menos de três falas coincidem.
pub fn estimate_offset(cues: &[Cue], segments: &[Segment]) -> Option<f64> {
    let cue_words: Vec<HashSet<String>> = cues.iter().map(|cue| word_set(&cue.text)).collect();
    let candidates: Vec<&Segment> = segments.iter().filter(|segment| word_set(&segment.text).len() >= 4).collect();
    let step = (candidates.len() / 80).max(1);
    let mut diffs: Vec<f64> = Vec::new();
    for segment in candidates.iter().step_by(step) {
        let words = word_set(&segment.text);
        let best = cue_words
            .iter()
            .enumerate()
            .map(|(index, cue)| (index, jaccard(&words, cue)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((index, score)) = best {
            if score >= 0.6 {
                diffs.push(cues[index].start - segment.start);
            }
        }
    }
    if diffs.len() < 3 {
        return None;
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(diffs[diffs.len() / 2])
}

/// Para cada voz da gravação, o nome que mais falou nos mesmos instantes.
pub fn match_speakers(cues: &[Cue], segments: &[Segment], offset: f64) -> Vec<SpeakerMatch> {
    const SLACK: f64 = 2.5;
    let mut votes: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for segment in segments {
        let at = segment.start + offset;
        let cue = cues
            .iter()
            .filter(|cue| at >= cue.start - SLACK && at <= cue.end + SLACK)
            .min_by(|a, b| (a.start - at).abs().partial_cmp(&(b.start - at).abs()).unwrap_or(std::cmp::Ordering::Equal));
        let Some(cue) = cue else { continue };
        if !order.contains(&segment.speaker_key) {
            order.push(segment.speaker_key.clone());
        }
        *votes.entry(segment.speaker_key.clone()).or_default().entry(cue.speaker.clone()).or_default() += 1;
    }
    let mut matches = Vec::new();
    for key in order {
        let names = &votes[&key];
        let total: usize = names.values().sum();
        let Some((name, count)) = names.iter().max_by_key(|(name, count)| (**count, std::cmp::Reverse((*name).clone()))) else { continue };
        let share = *count as f64 / total as f64;
        if *count >= 2 && share >= 0.6 {
            matches.push(SpeakerMatch { speaker_key: key, name: name.clone(), share, segments: *count });
        }
    }
    matches
}

// ---------------------------------------------------------------------------
// Banco
// ---------------------------------------------------------------------------

pub async fn list(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<Participant>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, meeting_id, name, email, organization, source FROM meeting_participants
         WHERE meeting_id = ? ORDER BY rowid",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| Participant {
            id: row.try_get("id").unwrap_or_default(),
            meeting_id: row.try_get("meeting_id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            email: row.try_get::<Option<String>, _>("email").ok().flatten().filter(|v| !v.is_empty()),
            organization: row.try_get::<Option<String>, _>("organization").ok().flatten().filter(|v| !v.is_empty()),
            source: row.try_get("source").unwrap_or_default(),
        })
        .collect())
}

/// Nomes de todas as reuniões, para a busca da agenda: `(reunião, nomes)`.
pub async fn names_by_meeting(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, sqlx::Error> {
    let rows = sqlx::query("SELECT meeting_id, name FROM meeting_participants ORDER BY meeting_id, rowid")
        .fetch_all(pool)
        .await?;
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for row in rows {
        let meeting_id: String = row.try_get("meeting_id").unwrap_or_default();
        let name: String = row.try_get("name").unwrap_or_default();
        match out.last_mut() {
            Some((last, names)) if *last == meeting_id => names.push(name),
            _ => out.push((meeting_id, vec![name])),
        }
    }
    Ok(out)
}

/// Participantes de todas as reuniões, por id da reunião.
#[tauri::command]
pub async fn participants_index<R: Runtime>(
    app: AppHandle<R>,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let pool = pool_from(&app)?;
    Ok(names_by_meeting(&pool).await.map_err(db_error)?.into_iter().collect())
}

/// Grava quem ainda não está na lista; completa o e-mail de quem já está.
pub async fn add_people(pool: &SqlitePool, meeting_id: &str, people: &[Person], source: &str) -> Result<usize, sqlx::Error> {
    let mut added = 0;
    for person in people {
        let organization = person.email.as_deref().and_then(company_domain);
        let result = sqlx::query(
            "INSERT OR IGNORE INTO meeting_participants (id, meeting_id, name, email, organization, source, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id("person"))
        .bind(meeting_id)
        .bind(&person.name)
        .bind(&person.email)
        .bind(&organization)
        .bind(source)
        .bind(now())
        .execute(pool)
        .await?;
        if result.rows_affected() > 0 {
            added += 1;
        } else if person.email.is_some() {
            sqlx::query(
                "UPDATE meeting_participants SET email = ?, organization = ?
                 WHERE meeting_id = ? AND name = ? COLLATE NOCASE AND (email IS NULL OR email = '')",
            )
            .bind(&person.email)
            .bind(&organization)
            .bind(meeting_id)
            .bind(&person.name)
            .execute(pool)
            .await?;
        }
    }
    Ok(added)
}

async fn segments(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<Segment>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT speaker_key, audio_start_time, transcript FROM transcripts
         WHERE meeting_id = ? AND speaker_key IS NOT NULL AND audio_start_time IS NOT NULL
         ORDER BY audio_start_time",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(Segment {
                speaker_key: row.try_get::<Option<String>, _>("speaker_key").ok().flatten()?,
                start: row.try_get::<Option<f64>, _>("audio_start_time").ok().flatten()?,
                text: row.try_get("transcript").unwrap_or_default(),
            })
        })
        .collect())
}

async fn after_change<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str) {
    super::clients::apply_domain_hint(app, pool, meeting_id).await;
    emit(app, EVENT_PARTICIPANTS, Some(meeting_id));
}

pub async fn import_bytes(pool: &SqlitePool, meeting_id: &str, file_name: &str, bytes: &[u8]) -> Result<ImportResult, String> {
    let text = decode_text(bytes);
    let lower = file_name.to_lowercase();
    let looks_vtt = lower.ends_with(".vtt") || text.trim_start().starts_with("WEBVTT");
    let looks_ics = lower.ends_with(".ics") || text.contains("BEGIN:VCALENDAR");
    let (kind, people, matches, aligned_by_text) = if looks_vtt {
        let cues = parse_vtt(&text);
        if cues.is_empty() {
            return Err("Não encontrei falas com nome nesse arquivo. No Teams, baixe a transcrição em .vtt.".to_string());
        }
        let mut people = Vec::new();
        for cue in &cues {
            push_person(&mut people, Some(cue.speaker.clone()), None);
        }
        let segments = segments(pool, meeting_id).await.map_err(db_error)?;
        let offset = estimate_offset(&cues, &segments);
        let matches = match_speakers(&cues, &segments, offset.unwrap_or(0.0));
        ("vtt", people, matches, offset.is_some())
    } else if looks_ics {
        ("ics", parse_ics_people(&text), Vec::new(), false)
    } else {
        ("csv", parse_attendance(&text), Vec::new(), false)
    };
    if people.is_empty() {
        return Err("Não encontrei participantes nesse arquivo.".to_string());
    }
    let source = if kind == "ics" { "calendar" } else { "teams" };
    let added = add_people(pool, meeting_id, &people, source).await.map_err(db_error)?;
    Ok(ImportResult {
        kind: kind.to_string(),
        added,
        participants: list(pool, meeting_id).await.map_err(db_error)?,
        matches,
        aligned_by_text,
    })
}

// ---------------------------------------------------------------------------
// Agenda do Mac
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarMatch {
    pub title: String,
    pub people: Vec<Person>,
}

/// Linhas `título<TAB>Nome <email>|Nome <email>|`.
pub fn parse_calendar_people(output: &str) -> Vec<CalendarMatch> {
    output
        .lines()
        .filter_map(|line| {
            let (title, attendees) = line.split_once('\t')?;
            let mut people = Vec::new();
            for entry in attendees.split('|') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let (name, email) = match entry.rsplit_once('<') {
                    Some((name, email)) => (name.trim(), clean_email(email)),
                    None => (entry, None),
                };
                let name = if name.contains('@') || name == "missing value" { None } else { clean_name(name) };
                push_person(&mut people, name, email);
            }
            Some(CalendarMatch { title: title.trim().to_string(), people })
        })
        .collect()
}

/// O compromisso que mais parece a reunião gravada: título parecido primeiro,
/// depois o que tem mais convidados.
pub fn pick_event<'a>(events: &'a [CalendarMatch], meeting_title: &str) -> Option<&'a CalendarMatch> {
    let title = word_set(meeting_title);
    events
        .iter()
        .filter(|event| !event.people.is_empty())
        .max_by(|a, b| {
            let score = |event: &CalendarMatch| (jaccard(&title, &word_set(&event.title)), event.people.len());
            score(a).partial_cmp(&score(b)).unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[cfg(target_os = "macos")]
fn read_calendar(window_seconds: i64) -> Result<Vec<CalendarMatch>, String> {
    let script = format!(
        r#"
set output to ""
set windowEnd to current date
set windowStart to windowEnd - {window_seconds}
with timeout of 25 seconds
  tell application "Calendar"
    repeat with cal in calendars
      try
        set evs to (every event of cal whose start date ≤ windowEnd and end date ≥ windowStart)
        repeat with ev in evs
          if allday event of ev is false then
            set people to ""
            try
              repeat with att in attendees of ev
                set attName to ""
                set attMail to ""
                try
                  set attName to display name of att as text
                end try
                try
                  set attMail to email of att as text
                end try
                set people to people & attName & " <" & attMail & ">|"
              end repeat
            end try
            set output to output & (summary of ev) & tab & people & linefeed
          end if
        end repeat
      end try
    end repeat
  end tell
end timeout
return output
"#
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|_| "Não foi possível falar com o app Calendário.".to_string())?;
    if !output.status.success() {
        return Err("O Calendário não autorizou a leitura. Aceite o pedido de controle do app Calendário.".to_string());
    }
    Ok(parse_calendar_people(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(not(target_os = "macos"))]
fn read_calendar(_window_seconds: i64) -> Result<Vec<CalendarMatch>, String> {
    Err("A agenda local só é lida no macOS. Importe um .ics ou o relatório de presença do Teams.".to_string())
}

/// Lê da agenda os convidados do compromisso que aconteceu durante a reunião.
async fn capture_from_calendar<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str) -> Result<usize, String> {
    let meeting = crate::organize::get_organized_meeting(pool, meeting_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "Conversa não encontrada.".to_string())?;
    let age = chrono::Utc::now().signed_duration_since(meeting.created).num_seconds().max(0);
    if age > 12 * 3600 {
        return Err("A agenda só é consultada para reuniões das últimas 12 horas. Importe um arquivo do Teams.".to_string());
    }
    // A reunião é salva no fim: a janela cobre a duração dela e uma folga.
    let window = age + meeting.duration_seconds.unwrap_or(0.0) as i64 + 15 * 60;
    let events = tauri::async_runtime::spawn_blocking(move || read_calendar(window))
        .await
        .map_err(|_| "A leitura da agenda foi interrompida.".to_string())??;
    let Some(event) = pick_event(&events, &meeting.title) else {
        return Ok(0);
    };
    let added = add_people(pool, meeting_id, &event.people, "calendar").await.map_err(db_error)?;
    if added > 0 {
        after_change(app, pool, meeting_id).await;
    }
    Ok(added)
}

/// Depois de salvar a reunião: busca os convidados na agenda, se a leitura da
/// agenda já estiver ligada nos Recursos.
pub(crate) fn schedule_capture<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        let Ok(pool) = pool_from(&app) else { return };
        let wanted = load_settings(&pool).await.map(|settings| settings.auto_participants).unwrap_or(false);
        let calendar_on = crate::assist_plus::load_effective(&pool).await.map(|toggles| toggles.calendar_prep).unwrap_or(false);
        if !wanted || !calendar_on {
            return;
        }
        match capture_from_calendar(&app, &pool, &meeting_id).await {
            Ok(added) => log::info!("People: {} participant(s) read from the calendar", added),
            Err(error) => log::info!("People: calendar not read ({} chars)", error.chars().count()),
        }
    });
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn participants_list<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<Vec<Participant>, String> {
    let pool = pool_from(&app)?;
    list(&pool, &meeting_id).await.map_err(db_error)
}

#[tauri::command]
pub async fn participants_add<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    name: String,
    email: Option<String>,
) -> Result<Vec<Participant>, String> {
    let pool = pool_from(&app)?;
    let email = email.as_deref().and_then(clean_email);
    let name = clean_name(&name)
        .or_else(|| email.as_deref().and_then(name_from_email))
        .ok_or_else(|| "Escreva o nome da pessoa.".to_string())?;
    add_people(&pool, &meeting_id, &[Person { name, email }], "manual").await.map_err(db_error)?;
    after_change(&app, &pool, &meeting_id).await;
    list(&pool, &meeting_id).await.map_err(db_error)
}

#[tauri::command]
pub async fn participants_remove<R: Runtime>(app: AppHandle<R>, id: String) -> Result<bool, String> {
    let pool = pool_from(&app)?;
    let meeting_id: Option<String> = sqlx::query_scalar("SELECT meeting_id FROM meeting_participants WHERE id = ?")
        .bind(&id)
        .fetch_optional(&pool)
        .await
        .map_err(db_error)?;
    let result = sqlx::query("DELETE FROM meeting_participants WHERE id = ?").bind(&id).execute(&pool).await.map_err(db_error)?;
    emit(&app, EVENT_PARTICIPANTS, meeting_id.as_deref());
    Ok(result.rows_affected() > 0)
}

#[tauri::command]
pub async fn participants_from_calendar<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<Vec<Participant>, String> {
    let pool = pool_from(&app)?;
    capture_from_calendar(&app, &pool, &meeting_id).await?;
    list(&pool, &meeting_id).await.map_err(db_error)
}

/// Abre o seletor de arquivo e importa a transcrição .vtt, o relatório de
/// presença .csv ou um .ics. `None` quando o usuário cancela.
#[tauri::command]
pub async fn participants_import_file<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<Option<ImportResult>, String> {
    use tauri_plugin_dialog::DialogExt;
    let pool = pool_from(&app)?;
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("Arquivo do Teams ou da agenda")
            .add_filter("Transcrição, presença ou convite", &["vtt", "csv", "tsv", "txt", "ics"])
            .blocking_pick_file()
    })
    .await
    .map_err(|_| "Não foi possível abrir o seletor de arquivo.".to_string())?;
    let Some(path) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    if size > 20 * 1024 * 1024 {
        return Err("Arquivo grande demais (limite de 20 MB).".to_string());
    }
    let bytes = std::fs::read(&path).map_err(|_| "Não foi possível ler o arquivo.".to_string())?;
    let file_name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let result = import_bytes(&pool, &meeting_id, &file_name, &bytes).await?;
    after_change(&app, &pool, &meeting_id).await;
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VTT: &str = "WEBVTT\n\n\
1\n00:00:02.000 --> 00:00:06.000\n<v Ana Souza>Bom dia pessoal, vamos começar a reunião de planejamento.</v>\n\n\
2\n00:00:07.000 --> 00:00:12.000\n<v Carlos Lima>Perfeito, eu trouxe os números do trimestre para revisar.</v>\n\n\
3\n00:00:13.000 --> 00:00:18.000\n<v Ana Souza>Ótimo, então abre a planilha de receita por favor.</v>\n\n\
4\n00:00:19.000 --> 00:00:25.000\n<v Carlos Lima>A receita cresceu doze por cento em relação ao trimestre passado.</v>\n\n\
5\n00:00:26.000 --> 00:00:31.000\n<v Ana Souza>Excelente resultado, precisamos manter esse ritmo no próximo ciclo.</v>\n";

    fn segment(key: &str, start: f64, text: &str) -> Segment {
        Segment { speaker_key: key.to_string(), start, text: text.to_string() }
    }

    #[test]
    fn reads_teams_vtt() {
        let cues = parse_vtt(VTT);
        assert_eq!(cues.len(), 5);
        assert_eq!(cues[0].speaker, "Ana Souza");
        assert_eq!(cues[0].start, 2.0);
        assert_eq!(cues[1].end, 12.0);
        assert!(cues[3].text.starts_with("A receita cresceu"));
    }

    #[test]
    fn vtt_with_plain_name_prefix() {
        let cues = parse_vtt("WEBVTT\n\n00:01.000 --> 00:04.500\nAna: tudo certo\ncom o contrato\n");
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].speaker, "Ana");
        assert_eq!(cues[0].text, "tudo certo com o contrato");
        assert_eq!(cues[0].end, 4.5);
    }

    #[test]
    fn aligns_by_text_when_clocks_differ() {
        let cues = parse_vtt(VTT);
        // A gravação começou 40 s depois da transcrição do Teams.
        let segments = vec![
            segment("system:0", -38.0 + 0.5, "bom dia pessoal vamos começar a reunião de planejamento"),
            segment("system:1", -33.0 + 0.5, "perfeito eu trouxe os números do trimestre para revisar"),
            segment("system:0", -27.0 + 0.5, "ótimo então abre a planilha de receita por favor"),
            segment("system:1", -21.0 + 0.5, "a receita cresceu doze por cento em relação ao trimestre passado"),
            segment("system:0", -14.0 + 0.5, "excelente resultado precisamos manter esse ritmo no próximo ciclo"),
        ];
        let offset = estimate_offset(&cues, &segments).unwrap();
        assert!((offset - 39.5).abs() < 0.01, "offset {offset}");
        let matches = match_speakers(&cues, &segments, offset);
        assert_eq!(matches.len(), 2);
        assert_eq!((matches[0].speaker_key.as_str(), matches[0].name.as_str()), ("system:0", "Ana Souza"));
        assert_eq!((matches[1].speaker_key.as_str(), matches[1].name.as_str()), ("system:1", "Carlos Lima"));
        assert_eq!(matches[0].share, 1.0);
    }

    #[test]
    fn unrelated_text_gives_no_offset_and_weak_votes_give_no_match() {
        let cues = parse_vtt(VTT);
        let segments = vec![segment("system:0", 3.0, "assunto completamente diferente sobre férias e viagens")];
        assert_eq!(estimate_offset(&cues, &segments), None);
        // Uma fala só não basta para dar nome a uma voz.
        assert!(match_speakers(&cues, &segments, 0.0).is_empty());
    }

    #[test]
    fn reads_teams_attendance_report() {
        let report = "1. Resumo\nTítulo da reunião\tPlanejamento\n\n2. Participantes\n\
Nome\tPrimeiro ingresso\tÚltima saída\tDuração na reunião\tEmail\tID do participante (UPN)\tFunção\n\
Souza, Ana (ACME)\t27/11/2026 10:00\t27/11/2026 11:00\t1h\tana.souza@acme.com.br\tana.souza@acme.com.br\tOrganizador\n\
Carlos Lima\t27/11/2026 10:01\t27/11/2026 11:00\t59m\tcarlos@gmail.com\tcarlos@gmail.com\tParticipante\n\n\
3. Atividades na reunião\nNome\tHora de ingresso\nSouza, Ana (ACME)\t27/11/2026 10:00\n";
        let people = parse_attendance(report);
        assert_eq!(people.len(), 2);
        assert_eq!(people[0], Person { name: "Ana Souza".to_string(), email: Some("ana.souza@acme.com.br".to_string()) });
        assert_eq!(company_domain(people[0].email.as_deref().unwrap()).as_deref(), Some("acme.com.br"));
        assert_eq!(company_domain(people[1].email.as_deref().unwrap()), None);
    }

    #[test]
    fn decodes_utf16_reports() {
        let text = "Nome\tEmail\nJoão Silva\tjoao@acme.com\n";
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(|unit| unit.to_le_bytes()));
        assert_eq!(decode_text(&bytes), text);
        assert_eq!(parse_attendance(&decode_text(&bytes))[0].name, "João Silva");
        let without_bom: Vec<u8> = text.encode_utf16().flat_map(|unit| unit.to_le_bytes()).collect();
        assert_eq!(decode_text(&without_bom), text);
    }

    #[test]
    fn reads_csv_with_commas_and_quotes() {
        let people = parse_attendance("Full Name,Role,Email\n\"Lima, Carlos\",Attendee,CARLOS@Beta.io\n,Attendee,maria.clara@beta.io\n");
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].name, "Carlos Lima");
        assert_eq!(people[0].email.as_deref(), Some("carlos@beta.io"));
        assert_eq!(people[1].name, "Maria Clara");
    }

    #[test]
    fn reads_ics_attendees_with_email() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Proposta\nORGANIZER;CN=Renan:mailto:renan@tf.com.br\n\
ATTENDEE;CN=\"Ana Souza\";ROLE=REQ-PARTICIPANT:mailto:ana@acme.com\nATTENDEE;CN=bia@acme.com:MAILTO:bia@acme.com\nEND:VEVENT\nEND:VCALENDAR\n";
        let people = parse_ics_people(ics);
        assert_eq!(people.len(), 3);
        assert_eq!(people[1], Person { name: "Ana Souza".to_string(), email: Some("ana@acme.com".to_string()) });
        assert_eq!(people[2].name, "Bia");
    }

    #[test]
    fn picks_the_calendar_event_that_looks_like_the_meeting() {
        let events = parse_calendar_people(
            "Almoço\t\nPlanejamento ACME Q1\tAna Souza <ana@acme.com>|missing value <bia@acme.com>|\nDaily interna\tCarlos <carlos@tf.com.br>|\n",
        );
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].people.len(), 2);
        assert_eq!(events[1].people[1].name, "Bia");
        assert_eq!(pick_event(&events, "ACME · Planejamento do Q1").unwrap().title, "Planejamento ACME Q1");
        // Sem título parecido, vale o compromisso com mais convidados.
        assert_eq!(pick_event(&events, "Microsoft Teams (gravação automática)").unwrap().title, "Planejamento ACME Q1");
        assert!(pick_event(&events[..1], "Almoço").is_none());
    }

    #[tokio::test]
    async fn import_adds_people_once_and_completes_emails() {
        let pool = crate::workspace::test_pool().await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Reunião', '2026-11-27T12:00:00Z', '2026-11-27T12:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        let first = import_bytes(&pool, "m1", "reuniao.vtt", VTT.as_bytes()).await.unwrap();
        assert_eq!((first.kind.as_str(), first.added), ("vtt", 2));
        let second = import_bytes(&pool, "m1", "presenca.csv", "Nome\tEmail\nana souza\tana@acme.com\nBia Reis\tbia@acme.com\n".as_bytes()).await.unwrap();
        assert_eq!(second.added, 1);
        let people = list(&pool, "m1").await.unwrap();
        assert_eq!(people.len(), 3);
        assert_eq!(people[0].email.as_deref(), Some("ana@acme.com"));
        assert_eq!(people[0].organization.as_deref(), Some("acme.com"));
        assert!(import_bytes(&pool, "m1", "vazio.csv", b"nada aqui").await.is_err());
    }
}
