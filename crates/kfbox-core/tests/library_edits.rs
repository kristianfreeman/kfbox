//! Ratings, My Tags, playlists and grid edits through the safe path, on
//! fixture libraries only. rbl-db's fixture has one My Tag column,
//! "Situation", holding "Peak" and "Warm-up".

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::Path;

use kfbox_core::library::{GridEdit, TagMode};
use kfbox_core::session::{Origin, Session, TrackRow};
use kfbox_core::testing;
use kfbox_core::tools::{self, Toolset};
use rbl_db::fixture::{self, Shape};
use rbl_db::LibraryLocation;
use serde_json::json;
use sha2::{Digest, Sha256};

struct Bench {
    dir: tempfile::TempDir,
    location: LibraryLocation,
    session: Session,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let mut location = fixture::build(&lib, Shape { tracks: 8, ..Shape::default() }).unwrap();
    location.share_root = lib.join("share");
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.open_db(location.clone()).unwrap();
    Bench { dir, location, session }
}

fn id(i: usize) -> String {
    fixture::track_id(i)
}

fn row(b: &Bench, id: &str) -> TrackRow {
    b.session.search("", 500).unwrap().into_iter().find(|r| r.id == id).unwrap()
}

fn sha(path: &Path) -> Vec<u8> {
    Sha256::digest(std::fs::read(path).unwrap()).to_vec()
}

#[test]
fn my_tags_are_the_library_s_own_and_nothing_else() {
    let b = bench();
    let columns = b.session.my_tags().unwrap();
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].name, "Situation");
    assert_eq!(columns[0].tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Peak", "Warm-up"]);

    let refused = b.session.set_my_tags(&[id(0)], &["Mood: Dark".into()], TagMode::Add, Origin::Hand).unwrap_err().to_string();
    assert!(refused.contains("Situation: Peak, Situation: Warm-up"), "the refusal lists what there is: {refused}");

    b.session.set_my_tags(&[id(0), id(1)], &["situation: peak".into()], TagMode::Add, Origin::Hand).unwrap();
    assert_eq!(row(&b, &id(0)).my_tags, ["Situation: Peak"]);
    b.session.set_my_tags(&[id(0)], &["Situation: Warm-up".into()], TagMode::Add, Origin::Hand).unwrap();
    assert_eq!(row(&b, &id(0)).my_tags, ["Situation: Peak", "Situation: Warm-up"]);
    b.session.set_my_tags(&[id(0)], &["Situation: Peak".into()], TagMode::Remove, Origin::Hand).unwrap();
    assert_eq!(row(&b, &id(0)).my_tags, ["Situation: Warm-up"]);
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Tag 1 track");

    b.session.undo().unwrap();
    assert_eq!(row(&b, &id(0)).my_tags, ["Situation: Peak", "Situation: Warm-up"]);
}

#[test]
fn an_agent_tags_through_its_tool_with_existing_values_only() {
    let b = bench();
    let names: Vec<String> = tools::definitions(Toolset::Agent).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
    assert!(names.contains(&"set_my_tags".to_owned()) && names.contains(&"list_my_tags".to_owned()));

    let listed = tools::call(&b.session, Toolset::Agent, "list_my_tags", &json!({}), "Agent").unwrap().unwrap();
    assert_eq!(listed[0]["tags"][1]["name"], "Warm-up");
    let args = json!({ "ids": [id(2)], "tags": ["Situation: Warm-up"], "mode": "set" });
    tools::call(&b.session, Toolset::Agent, "set_my_tags", &args, "Agent").unwrap().unwrap();
    assert_eq!(row(&b, &id(2)).my_tags, ["Situation: Warm-up"]);
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Agent: Tag 1 track");

    let invented = json!({ "ids": [id(2)], "tags": ["Situation: Afterparty"] });
    assert!(tools::call(&b.session, Toolset::Agent, "set_my_tags", &invented, "Agent").unwrap().unwrap_err().contains("no My Tag"));
}

#[test]
fn a_rating_and_its_undo_leave_master_db_byte_for_byte_as_it_was() {
    let b = bench();
    // The first write records the library as kfbox first saw it.
    b.session.set_rating(&[id(4)], 2).unwrap();
    let before = sha(&b.location.master_db);
    let mode = std::fs::metadata(&b.location.master_db).unwrap().permissions();

    b.session.set_rating(&[id(3), id(4)], 4).unwrap();
    assert_eq!((row(&b, &id(3)).rating, row(&b, &id(4)).rating), (4, 4));
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Rate 2 tracks ★★★★");
    assert_ne!(sha(&b.location.master_db), before);

    b.session.undo().unwrap();
    assert_eq!(sha(&b.location.master_db), before, "undo puts back the very bytes");
    assert_eq!(std::fs::metadata(&b.location.master_db).unwrap().permissions(), mode);
    assert_eq!(row(&b, &id(4)).rating, 2);
    assert!(b.session.set_rating(&[id(3)], 6).is_err());
}

#[test]
fn playlists_are_made_filled_emptied_renamed_and_deleted_each_undoably() {
    let b = bench();
    let listing = || b.session.fresh_listing().unwrap();
    let index_of = |path: &str| listing().playlists.iter().find(|p| p.path == path).map(|p| p.index);
    let members = |path: &str| b.session.playlist_rows(index_of(path)).unwrap().into_iter().map(|r| r.id).collect::<Vec<_>>();

    b.session.create_playlist("Tonight", None, &[id(5), id(6)]).unwrap();
    assert_eq!(members("Tonight"), [id(5), id(6)]);
    b.session.add_to_playlist(index_of("Tonight").unwrap(), &[id(7)]).unwrap();
    assert_eq!(members("Tonight"), [id(5), id(6), id(7)]);
    b.session.remove_from_playlist(index_of("Tonight").unwrap(), &[id(6)]).unwrap();
    assert_eq!(members("Tonight"), [id(5), id(7)]);
    b.session.rename_playlist(index_of("Tonight").unwrap(), "Saturday").unwrap();
    assert_eq!(index_of("Tonight"), None);
    assert_eq!(members("Saturday"), [id(5), id(7)]);
    b.session.delete_playlist(index_of("Saturday").unwrap()).unwrap();
    assert_eq!(index_of("Saturday"), None);

    let summaries: Vec<String> = b.session.history(5).unwrap().into_iter().map(|e| e.summary).collect();
    assert_eq!(
        summaries,
        [
            "Delete playlist Saturday",
            "Rename Tonight to Saturday",
            "Remove 1 track from Tonight",
            "Add 1 track to Tonight",
            "New playlist Tonight"
        ]
    );
    b.session.undo().unwrap();
    assert_eq!(members("Saturday"), [id(5), id(7)], "undo brings the playlist back");
}

#[test]
fn a_grid_edit_rewrites_the_analysis_file_and_the_bpm_and_moves_no_cue() {
    let b = bench();
    let song = b.dir.path().join("music/song.wav");
    std::fs::create_dir_all(song.parent().unwrap()).unwrap();
    testing::wav(&song, 2).unwrap();
    let imported = b.session.import_files(std::slice::from_ref(&song)).unwrap().imported.remove(0);
    let first = |b: &Bench| b.session.track_summary(&imported.id, false).unwrap();
    let before = first(&b);
    let cues_before: Vec<f64> = before.cues.iter().map(|c| c.start_secs).collect();
    let downbeat = before.first_downbeat_secs.unwrap();

    b.session.edit_grid(&imported.id, GridEdit::Shift { ms: 20.0 }).unwrap();
    let shifted = first(&b);
    assert!((shifted.first_downbeat_secs.unwrap() - (downbeat + 0.020)).abs() < 0.0015, "{:?}", shifted.first_downbeat_secs);
    assert_eq!(shifted.cues.iter().map(|c| c.start_secs).collect::<Vec<_>>(), cues_before, "cues stay put");

    b.session.edit_grid(&imported.id, GridEdit::Bpm { bpm: 124.0 }).unwrap();
    assert!((first(&b).analysis.bpm - 124.0).abs() < 0.01);
    assert!((row(&b, &imported.id).bpm - 124.0).abs() < 0.01, "the row's BPM follows");
    assert!(b.session.history(1).unwrap()[0].summary.starts_with("Grid of "));

    // Back to before both edits: the history holds the analysis files too.
    let cued = b.session.history(10).unwrap().into_iter().find(|e| e.summary == "Apply rules to 1 track").unwrap();
    b.session.restore(&cued.commit).unwrap();
    let restored = first(&b);
    assert!((restored.first_downbeat_secs.unwrap() - downbeat).abs() < 0.0015, "restoring puts the grid file back");
    assert!((restored.analysis.bpm - before.analysis.bpm).abs() < 0.01);
    assert!(b.session.edit_grid(&imported.id, GridEdit::Bpm { bpm: 900.0 }).is_err());
}
