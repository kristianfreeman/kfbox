//! The readable half of the history. `master.db` is encrypted, so git can't
//! show what changed in it. Each commit also holds a JSON file per track, with
//! its cues, tags and rating, which gives real diffs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rbl_db::Library;
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{Map, Value};

use super::SafeError;

const TRACK_COLUMNS: &str = "ID, Title, FolderPath, Rating, ColorID, Commnt, BPM, KeyID, AnalysisDataPath";
const CUE_COLUMNS: &str = "ID, Kind, InMsec, OutMsec, Color, ColorTableIndex, ActiveLoop, BeatLoopSize, Comment";

fn value(cell: ValueRef<'_>) -> Value {
    match cell {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Value::from("<blob>"),
    }
}

fn rows(connection: &Connection, sql: &str) -> Result<Vec<Map<String, Value>>, SafeError> {
    let mut statement = connection.prepare(sql)?;
    let names: Vec<String> = statement.column_names().iter().map(|n| (*n).to_owned()).collect();
    let mut found = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = found.next()? {
        let mut map = Map::new();
        for (i, name) in names.iter().enumerate() {
            map.insert(name.clone(), value(row.get_ref(i)?));
        }
        out.push(map);
    }
    Ok(out)
}

/// Each track's JSON, by track ID.
fn read(library: &Library) -> Result<BTreeMap<String, String>, SafeError> {
    let connection = library.connection();
    let mut cues: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for mut row in rows(
        connection,
        &format!("SELECT ContentID, {CUE_COLUMNS} FROM djmdCue WHERE rb_local_deleted = 0 ORDER BY ContentID, InMsec, ID"),
    )? {
        if let Some(Value::String(track)) = row.remove("ContentID") {
            cues.entry(track).or_default().push(Value::Object(row));
        }
    }
    let mut tags: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for row in rows(connection, "SELECT s.ContentID AS track, t.Name AS name FROM djmdSongMyTag s JOIN djmdMyTag t ON t.ID = s.MyTagID WHERE s.rb_local_deleted = 0 ORDER BY s.ContentID, t.Name")? {
        if let (Some(Value::String(track)), Some(name)) = (row.get("track"), row.get("name")) {
            tags.entry(track.clone()).or_default().push(name.clone());
        }
    }
    let mut out = BTreeMap::new();
    for mut row in rows(connection, &format!("SELECT {TRACK_COLUMNS} FROM djmdContent WHERE rb_local_deleted = 0"))? {
        let Some(Value::String(id)) = row.get("ID").cloned() else { continue };
        row.insert("cues".to_owned(), Value::from(cues.remove(&id).unwrap_or_default()));
        row.insert("my_tags".to_owned(), Value::from(tags.remove(&id).unwrap_or_default()));
        out.insert(id, serde_json::to_string_pretty(&row).map_err(|e| SafeError::Verify(e.to_string()))? + "\n");
    }
    Ok(out)
}

/// Brings `dir` in line with the library, touching only the files that differ.
pub(super) fn write(library: &Library, dir: &Path) -> Result<(), SafeError> {
    rbl_core::durable::create_dir_all(dir)?;
    let tracks = read(library)?;
    for (id, text) in &tracks {
        let path = dir.join(format!("{id}.json"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
            std::fs::write(path, text)?;
        }
    }
    let keep: BTreeSet<String> = tracks.keys().map(|id| format!("{id}.json")).collect();
    for entry in std::fs::read_dir(dir)?.flatten() {
        if !keep.contains(&*entry.file_name().to_string_lossy()) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
