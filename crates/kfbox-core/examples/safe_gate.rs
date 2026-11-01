//! Milestone 3's real check: colours, names, a loop, a memory cue and a batch
//! on three tracks of rekordbox's real library, through the safe write path.
//! Without `--write` it only prints the plan, from a copy of the library:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example safe_gate
//! cargo run --release -p kfbox-core --example safe_gate -- --write
//! cargo run --release -p kfbox-core --example safe_gate -- --verify
//! cargo run --release -p kfbox-core --example safe_gate -- --undo
//! ```
//!
//! `--verify` only reads. `--undo` deletes exactly the cues `--write` added,
//! through the same safe path, so it keeps anything rekordbox changed since.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::unwrap_used, clippy::expect_used, reason = "a command-line check")]

use kfbox_core::master;
use kfbox_core::safe::{hot_colour_index, Change, Op, SafeLibrary, Slot};
use kfbox_core::{Rules, Track, DEFAULT_RULES};

const RED: [u8; 3] = [0xFF, 0, 0];
const GREEN: [u8; 3] = [0, 0xFF, 0];
const BLUE: [u8; 3] = [0, 0, 0xFF];
const TRACKS: usize = 3;
const LOOP_BEATS: i64 = 16;

fn ms(secs: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a time within a track")]
    let value = (secs * 1000.0).round() as u32;
    value
}

/// Where `--write` lists the cues it added, for `--verify` and `--undo`.
fn cue_list() -> std::path::PathBuf {
    dirs_root().join("safe-gate-cues.txt")
}

fn verify() {
    let location = rbl_db::detect().expect("find rekordbox's library");
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("master.db");
    std::fs::copy(&location.master_db, &copy).unwrap();
    let library = rbl_db::Library::open(rbl_db::LibraryLocation { master_db: copy, is_real_install: false, ..location }, rbl_db::OpenMode::ReadOnly).unwrap();
    let connection = library.connection();
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    println!("integrity_check: {integrity}");
    let ids = std::fs::read_to_string(cue_list()).unwrap_or_default();
    for id in ids.lines() {
        let row = connection.query_row(
            "SELECT t.Title, c.Kind, c.InMsec, c.OutMsec, c.Color, c.ColorTableIndex, c.BeatLoopSize, c.Comment, c.rb_local_deleted
             FROM djmdCue c JOIN djmdContent t ON t.ID = c.ContentID WHERE c.ID = ?1",
            [id],
            |r| {
                Ok(format!(
                    "{}: Kind {}, In {}, Out {:?}, Color {:?}, ColorTableIndex {:?}, BeatLoopSize {:?}, Comment {:?}, deleted {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, i64>(8)?,
                ))
            },
        );
        println!("cue {id}: {}", row.unwrap_or_else(|e| format!("not found ({e})")));
    }
}

fn open_real() -> SafeLibrary {
    let location = rbl_db::detect().expect("find rekordbox's library");
    match SafeLibrary::open_real(location, &dirs_root()) {
        Ok(safe) => safe,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn undo() {
    let ids = std::fs::read_to_string(cue_list()).expect("the list of added cues");
    let ops: Vec<Op> = ids.lines().map(|cue| Op::DeleteCue { cue: cue.to_owned() }).collect();
    let mut safe = open_real();
    match safe.apply(&Change { summary: "Remove the milestone 3 check's cues".to_owned(), ops, files: Vec::new() }) {
        Ok(receipt) => {
            std::fs::remove_file(cue_list()).unwrap();
            println!("Removed {} cues, commit {}.", ids.lines().count(), receipt.commit.unwrap_or_default());
        }
        Err(e) => {
            eprintln!("Nothing changed: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let mode = |flag: &str| std::env::args().any(|a| a == flag);
    if mode("--verify") {
        verify();
        return;
    }
    if mode("--undo") {
        undo();
        return;
    }
    let write = mode("--write");
    if write && cue_list().exists() {
        eprintln!("The check has already run. Nothing changed.");
        std::process::exit(1);
    }
    let library = master::load_installed().expect("read rekordbox's library");
    let rules = Rules::parse(DEFAULT_RULES).unwrap();
    let mut ops = Vec::new();
    let mut names = Vec::new();
    for source in library.all_tracks().into_iter().filter(|t| t.marks.is_empty() && t.path.is_some()) {
        if names.len() == TRACKS {
            break;
        }
        let Ok(track) = Track::from_rekordbox(source) else { continue };
        let landmarks = rules.apply(&track).landmarks;
        let (Some(start), Some(drop), Some(groove)) = (landmarks.start, landmarks.drop, landmarks.groove) else { continue };
        let at = |bar: usize| track.beat_secs(bar, 0).map(ms);
        let (Some(a), Some(c), Some(d), Some(d_end)) = (at(start), at(drop), at(groove), track.beat_secs(groove, LOOP_BEATS).map(ms)) else { continue };
        println!("{} – {}", source.attribute("Artist"), source.attribute("Name"));
        println!("  A green, named \"start\", at {:.1} s", f64::from(a) / 1000.0);
        println!("  C red, named \"drop\", at {:.1} s", f64::from(c) / 1000.0);
        println!("  D blue, named \"groove\", a {LOOP_BEATS}-beat loop at {:.1} s", f64::from(d) / 1000.0);
        println!("  a memory cue, named \"memory\", at {:.1} s", f64::from(c) / 1000.0);
        let id = &source.id;
        ops.push(Op::AddCue { track: id.clone(), slot: Slot::Hot(0), start_ms: a, colour: hot_colour_index(GREEN), name: "start".into() });
        ops.push(Op::AddCue { track: id.clone(), slot: Slot::Hot(2), start_ms: c, colour: hot_colour_index(RED), name: "drop".into() });
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a small constant")]
        ops.push(Op::AddLoop { track: id.clone(), slot: Slot::Hot(3), start_ms: d, end_ms: d_end, beats: LOOP_BEATS as u16, colour: hot_colour_index(BLUE), name: "groove".into() });
        ops.push(Op::AddCue { track: id.clone(), slot: Slot::Memory, start_ms: c, colour: None, name: "memory".into() });
        names.push(source.attribute("Name"));
    }
    if !write {
        println!("\nNothing written. Run with --write to make this change.");
        return;
    }
    let mut safe = open_real();
    let summary = format!("Milestone 3 check: cues on {}", names.join(", "));
    match safe.apply(&Change { summary, ops, files: Vec::new() }) {
        Ok(receipt) => {
            std::fs::write(cue_list(), receipt.added_cues.join("\n") + "\n").unwrap();
            println!("\nWrote {} cues, commit {}.", receipt.added_cues.len(), receipt.commit.unwrap_or_default());
        }
        Err(e) => {
            eprintln!("Nothing changed: {e}");
            std::process::exit(1);
        }
    }
}

fn dirs_root() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").expect("HOME");
    std::path::Path::new(&home).join("Library/Application Support/com.kristianfreeman.kfbox/safe")
}
