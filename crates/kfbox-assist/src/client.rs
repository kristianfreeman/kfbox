//! The Messages API over raw HTTP (there is no official Rust SDK), and the
//! tool loop: send, run the tools the model asks for, send their results,
//! until it answers.
//!
//! Shapes follow the API's raw-HTTP documentation: `x-api-key`,
//! `anthropic-version: 2026-10-21`, tools as `name`/`description`/
//! `input_schema`, every `tool_result` for one turn in a single user message,
//! and the assistant's content sent back whole, thinking blocks included.

use std::time::Duration;

use serde_json::{json, Value};

use crate::settings::Model;

const URL: &str = "https://api.anthropic.com/v1/messages";
const VERSION: &str = "2026-10-21";
/// Server-side fallback on a refusal, which Sonnet supports and Haiku doesn't.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_TOKENS: u32 = 16_000;
/// The longest one request may take.
const TIMEOUT: Duration = Duration::from_secs(180);
/// The most a tool result may hold, so one huge answer can't fill the context.
const MAX_RESULT_CHARS: usize = 60_000;

#[derive(Debug, thiserror::Error)]
pub enum AssistError {
    #[error("no API key is saved: add one in Settings")]
    NoKey,
    #[error("the API key was refused: {0}")]
    BadKey(String),
    #[error("the Keychain: {0}")]
    Keychain(String),
    #[error("couldn't reach the API: {0}")]
    Http(String),
    #[error("the API answered {status}: {message}")]
    Api { status: u16, message: String },
    #[error("the model declined this request ({0})")]
    Refused(String),
    #[error("the answer ran past its length limit")]
    TooLong,
    #[error("the model took more than {0} steps and was stopped")]
    TooManySteps(usize),
    #[error("{0}")]
    Other(String),
}

/// Sends one request. The real one is HTTP; tests script the answers.
pub trait Transport: Send + Sync {
    /// Posts `body` with `headers`, and returns the status and the JSON answer.
    fn post(&self, headers: &[(&str, &str)], body: &Value) -> Result<(u16, Value), AssistError>;
}

/// The Messages API over HTTPS.
pub struct HttpTransport {
    agent: ureq::Agent,
}

impl Default for HttpTransport {
    fn default() -> Self {
        let config = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(TIMEOUT)).build();
        Self { agent: config.into() }
    }
}

impl Transport for HttpTransport {
    fn post(&self, headers: &[(&str, &str)], body: &Value) -> Result<(u16, Value), AssistError> {
        let mut request = self.agent.post(URL);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request.send_json(body).map_err(|e| AssistError::Http(e.to_string()))?;
        let status = response.status().as_u16();
        let answer = response.body_mut().read_json::<Value>().map_err(|e| AssistError::Http(e.to_string()))?;
        Ok((status, answer))
    }
}

/// A tool the model called, and how it went, for the person to see.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Step {
    pub tool: String,
    pub ok: bool,
    /// The error, when it failed.
    pub error: Option<String>,
}

/// What a run of the loop ended with.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The model's last words.
    pub text: String,
    pub steps: Vec<Step>,
}

/// The request's headers. The key goes here and nowhere else.
fn headers(key: &str, model: Model) -> Vec<(&str, &str)> {
    let mut headers = vec![("content-type", "application/json"), ("x-api-key", key), ("anthropic-version", VERSION)];
    if model == Model::Sonnet {
        headers.push(("anthropic-beta", FALLBACK_BETA));
    }
    headers
}

/// MCP's tool shape to the Messages API's.
#[must_use]
pub fn api_tools(tools: &[Value]) -> Vec<Value> {
    tools.iter().map(|t| json!({ "name": t["name"], "description": t["description"], "input_schema": t["inputSchema"] })).collect()
}

fn text_of(content: &[Value]) -> String {
    content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n").trim().to_owned()
}

fn clip(mut text: String) -> String {
    if text.len() > MAX_RESULT_CHARS {
        let mut end = MAX_RESULT_CHARS;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n…cut short: ask for less, such as without levels.");
    }
    text
}

/// One conversation's fixed parts.
pub struct Call<'a> {
    pub transport: &'a dyn Transport,
    pub key: &'a str,
    pub model: Model,
    pub system: &'a str,
    pub tools: &'a [Value],
    /// Requests one run may make before it's stopped.
    pub max_steps: usize,
}

/// Runs the tool loop on `messages`, which ends with the user's turn, and
/// leaves the whole exchange in it. `run_tool` answers each tool call.
pub fn run(
    call: &Call<'_>,
    messages: &mut Vec<Value>,
    run_tool: &mut dyn FnMut(&str, &Value) -> Result<Value, String>,
) -> Result<Outcome, AssistError> {
    let Call { transport, key, model, system, tools, max_steps } = *call;
    let mut steps = Vec::new();
    for _ in 0..max_steps {
        let mut body = json!({
            "model": model.id(),
            "max_tokens": MAX_TOKENS,
            "system": system,
            "tools": tools,
            "messages": messages,
        });
        if model == Model::Sonnet {
            body["fallbacks"] = json!("default");
        }
        let (status, answer) = transport.post(&headers(key, model), &body)?;
        if status != 200 {
            let message = answer["error"]["message"].as_str().unwrap_or("no message").to_owned();
            return Err(if status == 401 { AssistError::BadKey(message) } else { AssistError::Api { status, message } });
        }
        let content = answer["content"].as_array().cloned().unwrap_or_default();
        match answer["stop_reason"].as_str().unwrap_or_default() {
            "refusal" => {
                let category = answer["stop_details"]["category"].as_str().unwrap_or("no category given");
                return Err(AssistError::Refused(category.to_owned()));
            }
            "max_tokens" => return Err(AssistError::TooLong),
            "tool_use" => {
                messages.push(json!({ "role": "assistant", "content": content }));
                let mut results = Vec::new();
                for block in content.iter().filter(|b| b["type"] == "tool_use") {
                    let name = block["name"].as_str().unwrap_or_default();
                    let outcome = run_tool(name, &block["input"]);
                    steps.push(Step { tool: name.to_owned(), ok: outcome.is_ok(), error: outcome.as_ref().err().cloned() });
                    let (text, is_error) = match outcome {
                        Ok(value) => (clip(serde_json::to_string(&value).unwrap_or_default()), false),
                        Err(error) => (error, true),
                    };
                    results.push(json!({ "type": "tool_result", "tool_use_id": block["id"], "content": text, "is_error": is_error }));
                }
                messages.push(json!({ "role": "user", "content": results }));
            }
            "pause_turn" => messages.push(json!({ "role": "assistant", "content": content })),
            _ => {
                messages.push(json!({ "role": "assistant", "content": content }));
                return Ok(Outcome { text: text_of(&content), steps });
            }
        }
    }
    Err(AssistError::TooManySteps(max_steps))
}
