//! The edits a change can make, and the checks that each one landed.

use std::path::PathBuf;

use rbl_db::write::{AnalysisRegistration, Writer};
use rbl_db::{DbError, Library};
use rusqlite::{params, OptionalExtension};

use super::{FileChange, FileContent, SafeError};
use crate::track::LibraryAnalysis;

/// Where a cue goes on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// A hot cue, from 0 (A) to 15 (P).
    Hot(u8),
    Memory,
}

impl Slot {
    /// `A` to `P`, any case, or `memory`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("memory") {
            return Some(Self::Memory);
        }
        match name.as_bytes() {
            [letter] if (b'A'..=b'P').contains(&letter.to_ascii_uppercase()) => Some(Self::Hot(letter.to_ascii_uppercase() - b'A')),
            _ => None,
        }
    }

    /// The hot cue slot from 0 (A), or `None` for a memory cue.
    #[must_use]
    pub const fn hot(self) -> Option<u8> {
        match self {
            Self::Hot(slot) => Some(slot),
            Self::Memory => None,
        }
    }

    /// rekordbox's `Kind`: 0 is a memory cue, A to C are 1 to 3, 4 is unused,
    /// and D onward start at 5.
    #[must_use]
    pub const fn kind(self) -> u8 {
        match self {
            Self::Memory => 0,
            Self::Hot(slot) if slot < 3 => slot + 1,
            Self::Hot(slot) => slot + 2,
        }
    }
}

/// The `ColorTableIndex` closest to a colour, for a hot cue.
#[must_use]
pub fn hot_colour_index(rgb: [u8; 3]) -> Option<u8> {
    let distance = |other: &[u8; 3]| -> i32 { rgb.iter().zip(other).map(|(a, b)| (i32::from(*a) - i32::from(*b)).pow(2)).sum() };
    rbl_anlz::MEASURED_CUE_COLOURS.iter().min_by_key(|(_, colour)| distance(colour)).map(|(index, _)| *index)
}

#[derive(Debug, Clone)]
pub enum Op {
    /// Refused if the hot cue slot is already taken: rules never replace a cue.
    AddCue {
        track: String,
        slot: Slot,
        start_ms: u32,
        /// `ColorTableIndex` on a hot cue, the eight-value `Color` on a memory one.
        colour: Option<u8>,
        name: String,
    },
    AddLoop {
        track: String,
        slot: Slot,
        start_ms: u32,
        end_ms: u32,
        beats: u16,
        colour: Option<u8>,
        name: String,
    },
    MoveCue {
        cue: String,
        start_ms: u32,
    },
    SetCueColour {
        cue: String,
        colour: Option<u8>,
    },
    SetCueName {
        cue: String,
        name: String,
    },
    DeleteCue {
        cue: String,
    },
    /// A song file added to the library where it is, with its analysis files
    /// and BPM when given. Refused if the library already holds that path.
    ImportTrack {
        path: PathBuf,
        analysis: Option<Box<LibraryAnalysis>>,
    },
    /// A track's file, found at another path.
    Relocate {
        track: String,
        path: PathBuf,
    },
}

/// What a change did to the cue table, to check against the file afterwards.
#[derive(Debug, Default)]
pub(super) struct Applied {
    pub added: Vec<String>,
    deleted: usize,
    expected: Vec<Expected>,
    /// The IDs of tracks added to the library.
    pub imported: Vec<String>,
    /// Analysis files the ops made, which go into the share folder with the change.
    pub files: Vec<FileChange>,
}

#[derive(Debug)]
struct Expected {
    cue: String,
    track: Option<String>,
    kind: Option<u8>,
    start_ms: Option<u32>,
    end_ms: Option<u32>,
    name: Option<String>,
    colour: Option<u8>,
}

impl Expected {
    fn on(cue: &str) -> Self {
        Self { cue: cue.to_owned(), track: None, kind: None, start_ms: None, end_ms: None, name: None, colour: None }
    }
}

/// A cue as read back from the file.
struct CueRow {
    track: Option<String>,
    kind: Option<i64>,
    start: Option<i64>,
    end: Option<i64>,
    name: Option<String>,
    colour: Option<i64>,
    table_colour: Option<i64>,
}

impl CueRow {
    fn holds(&self, want: &Expected) -> bool {
        // A memory cue keeps its colour in `Color`, a hot cue in `ColorTableIndex`.
        let colour = if matches!(self.kind, Some(0 | 4)) { self.colour } else { self.table_colour };
        want.track.as_ref().is_none_or(|t| self.track.as_ref() == Some(t))
            && want.kind.is_none_or(|k| self.kind == Some(i64::from(k)))
            && want.start_ms.is_none_or(|s| self.start == Some(i64::from(s)))
            && want.end_ms.is_none_or(|e| self.end == Some(i64::from(e)))
            && want.name.as_ref().is_none_or(|n| self.name.as_deref().unwrap_or_default() == n)
            && want.colour.is_none_or(|c| colour == Some(i64::from(c)))
    }
}

pub(super) fn cue_count(library: &Library) -> Result<i64, SafeError> {
    Ok(library.connection().query_row("SELECT COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0", [], |r| r.get(0))?)
}

pub(super) fn track_count(library: &Library) -> Result<i64, SafeError> {
    Ok(library.connection().query_row("SELECT COUNT(*) FROM djmdContent WHERE rb_local_deleted = 0", [], |r| r.get(0))?)
}

/// Sets a cue's comment. rbl-db has no call for it, so this does what its
/// other cue edits do: the column, the row's `rb_local_usn`, its `updated_at`,
/// and the library's update counter.
fn set_name(writer: &Writer, cue: &str, name: &str) -> Result<(), SafeError> {
    let connection = writer.library().connection();
    let usn: i64 =
        connection
            .query_row("SELECT COALESCE(int_1, 0) + 1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'", [], |r| r.get(0))?;
    let stamp = rbl_core::time::now();
    let rows = connection.execute(
        "UPDATE djmdCue SET Comment = ?2, rb_local_usn = ?3, updated_at = ?4 WHERE ID = ?1 AND rb_local_deleted = 0",
        params![cue, name, usn, stamp],
    )?;
    if rows != 1 {
        return Err(SafeError::Db(DbError::WriteRefused(format!("no cue {cue}"))));
    }
    connection
        .execute("UPDATE agentRegistry SET int_1 = ?1, updated_at = ?2 WHERE registry_id = 'localUpdateCount'", params![usn, stamp])?;
    Ok(())
}

fn refuse_taken_slot(writer: &Writer, track: &str, slot: Slot) -> Result<(), SafeError> {
    if slot == Slot::Memory {
        return Ok(());
    }
    let taken: Option<String> = writer
        .library()
        .connection()
        .query_row("SELECT ID FROM djmdCue WHERE ContentID = ?1 AND Kind = ?2 AND rb_local_deleted = 0", params![track, slot.kind()], |r| {
            r.get(0)
        })
        .optional()?;
    match taken {
        Some(cue) => Err(SafeError::SlotTaken { track: track.to_owned(), cue }),
        None => Ok(()),
    }
}

fn finish_added(writer: &mut Writer, id: &str, colour: Option<u8>, name: &str) -> Result<(), SafeError> {
    if colour.is_some() {
        writer.set_cue_colour(id, colour)?;
    }
    if !name.is_empty() {
        set_name(writer, id, name)?;
    }
    Ok(())
}

/// Writes a new track's analysis files and points its row at them. The files
/// go in with the rest of the change; the row is written now, in the staged copy.
fn register(writer: &mut Writer, id: &str, path: &std::path::Path, analysis: &LibraryAnalysis) -> Result<Vec<FileChange>, SafeError> {
    let dat = writer.analysis_data_path_for(id)?;
    let song_path = rbl_anlz::share_tree_ppth(&path.to_string_lossy());
    let overview: Option<&[[u8; 3]; rbl_anlz::encode::OVERVIEW_COLUMNS]> = analysis.overview.as_slice().try_into().ok();
    let files =
        rbl_anlz::encode::author_with_overview(&song_path, &analysis.beats, &analysis.columns, overview, rbl_anlz::Existing::default());
    let relative = PathBuf::from(dat.trim_start_matches('/'));
    let mut changes = Vec::new();
    for (extension, bytes) in [("DAT", files.dat), ("EXT", files.ext), ("2EX", files.two_ex)] {
        changes.push(FileChange { relative: relative.with_extension(extension), content: FileContent::Bytes(bytes) });
    }
    let register = |key| AnalysisRegistration { bpm_x100: analysis.bpm_x100, key, analysis_data_path: &dat };
    // A key name rekordbox has no row for is refused: the track keeps no key
    // rather than losing its analysis.
    if writer.register_analysis(id, &register(analysis.key.as_deref())).is_err() {
        writer.register_analysis(id, &register(None))?;
    }
    Ok(changes)
}

/// Applies every op through `writer`, which is open on a staged copy.
pub(super) fn apply(writer: &mut Writer, ops: &[Op]) -> Result<Applied, SafeError> {
    let mut applied = Applied::default();
    for op in ops {
        match op {
            Op::AddCue { track, slot, start_ms, colour, name } => {
                refuse_taken_slot(writer, track, *slot)?;
                let id = writer.add_cue(track, slot.kind(), *start_ms)?;
                finish_added(writer, &id, *colour, name)?;
                applied.expected.push(Expected {
                    cue: id.clone(),
                    track: Some(track.clone()),
                    kind: Some(slot.kind()),
                    start_ms: Some(*start_ms),
                    end_ms: None,
                    name: Some(name.clone()),
                    colour: *colour,
                });
                applied.added.push(id);
            }
            Op::AddLoop { track, slot, start_ms, end_ms, beats, colour, name } => {
                refuse_taken_slot(writer, track, *slot)?;
                let id = writer.add_loop(track, slot.kind(), *start_ms, *end_ms, *beats)?;
                finish_added(writer, &id, *colour, name)?;
                applied.expected.push(Expected {
                    cue: id.clone(),
                    track: Some(track.clone()),
                    kind: Some(slot.kind()),
                    start_ms: Some(*start_ms),
                    end_ms: Some(*end_ms),
                    name: Some(name.clone()),
                    colour: *colour,
                });
                applied.added.push(id);
            }
            Op::MoveCue { cue, start_ms } => {
                writer.move_cue(cue, *start_ms)?;
                applied.expected.push(Expected { start_ms: Some(*start_ms), ..Expected::on(cue) });
            }
            Op::SetCueColour { cue, colour } => {
                writer.set_cue_colour(cue, *colour)?;
                applied.expected.push(Expected { colour: *colour, ..Expected::on(cue) });
            }
            Op::SetCueName { cue, name } => {
                set_name(writer, cue, name)?;
                applied.expected.push(Expected { name: Some(name.clone()), ..Expected::on(cue) });
            }
            Op::ImportTrack { path, analysis } => {
                let id = writer.import_file(path)?;
                if let Some(analysis) = analysis {
                    applied.files.extend(register(writer, &id, path, analysis)?);
                }
                applied.imported.push(id);
            }
            Op::Relocate { track, path } => {
                if writer.relocate(track, path)?.rows != 1 {
                    return Err(SafeError::Db(DbError::WriteRefused(format!("no track {track}"))));
                }
            }
            Op::DeleteCue { cue } => {
                let changed = writer.delete_cue(cue)?;
                if changed.rows == 0 {
                    return Err(SafeError::Db(DbError::WriteRefused(format!("no cue {cue}"))));
                }
                applied.deleted += 1;
                // Later ops on a deleted cue would have failed, so it stays gone.
                applied.expected.retain(|e| e.cue != *cue);
                applied.added.retain(|id| id != cue);
            }
        }
    }
    Ok(applied)
}

/// Rereads the staged file: it must be sound, and hold exactly this change.
pub(super) fn verify(library: &Library, before_cues: i64, before_tracks: i64, applied: &Applied) -> Result<(), SafeError> {
    check_integrity(library)?;
    let bad = |what: String| Err(SafeError::Verify(what));
    let added = i64::try_from(applied.added.len()).unwrap_or(i64::MAX);
    let deleted = i64::try_from(applied.deleted).unwrap_or(i64::MAX);
    // An added cue deleted again in the same change counts on neither side,
    // since it left `added`, but its deletion was still counted.
    let cues = cue_count(library)?;
    if cues > before_cues + added || cues < before_cues + added - deleted {
        return bad(format!("{cues} cues after the change, from {before_cues} with {added} added and {deleted} deleted"));
    }
    let imported = i64::try_from(applied.imported.len()).unwrap_or(i64::MAX);
    if track_count(library)? != before_tracks + imported {
        return bad(format!("the number of tracks changed by other than the {imported} imported"));
    }
    for want in &applied.expected {
        let row = library
            .connection()
            .query_row(
                "SELECT ContentID, Kind, InMsec, OutMsec, Comment, Color, ColorTableIndex FROM djmdCue WHERE ID = ?1 AND rb_local_deleted = 0",
                [&want.cue],
                |r| {
                    Ok(CueRow {
                        track: r.get(0)?,
                        kind: r.get(1)?,
                        start: r.get(2)?,
                        end: r.get(3)?,
                        name: r.get(4)?,
                        colour: r.get(5)?,
                        table_colour: r.get(6)?,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            return bad(format!("cue {} isn't in the file", want.cue));
        };
        if !row.holds(want) {
            return bad(format!("cue {} doesn't hold what was written", want.cue));
        }
    }
    Ok(())
}

pub(super) fn check_integrity(library: &Library) -> Result<(), SafeError> {
    let result: String = library.connection().query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(SafeError::Verify(format!("integrity check: {result}")))
    }
}
