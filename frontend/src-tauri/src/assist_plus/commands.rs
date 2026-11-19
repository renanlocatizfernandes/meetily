use once_cell::sync::Lazy;
use std::sync::Mutex;

use sqlx::Row;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

use super::actions::{self, ActionItem};
use super::calendar::{self, CalendarEvent};
use super::email;
use super::lookup::LookupHit;
use super::scoreboard::{self, Turn};
use super::search::{self, Doc};
use super::toggles::{self, FeatureToggles};

static CACHE: Lazy<Mutex<super::lookup::LookupCache>> = Lazy::new(|| Mutex::new(super::lookup::LookupCache::empty()));

fn pool(app: &AppHandle) -> Result<sqlx::SqlitePool, String> {
    app.try_state::<AppState>()
        .map(|state| state.db_manager.pool().clone())
        .ok_or_else(|| "Banco de dados indisponível".to_string())
}

#[tauri::command]
pub async fn get_feature_toggles(app: AppHandle) -> Result<FeatureToggles, String> {
    toggles::load(&pool(&app)?)
        .await
        .map_err(|_| "Não foi possível ler os recursos.".to_string())
}

#[tauri::command]
pub async fn save_feature_toggles(app: AppHandle, toggles: FeatureToggles) -> Result<FeatureToggles, String> {
    let pool = pool(&app)?;
    let toggles = toggles.normalized();
    toggles::save(&pool, &toggles)
        .await
        .map_err(|_| "Não foi possível salvar os recursos.".to_string())?;
    if !toggles.voice_profiles {
        crate::speakers::set_profiles(Vec::new());
    } else {
        crate::speakers::load_voice_profiles(&pool).await;
    }
    Ok(toggles)
}

#[tauri::command]
pub async fn export_action_ics(app: AppHandle, text: String) -> Result<String, String> {
    let toggles = toggles::load(&pool(&app)?).await.unwrap_or_default();
    if !toggles.auto_tasks {
        return Err("Tarefas automáticas estão desligadas.".to_string());
    }
    let items = actions::parse_action_items(&text);
    if items.is_empty() {
        return Err("Nenhuma tarefa encontrada na transcrição.".to_string());
    }
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let ics = actions::to_ics(&items, &stamp);
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Não foi possível achar a pasta do app.".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|_| "Não foi possível criar a pasta.".to_string())?;
    let path = dir.join(format!("tarefas-{stamp}.ics"));
    std::fs::write(&path, ics).map_err(|_| "Não foi possível gravar o .ics.".to_string())?;
    open_path(&path);
    Ok(path.to_string_lossy().to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
    pub title: String,
    pub when: String,
    pub excerpt: String,
    pub pending: Vec<ActionItem>,
}

#[tauri::command]
pub async fn recall_meetings(app: AppHandle, hint: String) -> Result<Vec<MemoryHit>, String> {
    let pool = pool(&app)?;
    let toggles = toggles::load(&pool).await.unwrap_or_default();
    if !toggles.meeting_memory {
        return Ok(Vec::new());
    }
    let docs = load_docs(&pool, &hint).await.map_err(|_| "Não foi possível ler reuniões antigas.".to_string())?;
    Ok(search::rank_snippets(&hint, &docs)
        .into_iter()
        .map(|snippet| MemoryHit {
            pending: actions::parse_action_items(&snippet.excerpt),
            title: snippet.title,
            when: snippet.when,
            excerpt: snippet.excerpt,
        })
        .collect())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedContext {
    pub context: String,
    pub note: String,
}

#[tauri::command]
pub async fn prepare_from_calendar(app: AppHandle) -> Result<PreparedContext, String> {
    let toggles = toggles::load(&pool(&app)?).await.unwrap_or_default();
    if !toggles.calendar_prep {
        return Err("Preparação pela agenda está desligada.".to_string());
    }
    match calendar::upcoming_on_macos() {
        Ok(events) if !events.is_empty() => Ok(PreparedContext {
            context: calendar::prefill_context(&events),
            note: "Lido do Calendário neste Mac. Avise as pessoas se a reunião for gravada.".to_string(),
        }),
        Ok(_) => Err("Não há compromisso nas próximas 24 horas.".to_string()),
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub async fn prepare_from_ics(app: AppHandle, ics: String) -> Result<PreparedContext, String> {
    let toggles = toggles::load(&pool(&app)?).await.unwrap_or_default();
    if !toggles.calendar_prep {
        return Err("Preparação pela agenda está desligada.".to_string());
    }
    let events: Vec<CalendarEvent> = calendar::parse_ics(&ics);
    let context = calendar::prefill_context(&events);
    if context.is_empty() {
        return Err("O arquivo não tem um compromisso.".to_string());
    }
    Ok(PreparedContext {
        context,
        note: "Contexto preenchido a partir do .ics. Nada foi enviado.".to_string(),
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAnswer {
    pub snippets: Vec<search::Snippet>,
    pub answer: String,
    pub leaves_machine: bool,
}

#[tauri::command]
pub async fn search_meetings(app: AppHandle, question: String) -> Result<SearchAnswer, String> {
    let pool = pool(&app)?;
    let toggles = toggles::load(&pool).await.unwrap_or_default();
    if !toggles.meeting_search {
        return Err("A busca nas reuniões está desligada.".to_string());
    }
    let docs = load_docs(&pool, &question).await.map_err(|_| "Não foi possível buscar.".to_string())?;
    let snippets = search::rank_snippets(&question, &docs);
    if snippets.is_empty() {
        return Ok(SearchAnswer {
            snippets,
            answer: "Não encontrei isso nas reuniões salvas.".to_string(),
            leaves_machine: false,
        });
    }
    let excerpts = snippets
        .iter()
        .map(|snippet| format!("[{} {}] {}", snippet.when, snippet.title, snippet.excerpt))
        .collect::<Vec<_>>()
        .join("\n");
    let (answer, leaves) = answer_locally(&app, &pool, &question, &excerpts).await;
    Ok(SearchAnswer {
        snippets,
        answer,
        leaves_machine: leaves,
    })
}

#[tauri::command]
pub async fn meeting_scoreboard(app: AppHandle, turns: Vec<TurnInput>, agenda: String) -> Result<scoreboard::Scoreboard, String> {
    let toggles = toggles::load(&pool(&app)?).await.unwrap_or_default();
    if !toggles.scoreboard {
        return Err("O placar está desligado.".to_string());
    }
    let turns = turns
        .into_iter()
        .map(|turn| Turn {
            speaker: turn.speaker,
            seconds: turn.seconds,
            text: turn.text,
            at: turn.at,
        })
        .collect::<Vec<_>>();
    Ok(scoreboard::score(&turns, &agenda))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnInput {
    pub speaker: String,
    pub seconds: f64,
    pub text: String,
    pub at: f64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub subject: String,
    pub body: String,
    pub mailto: String,
}

#[tauri::command]
pub async fn draft_followup(app: AppHandle, title: String, summary: String) -> Result<Draft, String> {
    let toggles = toggles::load(&pool(&app)?).await.unwrap_or_default();
    if !toggles.followup_email {
        return Err("O rascunho de e-mail está desligado.".to_string());
    }
    let draft = email::draft_followup(&title, &summary);
    let mailto = email::mailto(&draft);
    Ok(Draft {
        subject: draft.subject,
        body: draft.body,
        mailto,
    })
}

pub async fn cached_or_fetch(pool: &sqlx::SqlitePool, term: &str) -> Option<LookupHit> {
    if let Ok(guard) = CACHE.lock() {
        if let Some(hit) = guard.get(term) {
            return hit;
        }
    }
    if let Some(hit) = read_db_cache(pool, term).await {
        if let Ok(mut guard) = CACHE.lock() {
            guard.insert(term, Some(hit.clone()));
        }
        return Some(hit);
    }
    let client = reqwest::Client::new();
    let hit = super::lookup::fetch_term(&client, term).await;
    if let Ok(mut guard) = CACHE.lock() {
        guard.insert(term, hit.clone());
    }
    if let Some(hit) = &hit {
        let _ = sqlx::query(
            "INSERT INTO lookup_cache (term, extract, url, source) VALUES (?, ?, ?, ?)
             ON CONFLICT(term) DO UPDATE SET extract = excluded.extract, url = excluded.url, source = excluded.source",
        )
        .bind(term.trim().to_lowercase())
        .bind(&hit.extract)
        .bind(&hit.url)
        .bind(&hit.source)
        .execute(pool)
        .await;
    }
    hit
}

async fn read_db_cache(pool: &sqlx::SqlitePool, term: &str) -> Option<LookupHit> {
    let row = sqlx::query("SELECT extract, url, source FROM lookup_cache WHERE term = ?")
        .bind(term.trim().to_lowercase())
        .fetch_optional(pool)
        .await
        .ok()??;
    Some(LookupHit {
        term: term.to_string(),
        extract: row.try_get("extract").ok()?,
        url: row.try_get("url").ok()?,
        source: row.try_get("source").ok()?,
    })
}

async fn load_docs(pool: &sqlx::SqlitePool, hint: &str) -> Result<Vec<Doc>, sqlx::Error> {
    let tokens = search::tokens(hint);
    let like = tokens.first().map(|token| format!("%{token}%")).unwrap_or_else(|| "%".to_string());
    let rows = sqlx::query(
        "SELECT m.id, m.title, m.created_at, t.transcript
         FROM meetings m
         JOIN transcripts t ON t.meeting_id = m.id
         WHERE lower(m.title) LIKE lower(?) OR lower(t.transcript) LIKE lower(?)
         ORDER BY m.created_at DESC
         LIMIT 200",
    )
    .bind(&like)
    .bind(&like)
    .fetch_all(pool)
    .await?;
    let mut docs: Vec<Doc> = Vec::new();
    for row in rows {
        let id: String = row.try_get("id")?;
        let text: String = row.try_get("transcript")?;
        if let Some(doc) = docs.iter_mut().find(|doc| doc.id == id) {
            doc.text.push('\n');
            doc.text.push_str(&text);
            continue;
        }
        docs.push(Doc {
            id,
            title: row.try_get("title")?,
            when: row.try_get::<String, _>("created_at").unwrap_or_default(),
            text,
        });
    }
    Ok(docs)
}

async fn answer_locally(app: &AppHandle, pool: &sqlx::SqlitePool, question: &str, excerpts: &str) -> (String, bool) {
    let hint = crate::live_assist::load_summary_hint(pool).await;
    let settings = crate::live_assist::settings_store::load_settings(pool).await.ok();
    let model = settings.as_ref().map(|settings| settings.model.as_str()).unwrap_or("llama3.2:latest");
    let Ok(target) = crate::live_assist::resolve_target(&hint, model) else {
        return ("Não consegui consultar o modelo. Os trechos acima são a resposta.".to_string(), false);
    };
    let client = reqwest::Client::new();
    let prompt = format!(
        "Pergunta: {question}\n\nTrechos salvos, com data e título:\n{excerpts}\n\nResponda em pt-BR, em até 40 palavras, só com o que está nos trechos. Cite a data. Se não estiver nos trechos, diga que não encontrou."
    );
    let app_data = app.path().app_data_dir().ok();
    match crate::summary::llm_client::generate_summary(
        &client,
        &target.provider,
        &target.model,
        &target.api_key,
        "Você responde só com base nos trechos fornecidos.",
        &prompt,
        target.ollama_endpoint.as_deref(),
        target.custom_openai_endpoint.as_deref(),
        target.max_tokens,
        target.temperature,
        target.top_p,
        app_data.as_ref(),
        None,
    )
    .await
    {
        Ok(completion) => (completion.content.trim().chars().take(500).collect(), target.leaves_machine),
        Err(_) => ("O modelo não respondeu. Use os trechos citados.".to_string(), false),
    }
}

fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn();
}
