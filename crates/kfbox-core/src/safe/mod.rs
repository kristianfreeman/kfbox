//! Every write to the rekordbox library goes through here.
//!
//! A change is made to a staged copy of `master.db`, read back and checked,
//! and swapped into place by one atomic rename, so the library holds the whole
//! change or none of it. rbl-db commits after each edit, which on the library
//! itself would leave half a batch after a crash; on a copy that can't happen.
//!
//! Three more things protect the library:
//!
//! - A private git repository records every action, with `master.db`, the
//!   analysis files it changed, and a readable JSON file per track.
//! - Once a day, before the first write, a full copy of the library goes to
//!   an archive outside that repository.
//! - A journal covers the swap. The rename of `master.db` is the point of no
//!   return: after a crash, a library matching the journal's result gets its
//!   commit finished, and any other gets its analysis files put back.
//!
//! Changes rekordbox made since the last commit are committed first, so a
//! restore or an undo can't quietly lose them.

mod git;
mod ops;
mod tracks;

use std::fs::{File, TryLockError};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rbl_core::durable;
use rbl_db::write::Writer;
use rbl_db::{DbError, Library, LibraryLocation, OpenMode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use git::{CommitKind, HistoryEntry};
pub use ops::{hot_colour_index, Op, Slot};

/// Full daily copies kept. The git history goes back further.
const ARCHIVES_KEPT: usize = 7;

/// The parts of rekordbox's `share` folder an archive holds.
const SHARED_FOLDERS: [&str; 2] = ["PIONEER/USBANLZ", "PIONEER/Artwork"];

const STAGED_PREFIX: &str = ".kfbox-staged-";

/// How long opening waits for the library's lock before calling it taken.
const LOCK_WAIT: Duration = Duration::from_secs(2);

/// Written in a transaction's folder once everything is in place.
const LANDED: &str = "landed";

#[derive(Debug, thiserror::Error)]
pub enum SafeError {
    #[error("rekordbox is running. Quit it and try again.")]
    RekordboxRunning,
    #[error("{0} has unfinished writes, so rekordbox may not have closed cleanly. Open and quit rekordbox, then try again.")]
    Unclean(PathBuf),
    #[error("this is the real library, and it needs SafeLibrary::open_real")]
    RealLibrary,
    #[error("another kfbox is writing to this library")]
    Busy,
    #[error("hot cue slot is taken on track {track} by cue {cue}")]
    SlotTaken { track: String, cue: String },
    #[error("the change didn't check out, so the library wasn't touched: {0}")]
    Verify(String),
    #[error("{0} isn't a commit in the history")]
    NoSuchCommit(String),
    #[error("there's nothing to undo: {0}")]
    NothingToUndo(String),
    #[error("{0} isn't a path inside rekordbox's share folder")]
    BadPath(PathBuf),
    #[error("no archive for {0}")]
    NoSuchArchive(String),
    #[error("history: {0}")]
    Git(String),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A change to an analysis file or artwork, as a path under `share`.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub relative: PathBuf,
    pub content: FileContent,
}

#[derive(Debug, Clone)]
pub enum FileContent {
    Bytes(Vec<u8>),
    CopyOf(PathBuf),
    Delete,
}

/// One user action: everything in it lands together or not at all.
#[derive(Debug, Clone, Default)]
pub struct Change {
    /// The history's message, such as `Apply rules to 40 tracks`.
    pub summary: String,
    pub ops: Vec<Op>,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone)]
pub struct Receipt {
    /// `None` when the action changed nothing.
    pub commit: Option<String>,
    /// The IDs of new cues, in the order they were added.
    pub added_cues: Vec<String>,
}

/// What a crash leaves behind. Written before anything in the library changes.
#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    summary: String,
    kind_is_restore: bool,
    before_sha: String,
    /// `master.db` once the change has landed.
    after_sha: String,
    files: Vec<JournalFile>,
    added_cues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalFile {
    relative: PathBuf,
    /// The original, saved in the transaction's folder. `None` if there wasn't one.
    before: Option<PathBuf>,
}

type CrashHook = Box<dyn Fn(&str) + Send>;
type RunningCheck = Box<dyn Fn() -> bool + Send>;

/// The library, with the only route kfbox has for changing it.
pub struct SafeLibrary {
    location: LibraryLocation,
    root: PathBuf,
    repo: git::Repo,
    running: RunningCheck,
    crash_hook: Option<CrashHook>,
    // Holds the root's lock while this exists.
    _lock: File,
}

fn sha_of(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        let Some(chunk) = buffer.get(..read).filter(|c| !c.is_empty()) else { break };
        hash.update(chunk);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn nonempty(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() > 0)
}

fn checked_relative(path: &Path) -> Result<(), SafeError> {
    if path.as_os_str().is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(SafeError::BadPath(path.to_path_buf()));
    }
    Ok(())
}

fn private_dir(path: &Path) -> Result<(), SafeError> {
    durable::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Copies a folder's files, which on APFS shares their blocks until they change.
fn copy_tree(from: &Path, to: &Path) -> Result<(), SafeError> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn tree_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SafeError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)?.flatten() {
        if entry.file_type()?.is_dir() {
            tree_files(root, &entry.path(), out)?;
        } else if let Ok(relative) = entry.path().strip_prefix(root) {
            out.push(relative.to_path_buf());
        }
    }
    Ok(())
}

impl SafeLibrary {
    /// Opens a copy or fixture of a library, finishing or undoing any change a
    /// crash interrupted. `root` holds the history, archives and journal.
    pub fn open(location: LibraryLocation, root: &Path) -> Result<Self, SafeError> {
        if location.is_real_install {
            return Err(SafeError::RealLibrary);
        }
        Self::open_any(location, root, Box::new(|| false))
    }

    /// Opens rekordbox's real library. Writes through it refuse to run while
    /// rekordbox is running, or if rekordbox left unfinished writes.
    pub fn open_real(location: LibraryLocation, root: &Path) -> Result<Self, SafeError> {
        Self::open_any(location, root, Box::new(rbl_db::is_rekordbox_running))
    }

    fn open_any(location: LibraryLocation, root: &Path, running: RunningCheck) -> Result<Self, SafeError> {
        private_dir(root)?;
        let lock = File::options().create(true).truncate(false).write(true).open(root.join("lock"))?;
        // A process that's starting at this moment can hold a copy of the lock
        // for an instant, so only a lock that stays held means another writer.
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                Err(TryLockError::WouldBlock) => return Err(SafeError::Busy),
                Err(TryLockError::Error(e)) => return Err(e.into()),
            }
        }
        let repo = git::Repo::open(&root.join("history"))?;
        let mut library = Self { location, root: root.to_path_buf(), repo, running, crash_hook: None, _lock: lock };
        library.recover()?;
        Ok(library)
    }

    /// For tests: stands in for "is rekordbox running?".
    pub fn set_running_check(&mut self, running: impl Fn() -> bool + Send + 'static) {
        self.running = Box::new(running);
    }

    /// For tests: called at each step of a write, so a test can kill the
    /// process there.
    pub fn set_crash_hook(&mut self, hook: impl Fn(&str) + Send + 'static) {
        self.crash_hook = Some(Box::new(hook));
    }

    fn step(&self, name: &str) {
        if let Some(hook) = &self.crash_hook {
            hook(name);
        }
    }

    fn master(&self) -> &Path {
        &self.location.master_db
    }

    fn transactions(&self) -> PathBuf {
        self.root.join("txn")
    }

    fn check_writable(&self) -> Result<(), SafeError> {
        if (self.running)() {
            return Err(SafeError::RekordboxRunning);
        }
        for suffix in ["-wal", "-journal"] {
            let path = with_suffix(self.master(), suffix);
            if nonempty(&path) {
                return Err(SafeError::Unclean(path));
            }
        }
        Ok(())
    }

    /// A copy of the library's location at `master_db`, safe to open for writing.
    fn at(&self, master_db: PathBuf) -> LibraryLocation {
        LibraryLocation { master_db, is_real_install: false, ..self.location.clone() }
    }

    // ---- recording -------------------------------------------------------

    /// Brings the repository's working tree in line with the library and
    /// commits it. `files` are the share-folder files this commit covers.
    fn record(&self, summary: &str, kind: CommitKind, files: &[PathBuf]) -> Result<Option<String>, SafeError> {
        let tree = self.repo.dir();
        durable::create_dir_all(tree.join("library"))?;
        let copy = tree.join("library/master.db");
        durable::copy(self.master(), &copy)?;
        let scratch = tempfile::tempdir_in(&self.root)?;
        let read_copy = scratch.path().join("master.db");
        durable::copy(self.master(), &read_copy)?;
        {
            let library = Library::open(self.at(read_copy), OpenMode::ReadOnly)?;
            tracks::write(&library, &tree.join("tracks"))?;
        }
        for relative in files {
            let source = self.location.share_root.join(relative);
            let mirror = tree.join("analysis").join(relative);
            if source.exists() {
                if let Some(parent) = mirror.parent() {
                    durable::create_dir_all(parent)?;
                }
                std::fs::copy(source, mirror)?;
            } else if mirror.exists() {
                std::fs::remove_file(mirror)?;
            }
        }
        self.step("before_commit");
        self.repo.commit_all(summary, kind)
    }

    /// Commits whatever changed outside kfbox since the last commit, including
    /// the current state of `files`, which are about to change.
    fn absorb_outside_changes(&self, files: &[PathBuf]) -> Result<(), SafeError> {
        let first = self.repo.history(1)?.is_empty();
        let (summary, kind) = if first { ("Library as kfbox first saw it", CommitKind::First) } else { ("Changed outside kfbox", CommitKind::Outside) };
        self.record(summary, kind, files)?;
        Ok(())
    }

    // ---- writing ---------------------------------------------------------

    /// Makes `change`, whole or not at all.
    pub fn apply(&mut self, change: &Change) -> Result<Receipt, SafeError> {
        self.check_writable()?;
        for file in &change.files {
            checked_relative(&file.relative)?;
        }
        let touched: Vec<PathBuf> = change.files.iter().map(|f| f.relative.clone()).collect();
        self.absorb_outside_changes(&touched)?;
        self.archive_today()?;

        let txn = self.new_transaction()?;
        let result = self.stage_and_publish(change, &touched, &txn);
        if result.is_err() {
            // Nothing in the library changed, or the journal would still be
            // here for recovery and this wouldn't be an error path.
            if !txn.join("journal.json").exists() {
                durable::remove_tree(&txn)?;
            }
        }
        result
    }

    fn new_transaction(&self) -> Result<PathBuf, SafeError> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let txn = self.transactions().join(format!("{nanos}"));
        durable::create_dir_all(&txn)?;
        Ok(txn)
    }

    fn stage_and_publish(&self, change: &Change, touched: &[PathBuf], txn: &Path) -> Result<Receipt, SafeError> {
        let work = txn.join("master.db");
        durable::copy(self.master(), &work)?;
        let (before_cues, before_tracks) = {
            let library = Library::open(self.at(work.clone()), OpenMode::ReadOnly)?;
            (ops::cue_count(&library)?, ops::track_count(&library)?)
        };
        let applied = {
            let mut writer = Writer::open(self.at(work.clone()), txn.join("unused-backups"))?;
            writer.disable_automatic_backups();
            ops::apply(&mut writer, &change.ops)?
        };
        self.step("after_ops");
        if nonempty(&with_suffix(&work, "-wal")) {
            return Err(SafeError::Verify("the staged copy still has unfinished writes".to_owned()));
        }
        {
            let library = Library::open(self.at(work.clone()), OpenMode::ReadOnly)?;
            ops::verify(&library, before_cues, before_tracks, &applied)?;
        }
        self.publish(txn, &change.summary, false, &change.files, touched, &work, applied.added)
    }

    /// Puts a checked `staged` database and its files in place.
    #[allow(clippy::too_many_arguments, reason = "one private call site per kind of change")]
    fn publish(
        &self,
        txn: &Path,
        summary: &str,
        kind_is_restore: bool,
        files: &[FileChange],
        touched: &[PathBuf],
        staged: &Path,
        added_cues: Vec<String>,
    ) -> Result<Receipt, SafeError> {
        // Originals first, while nothing has moved.
        let before_dir = txn.join("before");
        durable::create_dir_all(&before_dir)?;
        let mut journal_files = Vec::new();
        for (i, file) in files.iter().enumerate() {
            let target = self.location.share_root.join(&file.relative);
            let before = if target.exists() {
                let name = PathBuf::from(i.to_string());
                durable::copy(&target, &before_dir.join(&name))?;
                Some(name)
            } else {
                None
            };
            journal_files.push(JournalFile { relative: file.relative.clone(), before });
        }
        let journal = Journal {
            summary: summary.to_owned(),
            kind_is_restore,
            before_sha: sha_of(self.master())?,
            after_sha: sha_of(staged)?,
            files: journal_files,
            added_cues: added_cues.clone(),
        };
        self.step("before_journal");
        let text = serde_json::to_vec_pretty(&journal).map_err(|e| SafeError::Verify(e.to_string()))?;
        durable::write(&txn.join("journal.json"), &text)?;
        self.step("after_journal");

        let result = self.publish_journalled(txn, staged, files);
        if let Err(error) = result {
            self.roll_back(txn, &journal)?;
            return Err(error);
        }
        self.step("after_swap");
        let commit = self.record(summary, if kind_is_restore { CommitKind::Restore } else { CommitKind::Action }, touched)?;
        self.step("after_commit");
        durable::remove_tree(txn)?;
        Ok(Receipt { commit, added_cues })
    }

    /// The part a failure must undo. Everything before the rename of
    /// `master.db` can be put back; the rename itself is atomic.
    fn publish_journalled(&self, txn: &Path, staged: &Path, files: &[FileChange]) -> Result<(), SafeError> {
        for file in files {
            let target = self.location.share_root.join(&file.relative);
            match &file.content {
                FileContent::Bytes(bytes) => {
                    if let Some(parent) = target.parent() {
                        durable::create_dir_all(parent)?;
                    }
                    durable::write(&target, bytes)?;
                }
                FileContent::CopyOf(source) => {
                    if let Some(parent) = target.parent() {
                        durable::create_dir_all(parent)?;
                    }
                    durable::copy(source, &target)?;
                }
                FileContent::Delete => {
                    if target.exists() {
                        std::fs::remove_file(&target)?;
                    }
                }
            }
        }
        self.step("after_files");
        // rekordbox could have started during all of the above.
        self.check_writable()?;
        let beside = self.master().with_file_name(format!("{STAGED_PREFIX}{}", std::process::id()));
        durable::copy(staged, &beside)?;
        // The copy has its own permissions, and the library should keep its own.
        std::fs::set_permissions(&beside, std::fs::metadata(self.master())?.permissions())?;
        self.step("before_swap");
        let swapped = durable::replace(&beside, self.master());
        if swapped.is_err() {
            let _ = std::fs::remove_file(&beside);
        }
        swapped?;
        // Empty leftovers of the old file mean nothing to the new one, but
        // SQLite shouldn't find them beside it.
        for suffix in ["-wal", "-shm"] {
            let leftover = with_suffix(self.master(), suffix);
            if leftover.exists() {
                std::fs::remove_file(leftover)?;
            }
        }
        // A change to files alone leaves `master.db` as it was, so its hash
        // can't say whether the change finished. This can.
        durable::write(&txn.join(LANDED), b"")?;
        Ok(())
    }

    fn roll_back(&self, txn: &Path, journal: &Journal) -> Result<(), SafeError> {
        for file in &journal.files {
            let target = self.location.share_root.join(&file.relative);
            match &file.before {
                Some(name) => {
                    if let Some(parent) = target.parent() {
                        durable::create_dir_all(parent)?;
                    }
                    durable::copy(&txn.join("before").join(name), &target)?;
                }
                None => {
                    if target.exists() {
                        std::fs::remove_file(target)?;
                    }
                }
            }
        }
        durable::remove_tree(txn)?;
        Ok(())
    }

    /// Finishes what a crash interrupted.
    fn recover(&mut self) -> Result<(), SafeError> {
        // A staged copy beside the library never became it.
        if let Some(dir) = self.master().parent() {
            for entry in std::fs::read_dir(dir)?.flatten() {
                if entry.file_name().to_string_lossy().starts_with(STAGED_PREFIX) {
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
        let Ok(transactions) = std::fs::read_dir(self.transactions()) else { return Ok(()) };
        for entry in transactions.flatten() {
            let txn = entry.path();
            let Ok(text) = std::fs::read(txn.join("journal.json")) else {
                // It died before the journal, so nothing in the library changed.
                durable::remove_tree(&txn)?;
                continue;
            };
            let journal: Journal = serde_json::from_slice(&text).map_err(|e| SafeError::Verify(format!("journal: {e}")))?;
            let swapped = journal.before_sha != journal.after_sha && sha_of(self.master())? == journal.after_sha;
            if swapped || txn.join(LANDED).exists() {
                let touched: Vec<PathBuf> = journal.files.iter().map(|f| f.relative.clone()).collect();
                let kind = if journal.kind_is_restore { CommitKind::Restore } else { CommitKind::Action };
                self.record(&journal.summary, kind, &touched)?;
                durable::remove_tree(&txn)?;
            } else {
                self.roll_back(&txn, &journal)?;
            }
        }
        Ok(())
    }

    // ---- history ---------------------------------------------------------

    /// The latest commits, newest first.
    pub fn history(&self, limit: usize) -> Result<Vec<HistoryEntry>, SafeError> {
        self.repo.history(limit)
    }

    /// Puts the library, and the analysis files the history tracks, back as
    /// they were at `commit`. This is a commit too, so it can be undone.
    pub fn restore(&mut self, commit: &str) -> Result<Receipt, SafeError> {
        self.check_writable()?;
        let commit = self.repo.resolve(commit)?;
        let short: String = commit.chars().take(8).collect();
        self.restore_commit(&commit, &format!("Restore to {short}"))
    }

    fn restore_commit(&mut self, commit: &str, summary: &str) -> Result<Receipt, SafeError> {
        let wanted: Vec<String> = self.repo.files_at(commit, "analysis")?;
        let txn_for_files = self.new_transaction()?;
        let staged = txn_for_files.join("master.db");
        durable::write(&staged, &self.repo.file_at(commit, "library/master.db")?)?;
        {
            let library = Library::open(self.at(staged.clone()), OpenMode::ReadOnly)?;
            ops::check_integrity(&library)?;
        }
        let head = self.repo.history(1)?.first().map(|e| e.commit.clone());
        let current = match &head {
            Some(head) => self.repo.files_at(head, "analysis")?,
            None => Vec::new(),
        };
        let mut files = Vec::new();
        for relative in &wanted {
            files.push(FileChange {
                relative: PathBuf::from(relative),
                content: FileContent::Bytes(self.repo.file_at(commit, &format!("analysis/{relative}"))?),
            });
        }
        for relative in current.iter().filter(|r| !wanted.contains(r)) {
            files.push(FileChange { relative: PathBuf::from(relative), content: FileContent::Delete });
        }
        let touched: Vec<PathBuf> = files.iter().map(|f| f.relative.clone()).collect();
        let prepared = (|| {
            self.absorb_outside_changes(&touched)?;
            self.archive_today()
        })();
        if let Err(error) = prepared {
            durable::remove_tree(&txn_for_files)?;
            return Err(error);
        }
        self.publish(&txn_for_files, summary, true, &files, &touched, &staged, Vec::new())
    }

    /// Undoes the latest action, if nothing has changed since it.
    pub fn undo(&mut self) -> Result<Receipt, SafeError> {
        self.check_writable()?;
        self.absorb_outside_changes(&[])?;
        let Some(latest) = self.repo.history(1)?.into_iter().next() else {
            return Err(SafeError::NothingToUndo("the history is empty".to_owned()));
        };
        match latest.kind {
            CommitKind::First => Err(SafeError::NothingToUndo("the history only holds the library as first seen".to_owned())),
            CommitKind::Outside => Err(SafeError::NothingToUndo("the library changed outside kfbox after the last action".to_owned())),
            CommitKind::Action | CommitKind::Restore => {
                let parent = self.repo.parent(&latest.commit)?.ok_or_else(|| SafeError::NothingToUndo("it has no earlier state".to_owned()))?;
                self.restore_commit(&parent, &format!("Undo: {}", latest.summary))
            }
        }
    }

    /// Undoes an undo, when that's the latest change, as `Redo: …`. Anything
    /// else, a redo included, has nothing to redo.
    pub fn redo(&mut self) -> Result<Receipt, SafeError> {
        self.check_writable()?;
        self.absorb_outside_changes(&[])?;
        let latest = self.repo.history(1)?.into_iter().next();
        let Some((latest, undone)) = latest.as_ref().and_then(|e| Some((e, e.summary.strip_prefix("Undo: ")?))) else {
            return Err(SafeError::NothingToUndo("the latest change isn't an undo, so there's nothing to redo".to_owned()));
        };
        let parent = self.repo.parent(&latest.commit)?.ok_or_else(|| SafeError::NothingToUndo("it has no earlier state".to_owned()))?;
        self.restore_commit(&parent, &format!("Redo: {undone}"))
    }

    /// The history's size on disk, and whether git finds it intact.
    pub fn check_history(&self) -> Result<u64, SafeError> {
        self.repo.check()?;
        Ok(self.repo.size_bytes())
    }

    /// Packs the history now rather than at its next regular point.
    pub fn pack_history(&self) -> Result<(), SafeError> {
        self.repo.pack_now()
    }

    // ---- archives --------------------------------------------------------

    fn archives(&self) -> PathBuf {
        self.root.join("archives")
    }

    /// The day's archive, if it doesn't exist yet. Returns its folder.
    pub fn archive_today(&self) -> Result<PathBuf, SafeError> {
        let day = rbl_core::time::now().chars().take(10).collect::<String>();
        let folder = self.archives().join(&day);
        if folder.exists() {
            return Ok(folder);
        }
        private_dir(&self.archives())?;
        // Under a temporary name, so a crash can't leave a partial archive
        // that looks complete.
        let partial = self.archives().join(format!(".partial-{day}"));
        if partial.exists() {
            durable::remove_tree(&partial)?;
        }
        durable::create_dir_all(&partial)?;
        durable::copy(self.master(), &partial.join("master.db"))?;
        for shared in SHARED_FOLDERS {
            copy_tree(&self.location.share_root.join(shared), &partial.join("share").join(shared))?;
        }
        durable::rename(&partial, &folder)?;
        self.prune_archives()?;
        Ok(folder)
    }

    fn prune_archives(&self) -> Result<(), SafeError> {
        let mut days = self.list_archives()?;
        days.reverse();
        for day in days.iter().skip(ARCHIVES_KEPT) {
            durable::remove_tree(&self.archives().join(day))?;
        }
        Ok(())
    }

    /// The days with an archive, oldest first.
    pub fn list_archives(&self) -> Result<Vec<String>, SafeError> {
        let Ok(entries) = std::fs::read_dir(self.archives()) else { return Ok(Vec::new()) };
        let mut days: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .collect();
        days.sort();
        Ok(days)
    }

    /// Puts a day's archive back as the library. It needs nothing from the
    /// history, so if that is lost, delete its folder and this still works. A
    /// file added to `share` since then stays, unused.
    pub fn restore_archive(&mut self, day: &str) -> Result<Receipt, SafeError> {
        self.check_writable()?;
        let folder = self.archives().join(day);
        let archived = folder.join("master.db");
        if !archived.is_file() {
            return Err(SafeError::NoSuchArchive(day.to_owned()));
        }
        let txn = self.new_transaction()?;
        let staged = txn.join("master.db");
        durable::copy(&archived, &staged)?;
        {
            let library = Library::open(self.at(staged.clone()), OpenMode::ReadOnly)?;
            ops::check_integrity(&library)?;
        }
        let share = folder.join("share");
        let mut names = Vec::new();
        tree_files(&share, &share, &mut names)?;
        let mut files = Vec::new();
        for relative in names {
            let source = share.join(&relative);
            let live = self.location.share_root.join(&relative);
            if !live.exists() || sha_of(&live)? != sha_of(&source)? {
                files.push(FileChange { relative, content: FileContent::CopyOf(source) });
            }
        }
        let touched: Vec<PathBuf> = files.iter().map(|f| f.relative.clone()).collect();
        self.absorb_outside_changes(&touched)?;
        self.publish(&txn, &format!("Restore archive of {day}"), true, &files, &touched, &staged, Vec::new())
    }
}
