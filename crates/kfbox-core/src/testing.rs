//! Made-up songs and collections, for tests of kfbox and its front ends.

use std::path::{Path, PathBuf};

/// A 30-second click track at 120 BPM, as a mono 16-bit WAV. `seed` changes
/// the audio, so two files differ.
pub fn wav(path: &Path, seed: u8) -> std::io::Result<()> {
    let rate = 44_100_u32;
    let samples = (0..rate * 30).map(|i| {
        let t = i % (rate / 2);
        if t < 2000 {
            // At most 20,000 plus 25,500: inside i16.
            let level = i32::from(seed) * 100 + 20_000 - i32::try_from(t).unwrap_or(0) * 10;
            i16::try_from(level).unwrap_or(i16::MAX)
        } else {
            0
        }
    });
    let count = rate * 30;
    let bytes = count * 2;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, out)
}

/// A rekordbox XML in `dir` with three songs on a 120 BPM grid: "Alpha" by
/// "First Artist" with hot cue A already set, "Beta" and "Gamma" by "Second
/// Artist", and a playlist "Sets › Warm up" holding Beta and Alpha.
pub fn collection(dir: &Path) -> std::io::Result<PathBuf> {
    let songs = [("1", "Alpha", "First Artist"), ("2", "Beta", "Second Artist"), ("3", "Gamma", "Second Artist")];
    let mut tracks = String::new();
    for (i, (id, title, artist)) in songs.iter().enumerate() {
        let path = dir.join(format!("{title}.wav"));
        wav(&path, u8::try_from(i).unwrap_or(0))?;
        let cue = if *id == "1" { r#"<POSITION_MARK Name="mine" Type="0" Start="2.000" Num="0"/>"# } else { "" };
        tracks.push_str(&format!(
            r#"<TRACK TrackID="{id}" Name="{title}" Artist="{artist}" Genre="Techno" AverageBpm="120.00" TotalTime="30" Tonality="Am" Location="file://localhost{}"><TEMPO Inizio="0.000" Bpm="120.00" Metro="4/4" Battito="1"/>{cue}</TRACK>"#,
            path.display()
        ));
    }
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><DJ_PLAYLISTS Version="1.0.0"><COLLECTION Entries="3">{tracks}</COLLECTION><PLAYLISTS><NODE Type="0" Name="ROOT" Count="1"><NODE Type="0" Name="Sets" Count="1"><NODE Name="Warm up" Type="1" KeyType="0" Entries="2"><TRACK Key="2"/><TRACK Key="1"/></NODE></NODE></NODE></PLAYLISTS></DJ_PLAYLISTS>"#
    );
    let path = dir.join("collection.xml");
    std::fs::write(&path, xml)?;
    Ok(path)
}
