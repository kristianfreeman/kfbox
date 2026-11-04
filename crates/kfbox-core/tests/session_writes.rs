//! Writes through the session, on throwaway fixture libraries.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kfbox_core::safe::SafeLibrary;
use kfbox_core::session::{CommandError, CueEdit, Session};
use kfbox_core::testing;
use rbl_db::fixture::{self, Shape};
use rbl_db::LibraryLocation;

struct Bench {
    dir: tempfile::TempDir,
    location: LibraryLocation,
    session: Session,
}

/// A fixture library whose first three tracks are real (made-up) audio files.
fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let location = fixture::build(&lib, Shape::default()).unwrap();
    for i in 0..3 {
        let song = dir.path().join(format!("song{i}.wav"));
        testing::wav(&song, u8::try_from(i).unwrap()).unwrap();
        fixture::point_at_audio(&location, i, &song.display().to_string(), 30).unwrap();
    }
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.open_db(location.clone()).unwrap();
    Bench { dir, location, session }
}

fn id(i: usize) -> String {
    fixture::track_id(i)
}

fn add(track: usize, slot: &str, secs: f64, name: &str) -> CueEdit {
    CueEdit::Add { track: id(track), slot: slot.into(), start_secs: secs, end_secs: None, name: name.into(), color: None, replace: false }
}

fn hot(b: &Bench, track: usize) -> String {
    b.session.search("", 100).unwrap().into_iter().find(|r| r.id == id(track)).unwrap().hot
}

fn cue_id(b: &Bench, track: usize, slot: &str) -> String {
    b.session.track_summary(&id(track), false).unwrap().cues.into_iter().find(|c| c.slot == slot).unwrap().id.unwrap()
}

#[test]
fn a_save_lands_and_the_session_reads_it_back() {
    let b = bench();
    let saved = b.session.save_cues("Set cues", &[add(0, "A", 1.0, "start"), add(0, "C", 8.0, "drop")]).unwrap();
    assert!(saved.commit.is_some());
    assert_eq!(saved.added.len(), 2);
    assert_eq!(saved.changed.iter().map(|r| (r.id.clone(), r.hot.clone())).collect::<Vec<_>>(), vec![(id(0), "AC".to_owned())]);
    let summary = b.session.track_summary(&id(0), false).unwrap();
    let c = summary.cues.iter().find(|c| c.slot == "C").unwrap();
    assert_eq!((c.start_secs, c.name.as_str(), c.existing), (8.0, "drop", true));
    assert!(c.id.is_some(), "a saved cue can be edited, so it has its ID");
}

#[test]
fn edits_move_rename_recolour_replace_and_delete() {
    let b = bench();
    b.session.save_cues("Set", &[add(1, "A", 1.0, "one"), add(1, "memory", 3.0, "mem")]).unwrap();
    let a = cue_id(&b, 1, "A");
    b.session
        .save_cues(
            "Edit",
            &[
                CueEdit::Move { cue: a.clone(), start_secs: 2.5 },
                CueEdit::Rename { cue: a.clone(), name: "renamed".into() },
                CueEdit::Recolour { cue: a.clone(), color: Some("#ff0000".into()) },
            ],
        )
        .unwrap();
    let summary = b.session.track_summary(&id(1), false).unwrap();
    let cue = summary.cues.iter().find(|c| c.slot == "A").unwrap();
    assert_eq!((cue.start_secs, cue.name.as_str(), cue.color.as_deref()), (2.5, "renamed", Some("#ff0000")));

    // A taken slot is refused unless the edit says to replace it.
    let taken = b.session.save_cues("Clash", &[add(1, "A", 5.0, "x")]);
    assert!(matches!(taken, Err(CommandError::Safe(_))), "{taken:?}");
    let replace =
        CueEdit::Add { track: id(1), slot: "A".into(), start_secs: 5.0, end_secs: None, name: "new".into(), color: None, replace: true };
    b.session.save_cues("Replace A", &[replace]).unwrap();
    let cues = b.session.track_summary(&id(1), false).unwrap().cues;
    assert_eq!(cues.iter().filter(|c| c.slot == "A").count(), 1);
    assert_eq!(cues.iter().find(|c| c.slot == "A").unwrap().start_secs, 5.0);

    let memory = cue_id(&b, 1, "memory");
    b.session.save_cues("Delete", &[CueEdit::Delete { cue: memory }]).unwrap();
    assert!(b.session.track_summary(&id(1), false).unwrap().cues.iter().all(|c| c.slot != "memory"));

    assert!(matches!(b.session.save_cues("Bad slot", &[add(1, "Z", 1.0, "")]), Err(CommandError::Refused(_))));
    assert!(matches!(b.session.save_cues("Nothing", &[]), Err(CommandError::Refused(_))));
}

#[test]
fn a_loop_saves_with_its_end() {
    let b = bench();
    let edit = CueEdit::Add {
        track: id(2),
        slot: "D".into(),
        start_secs: 4.0,
        end_secs: Some(12.0),
        name: "groove".into(),
        color: Some("#0000ff".into()),
        replace: false,
    };
    b.session.save_cues("Loop", &[edit]).unwrap();
    let d = b.session.track_summary(&id(2), false).unwrap().cues.into_iter().find(|c| c.slot == "D").unwrap();
    assert_eq!((d.start_secs, d.end_secs, d.color.as_deref()), (4.0, Some(12.0), Some("#0000ff")));
}

#[test]
fn the_rules_write_straight_in_as_one_change() {
    let b = bench();
    let rules = "[[cue]]\nslot = \"A\"\nat = \"start\"\nname = \"start\"\n[[cue]]\nslot = \"memory\"\nat = \"start\"\noffset_bars = 4\n";
    let saved = b.session.write_rule_cues(&[id(0), id(1)], Some(rules)).unwrap();
    assert_eq!(saved.added.len(), 4);
    assert_eq!(saved.changed.len(), 2);
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Apply rules to 2 tracks");

    // Applying again adds nothing: the slots are filled.
    let again = b.session.write_rule_cues(&[id(0)], Some(rules)).unwrap();
    assert!(again.commit.is_none() && again.added.is_empty());
}

#[test]
fn undo_redo_and_restore_through_the_session() {
    let b = bench();
    assert!(matches!(b.session.redo(), Err(CommandError::Safe(_))));
    b.session.save_cues("One", &[add(0, "A", 1.0, "")]).unwrap();
    b.session.save_cues("Two", &[add(0, "B", 2.0, "")]).unwrap();
    assert_eq!(hot(&b, 0), "AB");

    let undone = b.session.undo().unwrap();
    assert_eq!(undone.changed.iter().map(|r| r.hot.clone()).collect::<Vec<_>>(), vec!["A".to_owned()]);
    assert_eq!(hot(&b, 0), "A");
    b.session.redo().unwrap();
    assert_eq!(hot(&b, 0), "AB");
    assert!(matches!(b.session.redo(), Err(CommandError::Safe(_))), "only an undo can be redone");

    let history = b.session.history(10).unwrap();
    let summaries: Vec<&str> = history.iter().map(|e| e.summary.as_str()).collect();
    assert_eq!(summaries, vec!["Redo: Two", "Undo: Two", "Two", "One", "Library as kfbox first saw it"]);
    let first = history.last().unwrap().commit.clone();
    b.session.restore(&first).unwrap();
    assert_eq!(hot(&b, 0), "");
}

#[test]
fn the_lock_is_held_only_while_writing() {
    let b = bench();
    b.session.save_cues("One", &[add(0, "A", 1.0, "")]).unwrap();
    // Another writer, as an agent would be, gets in between the app's writes.
    {
        let other = SafeLibrary::open(b.location.clone(), &b.dir.path().join("app/safe")).unwrap();
        other.history(1).unwrap();
    }
    b.session.save_cues("Two", &[add(0, "B", 2.0, "")]).unwrap();
}

#[test]
fn a_rekordbox_xml_is_never_written() {
    let b = bench();
    let xml = testing::collection(b.dir.path()).unwrap();
    b.session.open_xml(&xml).unwrap();
    let refused = b.session.save_cues(
        "x",
        &[CueEdit::Add {
            track: "2".into(),
            slot: "B".into(),
            start_secs: 1.0,
            end_secs: None,
            name: String::new(),
            color: None,
            replace: false,
        }],
    );
    assert!(matches!(refused, Err(CommandError::Refused(_))));
}
