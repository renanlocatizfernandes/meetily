//! Minimal client for the System One API (TypeSafe's Jev and compatible servers).
//!
//! Contract (docs.typesafe.ai/api): `POST {base}/v1/systemone` with
//! `{model, state, questions}` returns typed `answers` with probabilities;
//! `GET {base}/v1/models` lists the model names. Only input tokens are billed.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const USER_AGENT: &str = concat!("MeetilyLivePlus/", env!("CARGO_PKG_VERSION"));

/// One question for the model. Images (`box`) are left out on purpose:
/// meeting features only send text.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// Yes/no; the answer is the probability of yes.
    Noul { instructions: String },
    /// Picks one of 1 to 255 named options (name -> description).
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    /// Ordered levels (1 to 10); the answer is a probability-weighted index.
    Score { instructions: String, criteria: Vec<String> },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DecideRequest {
    pub model: String,
    pub state: Value,
    pub questions: BTreeMap<String, Question>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
    },
    Score {
        score: f64,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
    },
    /// Types this client does not ask for (e.g. `box`).
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DecideResponse {
    #[serde(default)]
    pub model: Option<String>,
    pub answers: BTreeMap<String, Answer>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Failures, each with a message the settings card can show as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JevError {
    Disabled,
    MissingKey,
    InvalidSettings(String),
    Unauthorized,
    PaymentRequired,
    Forbidden,
    NotFound,
    RateLimited,
    Rejected(String),
    Server(u16),
    Timeout,
    Network,
    BadResponse,
}

impl JevError {
    pub fn user_message(&self) -> String {
        match self {
            JevError::Disabled => "A conexão com o Jev está desligada.".into(),
            JevError::MissingKey => "Informe a chave da API do Jev antes de testar.".into(),
            JevError::InvalidSettings(msg) => msg.clone(),
            JevError::Unauthorized => "A chave não foi aceita (401). Confira a chave cadastrada.".into(),
            JevError::PaymentRequired => "Sem saldo na conta (402). Adicione créditos no provedor.".into(),
            JevError::Forbidden => "A conta não tem permissão para esta chamada (403).".into(),
            JevError::NotFound => "O endereço respondeu, mas não tem a API System One (404). Confira a URL.".into(),
            JevError::RateLimited => "Limite de uso atingido (429). Aguarde um pouco e tente de novo.".into(),
            JevError::Rejected(detail) => format!("O serviço recusou a chamada: {detail}"),
            JevError::Server(status) => format!("O serviço está com problema ({status}). Tente mais tarde."),
            JevError::Timeout => "O serviço demorou para responder.".into(),
            JevError::Network => "Não foi possível conectar. Confira a internet e o endereço.".into(),
            JevError::BadResponse => "O serviço respondeu num formato inesperado.".into(),
        }
    }
}

/// Maps an HTTP error to a `JevError`. `body` is only used for the short
/// validation detail of 400/422 (never contains our request content).
pub fn error_for_status(status: u16, body: &str) -> JevError {
    match status {
        401 => JevError::Unauthorized,
        402 => JevError::PaymentRequired,
        403 => JevError::Forbidden,
        404 => JevError::NotFound,
        429 => JevError::RateLimited,
        400 | 422 => JevError::Rejected(validation_detail(body)),
        s => JevError::Server(s),
    }
}

fn validation_detail(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let text = match parsed.as_ref().and_then(|v| v.get("detail")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let msg = item.get("msg")?.as_str()?;
                let loc = item
                    .get("loc")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(|p| p.as_str().map(str::to_string).or_else(|| p.as_u64().map(|n| n.to_string()))).collect::<Vec<_>>().join("."))
                    .unwrap_or_default();
                Some(if loc.is_empty() { msg.to_string() } else { format!("{loc}: {msg}") })
            })
            .collect::<Vec<_>>()
            .join("; "),
        Some(other) => other.to_string(),
        None => "pedido inválido".to_string(),
    };
    text.chars().take(200).collect()
}

/// Accepts the OpenAI-style `{"data":[{"id":…}]}`, `{"models":[…]}` or a bare
/// array of names/objects, since compatible servers differ.
pub fn parse_model_ids(value: &Value) -> Vec<String> {
    let list = value
        .get("data")
        .or_else(|| value.get("models"))
        .unwrap_or(value)
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|item| match item {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o
                .get("id")
                .or_else(|| o.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        })
        .collect()
}

pub struct JevClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl JevClient {
    /// `base_url` must already be normalized (see `settings::normalize_base_url`).
    pub fn new(base_url: &str, api_key: Option<String>, timeout: Duration) -> Result<Self, JevError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|_| JevError::Network)?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.filter(|k| !k.is_empty()),
        })
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }

    /// `GET /v1/models`. Free of charge.
    pub async fn list_models(&self) -> Result<Vec<String>, JevError> {
        let url = format!("{}/v1/models", self.base_url);
        let response = self.authorized(self.http.get(url)).send().await.map_err(transport_error)?;
        let value: Value = read_json(response).await?;
        Ok(parse_model_ids(&value))
    }

    /// `POST /v1/systemone`. Billed by input tokens.
    pub async fn decide(&self, request: &DecideRequest) -> Result<DecideResponse, JevError> {
        if request.questions.is_empty() {
            return Err(JevError::InvalidSettings("Nenhuma pergunta para o Jev.".into()));
        }
        let url = format!("{}/v1/systemone", self.base_url);
        let response = self
            .authorized(self.http.post(url))
            .json(request)
            .send()
            .await
            .map_err(transport_error)?;
        let parsed: DecideResponse = read_json(response).await?;
        Ok(parsed)
    }
}

fn transport_error(error: reqwest::Error) -> JevError {
    if error.is_timeout() {
        JevError::Timeout
    } else {
        JevError::Network
    }
}

async fn read_json<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T, JevError> {
    let status = response.status();
    let body = response.text().await.map_err(transport_error)?;
    if !status.is_success() {
        return Err(error_for_status(status.as_u16(), &body));
    }
    serde_json::from_str(&body).map_err(|_| JevError::BadResponse)
}

/// The cheapest possible call that proves key, model and contract work:
/// a one-sentence state and one yes/no question (a few dozen input tokens).
pub fn ping_request(model: &str) -> DecideRequest {
    let mut questions = BTreeMap::new();
    questions.insert(
        "ping".to_string(),
        Question::Noul {
            instructions: "Is this text a connection test?".to_string(),
        },
    );
    DecideRequest {
        model: model.to_string(),
        state: Value::String("Connection test from Meetily Live Plus.".to_string()),
        questions,
    }
}

/// True when the ping answer has the expected shape.
pub fn ping_ok(response: &DecideResponse) -> bool {
    matches!(response.answers.get("ping"), Some(Answer::Noul { noul }) if (0.0..=1.0).contains(noul))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[test]
    fn jev_request_matches_the_documented_shape() {
        let mut questions = BTreeMap::new();
        questions.insert("billing".into(), Question::Noul { instructions: "Is this about billing?".into() });
        questions.insert(
            "tone".into(),
            Question::Choice {
                instructions: "Tone?".into(),
                criteria: BTreeMap::from([("calm".to_string(), "Polite".to_string())]),
            },
        );
        questions.insert(
            "urgency".into(),
            Question::Score { instructions: "Urgency?".into(), criteria: vec!["Can wait".into(), "Today".into()] },
        );
        let request = DecideRequest { model: "jev-latest".into(), state: json!("text"), questions };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["model"], "jev-latest");
        assert_eq!(value["questions"]["billing"], json!({"type":"noul","instructions":"Is this about billing?"}));
        assert_eq!(value["questions"]["tone"]["type"], "choice");
        assert_eq!(value["questions"]["tone"]["criteria"]["calm"], "Polite");
        assert_eq!(value["questions"]["urgency"]["criteria"][1], "Today");
    }

    #[test]
    fn jev_parses_the_documented_response() {
        let body = json!({
            "model": "jev-latest",
            "answers": {
                "billing": {"type": "noul", "noul": 0.98},
                "tone": {"type": "choice", "choice": "calm", "confidence": 0.7, "probabilities": {"angry": 0.15, "calm": 0.8}},
                "urgency": {"type": "score", "score": 1.7, "confidence": 0.55, "legend": {"0": "Can wait"}, "probabilities": {"0": 0.1}},
                "where": {"type": "box", "box": {"top": 0.1, "bottom": 0.2, "left": 0.1, "right": 0.3}, "confidence": null}
            },
            "usage": {"input_tokens": 120, "output_tokens": 12}
        });
        let parsed: DecideResponse = serde_json::from_value(body).unwrap();
        assert_eq!(parsed.answers["billing"], Answer::Noul { noul: 0.98 });
        assert!(matches!(&parsed.answers["tone"], Answer::Choice { choice, .. } if choice == "calm"));
        assert!(matches!(parsed.answers["urgency"], Answer::Score { score, .. } if (score - 1.7).abs() < 1e-9));
        assert_eq!(parsed.answers["where"], Answer::Other);
        assert_eq!(parsed.usage.unwrap().input_tokens, 120);
    }

    #[test]
    fn jev_status_codes_map_to_messages() {
        assert_eq!(error_for_status(401, ""), JevError::Unauthorized);
        assert_eq!(error_for_status(402, ""), JevError::PaymentRequired);
        assert_eq!(error_for_status(403, ""), JevError::Forbidden);
        assert_eq!(error_for_status(404, ""), JevError::NotFound);
        assert_eq!(error_for_status(429, ""), JevError::RateLimited);
        assert_eq!(error_for_status(503, ""), JevError::Server(503));
        assert_eq!(error_for_status(400, r#"{"detail":"unknown model"}"#), JevError::Rejected("unknown model".into()));
        assert_eq!(
            error_for_status(422, r#"{"detail":[{"loc":["body","state"],"msg":"Field required","type":"missing"}]}"#),
            JevError::Rejected("body.state: Field required".into())
        );
        assert_eq!(error_for_status(422, "not json"), JevError::Rejected("pedido inválido".into()));
        assert!(JevError::Unauthorized.user_message().contains("401"));
    }

    #[test]
    fn jev_model_lists_in_several_shapes() {
        assert_eq!(parse_model_ids(&json!({"data":[{"id":"jev-latest"},{"id":"jev-preview"}]})), vec!["jev-latest", "jev-preview"]);
        assert_eq!(parse_model_ids(&json!({"models":[{"name":"kev-9b"}]})), vec!["kev-9b"]);
        assert_eq!(parse_model_ids(&json!(["jev-1.14.0"])), vec!["jev-1.14.0"]);
        assert!(parse_model_ids(&json!({"unexpected": true})).is_empty());
    }

    #[test]
    fn jev_ping_is_tiny_and_checked() {
        let request = ping_request("jev-latest");
        assert_eq!(request.questions.len(), 1);
        assert!(serde_json::to_string(&request).unwrap().len() < 300);
        let ok: DecideResponse = serde_json::from_value(json!({"answers":{"ping":{"type":"noul","noul":0.9}}})).unwrap();
        assert!(ping_ok(&ok));
        let wrong: DecideResponse = serde_json::from_value(json!({"answers":{"other":{"type":"noul","noul":0.9}}})).unwrap();
        assert!(!ping_ok(&wrong));
    }

    async fn mock_server(status: u16, body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        mock::serve(vec![(status, body)]).await
    }

    /// The server records the request before answering, so it is there once the call returns.
    async fn wait_for(seen: &Arc<Mutex<Vec<String>>>) -> String {
        seen.lock().unwrap().first().cloned().expect("mock server saw no request")
    }

    fn client(base: &str, key: Option<&str>) -> JevClient {
        JevClient::new(base, key.map(str::to_string), Duration::from_secs(5)).unwrap()
    }

    #[tokio::test]
    async fn jev_lists_models_with_bearer_key() {
        let (base, seen) = mock_server(200, r#"{"data":[{"id":"jev-latest"}]}"#).await;
        let models = client(&base, Some("test-key")).list_models().await.unwrap();
        assert_eq!(models, vec!["jev-latest"]);
        let request = wait_for(&seen).await;
        assert!(request.starts_with("GET /v1/models "));
        assert!(request.to_ascii_lowercase().contains("authorization: bearer test-key"));
    }

    #[tokio::test]
    async fn jev_without_key_sends_no_authorization() {
        let (base, seen) = mock_server(200, "[]").await;
        client(&base, None).list_models().await.unwrap();
        assert!(!wait_for(&seen).await.to_ascii_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn jev_rejected_key_is_reported() {
        let (base, _) = mock_server(401, r#"{"detail":"invalid key"}"#).await;
        assert_eq!(client(&base, Some("bad")).list_models().await, Err(JevError::Unauthorized));
    }

    #[tokio::test]
    async fn jev_decide_posts_questions_and_reads_answers() {
        let (base, seen) = mock_server(200, r#"{"model":"jev-latest","answers":{"ping":{"type":"noul","noul":0.97}},"usage":{"input_tokens":21,"output_tokens":1}}"#).await;
        let response = client(&base, Some("k")).decide(&ping_request("jev-latest")).await.unwrap();
        assert!(ping_ok(&response));
        assert_eq!(response.usage.unwrap().input_tokens, 21);
        let request = wait_for(&seen).await;
        assert!(request.starts_with("POST /v1/systemone "));
        assert!(request.contains(r#""model":"jev-latest""#));
        assert!(request.contains(r#""type":"noul""#));
    }

    #[tokio::test]
    async fn jev_unexpected_body_is_a_bad_response() {
        let (base, _) = mock_server(200, "<html>proxy</html>").await;
        assert_eq!(client(&base, None).decide(&ping_request("m")).await, Err(JevError::BadResponse));
    }

    #[tokio::test]
    async fn jev_unreachable_server_is_a_network_error() {
        // Bind and drop to get a port nobody listens on.
        let port = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();
        let result = client(&format!("http://127.0.0.1:{port}"), None).list_models().await;
        assert_eq!(result, Err(JevError::Network));
    }
}

/// Tiny HTTP server for tests: answers each connection with the next canned
/// response and records the raw requests. No external dependency.
#[cfg(test)]
pub(crate) mod mock {
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub(crate) async fn serve(responses: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_task = seen.clone();
        tokio::spawn(async move {
            for (status, body) in responses {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let request = read_request(&mut socket).await;
                seen_task.lock().unwrap().push(request);
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    /// Reads headers plus `Content-Length` bytes of body.
    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(head_end) = text.find("\r\n\r\n") {
                let length = text[..head_end]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                if buf.len() >= head_end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&buf).to_string()
    }
}
