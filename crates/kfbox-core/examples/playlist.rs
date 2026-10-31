//! Applies the default rules to a playlist of a rekordbox XML, or of
//! rekordbox's own library when the source is `rekordbox`, and writes the
//! result:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example playlist -- rekordbox           # your library's playlists
//! cargo run --release -p kfbox-core --example playlist -- rekordbox.xml           # lists the playlists
//! cargo run --release -p kfbox-core --example playlist -- rekordbox.xml 3 out.xml # the fourth one
//! cargo run --release -p kfbox-core --example playlist -- rekordbox all out.xml   # the whole collection
//! ```
//!
//! A rules file after `out.xml` replaces the default rules.

#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a command-line tool")]

use std::process::ExitCode;

use kfbox_core::track::GridSource;
use kfbox_core::{write_xml, Cue, RbLibrary, Rules, Track, DEFAULT_RULES};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(source) = args.first() else {
        eprintln!("usage: playlist <rekordbox.xml> [index out.xml]");
        return ExitCode::FAILURE;
    };
    let loaded = if source == "rekordbox" {
        kfbox_core::master::load_installed().map_err(|e| e.to_string())
    } else {
        std::fs::read_to_string(source).map_err(|e| e.to_string()).and_then(|t| RbLibrary::parse(&t).map_err(|e| e.to_string()))
    };
    let library = match loaded {
        Ok(library) => library,
        Err(e) => {
            eprintln!("{source}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (Some(which), Some(out)) = (args.get(1), args.get(2)) else {
        println!("{} tracks", library.tracks.len());
        for (i, p) in library.playlists.iter().enumerate().filter(|(_, p)| !p.folder) {
            println!("{i:>4}  {}  ({} tracks)", p.path, p.track_ids.len());
        }
        return ExitCode::SUCCESS;
    };
    let text = match args.get(3).map(std::fs::read_to_string) {
        Some(Ok(text)) => text,
        Some(Err(e)) => {
            eprintln!("rules: {e}");
            return ExitCode::FAILURE;
        }
        None => DEFAULT_RULES.to_owned(),
    };
    let rules = match Rules::parse(&text) {
        Ok(rules) => rules,
        Err(e) => {
            eprintln!("rules: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut done = Vec::new();
    let tracks = match which.parse::<usize>() {
        Ok(index) => library.playlist_tracks(index),
        Err(_) => library.all_tracks(),
    };
    for source in tracks {
        match Track::from_rekordbox(source) {
            Ok(track) => {
                let applied = rules.apply(&track);
                let grid = if track.grid == GridSource::Rekordbox { "rekordbox grid" } else { "kfbox grid" };
                let cues: String = applied
                    .cues
                    .iter()
                    .map(|c| {
                        let slot = c.hot.map_or_else(|| "m".to_owned(), |h| char::from(b'A' + h).to_string());
                        if c.existing { slot.to_lowercase() } else { slot }
                    })
                    .collect();
                println!("{} — {} ({grid}): {cues}", track.tags.artist, track.tags.title);
                done.push((track, applied));
            }
            Err(e) => eprintln!("{}: {e}", source.attribute("Name")),
        }
    }
    let pairs: Vec<(&Track, &[Cue])> = done.iter().map(|(t, a)| (t, a.cues.as_slice())).collect();
    if let Err(e) = std::fs::write(out, write_xml(&pairs)) {
        eprintln!("{out}: {e}");
        return ExitCode::FAILURE;
    }
    println!("wrote {} tracks to {out} (new cues in capitals, existing in lower case)", pairs.len());
    ExitCode::SUCCESS
}
