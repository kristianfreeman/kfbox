//! Prints a song's analysis and the default rules' cues as JSON, the shape
//! the app's page receives:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example dump -- song.mp3 > song.json
//! cargo run --release -p kfbox-core --example dump -- rekordbox.xml 123 > song.json
//! ```
//!
//! The second form takes `TrackID` 123 from a rekordbox XML, on its own grid
//! and with the cues it already has.

#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a command-line tool")]

use std::process::ExitCode;

use kfbox_core::track::TrackError;
use kfbox_core::{RbLibrary, Rules, Track, DEFAULT_RULES};

fn load(song: &str, id: Option<&String>) -> Result<Track, String> {
    let Some(id) = id else { return Track::analyse(song.as_ref()).map_err(|e| e.to_string()) };
    let text = std::fs::read_to_string(song).map_err(|e| e.to_string())?;
    let library = RbLibrary::parse(&text).map_err(|e| e.to_string())?;
    let source = library.track(id).ok_or_else(|| format!("no TrackID {id}"))?;
    Track::from_rekordbox(source).map_err(|e: TrackError| e.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(song) = args.first() else {
        eprintln!("usage: dump <song> | dump <rekordbox.xml> <TrackID>");
        return ExitCode::FAILURE;
    };
    let (track, rules) = match (load(song, args.get(1)), Rules::parse(DEFAULT_RULES)) {
        (Ok(track), Ok(rules)) => (track, rules),
        (Err(e), _) => {
            eprintln!("{song}: {e}");
            return ExitCode::FAILURE;
        }
        (_, Err(e)) => {
            eprintln!("rules: {e}");
            return ExitCode::FAILURE;
        }
    };
    let applied = rules.apply(&track);
    match serde_json::to_string(&serde_json::json!({ "track": track, "applied": applied })) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
