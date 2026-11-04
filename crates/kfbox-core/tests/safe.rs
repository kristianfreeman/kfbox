//! Writes on throwaway fixture libraries, never a real one.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use kfbox_core::safe::{Change, CommitKind, FileChange, FileContent, Op, SafeError, SafeLibrary, Slot};
use rbl_db::fixture::{self, Shape, FIXTURE_PASSPHRASE};
use rbl_db::write::Writer;
use rbl_db::{Library, LibraryLocation, OpenMode};

struct Bench {
    _dir: tempfile::TempDir,
    location: LibraryLocation,
    root: PathBuf,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let mut location = fixture::build(&lib, Shape::default()).unwrap();
    location.share_root = lib.join("share");
    std::fs::create_dir_all(&location.share_root).unwrap();
    let root = dir.path().join("kfbox");
    Bench { _dir: dir, location, root }
}

fn cue(track: usize, slot: u8, ms: u32) -> Op {
    Op::AddCue { track: fixture::track_id(track), slot: Slot::Hot(slot), start_ms: ms, colour: None, name: String::new() }
}

fn change(summary: &str, ops: Vec<Op>) -> Change {
    Change { summary: summary.to_owned(), ops, files: Vec::new() }
}

fn cues(location: &LibraryLocation) -> i64 {
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("master.db");
    std::fs::copy(&location.master_db, &copy).unwrap();
    let library = Library::open(LibraryLocation { master_db: copy, ..location.clone() }, OpenMode::ReadOnly).unwrap();
    let result: String = library.connection().query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(result, "ok");
    library.connection().query_row("SELECT COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0", [], |r| r.get(0)).unwrap()
}

fn leftovers(bench: &Bench) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(bench.root.join("txn"))
        .map_or(Vec::new(), |d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect());
    found.extend(
        std::fs::read_dir(bench.location.master_db.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".kfbox")),
    );
    found
}

#[test]
fn a_batch_lands_whole_with_colours_names_and_loops() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    let ops = vec![
        Op::AddCue { track: fixture::track_id(0), slot: Slot::Hot(0), start_ms: 1000, colour: Some(5), name: "start".into() },
        Op::AddCue { track: fixture::track_id(0), slot: Slot::Hot(3), start_ms: 2000, colour: None, name: "groove".into() },
        Op::AddLoop {
            track: fixture::track_id(1),
            slot: Slot::Hot(1),
            start_ms: 3000,
            end_ms: 4000,
            beats: 4,
            colour: Some(9),
            name: "loop".into(),
        },
        Op::AddCue { track: fixture::track_id(2), slot: Slot::Memory, start_ms: 5000, colour: Some(3), name: "memory".into() },
    ];
    let receipt = safe.apply(&change("Accept cues on 3 tracks", ops)).unwrap();
    assert_eq!(receipt.added_cues.len(), 4);
    assert_eq!(cues(&b.location), 4);
    assert_eq!(leftovers(&b), [] as [std::string::String; 0]);

    let history = safe.history(10).unwrap();
    assert_eq!(history.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![CommitKind::Action, CommitKind::First]);
    assert_eq!(history[0].summary, "Accept cues on 3 tracks");

    let json = std::fs::read_to_string(b.root.join("history/tracks").join(format!("{}.json", fixture::track_id(1)))).unwrap();
    assert!(
        json.contains("\"Comment\": \"loop\"") && json.contains("\"ColorTableIndex\": 9") && json.contains("\"OutMsec\": 4000"),
        "{json}"
    );
    safe.check_history().unwrap();
}

#[cfg(unix)]
#[test]
fn the_library_keeps_its_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let b = bench();
    std::fs::set_permissions(&b.location.master_db, std::fs::Permissions::from_mode(0o644)).unwrap();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    safe.apply(&change("one", vec![cue(0, 0, 1000)])).unwrap();
    let mode = std::fs::metadata(&b.location.master_db).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
    safe.undo().unwrap();
    let mode = std::fs::metadata(&b.location.master_db).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn a_failed_op_leaves_the_library_exactly_as_it_was() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    safe.apply(&change("first", vec![cue(0, 0, 1000)])).unwrap();
    let before = std::fs::read(&b.location.master_db).unwrap();
    // The second op's slot is taken, and it comes after one that would succeed.
    let error = safe.apply(&change("clash", vec![cue(1, 0, 1000), cue(0, 0, 2000)])).unwrap_err();
    assert!(matches!(error, SafeError::SlotTaken { .. }), "{error}");
    let unknown = safe.apply(&change("unknown", vec![Op::MoveCue { cue: "424242".into(), start_ms: 5 }]));
    assert!(unknown.is_err());
    assert_eq!(std::fs::read(&b.location.master_db).unwrap(), before);
    assert_eq!(leftovers(&b), [] as [std::string::String; 0]);
    assert_eq!(safe.history(10).unwrap().len(), 2);
}

#[test]
fn it_refuses_while_rekordbox_runs_even_if_it_starts_mid_write() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    let before = std::fs::read(&b.location.master_db).unwrap();

    safe.set_running_check(|| true);
    assert!(matches!(safe.apply(&change("x", vec![cue(0, 0, 1)])), Err(SafeError::RekordboxRunning)));

    // It starts while the change is being staged.
    let running = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&running);
    safe.set_running_check(move || seen.load(Ordering::SeqCst));
    let starts = Arc::clone(&running);
    safe.set_crash_hook(move |step| {
        if step == "after_ops" {
            starts.store(true, Ordering::SeqCst);
        }
    });
    let mut with_file = change("y", vec![cue(0, 0, 1)]);
    with_file.files.push(FileChange { relative: "PIONEER/USBANLZ/a/ANLZ0000.DAT".into(), content: FileContent::Bytes(b"new".to_vec()) });
    assert!(matches!(safe.apply(&with_file), Err(SafeError::RekordboxRunning)));
    assert_eq!(std::fs::read(&b.location.master_db).unwrap(), before);
    assert!(!b.location.share_root.join("PIONEER/USBANLZ/a/ANLZ0000.DAT").exists(), "the file goes back too");
    assert_eq!(leftovers(&b), [] as [std::string::String; 0]);
}

#[test]
fn it_refuses_a_library_with_unfinished_writes_and_never_opens_a_real_one() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    let mut wal = b.location.master_db.clone().into_os_string();
    wal.push("-wal");
    std::fs::write(&wal, b"pending").unwrap();
    assert!(matches!(safe.apply(&change("x", vec![cue(0, 0, 1)])), Err(SafeError::Unclean(_))));
    std::fs::remove_file(&wal).unwrap();

    let real = LibraryLocation { is_real_install: true, ..b.location.clone() };
    assert!(matches!(SafeLibrary::open(real, &b.root.join("other")), Err(SafeError::RealLibrary)));
}

#[test]
fn only_one_writer_at_a_time() {
    let b = bench();
    let _first = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    assert!(matches!(SafeLibrary::open(b.location.clone(), &b.root), Err(SafeError::Busy)));
}

#[test]
fn undo_and_restore_bring_back_the_exact_file() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    assert!(matches!(safe.undo(), Err(SafeError::NothingToUndo(_))));

    safe.apply(&change("one", vec![cue(0, 0, 1000)])).unwrap();
    let after_one = std::fs::read(&b.location.master_db).unwrap();
    safe.apply(&change("two", vec![cue(0, 1, 2000)])).unwrap();
    let after_two = std::fs::read(&b.location.master_db).unwrap();

    let undone = safe.undo().unwrap();
    assert!(undone.commit.is_some());
    assert_eq!(std::fs::read(&b.location.master_db).unwrap(), after_one);
    assert_eq!(safe.history(1).unwrap()[0].summary, "Undo: two");

    // An undo is itself undoable.
    safe.undo().unwrap();
    assert_eq!(std::fs::read(&b.location.master_db).unwrap(), after_two);

    let first = safe.history(50).unwrap().last().unwrap().commit.clone();
    safe.restore(&first).unwrap();
    assert_eq!(cues(&b.location), 0);
    assert!(matches!(safe.restore("0000000000"), Err(SafeError::NoSuchCommit(_))));
    safe.check_history().unwrap();
}

/// rekordbox editing the library, as a separate writer on the file.
fn outside_edit(location: &LibraryLocation, track: usize) {
    let scratch = tempfile::tempdir().unwrap();
    let mut writer = Writer::open(location.clone(), scratch.path().join("b")).unwrap();
    writer.disable_automatic_backups();
    writer.add_cue(&fixture::track_id(track), 10, 7777).unwrap();
}

#[test]
fn changes_made_outside_are_committed_first_and_block_a_blind_undo() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    safe.apply(&change("mine", vec![cue(0, 0, 1000)])).unwrap();

    outside_edit(&b.location, 5);
    assert!(matches!(safe.undo(), Err(SafeError::NothingToUndo(_))), "undo mustn't erase it");
    assert_eq!(safe.history(1).unwrap()[0].kind, CommitKind::Outside);
    assert_eq!(cues(&b.location), 2);

    outside_edit(&b.location, 6);
    safe.apply(&change("mine again", vec![cue(1, 0, 1000)])).unwrap();
    let kinds: Vec<_> = safe.history(10).unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(kinds, vec![CommitKind::Action, CommitKind::Outside, CommitKind::Outside, CommitKind::Action, CommitKind::First]);

    // Undoing the last action keeps what happened outside before it.
    safe.undo().unwrap();
    assert_eq!(cues(&b.location), 3);
}

#[test]
fn analysis_files_change_with_the_library_and_come_back_with_it() {
    let b = bench();
    let path = "PIONEER/USBANLZ/ab/ANLZ0000.DAT";
    let live = b.location.share_root.join(path);
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    std::fs::write(&live, b"rekordbox's grid").unwrap();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();

    let mut edit = change("nudge grid", vec![cue(0, 0, 1000)]);
    edit.files.push(FileChange { relative: path.into(), content: FileContent::Bytes(b"kfbox's grid".to_vec()) });
    edit.files.push(FileChange { relative: "PIONEER/USBANLZ/cd/NEW.DAT".into(), content: FileContent::Bytes(b"added".to_vec()) });
    safe.apply(&edit).unwrap();
    assert_eq!(std::fs::read(&live).unwrap(), b"kfbox's grid");

    safe.undo().unwrap();
    assert_eq!(std::fs::read(&live).unwrap(), b"rekordbox's grid");
    assert!(!b.location.share_root.join("PIONEER/USBANLZ/cd/NEW.DAT").exists());
    assert_eq!(cues(&b.location), 0);

    let mut bad = change("escape", Vec::new());
    bad.files.push(FileChange { relative: "../outside".into(), content: FileContent::Delete });
    assert!(matches!(safe.apply(&bad), Err(SafeError::BadPath(_))));
}

#[test]
fn a_change_to_files_alone_commits_and_undoes() {
    let b = bench();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    let mut edit = change("artwork", Vec::new());
    edit.files.push(FileChange { relative: "PIONEER/Artwork/a.jpg".into(), content: FileContent::Bytes(b"img".to_vec()) });
    assert!(safe.apply(&edit).unwrap().commit.is_some());
    safe.undo().unwrap();
    assert!(!b.location.share_root.join("PIONEER/Artwork/a.jpg").exists());
}

#[test]
fn the_daily_archive_restores_even_without_the_history() {
    let b = bench();
    let file = b.location.share_root.join("PIONEER/USBANLZ/ab/A.DAT");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"original").unwrap();
    let original = std::fs::read(&b.location.master_db).unwrap();

    let days = {
        let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
        let mut edit = change("one", vec![cue(0, 0, 1000)]);
        edit.files.push(FileChange { relative: "PIONEER/USBANLZ/ab/A.DAT".into(), content: FileContent::Bytes(b"changed".to_vec()) });
        safe.apply(&edit).unwrap();
        safe.apply(&change("two", vec![cue(1, 0, 1000)])).unwrap();
        safe.list_archives().unwrap()
    };
    assert_eq!(days.len(), 1, "one archive a day, however many writes");

    std::fs::remove_dir_all(b.root.join("history")).unwrap();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    safe.restore_archive(&days[0]).unwrap();
    assert_eq!(std::fs::read(&b.location.master_db).unwrap(), original);
    assert_eq!(std::fs::read(&file).unwrap(), b"original");
    assert!(matches!(safe.restore_archive("1999-01-01"), Err(SafeError::NoSuchArchive(_))));
}

#[test]
fn a_stale_git_lock_from_a_crash_is_cleared() {
    let b = bench();
    {
        let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
        safe.apply(&change("one", vec![cue(0, 0, 1000)])).unwrap();
    }
    std::fs::write(b.root.join("history/.git/index.lock"), b"").unwrap();
    let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
    safe.apply(&change("two", vec![cue(1, 0, 1000)])).unwrap();
}

// ---- killing the process at each step -------------------------------------

/// Every step of a write, and which time it's reached: the commit step comes
/// up first for the outside-changes check.
const KILL_POINTS: [(&str, usize); 9] = [
    ("before_commit", 1),
    ("after_ops", 1),
    ("before_journal", 1),
    ("after_journal", 1),
    ("after_files", 1),
    ("before_swap", 1),
    ("after_swap", 1),
    ("before_commit", 2),
    ("after_commit", 1),
];

const FILE: &str = "PIONEER/USBANLZ/ab/ANLZ0000.DAT";

fn location_at(master: &Path, share: &Path) -> LibraryLocation {
    LibraryLocation {
        master_db: master.to_path_buf(),
        share_root: share.to_path_buf(),
        passphrase: FIXTURE_PASSPHRASE.to_owned(),
        is_real_install: false,
    }
}

fn killed_change() -> Change {
    let mut edit = change("killed action", vec![cue(3, 0, 1000), cue(4, 0, 2000)]);
    edit.files.push(FileChange { relative: FILE.into(), content: FileContent::Bytes(b"after".to_vec()) });
    edit
}

/// Runs in a child process: makes the change and dies at the named step.
#[test]
fn kill_child() {
    let Ok(at) = std::env::var("KFBOX_KILL_AT") else { return };
    let nth: usize = std::env::var("KFBOX_KILL_NTH").unwrap().parse().unwrap();
    let location = location_at(Path::new(&std::env::var("KFBOX_MASTER").unwrap()), Path::new(&std::env::var("KFBOX_SHARE").unwrap()));
    let mut safe = SafeLibrary::open(location, Path::new(&std::env::var("KFBOX_ROOT").unwrap())).unwrap();
    let count = AtomicUsize::new(0);
    safe.set_crash_hook(move |step| {
        if step == at && count.fetch_add(1, Ordering::SeqCst) + 1 == nth {
            std::process::abort();
        }
    });
    if std::env::var("KFBOX_KILL_RESTORE").is_ok() {
        safe.undo().unwrap();
    } else {
        safe.apply(&killed_change()).unwrap();
    }
    // Reaching here means the step was never hit, which the parent reports.
    std::process::exit(7);
}

fn run_child(b: &Bench, at: &str, nth: usize, restore: bool) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "kill_child", "--nocapture"])
        .env("KFBOX_KILL_AT", at)
        .env("KFBOX_KILL_NTH", nth.to_string())
        .env("KFBOX_MASTER", &b.location.master_db)
        .env("KFBOX_SHARE", &b.location.share_root)
        .env("KFBOX_ROOT", &b.root);
    if restore {
        command.env("KFBOX_KILL_RESTORE", "1");
    }
    let output = command.output().unwrap();
    assert!(!output.status.success() && output.status.code() != Some(7), "{at}#{nth} wasn't reached: {output:?}");
}

#[test]
fn killing_a_write_at_any_step_leaves_it_whole_or_not_at_all() {
    for (at, nth) in KILL_POINTS {
        let b = bench();
        let file = b.location.share_root.join(FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"before").unwrap();
        let before = std::fs::read(&b.location.master_db).unwrap();

        run_child(&b, at, nth, false);

        let safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
        let landed = std::fs::read(&b.location.master_db).unwrap() != before;
        let count = cues(&b.location);
        let contents = std::fs::read(&file).unwrap();
        if landed {
            assert_eq!((count, contents.as_slice()), (2, b"after".as_slice()), "{at}#{nth} landed");
            assert_eq!(safe.history(1).unwrap()[0].summary, "killed action", "{at}#{nth}: recovery finishes the commit");
        } else {
            assert_eq!((count, contents.as_slice()), (0, b"before".as_slice()), "{at}#{nth} didn't land");
        }
        assert!(leftovers(&b).is_empty(), "{at}#{nth}: {:?}", leftovers(&b));
        safe.check_history().unwrap();

        // And the library still takes writes.
        drop(safe);
        let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
        safe.apply(&change("next", vec![cue(9, 0, 1000)])).unwrap();
        safe.check_history().unwrap();
        // The steps before the swap must not land; those from it on must.
        let must_land = matches!(at, "after_swap" | "after_commit") || (at == "before_commit" && nth == 2);
        assert_eq!(landed, must_land, "{at}#{nth}");
    }
}

#[test]
fn killing_an_undo_at_any_step_leaves_it_whole_or_not_at_all() {
    for (at, nth) in KILL_POINTS.iter().filter(|(at, nth)| *at != "after_ops" && !(*at == "before_commit" && *nth == 2)) {
        let b = bench();
        let file = b.location.share_root.join(FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"before").unwrap();
        {
            let mut safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
            safe.apply(&killed_change()).unwrap();
        }
        let done = std::fs::read(&b.location.master_db).unwrap();

        run_child(&b, at, *nth, true);

        let safe = SafeLibrary::open(b.location.clone(), &b.root).unwrap();
        let landed = std::fs::read(&b.location.master_db).unwrap() != done;
        let contents = std::fs::read(&file).unwrap();
        if landed {
            assert_eq!((cues(&b.location), contents.as_slice()), (0, b"before".as_slice()), "{at}#{nth}");
            assert!(safe.history(1).unwrap()[0].summary.starts_with("Undo:"), "{at}#{nth}");
        } else {
            assert_eq!((cues(&b.location), contents.as_slice()), (2, b"after".as_slice()), "{at}#{nth}");
        }
        assert!(leftovers(&b).is_empty(), "{at}#{nth}: {:?}", leftovers(&b));
        safe.check_history().unwrap();
    }
}
