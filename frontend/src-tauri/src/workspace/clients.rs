//! Clientes: a lista conhecida ganha apelidos ("Aurora Alimentos" = "Aurora",
//! "AAL") e domínios de e-mail. Um participante com e-mail do domínio de um
//! cliente identifica a reunião sem depender do que foi dito.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Emitter, Runtime};

use crate::organize::fold;

use super::{clean_list, db_error, pool_from, string_list};

const MAX_ALIASES: usize = 12;
const MAX_DOMAINS: usize = 8;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub domains: Vec<String>,
    pub notes: String,
    pub meetings: i64,
    /// RFC 3339 da reunião mais recente.
    pub last_meeting: Option<String>,
    pub total_seconds: f64,
    pub open_tasks: i64,
    /// Pessoas que mais aparecem nas reuniões desse cliente.
    pub people: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInput {
    pub name: String,
    pub aliases: Vec<String>,
    pub domains: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientRule {
    pub name: String,
    pub aliases: Vec<String>,
    pub domains: Vec<String>,
}

pub fn clean_domain(raw: &str) -> Option<String> {
    let domain = raw.trim().to_lowercase();
    let domain = domain.rsplit('@').next().unwrap_or("");
    let domain = domain.trim_start_matches("http://").trim_start_matches("https://").trim_start_matches("www.");
    let domain = domain.split('/').next().unwrap_or("").trim_matches('.');
    let valid = domain.contains('.')
        && domain.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && domain.len() <= 80;
    valid.then(|| domain.to_string())
}

fn clean_domains(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        if let Some(domain) = clean_domain(value) {
            if !out.contains(&domain) && out.len() < MAX_DOMAINS {
                out.push(domain);
            }
        }
    }
    out
}

/// Cliente cujo domínio é o do e-mail (ou um subdomínio dele).
pub fn client_for_domain(rules: &[ClientRule], domain: &str) -> Option<String> {
    let domain = domain.to_lowercase();
    rules
        .iter()
        .find(|rule| rule.domains.iter().any(|own| domain == *own || domain.ends_with(&format!(".{own}"))))
        .map(|rule| rule.name.clone())
}

/// Troca um apelido pelo nome oficial do cliente.
pub fn canonical_client(rules: &[ClientRule], name: &str) -> Option<String> {
    let key = fold(name.trim());
    if key.is_empty() {
        return None;
    }
    rules
        .iter()
        .find(|rule| fold(&rule.name) == key || rule.aliases.iter().any(|alias| fold(alias) == key))
        .map(|rule| rule.name.clone())
}

/// Nomes extras que o classificador deve procurar na conversa, com o cliente
/// a que cada um pertence.
pub fn alias_index(rules: &[ClientRule]) -> Vec<(String, String)> {
    rules
        .iter()
        .flat_map(|rule| rule.aliases.iter().map(move |alias| (alias.clone(), rule.name.clone())))
        .collect()
}

pub async fn load_rules(pool: &SqlitePool) -> Result<Vec<ClientRule>, sqlx::Error> {
    let rows = sqlx::query("SELECT name, aliases, domains FROM clients ORDER BY name").fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| ClientRule {
            name: row.try_get("name").unwrap_or_default(),
            aliases: string_list(&row.try_get::<String, _>("aliases").unwrap_or_default()),
            domains: string_list(&row.try_get::<String, _>("domains").unwrap_or_default()),
        })
        .collect())
}

/// Cliente indicado pelos e-mails dos participantes: o domínio com mais
/// pessoas vence.
pub async fn client_from_participants(pool: &SqlitePool, meeting_id: &str) -> Result<Option<String>, sqlx::Error> {
    let rules = load_rules(pool).await?;
    if rules.iter().all(|rule| rule.domains.is_empty()) {
        return Ok(None);
    }
    let domains: Vec<Option<String>> = sqlx::query_scalar("SELECT organization FROM meeting_participants WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
    let mut votes: Vec<(String, usize)> = Vec::new();
    for domain in domains.into_iter().flatten() {
        if let Some(client) = client_for_domain(&rules, &domain) {
            match votes.iter_mut().find(|(name, _)| *name == client) {
                Some((_, count)) => *count += 1,
                None => votes.push((client, 1)),
            }
        }
    }
    // Empate: fica o primeiro a aparecer na lista de participantes.
    let mut best: Option<(String, usize)> = None;
    for (name, count) in votes {
        if best.as_ref().map_or(true, |(_, top)| count > *top) {
            best = Some((name, count));
        }
    }
    Ok(best.map(|(name, _)| name))
}

/// Quando os participantes mudam: grava o cliente indicado pelo domínio, a
/// menos que você já tenha classificado a reunião à mão.
pub(crate) async fn apply_domain_hint<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str) {
    let Ok(Some(client)) = client_from_participants(pool, meeting_id).await else { return };
    let updated = sqlx::query(
        "UPDATE meetings SET client = ?, classification_source = CASE WHEN classification_source = 'none' THEN 'auto' ELSE classification_source END
         WHERE id = ? AND classification_source != 'manual' AND (client IS NULL OR client != ?)",
    )
    .bind(&client)
    .bind(meeting_id)
    .bind(&client)
    .execute(pool)
    .await
    .map(|result| result.rows_affected() > 0)
    .unwrap_or(false);
    if updated {
        if let Ok(Some(meeting)) = crate::organize::get_organized_meeting(pool, meeting_id).await {
            let _ = app.emit("meeting-organized", &meeting);
        }
    }
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ClientInfo>, sqlx::Error> {
    let known = crate::organize::load_organization_settings(pool).await?.known_clients;
    let mut clients: Vec<ClientInfo> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut ensure = |clients: &mut Vec<ClientInfo>, name: &str| -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let key = fold(name);
        Some(*index.entry(key).or_insert_with(|| {
            clients.push(ClientInfo {
                name: name.to_string(),
                aliases: Vec::new(),
                domains: Vec::new(),
                notes: String::new(),
                meetings: 0,
                last_meeting: None,
                total_seconds: 0.0,
                open_tasks: 0,
                people: Vec::new(),
            });
            clients.len() - 1
        }))
    };
    for name in &known {
        ensure(&mut clients, name);
    }
    let rows = sqlx::query("SELECT name, aliases, domains, notes FROM clients").fetch_all(pool).await?;
    for row in &rows {
        let name: String = row.try_get("name").unwrap_or_default();
        if let Some(at) = ensure(&mut clients, &name) {
            clients[at].aliases = string_list(&row.try_get::<String, _>("aliases").unwrap_or_default());
            clients[at].domains = string_list(&row.try_get::<String, _>("domains").unwrap_or_default());
            clients[at].notes = row.try_get("notes").unwrap_or_default();
        }
    }
    for meeting in crate::organize::list_organized_meetings(pool).await? {
        let Some(at) = meeting.client.as_deref().and_then(|name| ensure(&mut clients, name)) else { continue };
        let client = &mut clients[at];
        client.meetings += 1;
        client.total_seconds += meeting.duration_seconds.unwrap_or(0.0);
        // A lista vem da mais nova para a mais antiga.
        if client.last_meeting.is_none() {
            client.last_meeting = Some(meeting.created_at.clone());
        }
    }
    let rows = sqlx::query(
        "SELECT m.client AS client, COUNT(*) AS total FROM tasks t JOIN meetings m ON m.id = t.meeting_id
         WHERE t.status = 'open' AND m.client IS NOT NULL GROUP BY m.client",
    )
    .fetch_all(pool)
    .await?;
    for row in &rows {
        let name: String = row.try_get("client").unwrap_or_default();
        if let Some(at) = ensure(&mut clients, &name) {
            clients[at].open_tasks = row.try_get("total").unwrap_or(0);
        }
    }
    let rows = sqlx::query(
        "SELECT m.client AS client, p.name AS name, COUNT(*) AS total FROM meeting_participants p
         JOIN meetings m ON m.id = p.meeting_id WHERE m.client IS NOT NULL
         GROUP BY m.client, p.name COLLATE NOCASE ORDER BY total DESC, p.name",
    )
    .fetch_all(pool)
    .await?;
    for row in &rows {
        let client: String = row.try_get("client").unwrap_or_default();
        if let Some(at) = ensure(&mut clients, &client) {
            if clients[at].people.len() < 6 {
                clients[at].people.push(row.try_get("name").unwrap_or_default());
            }
        }
    }
    clients.sort_by(|a, b| b.last_meeting.cmp(&a.last_meeting).then_with(|| fold(&a.name).cmp(&fold(&b.name))));
    Ok(clients)
}

pub async fn save(pool: &SqlitePool, input: &ClientInput) -> Result<(), String> {
    let name: String = input.name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().take(60).collect();
    if name.is_empty() {
        return Err("O cliente precisa de um nome.".to_string());
    }
    let aliases: Vec<String> = clean_list(&input.aliases, MAX_ALIASES, 60)
        .into_iter()
        .filter(|alias| fold(alias) != fold(&name) && alias.chars().count() >= 2)
        .collect();
    let domains = clean_domains(&input.domains);
    // Um apelido ou domínio pertence a um cliente só.
    let others = load_rules(pool).await.map_err(db_error)?;
    for other in others.iter().filter(|other| fold(&other.name) != fold(&name)) {
        if let Some(alias) = aliases.iter().find(|alias| other.aliases.iter().any(|a| fold(a) == fold(alias)) || fold(&other.name) == fold(alias)) {
            return Err(format!("\"{alias}\" já identifica o cliente {}.", other.name));
        }
        if let Some(domain) = domains.iter().find(|domain| other.domains.contains(domain)) {
            return Err(format!("O domínio {domain} já é do cliente {}.", other.name));
        }
    }
    sqlx::query(
        "INSERT INTO clients (name, aliases, domains, notes) VALUES (?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET aliases = excluded.aliases, domains = excluded.domains, notes = excluded.notes",
    )
    .bind(&name)
    .bind(serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string()))
    .bind(serde_json::to_string(&domains).unwrap_or_else(|_| "[]".to_string()))
    .bind(input.notes.as_deref().unwrap_or("").trim().chars().take(600).collect::<String>())
    .execute(pool)
    .await
    .map_err(db_error)?;
    remember(pool, &name).await.map_err(db_error)
}

async fn remember(pool: &SqlitePool, name: &str) -> Result<(), sqlx::Error> {
    let mut settings = crate::organize::load_organization_settings(pool).await?;
    if !settings.known_clients.iter().any(|known| fold(known) == fold(name)) {
        settings.known_clients.push(name.to_string());
        crate::organize::save_organization_settings(pool, &settings).await?;
    }
    Ok(())
}

/// Renomeia um cliente em todas as reuniões. Se o nome novo já existe, os
/// dois viram um só (apelidos e domínios são somados).
pub async fn rename(pool: &SqlitePool, from: &str, to: &str) -> Result<i64, String> {
    let to: String = to.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
    let from = from.trim();
    if to.is_empty() || from.is_empty() {
        return Err("Informe o nome do cliente.".to_string());
    }
    let rules = load_rules(pool).await.map_err(db_error)?;
    let old = rules.iter().find(|rule| fold(&rule.name) == fold(from)).cloned();
    let target = rules.iter().find(|rule| fold(&rule.name) == fold(&to) && fold(&rule.name) != fold(from)).cloned();
    let notes = |name: &str| {
        let name = name.to_string();
        async move {
            sqlx::query_scalar::<_, String>("SELECT notes FROM clients WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .unwrap_or_default()
        }
    };
    let old_notes = notes(from).await;
    let target_notes = match &target {
        Some(target) => notes(&target.name).await,
        None => String::new(),
    };
    let mut aliases = target.as_ref().map(|rule| rule.aliases.clone()).unwrap_or_default();
    let mut domains = target.as_ref().map(|rule| rule.domains.clone()).unwrap_or_default();
    if let Some(old) = &old {
        aliases.extend(old.aliases.clone());
        domains.extend(old.domains.clone());
    }
    // O nome antigo continua identificando o cliente.
    if fold(from) != fold(&to) {
        aliases.push(from.to_string());
    }
    let aliases: Vec<String> = clean_list(&aliases, MAX_ALIASES, 60).into_iter().filter(|alias| fold(alias) != fold(&to)).collect();
    let domains = clean_domains(&domains);

    let mut tx = pool.begin().await.map_err(db_error)?;
    let changed = sqlx::query("UPDATE meetings SET client = ? WHERE client = ? COLLATE NOCASE")
        .bind(&to)
        .bind(from)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?
        .rows_affected() as i64;
    sqlx::query("DELETE FROM clients WHERE name = ? OR name = ?").bind(from).bind(&to).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO clients (name, aliases, domains, notes) VALUES (?, ?, ?, ?)")
        .bind(&to)
        .bind(serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&domains).unwrap_or_else(|_| "[]".to_string()))
        .bind(if target_notes.trim().is_empty() { old_notes } else { target_notes })
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;

    let mut settings = crate::organize::load_organization_settings(pool).await.map_err(db_error)?;
    settings.known_clients.retain(|known| fold(known) != fold(from) && fold(known) != fold(&to));
    settings.known_clients.push(to.clone());
    crate::organize::save_organization_settings(pool, &settings).await.map_err(db_error)?;
    Ok(changed)
}

/// Tira o cliente da lista conhecida. As reuniões só perdem o cliente quando
/// `clear_meetings` é pedido.
pub async fn remove(pool: &SqlitePool, name: &str, clear_meetings: bool) -> Result<(), String> {
    sqlx::query("DELETE FROM clients WHERE name = ?").bind(name).execute(pool).await.map_err(db_error)?;
    if clear_meetings {
        sqlx::query("UPDATE meetings SET client = NULL WHERE client = ? COLLATE NOCASE")
            .bind(name)
            .execute(pool)
            .await
            .map_err(db_error)?;
    }
    let mut settings = crate::organize::load_organization_settings(pool).await.map_err(db_error)?;
    settings.known_clients.retain(|known| fold(known) != fold(name));
    crate::organize::save_organization_settings(pool, &settings).await.map_err(db_error)?;
    Ok(())
}

#[tauri::command]
pub async fn clients_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<ClientInfo>, String> {
    let pool = pool_from(&app)?;
    list(&pool).await.map_err(db_error)
}

#[tauri::command]
pub async fn clients_save<R: Runtime>(app: AppHandle<R>, input: ClientInput) -> Result<Vec<ClientInfo>, String> {
    let pool = pool_from(&app)?;
    save(&pool, &input).await?;
    list(&pool).await.map_err(db_error)
}

#[tauri::command]
pub async fn clients_rename<R: Runtime>(app: AppHandle<R>, from: String, to: String) -> Result<Vec<ClientInfo>, String> {
    let pool = pool_from(&app)?;
    rename(&pool, &from, &to).await?;
    let _ = app.emit("clients-updated", ());
    list(&pool).await.map_err(db_error)
}

#[tauri::command]
pub async fn clients_remove<R: Runtime>(app: AppHandle<R>, name: String, clear_meetings: Option<bool>) -> Result<Vec<ClientInfo>, String> {
    let pool = pool_from(&app)?;
    remove(&pool, &name, clear_meetings.unwrap_or(false)).await?;
    let _ = app.emit("clients-updated", ());
    list(&pool).await.map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Vec<ClientRule> {
        vec![
            ClientRule { name: "Aurora Alimentos".into(), aliases: vec!["Aurora".into(), "AAL".into()], domains: vec!["aurora.com.br".into()] },
            ClientRule { name: "Beta".into(), aliases: vec![], domains: vec!["beta.io".into()] },
        ]
    }

    #[test]
    fn domains_and_aliases_identify_the_client() {
        let rules = rules();
        assert_eq!(client_for_domain(&rules, "aurora.com.br").as_deref(), Some("Aurora Alimentos"));
        assert_eq!(client_for_domain(&rules, "TI.Aurora.com.br").as_deref(), Some("Aurora Alimentos"));
        assert_eq!(client_for_domain(&rules, "naoaurora.com.br"), None);
        assert_eq!(canonical_client(&rules, " aal ").as_deref(), Some("Aurora Alimentos"));
        assert_eq!(canonical_client(&rules, "Gama"), None);
        assert_eq!(alias_index(&rules).len(), 2);
    }

    #[test]
    fn cleans_domains() {
        assert_eq!(clean_domain(" Ana@ACME.com.br ").as_deref(), Some("acme.com.br"));
        assert_eq!(clean_domain("https://www.acme.com/contato").as_deref(), Some("acme.com"));
        assert_eq!(clean_domain("acme"), None);
        assert_eq!(clean_domain("acme .com"), None);
    }

    async fn pool() -> SqlitePool {
        let pool = crate::workspace::test_pool().await;
        for (id, client, date) in [("m1", "Aurora", "2026-11-20T12:00:00Z"), ("m2", "aurora", "2026-11-25T12:00:00Z"), ("m3", "Beta", "2026-11-26T12:00:00Z")] {
            sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, client) VALUES (?, 'Reunião', ?, ?, ?)")
                .bind(id)
                .bind(date)
                .bind(date)
                .bind(client)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn lists_clients_with_meetings_and_merges_on_rename() {
        let pool = pool().await;
        save(&pool, &ClientInput { name: "Aurora".into(), aliases: vec!["AAL".into()], domains: vec!["@aurora.com.br".into()], notes: None }).await.unwrap();
        save(&pool, &ClientInput { name: "Aurora Alimentos".into(), aliases: vec![], domains: vec![], notes: Some("Conta nova".into()) }).await.unwrap();
        let clients = list(&pool).await.unwrap();
        assert_eq!(clients.iter().find(|c| c.name == "Aurora").unwrap().meetings, 2);
        assert_eq!(clients[0].name, "Beta"); // reunião mais recente primeiro

        // "AAL" já é da Aurora.
        let clash = save(&pool, &ClientInput { name: "Beta".into(), aliases: vec!["aal".into()], domains: vec![], notes: None }).await;
        assert!(clash.unwrap_err().contains("Aurora"));

        assert_eq!(rename(&pool, "Aurora", "Aurora Alimentos").await.unwrap(), 2);
        let clients = list(&pool).await.unwrap();
        assert_eq!(clients.len(), 2);
        let merged = clients.iter().find(|c| c.name == "Aurora Alimentos").unwrap();
        assert_eq!(merged.meetings, 2);
        assert_eq!(merged.domains, vec!["aurora.com.br".to_string()]);
        assert!(merged.aliases.contains(&"Aurora".to_string()) && merged.aliases.contains(&"AAL".to_string()));
        assert_eq!(merged.notes, "Conta nova");
        let known = crate::organize::load_organization_settings(&pool).await.unwrap().known_clients;
        assert!(known.contains(&"Aurora Alimentos".to_string()) && !known.contains(&"Aurora".to_string()));
    }

    #[tokio::test]
    async fn participant_domain_points_to_the_client() {
        let pool = pool().await;
        save(&pool, &ClientInput { name: "Beta".into(), aliases: vec![], domains: vec!["beta.io".into()], notes: None }).await.unwrap();
        let people = [
            crate::workspace::people::Person { name: "Ana".into(), email: Some("ana@beta.io".into()) },
            crate::workspace::people::Person { name: "Carlos".into(), email: Some("carlos@gmail.com".into()) },
        ];
        crate::workspace::people::add_people(&pool, "m1", &people, "teams").await.unwrap();
        assert_eq!(client_from_participants(&pool, "m1").await.unwrap().as_deref(), Some("Beta"));
        assert_eq!(client_from_participants(&pool, "m3").await.unwrap(), None);
    }
}
