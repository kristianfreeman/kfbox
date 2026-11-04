//! A USB stick for CDJs, written with rbxport's `rbl-export`: the audio under
//! `Contents/`, each track's analysis with its cues, colours and loops under
//! `PIONEER/USBANLZ/`, and `export.pdb` and `exportLibrary.db`.
//!
//! Exporting to a stick that already holds one of these is a sync: only what
//! changed is written, and tracks no longer chosen come off. Nothing here
//! writes to the library; the stick is the only thing that changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rbl_export::{ExportOptions, ExportProgress, ExportRoot, SourceMyTag, SourcePlaylist, SourceTrack, SyncNode, SyncSource};
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

/// How an export treats what's already on the stick.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsbOptions {
    /// The stick's file system, as the drive list gives it, which picks the
    /// folder rekordbox would use there.
    pub file_system: Option<String>,
    /// Take off tracks the stick holds that the chosen playlists don't.
    /// Without it they stay.
    pub remove: bool,
    /// The stick holds rekordbox's export, and it's to be replaced: its
    /// `PIONEER` and `Contents` folders go, and nothing else on the stick.
    pub replace: bool,
}

/// What an export of some playlists hands rbl-export.
struct Prepared {
    tracks: Vec<SourceTrack>,
    playlists: Vec<SourcePlaylist>,
    my_tags: Vec<SourceMyTag>,
    sync: SyncSource,
}

/// What a stick already holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StickInfo {
    /// kfbox (or rbxport) wrote it last; otherwise rekordbox did.
    pub ours: bool,
    pub tracks: usize,
    /// Its playlists, with the folders above them joined by ` › `.
    pub playlists: Vec<String>,
}

/// What an export would do to a stick, worked out before anything is written.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsbPlan {
    /// What's on the stick now; `None` for one with no export yet.
    pub stick: Option<StickInfo>,
    /// The stick holds rekordbox's export, which kfbox replaces rather than
    /// updates: everything chosen is copied fresh, and the rest goes.
    pub replaces: bool,
    /// Tracks to copy on.
    pub add: usize,
    /// Tracks already there whose cues will change.
    pub cues_changed: usize,
    /// Tracks already there that stay.
    pub keep: usize,
    /// Tracks to take off.
    pub remove: usize,
    /// Chosen tracks whose files can't be found.
    pub missing: usize,
    pub copy_bytes: u64,
}

/// The stick's export, read from its `export.pdb`, if it has one.
#[must_use]
pub fn read_stick(destination: &Path) -> Option<StickInfo> {
    let bytes = std::fs::read(rbl_export::export_root(destination).join("rekordbox/export.pdb")).ok()?;
    let pdb = rbl_pdb::Pdb::parse(&bytes).ok()?;
    let tracks = pdb.table(rbl_pdb::PageType::Tracks).map_or(0, |t| pdb.track_rows(t).len());
    let nodes = pdb.table(rbl_pdb::PageType::PlaylistTree).map(|t| pdb.playlist_nodes(t)).unwrap_or_default();
    let path_of = |node: &rbl_pdb::PlaylistNode| {
        let mut parts = vec![node.name.clone()];
        let mut parent = node.parent_id;
        // A corrupt tree can't loop: no path is deeper than the tree is big.
        for _ in 0..nodes.len() {
            let Some(above) = nodes.iter().find(|n| n.id == parent && parent != 0) else { break };
            parts.insert(0, above.name.clone());
            parent = above.parent_id;
        }
        parts.join(" › ")
    };
    let playlists = nodes.iter().filter(|n| !n.is_folder).map(path_of).collect();
    Some(StickInfo { ours: rbl_export::Manifest::load(destination).is_some(), tracks, playlists })
}

/// The library IDs of the tracks the stick holds.
fn stick_track_ids(destination: &Path) -> HashSet<u64> {
    let Ok(bytes) = std::fs::read(rbl_export::export_root(destination).join("rekordbox/export.pdb")) else { return HashSet::new() };
    let Ok(pdb) = rbl_pdb::Pdb::parse(&bytes) else { return HashSet::new() };
    pdb.table(rbl_pdb::PageType::Tracks)
        .map(|t| pdb.track_rows(t))
        .unwrap_or_default()
        .iter()
        .map(|r| u64::from(r.master_content_id))
        .collect()
}

/// How many of these tracks the stick holds with other cues than the library's.
fn cues_changed(destination: &Path, tracks: &[SourceTrack]) -> usize {
    type Cues = std::collections::BTreeSet<(u32, u32, u32)>;
    let Ok(bytes) = std::fs::read(rbl_export::export_root(destination).join("rekordbox/export.pdb")) else { return 0 };
    let Ok(pdb) = rbl_pdb::Pdb::parse(&bytes) else { return 0 };
    let rows = pdb.table(rbl_pdb::PageType::Tracks).map(|t| pdb.track_rows(t)).unwrap_or_default();
    tracks
        .iter()
        .filter(|track| {
            let Some(row) = rows.iter().find(|r| u64::from(r.master_content_id) == track.id && track.id != 0) else { return false };
            let dat = destination.join(row.analyze_path.trim_start_matches('/'));
            let on_stick: Cues = [dat.clone(), dat.with_extension("EXT")]
                .iter()
                .filter_map(|p| rbl_anlz::Anlz::read(p).ok())
                .flat_map(|a| a.cue_entries())
                .map(|c| (c.hot_cue, c.time_ms, if c.kind == 2 { c.loop_time_ms } else { 0 }))
                .collect();
            let wanted: Cues = track.cues.iter().flatten().map(|c| (c.slot(), c.time_ms, c.loop_time_ms.unwrap_or(0))).collect();
            on_stick != wanted
        })
        .count()
}

/// Takes rekordbox's export off a stick: its `PIONEER` (or hidden
/// `.PIONEER`) folder and the `Contents` folder of audio it copied. Anything
/// else on the stick stays, and nothing is reformatted.
fn remove_rekordbox_export(destination: &Path) -> CommandResult<()> {
    for folder in ["PIONEER", ".PIONEER", "Contents"] {
        let path = destination.join(folder);
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        }
    }
    Ok(())
}

/// The `djmdPlaylist.ID` of the folder above the playlist at `path`, 0 at the top.
fn parent_of(library: &crate::RbLibrary, path: &str) -> u64 {
    path.rsplit_once(" › ")
        .and_then(|(above, _)| library.playlists.iter().find(|p| p.folder && p.path == above))
        .and_then(|p| p.id.as_deref())
        .map_or(0, number)
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
    /// What the stick needs from the library for these playlists: its tracks,
    /// the playlists and their folders, the My Tags, and who the library is,
    /// so a stick rekordbox wrote for it keeps the copies it already holds.
    fn prepare(&self, destination: &Path, chosen: &[usize]) -> CommandResult<Prepared> {
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
        let (extras, tag_rows, db_id) = crate::master::with_copy(&location, |db| {
            let ids: Vec<String> = tracks.iter().map(|t| t.0.clone()).collect();
            Ok((
                rbl_db::export_info::track_extras(db.connection(), &ids)?,
                rbl_db::export_info::my_tags(db.connection())?,
                rbl_db::export_info::db_id(db.connection())?,
            ))
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
        let tree = self.with_library(|library| {
            Ok(library
                .playlists
                .iter()
                .map(|p| SyncNode {
                    id: p.id.as_deref().map_or(0, number),
                    parent: parent_of(library, &p.path),
                    attribute: if p.folder {
                        1
                    } else if p.smart {
                        4
                    } else {
                        0
                    },
                })
                .collect())
        })?;
        Ok(Prepared { tracks: source, playlists, my_tags, sync: SyncSource { db_id, tree, ..SyncSource::default() } })
    }

    /// What exporting these playlists to the stick would do, before anything
    /// is written. `remove` says whether tracks the stick holds that the
    /// playlists don't are taken off; without it they stay.
    pub fn usb_plan(&self, destination: &Path, chosen: &[usize], remove: bool) -> CommandResult<UsbPlan> {
        let prepared = self.prepare(destination, chosen)?;
        let stick = read_stick(destination);
        if let Some(theirs) = stick.clone().filter(|s| !s.ours) {
            return Ok(UsbPlan {
                add: prepared.tracks.len(),
                remove: theirs.tracks,
                stick: Some(theirs),
                replaces: true,
                ..UsbPlan::default()
            });
        }
        let options = ExportOptions { sync: Some(&prepared.sync), keep_unlisted: !remove, ..ExportOptions::default() };
        // rbl-export's preview finds a stick that can't be synced (one left
        // half-written, say). Its counts aren't used: with tracks kept, it
        // counts each kept track as taken off and copied again, though the
        // export itself copies nothing for them.
        let preview = rbl_export::plan::preview(destination, &prepared.tracks, &prepared.playlists, &prepared.my_tags, &options)
            .map_err(|e| CommandError::Refused(format!("this stick can't be updated safely: {e}")))?;
        let held = stick_track_ids(destination);
        let chosen_ids: HashSet<u64> = prepared.tracks.iter().map(|t| t.id).collect();
        let added: Vec<&SourceTrack> = prepared.tracks.iter().filter(|t| !held.contains(&t.id)).collect();
        let removed = if remove { held.iter().filter(|id| !chosen_ids.contains(id)).count() } else { 0 };
        Ok(UsbPlan {
            stick,
            replaces: false,
            add: added.len(),
            cues_changed: cues_changed(destination, &prepared.tracks),
            keep: held.len() - removed,
            remove: removed,
            missing: preview.estimate.tracks_missing,
            copy_bytes: added.iter().filter_map(|t| std::fs::metadata(&t.source_path).ok()).map(|m| m.len()).sum(),
        })
    }

    /// Writes the playlists at `chosen` (a folder brings everything in it),
    /// their tracks and the library's My Tags to the stick at `destination`.
    /// A stick kfbox wrote is updated: copies already there stay, and tracks
    /// the playlists don't hold are taken off only when `options.remove`
    /// says so. A stick holding rekordbox's export is refused unless
    /// `options.replace` says to replace it.
    pub fn export_usb(
        &self,
        destination: &Path,
        chosen: &[usize],
        options: &UsbOptions,
        progress: &mut dyn FnMut(&UsbProgress),
        cancelled: &dyn Fn() -> bool,
    ) -> CommandResult<UsbReport> {
        let prepared = self.prepare(destination, chosen)?;
        if read_stick(destination).is_some_and(|s| !s.ours) {
            if !options.replace {
                return Err(CommandError::Refused(
                    "this stick holds rekordbox's export; replace it with kfbox's to export here".to_owned(),
                ));
            }
            remove_rekordbox_export(destination)?;
        }
        let export_options = ExportOptions {
            root: options.file_system.as_deref().and_then(ExportRoot::for_file_system),
            sync: Some(&prepared.sync),
            keep_unlisted: !options.remove,
            ..ExportOptions::default()
        };
        let mut report_progress = |p: &ExportProgress| {
            progress(&UsbProgress { stage: p.stage.to_owned(), done: p.done, total: p.total, title: p.title.clone() });
        };
        let report = rbl_export::export_cancellable(
            destination,
            &prepared.tracks,
            &prepared.playlists,
            &prepared.my_tags,
            &export_options,
            &mut report_progress,
            cancelled,
        )
        .map_err(|e| CommandError::Refused(format!("the export stopped, and the stick is as it was: {e}")))?;
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

    /// The library's playlists the stick at `destination` already has, by
    /// path (or by name, where only one has it), to tick for an update.
    pub fn stick_playlists(&self, destination: &Path) -> CommandResult<Vec<usize>> {
        let Some(stick) = read_stick(destination) else { return Ok(Vec::new()) };
        self.with_library(|library| {
            let mut found = Vec::new();
            for path in &stick.playlists {
                let name = path.rsplit(" › ").next().unwrap_or(path).to_lowercase();
                let by_path = library.playlists.iter().position(|p| !p.folder && p.path.eq_ignore_ascii_case(path));
                let named: Vec<usize> = library
                    .playlists
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| !p.folder && p.name.to_lowercase() == name)
                    .map(|(i, _)| i)
                    .collect();
                if let Some(index) = by_path.or(if named.len() == 1 { named.first().copied() } else { None }) {
                    if !found.contains(&index) {
                        found.push(index);
                    }
                }
            }
            Ok(found)
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
                let parent = parent_of(library, &playlist.path);
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
