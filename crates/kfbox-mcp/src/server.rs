//! The MCP side: JSON-RPC 2.0, one message a line, and the tools.

use std::path::PathBuf;

use kfbox_core::session::{CommandError, Session};
use kfbox_core::tools::{self, Toolset};
use serde_json::{json, Value};

/// The protocol versions this server speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "kfbox reads Kristian's rekordbox library and places DJ cues by his rules. \
Track IDs come from search_tracks or playlist_tracks. propose_cues is a dry run: it shows the cues the rules would add, \
beside the ones a track has, and flags a drop it isn't sure of. write_cues and write_rule_cues change the library straight \
away, as one change each that the user sees in kfbox's History marked \"Agent\" and can undo; undo takes back the latest \
change. Writes are refused while rekordbox is open. Look at a track with propose_cues or track_detail before writing to it.";

pub struct Server {
    session: Session,
    xml: Option<PathBuf>,
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// A tool's answer, as MCP's text content.
fn tool_result(value: Result<Value, String>) -> Value {
    match value {
        Ok(value) => {
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }], "isError": false })
        }
        Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
    }
}

impl Server {
    pub fn new(session: Session, xml: Option<PathBuf>) -> Self {
        Self { session, xml }
    }

    /// Answers one line of input; `None` for a notification, which has no reply.
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return Some(error(&Value::Null, -32700, "not JSON"));
        };
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let id = id?;
        Some(match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version = VERSIONS.iter().find(|v| **v == asked).unwrap_or(&VERSIONS[0]);
                result(
                    &id,
                    &json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": "kfbox", "version": env!("CARGO_PKG_VERSION") },
                        "instructions": INSTRUCTIONS
                    }),
                )
            }
            "ping" => result(&id, &json!({})),
            "tools/list" => result(&id, &json!({ "tools": tools::definitions(Toolset::Agent) })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match self.call(name, &args) {
                    Some(answer) => result(&id, &tool_result(answer)),
                    None => error(&id, -32602, &format!("no tool {name}")),
                }
            }
            _ => error(&id, -32601, &format!("no method {method}")),
        })
    }

    fn fresh(&self) -> Result<(), CommandError> {
        match &self.xml {
            // Opened once, then read again on each call, so edits show.
            Some(path) => self.session.open_xml(path).map(|_| ()),
            None => self.session.fresh_library().map(|_| ()),
        }
    }

    /// Runs a tool on a freshly read library. `None` if there's no such tool.
    fn call(&self, name: &str, args: &Value) -> Option<Result<Value, String>> {
        if let Err(e) = self.fresh() {
            return tools::definitions(Toolset::Agent).iter().any(|t| t["name"] == name).then(|| Err(e.to_string()));
        }
        tools::call(&self.session, Toolset::Agent, name, args, "Agent")
    }
}
