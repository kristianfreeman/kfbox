//! Applies a rules file to songs from the command line and writes the XML:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example cue -- rules.toml out.xml song.mp3 …
//! ```
//!
//! Pass `-` for the rules to use the defaults.

#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a command-line tool")]

use std::path::PathBuf;
use std::process::ExitCode;

use kfbox_core::{write_xml, Rules, Track, DEFAULT_RULES};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [rules_path, out, songs @ ..] = args.as_slice() else {
        eprintln!("usage: cue <rules.toml|-> <out.xml> <song>…");
        return ExitCode::FAILURE;
    };
    let text = if rules_path == "-" {
        DEFAULT_RULES.to_owned()
    } else {
        match std::fs::read_to_string(rules_path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("{rules_path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    };
    let rules = match Rules::parse(&text) {
        Ok(rules) => rules,
        Err(e) => {
            eprintln!("{rules_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut done = Vec::new();
    for song in songs {
        let started = std::time::Instant::now();
        match Track::analyse(&PathBuf::from(song)) {
            Ok(track) => {
                let applied = rules.apply(&track);
                println!(
                    "{} — {} ({:.2} BPM, {}, {} bars, {:.1}s)",
                    track.tags.artist,
                    track.tags.title,
                    track.bpm,
                    track.key.as_deref().unwrap_or("?"),
                    track.bars.len(),
                    started.elapsed().as_secs_f64()
                );
                println!("  landmarks: {:?}", applied.landmarks);
                for cue in &applied.cues {
                    let slot = cue.hot.map_or_else(|| "mem".to_owned(), |h| char::from(b'A' + h).to_string());
                    let end = cue.end_secs.map(|e| format!(" → {e:.2}")).unwrap_or_default();
                    println!("  {slot} {:>7.2}s{end}  {}", cue.start_secs, cue.name);
                }
                for note in &applied.notes {
                    println!("  note: {note}");
                }
                done.push((track, applied));
            }
            Err(e) => eprintln!("{song}: {e}"),
        }
    }
    let pairs: Vec<(&Track, &[kfbox_core::Cue])> = done.iter().map(|(t, a)| (t, a.cues.as_slice())).collect();
    if let Err(e) = std::fs::write(out, write_xml(&pairs)) {
        eprintln!("{out}: {e}");
        return ExitCode::FAILURE;
    }
    println!("wrote {} tracks to {out}", pairs.len());
    ExitCode::SUCCESS
}
