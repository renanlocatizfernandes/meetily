//! Streaming chat call to Ollama's OpenAI-compatible endpoint.
//!
//! Same URL and body as `summary::llm_client::generate_summary` uses for
//! Ollama (`/v1/chat/completions`), plus `"stream": true`. The answer arrives
//! as server-sent events: `data: {json}` lines and a final `data: [DONE]`.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use crate::summary::llm_client::{build_openai_compat_chat_body, ollama_rejects_reasoning_effort};
use crate::summary::LLMProvider;

const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
/// The expected answer is at most six short strings. The cap stops a small
/// model that starts repeating itself from generating until the time limit.
const STREAM_MAX_TOKENS: u32 = 512;
/// Context window asked from Ollama for live assist. The prompt is capped at a
/// few thousand characters, so 4096 tokens is enough, and it keeps the model
/// from being loaded with the much larger window Ollama picks on its own
/// (16k on a 24 GB Mac), which costs several GB of memory.
pub const LIVE_ASSIST_NUM_CTX: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    Cancelled,
    /// Nothing arrived from the model within the first-token limit.
    FirstTokenTimeout,
    /// The answer started but did not finish within the total limit.
    TotalTimeout,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamTimings {
    pub first_token: Duration,
    pub total: Duration,
}

#[derive(Debug, Clone)]
pub struct StreamOutcome {
    pub content: String,
    pub timings: StreamTimings,
}

pub struct StreamLimits {
    pub first_token: Duration,
    pub total: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SseEvent {
    /// Answer text to append.
    Content(String),
    /// The model is producing output that is not answer text (reasoning).
    Activity,
    Done,
    Ignore,
}

/// Reads one line of Ollama's native `/api/chat` stream (one JSON object per line).
pub(crate) fn parse_native_line(line: &str) -> SseEvent {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
        return SseEvent::Ignore;
    };
    let text_of = |key: &str| {
        value
            .get("message")
            .and_then(|message| message.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|text| !text.is_empty())
    };
    if let Some(text) = text_of("content") {
        return SseEvent::Content(text.to_string());
    }
    if value.get("done").and_then(serde_json::Value::as_bool) == Some(true) {
        return SseEvent::Done;
    }
    if text_of("thinking").is_some() {
        SseEvent::Activity
    } else {
        SseEvent::Ignore
    }
}

/// Reads one line of either stream format: server-sent events from the
/// OpenAI-compatible endpoint or JSON lines from the native one.
pub(crate) fn parse_stream_line(line: &str) -> SseEvent {
    if line.trim_start().starts_with('{') {
        parse_native_line(line)
    } else {
        parse_sse_line(line)
    }
}

/// Body for Ollama's native chat endpoint, the only one that accepts `num_ctx`.
pub(crate) fn build_native_chat_body(model: &str, system_prompt: &str, user_prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt },
        ],
        "stream": true,
        // Reasoning models would spend seconds thinking before the first suggestion.
        "think": false,
        "options": { "num_ctx": LIVE_ASSIST_NUM_CTX, "num_predict": STREAM_MAX_TOKENS },
    })
}

/// Reads one line of the event stream.
pub(crate) fn parse_sse_line(line: &str) -> SseEvent {
    let line = line.trim();
    let Some(data) = line.strip_prefix("data:") else {
        return SseEvent::Ignore;
    };
    let data = data.trim();
    if data == "[DONE]" {
        return SseEvent::Done;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return SseEvent::Ignore;
    };
    let Some(delta) = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("delta"))
    else {
        return SseEvent::Ignore;
    };
    match delta.get("content").and_then(serde_json::Value::as_str) {
        Some(text) if !text.is_empty() => SseEvent::Content(text.to_string()),
        _ => {
            let reasoning = ["reasoning", "reasoning_content"].iter().any(|key| {
                delta
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| !text.is_empty())
            });
            if reasoning {
                SseEvent::Activity
            } else {
                SseEvent::Ignore
            }
        }
    }
}

/// Takes the complete lines out of `buffer` and leaves the unfinished tail.
pub(crate) fn drain_lines(buffer: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=position).collect();
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if !text.is_empty() {
            lines.push(text.to_string());
        }
    }
    lines
}

async fn send(
    client: &Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response, StreamError> {
    client
        .post(url)
        .header(reqwest::header::AUTHORIZATION, "Bearer ")
        .json(body)
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                StreamError::Failed("connect".to_string())
            } else {
                StreamError::Failed("request".to_string())
            }
        })
}

async fn open_stream(
    client: &Client,
    endpoint: Option<&str>,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<reqwest::Response, StreamError> {
    let host = endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_OLLAMA_ENDPOINT)
        .trim_end_matches('/');

    // Native endpoint first: it lets us cap the context window. Anything that
    // is not a plain success falls back to the OpenAI-compatible call below,
    // so an older Ollama or another compatible server keeps working.
    let native_url = format!("{host}/api/chat");
    let mut native_body = build_native_chat_body(model, system_prompt, user_prompt);
    for _ in 0..2 {
        match send(client, &native_url, &native_body).await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) if response.status().as_u16() == 400 => {
                // Models without a thinking mode can refuse the `think` field.
                let had_think = native_body.as_object_mut().and_then(|object| object.remove("think")).is_some();
                if !had_think {
                    break;
                }
            }
            Ok(_) => break,
            Err(error) => return Err(error),
        }
    }

    let url = format!("{host}/v1/chat/completions");
    let mut body = build_openai_compat_chat_body(
        &LLMProvider::Ollama,
        model,
        system_prompt,
        user_prompt,
        None,
        None,
        None,
    );
    if let Some(object) = body.as_object_mut() {
        object.insert("stream".to_string(), serde_json::Value::Bool(true));
        object.insert("max_tokens".to_string(), serde_json::Value::from(STREAM_MAX_TOKENS));
    }

    let response = send(client, &url, &body).await?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let error_body = response.text().await.unwrap_or_default();
    if !ollama_rejects_reasoning_effort(status, &error_body) {
        return Err(StreamError::Failed(format!("status {}", status.as_u16())));
    }
    // Older Ollama builds refuse `reasoning_effort`. Same retry the summary call does.
    if let Some(object) = body.as_object_mut() {
        object.remove("reasoning_effort");
    }
    let response = send(client, &url, &body).await?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(StreamError::Failed(format!("status {}", response.status().as_u16())))
    }
}

/// Runs one streaming call. `on_content` receives the whole answer text so
/// far each time it grows. The first-token limit covers connection, model
/// load and the first output of any kind; the total limit covers the call.
pub async fn stream_ollama_chat<F>(
    client: &Client,
    endpoint: Option<&str>,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    token: &CancellationToken,
    limits: StreamLimits,
    mut on_content: F,
) -> Result<StreamOutcome, StreamError>
where
    F: FnMut(&str),
{
    if token.is_cancelled() {
        return Err(StreamError::Cancelled);
    }
    let started = Instant::now();
    let first_deadline = tokio::time::Instant::now() + limits.first_token;
    let total_deadline = tokio::time::Instant::now() + limits.total;

    let response = tokio::select! {
        _ = token.cancelled() => return Err(StreamError::Cancelled),
        _ = tokio::time::sleep_until(first_deadline.min(total_deadline)) => {
            return Err(StreamError::FirstTokenTimeout);
        }
        response = open_stream(client, endpoint, model, system_prompt, user_prompt) => response?,
    };

    let mut bytes = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut content = String::new();
    let mut first_token: Option<Duration> = None;

    'read: loop {
        let deadline = if first_token.is_some() {
            total_deadline
        } else {
            first_deadline.min(total_deadline)
        };
        let chunk = tokio::select! {
            _ = token.cancelled() => return Err(StreamError::Cancelled),
            _ = tokio::time::sleep_until(deadline) => {
                return Err(if first_token.is_some() {
                    StreamError::TotalTimeout
                } else {
                    StreamError::FirstTokenTimeout
                });
            }
            chunk = bytes.next() => chunk,
        };
        let mut finished = false;
        match chunk {
            None => finished = true,
            Some(Err(_)) => return Err(StreamError::Failed("stream".to_string())),
            Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
        }
        if finished && !buffer.is_empty() {
            // A last line without the trailing newline.
            buffer.push(b'\n');
        }
        let mut grew = false;
        for line in drain_lines(&mut buffer) {
            match parse_stream_line(&line) {
                SseEvent::Content(text) => {
                    first_token.get_or_insert_with(|| started.elapsed());
                    content.push_str(&text);
                    grew = true;
                }
                SseEvent::Activity => {
                    first_token.get_or_insert_with(|| started.elapsed());
                }
                SseEvent::Done => {
                    finished = true;
                }
                SseEvent::Ignore => {}
            }
        }
        if grew {
            on_content(&content);
        }
        if finished {
            break 'read;
        }
    }

    let total = started.elapsed();
    let Some(first_token) = first_token else {
        return Err(StreamError::Failed("empty".to_string()));
    };
    Ok(StreamOutcome {
        content,
        timings: StreamTimings { first_token, total },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_native_stream_and_caps_the_context() {
        assert_eq!(
            parse_stream_line(r#"{"model":"m","message":{"role":"assistant","content":"{\"per"},"done":false}"#),
            SseEvent::Content("{\"per".to_string())
        );
        assert_eq!(
            parse_stream_line(r#"{"message":{"role":"assistant","content":"","thinking":"hm"},"done":false}"#),
            SseEvent::Activity
        );
        assert_eq!(
            parse_stream_line(r#"{"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}"#),
            SseEvent::Done
        );
        assert_eq!(parse_stream_line(r#"{"message":{"content":""},"done":false}"#), SseEvent::Ignore);
        assert_eq!(parse_stream_line("data: [DONE]"), SseEvent::Done);

        let body = build_native_chat_body("m", "sistema", "usuario");
        assert_eq!(body["options"]["num_ctx"], LIVE_ASSIST_NUM_CTX);
        assert_eq!(body["options"]["num_predict"], STREAM_MAX_TOKENS);
        assert_eq!(body["stream"], true);
        assert_eq!(body["think"], false);
        assert_eq!(body["messages"][1]["content"], "usuario");
    }

    #[test]
    fn reads_content_reasoning_done_and_noise() {
        assert_eq!(
            parse_sse_line(r#"data: {"choices":[{"delta":{"content":"{\"per"}}]}"#),
            SseEvent::Content("{\"per".to_string())
        );
        assert_eq!(
            parse_sse_line(r#"data:{"choices":[{"delta":{"role":"assistant","content":""}}]}"#),
            SseEvent::Ignore
        );
        assert_eq!(
            parse_sse_line(r#"data: {"choices":[{"delta":{"content":"","reasoning":"pensando"}}]}"#),
            SseEvent::Activity
        );
        assert_eq!(parse_sse_line("data: [DONE]"), SseEvent::Done);
        assert_eq!(parse_sse_line(": keep-alive"), SseEvent::Ignore);
        assert_eq!(parse_sse_line("data: {quecedro"), SseEvent::Ignore);
        assert_eq!(parse_sse_line(r#"data: {"choices":[]}"#), SseEvent::Ignore);
    }

    #[test]
    fn lines_split_across_chunks_are_joined() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"data: {\"a\":1}\n\ndata: {\"b\"");
        assert_eq!(drain_lines(&mut buffer), vec!["data: {\"a\":1}".to_string()]);
        assert_eq!(buffer, b"data: {\"b\"".to_vec());
        // A multi-byte character cut between two chunks stays intact.
        let text = "data: não\r\n".as_bytes();
        let cut = text.iter().position(|byte| *byte == 0xC3).unwrap() + 1;
        let mut buffer = text[..cut].to_vec();
        assert!(drain_lines(&mut buffer).is_empty());
        buffer.extend_from_slice(&text[cut..]);
        assert_eq!(drain_lines(&mut buffer), vec!["data: não".to_string()]);
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn streams_from_a_local_server_and_reports_progress() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 8192];
            let _ = socket.read(&mut request).await;
            let events = [
                "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"perguntas\\\":[\\\"Qual\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\" prazo?\\\"]}\"}}]}\n\n",
                "data: [DONE]\n\n",
            ];
            let body: String = events.concat();
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            for event in events {
                socket.write_all(event.as_bytes()).await.unwrap();
                socket.flush().await.unwrap();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });

        let mut seen = Vec::new();
        let outcome = stream_ollama_chat(
            &Client::new(),
            Some(&format!("http://{address}")),
            "modelo",
            "sistema",
            "usuario",
            &CancellationToken::new(),
            StreamLimits {
                first_token: Duration::from_secs(5),
                total: Duration::from_secs(5),
            },
            |text| seen.push(text.to_string()),
        )
        .await
        .expect("stream");
        assert_eq!(outcome.content, "{\"perguntas\":[\"Qual prazo?\"]}");
        assert!(outcome.timings.first_token <= outcome.timings.total);
        assert!(!seen.is_empty());
        assert_eq!(seen.last().unwrap(), &outcome.content);
    }

    #[tokio::test]
    async fn a_silent_server_hits_the_first_token_limit_and_cancel_wins() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let Ok((socket, _)) = listener.accept().await else { break };
                held.push(socket);
            }
        });
        let endpoint = format!("http://{address}");
        let limits = || StreamLimits {
            first_token: Duration::from_millis(150),
            total: Duration::from_secs(5),
        };
        let result = stream_ollama_chat(
            &Client::new(),
            Some(&endpoint),
            "modelo",
            "s",
            "u",
            &CancellationToken::new(),
            limits(),
            |_| {},
        )
        .await;
        assert_eq!(result.unwrap_err(), StreamError::FirstTokenTimeout);

        let token = CancellationToken::new();
        let canceller = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            canceller.cancel();
        });
        let result = stream_ollama_chat(&Client::new(), Some(&endpoint), "modelo", "s", "u", &token, limits(), |_| {}).await;
        assert_eq!(result.unwrap_err(), StreamError::Cancelled);
    }
}
