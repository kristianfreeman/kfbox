//! Scores the rules against the tracks in rekordbox's library that have
//! hot cues A to D by hand. Each track's cues are hidden, the rules placed,
//! and the bars compared. The other tracks can't be scored, so they are
//! listed with where their drop landed, to catch a rule that goes wrong on
//! music it wasn't tuned on:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example score
//! cargo run --release -p kfbox-core --example score -- rules.toml
//! cargo run --release -p kfbox-core --example score -- - bars.json   # also writes each track's bars
//! ```

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::many_single_char_names,
    reason = "a command-line tool, and A to D are cue names"
)]

use std::process::ExitCode;

use kfbox_core::{Rules, Track, DEFAULT_RULES};
use serde::Serialize;

#[derive(Serialize)]
struct Row {
    title: String,
    /// The bars of A to D by hand, fractional when off a downbeat.
    hand: [f64; 4],
    bars: Vec<[f32; 3]>,
}

/// The cue's place in bars, counting a part-way bar as a fraction.
fn bar_of(track: &Track, secs: f64) -> f64 {
    let next = track.bars.iter().position(|b| b.start_secs > secs + 0.02).unwrap_or(track.bars.len());
    let Some(index) = next.checked_sub(1) else { return 0.0 };
    let Some(bar) = track.bars.get(index) else { return 0.0 };
    #[allow(clippy::cast_precision_loss, reason = "a bar index")]
    let whole = index as f64;
    whole + ((secs - bar.start_secs) / (bar.end_secs - bar.start_secs)).max(0.0)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = match args.first().filter(|a| *a != "-").map(std::fs::read_to_string) {
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
    let library = match kfbox_core::master::load_installed() {
        Ok(library) => library,
        Err(e) => {
            eprintln!("rekordbox: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut hits = [0_usize; 4];
    let mut rows = Vec::new();
    let mut others = Vec::new();
    println!("{:<52} {:>18} {:>18}", "track", "A B C D by hand", "difference");
    for source in library.all_tracks() {
        let hand: Vec<f64> = (0..4).filter_map(|num| source.marks.iter().find(|m| m.num == num).map(|m| m.start_secs)).collect();
        let &[a, b, c, d] = hand.as_slice() else {
            let mut hidden = source.clone();
            hidden.marks.clear();
            if let Ok(track) = Track::from_rekordbox(&hidden) {
                let found = rules.apply(&track).landmarks;
                let name: String = format!("{} – {}", source.attribute("Artist"), source.attribute("Name")).chars().take(50).collect();
                others.push((name, track.bars.len(), found.drop, found.groove));
            }
            continue;
        };
        let mut hidden = source.clone();
        hidden.marks.clear();
        let track = match Track::from_rekordbox(&hidden) {
            Ok(track) => track,
            Err(e) => {
                eprintln!("{}: {e}", source.attribute("Name"));
                continue;
            }
        };
        let applied = rules.apply(&track);
        let hand = [a, b, c, d].map(|secs| bar_of(&track, secs));
        let mut diffs = [None; 4];
        for slot in 0..4_u8 {
            let index = usize::from(slot);
            if let (Some(cue), Some(want)) = (applied.cues.iter().find(|c| c.hot == Some(slot)), hand.get(index)) {
                let diff = bar_of(&track, cue.start_secs) - want;
                if let Some(place) = diffs.get_mut(index) {
                    *place = Some(diff);
                }
                if diff.abs() < 0.5 {
                    if let Some(hit) = hits.get_mut(index) {
                        *hit += 1;
                    }
                }
            }
        }
        let name: String = format!("{} – {}", source.attribute("Artist"), source.attribute("Name")).chars().take(50).collect();
        let shown = |v: &[f64; 4]| v.map(|x| format!("{x:>4.0}")).join("");
        let diff_text = diffs.map(|d| d.map_or_else(|| "   ?".to_owned(), |x| format!("{x:>+4.0}"))).join("");
        println!("{name:<52} {:>18} {diff_text:>18}", shown(&hand));
        rows.push(Row { title: name, hand, bars: track.bars.iter().map(|b| [b.volume, b.bass, b.busy]).collect() });
    }
    let total: usize = hits.iter().sum();
    println!(
        "\nsame bar over {} tracks: A {} B {} C {} D {}, {total} of {}",
        rows.len(),
        hits[0],
        hits[1],
        hits[2],
        hits[3],
        rows.len() * 4
    );
    println!("\n{} tracks not cued by hand: bars, drop, groove", others.len());
    for (name, bars, drop, groove) in &others {
        let at = |bar: &Option<usize>| bar.map_or_else(|| "none".to_owned(), |b| b.to_string());
        println!("{name:<52} {bars:>4} {:>5} {:>5}", at(drop), at(groove));
    }
    if let Some(out) = args.get(1) {
        let json = serde_json::to_string(&rows).unwrap_or_default();
        if let Err(e) = std::fs::write(out, json) {
            eprintln!("{out}: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
