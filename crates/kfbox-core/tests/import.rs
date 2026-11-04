//! Importing song files into a fixture library: analysed, cued, with their
//! grid on rekordbox's timeline; duplicates and missing files. Never a real
//! library.
//!
//! The two MP3s in `fixtures/` are the same 120 BPM click, encoded by LAME
//! with and without its Info frame. rekordbox plays that frame as 1,152
//! samples of silence, so the LAME file's clicks, and its grid, are 26 ms
//! later on rekordbox's timeline than the plain file's (NOTES.md).

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::{Path, PathBuf};

use kfbox_core::session::Session;
use kfbox_core::testing;
use rbl_db::fixture::{self, Shape};
use rbl_db::{Library, LibraryLocation, OpenMode};

struct Bench {
    dir: tempfile::TempDir,
    location: LibraryLocation,
    session: Session,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let mut location = fixture::build(&lib, Shape { tracks: 6, ..Shape::default() }).unwrap();
    location.share_root = lib.join("share");
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.open_db(location.clone()).unwrap();
    Bench { dir, location, session }
}

/// A fixture copied into the bench, so the library points at a file that lives as long as it.
fn song(b: &Bench, name: &str) -> PathBuf {
    let to = b.dir.path().join("music").join(name);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name), &to).unwrap();
    to
}

/// The first beat of a track's grid as its `.DAT` in the share folder holds it, in ms.
fn first_beat_ms(b: &Bench, path: &Path) -> u32 {
    let row = b.session.search("", 100).unwrap().into_iter().find(|r| Path::new(&r.path) == path).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("master.db");
    std::fs::copy(&b.location.master_db, &copy).unwrap();
    let library = Library::open(LibraryLocation { master_db: copy, ..b.location.clone() }, OpenMode::ReadOnly).unwrap();
    let analysis_path: String =
        library.connection().query_row("SELECT AnalysisDataPath FROM djmdContent WHERE ID = ?1", [&row.id], |r| r.get(0)).unwrap();
    let dat = rbl_anlz::Anlz::read(&b.location.share_root.join(analysis_path.trim_start_matches('/'))).unwrap();
    dat.beat_grid().unwrap()[0].time_ms
}

/// Where the first click starts in the audio rekordbox plays.
fn first_click_ms(path: &Path) -> f64 {
    let audio = rbl_audio::decode_mono(path, None).unwrap();
    let at = audio.samples.iter().position(|s| s.abs() > 0.2).unwrap();
    #[allow(clippy::cast_precision_loss, reason = "a sample index")]
    let ms = at as f64 * 1000.0 / f64::from(audio.sample_rate);
    ms
}

#[test]
fn the_fixtures_are_one_mp3_with_the_lame_lead_and_one_without() {
    let lead = |name: &str| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        rbl_core::mpeg::rekordbox_lead_frames(&mut std::fs::File::open(path).unwrap())
    };
    assert_eq!(lead("click-lame.mp3"), 1152);
    assert_eq!(lead("click-plain.mp3"), 0);
}

#[test]
fn dropped_files_land_analysed_and_cued_with_the_grid_on_rekordbox_s_timeline() {
    let b = bench();
    let lame = song(&b, "click-lame.mp3");
    let plain = song(&b, "click-plain.mp3");
    let wav = b.dir.path().join("music/click.wav");
    testing::wav(&wav, 3).unwrap();
    let before = b.session.search("", 100).unwrap().len();

    // A folder is searched for audio; anything else in it is left alone.
    std::fs::write(b.dir.path().join("music/notes.txt"), "not audio").unwrap();
    let report = b.session.import_files(&[b.dir.path().join("music")]).unwrap();
    assert_eq!(report.imported.len(), 3, "{report:?}");
    assert_eq!(report.problems, []);
    assert_eq!(report.commits.len(), 2, "the import, then the cues");
    assert_eq!(b.session.search("", 100).unwrap().len(), before + 3);
    let history: Vec<String> = b.session.history(2).unwrap().into_iter().map(|e| e.summary).collect();
    assert_eq!(history, ["Apply rules to 3 tracks", "Import 3 tracks"]);

    for row in &report.imported {
        assert!(row.hot.contains('A'), "{} has the rules' start cue: {:?}", row.title, row.hot);
        assert!(!row.missing);
        // Read back as kfbox reads any library track: on the grid it now has.
        let summary = b.session.track_summary(&row.id, false).unwrap();
        assert_eq!(summary.analysis.grid, kfbox_core::track::GridSource::Rekordbox, "{}", row.title);
        assert!((summary.analysis.bpm - 120.0).abs() < 0.01, "{}: {}", row.title, summary.analysis.bpm);
    }
    assert_eq!(report.imported.iter().find(|r| Path::new(&r.path) == lame).unwrap().title, "Click Lame", "tags are read");

    // The grid starts on the first click as rekordbox plays it, for both files.
    let (lame_beat, plain_beat) = (first_beat_ms(&b, &lame), first_beat_ms(&b, &plain));
    for (path, beat) in [(&lame, lame_beat), (&plain, plain_beat)] {
        let click = first_click_ms(path);
        assert!((f64::from(beat) - click).abs() <= 3.0, "{}: grid {beat} ms, click {click:.1} ms", path.display());
    }
    // And the LAME file's is the lead later: 1,152 samples at 44.1 kHz is 26.1 ms.
    let lead = f64::from(lame_beat) - f64::from(plain_beat);
    assert!((lead - 26.1).abs() <= 2.0, "the LAME lead moved the grid {lead} ms");
}

#[test]
fn files_already_there_and_copies_are_left_out_and_said_why() {
    let b = bench();
    let lame = song(&b, "click-lame.mp3");
    b.session.import_files(std::slice::from_ref(&lame)).unwrap();

    let copy = b.dir.path().join("elsewhere/same song.mp3");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(&lame, &copy).unwrap();
    let again = b.session.import_files(&[lame.clone(), copy.clone()]).unwrap();
    assert_eq!(again.imported, []);
    assert_eq!(again.already, [lame.display().to_string()]);
    assert_eq!(again.duplicates.len(), 1);
    assert_eq!((again.duplicates[0].path.as_str(), again.duplicates[0].kind.as_str()), (copy.to_str().unwrap(), "copy"));
    assert_eq!(again.duplicates[0].of.title, "Click Lame");
    assert_eq!(again.commits, [] as [String; 0], "nothing to write, so nothing in the history");

    // Another encoding with the same artist, title and length is imported, and flagged.
    let plain = song(&b, "click-plain.mp3");
    let bytes = std::fs::read(&plain).unwrap();
    let at = bytes.windows(11).position(|w| w == b"Click Plain").unwrap();
    let mut retagged = bytes.clone();
    retagged[at..at + 11].copy_from_slice(b"Click Lame ");
    let other = b.dir.path().join("music/other encode.mp3");
    std::fs::write(&other, retagged).unwrap();
    let report = b.session.import_files(&[other.clone(), plain]).unwrap();
    assert_eq!(report.imported.len(), 2);
    assert_eq!(report.duplicates.len(), 1);
    let likely = &report.duplicates[0];
    assert_eq!((likely.path.as_str(), likely.kind.as_str(), likely.of.title.as_str()), (other.to_str().unwrap(), "likely", "Click Lame"));
}

#[test]
fn missing_files_are_listed_and_found_again_in_a_folder() {
    let b = bench();
    let lame = song(&b, "click-lame.mp3");
    b.session.import_files(std::slice::from_ref(&lame)).unwrap();
    assert_eq!(b.session.missing_files().unwrap().iter().filter(|r| r.path == lame.to_str().unwrap()).count(), 0);

    let moved = b.dir.path().join("moved/deeper/click-lame.mp3");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&lame, &moved).unwrap();
    let missing = b.session.missing_files().unwrap();
    let ours = missing.iter().find(|r| r.path == lame.to_str().unwrap()).expect("listed as missing");
    assert!(ours.missing);

    let saved = b.session.relocate_missing(&b.dir.path().join("moved")).unwrap();
    assert!(saved.commit.is_some());
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Find 1 missing file");
    let row = b.session.search("click lame", 10).unwrap().pop().unwrap();
    assert_eq!((row.path.as_str(), row.missing), (moved.to_str().unwrap(), false));
    assert!(b.session.missing_files().unwrap().iter().all(|r| r.id != row.id));
}
