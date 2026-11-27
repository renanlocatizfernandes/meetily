//! Tarefas que saem das reuniões e não se perdem: cada compromisso vira um
//! item com dono, prazo e estado. "Minhas" são as que você deve fazer;
//! "aguardando retorno" são as que outra pessoa ficou de devolver.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Runtime};

use crate::organize::fold;

use super::{db_error, emit, load_settings, new_id, now, pool_from, WorkspaceSettings, EVENT_TASKS};

pub const DIRECTION_MINE: &str = "mine";
pub const DIRECTION_THEIRS: &str = "theirs";
pub const DIRECTION_UNASSIGNED: &str = "unassigned";

const MAX_TASKS: usize = 25;
const MAX_TEXT_CHARS: usize = 240;
const MAX_OWNER_CHARS: usize = 60;
const MAX_DUE_CHARS: usize = 40;
const MAX_EVIDENCE_CHARS: usize = 200;
const MIN_TRANSCRIPT_WORDS: usize = 40;
const HEAD_CHARS: usize = 16_000;
const TAIL_CHARS: usize = 8_000;
const LLM_TIMEOUT: Duration = Duration::from_secs(240);

static GENERATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub meeting_id: Option<String>,
    pub meeting_title: Option<String>,
    /// RFC 3339 da reunião de origem.
    pub meeting_date: Option<String>,
    pub client: Option<String>,
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    /// AAAA-MM-DD.
    pub due_date: Option<String>,
    pub direction: String,
    pub status: String,
    pub source: String,
    pub evidence: Option<String>,
    pub created_at: String,
    pub done_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewTask {
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub due_date: Option<String>,
    pub direction: String,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub text: String,
    pub meeting_id: Option<String>,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub direction: Option<String>,
}

/// Campo ausente não muda nada; texto vazio limpa o campo.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskPatch {
    pub text: Option<String>,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub direction: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGeneration {
    /// model | autopilot | rules | empty
    pub origin: String,
    pub added: usize,
    pub leaves_machine: bool,
    pub tasks: Vec<Task>,
}

// ---------------------------------------------------------------------------
// Regras puras
// ---------------------------------------------------------------------------

fn clamp(text: &str, max: usize) -> String {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(max).collect::<String>().trim().to_string()
}

fn optional(text: Option<&str>, max: usize) -> Option<String> {
    let text = clamp(text?, max);
    let folded = fold(&text);
    let blank = matches!(
        folded.as_str(),
        "" | "null" | "nenhum" | "nenhuma" | "n/a" | "na" | "-" | "a definir" | "indefinido" | "sem prazo"
            | "nao informado" | "desconhecido" | "ninguem"
    );
    (!blank).then_some(text)
}

/// Nomes pelos quais o dono do app aparece na conversa.
pub fn my_names(settings: &WorkspaceSettings) -> Vec<String> {
    let mut names: Vec<String> = ["voce", "eu", "mim", "me", "nos", "a gente"].iter().map(|n| n.to_string()).collect();
    let mut push = |name: &str| {
        let name = fold(name.trim());
        if name.chars().count() >= 2 && !names.contains(&name) {
            names.push(name);
        }
    };
    push(&settings.user_name);
    if let Some(first) = settings.user_name.split_whitespace().next() {
        push(first);
    }
    for alias in &settings.user_aliases {
        push(alias);
    }
    names
}

/// De quem é a tarefa. O nome do responsável decide; o tipo dito pelo modelo
/// só vale quando não há nome.
pub fn direction_for(owner: Option<&str>, kind: Option<&str>, mine: &[String]) -> &'static str {
    if let Some(owner) = owner.map(fold).filter(|owner| !owner.trim().is_empty()) {
        let owner = owner.trim();
        let is_me = mine.iter().any(|name| {
            owner == name || owner.split(|c: char| !c.is_alphanumeric()).any(|word| word == name && word.len() > 3)
        });
        return if is_me { DIRECTION_MINE } else { DIRECTION_THEIRS };
    }
    match kind.map(fold).as_deref() {
        Some("minha") | Some("mine") => DIRECTION_MINE,
        Some("deles") | Some("dele") | Some("dela") | Some("theirs") => DIRECTION_THEIRS,
        _ => DIRECTION_UNASSIGNED,
    }
}

fn weekday_from(word: &str) -> Option<Weekday> {
    Some(match word {
        "segunda" | "segunda-feira" => Weekday::Mon,
        "terca" | "terca-feira" => Weekday::Tue,
        "quarta" | "quarta-feira" => Weekday::Wed,
        "quinta" | "quinta-feira" => Weekday::Thu,
        "sexta" | "sexta-feira" => Weekday::Fri,
        "sabado" => Weekday::Sat,
        "domingo" => Weekday::Sun,
        _ => return None,
    })
}

fn last_day_of_month(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 { (date.year() + 1, 1) } else { (date.year(), date.month() + 1) };
    NaiveDate::from_ymd_opt(year, month, 1).map(|first| first - ChronoDuration::days(1)).unwrap_or(date)
}

/// Transforma "até sexta", "amanhã", "15/12" em data, a partir do dia da
/// reunião. Devolve `None` quando o texto não permite calcular.
pub fn resolve_due(text: &str, base: NaiveDate) -> Option<NaiveDate> {
    let folded = fold(text);
    let folded = folded.trim();
    if folded.is_empty() {
        return None;
    }
    if let Ok(date) = NaiveDate::parse_from_str(folded, "%Y-%m-%d") {
        return Some(date);
    }
    // dd/mm ou dd/mm/aaaa em qualquer ponto do texto.
    for token in folded.split(|c: char| !(c.is_ascii_digit() || c == '/')) {
        let parts: Vec<&str> = token.split('/').collect();
        if parts.len() < 2 || parts.len() > 3 {
            continue;
        }
        let (Ok(day), Ok(month)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) else { continue };
        let year = match parts.get(2).and_then(|y| y.parse::<i32>().ok()) {
            Some(year) if year < 100 => Some(2000 + year),
            Some(year) => Some(year),
            None => None,
        };
        if let Some(year) = year {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                return Some(date);
            }
            continue;
        }
        if let Some(date) = NaiveDate::from_ymd_opt(base.year(), month, day) {
            // Data sem ano que já passou há mais de um mês é do ano que vem.
            return Some(if date < base - ChronoDuration::days(31) {
                NaiveDate::from_ymd_opt(base.year() + 1, month, day).unwrap_or(date)
            } else {
                date
            });
        }
    }
    let words: Vec<&str> = folded.split(|c: char| !(c.is_alphanumeric() || c == '-')).filter(|w| !w.is_empty()).collect();
    let has = |needle: &str| folded.contains(needle);
    if has("depois de amanha") {
        return Some(base + ChronoDuration::days(2));
    }
    if words.contains(&"amanha") {
        return Some(base + ChronoDuration::days(1));
    }
    if words.contains(&"hoje") {
        return Some(base);
    }
    let next_week = has("semana que vem") || has("proxima semana");
    if let Some(weekday) = words.iter().find_map(|word| weekday_from(word)) {
        let mut ahead = (weekday.num_days_from_monday() + 7 - base.weekday().num_days_from_monday()) % 7;
        if ahead == 0 {
            ahead = 7;
        }
        if next_week && ahead < 7 - base.weekday().num_days_from_monday() {
            ahead += 7;
        }
        return Some(base + ChronoDuration::days(ahead as i64));
    }
    if next_week {
        return Some(base + ChronoDuration::days(7));
    }
    if has("fim do mes") || has("final do mes") {
        return Some(last_day_of_month(base));
    }
    if has("fim da semana") || has("final da semana") {
        let ahead = (Weekday::Fri.num_days_from_monday() + 7 - base.weekday().num_days_from_monday()) % 7;
        return Some(base + ChronoDuration::days(ahead as i64));
    }
    None
}

fn strip_reasoning(raw: &str) -> String {
    let mut text = raw.to_string();
    while let (Some(start), Some(end)) = (text.find("<think>"), text.find("</think>")) {
        if end < start {
            break;
        }
        text.replace_range(start..end + "</think>".len(), "");
    }
    text
}

fn json_object(raw: &str) -> Option<Value> {
    let text = strip_reasoning(raw);
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

fn field<'a>(item: &'a Value, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| item.get(*name).and_then(Value::as_str))
}

/// Lê a resposta do modelo. `Err` quando não há JSON com a lista de tarefas;
/// lista vazia é resposta válida.
pub fn parse_tasks(raw: &str, mine: &[String], base: NaiveDate) -> Result<Vec<NewTask>, String> {
    let value = json_object(raw).ok_or_else(|| "resposta sem JSON".to_string())?;
    let list = value
        .get("tarefas")
        .or_else(|| value.get("tasks"))
        .and_then(Value::as_array)
        .ok_or_else(|| "JSON sem a lista de tarefas".to_string())?;
    let mut seen = HashSet::new();
    let mut tasks = Vec::new();
    for item in list {
        let Some(text) = optional(field(item, &["texto", "text", "tarefa"]), MAX_TEXT_CHARS) else { continue };
        if text.chars().count() < 4 || !seen.insert(fold(&text)) {
            continue;
        }
        let owner = optional(field(item, &["responsavel", "responsável", "owner", "dono"]), MAX_OWNER_CHARS);
        let due = optional(field(item, &["prazo", "due"]), MAX_DUE_CHARS);
        let due_date = optional(field(item, &["data", "date"]), 12)
            .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
            .or_else(|| due.as_deref().and_then(|due| resolve_due(due, base)))
            .map(|date| date.format("%Y-%m-%d").to_string());
        let direction = direction_for(owner.as_deref(), field(item, &["tipo", "kind"]), mine).to_string();
        tasks.push(NewTask {
            text,
            owner,
            due,
            due_date,
            direction,
            evidence: optional(field(item, &["trecho", "evidence"]), MAX_EVIDENCE_CHARS),
        });
        if tasks.len() == MAX_TASKS {
            break;
        }
    }
    Ok(tasks)
}

const SYSTEM_PROMPT: &str = "Você extrai compromissos de reuniões de trabalho. Responda somente com JSON válido, sem texto antes ou depois.";
const RETRY_SUFFIX: &str = "\n\nA resposta anterior não era JSON válido. Responda apenas com o objeto JSON pedido.";

fn clamp_transcript(transcript: &str) -> String {
    let total = transcript.chars().count();
    if total <= HEAD_CHARS + TAIL_CHARS {
        return transcript.to_string();
    }
    let head: String = transcript.chars().take(HEAD_CHARS).collect();
    let tail: String = transcript.chars().skip(total - TAIL_CHARS).collect();
    format!("{head}\n[... trecho do meio omitido ...]\n{tail}")
}

pub fn build_prompt(transcript: &str, summary: Option<&str>, date: NaiveDate, participants: &[String]) -> String {
    let weekday = match date.weekday() {
        Weekday::Mon => "segunda-feira",
        Weekday::Tue => "terça-feira",
        Weekday::Wed => "quarta-feira",
        Weekday::Thu => "quinta-feira",
        Weekday::Fri => "sexta-feira",
        Weekday::Sat => "sábado",
        Weekday::Sun => "domingo",
    };
    let mut prompt = format!(
        "Liste os compromissos concretos assumidos nesta reunião: algo que alguém ficou de fazer, enviar, verificar ou responder.\n\
         Regras:\n\
         - \"Você\" na transcrição é o dono do aplicativo. Tarefa dele tem \"tipo\": \"minha\".\n\
         - Quando outra pessoa ficou de fazer ou devolver algo, use \"tipo\": \"deles\" e o nome dela em \"responsavel\", como aparece na conversa.\n\
         - Sem dono claro: \"tipo\": \"indefinida\" e \"responsavel\": null.\n\
         - \"prazo\" é o prazo como foi dito (ou null). \"data\" é o prazo em AAAA-MM-DD quando dá para calcular (ou null).\n\
         - \"trecho\" é uma citação curta da conversa que sustenta a tarefa (até 15 palavras).\n\
         - Não invente tarefas. Opinião, ideia solta e assunto discutido não são tarefa. Lista vazia é resposta válida.\n\
         - No máximo {MAX_TASKS} tarefas, cada uma começando por um verbo.\n\
         A reunião foi em {} ({weekday}).\n",
        date.format("%d/%m/%Y")
    );
    if !participants.is_empty() {
        prompt.push_str(&format!("Participantes: {}.\n", participants.join(", ")));
    }
    prompt.push_str(
        "Formato: {\"tarefas\":[{\"texto\":\"...\",\"responsavel\":\"...\",\"tipo\":\"minha|deles|indefinida\",\"prazo\":\"...\",\"data\":\"AAAA-MM-DD\",\"trecho\":\"...\"}]}\n",
    );
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        let summary: String = summary.chars().take(4_000).collect();
        prompt.push_str(&format!("\nResumo da reunião:\n{summary}\n"));
    }
    prompt.push_str(&format!("\nTranscrição:\n{}\n", clamp_transcript(transcript)));
    prompt
}

/// Sem modelo: usa as linhas de tarefa do resumo ("Ana: enviar a proposta até sexta").
pub fn tasks_from_rules(summary: &str, mine: &[String], base: NaiveDate) -> Vec<NewTask> {
    crate::assist_plus::parse_action_items(summary)
        .into_iter()
        .map(|item| {
            let owner = optional(Some(&item.who), MAX_OWNER_CHARS);
            let due = item.deadline.as_deref().and_then(|d| optional(Some(d), MAX_DUE_CHARS));
            NewTask {
                direction: direction_for(owner.as_deref(), None, mine).to_string(),
                due_date: due
                    .as_deref()
                    .and_then(|due| resolve_due(due, base))
                    .map(|date| date.format("%Y-%m-%d").to_string()),
                text: clamp(&item.what, MAX_TEXT_CHARS),
                owner,
                due,
                evidence: None,
            }
        })
        .filter(|task| task.text.chars().count() >= 4)
        .collect()
}

// ---------------------------------------------------------------------------
// Banco
// ---------------------------------------------------------------------------

const TASK_SELECT: &str = "SELECT t.id, t.meeting_id, t.text, t.owner, t.due, t.due_date, t.direction, t.status, t.source,
        t.evidence, t.created_at, t.done_at, m.title AS meeting_title, m.client AS meeting_client,
        m.created_at AS meeting_created
     FROM tasks t LEFT JOIN meetings m ON m.id = t.meeting_id";

fn task_from_row(row: &SqliteRow) -> Task {
    let text = |name: &str| -> Option<String> {
        row.try_get::<Option<String>, _>(name).ok().flatten().filter(|value| !value.trim().is_empty())
    };
    Task {
        id: row.try_get("id").unwrap_or_default(),
        meeting_id: text("meeting_id"),
        meeting_title: text("meeting_title"),
        meeting_date: super::row_datetime(row, "meeting_created").map(|date| date.to_rfc3339()),
        client: text("meeting_client"),
        text: row.try_get("text").unwrap_or_default(),
        owner: text("owner"),
        due: text("due"),
        due_date: text("due_date"),
        direction: text("direction").unwrap_or_else(|| DIRECTION_UNASSIGNED.to_string()),
        status: text("status").unwrap_or_else(|| "open".to_string()),
        source: text("source").unwrap_or_else(|| "auto".to_string()),
        evidence: text("evidence"),
        created_at: row.try_get("created_at").unwrap_or_default(),
        done_at: text("done_at"),
    }
}

pub async fn list(pool: &SqlitePool, meeting_id: Option<&str>) -> Result<Vec<Task>, sqlx::Error> {
    let order = "ORDER BY t.status = 'done', t.due_date IS NULL, t.due_date, t.created_at DESC, t.rowid";
    let rows = match meeting_id {
        Some(id) => {
            sqlx::query(&format!("{TASK_SELECT} WHERE t.meeting_id = ? {order}"))
                .bind(id)
                .fetch_all(pool)
                .await?
        }
        None => sqlx::query(&format!("{TASK_SELECT} {order}")).fetch_all(pool).await?,
    };
    Ok(rows.iter().map(task_from_row).collect())
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<Task>, sqlx::Error> {
    let row = sqlx::query(&format!("{TASK_SELECT} WHERE t.id = ?")).bind(id).fetch_optional(pool).await?;
    Ok(row.as_ref().map(task_from_row))
}

async fn insert(pool: &SqlitePool, meeting_id: Option<&str>, task: &NewTask, source: &str) -> Result<String, sqlx::Error> {
    let id = new_id("task");
    let stamp = now();
    sqlx::query(
        "INSERT INTO tasks (id, meeting_id, text, owner, due, due_date, direction, status, source, evidence, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'open', ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(meeting_id)
    .bind(&task.text)
    .bind(&task.owner)
    .bind(&task.due)
    .bind(&task.due_date)
    .bind(&task.direction)
    .bind(source)
    .bind(&task.evidence)
    .bind(&stamp)
    .bind(&stamp)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Troca as tarefas automáticas ainda intocadas da reunião pelas novas. O que
/// foi editado, concluído ou criado à mão fica, e não é duplicado.
pub async fn replace_auto(pool: &SqlitePool, meeting_id: &str, tasks: &[NewTask], origin: &str) -> Result<usize, sqlx::Error> {
    sqlx::query("DELETE FROM tasks WHERE meeting_id = ? AND source = 'auto' AND status = 'open' AND edited = 0")
        .bind(meeting_id)
        .execute(pool)
        .await?;
    let kept: Vec<String> = sqlx::query_scalar("SELECT text FROM tasks WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
    let mut seen: HashSet<String> = kept.iter().map(|text| fold(text)).collect();
    let mut added = 0;
    for task in tasks {
        if !seen.insert(fold(&task.text)) {
            continue;
        }
        insert(pool, Some(meeting_id), task, "auto").await?;
        added += 1;
    }
    sqlx::query(
        "INSERT INTO task_runs (meeting_id, origin, generated_at) VALUES (?, ?, ?)
         ON CONFLICT(meeting_id) DO UPDATE SET origin = excluded.origin, generated_at = excluded.generated_at",
    )
    .bind(meeting_id)
    .bind(origin)
    .bind(now())
    .execute(pool)
    .await?;
    Ok(added)
}

async fn already_generated(pool: &SqlitePool, meeting_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM task_runs WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some()
}

async fn autopilot_tasks(pool: &SqlitePool, meeting_id: &str, mine: &[String], base: NaiveDate) -> Vec<NewTask> {
    let raw: Option<String> = sqlx::query_scalar("SELECT tasks FROM autopilot_results WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let Some(Value::Array(items)) = raw.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()) else {
        return Vec::new();
    };
    parse_tasks(&serde_json::json!({ "tarefas": items }).to_string(), mine, base).unwrap_or_default()
}

pub async fn apply_patch(pool: &SqlitePool, id: &str, patch: &TaskPatch) -> Result<Option<Task>, String> {
    let Some(current) = get(pool, id).await.map_err(db_error)? else {
        return Ok(None);
    };
    let text = match patch.text.as_deref() {
        Some(text) => {
            let text = clamp(text, MAX_TEXT_CHARS);
            if text.is_empty() {
                return Err("A tarefa precisa de um texto.".to_string());
            }
            text
        }
        None => current.text.clone(),
    };
    let owner = match patch.owner.as_deref() {
        Some(owner) => optional(Some(owner), MAX_OWNER_CHARS),
        None => current.owner.clone(),
    };
    let due_date = match patch.due_date.as_deref() {
        Some(date) if date.trim().is_empty() => None,
        Some(date) => Some(
            NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
                .map_err(|_| "Data inválida.".to_string())?
                .format("%Y-%m-%d")
                .to_string(),
        ),
        None => current.due_date.clone(),
    };
    let direction = match patch.direction.as_deref() {
        Some(DIRECTION_MINE) => DIRECTION_MINE.to_string(),
        Some(DIRECTION_THEIRS) => DIRECTION_THEIRS.to_string(),
        Some(DIRECTION_UNASSIGNED) => DIRECTION_UNASSIGNED.to_string(),
        Some(_) => return Err("Tipo de tarefa inválido.".to_string()),
        None => current.direction.clone(),
    };
    let status = match patch.status.as_deref() {
        Some("open") => "open".to_string(),
        Some("done") => "done".to_string(),
        Some(_) => return Err("Estado inválido.".to_string()),
        None => current.status.clone(),
    };
    let done_at = if status == "done" { current.done_at.clone().or_else(|| Some(now())) } else { None };
    let content_changed = text != current.text || owner != current.owner || due_date != current.due_date || direction != current.direction;
    // Prazo falado deixa de valer quando a data é trocada à mão.
    let due = if due_date != current.due_date { None } else { current.due.clone() };
    sqlx::query(
        "UPDATE tasks SET text = ?, owner = ?, due = ?, due_date = ?, direction = ?, status = ?, done_at = ?,
            edited = CASE WHEN ? THEN 1 ELSE edited END, updated_at = ?
         WHERE id = ?",
    )
    .bind(&text)
    .bind(&owner)
    .bind(&due)
    .bind(&due_date)
    .bind(&direction)
    .bind(&status)
    .bind(&done_at)
    .bind(content_changed)
    .bind(now())
    .bind(id)
    .execute(pool)
    .await
    .map_err(db_error)?;
    get(pool, id).await.map_err(db_error)
}

// ---------------------------------------------------------------------------
// Geração
// ---------------------------------------------------------------------------

async fn generate<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str, force: bool) -> Result<TaskGeneration, String> {
    let _guard = GENERATE_LOCK.lock().await;
    let meeting = crate::organize::get_organized_meeting(pool, meeting_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "Conversa não encontrada.".to_string())?;
    let settings = load_settings(pool).await.map_err(db_error)?;
    let mine = my_names(&settings);
    let base = crate::organize::local_time(meeting.created).date();
    let transcript = crate::organize::transcript_text(pool, meeting_id).await.map_err(db_error)?;
    let summary = crate::organize::summary_markdown(pool, meeting_id).await.map_err(db_error)?;

    let mut origin = "empty";
    let mut leaves_machine = false;
    let mut tasks: Vec<NewTask> = Vec::new();
    if transcript.split_whitespace().count() >= MIN_TRANSCRIPT_WORDS {
        // O piloto automático já pode ter lido a mesma conversa: não paga outra chamada.
        if !force {
            tasks = autopilot_tasks(pool, meeting_id, &mine, base).await;
            if !tasks.is_empty() {
                origin = "autopilot";
            }
        }
        if tasks.is_empty() {
            let participants: Vec<String> = super::people::list(pool, meeting_id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|person| person.name)
                .collect();
            let prompt = build_prompt(&transcript, summary.as_deref(), base, &participants);
            let mut answered = false;
            for attempt in 0..2 {
                let user = if attempt == 0 { prompt.clone() } else { format!("{prompt}{RETRY_SUFFIX}") };
                match crate::organize::complete_text(app, pool, SYSTEM_PROMPT, &user, LLM_TIMEOUT, None).await {
                    Ok((raw, leaves)) => {
                        leaves_machine = leaves;
                        match parse_tasks(&raw, &mine, base) {
                            Ok(parsed) => {
                                tasks = parsed;
                                answered = true;
                                break;
                            }
                            // Só o motivo vai para o log, nunca o texto do modelo.
                            Err(reason) => log::warn!("Tasks: invalid model answer for {} ({})", meeting_id, reason),
                        }
                        // Provedor pago não recebe segunda chamada por resposta malformada.
                        if leaves {
                            break;
                        }
                    }
                    Err(error) => {
                        log::warn!("Tasks: model call failed for {} ({} chars)", meeting_id, error.chars().count());
                        break;
                    }
                }
            }
            if answered {
                origin = "model";
            } else if let Some(summary) = summary.as_deref() {
                tasks = tasks_from_rules(summary, &mine, base);
                origin = if tasks.is_empty() { "empty" } else { "rules" };
            }
        }
    }
    let added = replace_auto(pool, meeting_id, &tasks, origin).await.map_err(db_error)?;
    emit(app, EVENT_TASKS, Some(meeting_id));
    log::info!("Tasks: meeting {} generated {} task(s) from {}", meeting_id, added, origin);
    Ok(TaskGeneration {
        origin: origin.to_string(),
        added,
        leaves_machine,
        tasks: list(pool, Some(meeting_id)).await.map_err(db_error)?,
    })
}

/// Depois do resumo: gera as tarefas uma vez por reunião, se a opção estiver ligada.
pub(crate) async fn run_automatic<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    let Ok(pool) = pool_from(app) else { return };
    let enabled = load_settings(&pool).await.map(|settings| settings.auto_tasks).unwrap_or(false);
    if !enabled || already_generated(&pool, meeting_id).await {
        return;
    }
    if let Err(error) = generate(app, &pool, meeting_id, false).await {
        log::warn!("Tasks: automatic generation failed for {}: {}", meeting_id, error);
    }
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn tasks_list<R: Runtime>(app: AppHandle<R>, meeting_id: Option<String>) -> Result<Vec<Task>, String> {
    let pool = pool_from(&app)?;
    list(&pool, meeting_id.as_deref()).await.map_err(db_error)
}

#[tauri::command]
pub async fn tasks_generate<R: Runtime>(app: AppHandle<R>, meeting_id: String, force: Option<bool>) -> Result<TaskGeneration, String> {
    let pool = pool_from(&app)?;
    generate(&app, &pool, &meeting_id, force.unwrap_or(true)).await
}

#[tauri::command]
pub async fn tasks_add<R: Runtime>(app: AppHandle<R>, input: TaskInput) -> Result<Task, String> {
    let pool = pool_from(&app)?;
    let text = clamp(&input.text, MAX_TEXT_CHARS);
    if text.is_empty() {
        return Err("Escreva a tarefa.".to_string());
    }
    let settings = load_settings(&pool).await.map_err(db_error)?;
    let owner = optional(input.owner.as_deref(), MAX_OWNER_CHARS);
    let direction = match input.direction.as_deref() {
        Some(DIRECTION_MINE) => DIRECTION_MINE,
        Some(DIRECTION_THEIRS) => DIRECTION_THEIRS,
        Some(DIRECTION_UNASSIGNED) => DIRECTION_UNASSIGNED,
        _ if owner.is_some() => direction_for(owner.as_deref(), None, &my_names(&settings)),
        _ => DIRECTION_MINE,
    };
    let due_date = match input.due_date.as_deref().map(str::trim).filter(|date| !date.is_empty()) {
        Some(date) => Some(
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| "Data inválida.".to_string())?
                .format("%Y-%m-%d")
                .to_string(),
        ),
        None => None,
    };
    let meeting_id = input.meeting_id.as_deref().map(str::trim).filter(|id| !id.is_empty());
    let task = NewTask { text, owner, due: None, due_date, direction: direction.to_string(), evidence: None };
    let id = insert(&pool, meeting_id, &task, "manual").await.map_err(db_error)?;
    emit(&app, EVENT_TASKS, meeting_id);
    get(&pool, &id).await.map_err(db_error)?.ok_or_else(|| "Tarefa não encontrada.".to_string())
}

#[tauri::command]
pub async fn tasks_update<R: Runtime>(app: AppHandle<R>, id: String, patch: TaskPatch) -> Result<Task, String> {
    let pool = pool_from(&app)?;
    let task = apply_patch(&pool, &id, &patch).await?.ok_or_else(|| "Tarefa não encontrada.".to_string())?;
    emit(&app, EVENT_TASKS, task.meeting_id.as_deref());
    Ok(task)
}

#[tauri::command]
pub async fn tasks_delete<R: Runtime>(app: AppHandle<R>, id: String) -> Result<bool, String> {
    let pool = pool_from(&app)?;
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?").bind(&id).execute(&pool).await.map_err(db_error)?;
    emit(&app, EVENT_TASKS, None);
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn mine() -> Vec<String> {
        my_names(&WorkspaceSettings { user_name: "Renan Fernandes".to_string(), ..WorkspaceSettings::default() })
    }

    #[test]
    fn owner_decides_whose_task_it_is() {
        let mine = mine();
        assert_eq!(direction_for(Some("Você"), None, &mine), DIRECTION_MINE);
        assert_eq!(direction_for(Some("Renan"), Some("deles"), &mine), DIRECTION_MINE);
        assert_eq!(direction_for(Some("renan fernandes"), None, &mine), DIRECTION_MINE);
        assert_eq!(direction_for(Some("Ana Paula"), Some("minha"), &mine), DIRECTION_THEIRS);
        // "Eugênia" e "Meire" não são "eu" nem "me".
        assert_eq!(direction_for(Some("Eugênia"), None, &mine), DIRECTION_THEIRS);
        assert_eq!(direction_for(Some("Meire"), None, &mine), DIRECTION_THEIRS);
        assert_eq!(direction_for(None, Some("minha"), &mine), DIRECTION_MINE);
        assert_eq!(direction_for(None, Some("indefinida"), &mine), DIRECTION_UNASSIGNED);
        assert_eq!(direction_for(Some("  "), None, &mine), DIRECTION_UNASSIGNED);
    }

    #[test]
    fn spoken_deadlines_become_dates() {
        // 27/11/2026 é uma sexta-feira.
        let base = day(2026, 11, 27);
        assert_eq!(base.weekday(), Weekday::Fri);
        assert_eq!(resolve_due("amanhã", base), Some(day(2026, 11, 28)));
        assert_eq!(resolve_due("depois de amanhã", base), Some(day(2026, 11, 29)));
        assert_eq!(resolve_due("hoje à tarde", base), Some(base));
        assert_eq!(resolve_due("até terça", base), Some(day(2026, 12, 1)));
        assert_eq!(resolve_due("sexta-feira", base), Some(day(2026, 12, 4)));
        assert_eq!(resolve_due("semana que vem", base), Some(day(2026, 12, 4)));
        assert_eq!(resolve_due("até 15/12", base), Some(day(2026, 12, 15)));
        assert_eq!(resolve_due("10/01", base), Some(day(2027, 1, 10)));
        assert_eq!(resolve_due("05/03/27", base), Some(day(2027, 3, 5)));
        assert_eq!(resolve_due("2026-12-24", base), Some(day(2026, 12, 24)));
        assert_eq!(resolve_due("fim do mês", base), Some(day(2026, 11, 30)));
        assert_eq!(resolve_due("quando der", base), None);
        assert_eq!(resolve_due("31/02", base), None);
    }

    #[test]
    fn next_week_weekday_skips_this_week() {
        // Segunda-feira 23/11/2026.
        let base = day(2026, 11, 23);
        assert_eq!(resolve_due("quarta", base), Some(day(2026, 11, 25)));
        assert_eq!(resolve_due("quarta da semana que vem", base), Some(day(2026, 12, 2)));
    }

    #[test]
    fn parses_the_model_answer() {
        let raw = "<think>hmm</think>Aqui está:\n```json\n{\"tarefas\":[\
            {\"texto\":\"Enviar a proposta revisada\",\"responsavel\":\"Você\",\"tipo\":\"minha\",\"prazo\":\"até terça\",\"data\":null,\"trecho\":\"eu mando a proposta\"},\
            {\"texto\":\"Devolver o contrato assinado\",\"responsavel\":\"Ana\",\"tipo\":\"deles\",\"prazo\":null,\"data\":\"2026-12-10\"},\
            {\"texto\":\"enviar a proposta revisada\",\"responsavel\":\"Você\"},\
            {\"texto\":\"Definir o escopo\",\"responsavel\":\"a definir\",\"tipo\":\"indefinida\"},\
            {\"texto\":\"   \"}]}\n```";
        let tasks = parse_tasks(raw, &mine(), day(2026, 11, 27)).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].direction, DIRECTION_MINE);
        assert_eq!(tasks[0].due_date.as_deref(), Some("2026-12-01"));
        assert_eq!(tasks[0].evidence.as_deref(), Some("eu mando a proposta"));
        assert_eq!(tasks[1].direction, DIRECTION_THEIRS);
        assert_eq!(tasks[1].due_date.as_deref(), Some("2026-12-10"));
        assert_eq!(tasks[2].owner, None);
        assert_eq!(tasks[2].direction, DIRECTION_UNASSIGNED);
    }

    #[test]
    fn empty_list_is_valid_and_prose_is_not() {
        assert_eq!(parse_tasks("{\"tarefas\":[]}", &mine(), day(2026, 1, 1)).unwrap(), Vec::new());
        assert!(parse_tasks("Não encontrei tarefas.", &mine(), day(2026, 1, 1)).is_err());
        assert!(parse_tasks("{\"resumo\":\"x\"}", &mine(), day(2026, 1, 1)).is_err());
    }

    #[test]
    fn long_transcripts_keep_both_ends() {
        let transcript = format!("INICIO {} FIM", "palavra ".repeat(6_000));
        let prompt = build_prompt(&transcript, Some("Resumo curto"), day(2026, 11, 27), &["Ana".to_string()]);
        assert!(prompt.contains("INICIO") && prompt.contains("FIM") && prompt.contains("omitido"));
        assert!(prompt.contains("27/11/2026 (sexta-feira)"));
        assert!(prompt.contains("Participantes: Ana."));
    }

    #[test]
    fn rules_read_summary_lines() {
        let tasks = tasks_from_rules("- Ana: enviar o contrato até sexta\n- Você: revisar a proposta", &mine(), day(2026, 11, 23));
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].direction, DIRECTION_THEIRS);
        assert_eq!(tasks[0].due_date.as_deref(), Some("2026-11-27"));
        assert_eq!(tasks[1].direction, DIRECTION_MINE);
    }

    async fn pool_with_meeting() -> SqlitePool {
        let pool = crate::workspace::test_pool().await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Reunião', '2026-11-27T12:00:00Z', '2026-11-27T12:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn task(text: &str) -> NewTask {
        NewTask { text: text.to_string(), direction: DIRECTION_MINE.to_string(), ..NewTask::default() }
    }

    #[tokio::test]
    async fn regenerating_keeps_what_the_user_touched() {
        let pool = pool_with_meeting().await;
        assert_eq!(replace_auto(&pool, "m1", &[task("Enviar proposta"), task("Marcar retorno"), task("Revisar contrato")], "model").await.unwrap(), 3);
        let tasks = list(&pool, Some("m1")).await.unwrap();
        let by_text = |text: &str| tasks.iter().find(|t| t.text == text).unwrap().id.clone();
        // Uma concluída, uma editada, uma intocada.
        apply_patch(&pool, &by_text("Enviar proposta"), &TaskPatch { status: Some("done".into()), ..TaskPatch::default() }).await.unwrap();
        apply_patch(&pool, &by_text("Marcar retorno"), &TaskPatch { owner: Some("Ana".into()), ..TaskPatch::default() }).await.unwrap();

        let added = replace_auto(&pool, "m1", &[task("enviar proposta"), task("Ligar para o cliente")], "model").await.unwrap();
        assert_eq!(added, 1);
        let mut texts: Vec<String> = list(&pool, Some("m1")).await.unwrap().into_iter().map(|t| t.text).collect();
        texts.sort();
        assert_eq!(texts, vec!["Enviar proposta", "Ligar para o cliente", "Marcar retorno"]);
        assert!(already_generated(&pool, "m1").await);
    }

    #[tokio::test]
    async fn patch_validates_and_tracks_completion() {
        let pool = pool_with_meeting().await;
        replace_auto(&pool, "m1", &[task("Enviar proposta")], "model").await.unwrap();
        let id = list(&pool, None).await.unwrap()[0].id.clone();
        let done = apply_patch(&pool, &id, &TaskPatch { status: Some("done".into()), ..TaskPatch::default() }).await.unwrap().unwrap();
        assert_eq!(done.status, "done");
        assert!(done.done_at.is_some());
        assert_eq!(done.meeting_title.as_deref(), Some("Reunião"));
        let reopened = apply_patch(&pool, &id, &TaskPatch { status: Some("open".into()), due_date: Some("2026-12-01".into()), ..TaskPatch::default() }).await.unwrap().unwrap();
        assert_eq!(reopened.done_at, None);
        assert_eq!(reopened.due_date.as_deref(), Some("2026-12-01"));
        assert!(apply_patch(&pool, &id, &TaskPatch { due_date: Some("amanhã".into()), ..TaskPatch::default() }).await.is_err());
        assert!(apply_patch(&pool, &id, &TaskPatch { text: Some("  ".into()), ..TaskPatch::default() }).await.is_err());
        assert_eq!(apply_patch(&pool, "nope", &TaskPatch::default()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn autopilot_tasks_are_reused() {
        let pool = pool_with_meeting().await;
        sqlx::query("INSERT INTO autopilot_results (meeting_id, status, tasks, created_at, updated_at) VALUES ('m1', 'done', ?, 'x', 'x')")
            .bind("[{\"text\":\"Enviar ata\",\"owner\":\"Ana\",\"due\":\"amanhã\"}]")
            .execute(&pool)
            .await
            .unwrap();
        let tasks = autopilot_tasks(&pool, "m1", &mine(), day(2026, 11, 27)).await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].direction, DIRECTION_THEIRS);
        assert_eq!(tasks[0].due_date.as_deref(), Some("2026-11-28"));
    }
}
