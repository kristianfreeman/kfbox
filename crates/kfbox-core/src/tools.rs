//! kfbox's commands as tools for a model: the definitions it reads and the
//! calls it makes. The MCP server offers the reading set; the in-app
//! assistant also gets the writing set, which goes through the safe path like
//! any other write.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::session::{CommandError, CueEdit, Session};

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 500;

/// Which tools a caller gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toolset {
    /// Reading, proposing and exporting XML: the MCP server's set.
    Read,
    /// Reading and proposing, plus writing cues and undoing: the assistant's set.
    Write,
}

const READ: [&str; 6] = ["list_playlists", "search_tracks", "playlist_tracks", "track_detail", "propose_cues", "export_xml"];
const WRITE: [&str; 3] = ["write_cues", "write_rule_cues", "undo"];

impl Toolset {
    fn allows(self, name: &str) -> bool {
        match self {
            Self::Read => READ.contains(&name),
            // The assistant writes to the library, not to files.
            Self::Write => (READ.contains(&name) && name != "export_xml") || WRITE.contains(&name),
        }
    }
}

#[allow(clippy::too_many_lines, reason = "the tools' descriptions and schemas, as data")]
/// The tools' definitions, each with `name`, `description`, `inputSchema` and
/// `annotations`, as MCP lists them.
#[must_use]
pub fn definitions(set: Toolset) -> Vec<Value> {
    let all = json!([
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
    ,
        {
            "name": "write_cues",
            "description": "Saves cue edits to the library straight away, all together or not at all, as one change in its history that the user can undo. Refused while rekordbox is open. Each edit is an object with \"op\": \"add\" (track, slot A-P or memory, start_secs, optional end_secs for a loop, name, color as #rrggbb, replace to overwrite a taken hot cue slot), \"move\" (cue, start_secs), \"rename\" (cue, name), \"recolour\" (cue, color) or \"delete\" (cue). Cue IDs come from track_detail. Put cues on beats: track_detail gives each bar's start.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "What the change does, for the history, such as \"Move C to the second drop on Kotel\"." },
                    "edits": { "type": "array", "items": { "type": "object" }, "minItems": 1 }
                },
                "required": ["summary", "edits"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true }
        },
        {
            "name": "write_rule_cues",
            "description": "Writes the cues the rules place on these tracks into their empty slots, as one change the user can undo. Uses the saved rules unless a rules file is given; try one with propose_cues first. Cues already there are never moved.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ids": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "rules": { "type": "string", "description": "A rules file in kfbox's TOML format, instead of the saved one." }
                },
                "required": ["ids"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false }
        },
        {
            "name": "undo",
            "description": "Undoes the latest change to the library, if nothing has changed since it.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": false, "destructiveHint": true }
        }
    ]);
    all.as_array()
        .map_or_else(Vec::new, |tools| tools.iter().filter(|t| t["name"].as_str().is_some_and(|n| set.allows(n))).cloned().collect())
}

fn text_arg<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

fn required<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    text_arg(args, name).ok_or_else(|| format!("{name} is required"))
}

fn strings(args: &Value, name: &str) -> Vec<String> {
    args.get(name)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// A playlist by name, path, or the index `list_playlists` gives.
fn playlist(session: &Session, name: &str) -> Result<usize, CommandError> {
    name.parse::<usize>().ok().filter(|i| session.playlist_rows(Some(*i)).is_ok()).map_or_else(|| session.find_playlist(name), Ok)
}

fn to_json<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Runs a tool on `session`, which must have a library open. `None` if `set`
/// has no such tool. `label` starts the history's message for a write, such as
/// `Assistant`, so the user can tell who made it.
pub fn call(session: &Session, set: Toolset, name: &str, args: &Value, label: &str) -> Option<Result<Value, String>> {
    if !set.allows(name) {
        return None;
    }
    let fail = |e: CommandError| e.to_string();
    let run = || -> Result<Value, String> {
        match name {
            "list_playlists" => to_json(session.fresh_listing().map_err(fail)?),
            "search_tracks" => {
                let query = required(args, "query")?;
                #[allow(clippy::cast_possible_truncation, reason = "clamped to MAX_LIMIT")]
                let limit = args.get("limit").and_then(Value::as_u64).map_or(DEFAULT_LIMIT, |n| (n as usize).clamp(1, MAX_LIMIT));
                let within = text_arg(args, "playlist").map(|name| playlist(session, name)).transpose().map_err(fail)?;
                to_json(session.search_in(within, query, limit).map_err(fail)?)
            }
            "playlist_tracks" => {
                let index = playlist(session, required(args, "playlist")?).map_err(fail)?;
                to_json(session.playlist_rows(Some(index)).map_err(fail)?)
            }
            "track_detail" => {
                let levels = args.get("levels").and_then(Value::as_bool).unwrap_or(false);
                to_json(session.track_summary(required(args, "id")?, levels).map_err(fail)?)
            }
            "propose_cues" => to_json(session.propose_cues(required(args, "id")?, text_arg(args, "rules")).map_err(fail)?),
            "export_xml" => {
                let path = PathBuf::from(required(args, "path")?);
                let mut ids = strings(args, "ids");
                if let Some(name) = text_arg(args, "playlist") {
                    let index = playlist(session, name).map_err(fail)?;
                    for row in session.playlist_rows(Some(index)).map_err(fail)? {
                        if !ids.contains(&row.id) {
                            ids.push(row.id);
                        }
                    }
                }
                if ids.is_empty() {
                    return Err("give track IDs, a playlist, or both".to_owned());
                }
                let overwrite = args.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
                let count = session.export_tracks(&path, &ids, overwrite).map_err(fail)?;
                Ok(json!({ "path": path, "tracks": count }))
            }
            "write_cues" => {
                let summary = required(args, "summary")?;
                let edits: Vec<CueEdit> =
                    serde_json::from_value(args.get("edits").cloned().unwrap_or(Value::Null)).map_err(|e| format!("edits: {e}"))?;
                to_json(session.save_cues(&format!("{label}: {summary}"), &edits).map_err(fail)?)
            }
            "write_rule_cues" => {
                let ids = strings(args, "ids");
                if ids.is_empty() {
                    return Err("give track IDs".to_owned());
                }
                to_json(session.write_rule_cues_as(&ids, text_arg(args, "rules"), label).map_err(fail)?)
            }
            _ => to_json(session.undo().map_err(fail)?),
        }
    };
    Some(run())
}
