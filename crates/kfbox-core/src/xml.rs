//! rekordbox XML, for `File › Import › Import Collection in xml format` or
//! the `rekordbox xml` tree.
//!
//! The layout follows rbxport's `rbl-index/src/xml_export.rs`: a
//! `DJ_PLAYLISTS` document with a `TRACK` per song carrying its cues as
//! `POSITION_MARK`s, and one playlist holding them all.
//!
//! A track loaded from a rekordbox XML is written back as it was read —
//! its `TrackID`, every attribute, its `TEMPO`s and its cues unchanged —
//! with the new cues added. Any other track gets kfbox's grid as `TEMPO`
//! entries, because its cues were placed on that grid.

use std::fmt::Write as _;
use std::path::Path;

use crate::rekordbox::Attributes;
use crate::rules::Cue;
use crate::track::Track;

/// The playlist the tracks are listed under.
pub const PLAYLIST_NAME: &str = "kfbox";

/// The document for these tracks and their cues.
pub fn write_xml(tracks: &[(&Track, &[Cue])]) -> String {
    let mut out = String::with_capacity(tracks.len() * 1200 + 400);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\">\n");
    let _ = writeln!(out, "  <PRODUCT Name=\"kfbox\" Version=\"{}\" Company=\"kfbox\"/>", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "  <COLLECTION Entries=\"{}\">", tracks.len());
    // rekordbox's own IDs stay; the others number on from the largest.
    let mut next_id = tracks
        .iter()
        .filter_map(|(t, _)| t.rekordbox.as_ref()?.id.trim().parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let mut ids = Vec::with_capacity(tracks.len());
    for (track, cues) in tracks {
        let id = match &track.rekordbox {
            Some(source) => {
                push_rekordbox_track(&mut out, track, &source.attributes, cues);
                source.id.clone()
            }
            None => {
                next_id += 1;
                push_track(&mut out, next_id, track, cues);
                next_id.to_string()
            }
        };
        ids.push(id);
    }
    out.push_str("  </COLLECTION>\n  <PLAYLISTS>\n    <NODE Type=\"0\" Name=\"ROOT\" Count=\"1\">\n");
    let _ = writeln!(out, "      <NODE Name=\"{PLAYLIST_NAME}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">", tracks.len());
    for id in ids {
        let _ = writeln!(out, "        <TRACK Key=\"{}\"/>", escape(&id));
    }
    out.push_str("      </NODE>\n    </NODE>\n  </PLAYLISTS>\n</DJ_PLAYLISTS>\n");
    out
}

/// A track as rekordbox gave it, plus the new cues. A track rekordbox had
/// no grid for gets kfbox's.
fn push_rekordbox_track(out: &mut String, track: &Track, attributes: &Attributes, cues: &[Cue]) {
    out.push_str("    ");
    push_element(out, "TRACK", attributes, false);
    match track.rekordbox.as_ref().filter(|s| !s.tempos.is_empty()) {
        Some(source) => {
            for tempo in &source.tempos {
                out.push_str("      ");
                push_element(out, "TEMPO", &tempo.attributes, true);
            }
        }
        None => push_tempos(out, track),
    }
    push_cues(out, cues);
    out.push_str("    </TRACK>\n");
}

/// `<NAME a="b" …>` with the attributes as read, closed with `/>` when
/// `empty`, and a newline.
fn push_element(out: &mut String, name: &str, attributes: &Attributes, empty: bool) {
    out.push('<');
    out.push_str(name);
    for (key, value) in attributes {
        let _ = write!(out, " {key}=\"{}\"", escape(value));
    }
    out.push_str(if empty { "/>\n" } else { ">\n" });
}

fn push_track(out: &mut String, id: u64, track: &Track, cues: &[Cue]) {
    let _ = writeln!(
        out,
        "    <TRACK TrackID=\"{id}\" Name=\"{}\" Artist=\"{}\" Album=\"{}\" Genre=\"{}\" Kind=\"{}\" \
         TotalTime=\"{:.0}\" AverageBpm=\"{:.2}\" Tonality=\"{}\" Location=\"{}\">",
        escape(&track.tags.title),
        escape(&track.tags.artist),
        escape(&track.tags.album),
        escape(&track.tags.genre),
        kind(&track.path),
        track.duration_secs.floor(),
        track.bpm,
        escape(track.key.as_deref().unwrap_or("")),
        escape(&location(&track.path)),
    );
    push_tempos(out, track);
    push_cues(out, cues);
    out.push_str("    </TRACK>\n");
}

/// One `TEMPO` where kfbox's grid starts and one at each tempo change,
/// with the beat's place in the bar.
fn push_tempos(out: &mut String, track: &Track) {
    let mut previous_bpm = None;
    for beat in &track.beats {
        if previous_bpm.is_some_and(|bpm: f64| (bpm - beat.bpm).abs() < 0.005) {
            continue;
        }
        previous_bpm = Some(beat.bpm);
        let _ = writeln!(
            out,
            "      <TEMPO Inizio=\"{:.3}\" Bpm=\"{:.2}\" Metro=\"4/4\" Battito=\"{}\"/>",
            beat.secs, beat.bpm, beat.number
        );
    }
}

/// Existing cues as they were read, new ones as kfbox writes them.
fn push_cues(out: &mut String, cues: &[Cue]) {
    for cue in cues {
        match &cue.attributes {
            Some(attributes) => {
                out.push_str("      ");
                push_element(out, "POSITION_MARK", attributes, true);
            }
            None => push_cue(out, cue),
        }
    }
}

fn push_cue(out: &mut String, cue: &Cue) {
    // `Num` -1 is a memory cue; a hot cue is its slot from 0. `Type` 0 is a
    // cue and 4 a loop, which also carries its `End`.
    let num = cue.hot.map_or(-1, i32::from);
    let _ = write!(out, "      <POSITION_MARK Name=\"{}\" ", escape(&cue.name));
    match cue.end_secs {
        Some(end) => {
            let _ = write!(out, "Type=\"4\" Start=\"{:.3}\" End=\"{end:.3}\" Num=\"{num}\"", cue.start_secs);
        }
        None => {
            let _ = write!(out, "Type=\"0\" Start=\"{:.3}\" Num=\"{num}\"", cue.start_secs);
        }
    }
    if let Some([r, g, b]) = cue.color {
        let _ = write!(out, " Red=\"{r}\" Green=\"{g}\" Blue=\"{b}\"");
    }
    out.push_str("/>\n");
}

/// The file type as rekordbox's `Kind` column names it.
fn kind(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp3") => "MP3 File",
        Some("wav") => "WAV File",
        Some("aif" | "aiff") => "AIFF File",
        Some("flac") => "FLAC File",
        Some("m4a" | "mp4" | "aac") => "M4A File",
        _ => "",
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// A path as rekordbox writes `Location`: `file://localhost` and the path
/// percent-encoded, `/` and the unreserved characters left as they are.
pub(crate) fn location(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let mut out = String::with_capacity(path.len() + 16);
    out.push_str("file://localhost");
    if !path.starts_with('/') {
        out.push('/');
    }
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(char::from(byte)),
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::rekordbox::RbLibrary;
    use crate::track::{Beat, GridSource, Tags};

    #[test]
    fn a_location_is_a_file_url_with_the_path_percent_encoded() {
        assert_eq!(location(Path::new("/Users/me/Music/All U Need.mp3")), "file://localhost/Users/me/Music/All%20U%20Need.mp3");
        assert_eq!(location(Path::new("/Müsic/♪&.mp3")), "file://localhost/M%C3%BCsic/%E2%99%AA%26.mp3");
    }

    fn new_cue(hot: Option<u8>, start_secs: f64, end_secs: Option<f64>, color: Option<[u8; 3]>, name: &str) -> Cue {
        Cue { id: None, hot, start_secs, end_secs, color, name: name.into(), existing: false, attributes: None }
    }

    #[test]
    fn a_rekordbox_track_is_written_back_as_read_with_the_new_cues() {
        let library = RbLibrary::parse(crate::rekordbox::tests::DOC).unwrap();
        let source = library.track("11").unwrap().clone();
        let track = Track {
            path: source.path.clone().unwrap(),
            tags: Tags::default(),
            duration_secs: 120.0,
            bpm: 128.0,
            key: None,
            camelot: None,
            beats: Vec::new(),
            bars: Vec::new(),
            overview: Vec::new(),
            grid: GridSource::Rekordbox,
            rekordbox: Some(source),
        };
        let cues: Vec<Cue> = crate::Rules::parse("").unwrap().apply(&track).cues;
        let mut all = cues.clone();
        all.push(new_cue(Some(1), 30.0, None, None, "new"));
        let plain = Track { rekordbox: None, ..track.clone() };
        let xml = write_xml(&[(&track, &all), (&plain, &[])]);
        assert!(xml.contains("<TRACK TrackID=\"11\" Name=\"All U Need\" Artist=\"A &amp; B\" Tonality=\"Fm\" AverageBpm=\"128.00\""));
        assert!(xml.contains("<TEMPO Inizio=\"60.025\" Bpm=\"130.00\" Metro=\"4/4\" Battito=\"3\"/>"));
        assert!(xml.contains("<POSITION_MARK Name=\"in\" Type=\"0\" Start=\"0.025\" Num=\"0\" Red=\"40\" Green=\"226\" Blue=\"20\"/>"));
        assert!(xml.contains("<POSITION_MARK Name=\"\" Type=\"4\" Start=\"60\" End=\"63.75\" Num=\"2\"/>"));
        assert!(xml.contains("<POSITION_MARK Name=\"new\" Type=\"0\" Start=\"30.000\" Num=\"1\"/>"));
        assert_eq!(xml.matches("<POSITION_MARK").count(), 4);
        assert!(xml.contains("<TRACK TrackID=\"12\""), "a file added by hand numbers on from the largest ID");
        assert!(xml.contains("<TRACK Key=\"11\"/>\n        <TRACK Key=\"12\"/>"));
    }

    #[test]
    fn a_track_has_its_grid_cues_and_a_playlist_entry() {
        let track = Track {
            path: "/m/Roax - Decompose.mp3".into(),
            tags: Tags { title: "Decompose".into(), artist: "Roax & Co".into(), ..Tags::default() },
            duration_secs: 301.6,
            bpm: 130.0,
            key: Some("Ebm".into()),
            camelot: Some("2A".into()),
            beats: vec![
                Beat { secs: 0.1, number: 1, bpm: 130.0 },
                Beat { secs: 0.56, number: 2, bpm: 130.0 },
            ],
            bars: Vec::new(),
            overview: Vec::new(),
            grid: GridSource::Kfbox,
            rekordbox: None,
        };
        let cues = [
            new_cue(Some(0), 0.1, None, Some([0, 255, 0]), "start"),
            new_cue(Some(3), 200.0, Some(207.385), None, ""),
            new_cue(None, 50.0, None, None, ""),
        ];
        let xml = write_xml(&[(&track, &cues)]);
        assert!(xml.contains("Artist=\"Roax &amp; Co\""));
        assert!(xml.contains("TotalTime=\"301\" AverageBpm=\"130.00\" Tonality=\"Ebm\""));
        assert!(xml.contains("Kind=\"MP3 File\""));
        assert_eq!(xml.matches("<TEMPO ").count(), 1);
        assert!(xml.contains("<TEMPO Inizio=\"0.100\" Bpm=\"130.00\" Metro=\"4/4\" Battito=\"1\"/>"));
        assert!(xml.contains("<POSITION_MARK Name=\"start\" Type=\"0\" Start=\"0.100\" Num=\"0\" Red=\"0\" Green=\"255\" Blue=\"0\"/>"));
        assert!(xml.contains("<POSITION_MARK Name=\"\" Type=\"4\" Start=\"200.000\" End=\"207.385\" Num=\"3\"/>"));
        assert!(xml.contains("Num=\"-1\""));
        assert!(xml.contains("<TRACK Key=\"1\"/>"));
    }
}
