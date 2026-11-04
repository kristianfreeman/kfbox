//! A USB stick for CDJs, written with rbxport's `rbl-export`: the audio under
//! `Contents/`, each track's analysis with its cues, colours and loops under
//! `PIONEER/USBANLZ/`, and `export.pdb` and `exportLibrary.db`.
//!
//! Exporting to a stick that already holds one of these is a sync: only what
//! changed is written, and tracks no longer chosen come off. Nothing here
//! writes to the library; the stick is the only thing that changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rbl_export::{ExportOptions, ExportProgress, ExportRoot, SourceMyTag, SourcePlaylist, SourceTrack};
use serde::{Deserialize, Serialize};

use crate::session::{CommandError, CommandResult, Session};

/// Where an export has got to, for a progress bar.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsbProgress {
    /// `checking`, `copying`, `database`, `verifying` or `publishing`.
    pub stage: String,
    pub done: usize,
    pub total: usize,
    /// The track just dealt with.
    pub title: String,
}

/// What an export did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsbReport {
    pub tracks: usize,
    pub playlists: usize,
    pub bytes_copied: u64,
    /// Tracks whose audio was already on the stick, unchanged.
    pub reused: usize,
    /// Tracks taken off because they're no longer chosen.
    pub removed: usize,
    /// Tracks left out, as "title: why".
    pub failed: Vec<String>,
    /// rbl-export read the stick back and found it sound.
    pub verified: bool,
    pub elapsed_ms: u64,
}

/// `djmdPlaylist.ID`s and `djmdContent.ID`s are numbers in the database.
fn number(id: &str) -> u64 {
    id.parse().unwrap_or(0)
}

/// The analysis files beside a `.DAT`, as (extension, bytes).
fn analysis_files(share_root: &Path, dat: &str) -> Vec<(String, Vec<u8>)> {
    if dat.is_empty() {
        return Vec::new();
    }
    let dat = share_root.join(dat.trim_start_matches('/'));
    ["DAT", "EXT", "2EX"].iter().filter_map(|ext| Some(((*ext).to_owned(), std::fs::read(dat.with_extension(ext)).ok()?))).collect()
}

impl Session {
    /// Writes the playlists at `chosen` (a folder brings everything in it),
    /// their tracks and the library's My Tags to the stick at `destination`.
    /// `file_system` is the stick's, as the drive list gives it, which picks
    /// the folder rekordbox would use there.
    pub fn export_usb(
        &self,
        destination: &Path,
        chosen: &[usize],
        file_system: Option<&str>,
        progress: &mut dyn FnMut(&UsbProgress),
        cancelled: &dyn Fn() -> bool,
    ) -> CommandResult<UsbReport> {
        if !destination.is_dir() {
            return Err(CommandError::Refused(format!("{} isn't a folder or a mounted drive", destination.display())));
        }
        let location = self.library_location()?;
        if destination.starts_with(&location.share_root) || location.master_db.starts_with(destination) {
            return Err(CommandError::Refused("that's rekordbox's own library folder, not a stick".to_owned()));
        }
        let (tracks, playlists) = self.selection(chosen)?;
        if tracks.is_empty() {
            return Err(CommandError::Refused("those playlists hold no tracks".to_owned()));
        }

        let (extras, tag_rows) = crate::master::with_copy(&location, |db| {
            let ids: Vec<String> = tracks.iter().map(|t| t.0.clone()).collect();
            Ok((rbl_db::export_info::track_extras(db.connection(), &ids)?, rbl_db::export_info::my_tags(db.connection())?))
        })?;
        let source: Vec<SourceTrack> = tracks
            .iter()
            .map(|(id, row)| {
                let extra = extras.get(id).cloned().unwrap_or_default();
                let facts = extra.analysis.unwrap_or_default();
                SourceTrack {
                    id: number(id),
                    source_path: PathBuf::from(&row.path),
                    title: row.title.clone(),
                    artist: row.artist.clone(),
                    album: row.album.clone(),
                    genre: row.genre.clone(),
                    key: if facts.key.is_empty() { row.key.clone() } else { facts.key.clone() },
                    date_added: row.added.clone(),
                    bpm_x100: facts.bpm_x100,
                    duration_sec: u16::try_from(facts.length_sec).unwrap_or(u16::MAX),
                    rating: row.rating,
                    analysis: analysis_files(&location.share_root, &facts.analysis_path),
                    // The library's cues, as rekordbox writes them to a stick.
                    cues: Some(extra.cues),
                    my_tags: extra.my_tags.iter().map(|t| number(t)).collect(),
                    ..SourceTrack::default()
                }
            })
            .collect();
        let my_tags: Vec<SourceMyTag> = tag_rows
            .iter()
            .map(|t| SourceMyTag {
                id: number(&t.id),
                seq: u32::try_from(t.seq).unwrap_or(0),
                name: t.name.clone(),
                attribute: u8::try_from(t.attribute).unwrap_or(0),
                parent: if t.parent == "root" { 0 } else { number(&t.parent) },
            })
            .collect();
        let options = ExportOptions { root: file_system.and_then(ExportRoot::for_file_system), ..ExportOptions::default() };
        let mut report_progress = |p: &ExportProgress| {
            progress(&UsbProgress { stage: p.stage.to_owned(), done: p.done, total: p.total, title: p.title.clone() });
        };
        let report = rbl_export::export_cancellable(destination, &source, &playlists, &my_tags, &options, &mut report_progress, cancelled)
            .map_err(|e| CommandError::Refused(format!("the export stopped: {e}")))?;
        Ok(UsbReport {
            tracks: report.tracks,
            playlists: report.playlists,
            bytes_copied: report.bytes_copied,
            reused: report.reused,
            removed: report.removed,
            failed: report.failed.iter().chain(&report.skipped).cloned().collect(),
            verified: report.verified,
            elapsed_ms: report.elapsed_ms,
        })
    }

    /// The tracks of the chosen playlists, once each in order, and the
    /// playlists with the folders above them.
    #[allow(clippy::type_complexity, reason = "a private pair of lists")]
    fn selection(&self, chosen: &[usize]) -> CommandResult<(Vec<(String, crate::session::TrackRow)>, Vec<SourcePlaylist>)> {
        self.with_library(|library| {
            let mut wanted: HashSet<usize> = HashSet::new();
            for &index in chosen {
                let playlist = library.playlists.get(index).ok_or_else(|| CommandError::NoPlaylist(index.to_string()))?;
                wanted.insert(index);
                // A folder brings what's in it; a playlist brings the folders above it.
                for (i, other) in library.playlists.iter().enumerate() {
                    let inside = other.path.starts_with(&format!("{} › ", playlist.path));
                    let above = playlist.path.starts_with(&format!("{} › ", other.path));
                    if (playlist.folder && inside) || (above && other.folder) {
                        wanted.insert(i);
                    }
                }
            }
            let mut tracks: Vec<(String, crate::session::TrackRow)> = Vec::new();
            let mut at: HashMap<String, usize> = HashMap::new();
            let mut playlists = Vec::new();
            for (index, playlist) in library.playlists.iter().enumerate().filter(|(i, _)| wanted.contains(i)) {
                let parent = playlist
                    .path
                    .rsplit_once(" › ")
                    .and_then(|(above, _)| library.playlists.iter().find(|p| p.folder && p.path == above))
                    .and_then(|p| p.id.as_deref())
                    .map_or(0, number);
                let mut indices = Vec::new();
                if !playlist.folder {
                    for track in library.playlist_tracks(index) {
                        if track.path.as_ref().is_none_or(|p| !p.exists()) {
                            continue;
                        }
                        let slot = *at.entry(track.id.clone()).or_insert_with(|| {
                            tracks.push((track.id.clone(), crate::session::TrackRow::of(track)));
                            tracks.len() - 1
                        });
                        indices.push(slot);
                    }
                }
                playlists.push(SourcePlaylist {
                    id: playlist.id.as_deref().map_or(0, number),
                    name: playlist.name.clone(),
                    parent_id: parent,
                    folder: playlist.folder,
                    track_indices: indices,
                    ..SourcePlaylist::default()
                });
            }
            Ok((tracks, playlists))
        })
    }
}
