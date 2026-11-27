//! Área de trabalho em volta das reuniões: tarefas que persistem entre
//! reuniões, participantes, clientes com apelidos e domínios, exportação em
//! texto e a conexão com um cofre do Obsidian.
//!
//! Tudo lê e grava no banco local. A única chamada a modelo é a extração de
//! tarefas, que usa o provedor já configurado para os resumos.

pub mod clients;
pub mod export;
pub mod people;
pub mod tasks;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

pub(crate) const EVENT_TASKS: &str = "tasks-updated";
pub(crate) const EVENT_PARTICIPANTS: &str = "participants-updated";

pub(crate) fn pool_from<R: Runtime>(app: &AppHandle<R>) -> Result<SqlitePool, String> {
    Ok(app
        .try_state::<AppState>()
        .ok_or_else(|| "Banco de dados indisponível".to_string())?
        .db_manager
        .pool()
        .clone())
}

pub(crate) fn db_error(error: sqlx::Error) -> String {
    log::error!("Workspace: database error: {}", error);
    "Não foi possível acessar o banco de dados.".to_string()
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}

pub(crate) fn emit<R: Runtime>(app: &AppHandle<R>, event: &str, meeting_id: Option<&str>) {
    let payload = serde_json::json!({ "meetingId": meeting_id });
    if let Err(error) = app.emit(event, payload) {
        log::warn!("Workspace: could not emit {}: {}", event, error);
    }
}

/// Data gravada pelo sqlx (com ou sem fuso) como UTC.
pub(crate) fn row_datetime(row: &sqlx::sqlite::SqliteRow, column: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(Some(value)) = row.try_get::<Option<chrono::DateTime<Utc>>, _>(column) {
        return Some(value);
    }
    row.try_get::<Option<chrono::NaiveDateTime>, _>(column)
        .ok()
        .flatten()
        .map(|value| chrono::DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
}

pub(crate) fn string_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// Apara, tira vazios e repetidos (sem diferenciar maiúsculas e acentos).
pub(crate) fn clean_list(values: &[String], max_items: usize, max_chars: usize) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for value in values {
        let value: String = value.split_whitespace().collect::<Vec<_>>().join(" ");
        let value: String = value.chars().take(max_chars).collect();
        if value.is_empty() {
            continue;
        }
        let key = crate::organize::fold(&value);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(value);
        if out.len() == max_items {
            break;
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    /// Como você aparece nas reuniões; usado para saber quais tarefas são suas.
    pub user_name: String,
    pub user_aliases: Vec<String>,
    pub auto_tasks: bool,
    pub auto_participants: bool,
    pub obsidian_vault: String,
    pub obsidian_folder: String,
    pub obsidian_auto: bool,
    pub obsidian_transcript: bool,
    pub obsidian_links: bool,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            user_name: String::new(),
            user_aliases: Vec::new(),
            auto_tasks: true,
            auto_participants: true,
            obsidian_vault: String::new(),
            obsidian_folder: "Reuniões".to_string(),
            obsidian_auto: false,
            obsidian_transcript: true,
            obsidian_links: true,
        }
    }
}

impl WorkspaceSettings {
    pub fn normalized(mut self) -> Self {
        self.user_name = self.user_name.trim().chars().take(60).collect();
        self.user_aliases = clean_list(&self.user_aliases, 8, 60);
        self.obsidian_vault = self.obsidian_vault.trim().to_string();
        let folder: String = self
            .obsidian_folder
            .split(['/', '\\'])
            .map(export::safe_file_name)
            .filter(|part| !part.is_empty() && part != "." && part != "..")
            .collect::<Vec<_>>()
            .join("/");
        self.obsidian_folder = folder;
        self
    }
}

pub(crate) async fn load_settings(pool: &SqlitePool) -> Result<WorkspaceSettings, sqlx::Error> {
    let row = sqlx::query(
        "SELECT user_name, user_aliases, auto_tasks, auto_participants, obsidian_vault, obsidian_folder,
                obsidian_auto, obsidian_transcript, obsidian_links
         FROM workspace_settings WHERE id = '1'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(WorkspaceSettings::default());
    };
    let flag = |name: &str, default: i64| row.try_get::<i64, _>(name).unwrap_or(default) != 0;
    Ok(WorkspaceSettings {
        user_name: row.try_get("user_name").unwrap_or_default(),
        user_aliases: string_list(&row.try_get::<String, _>("user_aliases").unwrap_or_default()),
        auto_tasks: flag("auto_tasks", 1),
        auto_participants: flag("auto_participants", 1),
        obsidian_vault: row.try_get("obsidian_vault").unwrap_or_default(),
        obsidian_folder: row.try_get("obsidian_folder").unwrap_or_else(|_| "Reuniões".to_string()),
        obsidian_auto: flag("obsidian_auto", 0),
        obsidian_transcript: flag("obsidian_transcript", 1),
        obsidian_links: flag("obsidian_links", 1),
    })
}

pub(crate) async fn save_settings(
    pool: &SqlitePool,
    settings: &WorkspaceSettings,
) -> Result<WorkspaceSettings, sqlx::Error> {
    let settings = settings.clone().normalized();
    sqlx::query(
        "INSERT INTO workspace_settings
            (id, user_name, user_aliases, auto_tasks, auto_participants, obsidian_vault, obsidian_folder,
             obsidian_auto, obsidian_transcript, obsidian_links)
         VALUES ('1', ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            user_name = excluded.user_name,
            user_aliases = excluded.user_aliases,
            auto_tasks = excluded.auto_tasks,
            auto_participants = excluded.auto_participants,
            obsidian_vault = excluded.obsidian_vault,
            obsidian_folder = excluded.obsidian_folder,
            obsidian_auto = excluded.obsidian_auto,
            obsidian_transcript = excluded.obsidian_transcript,
            obsidian_links = excluded.obsidian_links",
    )
    .bind(&settings.user_name)
    .bind(serde_json::to_string(&settings.user_aliases).unwrap_or_else(|_| "[]".to_string()))
    .bind(settings.auto_tasks as i64)
    .bind(settings.auto_participants as i64)
    .bind(&settings.obsidian_vault)
    .bind(&settings.obsidian_folder)
    .bind(settings.obsidian_auto as i64)
    .bind(settings.obsidian_transcript as i64)
    .bind(settings.obsidian_links as i64)
    .execute(pool)
    .await?;
    Ok(settings)
}

#[tauri::command]
pub async fn workspace_get_settings<R: Runtime>(app: AppHandle<R>) -> Result<WorkspaceSettings, String> {
    let pool = pool_from(&app)?;
    load_settings(&pool).await.map_err(db_error)
}

#[tauri::command]
pub async fn workspace_save_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: WorkspaceSettings,
) -> Result<WorkspaceSettings, String> {
    let pool = pool_from(&app)?;
    save_settings(&pool, &settings).await.map_err(db_error)
}

/// Títulos que o próprio app dá e que não dizem nada sobre a reunião.
pub(crate) fn is_generic_title(title: &str) -> bool {
    let title = crate::organize::fold(title.trim());
    title.is_empty()
        || title.contains("gravacao automatica")
        // "Meeting 27_11_26_14_30_00", o nome padrão de uma gravação manual.
        || title.strip_prefix("meeting ").map_or(false, |rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        || title == "nova reuniao"
        || title.contains("new call")
}

/// Chamado quando a transcrição de uma reunião acaba de ser salva.
pub fn on_meeting_saved<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    people::schedule_capture(app, meeting_id);
}

/// Chamado quando o resumo de uma reunião termina.
pub fn on_summary_completed<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    let app = app.clone();
    let meeting_id = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        tasks::run_automatic(&app, &meeting_id).await;
        export::run_automatic(&app, &meeting_id).await;
    });
}

#[cfg(test)]
pub(crate) async fn test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("memory db");
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrations");
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_cannot_leave_the_vault() {
        let settings = WorkspaceSettings {
            obsidian_folder: "../../etc/Reuniões//Clientes".to_string(),
            ..WorkspaceSettings::default()
        }
        .normalized();
        assert_eq!(settings.obsidian_folder, "etc/Reuniões/Clientes");
    }

    #[test]
    fn default_titles_are_generic_and_real_names_are_not() {
        assert!(is_generic_title("Meeting 27_11_26_14_30_00"));
        assert!(is_generic_title("Microsoft Teams (gravação automática)"));
        assert!(is_generic_title("+ New Call"));
        assert!(!is_generic_title("Reunião semanal ACME"));
        assert!(!is_generic_title("Meeting with Ana"));
    }

    #[tokio::test]
    async fn settings_round_trip() {
        let pool = test_pool().await;
        let saved = save_settings(
            &pool,
            &WorkspaceSettings {
                user_name: "  Renan  ".to_string(),
                user_aliases: vec!["Renan F".to_string(), "renan f".to_string()],
                ..WorkspaceSettings::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.user_name, "Renan");
        assert_eq!(saved.user_aliases, vec!["Renan F".to_string()]);
        assert_eq!(load_settings(&pool).await.unwrap(), saved);
    }
}
