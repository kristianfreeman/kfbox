//! Tries the safe write path on a temporary copy of rekordbox's library and
//! compares what it writes with the rows rekordbox wrote itself. It only reads
//! the real library, to copy it:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example safe_check
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::unwrap_used, clippy::expect_used, reason = "a command-line check")]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use kfbox_core::safe::{Change, Op, SafeLibrary, Slot};
use rbl_db::{Library, LibraryLocation, OpenMode};
use rusqlite::types::ValueRef;

/// Each cue column's kinds of value, over the rows `filter` picks.
fn shapes(location: &LibraryLocation, filter: &str) -> (usize, BTreeMap<String, BTreeSet<&'static str>>) {
    let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
    let mut statement = library.connection().prepare(&format!("SELECT * FROM djmdCue WHERE {filter}")).unwrap();
    let names: Vec<String> = statement.column_names().iter().map(|n| (*n).to_owned()).collect();
    let mut rows = statement.query([]).unwrap();
    let mut found: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let mut count = 0;
    while let Some(row) = rows.next().unwrap() {
        count += 1;
        for (i, name) in names.iter().enumerate() {
            let kind = match row.get_ref(i).unwrap() {
                ValueRef::Null => "null",
                ValueRef::Integer(_) => "int",
                ValueRef::Real(_) => "real",
                ValueRef::Text(_) => "text",
                ValueRef::Blob(_) => "blob",
            };
            found.entry(name.clone()).or_default().insert(kind);
        }
    }
    (count, found)
}

fn main() {
    let real = rbl_db::detect().expect("find rekordbox's library");
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("lib/master.db");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(&real.master_db, &copy).unwrap();
    let share = scratch.path().join("lib/share");
    std::fs::create_dir_all(&share).unwrap();
    let location = LibraryLocation { master_db: copy.clone(), share_root: share, is_real_install: false, ..real.clone() };

    // Tracks with every slot from M up free, so nothing is refused.
    let tracks: Vec<String> = {
        let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
        let mut statement = library.connection().prepare("SELECT ID FROM djmdContent WHERE rb_local_deleted = 0 AND ID NOT IN (SELECT ContentID FROM djmdCue WHERE Kind >= 14) LIMIT 40").unwrap();
        statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    let highest: String = {
        let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
        library.connection().query_row("SELECT COALESCE(MAX(updated_at), '') FROM djmdCue", [], |r| r.get(0)).unwrap()
    };

    let mut safe = SafeLibrary::open(location.clone(), &scratch.path().join("kfbox")).unwrap();
    let mut ops = Vec::new();
    for track in &tracks {
        ops.push(Op::AddCue { track: track.clone(), slot: Slot::Hot(12), start_ms: 1000, colour: Some(5), name: "start".into() });
        ops.push(Op::AddCue { track: track.clone(), slot: Slot::Hot(13), start_ms: 9000, colour: None, name: "drop".into() });
        ops.push(Op::AddLoop {
            track: track.clone(),
            slot: Slot::Hot(14),
            start_ms: 20000,
            end_ms: 26000,
            beats: 16,
            colour: Some(9),
            name: "groove".into(),
        });
        ops.push(Op::AddCue { track: track.clone(), slot: Slot::Memory, start_ms: 31234, colour: Some(3), name: "memory".into() });
    }
    let before = std::fs::read(&copy).unwrap();
    let timer = Instant::now();
    let receipt = safe.apply(&Change { summary: format!("Check {} tracks", tracks.len()), ops: ops.clone(), files: Vec::new() }).unwrap();
    println!("batch: {} ops on {} tracks in {:?}, {} cues added", ops.len(), tracks.len(), timer.elapsed(), receipt.added_cues.len());

    let (theirs, their_shape) = shapes(&location, &format!("rb_local_deleted = 0 AND updated_at <= '{highest}' AND Kind > 0"));
    let (mine, my_shape) = shapes(&location, &format!("rb_local_deleted = 0 AND updated_at > '{highest}' AND Kind > 0"));
    println!("hot cue rows: {theirs} from rekordbox, {mine} from kfbox");
    let mut differ = 0;
    for (column, kinds) in &my_shape {
        let rekordbox = their_shape.get(column).cloned().unwrap_or_default();
        if !kinds.is_subset(&rekordbox) {
            differ += 1;
            println!("  {column}: kfbox writes {kinds:?}, rekordbox has {rekordbox:?}");
        }
    }
    println!("columns where kfbox writes a kind of value rekordbox's rows don't have: {differ}");

    let timer = Instant::now();
    safe.undo().unwrap();
    println!("undo: {:?}, file back exactly: {}", timer.elapsed(), std::fs::read(&copy).unwrap() == before);

    for i in 0..30 {
        let op =
            Op::AddCue { track: tracks[i % tracks.len()].clone(), slot: Slot::Hot(12), start_ms: 1000, colour: None, name: String::new() };
        safe.apply(&Change { summary: format!("One cue {i}"), ops: vec![op], files: Vec::new() }).unwrap();
        safe.undo().unwrap();
    }
    safe.pack_history().unwrap();
    println!("history: {} MB after 60 more commits and a pack; git finds it intact", safe.check_history().unwrap() / 1_000_000);
    println!("real library untouched: {}", std::fs::read(&real.master_db).unwrap() == before);
}
