//! Every command, run with no front end, on a made-up collection.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kfbox_core::session::{CommandError, Session};
use kfbox_core::testing;

struct Bench {
    dir: tempfile::TempDir,
    session: Session,
    xml: std::path::PathBuf,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let xml = testing::collection(dir.path()).unwrap();
    let session = Session::open(&dir.path().join("app")).unwrap();
    Bench { dir, session, xml }
}

#[test]
fn nothing_answers_before_a_library_is_open() {
    let b = bench();
    assert!(matches!(b.session.search("", 10), Err(CommandError::NotOpen)));
    assert!(matches!(b.session.analyse_track("1"), Err(CommandError::NotOpen)));
}

#[test]
fn an_xml_lists_its_collection_and_playlists() {
    let b = bench();
    let listing = b.session.open_xml(&b.xml).unwrap();
    assert_eq!(listing.total, 3);
    let paths: Vec<&str> = listing.playlists.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(paths, vec!["Sets", "Sets › Warm up"]);
    assert!(listing.playlists[0].folder);
    assert_eq!(listing.playlists[1].count, 2);
}

#[test]
fn playlists_come_in_their_order_and_the_collection_by_artist() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    let all: Vec<String> = b.session.load_playlist(None).unwrap().into_iter().map(|r| r.title).collect();
    assert_eq!(all, vec!["Alpha", "Beta", "Gamma"]);
    let warm = b.session.find_playlist("warm up").unwrap();
    assert_eq!(b.session.find_playlist("Sets › Warm up").unwrap(), warm);
    let titles: Vec<String> = b.session.load_playlist(Some(warm)).unwrap().into_iter().map(|r| r.title).collect();
    assert_eq!(titles, vec!["Beta", "Alpha"]);
    assert!(matches!(b.session.find_playlist("Sets"), Err(CommandError::NoPlaylist(_))), "a folder isn't a playlist");
    assert!(matches!(b.session.playlist_rows(Some(99)), Err(CommandError::NoPlaylist(_))));
}

#[test]
fn search_needs_every_word_and_respects_the_limit() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    let titles = |q: &str, limit| b.session.search(q, limit).unwrap().into_iter().map(|r| r.title).collect::<Vec<_>>();
    assert_eq!(titles("second gam", 10), vec!["Gamma"]);
    assert_eq!(titles("techno", 2).len(), 2);
    assert!(titles("nothing like this", 10).is_empty());
    let alpha = b.session.search("alpha", 1).unwrap().remove(0);
    assert_eq!((alpha.hot.as_str(), alpha.bpm), ("A", 120.0));
}

#[test]
fn analysis_is_kept_cached_and_read_back_whole() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    let first = b.session.analyse_track("2").unwrap();
    assert!(!first.cached);
    assert_eq!(first.title, "Beta");
    let track = b.session.track(&first.path).unwrap();
    assert!(track.bars.len() > 10);

    // Another session on the same folder uses the same cache.
    let other = Session::open(&b.dir.path().join("app")).unwrap();
    other.open_xml(&b.xml).unwrap();
    assert!(other.analyse_track("2").unwrap().cached);
    assert!(matches!(other.track("/no/such.wav"), Err(CommandError::NotLoaded(_))));
    assert!(matches!(other.analyse_track("99"), Err(CommandError::NoTrack(_))));
}

#[test]
fn rules_place_cues_and_save_only_when_asked() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    let path = b.session.analyse_track("1").unwrap().path;
    let rules = Session::default_rules();
    let applied = b.session.apply_rules_to(&path, &rules).unwrap();
    assert!(applied.cues.iter().any(|c| c.existing && c.hot == Some(0)), "Alpha's own A is kept");

    let all = b.session.apply_rules(&rules, false).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(b.session.load_rules(), rules, "nothing saved yet, so the defaults");
    let edited = rules.replace("name = \"drop\"", "name = \"the drop\"");
    b.session.apply_rules(&edited, true).unwrap();
    assert_eq!(b.session.load_rules(), edited);
    assert!(b.session.apply_rules("[[cue]]\nslot = \"Z\"\nat = \"start\"\n", true).is_err());
    assert_eq!(b.session.load_rules(), edited, "bad rules aren't saved");
}

#[test]
fn a_summary_and_a_proposal_write_nothing() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    let before = std::fs::read(&b.xml).unwrap();

    let summary = b.session.track_summary("1", true).unwrap();
    assert_eq!(summary.row.title, "Alpha");
    assert_eq!(summary.cues.len(), 1);
    assert_eq!((summary.cues[0].slot.as_str(), summary.cues[0].start_secs, summary.cues[0].bar), ("A", 2.0, 2));
    assert!(summary.cues[0].existing);
    assert_eq!(summary.levels.as_ref().map(Vec::len), Some(summary.bars));
    assert_eq!(b.session.track_summary("1", false).unwrap().levels, None);

    // A click track has a start but no drop, so the default rules say so.
    let proposal = b.session.propose_cues("1", None).unwrap();
    assert!(proposal.cues.iter().any(|c| c.slot == "A" && c.existing));
    assert!(proposal.kept.iter().any(|k| k.contains("cue A")));
    assert!(proposal.notes.iter().any(|n| n.contains("no drop")), "{:?}", proposal.notes);
    let memory = b.session.propose_cues("1", Some("[[cue]]\nslot = \"memory\"\nat = \"start\"\noffset_bars = 4\nname = \"four in\"\n")).unwrap();
    let added: Vec<_> = memory.cues.iter().filter(|c| !c.existing).collect();
    assert_eq!(added.len(), 1);
    assert_eq!((added[0].slot.as_str(), added[0].bar, added[0].name.as_str()), ("memory", 5, "four in"));
    assert!(b.session.propose_cues("1", Some("not = [valid")).is_err());

    assert_eq!(std::fs::read(&b.xml).unwrap(), before);
}

#[test]
fn exports_write_what_was_asked_and_refuse_the_rest() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    b.session.analyse_track("1").unwrap();
    b.session.analyse_track("2").unwrap();
    let out = b.dir.path().join("out.xml");
    assert_eq!(b.session.export_xml(&out, &Session::default_rules()).unwrap(), 2);
    assert!(std::fs::read_to_string(&out).unwrap().contains("Name=\"kfbox\""));

    let chosen = b.dir.path().join("chosen.xml");
    assert_eq!(b.session.export_tracks(&chosen, &["3".to_owned()], false).unwrap(), 1);
    let refused = |path: std::path::PathBuf| matches!(b.session.export_tracks(&path, &["3".to_owned()], false), Err(CommandError::Refused(_)));
    assert!(refused(chosen.clone()), "it exists");
    assert!(b.session.export_tracks(&chosen, &["3".to_owned()], true).is_ok());
    assert!(refused("relative.xml".into()));
    assert!(refused(b.dir.path().join("out.txt")));
    assert!(refused(b.dir.path().join("Pioneer/rekordbox/x.xml")));
    assert!(refused(b.dir.path().join("missing/x.xml")));
    assert!(matches!(b.session.export_tracks(&b.dir.path().join("x.xml"), &["99".to_owned()], false), Err(CommandError::NoTrack(_))));
}

#[test]
fn a_hand_added_file_and_its_waveform() {
    let b = bench();
    let song = b.dir.path().join("loose.wav");
    testing::wav(&song, 7).unwrap();
    let analysis = b.session.analyse_file(&song).unwrap();
    assert_eq!(analysis.grid, kfbox_core::track::GridSource::Kfbox);
    assert!(b.session.waveform(&song).unwrap().len() > 1000);
    b.session.remove(&analysis.path);
    assert!(b.session.track(&analysis.path).is_err());
}

#[test]
fn the_xml_is_read_again_when_asked_for_fresh() {
    let b = bench();
    b.session.open_xml(&b.xml).unwrap();
    let text = std::fs::read_to_string(&b.xml).unwrap().replace("Name=\"Gamma\"", "Name=\"Gamma Two\"");
    std::fs::write(&b.xml, text).unwrap();
    b.session.fresh_library().unwrap();
    assert_eq!(b.session.search("gamma two", 5).unwrap().len(), 1);
}
