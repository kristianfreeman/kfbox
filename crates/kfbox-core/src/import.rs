//! Adding song files to rekordbox's library where they are, analysed and
//! cued, and finding the ones it has lost.
//!
//! An import is two changes in the history: `Import N tracks`, which adds the
//! rows and their analysis files (grid, waveforms, BPM and key), then
//! `Apply rules to N tracks` with the cues. Each undoes on its own. Files stay
//! where they are; nothing is copied or moved.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::safe::Op;
use crate::session::{CommandError, CommandResult, Origin, Saved, Session, TrackRow};
use crate::track::Track;

/// How deep and how wide a dropped folder is searched.
const FOLDER_DEPTH: usize = 8;
const FOLDER_ENTRIES: usize = 50_000;

/// Bytes from each end of a file that identify its audio.
const FINGERPRINT_BYTES: u64 = 256 * 1024;

/// A file the library already holds under another path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Duplicate {
    pub path: String,
    /// The track it duplicates.
    pub of: TrackRow,
    /// `copy`: the same bytes, so it wasn't imported. `likely`: the same
    /// artist, title and length in another file, imported anyway.
    pub kind: String,
}

/// A file that couldn't be imported, or imported without its analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub why: String,
}

/// What an import did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ImportReport {
    /// The new tracks, as the library now has them, cues included.
    pub imported: Vec<TrackRow>,
    /// Files the library already holds at that path.
    pub already: Vec<String>,
    pub duplicates: Vec<Duplicate>,
    /// Imported without analysis, or not imported at all, and why.
    pub problems: Vec<Problem>,
    /// The history's commits: the import, then the cues.
    pub commits: Vec<String>,
}

/// Every audio file named, a folder's own included, once each, in order.
#[must_use]
pub fn audio_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for path in paths {
        let files = if path.is_dir() {
            let mut budget = rbl_db::import::WalkBudget::new(FOLDER_DEPTH, FOLDER_ENTRIES);
            let mut files = rbl_db::import::audio_files_in(path, &mut budget);
            files.sort();
            files
        } else if rbl_db::import::is_audio(path) {
            vec![path.clone()]
        } else {
            Vec::new()
        };
        for file in files {
            if seen.insert(file.clone()) {
                found.push(file);
            }
        }
    }
    found
}

/// The file's size and a hash of its two ends: equal for a copy of the same
/// file, whatever it's called.
fn fingerprint(path: &Path) -> Option<(u64, Vec<u8>)> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; usize::try_from(FINGERPRINT_BYTES.min(size)).ok()?];
    file.read_exact(&mut buffer).ok()?;
    hash.update(&buffer);
    file.seek(SeekFrom::Start(size.saturating_sub(FINGERPRINT_BYTES))).ok()?;
    buffer.clear();
    file.read_to_end(&mut buffer).ok()?;
    hash.update(&buffer);
    Some((size, hash.finalize().to_vec()))
}

/// Artist, title and length to the nearest two seconds, for a likely duplicate.
fn identity(artist: &str, title: &str, secs: f64) -> Option<(String, String, i64)> {
    let (artist, title) = (artist.trim().to_lowercase(), title.trim().to_lowercase());
    #[allow(clippy::cast_possible_truncation, reason = "a song's length")]
    let rounded = (secs / 2.0).round() as i64;
    (!artist.is_empty() && !title.is_empty() && secs > 0.0).then_some((artist, title, rounded))
}

impl Session {
    /// Adds song files, and the audio files in folders, to the library where
    /// they are: analysed, with the rules' cues. Files the library already
    /// holds, by path or as a byte-for-byte copy, are left out.
    pub fn import_files(&self, paths: &[PathBuf]) -> CommandResult<ImportReport> {
        let files = audio_files(paths);
        if files.is_empty() {
            return Err(CommandError::Refused("there are no song files there to import".to_owned()));
        }
        let rows = self.playlist_rows(None)?;
        let by_path: HashMap<PathBuf, &TrackRow> =
            rows.iter().filter(|r| !r.path.is_empty()).map(|r| (PathBuf::from(&r.path), r)).collect();
        let mut report = ImportReport::default();

        // A copy has the same size, so only same-sized tracks are hashed.
        let sizes: HashMap<u64, Vec<&TrackRow>> = {
            let wanted: HashSet<u64> = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).collect();
            let mut sizes: HashMap<u64, Vec<&TrackRow>> = HashMap::new();
            for row in rows.iter().filter(|r| !r.path.is_empty() && !r.missing) {
                if let Ok(meta) = std::fs::metadata(&row.path) {
                    if wanted.contains(&meta.len()) {
                        sizes.entry(meta.len()).or_default().push(row);
                    }
                }
            }
            sizes
        };
        let identities: HashMap<(String, String, i64), &TrackRow> =
            rows.iter().filter_map(|r| identity(&r.artist, &r.title, r.duration_secs).map(|id| (id, r))).collect();

        let mut ops = Vec::new();
        let mut likely = Vec::new();
        for file in files {
            let shown = file.display().to_string();
            if by_path.contains_key(&file) {
                report.already.push(shown);
                continue;
            }
            let print = fingerprint(&file);
            let copy = print.as_ref().and_then(|(size, hash)| {
                sizes.get(size)?.iter().find(|row| fingerprint(Path::new(&row.path)).is_some_and(|(_, h)| h == *hash)).copied()
            });
            if let Some(of) = copy {
                report.duplicates.push(Duplicate { path: shown, of: of.clone(), kind: "copy".to_owned() });
                continue;
            }
            let analysis = match Track::analyse_for_library(&file) {
                Ok((track, analysis)) => {
                    if let Some(of) =
                        identity(&track.tags.artist, &track.tags.title, track.duration_secs).and_then(|id| identities.get(&id))
                    {
                        likely.push(Duplicate { path: shown.clone(), of: (*of).clone(), kind: "likely".to_owned() });
                    }
                    Some(Box::new(analysis))
                }
                Err(e) => {
                    report.problems.push(Problem { path: shown, why: format!("imported without analysis: {e}") });
                    None
                }
            };
            ops.push(Op::ImportTrack { path: file, analysis });
        }
        report.duplicates.extend(likely);
        if ops.is_empty() {
            return Ok(report);
        }
        let count = ops.len();
        let summary = if count == 1 { "Import 1 track".to_owned() } else { format!("Import {count} tracks") };
        let saved = self.write(&summary, ops, Origin::Hand)?;
        report.commits.extend(saved.commit);
        // The cues, as their own change: undoing them keeps the tracks.
        let cued = self.write_rule_cues(&saved.imported, None)?;
        report.commits.extend(cued.commit);
        let fresh = self.playlist_rows(None)?;
        report.imported = saved.imported.iter().filter_map(|id| fresh.iter().find(|r| r.id == *id).cloned()).collect();
        Ok(report)
    }

    /// The library's tracks whose files aren't where it says.
    pub fn missing_files(&self) -> CommandResult<Vec<TrackRow>> {
        Ok(self.playlist_rows(None)?.into_iter().filter(|r| r.missing).collect())
    }

    /// Points missing tracks at files of the same name found in `folder` and
    /// the folders in it, as one change. Tracks with no such file stay missing.
    pub fn relocate_missing(&self, folder: &Path) -> CommandResult<Saved> {
        let missing = self.missing_files()?;
        let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for file in audio_files(&[folder.to_path_buf()]) {
            if let Some(name) = file.file_name() {
                by_name.entry(name.to_string_lossy().to_lowercase()).or_default().push(file);
            }
        }
        let ops: Vec<Op> = missing
            .iter()
            .filter_map(|row| {
                let name = Path::new(&row.path).file_name()?.to_string_lossy().to_lowercase();
                // Two files of that name: which is meant can't be told.
                match by_name.get(&name)?.as_slice() {
                    [only] => Some(Op::Relocate { track: row.id.clone(), path: only.clone() }),
                    _ => None,
                }
            })
            .collect();
        if ops.is_empty() {
            return Ok(Saved::default());
        }
        let summary = if ops.len() == 1 { "Find 1 missing file".to_owned() } else { format!("Find {} missing files", ops.len()) };
        let saved = self.write(&summary, ops, Origin::Hand)?;
        // A relocated row is a changed row even with its cues unchanged.
        Ok(saved)
    }
}
