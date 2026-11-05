//! Builds a test library in a folder, with made-up audio for some tracks and
//! the kinds of data a real library has: hot cues past D with colours, a
//! loop, memory cues, a rating. For the app's smoke test, which opens it in
//! place of rekordbox's.
//!
//! `cargo run -p kfbox-core --example fixture -- <dir>`

use std::path::PathBuf;

use kfbox_core::session::{CueEdit, Session};
use rbl_db::fixture::{self, Shape};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args_os().nth(1).ok_or("usage: fixture <dir>")?);
    let lib = dir.join("lib");
    std::fs::create_dir_all(&lib)?;
    let location = fixture::build(&lib, Shape { tracks: 120, ..Shape::default() })?;
    for i in 0..3_u8 {
        let song = dir.join(format!("song{i}.wav"));
        kfbox_core::testing::wav(&song, i)?;
        fixture::point_at_audio(&location, usize::from(i), &song.display().to_string(), 30)?;
    }

    // Cues and a rating as rekordbox users have them, written as the app would.
    let session = Session::open(&dir.join("app"))?;
    session.open_db(location)?;
    let first = fixture::track_id(0);
    let add = |slot: &str, start_secs: f64, end_secs: Option<f64>, name: &str, color: Option<&str>| CueEdit::Add {
        track: first.clone(),
        slot: slot.into(),
        start_secs,
        end_secs,
        name: name.into(),
        color: color.map(Into::into),
        replace: false,
    };
    session.save_cues(
        "Cues as a person sets them",
        &[
            add("A", 0.0, None, "", Some("#ff008a")),
            add("B", 4.0, Some(8.0), "loop", None),
            add("F", 12.0, None, "late hot cue", Some("#00ffff")),
            add("memory", 2.0, None, "", None),
            add("memory", 6.0, None, "a memory cue", None),
        ],
    )?;
    session.set_rating(&[first, fixture::track_id(1)], 4)?;
    println!("{}", lib.display());
    Ok(())
}
