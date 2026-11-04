//! A USB export to a temp folder, read back with rbxport's own readers the
//! way a CDJ reads a stick: `export.pdb` for the tracks and playlists, each
//! track's `.DAT` and `.EXT` for its cues, colours and loops. Never a real
//! drive, and never the real library.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, reason = "tests")]

use std::path::Path;

use kfbox_core::safe::hot_colour_index;
use kfbox_core::session::{CueEdit, Session};
use kfbox_core::testing;
use rbl_anlz::{Anlz, CueEntry};
use rbl_db::fixture::{self, Shape};
use rbl_pdb::{PageType, Pdb};

struct Bench {
    dir: tempfile::TempDir,
    session: Session,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let mut location = fixture::build(&lib, Shape { tracks: 4, ..Shape::default() }).unwrap();
    location.share_root = lib.join("share");
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.open_db(location).unwrap();
    Bench { dir, session }
}

fn playlist(b: &Bench, path: &str) -> usize {
    b.session.fresh_listing().unwrap().playlists.iter().find(|p| p.path == path).unwrap().index
}

/// Every cue entry in a track's `.DAT` and `.EXT` on the stick, without the
/// copies the `.EXT`'s extended lists hold of the same cues.
fn cues_on_stick(stick: &Path, analyze_path: &str) -> Vec<CueEntry> {
    let dat = stick.join(analyze_path.trim_start_matches('/'));
    let mut cues = Anlz::read(&dat).unwrap().cue_entries();
    let ext = Anlz::read(&dat.with_extension("EXT")).unwrap().cue_entries();
    for cue in ext {
        if !cues.iter().any(|c| c.hot_cue == cue.hot_cue && c.time_ms == cue.time_ms) {
            cues.push(cue);
        }
    }
    cues
}

#[test]
fn a_stick_holds_the_playlist_its_tracks_and_their_cues_colours_and_loops() {
    let b = bench();
    let music = b.dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    for i in 0..3_u8 {
        testing::wav(&music.join(format!("song {i}.wav")), i).unwrap();
    }
    let imported = b.session.import_files(&[music]).unwrap().imported;
    let ids: Vec<String> = imported.iter().map(|r| r.id.clone()).collect();
    b.session.create_playlist("Gig", None, &ids).unwrap();
    b.session.set_rating(&ids[..1], 5).unwrap();

    // On the first track: a red B, a four-beat purple loop on C, and a memory cue.
    let first = &ids[0];
    let add = |slot: &str, start: f64, end: Option<f64>, name: &str, color: Option<&str>| CueEdit::Add {
        track: first.clone(),
        slot: slot.into(),
        start_secs: start,
        end_secs: end,
        name: name.into(),
        color: color.map(str::to_owned),
        replace: true,
    };
    b.session
        .save_cues(
            "Set B, a loop on C, and a memory cue",
            &[
                add("B", 4.0, None, "build", Some("#ff0000")),
                add("C", 8.0, Some(10.0), "loop", Some("#9900ff")),
                add("memory", 12.0, None, "", None),
            ],
        )
        .unwrap();

    let stick = tempfile::tempdir().unwrap();
    let mut stages = Vec::new();
    let report = b
        .session
        .export_usb(stick.path(), &[playlist(&b, "Gig")], Some("msdos"), &mut |p| stages.push(p.stage.clone()), &|| false)
        .unwrap();
    assert_eq!((report.tracks, report.playlists, report.failed.len()), (3, 1, 0), "{report:?}");
    assert!(stages.iter().any(|s| s == "copying"), "progress is reported: {stages:?}");

    // The database, as a player reads it.
    let bytes = std::fs::read(stick.path().join("PIONEER/rekordbox/export.pdb")).unwrap();
    let pdb = Pdb::parse(&bytes).unwrap();
    let tracks = pdb.track_rows(pdb.table(PageType::Tracks).unwrap());
    assert_eq!(tracks.len(), 3);
    let nodes = pdb.playlist_nodes(pdb.table(PageType::PlaylistTree).unwrap());
    assert_eq!(nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["Gig"]);
    let entries = pdb.playlist_entries(pdb.table(PageType::PlaylistEntries).unwrap());
    assert_eq!(entries.len(), 3);
    for track in &tracks {
        assert!(stick.path().join(track.file_path.trim_start_matches('/')).is_file(), "{} has its audio", track.title);
    }
    let first_row =
        tracks.iter().find(|t| u64::from(t.master_content_id) == first.parse::<u64>().unwrap()).expect("the stick knows the library's ID");
    assert_eq!(first_row.rating, 5);
    assert!((f64::from(first_row.tempo_x100) / 100.0 - 120.0).abs() < 0.01);

    // The analysis, cues and all.
    let cues = cues_on_stick(stick.path(), &first_row.analyze_path);
    let hot = |slot: u32| cues.iter().find(|c| c.hot_cue == slot).unwrap_or_else(|| panic!("no hot cue {slot} in {cues:?}"));
    assert_eq!(hot(1).time_ms, 0, "A, the rules' start");
    let b_cue = hot(2);
    assert_eq!((b_cue.time_ms, b_cue.kind, b_cue.color_code), (4000, 1, hot_colour_index([255, 0, 0])));
    assert_eq!(b_cue.comment.as_deref(), Some("build"));
    let c_cue = hot(3);
    assert_eq!((c_cue.time_ms, c_cue.kind, c_cue.loop_time_ms), (8000, 2, 10_000), "C is a loop to 10 s");
    assert_eq!(c_cue.color_code, hot_colour_index([0x99, 0x00, 0xff]));
    assert!(cues.iter().any(|c| c.hot_cue == 0 && c.time_ms == 12_000), "the memory cue: {cues:?}");

    // And the grid: the analysis went with it.
    let dat = Anlz::read(&stick.path().join(first_row.analyze_path.trim_start_matches('/'))).unwrap();
    assert!(dat.beat_grid().is_some_and(|g| g.len() > 50));

    let check = rbl_export::verify(stick.path()).unwrap();
    assert!(check.is_ok(), "{check:?}");
    assert_eq!((check.audio_present, check.analysis_present), (3, 3));
}

#[test]
fn exporting_again_syncs_rather_than_rewrites() {
    let b = bench();
    let music = b.dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    testing::wav(&music.join("one.wav"), 1).unwrap();
    testing::wav(&music.join("two.wav"), 2).unwrap();
    let ids: Vec<String> = b.session.import_files(&[music]).unwrap().imported.into_iter().map(|r| r.id).collect();
    b.session.create_playlist("Gig", None, &ids).unwrap();
    let stick = tempfile::tempdir().unwrap();
    let export = || b.session.export_usb(stick.path(), &[playlist(&b, "Gig")], None, &mut |_| {}, &|| false).unwrap();
    assert_eq!(export().tracks, 2);

    b.session.remove_from_playlist(playlist(&b, "Gig"), &ids[1..]).unwrap();
    let again = export();
    assert_eq!((again.tracks, again.reused, again.removed), (1, 1, 1), "{again:?}");
}

#[test]
fn it_refuses_to_write_anywhere_but_a_folder_outside_the_library() {
    let b = bench();
    let lib = b.dir.path().join("lib");
    b.session.create_playlist("Empty", None, &[]).unwrap();
    let into_library = b.session.export_usb(&lib.join("share"), &[playlist(&b, "Empty")], None, &mut |_| {}, &|| false);
    assert!(into_library.unwrap_err().to_string().contains("rekordbox's own library"));
    let nowhere = b.session.export_usb(&b.dir.path().join("no such drive"), &[], None, &mut |_| {}, &|| false);
    assert!(nowhere.unwrap_err().to_string().contains("isn't a folder"));
    let stick = tempfile::tempdir().unwrap();
    let empty = b.session.export_usb(stick.path(), &[playlist(&b, "Empty")], None, &mut |_| {}, &|| false);
    assert!(empty.unwrap_err().to_string().contains("no tracks"));
    assert_eq!(std::fs::read_dir(stick.path()).unwrap().count(), 0, "nothing written");
}
