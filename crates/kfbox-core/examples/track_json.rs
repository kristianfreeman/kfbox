//! Analyses audio files, read only, and writes each `Track`, and its zoomed view's columns, as JSON: for
//! checking the waveform and bars on real music without opening a library.
//!
//! `cargo run --release -p kfbox-core --example track_json -- <out dir> <audio>...`

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let out = PathBuf::from(args.next().ok_or("usage: track_json <out dir> <audio>...")?);
    std::fs::create_dir_all(&out)?;
    for path in args {
        let path = PathBuf::from(path);
        let track = kfbox_core::track::Track::analyse(&path)?;
        let name = path.file_stem().map_or_else(|| "track".into(), |s| s.to_string_lossy().replace(' ', "_"));
        let file = out.join(format!("{name}.json"));
        std::fs::write(&file, serde_json::to_vec(&track)?)?;
        // The zoomed view's closer columns, as the app makes them.
        let closer = kfbox_core::track::closer_waveform(&path)?;
        std::fs::write(out.join(format!("{name}.closer.json")), serde_json::to_vec(&closer)?)?;
        println!("{}", file.display());
    }
    Ok(())
}
