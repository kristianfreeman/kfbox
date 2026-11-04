//! The MCP side: JSON-RPC 2.0, one message a line, and the tools.

use std::path::PathBuf;

use kfbox_core::session::{CommandError, Session};
use serde_json::{json, Value};

/// The protocol versions this server speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 500;

const INSTRUCTIONS: &str = "kfbox reads Kristian's rekordbox library and places DJ cues by his rules. \
These tools only read and propose: none changes the library. Track IDs come from search_tracks or playlist_tracks. \
propose_cues shows the cues the rules would add, beside the ones a track has; existing cues are never moved.";

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

fn tools() -> Value {
    json!([
        {
            "name": "list_playlists",
            "description": "The library's size and its playlists and folders, with the number of tracks in each.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "search_tracks",
            "description": "Tracks whose title, artist, album, genre or key contain every word of the query. An empty query lists tracks in artist order. Each result has its ID, BPM, key, length and existing hot cue letters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to match, any case." },
                    "playlist": { "type": "string", "description": "Only search this playlist, by name or by its path like \"Sets › Warm up\"." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT }
                },
                "required": ["query"]
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "playlist_tracks",
            "description": "A playlist's tracks, in its order.",
            "inputSchema": {
                "type": "object",
                "properties": { "playlist": { "type": "string", "description": "Its name, or its path like \"Sets › Warm up\"." } },
                "required": ["playlist"]
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "track_detail",
            "description": "One track: what the collection says, its beat grid, every cue it has with its bar, and optionally each bar's volume, bass and busyness (0 to 1), which is what the cue rules read. The first look at a track analyses its audio, which takes a few seconds; after that it's cached.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The track's ID." },
                    "levels": { "type": "boolean", "default": false, "description": "Include every bar's levels." }
                },
                "required": ["id"]
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "propose_cues",
            "description": "A dry run: the cues kfbox's rules would place on a track, beside the cues it already has, which are never moved. Uses the saved rules unless rules are given. Nothing is written.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The track's ID." },
                    "rules": { "type": "string", "description": "A rules file in kfbox's TOML format, to try instead of the saved one." }
                },
                "required": ["id"]
            },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "export_xml",
            "description": "Writes tracks with the saved rules' cues to a new rekordbox XML file, for rekordbox's XML import. Never touches the library itself. Give track IDs, a playlist, or both.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "An absolute path ending in .xml, outside rekordbox's own folders." },
                    "ids": { "type": "array", "items": { "type": "string" } },
                    "playlist": { "type": "string" },
                    "overwrite": { "type": "boolean", "default": false }
                },
                "required": ["path"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false }
        }
    ])
}

fn text_arg<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

fn required<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    text_arg(args, name).ok_or_else(|| format!("{name} is required"))
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
            "tools/list" => result(&id, &json!({ "tools": tools() })),
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

    fn playlist(&self, name: &str) -> Result<usize, CommandError> {
        // An index works too, as list_playlists gives them.
        name.parse::<usize>()
            .ok()
            .filter(|i| self.session.playlist_rows(Some(*i)).is_ok())
            .map_or_else(|| self.session.find_playlist(name), Ok)
    }

    /// Runs a tool. `None` if there's no such tool.
    fn call(&self, name: &str, args: &Value) -> Option<Result<Value, String>> {
        let known = ["list_playlists", "search_tracks", "playlist_tracks", "track_detail", "propose_cues", "export_xml"];
        if !known.contains(&name) {
            return None;
        }
        let run = || -> Result<Value, String> {
            self.fresh().map_err(|e| e.to_string())?;
            let json = |value: Result<Value, serde_json::Error>| value.map_err(|e| e.to_string());
            let fail = |e: CommandError| e.to_string();
            match name {
                "list_playlists" => json(serde_json::to_value(self.session.fresh_listing().map_err(fail)?)),
                "search_tracks" => {
                    let query = required(args, "query")?;
                    #[allow(clippy::cast_possible_truncation, reason = "clamped to MAX_LIMIT")]
                    let limit = args.get("limit").and_then(Value::as_u64).map_or(DEFAULT_LIMIT, |n| (n as usize).clamp(1, MAX_LIMIT));
                    let playlist = text_arg(args, "playlist").map(|name| self.playlist(name)).transpose().map_err(fail)?;
                    let rows = self.session.search_in(playlist, query, limit).map_err(fail)?;
                    json(serde_json::to_value(rows))
                }
                "playlist_tracks" => {
                    let index = self.playlist(required(args, "playlist")?).map_err(fail)?;
                    json(serde_json::to_value(self.session.playlist_rows(Some(index)).map_err(fail)?))
                }
                "track_detail" => {
                    let levels = args.get("levels").and_then(Value::as_bool).unwrap_or(false);
                    json(serde_json::to_value(self.session.track_summary(required(args, "id")?, levels).map_err(fail)?))
                }
                "propose_cues" => {
                    json(serde_json::to_value(self.session.propose_cues(required(args, "id")?, text_arg(args, "rules")).map_err(fail)?))
                }
                _ => {
                    let path = PathBuf::from(required(args, "path")?);
                    let mut ids: Vec<String> = args
                        .get("ids")
                        .and_then(Value::as_array)
                        .map(|ids| ids.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                        .unwrap_or_default();
                    if let Some(playlist) = text_arg(args, "playlist") {
                        let index = self.playlist(playlist).map_err(fail)?;
                        for row in self.session.playlist_rows(Some(index)).map_err(fail)? {
                            if !ids.contains(&row.id) {
                                ids.push(row.id);
                            }
                        }
                    }
                    if ids.is_empty() {
                        return Err("give track IDs, a playlist, or both".to_owned());
                    }
                    let overwrite = args.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
                    let count = self.session.export_tracks(&path, &ids, overwrite).map_err(fail)?;
                    Ok(json!({ "path": path, "tracks": count }))
                }
            }
        };
        Some(run())
    }
}
