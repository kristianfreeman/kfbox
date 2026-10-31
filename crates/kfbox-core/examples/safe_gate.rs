//! Milestone 3's real check: colours, names, a loop, a memory cue and a batch
//! on three tracks of rekordbox's real library, through the safe write path.
//! Without `--write` it only prints the plan, from a copy of the library:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example safe_gate
//! cargo run --release -p kfbox-core --example safe_gate -- --write
//! ```

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

fn main() {
    let write = std::env::args().any(|a| a == "--write");
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
    let location = rbl_db::detect().expect("find rekordbox's library");
    let root = dirs_root();
    let mut safe = match SafeLibrary::open_real(location, &root) {
        Ok(safe) => safe,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let summary = format!("Milestone 3 check: cues on {}", names.join(", "));
    match safe.apply(&Change { summary, ops, files: Vec::new() }) {
        Ok(receipt) => {
            println!("\nWrote {} cues, commit {}.", receipt.added_cues.len(), receipt.commit.unwrap_or_default());
            println!("To undo: the history and archive are in {}", root.display());
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
