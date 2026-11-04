//! Auto-cue on fixture libraries: uncued tracks get the rules' cues, hand
//! cues and Kristian's fixes are left alone, and nothing is written while
//! rekordbox is open. Never a real library.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kfbox_core::session::{CueEdit, Session};
use kfbox_core::testing;
use rbl_db::fixture::{self, Shape};

/// A at the start and C four bars in: on the 120 BPM click track, 0 s and 8 s.
const RULES: &str = "[[cue]]\nslot = \"A\"\nat = \"start\"\n[[cue]]\nslot = \"C\"\nat = \"start\"\noffset_bars = 4\n";

struct Bench {
    _dir: tempfile::TempDir,
    session: Session,
}

/// Six fixture tracks, the first three with real (made-up) audio; the rest
/// point at files that aren't there.
fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let location = fixture::build(&lib, Shape { tracks: 6, ..Shape::default() }).unwrap();
    for i in 0..3 {
        let song = dir.path().join(format!("song{i}.wav"));
        testing::wav(&song, u8::try_from(i).unwrap()).unwrap();
        fixture::point_at_audio(&location, i, &song.display().to_string(), 30).unwrap();
    }
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.save_rules(RULES).unwrap();
    session.open_db(location).unwrap();
    Bench { _dir: dir, session }
}

fn id(i: usize) -> String {
    fixture::track_id(i)
}

fn hot(b: &Bench, track: usize) -> String {
    b.session.search("", 100).unwrap().into_iter().find(|r| r.id == id(track)).unwrap().hot
}

fn start_of(b: &Bench, track: usize, slot: &str) -> f64 {
    b.session.track_summary(&id(track), false).unwrap().cues.into_iter().find(|c| c.slot == slot).unwrap().start_secs
}

fn run(b: &Bench) -> kfbox_core::autocue::AutoCueOutcome {
    let ids = b.session.auto_cue_candidates().unwrap();
    b.session.auto_cue(&ids, &mut |_| {}, &|| false).unwrap()
}

#[test]
fn on_open_every_uncued_track_with_its_file_gets_the_rules_cues_in_one_change() {
    let b = bench();
    assert!(b.session.auto_cue_enabled(), "on unless turned off");
    assert_eq!(b.session.auto_cue_candidates().unwrap(), [id(0), id(1), id(2)], "the tracks with no file are left out");
    let mut seen = Vec::new();
    let outcome = b.session.auto_cue(&b.session.auto_cue_candidates().unwrap(), &mut |p| seen.push(p.clone()), &|| false).unwrap();
    assert_eq!((outcome.cued, outcome.waiting, outcome.commits.len()), (3, 0, 1));
    assert_eq!(seen.last().map(|p| (p.done, p.total, p.cued)), Some((3, 3, 3)));
    for track in 0..3 {
        assert_eq!(hot(&b, track), "AC");
    }
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Auto-cue 3 tracks");
    assert_eq!(b.session.auto_cue_candidates().unwrap(), [] as [String; 0], "nothing left to do");

    // One undo takes the whole batch back.
    b.session.undo().unwrap();
    assert_eq!(hot(&b, 0), "");
}

#[test]
fn a_hand_cue_stays_where_it_is() {
    let b = bench();
    let mine =
        CueEdit::Add { track: id(1), slot: "A".into(), start_secs: 2.0, end_secs: None, name: "mine".into(), color: None, replace: false };
    b.session.save_cues("My A", &[mine]).unwrap();
    run(&b);
    assert_eq!(hot(&b, 1), "AC");
    assert!((start_of(&b, 1, "A") - 2.0).abs() < 0.001, "the hand cue wasn't moved or replaced");
}

#[test]
fn a_placed_cue_deleted_or_undone_by_hand_is_not_put_back() {
    let b = bench();
    run(&b);
    let c = b.session.track_summary(&id(0), false).unwrap().cues.into_iter().find(|c| c.slot == "C").unwrap().id.unwrap();
    b.session.save_cues("Delete C", &[CueEdit::Delete { cue: c }]).unwrap();
    // A move is kept where it was moved to.
    let a = b.session.track_summary(&id(2), false).unwrap().cues.into_iter().find(|c| c.slot == "A").unwrap().id.unwrap();
    b.session.save_cues("Move A", &[CueEdit::Move { cue: a, start_secs: 4.0 }]).unwrap();

    // Next launch.
    assert_eq!(b.session.auto_cue_candidates().unwrap(), [] as [String; 0]);
    assert_eq!(run(&b).commits, [] as [String; 0], "nothing to write");
    assert_eq!(hot(&b, 0), "A");
    assert!((start_of(&b, 2, "A") - 4.0).abs() < 0.001);

    // Apply rules, asked for by hand, still fills the slot.
    b.session.write_rule_cues(&[id(0)], None).unwrap();
    assert_eq!(hot(&b, 0), "AC");
}

#[test]
fn an_undone_auto_cue_batch_stays_undone() {
    let b = bench();
    run(&b);
    b.session.undo().unwrap();
    assert_eq!(b.session.auto_cue_candidates().unwrap(), [] as [String; 0], "the undo said no to those slots");
    assert_eq!(hot(&b, 1), "");
}

#[test]
fn with_rekordbox_open_the_tracks_wait_then_get_their_cues_when_it_closes() {
    let b = bench();
    let open = Arc::new(AtomicBool::new(true));
    let running = Arc::clone(&open);
    b.session.set_running_check(move || running.load(Ordering::SeqCst));
    let before = b.session.history(10).map_or(0, |h| h.len());

    let waiting = run(&b);
    assert_eq!((waiting.cued, waiting.waiting), (0, 3));
    assert_eq!(hot(&b, 0), "");
    assert_eq!(b.session.history(10).map_or(0, |h| h.len()), before, "nothing written");

    open.store(false, Ordering::SeqCst);
    let done = run(&b);
    assert_eq!((done.cued, done.waiting), (3, 0));
    assert_eq!(hot(&b, 0), "AC");
}

#[test]
fn the_setting_turns_it_off_and_stays_off() {
    let b = bench();
    b.session.set_auto_cue(false).unwrap();
    assert!(!b.session.auto_cue_enabled());
    b.session.set_auto_cue(true).unwrap();
    assert!(b.session.auto_cue_enabled());
}
