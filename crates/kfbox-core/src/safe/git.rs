//! The private repository under the library's history: one commit per
//! action. Plain `git`, kept apart from the user's own configuration and
//! hooks, with its own automatic housekeeping off.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::SafeError;

/// How often the repository is packed. Each commit adds a full 3 MB compressed
/// copy of `master.db` until it is packed, down to well under 100 KB each.
const PACK_EVERY_COMMITS: u64 = 25;

/// A commit in the history, newest first when listed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryEntry {
    pub commit: String,
    pub unix_secs: i64,
    pub summary: String,
    pub kind: CommitKind,
}

/// Why a commit exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitKind {
    /// The library as kfbox first saw it.
    First,
    /// An action in kfbox.
    Action,
    /// A restore to an earlier commit.
    Restore,
    /// Changes made outside kfbox, found before kfbox's next write.
    Outside,
}

impl CommitKind {
    const fn trailer(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Action => "action",
            Self::Restore => "restore",
            Self::Outside => "outside",
        }
    }

    fn parse(text: &str) -> Self {
        match text.trim() {
            "first" => Self::First,
            "restore" => Self::Restore,
            "outside" => Self::Outside,
            _ => Self::Action,
        }
    }
}

pub(super) struct Repo {
    dir: PathBuf,
}

impl Repo {
    pub(super) fn open(dir: &Path) -> Result<Self, SafeError> {
        let repo = Self { dir: dir.to_path_buf() };
        rbl_core::durable::create_dir_all(dir)?;
        if !dir.join(".git").exists() {
            repo.run(&["init", "-q", "-b", "main"])?;
        }
        // Only this process, which holds the library's lock, makes commits, so
        // a lock left here was left by a crash.
        let stale = dir.join(".git/index.lock");
        if stale.exists() {
            std::fs::remove_file(stale)?;
        }
        Ok(repo)
    }

    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=kfbox", "-c", "user.email=kfbox@localhost"])
            .args(["-c", "commit.gpgSign=false", "-c", "gc.auto=0", "-c", "core.fsync=all"])
            .arg("-C")
            .arg(&self.dir)
            .stdin(Stdio::null());
        command
    }

    fn run_bytes(&self, args: &[&str]) -> Result<Vec<u8>, SafeError> {
        let output = self.command().args(args).output().map_err(|e| SafeError::Git(format!("git didn't run: {e}")))?;
        if !output.status.success() {
            return Err(SafeError::Git(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim())));
        }
        Ok(output.stdout)
    }

    fn run(&self, args: &[&str]) -> Result<String, SafeError> {
        Ok(String::from_utf8_lossy(&self.run_bytes(args)?).into_owned())
    }

    /// Commits everything in the working tree. `None` when nothing changed.
    pub(super) fn commit_all(&self, summary: &str, kind: CommitKind) -> Result<Option<String>, SafeError> {
        self.run(&["add", "-A"])?;
        if self.run(&["status", "--porcelain"])?.trim().is_empty() {
            return Ok(None);
        }
        let trailer = format!("Kfbox-Kind: {}", kind.trailer());
        self.run(&["commit", "-q", "-m", summary, "-m", &trailer])?;
        self.pack_if_due()?;
        self.run(&["rev-parse", "HEAD"]).map(|head| Some(head.trim().to_owned()))
    }

    fn pack_if_due(&self) -> Result<(), SafeError> {
        let count: u64 = self.run(&["rev-list", "--count", "HEAD"])?.trim().parse().unwrap_or(0);
        if count.is_multiple_of(PACK_EVERY_COMMITS) {
            // In the background: a commit shouldn't wait for it, and git keeps
            // a running pack safe from a commit made alongside it.
            let mut child = self.command().args(["gc", "-q"]).spawn().map_err(|e| SafeError::Git(format!("git gc didn't run: {e}")))?;
            std::thread::spawn(move || child.wait());
        }
        Ok(())
    }

    pub(super) fn pack_now(&self) -> Result<(), SafeError> {
        self.run(&["gc", "-q"]).map(|_| ())
    }

    pub(super) fn history(&self, limit: usize) -> Result<Vec<HistoryEntry>, SafeError> {
        if self.run_bytes(&["rev-parse", "-q", "--verify", "HEAD"]).is_err() {
            return Ok(Vec::new());
        }
        let log = self.run(&["log", &format!("-{limit}"), "--format=%H%x1f%ct%x1f%s%x1f%(trailers:key=Kfbox-Kind,valueonly)%x1e"])?;
        Ok(log
            .split('\u{1e}')
            .filter_map(|row| {
                let mut fields = row.trim().split('\u{1f}');
                Some(HistoryEntry {
                    commit: fields.next().filter(|c| !c.is_empty())?.to_owned(),
                    unix_secs: fields.next()?.parse().ok()?,
                    summary: fields.next()?.to_owned(),
                    kind: CommitKind::parse(fields.next().unwrap_or_default()),
                })
            })
            .collect())
    }

    /// The parent of `commit`, unless it is the first.
    pub(super) fn parent(&self, commit: &str) -> Option<String> {
        self.run_bytes(&["rev-parse", "-q", "--verify", &format!("{commit}^")])
            .ok()
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Resolves a commit name, so a mistyped one fails before anything changes.
    pub(super) fn resolve(&self, commit: &str) -> Result<String, SafeError> {
        let id = self
            .run(&["rev-parse", "--verify", "-q", &format!("{commit}^{{commit}}")])
            .map_err(|_| SafeError::NoSuchCommit(commit.to_owned()))?;
        Ok(id.trim().to_owned())
    }

    pub(super) fn file_at(&self, commit: &str, path: &str) -> Result<Vec<u8>, SafeError> {
        self.run_bytes(&["show", &format!("{commit}:{path}")])
    }

    /// Every file under `prefix` in `commit`, relative to `prefix`.
    pub(super) fn files_at(&self, commit: &str, prefix: &str) -> Result<Vec<String>, SafeError> {
        let listing = self.run_bytes(&["ls-tree", "-r", "-z", "--name-only", commit, "--", prefix])?;
        Ok(listing
            .split(|b| *b == 0)
            .filter(|name| !name.is_empty())
            .filter_map(|name| String::from_utf8_lossy(name).strip_prefix(&format!("{prefix}/")).map(str::to_owned))
            .collect())
    }

    /// Whether git finds the repository intact.
    pub(super) fn check(&self) -> Result<(), SafeError> {
        self.run(&["fsck", "--no-progress", "--strict"]).map(|_| ())
    }

    pub(super) fn size_bytes(&self) -> u64 {
        fn walk(dir: &Path) -> u64 {
            std::fs::read_dir(dir).map_or(0, |entries| {
                entries
                    .flatten()
                    .map(|e| match e.metadata() {
                        Ok(m) if m.is_dir() => walk(&e.path()),
                        Ok(m) => m.len(),
                        Err(_) => 0,
                    })
                    .sum()
            })
        }
        walk(&self.dir.join(".git"))
    }
}
