//! Writes the default rules' new hot cues for one track straight into
//! rekordbox's `master.db`, to see whether rekordbox accepts them. Without
//! `--write` it only prints what it would do, and reads a copy of the library:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example write_one                       # pick an uncued track
//! cargo run --release -p kfbox-core --example write_one -- <track id>         # show that track's cues
//! cargo run --release -p kfbox-core --example write_one -- <track id> --write <backup dir>
//! ```
//!
//! rbl-db's writer refuses while rekordbox is running and copies the library
//! to the backup folder before its first change.

#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a command-line tool")]

use std::process::ExitCode;

use kfbox_core::track::GridSource;
use kfbox_core::{Cue, Rules, Track, DEFAULT_RULES};
use rbl_db::write::Writer;

/// rekordbox's `Kind` for a hot cue slot: A to C are 1 to 3, 4 is unused, and
/// D onward start at 5.
fn kind(slot: u8) -> u8 {
    if slot < 3 {
        slot + 1
    } else {
        slot + 2
    }
}

fn new_hot_cues(track: &Track, rules: &Rules) -> Vec<Cue> {
    rules.apply(track).cues.into_iter().filter(|c| !c.existing && c.hot.is_some() && c.end_secs.is_none()).collect()
}

fn print_cues(cues: &[Cue]) {
    for cue in cues {
        let slot = cue.hot.map_or('?', |h| char::from(b'A' + h));
        println!("  {slot}  {:>8.3} s  ({})", cue.start_secs, cue.name);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rules = Rules::parse(DEFAULT_RULES).map_err(|e| e.to_string())?;
    let library = kfbox_core::master::load_installed().map_err(|e| e.to_string())?;

    let Some(id) = args.first() else {
        // The first uncued track on rekordbox's grid that gets all four cues.
        for source in library.all_tracks().into_iter().filter(|t| t.marks.is_empty()) {
            let Ok(track) = Track::from_rekordbox(source) else { continue };
            let cues = new_hot_cues(&track, &rules);
            if track.grid == GridSource::Rekordbox && cues.len() == 4 {
                let playlists: Vec<&str> = library
                    .playlists
                    .iter()
                    .filter(|p| !p.folder && p.track_ids.contains(&source.id))
                    .map(|p| p.path.as_str())
                    .collect();
                println!("{}  {} — {}", source.id, track.tags.artist, track.tags.title);
                println!("  file: {:?}", source.path);
                println!("  playlists: {playlists:?}");
                print_cues(&cues);
                return Ok(());
            }
        }
        return Err("no uncued track on rekordbox's grid gets all four cues".into());
    };

    let source = library.track(id).ok_or_else(|| format!("no track {id}"))?;
    if !source.marks.is_empty() {
        return Err(format!("track {id} already has {} cues; this test only uses uncued tracks", source.marks.len()));
    }
    let track = Track::from_rekordbox(source).map_err(|e| e.to_string())?;
    if track.grid != GridSource::Rekordbox {
        return Err(format!("track {id} is not on rekordbox's grid"));
    }
    let cues = new_hot_cues(&track, &rules);
    println!("{id}  {} — {}", track.tags.artist, track.tags.title);
    print_cues(&cues);

    if args.get(1).map(String::as_str) != Some("--write") {
        return Ok(());
    }
    let backups = args.get(2).ok_or("--write needs a backup folder")?;

    let location = rbl_db::detect().map_err(|e| e.to_string())?;
    let mut writer = Writer::open(location, backups).map_err(|e| e.to_string())?;
    let backup = writer.back_up_now().map_err(|e| e.to_string())?;
    println!("backup: {}", backup.display());
    for cue in &cues {
        let Some(slot) = cue.hot else { continue };
        let ms = (cue.start_secs * 1000.0).round() as u32;
        let cue_id = writer.add_cue(id, kind(slot), ms).map_err(|e| e.to_string())?;
        println!("wrote {} as cue {cue_id}", char::from(b'A' + slot));
    }

    let conn = writer.library().connection();
    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    println!("integrity_check: {check}");
    let mut rows = conn
        .prepare(
            "SELECT Kind, InMsec, ColorTableIndex, Color FROM djmdCue
             WHERE ContentID = ?1 AND rb_local_deleted = 0 ORDER BY Kind",
        )
        .map_err(|e| e.to_string())?;
    let found = rows
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?)))
        .map_err(|e| e.to_string())?;
    for row in found {
        let (k, ms, index, colour) = row.map_err(|e| e.to_string())?;
        println!("  row: Kind {k}, InMsec {ms}, ColorTableIndex {index:?}, Color {colour:?}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
