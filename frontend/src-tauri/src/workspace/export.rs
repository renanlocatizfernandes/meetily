//! Exportação de uma reunião em texto puro (.txt), em Markdown e como nota
//! num cofre do Obsidian. Nada sai da máquina: são arquivos gravados onde
//! você escolhe.

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager, Runtime};

use crate::database::repositories::transcript::TranscriptsRepository;
use crate::organize::fold;

use super::tasks::{Task, DIRECTION_MINE, DIRECTION_THEIRS};
use super::{db_error, load_settings, now, pool_from, WorkspaceSettings};

/// Tudo abaixo desta linha numa nota do Obsidian é seu e sobrevive a uma
/// nova exportação.
pub const KEEP_MARKER: &str = "%% meetily: o que estiver abaixo desta linha não é alterado %%";

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub start: Option<f64>,
    pub speaker: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeetingDoc {
    pub id: String,
    pub title: String,
    pub when: NaiveDateTime,
    pub duration_seconds: Option<f64>,
    pub client: Option<String>,
    pub project: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub participants: Vec<String>,
    pub summary: Option<String>,
    pub tasks: Vec<Task>,
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub name: String,
    pub path: String,
    /// A pasta tem a subpasta `.obsidian`.
    pub is_vault: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianNote {
    pub path: String,
    pub exists: bool,
}

// ---------------------------------------------------------------------------
// Texto
// ---------------------------------------------------------------------------

/// Nome de arquivo aceito no macOS, no Windows e pelo Obsidian.
pub fn safe_file_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned: String = cleaned.chars().take(110).collect();
    cleaned.trim_matches(['.', ' ']).to_string()
}

pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn duration_label(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as u64;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "menos de 1 min".to_string(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn speaker_label(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    let name = raw.trim();
    match name {
        "" => None,
        "mic" => Some("Você".to_string()),
        "system" => Some("Participantes".to_string()),
        other => Some(other.to_string()),
    }
}

/// Junta falas seguidas da mesma pessoa numa só.
pub fn merge_turns(segments: Vec<Turn>) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some(last) if last.speaker == segment.speaker && last.text.chars().count() < 1_200 => {
                last.text.push(' ');
                last.text.push_str(text);
            }
            _ => turns.push(Turn { start: segment.start, speaker: segment.speaker, text: text.to_string() }),
        }
    }
    turns
}

/// Markdown do resumo como texto corrido.
pub fn markdown_to_plain(markdown: &str) -> String {
    let mut out = String::new();
    for line in markdown.lines() {
        let trimmed = line.trim_end();
        let heading = trimmed.trim_start().trim_start_matches('#');
        let is_heading = trimmed.trim_start().starts_with('#') && heading.starts_with(' ');
        let mut text = if is_heading { heading.trim().to_uppercase() } else { trimmed.to_string() };
        for mark in ["**", "__", "`"] {
            text = text.replace(mark, "");
        }
        // [texto](link) → texto (link)
        while let (Some(open), Some(mid)) = (text.find('['), text.find("](")) {
            let Some(close) = text[mid..].find(')') else { break };
            if open > mid {
                break;
            }
            let label = text[open + 1..mid].to_string();
            let link = text[mid + 2..mid + close].to_string();
            text.replace_range(open..mid + close + 1, &format!("{label} ({link})"));
        }
        if is_heading && !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&text);
        out.push('\n');
    }
    out.trim().to_string()
}

fn task_line(task: &Task) -> String {
    let mut line = task.text.clone();
    let mut details = Vec::new();
    if let Some(owner) = &task.owner {
        details.push(owner.clone());
    }
    if let Some(date) = task.due_date.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
        details.push(format!("até {}", date.format("%d/%m/%Y")));
    } else if let Some(due) = &task.due {
        details.push(due.clone());
    }
    if !details.is_empty() {
        line.push_str(&format!(" ({})", details.join(", ")));
    }
    line
}

fn task_groups(tasks: &[Task]) -> Vec<(&'static str, Vec<&Task>)> {
    let pick = |direction: Option<&str>| -> Vec<&Task> {
        tasks
            .iter()
            .filter(|task| match direction {
                Some(direction) => task.direction == direction,
                None => task.direction != DIRECTION_MINE && task.direction != DIRECTION_THEIRS,
            })
            .collect()
    };
    [("Minhas", pick(Some(DIRECTION_MINE))), ("Aguardando retorno", pick(Some(DIRECTION_THEIRS))), ("Sem responsável", pick(None))]
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .collect()
}

fn header_lines(doc: &MeetingDoc) -> Vec<String> {
    let mut when = format!("Data: {}", doc.when.format("%d/%m/%Y %H:%M"));
    if let Some(seconds) = doc.duration_seconds {
        when.push_str(&format!(" · Duração: {}", duration_label(seconds)));
    }
    let mut lines = vec![when];
    let classification: Vec<String> = [("Cliente", &doc.client), ("Projeto", &doc.project), ("Categoria", &doc.category)]
        .iter()
        .filter_map(|(label, value)| value.as_ref().map(|value| format!("{label}: {value}")))
        .collect();
    if !classification.is_empty() {
        lines.push(classification.join(" · "));
    }
    if !doc.tags.is_empty() {
        lines.push(format!("Etiquetas: {}", doc.tags.join(", ")));
    }
    if !doc.participants.is_empty() {
        lines.push(format!("Participantes: {}", doc.participants.join(", ")));
    }
    lines
}

pub fn render_txt(doc: &MeetingDoc) -> String {
    let rule = |title: &str| format!("{title}\n{}\n", "-".repeat(title.chars().count()));
    let mut out = format!("{}\n{}\n", doc.title, "=".repeat(doc.title.chars().count().max(3)));
    out.push_str(&header_lines(doc).join("\n"));
    out.push('\n');
    if let Some(summary) = doc.summary.as_deref().map(markdown_to_plain).filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n{}{}\n", rule("RESUMO"), summary));
    }
    if !doc.tasks.is_empty() {
        out.push_str(&format!("\n{}", rule("TAREFAS")));
        for (label, tasks) in task_groups(&doc.tasks) {
            out.push_str(&format!("{label}:\n"));
            for task in tasks {
                let mark = if task.status == "done" { "[x]" } else { "[ ]" };
                out.push_str(&format!("  {mark} {}\n", task_line(task)));
            }
        }
    }
    if !doc.turns.is_empty() {
        out.push_str(&format!("\n{}", rule("CONVERSA")));
        for turn in &doc.turns {
            let time = turn.start.map(|start| format!("[{}] ", clock(start))).unwrap_or_default();
            match &turn.speaker {
                Some(speaker) => out.push_str(&format!("{time}{speaker}: {}\n", turn.text)),
                None => out.push_str(&format!("{time}{}\n", turn.text)),
            }
        }
    }
    out
}

fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn obsidian_tag(raw: &str) -> Option<String> {
    let tag: String = fold(raw)
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '/' { c } else { '-' })
        .collect();
    let tag = tag.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
    // Etiqueta só de números não é aceita pelo Obsidian.
    (!tag.is_empty() && !tag.chars().all(|c| c.is_ascii_digit())).then_some(tag)
}

fn wiki(name: &str, links: bool) -> String {
    let clean = name.replace(['[', ']', '|', '#', '^'], " ");
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if links && !clean.is_empty() { format!("[[{clean}]]") } else { clean }
}

/// Rebaixa os títulos do resumo para caberem debaixo de "## Resumo".
fn demote_headings(markdown: &str) -> String {
    let level = |line: &str| -> Option<usize> {
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        (hashes > 0 && trimmed[hashes..].starts_with(' ')).then_some(hashes)
    };
    let Some(top) = markdown.lines().filter_map(level).min() else {
        return markdown.trim().to_string();
    };
    let shift = 3usize.saturating_sub(top);
    markdown
        .lines()
        .map(|line| match level(line) {
            Some(hashes) => format!("{}{}", "#".repeat((hashes + shift).min(6)), &line.trim_start()[hashes..]),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn render_obsidian(doc: &MeetingDoc, include_transcript: bool, links: bool) -> String {
    let mut out = String::from("---\n");
    out.push_str("tipo: reuniao\n");
    out.push_str(&format!("data: {}\n", doc.when.format("%Y-%m-%d")));
    out.push_str(&format!("hora: \"{}\"\n", doc.when.format("%H:%M")));
    if let Some(seconds) = doc.duration_seconds {
        out.push_str(&format!("duracao_min: {}\n", (seconds / 60.0).round() as u64));
    }
    for (key, value) in [("cliente", &doc.client), ("projeto", &doc.project), ("categoria", &doc.category)] {
        if let Some(value) = value {
            out.push_str(&format!("{key}: {}\n", yaml_string(value)));
        }
    }
    if !doc.participants.is_empty() {
        out.push_str("participantes:\n");
        for person in &doc.participants {
            out.push_str(&format!("  - {}\n", yaml_string(person)));
        }
    }
    let mut tags = vec!["reuniao".to_string()];
    for tag in doc.tags.iter().filter_map(|tag| obsidian_tag(tag)) {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    out.push_str("tags:\n");
    for tag in &tags {
        out.push_str(&format!("  - {tag}\n"));
    }
    out.push_str(&format!("meetily_id: {}\n---\n\n# {}\n\n", yaml_string(&doc.id), doc.title));

    let mut facts = Vec::new();
    if let Some(client) = &doc.client {
        facts.push(format!("**Cliente:** {}", wiki(client, links)));
    }
    if let Some(project) = &doc.project {
        facts.push(format!("**Projeto:** {}", wiki(project, links)));
    }
    let mut when = format!("**Data:** {}", doc.when.format("%d/%m/%Y %H:%M"));
    if let Some(seconds) = doc.duration_seconds {
        when.push_str(&format!(" ({})", duration_label(seconds)));
    }
    facts.push(when);
    out.push_str(&facts.join(" · "));
    out.push('\n');
    if !doc.participants.is_empty() {
        let people: Vec<String> = doc.participants.iter().map(|person| wiki(person, links)).collect();
        out.push_str(&format!("**Participantes:** {}\n", people.join(", ")));
    }

    if let Some(summary) = doc.summary.as_deref().map(demote_headings).filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n## Resumo\n\n{summary}\n"));
    }
    if !doc.tasks.is_empty() {
        out.push_str("\n## Tarefas\n");
        for (label, tasks) in task_groups(&doc.tasks) {
            out.push_str(&format!("\n### {label}\n\n"));
            for task in tasks {
                let mark = if task.status == "done" { "x" } else { " " };
                let mut line = format!("- [{mark}] {}", task.text);
                if let Some(owner) = &task.owner {
                    line.push_str(&format!(" — {}", wiki(owner, links && task.direction == DIRECTION_THEIRS)));
                }
                // Formato lido pelo plugin Tasks do Obsidian.
                if let Some(date) = &task.due_date {
                    line.push_str(&format!(" 📅 {date}"));
                } else if let Some(due) = &task.due {
                    line.push_str(&format!(" ({due})"));
                }
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    if include_transcript && !doc.turns.is_empty() {
        out.push_str("\n## Conversa\n");
        for turn in &doc.turns {
            let time = turn.start.map(clock).unwrap_or_default();
            let head = match (&turn.speaker, time.is_empty()) {
                (Some(speaker), false) => format!("**{speaker}** · {time}"),
                (Some(speaker), true) => format!("**{speaker}**"),
                (None, false) => format!("*{time}*"),
                (None, true) => String::new(),
            };
            if head.is_empty() {
                out.push_str(&format!("\n{}\n", turn.text));
            } else {
                out.push_str(&format!("\n{head}\n{}\n", turn.text));
            }
        }
    }
    out.push_str(&format!("\n{KEEP_MARKER}\n"));
    out
}

/// Junta a nota nova com o que você escreveu abaixo do marcador na nota antiga.
pub fn keep_user_part(new_note: &str, old_note: Option<&str>) -> String {
    let user_part = old_note
        .and_then(|old| old.split_once(KEEP_MARKER))
        .map(|(_, rest)| rest.trim_start_matches(['\r', '\n']).to_string())
        .filter(|rest| !rest.trim().is_empty());
    match user_part {
        Some(rest) => format!("{new_note}\n{rest}"),
        None => format!("{new_note}\n## Minhas anotações\n\n"),
    }
}

pub fn note_file_name(doc: &MeetingDoc) -> String {
    let title = safe_file_name(&doc.title);
    let title = if title.is_empty() { "Reunião".to_string() } else { title };
    format!("{} {}.md", doc.when.format("%Y-%m-%d %H%M"), title)
}

pub fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Banco e arquivos
// ---------------------------------------------------------------------------

pub async fn load_doc(pool: &SqlitePool, meeting_id: &str) -> Result<MeetingDoc, String> {
    let meeting = crate::organize::get_organized_meeting(pool, meeting_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "Conversa não encontrada.".to_string())?;
    let rows = sqlx::query(
        "SELECT transcript, speaker, speaker_key, audio_start_time FROM transcripts
         WHERE meeting_id = ? ORDER BY audio_start_time IS NULL, audio_start_time, rowid",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    let aliases = TranscriptsRepository::speaker_aliases(pool, meeting_id).await.unwrap_or_default();
    let segments = rows
        .iter()
        .map(|row| {
            let speaker: Option<String> = row.try_get("speaker").unwrap_or(None);
            let speaker_key: Option<String> = row.try_get("speaker_key").unwrap_or(None);
            Turn {
                start: row.try_get::<Option<f64>, _>("audio_start_time").unwrap_or(None),
                speaker: speaker_label(TranscriptsRepository::display_speaker(&aliases, speaker, speaker_key)),
                text: row.try_get("transcript").unwrap_or_default(),
            }
        })
        .collect();
    let participants = super::people::list(pool, meeting_id).await.map_err(db_error)?;
    Ok(MeetingDoc {
        when: crate::organize::local_time(meeting.created),
        id: meeting.id,
        title: meeting.title,
        duration_seconds: meeting.duration_seconds,
        client: meeting.client,
        project: meeting.project,
        category: meeting.category,
        tags: meeting.tags,
        participants: participants.into_iter().map(|person| person.name).collect(),
        summary: crate::organize::summary_markdown(pool, meeting_id).await.map_err(db_error)?,
        tasks: super::tasks::list(pool, Some(meeting_id)).await.map_err(db_error)?,
        turns: merge_turns(segments),
    })
}

fn vault_info(path: &Path) -> VaultInfo {
    VaultInfo {
        name: path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
        path: path.to_string_lossy().to_string(),
        is_vault: path.join(".obsidian").is_dir(),
    }
}

/// Cofres listados no arquivo de configuração do próprio Obsidian.
pub fn vaults_from_config(json: &str) -> Vec<PathBuf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(vaults) = value.get("vaults").and_then(|vaults| vaults.as_object()) else { return Vec::new() };
    let mut entries: Vec<(i64, PathBuf)> = vaults
        .values()
        .filter_map(|vault| {
            let path = vault.get("path")?.as_str()?;
            Some((vault.get("ts").and_then(|ts| ts.as_i64()).unwrap_or(0), PathBuf::from(path)))
        })
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().map(|(_, path)| path).collect()
}

fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let temp = path.with_extension("md.tmp");
    std::fs::write(&temp, content)?;
    std::fs::rename(&temp, path)
}

/// Grava a nota no cofre. Uma nota já exportada dessa reunião é atualizada no
/// lugar (e renomeada se o título mudou), preservando o que está abaixo do
/// marcador.
pub async fn write_note(pool: &SqlitePool, settings: &WorkspaceSettings, meeting_id: &str) -> Result<PathBuf, String> {
    let vault = PathBuf::from(settings.obsidian_vault.trim());
    if settings.obsidian_vault.trim().is_empty() {
        return Err("Escolha o cofre do Obsidian em Configurações → Integrações.".to_string());
    }
    if !vault.is_dir() {
        return Err("A pasta do cofre não foi encontrada. Confira em Configurações → Integrações.".to_string());
    }
    let doc = load_doc(pool, meeting_id).await?;
    let mut folder = vault.clone();
    for part in settings.obsidian_folder.split('/').filter(|part| !part.is_empty() && *part != "." && *part != "..") {
        folder.push(part);
    }
    std::fs::create_dir_all(&folder).map_err(|_| "Não foi possível criar a pasta dentro do cofre.".to_string())?;
    let target = folder.join(note_file_name(&doc));

    let previous: Option<String> = sqlx::query_scalar("SELECT path FROM obsidian_notes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?;
    // Só mexe na nota anterior se ela ainda está dentro deste cofre.
    let previous = previous.map(PathBuf::from).filter(|path| path.is_file() && path.starts_with(&vault));
    let old_content = previous
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .or_else(|| std::fs::read_to_string(&target).ok());
    if let Some(old) = old_content.as_deref() {
        // Um arquivo com o mesmo nome que não é desta reunião não é sobrescrito.
        let same_meeting = old.contains(&format!("meetily_id: {}", yaml_string(&doc.id)));
        if !same_meeting {
            return Err("Já existe no cofre uma nota com esse nome que não veio desta reunião. Renomeie a reunião ou a nota.".to_string());
        }
    }
    let note = keep_user_part(&render_obsidian(&doc, settings.obsidian_transcript, settings.obsidian_links), old_content.as_deref());
    write_atomic(&target, &note).map_err(|_| "Não foi possível gravar a nota no cofre.".to_string())?;
    if let Some(previous) = previous.filter(|previous| *previous != target) {
        let _ = std::fs::remove_file(previous);
    }
    sqlx::query(
        "INSERT INTO obsidian_notes (meeting_id, path, exported_at) VALUES (?, ?, ?)
         ON CONFLICT(meeting_id) DO UPDATE SET path = excluded.path, exported_at = excluded.exported_at",
    )
    .bind(meeting_id)
    .bind(target.to_string_lossy().to_string())
    .bind(now())
    .execute(pool)
    .await
    .map_err(db_error)?;
    Ok(target)
}

fn open_in_obsidian(path: &Path) -> Result<(), String> {
    let uri = format!("obsidian://open?path={}", percent_encode(&path.to_string_lossy()));
    open::that_detached(uri).map_err(|_| "Não foi possível abrir o Obsidian. Ele está instalado?".to_string())
}

/// Depois do resumo: atualiza a nota no cofre, se a exportação automática
/// estiver ligada.
pub(crate) async fn run_automatic<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    let Ok(pool) = pool_from(app) else { return };
    let Ok(settings) = load_settings(&pool).await else { return };
    if !settings.obsidian_auto || settings.obsidian_vault.trim().is_empty() {
        return;
    }
    match write_note(&pool, &settings, meeting_id).await {
        Ok(_) => log::info!("Obsidian: note written for {}", meeting_id),
        Err(error) => log::warn!("Obsidian: automatic export failed for {}: {}", meeting_id, error),
    }
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

fn render(doc: &MeetingDoc, format: &str) -> Result<String, String> {
    match format {
        "txt" => Ok(render_txt(doc)),
        "md" => Ok(render_obsidian(doc, true, false).replace(&format!("\n{KEEP_MARKER}\n"), "")),
        _ => Err("Formato não suportado.".to_string()),
    }
}

/// Texto da exportação, para copiar ou pré-visualizar.
#[tauri::command]
pub async fn export_meeting_text<R: Runtime>(app: AppHandle<R>, meeting_id: String, format: String) -> Result<String, String> {
    let pool = pool_from(&app)?;
    render(&load_doc(&pool, &meeting_id).await?, &format)
}

/// Pergunta onde salvar e grava o arquivo. `None` quando o usuário cancela.
#[tauri::command]
pub async fn export_meeting_file<R: Runtime>(app: AppHandle<R>, meeting_id: String, format: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let pool = pool_from(&app)?;
    let doc = load_doc(&pool, &meeting_id).await?;
    let content = render(&doc, &format)?;
    let file_name = note_file_name(&doc).replace(".md", &format!(".{format}"));
    let label = if format == "txt" { "Texto" } else { "Markdown" };
    let dialog_app = app.clone();
    let extension = format.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("Exportar reunião")
            .set_file_name(&file_name)
            .add_filter(label, &[extension.as_str()])
            .blocking_save_file()
    })
    .await
    .map_err(|_| "Não foi possível abrir a janela de salvar.".to_string())?;
    let Some(path) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    let path = if path.extension().is_none() { path.with_extension(&format) } else { path };
    std::fs::write(&path, content).map_err(|_| "Não foi possível gravar o arquivo.".to_string())?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn obsidian_detect_vaults<R: Runtime>(app: AppHandle<R>) -> Result<Vec<VaultInfo>, String> {
    let mut vaults: Vec<VaultInfo> = Vec::new();
    for base in [app.path().data_dir().ok(), app.path().config_dir().ok()].into_iter().flatten() {
        let Ok(json) = std::fs::read_to_string(base.join("obsidian").join("obsidian.json")) else { continue };
        for path in vaults_from_config(&json) {
            let info = vault_info(&path);
            if path.is_dir() && !vaults.iter().any(|known| known.path == info.path) {
                vaults.push(info);
            }
        }
    }
    Ok(vaults)
}

#[tauri::command]
pub async fn obsidian_pick_vault<R: Runtime>(app: AppHandle<R>) -> Result<Option<VaultInfo>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Pasta do cofre do Obsidian").blocking_pick_folder()
    })
    .await
    .map_err(|_| "Não foi possível abrir o seletor de pasta.".to_string())?;
    Ok(picked.and_then(|folder| folder.into_path().ok()).map(|path| vault_info(&path)))
}

#[tauri::command]
pub async fn obsidian_export_meeting<R: Runtime>(app: AppHandle<R>, meeting_id: String, open: Option<bool>) -> Result<ObsidianNote, String> {
    let pool = pool_from(&app)?;
    let settings = load_settings(&pool).await.map_err(db_error)?;
    let path = write_note(&pool, &settings, &meeting_id).await?;
    if open.unwrap_or(false) {
        open_in_obsidian(&path)?;
    }
    Ok(ObsidianNote { path: path.to_string_lossy().to_string(), exists: true })
}

/// Nota já exportada desta reunião, se houver.
#[tauri::command]
pub async fn obsidian_note_for_meeting<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<Option<ObsidianNote>, String> {
    let pool = pool_from(&app)?;
    let path: Option<String> = sqlx::query_scalar("SELECT path FROM obsidian_notes WHERE meeting_id = ?")
        .bind(&meeting_id)
        .fetch_optional(&pool)
        .await
        .map_err(db_error)?;
    Ok(path.map(|path| ObsidianNote { exists: Path::new(&path).is_file(), path }))
}

#[tauri::command]
pub async fn obsidian_open_note<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<(), String> {
    let pool = pool_from(&app)?;
    let path: Option<String> = sqlx::query_scalar("SELECT path FROM obsidian_notes WHERE meeting_id = ?")
        .bind(&meeting_id)
        .fetch_optional(&pool)
        .await
        .map_err(db_error)?;
    let path = path.map(PathBuf::from).filter(|path| path.is_file()).ok_or_else(|| "Esta reunião ainda não foi enviada ao Obsidian.".to_string())?;
    open_in_obsidian(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn task(text: &str, direction: &str, owner: Option<&str>, due_date: Option<&str>, status: &str) -> Task {
        Task {
            id: text.to_string(),
            meeting_id: Some("m1".into()),
            meeting_title: None,
            meeting_date: None,
            client: None,
            text: text.to_string(),
            owner: owner.map(str::to_string),
            due: None,
            due_date: due_date.map(str::to_string),
            direction: direction.to_string(),
            status: status.to_string(),
            source: "auto".into(),
            evidence: None,
            created_at: String::new(),
            done_at: None,
        }
    }

    fn doc() -> MeetingDoc {
        MeetingDoc {
            id: "meeting-1".into(),
            title: "ACME · Proposta: fase 2?".into(),
            when: NaiveDate::from_ymd_opt(2026, 11, 27).unwrap().and_hms_opt(14, 30, 0).unwrap(),
            duration_seconds: Some(45.0 * 60.0),
            client: Some("ACME".into()),
            project: Some("Fase 2".into()),
            category: None,
            tags: vec!["Proposta Comercial".into(), "2026".into()],
            participants: vec!["Ana Souza".into(), "Você".into()],
            summary: Some("# Decisões\n- Fechar o **preço** até sexta\n\nVer [a planilha](https://exemplo.com/p).".into()),
            tasks: vec![
                task("Enviar a proposta", DIRECTION_MINE, Some("Você"), Some("2026-12-01"), "open"),
                task("Devolver o contrato", DIRECTION_THEIRS, Some("Ana Souza"), None, "done"),
            ],
            turns: merge_turns(vec![
                Turn { start: Some(3.0), speaker: Some("Ana Souza".into()), text: "Bom dia.".into() },
                Turn { start: Some(6.0), speaker: Some("Ana Souza".into()), text: "Vamos começar?".into() },
                Turn { start: Some(3725.0), speaker: Some("Você".into()), text: "Vamos.".into() },
                Turn { start: None, speaker: None, text: "  ".into() },
            ]),
        }
    }

    #[test]
    fn txt_has_every_section_in_plain_text() {
        let txt = render_txt(&doc());
        assert!(txt.starts_with("ACME · Proposta: fase 2?\n====="));
        assert!(txt.contains("Data: 27/11/2026 14:30 · Duração: 45 min"));
        assert!(txt.contains("Cliente: ACME · Projeto: Fase 2"));
        assert!(txt.contains("Participantes: Ana Souza, Você"));
        assert!(txt.contains("DECISÕES\n- Fechar o preço até sexta"));
        assert!(txt.contains("Ver a planilha (https://exemplo.com/p)."));
        assert!(txt.contains("Minhas:\n  [ ] Enviar a proposta (Você, até 01/12/2026)"));
        assert!(txt.contains("Aguardando retorno:\n  [x] Devolver o contrato (Ana Souza)"));
        assert!(txt.contains("[00:03] Ana Souza: Bom dia. Vamos começar?\n[1:02:05] Você: Vamos.\n"));
        assert!(!txt.contains("**") && !txt.contains('#'));
    }

    #[test]
    fn obsidian_note_has_frontmatter_links_and_tasks() {
        let note = render_obsidian(&doc(), true, true);
        assert!(note.starts_with("---\ntipo: reuniao\ndata: 2026-11-27\nhora: \"14:30\"\nduracao_min: 45\ncliente: \"ACME\"\n"));
        assert!(note.contains("participantes:\n  - \"Ana Souza\"\n"));
        // "2026" sozinho não é etiqueta válida no Obsidian.
        assert!(note.contains("tags:\n  - reuniao\n  - proposta-comercial\nmeetily_id: \"meeting-1\"\n---"));
        assert!(note.contains("**Cliente:** [[ACME]] · **Projeto:** [[Fase 2]]"));
        assert!(note.contains("## Resumo\n\n### Decisões\n- Fechar"));
        assert!(note.contains("### Minhas\n\n- [ ] Enviar a proposta — Você 📅 2026-12-01\n"));
        assert!(note.contains("### Aguardando retorno\n\n- [x] Devolver o contrato — [[Ana Souza]]\n"));
        assert!(note.contains("## Conversa\n\n**Ana Souza** · 00:03\nBom dia. Vamos começar?\n"));
        assert!(note.trim_end().ends_with(KEEP_MARKER));
        let short = render_obsidian(&doc(), false, false);
        assert!(!short.contains("## Conversa") && short.contains("**Cliente:** ACME"));
    }

    #[test]
    fn re_export_keeps_what_the_user_wrote() {
        let first = keep_user_part(&render_obsidian(&doc(), false, true), None);
        assert!(first.ends_with("## Minhas anotações\n\n"));
        let edited = format!("{first}Ligar para a Ana na segunda.\n");
        let second = keep_user_part(&render_obsidian(&doc(), true, true), Some(&edited));
        assert!(second.contains("## Conversa"));
        assert!(second.ends_with("## Minhas anotações\n\nLigar para a Ana na segunda.\n"));
        assert_eq!(second.matches(KEEP_MARKER).count(), 1);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(note_file_name(&doc()), "2026-11-27 1430 ACME · Proposta fase 2.md");
        assert_eq!(safe_file_name("  ../a/b:c*?.  "), "a b c");
        assert_eq!(safe_file_name("///"), "");
        assert_eq!(percent_encode("/Users/a b/Reuniões.md"), "%2FUsers%2Fa%20b%2FReuni%C3%B5es.md");
    }

    #[test]
    fn reads_obsidian_vault_list() {
        let json = r#"{"vaults":{"a1":{"path":"/Users/x/Notas","ts":10},"b2":{"path":"/Users/x/Trabalho","ts":20,"open":true}}}"#;
        assert_eq!(vaults_from_config(json), vec![PathBuf::from("/Users/x/Trabalho"), PathBuf::from("/Users/x/Notas")]);
        assert!(vaults_from_config("nada").is_empty());
    }

    #[tokio::test]
    async fn writes_and_updates_the_note_inside_the_vault() {
        let pool = crate::workspace::test_pool().await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, client) VALUES ('m1', 'Kickoff', '2026-11-27T12:00:00Z', '2026-11-27T12:00:00Z', 'ACME')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, speaker, audio_start_time) VALUES ('t1', 'm1', 'Olá a todos', '12:00', 'mic', 1.0)")
            .execute(&pool)
            .await
            .unwrap();
        let vault = std::env::temp_dir().join(format!("meetily-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&vault).unwrap();
        let settings = WorkspaceSettings { obsidian_vault: vault.to_string_lossy().to_string(), obsidian_folder: "Reuniões/Clientes".into(), ..WorkspaceSettings::default() };

        let first = write_note(&pool, &settings, "m1").await.unwrap();
        assert!(first.starts_with(vault.join("Reuniões").join("Clientes")));
        let content = std::fs::read_to_string(&first).unwrap();
        assert!(content.contains("**Você** · 00:01\nOlá a todos"));
        std::fs::write(&first, format!("{content}Anotação minha\n")).unwrap();

        // Título novo: a nota é renomeada e a anotação continua.
        sqlx::query("UPDATE meetings SET title = 'Kickoff ACME' WHERE id = 'm1'").execute(&pool).await.unwrap();
        let second = write_note(&pool, &settings, "m1").await.unwrap();
        assert_ne!(first, second);
        assert!(!first.exists());
        assert!(std::fs::read_to_string(&second).unwrap().ends_with("Anotação minha\n"));

        // Arquivo homônimo de outra origem não é sobrescrito.
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m2', 'Kickoff ACME', '2026-11-27T12:00:00Z', '2026-11-27T12:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(write_note(&pool, &settings, "m2").await.unwrap_err().contains("Já existe"));

        let missing = WorkspaceSettings { obsidian_vault: vault.join("nao-existe").to_string_lossy().to_string(), ..WorkspaceSettings::default() };
        assert!(write_note(&pool, &missing, "m1").await.is_err());
        let _ = std::fs::remove_dir_all(&vault);
    }
}
