//! Learning from fixes: a person moving or deleting a cue the rules or an
//! agent placed is recorded, and scoring counts it. Fixture libraries only.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kfbox_core::learning::Correction;
use kfbox_core::session::{CueEdit, Origin, Session};
use kfbox_core::testing;
use rbl_db::fixture::{self, Shape};
use rbl_db::write::Writer;
use rbl_db::LibraryLocation;

/// A at the start and C four bars in: on the 120 BPM click track, bar 1 and bar 5 (8 s).
const RULES: &str = "[[cue]]\nslot = \"A\"\nat = \"start\"\n[[cue]]\nslot = \"C\"\nat = \"start\"\noffset_bars = 4\n";

struct Bench {
    _dir: tempfile::TempDir,
    location: LibraryLocation,
    session: Session,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let location = fixture::build(&lib, Shape::default()).unwrap();
    for i in 0..2 {
        let song = dir.path().join(format!("song{i}.wav"));
        testing::wav(&song, u8::try_from(i).unwrap()).unwrap();
        fixture::point_at_audio(&location, i, &song.display().to_string(), 30).unwrap();
    }
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.open_db(location.clone()).unwrap();
    Bench { _dir: dir, location, session }
}

fn id(i: usize) -> String {
    fixture::track_id(i)
}

fn cue_id(b: &Bench, track: usize, slot: &str) -> String {
    b.session.track_summary(&id(track), false).unwrap().cues.into_iter().find(|c| c.slot == slot).unwrap().id.unwrap()
}

fn moved(b: &Bench, track: usize, slot: &str, secs: f64) -> CueEdit {
    CueEdit::Move { cue: cue_id(b, track, slot), start_secs: secs }
}

/// Track, slot, from bar, to bar, who placed it, where it was fixed.
type Fix = (String, String, Option<usize>, Option<usize>, String, String);

fn fixes(b: &Bench) -> Vec<Fix> {
    b.session
        .corrections()
        .unwrap()
        .into_iter()
        .map(|Correction { track, slot, from_bar, to_bar, source, via, .. }| (track, slot, from_bar, to_bar, source, via))
        .collect()
}

#[test]
fn moving_and_deleting_rule_cues_by_hand_are_recorded() {
    let b = bench();
    b.session.write_rule_cues(&[id(0)], Some(RULES)).unwrap();
    assert_eq!(fixes(&b), [], "placing isn't a fix");

    b.session.save_cues("Move C", &[moved(&b, 0, "C", 16.0)]).unwrap();
    b.session.save_cues("Delete A", &[CueEdit::Delete { cue: cue_id(&b, 0, "A") }]).unwrap();
    assert_eq!(
        fixes(&b),
        vec![
            (id(0), "C".into(), Some(5), Some(9), "rules".into(), "kfbox".into()),
            (id(0), "A".into(), Some(1), None, "rules".into(), "kfbox".into()),
        ]
    );
    let c = &b.session.corrections().unwrap()[0];
    assert!((c.from_secs - 8.0).abs() < 0.01 && c.to_secs == Some(16.0), "{c:?}");

    // Once fixed, the cue is the person's: moving it again teaches nothing new.
    b.session.save_cues("Move C again", &[moved(&b, 0, "C", 18.0)]).unwrap();
    assert_eq!(fixes(&b).len(), 2);
}

#[test]
fn an_agent_moving_a_rule_cue_is_not_a_fix_but_a_person_moving_it_after_is() {
    let b = bench();
    b.session.write_rule_cues(&[id(0)], Some(RULES)).unwrap();
    b.session.save_cues_as("Agent: move C", &[moved(&b, 0, "C", 12.0)], Origin::Agent).unwrap();
    assert_eq!(fixes(&b), []);
    b.session.save_cues("Move C", &[moved(&b, 0, "C", 16.0)]).unwrap();
    assert_eq!(fixes(&b), vec![(id(0), "C".into(), Some(7), Some(9), "agent".into(), "kfbox".into())]);
}

#[test]
fn an_undone_apply_is_forgotten_not_learned_from() {
    let b = bench();
    b.session.write_rule_cues(&[id(0)], Some(RULES)).unwrap();
    b.session.undo().unwrap();
    let add =
        CueEdit::Add { track: id(0), slot: "C".into(), start_secs: 4.0, end_secs: None, name: String::new(), color: None, replace: false };
    b.session.save_cues("My own C", &[add]).unwrap();
    assert_eq!(fixes(&b), [], "the person's own C isn't a fix of the undone one");
}

#[test]
fn a_fix_made_in_rekordbox_is_found_when_the_app_reads_the_library_again() {
    let b = bench();
    b.session.write_rule_cues(&[id(1)], Some(RULES)).unwrap();
    let cue = cue_id(&b, 1, "C");
    {
        // rekordbox, writing to the file itself.
        let scratch = tempfile::tempdir().unwrap();
        let mut writer = Writer::open(b.location.clone(), scratch.path().join("b")).unwrap();
        writer.disable_automatic_backups();
        writer.move_cue(&cue, 20_000).unwrap();
    }
    assert!(b.session.refresh_if_changed().unwrap().is_some());
    assert_eq!(fixes(&b), vec![(id(1), "C".into(), Some(5), Some(11), "rules".into(), "rekordbox".into())]);
    assert!(b.session.refresh_if_changed().unwrap().is_none());
    assert_eq!(fixes(&b).len(), 1, "found once");
}

#[test]
fn scoring_counts_the_fixes_as_evidence() {
    let b = bench();
    b.session.write_rule_cues(&[id(0)], Some(RULES)).unwrap();
    b.session.save_cues("Move C to bar 9", &[moved(&b, 0, "C", 16.0)]).unwrap();

    let c = |rules: &str| b.session.score_rules(rules).unwrap().slots.into_iter().find(|s| s.slot == "C").unwrap();
    let before = c(RULES);
    assert_eq!((before.fixes, before.fix_hits), (1, 0), "the rules that were fixed don't agree with the fix");
    let after = c(&RULES.replace("offset_bars = 4", "offset_bars = 8"));
    assert_eq!((after.fixes, after.fix_hits), (1, 1), "rules that put C where the person did score the hit");
    assert_eq!(b.session.score_rules(&RULES.replace("offset_bars = 4", "offset_bars = 8")).unwrap().hits(), 1);
}
