//! Everything kfbox can do, as plain functions any front end calls: the
//! Tauri window, the MCP server, and later a local HTTP server.
//!
//! A [`Session`] holds what the commands share: the library or XML that's
//! open, the tracks analysed so far, and the analysis cache in `kfbox.db`.
//! Each command takes and returns plain serde types, so a wrapper only
//! translates. Nothing here writes to rekordbox's library: writes go through
//! [`crate::safe`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use rbl_db::LibraryLocation;

use serde::{Deserialize, Serialize};

use crate::cache::{self, Cache};
use crate::learning::{Correction, Learning, Placed};
use crate::rekordbox::{Mark, RbLibrary, RbTrack, TagCategory, TAG_SEPARATOR};
use crate::rules::{Applied, Cue, Flag, Landmarks, Rules, RulesError};
use crate::safe::{hot_colour_index, Change, HistoryEntry, Op, SafeError, SafeLibrary, Slot};
use crate::track::{GridSource, Track, TrackError};
use crate::{master, write_xml, DEFAULT_RULES};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("no rekordbox library or XML is open")]
    NotOpen,
    #[error("no track {0} in the open library")]
    NoTrack(String),
    #[error("no playlist {0} in the open library")]
    NoPlaylist(String),
    #[error("the track at {0} isn't loaded")]
    NotLoaded(String),
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Rules(#[from] RulesError),
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Master(#[from] master::MasterError),
    #[error(transparent)]
    Xml(#[from] crate::rekordbox::RekordboxError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("kfbox.db: {0}")]
    Cache(#[from] rusqlite::Error),
    #[error(transparent)]
    Safe(#[from] SafeError),
    #[error(transparent)]
    Db(#[from] rbl_db::DbError),
}

pub type CommandResult<T> = Result<T, CommandError>;

/// The app's bundle identifier, which names its data folder.
pub const APP_ID: &str = "com.kristianfreeman.kfbox";

/// A folder or playlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistItem {
    pub index: usize,
    /// The folders above it and its name, joined by ` › `.
    pub path: String,
    pub folder: bool,
    pub count: usize,
    /// An intelligent playlist: its rule picks its tracks, so they can't be edited.
    #[serde(default)]
    pub smart: bool,
}

/// What a library or XML offers: its whole collection and its playlists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Listing {
    pub total: usize,
    pub playlists: Vec<PlaylistItem>,
    /// The library's My Tag columns and their tags.
    #[serde(default)]
    pub my_tags: Vec<TagCategory>,
}

/// A track as the collection describes it, before any analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackRow {
    pub id: String,
    /// Empty when the collection entry is not a file.
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub bpm: f64,
    pub key: String,
    pub duration_secs: f64,
    /// When it joined the collection, as `YYYY-MM-DD`; empty when unknown.
    pub added: String,
    /// The letters of the hot cues already in rekordbox.
    pub hot: String,
    pub memory: usize,
    /// Its file isn't where the library says.
    #[serde(default)]
    pub missing: bool,
    /// Stars, 0 to 5.
    #[serde(default)]
    pub rating: u8,
    /// Its My Tags, each as `Category: Tag`.
    #[serde(default)]
    pub my_tags: Vec<String>,
}

impl TrackRow {
    pub(crate) fn of(t: &RbTrack) -> Self {
        Self {
            id: t.id.clone(),
            path: t.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            title: t.attribute("Name"),
            artist: t.attribute("Artist"),
            album: t.attribute("Album"),
            genre: t.attribute("Genre"),
            bpm: t.attribute("AverageBpm").parse().unwrap_or(0.0),
            key: t.attribute("Tonality"),
            duration_secs: t.attribute("TotalTime").parse().unwrap_or(0.0),
            added: t.attribute("DateAdded").chars().take(10).collect(),
            hot: t.marks.iter().filter_map(|m| u8::try_from(m.num).ok()).map(|n| char::from(b'A'.saturating_add(n))).collect(),
            memory: t.marks.iter().filter(|m| m.num < 0).count(),
            missing: t.path.as_ref().is_some_and(|p| !p.exists()),
            // A library row holds stars; an XML holds 0 to 255 in steps of 51.
            rating: t.attribute("Rating").parse::<u16>().map_or(0, |r| u8::try_from(if r > 5 { r / 51 } else { r }).unwrap_or(0)),
            my_tags: t.attribute("MyTags").split(TAG_SEPARATOR).filter(|s| !s.is_empty()).map(str::to_owned).collect(),
        }
    }

    fn haystack(&self) -> String {
        format!("{} {} {} {} {}", self.title, self.artist, self.album, self.genre, self.key).to_lowercase()
    }
}

/// What the table shows of an analysed track. A whole [`Track`] is large, so
/// only the track on screen is sent in full.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub bpm: f64,
    pub key: Option<String>,
    pub camelot: Option<String>,
    pub duration_secs: f64,
    pub grid: GridSource,
    /// Read from `kfbox.db` rather than decoded.
    pub cached: bool,
}

impl Analysis {
    fn of(track: &Track, cached: bool) -> Self {
        Self {
            path: track.path.to_string_lossy().into_owned(),
            title: track.tags.title.clone(),
            artist: track.tags.artist.clone(),
            bpm: track.bpm,
            key: track.key.clone(),
            camelot: track.camelot.clone(),
            duration_secs: track.duration_secs,
            grid: track.grid,
            cached,
        }
    }
}

/// One change to a track's cues, as a front end asks for it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CueEdit {
    /// A new cue. A hot cue slot that's taken is refused unless `replace` is
    /// set, when the cue there is deleted in the same change.
    Add {
        track: String,
        /// `A` to `P`, or `memory`.
        slot: String,
        start_secs: f64,
        /// Makes it a loop.
        #[serde(default)]
        end_secs: Option<f64>,
        #[serde(default)]
        name: String,
        /// `#rrggbb`, matched to rekordbox's nearest hot cue colour.
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        replace: bool,
    },
    Move {
        cue: String,
        start_secs: f64,
    },
    Rename {
        cue: String,
        name: String,
    },
    Recolour {
        cue: String,
        color: Option<String>,
    },
    Delete {
        cue: String,
    },
}

/// What a write did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Saved {
    /// The history's commit, or `None` if nothing changed.
    pub commit: Option<String>,
    /// The IDs of new cues.
    pub added: Vec<String>,
    /// Every track whose cues changed, as the library now has it.
    pub changed: Vec<TrackRow>,
    /// The IDs of tracks added to the library.
    #[serde(default)]
    pub imported: Vec<String>,
}

/// A cue, in the terms an agent or a person reads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CueView {
    /// Its ID in rekordbox's library, for a cue already there.
    pub id: Option<String>,
    /// `A` to `P` for a hot cue, `memory` for a memory cue.
    pub slot: String,
    pub start_secs: f64,
    /// Set for a loop.
    pub end_secs: Option<f64>,
    /// The bar it's on, from 1.
    pub bar: usize,
    pub name: String,
    /// `#rrggbb`, when it has one.
    pub color: Option<String>,
    /// Already in rekordbox, rather than placed by the rules.
    pub existing: bool,
}

impl CueView {
    fn of(cue: &Cue, track: &Track) -> Self {
        let bar = track.bars.iter().rposition(|b| b.start_secs <= cue.start_secs + 0.01).unwrap_or(0) + 1;
        Self {
            id: cue.id.clone(),
            slot: cue.hot.map_or_else(|| "memory".to_owned(), |h| char::from(b'A' + h).to_string()),
            start_secs: round3(cue.start_secs),
            end_secs: cue.end_secs.map(round3),
            bar,
            name: cue.name.clone(),
            color: cue.color.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")),
            existing: cue.existing,
        }
    }
}

fn round3(secs: f64) -> f64 {
    (secs * 1000.0).round() / 1000.0
}

/// One bar's levels, each scaled so 1.0 is the track's own loud end.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarView {
    pub start_secs: f64,
    pub volume: f32,
    pub bass: f32,
    pub busy: f32,
}

/// A track for an agent: what the collection says, its grid, its cues, and
/// optionally the bars the rules read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackSummary {
    pub row: TrackRow,
    pub analysis: Analysis,
    pub beats: usize,
    pub bars: usize,
    /// The first downbeat, in seconds.
    pub first_downbeat_secs: Option<f64>,
    pub cues: Vec<CueView>,
    /// Each bar's levels, when asked for.
    pub levels: Option<Vec<BarView>>,
}

/// The cues the rules would place on a track, beside the ones it has. Nothing
/// is written.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proposal {
    pub row: TrackRow,
    /// The bars, from 0, where the rules found the start, the drop and the groove.
    pub landmarks: Landmarks,
    /// Every cue the track would have, the existing ones included.
    pub cues: Vec<CueView>,
    /// Rules that placed nothing, and why.
    pub notes: Vec<String>,
    /// Rules that left a cue already there alone.
    pub kept: Vec<String>,
    /// New cues the rules aren't sure of, such as a drop earlier than any
    /// cued by hand, with why. Worth a listen before writing.
    pub flags: Vec<Flag>,
}

#[derive(Debug, Clone)]
enum Source {
    /// A rekordbox `master.db`: the installed one, or a copy or fixture.
    Db {
        location: LibraryLocation,
        stamp: Option<Stamp>,
    },
    Xml(PathBuf),
}

struct Open {
    library: RbLibrary,
    source: Source,
}

/// Who a write is for, which decides what kfbox learns from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A person, in kfbox: moving or deleting a placed cue is a fix to learn from.
    Hand,
    /// The cue rules: the cues they add are remembered as placed.
    Rules,
    /// An agent over MCP, or the in-app assistant: its cues count as placed too.
    Agent,
    Assistant,
}

impl Origin {
    /// The origin for a tool call's label, such as `Agent`.
    #[must_use]
    pub fn of_label(label: &str) -> Self {
        match label {
            "Agent" => Self::Agent,
            "Assistant" => Self::Assistant,
            _ => Self::Hand,
        }
    }

    fn source(self) -> Option<&'static str> {
        match self {
            Self::Hand => None,
            Self::Rules => Some("rules"),
            Self::Agent => Some("agent"),
            Self::Assistant => Some("assistant"),
        }
    }
}

/// How a change reached the library, for [`Session::reconcile`].
#[derive(Debug, Clone, Copy)]
enum Who {
    /// A person's write: placed cues that moved or went are fixes.
    Person(&'static str),
    /// A write that placed cues: those it moved follow it, and its new ones are remembered.
    Placer(&'static str),
    /// An undo, redo or restore: placed cues that went are forgotten, not learned from.
    Rewind,
}

/// The state every command shares.
pub struct Session {
    dir: PathBuf,
    cache: Mutex<Cache>,
    learning: Mutex<Learning>,
    open: Mutex<Option<Open>>,
    /// Tracks analysed for the current list, which an export writes.
    tracks: Mutex<Vec<Track>>,
    /// Stands in for "is rekordbox running?", for tests.
    running: Mutex<Option<RunningCheck>>,
    /// The library "rekordbox's library" means, in place of the installed
    /// one: a fixture, for the app's smoke test.
    library: Mutex<Option<LibraryLocation>>,
}

type RunningCheck = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A command that panicked mid-way leaves data that's still usable: lists
    // and a cache handle, no half-written invariants.
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Enough of a file's metadata to tell that it changed: a write through the
/// safe path swaps in a new file, and rekordbox writes in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    inode: u64,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&meta);
    #[cfg(not(unix))]
    let inode = 0;
    Some(Stamp { modified: meta.modified().ok(), len: meta.len(), inode })
}

/// What changed in the library since kfbox last read it, when something else
/// wrote to it: an agent through `kfbox-mcp`, or rekordbox itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryChange {
    pub listing: Listing,
    /// Tracks that are new or whose row differs, as the library now has them.
    pub changed: Vec<TrackRow>,
    /// The IDs of tracks no longer in the library.
    pub removed: Vec<String>,
}

fn slot(name: &str) -> CommandResult<Slot> {
    Slot::parse(name).ok_or_else(|| CommandError::Refused(format!("{name:?} is not a slot from A to P, or memory")))
}

fn millis(secs: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a time within a track")]
    let ms = (secs.max(0.0) * 1000.0).round() as u32;
    ms
}

/// The `ColorTableIndex` nearest a `#rrggbb`, for a hot cue.
fn hot_colour(color: Option<&str>) -> CommandResult<Option<u8>> {
    let Some(text) = color else { return Ok(None) };
    let hex = text.strip_prefix('#').unwrap_or(text);
    let value = u32::from_str_radix(hex, 16)
        .ok()
        .filter(|_| hex.len() == 6)
        .ok_or_else(|| CommandError::Refused(format!("{text:?} isn't a #rrggbb colour")))?;
    let [_, r, g, b] = value.to_be_bytes();
    Ok(hot_colour_index([r, g, b]))
}

/// Each track's cues and row, to tell afterwards which a write changed.
fn snapshot(library: &RbLibrary) -> HashMap<String, (Vec<Mark>, TrackRow)> {
    library.tracks.iter().map(|t| (t.id.clone(), (t.marks.clone(), TrackRow::of(t)))).collect()
}

/// The tracks that differ from `before`, and the IDs of those that are gone.
fn differences(before: &HashMap<String, (Vec<Mark>, TrackRow)>, library: &RbLibrary) -> (Vec<TrackRow>, Vec<String>) {
    let changed = library
        .tracks
        .iter()
        .filter(|t| before.get(&t.id).is_none_or(|(marks, row)| *marks != t.marks || *row != TrackRow::of(t)))
        .map(TrackRow::of)
        .collect();
    let removed = before.keys().filter(|id| library.track(id).is_none()).cloned().collect();
    (changed, removed)
}

impl Session {
    /// A session keeping its cache and rules in `dir`.
    pub fn open(dir: &Path) -> CommandResult<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            cache: Mutex::new(Cache::open(&dir.join("kfbox.db"))?),
            learning: Mutex::new(Learning::open(&dir.join("learning.db"))?),
            open: Mutex::new(None),
            tracks: Mutex::default(),
            running: Mutex::new(None),
            library: Mutex::new(None),
        })
    }

    /// For tests: stands in for "is rekordbox running?" on every write and
    /// for [`Session::rekordbox_running`], fixture libraries included.
    pub fn set_running_check(&self, running: impl Fn() -> bool + Send + Sync + 'static) {
        *lock(&self.running) = Some(std::sync::Arc::new(running));
    }

    /// Whether a write would now be refused because rekordbox is running.
    /// Only the real library is guarded; a copy or fixture never is, unless
    /// a test says so.
    #[must_use]
    pub fn rekordbox_running(&self) -> bool {
        if let Some(check) = lock(&self.running).clone() {
            return check();
        }
        matches!(self.source_now(), Ok(Source::Db { location, .. }) if location.is_real_install && rbl_db::is_rekordbox_running())
    }

    // ---- settings ------------------------------------------------------------

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    fn settings(&self) -> serde_json::Value {
        std::fs::read(self.settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| serde_json::json!({}))
    }

    /// Whether kfbox cues tracks with empty slots by itself. On unless turned off.
    #[must_use]
    pub fn auto_cue_enabled(&self) -> bool {
        self.settings().get("auto_cue").and_then(serde_json::Value::as_bool).unwrap_or(true)
    }

    pub fn set_auto_cue(&self, on: bool) -> CommandResult<()> {
        let mut settings = self.settings();
        settings["auto_cue"] = serde_json::Value::Bool(on);
        std::fs::write(self.settings_path(), serde_json::to_vec_pretty(&settings).map_err(|e| CommandError::Refused(e.to_string()))?)?;
        Ok(())
    }

    // ---- rules -------------------------------------------------------------

    fn rules_path(&self) -> PathBuf {
        self.dir.join("rules.toml")
    }

    /// The saved rules, or the defaults before any are saved.
    #[must_use]
    pub fn load_rules(&self) -> String {
        std::fs::read_to_string(self.rules_path()).unwrap_or_else(|_| DEFAULT_RULES.to_owned())
    }

    #[must_use]
    pub fn default_rules() -> String {
        DEFAULT_RULES.to_owned()
    }

    fn previous_rules_path(&self) -> PathBuf {
        self.dir.join("rules.previous.toml")
    }

    /// Whether rules have been saved, rather than the defaults being in use.
    #[must_use]
    pub fn has_saved_rules(&self) -> bool {
        self.rules_path().exists()
    }

    /// Saves rules, keeping the rules they replace so [`Session::restore_previous_rules`]
    /// can put them back.
    pub fn save_rules_keeping_previous(&self, rules: &str) -> CommandResult<()> {
        Rules::parse(rules)?;
        std::fs::write(self.previous_rules_path(), self.load_rules())?;
        std::fs::write(self.rules_path(), rules)?;
        Ok(())
    }

    /// Swaps the saved rules with the ones they replaced, and returns the rules
    /// now in use. Doing it twice puts things back.
    pub fn restore_previous_rules(&self) -> CommandResult<String> {
        let previous = std::fs::read_to_string(self.previous_rules_path())
            .map_err(|_| CommandError::Refused("there are no earlier rules to go back to".to_owned()))?;
        std::fs::write(self.previous_rules_path(), self.load_rules())?;
        std::fs::write(self.rules_path(), &previous)?;
        Ok(previous)
    }

    /// Checks the rules and saves them.
    pub fn save_rules(&self, rules: &str) -> CommandResult<()> {
        Rules::parse(rules)?;
        std::fs::write(self.rules_path(), rules)?;
        Ok(())
    }

    // ---- the library ---------------------------------------------------------

    fn keep_open(&self, library: RbLibrary, source: Source) -> Listing {
        let total = library.all_tracks().len();
        let playlists = library
            .playlists
            .iter()
            .enumerate()
            .map(|(index, p)| PlaylistItem { index, path: p.path.clone(), folder: p.folder, count: p.track_ids.len(), smart: p.smart })
            .collect();
        let my_tags = library.my_tags.clone();
        *lock(&self.open) = Some(Open { library, source });
        Listing { total, playlists, my_tags }
    }

    /// Makes [`Session::open_library`] read `location` instead of the
    /// installed library: a fixture, for the app's smoke test.
    pub fn use_library(&self, location: LibraryLocation) {
        *lock(&self.library) = Some(location);
    }

    /// Reads rekordbox's own library, from a copy of `master.db`.
    pub fn open_library(&self) -> CommandResult<Listing> {
        let location = match lock(&self.library).clone() {
            Some(location) => location,
            None => rbl_db::detect()?,
        };
        let listing = self.open_db(location)?;
        self.reconcile_outside();
        Ok(listing)
    }

    /// Reads a library at `location`, from a copy: the installed one, or a
    /// copy or fixture of one.
    pub fn open_db(&self, location: LibraryLocation) -> CommandResult<Listing> {
        let stamp = stamp(&location.master_db);
        let library = master::load(&location)?;
        Ok(self.keep_open(library, Source::Db { location, stamp }))
    }

    /// Reads a rekordbox XML.
    pub fn open_xml(&self, path: &Path) -> CommandResult<Listing> {
        let library = RbLibrary::parse(&std::fs::read_to_string(path)?)?;
        Ok(self.keep_open(library, Source::Xml(path.to_path_buf())))
    }

    /// Opens rekordbox's library if nothing is open, and reads it again if it
    /// has changed since. For a front end with no list on screen to keep.
    pub fn fresh_library(&self) -> CommandResult<Listing> {
        let source = lock(&self.open).as_ref().map(|o| o.source.clone());
        match source {
            None => self.open_library(),
            Some(Source::Db { location, stamp: when }) if when != stamp(&location.master_db) => self.open_db(location),
            Some(Source::Xml(path)) => self.open_xml(&path),
            Some(Source::Db { .. }) => self.listing(),
        }
    }

    /// Reads the library again if something else has written to it since kfbox
    /// last read it, and says what changed. `None` when nothing has, or when
    /// what's open is an XML, which only changes when it's opened again.
    pub fn refresh_if_changed(&self) -> CommandResult<Option<LibraryChange>> {
        let Some(Source::Db { location, stamp: was }) = lock(&self.open).as_ref().map(|o| o.source.clone()) else {
            return Ok(None);
        };
        // A kfbox writer mid-write: look again once it has finished and
        // brought the record of placed cues up to date.
        if stamp(&location.master_db) == was || SafeLibrary::busy(&self.dir.join("safe")) {
            return Ok(None);
        }
        let before = self.with_library(|library| Ok(snapshot(library)))?;
        let listing = self.open_db(location)?;
        let (changed, removed) = self.with_library(|library| Ok(differences(&before, library)))?;
        self.reconcile_outside();
        Ok(Some(LibraryChange { listing, changed, removed }))
    }

    /// The safe-write history's own marker, which moves with every commit, so
    /// a front end can tell its history view is out of date.
    #[must_use]
    pub fn history_marker(&self) -> Option<(SystemTime, u64)> {
        let meta = std::fs::metadata(self.dir.join("safe/history/.git/logs/HEAD")).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    /// What's open now, without reading anything again.
    pub fn fresh_listing(&self) -> CommandResult<Listing> {
        self.listing()
    }

    fn listing(&self) -> CommandResult<Listing> {
        let open = lock(&self.open);
        let library = &open.as_ref().ok_or(CommandError::NotOpen)?.library;
        Ok(Listing {
            total: library.all_tracks().len(),
            playlists: library
                .playlists
                .iter()
                .enumerate()
                .map(|(index, p)| PlaylistItem { index, path: p.path.clone(), folder: p.folder, count: p.track_ids.len(), smart: p.smart })
                .collect(),
            my_tags: library.my_tags.clone(),
        })
    }

    pub(crate) fn with_library<T>(&self, f: impl FnOnce(&RbLibrary) -> CommandResult<T>) -> CommandResult<T> {
        let open = lock(&self.open);
        f(&open.as_ref().ok_or(CommandError::NotOpen)?.library)
    }

    /// A playlist's rows in its order, or the whole collection for `None`.
    pub fn playlist_rows(&self, index: Option<usize>) -> CommandResult<Vec<TrackRow>> {
        self.with_library(|library| {
            if let Some(i) = index {
                if library.playlists.get(i).is_none() {
                    return Err(CommandError::NoPlaylist(i.to_string()));
                }
            }
            let tracks = index.map_or_else(|| library.all_tracks(), |i| library.playlist_tracks(i));
            Ok(tracks.into_iter().map(TrackRow::of).collect())
        })
    }

    /// Makes a playlist, or the whole collection, the list being worked on.
    /// Nothing is analysed yet.
    pub fn load_playlist(&self, index: Option<usize>) -> CommandResult<Vec<TrackRow>> {
        let rows = self.playlist_rows(index)?;
        lock(&self.tracks).clear();
        Ok(rows)
    }

    /// The playlist whose path or last part is `name`, ignoring case.
    pub fn find_playlist(&self, name: &str) -> CommandResult<usize> {
        self.with_library(|library| {
            let wanted = name.trim().to_lowercase();
            library
                .playlists
                .iter()
                .position(|p| !p.folder && (p.path.to_lowercase() == wanted || p.name.to_lowercase() == wanted))
                .ok_or_else(|| CommandError::NoPlaylist(name.to_owned()))
        })
    }

    /// Tracks of the whole collection matching every word of `query` in their
    /// title, artist, album, genre or key, at most `limit` of them.
    pub fn search(&self, query: &str, limit: usize) -> CommandResult<Vec<TrackRow>> {
        self.search_in(None, query, limit)
    }

    /// [`Session::search`] within one playlist, or the whole collection for `None`.
    pub fn search_in(&self, playlist: Option<usize>, query: &str, limit: usize) -> CommandResult<Vec<TrackRow>> {
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
        Ok(self
            .playlist_rows(playlist)?
            .into_iter()
            .filter(|row| {
                let text = row.haystack();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .take(limit)
            .collect())
    }

    /// kfbox's data folder: its cache, rules and history. The app's own, unless
    /// `KFBOX_DIR` names another.
    #[must_use]
    pub fn default_dir() -> Option<PathBuf> {
        std::env::var_os("KFBOX_DIR").map(PathBuf::from).or_else(|| Some(dirs::data_dir()?.join(APP_ID)))
    }

    // ---- analysis ----------------------------------------------------------

    pub(crate) fn keep(&self, track: &Track) {
        let mut tracks = lock(&self.tracks);
        tracks.retain(|t| t.path != track.path);
        tracks.push(track.clone());
    }

    /// Analyses a song file on kfbox's own grid, or reads it from the cache.
    pub fn analyse_file(&self, path: &Path) -> CommandResult<Analysis> {
        let (track, cached) = cache::analyse(&self.cache, path)?;
        self.keep(&track);
        Ok(Analysis::of(&track, cached))
    }

    pub(crate) fn source(&self, id: &str) -> CommandResult<RbTrack> {
        self.with_library(|library| library.track(id).cloned().ok_or_else(|| CommandError::NoTrack(id.to_owned())))
    }

    /// Analyses a track of the open library on its own grid, or reads it from
    /// the cache, and keeps it with its cues.
    pub fn analyse_track(&self, id: &str) -> CommandResult<Analysis> {
        let source = self.source(id)?;
        let (track, cached) = cache::from_rekordbox(&self.cache, &source)?;
        self.keep(&track);
        Ok(Analysis::of(&track, cached))
    }

    /// The whole of a kept track.
    pub fn track(&self, path: &str) -> CommandResult<Track> {
        let path = PathBuf::from(path);
        lock(&self.tracks).iter().find(|t| t.path == path).cloned().ok_or_else(|| CommandError::NotLoaded(path.display().to_string()))
    }

    pub(crate) fn track_by_id(&self, id: &str) -> CommandResult<(TrackRow, Track, bool)> {
        let source = self.source(id)?;
        let (track, cached) = cache::from_rekordbox(&self.cache, &source)?;
        Ok((TrackRow::of(&source), track, cached))
    }

    /// The closer waveform for a zoomed view: flat `[low, mid, high, peak]` columns.
    pub fn waveform(&self, path: &Path) -> CommandResult<Vec<u8>> {
        Ok(cache::waveform(&self.cache, path)?)
    }

    // ---- cues ----------------------------------------------------------------

    /// Places the cues on every kept track, keyed by path. Saves the rules
    /// only when asked.
    pub fn apply_rules(&self, rules: &str, save: bool) -> CommandResult<Vec<(String, Applied)>> {
        let parsed = Rules::parse(rules)?;
        if save {
            self.save_rules(rules)?;
        }
        Ok(lock(&self.tracks).iter().map(|t| (t.path.to_string_lossy().into_owned(), parsed.apply(t))).collect())
    }

    /// Places the cues on one kept track.
    pub fn apply_rules_to(&self, path: &str, rules: &str) -> CommandResult<Applied> {
        let parsed = Rules::parse(rules)?;
        let track = self.track(path)?;
        Ok(parsed.apply(&track))
    }

    /// A track of the open library for an agent, with its cues and grid, and
    /// its per-bar levels when `levels` is set.
    pub fn track_summary(&self, id: &str, levels: bool) -> CommandResult<TrackSummary> {
        let (row, track, cached) = self.track_by_id(id)?;
        let applied = Rules::parse("")?.apply(&track);
        Ok(TrackSummary {
            analysis: Analysis::of(&track, cached),
            beats: track.beats.len(),
            bars: track.bars.len(),
            first_downbeat_secs: track.bars.first().map(|b| round3(b.start_secs)),
            cues: applied.cues.iter().map(|c| CueView::of(c, &track)).collect(),
            levels: levels.then(|| {
                track
                    .bars
                    .iter()
                    .map(|b| BarView { start_secs: round3(b.start_secs), volume: b.volume, bass: b.bass, busy: b.busy })
                    .collect()
            }),
            row,
        })
    }

    /// The cues `rules` (or the saved rules) would place on a track. A dry run:
    /// nothing is written anywhere.
    pub fn propose_cues(&self, id: &str, rules: Option<&str>) -> CommandResult<Proposal> {
        let text = rules.map_or_else(|| self.load_rules(), str::to_owned);
        let parsed = Rules::parse(&text)?;
        let (row, track, _) = self.track_by_id(id)?;
        let applied = parsed.apply(&track);
        Ok(Proposal {
            row,
            landmarks: applied.landmarks,
            cues: applied.cues.iter().map(|c| CueView::of(c, &track)).collect(),
            notes: applied.notes,
            kept: applied.kept,
            flags: applied.flags,
        })
    }

    // ---- writes ----------------------------------------------------------------

    /// Where the open library is, for one read from `master.db`.
    pub(crate) fn library_location(&self) -> CommandResult<LibraryLocation> {
        match self.source_now()? {
            Source::Db { location, .. } => Ok(location),
            Source::Xml(_) => Err(CommandError::Refused("that needs rekordbox's library open, not an XML".to_owned())),
        }
    }

    fn source_now(&self) -> CommandResult<Source> {
        lock(&self.open).as_ref().map(|o| o.source.clone()).ok_or(CommandError::NotOpen)
    }

    /// The safe write path for the open library. It holds the library's lock,
    /// so it's opened for one write and dropped straight after: anything else
    /// that writes, such as an agent, only ever waits for a write, not for
    /// the app being open.
    fn safe(&self) -> CommandResult<SafeLibrary> {
        let root = self.dir.join("safe");
        let mut safe = match self.source_now()? {
            Source::Db { location, .. } if location.is_real_install => SafeLibrary::open_real(location, &root)?,
            Source::Db { location, .. } => SafeLibrary::open(location, &root)?,
            Source::Xml(_) => {
                return Err(CommandError::Refused("a rekordbox XML can't be saved to: open rekordbox's library, or export".to_owned()))
            }
        };
        if let Some(check) = lock(&self.running).clone() {
            safe.set_running_check(move || check());
        }
        Ok(safe)
    }

    /// Reads the library again after a write, and says which tracks' cues changed.
    fn after_write(&self, before: &HashMap<String, (Vec<Mark>, TrackRow)>) -> CommandResult<Vec<TrackRow>> {
        if let Source::Db { location, .. } = self.source_now()? {
            self.open_db(location)?;
        }
        self.with_library(|library| Ok(differences(before, library).0))
    }

    pub(crate) fn write(&self, summary: &str, ops: Vec<Op>, origin: Origin) -> CommandResult<Saved> {
        let before = self.with_library(|library| Ok(snapshot(library)))?;
        // The lock is held until the record of placed cues matches the
        // library, so another process never reads one without the other.
        let mut safe = self.safe()?;
        let receipt = safe.apply(&Change { summary: summary.to_owned(), ops, files: Vec::new() })?;
        let changed = self.after_write(&before)?;
        let who = origin.source().map_or(Who::Person("kfbox"), Who::Placer);
        self.reconcile(who, Some(&changed), &receipt.added_cues);
        drop(safe);
        Ok(Saved { commit: receipt.commit, added: receipt.added_cues, changed, imported: receipt.imported })
    }

    /// Brings the record of placed cues up to date with the library, on the
    /// tracks in `changed` or on every track, learning from a person's fixes.
    fn reconcile(&self, who: Who, changed: Option<&[TrackRow]>, added: &[String]) {
        let Ok((marks, wanted)) = self.with_library(|library| {
            let ids: Vec<String> = match changed {
                Some(rows) => rows.iter().map(|r| r.id.clone()).collect(),
                None => library.tracks.iter().map(|t| t.id.clone()).collect(),
            };
            let marks: HashMap<String, Vec<Mark>> =
                ids.iter().filter_map(|id| library.track(id).map(|t| (id.clone(), t.marks.clone()))).collect();
            Ok((marks, ids))
        }) else {
            return;
        };
        let placed = {
            let learning = lock(&self.learning);
            let ids: Vec<&str> = wanted.iter().map(String::as_str).collect();
            learning.placed_on(&ids).unwrap_or_default()
        };
        let tracked: Vec<&str> = placed.iter().map(|p| p.cue.as_str()).collect();
        for record in &placed {
            let empty = Vec::new();
            let on_track = marks.get(&record.track).unwrap_or(&empty);
            let by_id = on_track.iter().find(|m| m.id.as_deref() == Some(record.cue.as_str()));
            // A cue taken out and put back in the same slot, as replacing does.
            let by_slot = || {
                Slot::parse(&record.slot).and_then(Slot::hot).and_then(|hot| {
                    on_track.iter().find(|m| m.num == i32::from(hot) && m.id.as_deref().is_none_or(|id| !tracked.contains(&id)))
                })
            };
            let now = by_id.or_else(by_slot);
            let unchanged = now.is_some_and(|m| (m.start_secs - record.start_secs).abs() < 0.005);
            let learning = lock(&self.learning);
            match who {
                Who::Person(via) if !unchanged => {
                    let to_secs = now.map(|m| m.start_secs);
                    let bar = |secs: f64| self.bar_of(&record.track, secs);
                    let _ = learning.record(&Correction {
                        track: record.track.clone(),
                        slot: record.slot.clone(),
                        from_secs: record.start_secs,
                        to_secs,
                        from_bar: bar(record.start_secs),
                        to_bar: to_secs.and_then(bar),
                        source: record.source.clone(),
                        via: via.to_owned(),
                        unix_secs: 0,
                    });
                    let _ = learning.forget(&record.cue);
                }
                Who::Placer(source) if !unchanged => match now.and_then(|m| Some((m.id.clone()?, m.start_secs))) {
                    Some((cue, start_secs)) => {
                        let _ = learning.forget(&record.cue);
                        let _ = learning.remember(&Placed { cue, start_secs, source: source.to_owned(), ..record.clone() });
                    }
                    None => {
                        let _ = learning.forget(&record.cue);
                    }
                },
                Who::Rewind if now.is_none() => {
                    // Taking a placed cue back is a no to that slot, which
                    // auto-cue respects; it says nothing about where it goes.
                    let _ = learning.decline(&record.track, &record.slot);
                    let _ = learning.forget(&record.cue);
                }
                _ => {}
            }
        }
        if let Who::Placer(source) = who {
            let learning = lock(&self.learning);
            for (track, mark) in marks.iter().flat_map(|(t, ms)| ms.iter().map(move |m| (t, m))) {
                if let Some(cue) = mark.id.as_ref().filter(|id| added.contains(id)) {
                    let slot = u8::try_from(mark.num).map_or_else(|_| "memory".to_owned(), |n| char::from(b'A' + n).to_string());
                    let _ = learning.remember(&Placed {
                        cue: cue.clone(),
                        track: track.clone(),
                        slot,
                        start_secs: mark.start_secs,
                        source: source.to_owned(),
                    });
                }
            }
        }
    }

    /// Learns from changes made outside kfbox, such as a cue moved in
    /// rekordbox. Every kfbox write keeps the record in step under the
    /// library's lock, so whatever differs now was done elsewhere.
    fn reconcile_outside(&self) {
        if !SafeLibrary::busy(&self.dir.join("safe")) {
            self.reconcile(Who::Person("rekordbox"), None, &[]);
        }
    }

    /// The bar, from 1, a time falls in on a track's cached analysis.
    fn bar_of(&self, id: &str, secs: f64) -> Option<usize> {
        let (_, track, _) = self.track_by_id(id).ok()?;
        Some(track.bars.iter().rposition(|b| b.start_secs <= secs + 0.01).unwrap_or(0) + 1)
    }

    /// The slots on each track that auto-cue leaves empty, as (track, slot).
    pub(crate) fn learning_refused(&self) -> CommandResult<std::collections::HashSet<(String, String)>> {
        Ok(lock(&self.learning).refused_slots()?)
    }

    /// Every fix a person made to a cue the rules or an agent placed, oldest first.
    pub fn corrections(&self) -> CommandResult<Vec<Correction>> {
        Ok(lock(&self.learning).corrections()?)
    }

    fn ops_for(&self, edits: &[CueEdit]) -> CommandResult<Vec<Op>> {
        let mut ops = Vec::new();
        for edit in edits {
            match edit {
                CueEdit::Add { track, slot: name, start_secs, end_secs, name: cue_name, color, replace } => {
                    let slot = slot(name)?;
                    let source = self.source(track)?;
                    if *replace {
                        if let Slot::Hot(hot) = slot {
                            for mark in source.marks.iter().filter(|m| m.num == i32::from(hot)) {
                                let cue = mark
                                    .id
                                    .clone()
                                    .ok_or_else(|| CommandError::Refused("that cue has no ID to replace it by".to_owned()))?;
                                ops.push(Op::DeleteCue { cue });
                            }
                        }
                    }
                    // A memory cue's colour is rekordbox's eight-value one, which
                    // nothing here sets yet.
                    let colour = if slot == Slot::Memory { None } else { hot_colour(color.as_deref())? };
                    let start_ms = millis(*start_secs);
                    ops.push(match end_secs {
                        Some(end) => Op::AddLoop {
                            track: track.clone(),
                            slot,
                            start_ms,
                            end_ms: millis(*end),
                            beats: 0,
                            colour,
                            name: cue_name.clone(),
                        },
                        None => Op::AddCue { track: track.clone(), slot, start_ms, colour, name: cue_name.clone() },
                    });
                }
                CueEdit::Move { cue, start_secs } => ops.push(Op::MoveCue { cue: cue.clone(), start_ms: millis(*start_secs) }),
                CueEdit::Rename { cue, name } => ops.push(Op::SetCueName { cue: cue.clone(), name: name.clone() }),
                CueEdit::Recolour { cue, color } => ops.push(Op::SetCueColour { cue: cue.clone(), colour: hot_colour(color.as_deref())? }),
                CueEdit::Delete { cue } => ops.push(Op::DeleteCue { cue: cue.clone() }),
            }
        }
        Ok(ops)
    }

    /// Makes these edits to the library, all together or not at all, as a
    /// person's: a placed cue they move or delete is a fix kfbox learns from.
    pub fn save_cues(&self, summary: &str, edits: &[CueEdit]) -> CommandResult<Saved> {
        self.save_cues_as(summary, edits, Origin::Hand)
    }

    /// [`Session::save_cues`] for whoever `origin` says.
    pub fn save_cues_as(&self, summary: &str, edits: &[CueEdit], origin: Origin) -> CommandResult<Saved> {
        if edits.is_empty() {
            return Err(CommandError::Refused("there's nothing to save".to_owned()));
        }
        let ops = self.ops_for(edits)?;
        self.write(summary, ops, origin)
    }

    /// Writes the cues `rules` (or the saved rules) place on these tracks, in
    /// one change. Cues already there stay as they are: a rule only fills an
    /// empty slot.
    pub fn write_rule_cues(&self, ids: &[String], rules: Option<&str>) -> CommandResult<Saved> {
        self.write_rule_cues_as(ids, rules, "")
    }

    /// [`Session::write_rule_cues`], with `label` and a colon before the
    /// history's message when it isn't empty, to say who made the change.
    pub fn write_rule_cues_as(&self, ids: &[String], rules: Option<&str>, label: &str) -> CommandResult<Saved> {
        let text = rules.map_or_else(|| self.load_rules(), str::to_owned);
        let parsed = Rules::parse(&text)?;
        let mut edits = Vec::new();
        for id in ids {
            let (_, track, _) = self.track_by_id(id)?;
            for cue in parsed.apply(&track).cues.into_iter().filter(|c| !c.existing) {
                edits.push(CueEdit::Add {
                    track: id.clone(),
                    slot: cue.hot.map_or_else(|| "memory".to_owned(), |h| char::from(b'A' + h).to_string()),
                    start_secs: cue.start_secs,
                    end_secs: cue.end_secs,
                    name: cue.name,
                    color: cue.color.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")),
                    replace: false,
                });
            }
        }
        if edits.is_empty() {
            return Ok(Saved::default());
        }
        let what = if ids.len() == 1 { "Apply rules to 1 track".to_owned() } else { format!("Apply rules to {} tracks", ids.len()) };
        let summary = if label.is_empty() { what } else { format!("{label}: {what}") };
        self.save_cues_as(&summary, &edits, Origin::Rules)
    }

    /// Undoes the latest change, if nothing has changed since it.
    pub fn undo(&self) -> CommandResult<Saved> {
        let before = self.with_library(|library| Ok(snapshot(library)))?;
        let mut safe = self.safe()?;
        let receipt = safe.undo()?;
        let changed = self.after_write(&before)?;
        self.reconcile(Who::Rewind, Some(&changed), &[]);
        drop(safe);
        Ok(Saved { commit: receipt.commit, changed, ..Saved::default() })
    }

    /// Undoes an undo, when that's the latest change.
    pub fn redo(&self) -> CommandResult<Saved> {
        let before = self.with_library(|library| Ok(snapshot(library)))?;
        let mut safe = self.safe()?;
        let receipt = safe.redo()?;
        let changed = self.after_write(&before)?;
        self.reconcile(Who::Rewind, Some(&changed), &[]);
        drop(safe);
        Ok(Saved { commit: receipt.commit, changed, ..Saved::default() })
    }

    /// The latest changes, newest first.
    pub fn history(&self, limit: usize) -> CommandResult<Vec<HistoryEntry>> {
        Ok(self.safe()?.history(limit)?)
    }

    /// Puts the library back as it was at `commit`. A restore is a change too,
    /// so it can be undone.
    pub fn restore(&self, commit: &str) -> CommandResult<Saved> {
        let before = self.with_library(|library| Ok(snapshot(library)))?;
        let mut safe = self.safe()?;
        let receipt = safe.restore(commit)?;
        let changed = self.after_write(&before)?;
        self.reconcile(Who::Rewind, Some(&changed), &[]);
        drop(safe);
        Ok(Saved { commit: receipt.commit, changed, ..Saved::default() })
    }

    // ---- export ----------------------------------------------------------------

    fn write_export(dest: &Path, tracks: &[Track], rules: &Rules) -> CommandResult<usize> {
        let applied: Vec<Applied> = tracks.iter().map(|t| rules.apply(t)).collect();
        let pairs: Vec<(&Track, &[Cue])> = tracks.iter().zip(&applied).map(|(t, a)| (t, a.cues.as_slice())).collect();
        std::fs::write(dest, write_xml(&pairs))?;
        Ok(pairs.len())
    }

    /// Writes every kept track with its cues to `dest`. Returns how many.
    pub fn export_xml(&self, dest: &Path, rules: &str) -> CommandResult<usize> {
        let parsed = Rules::parse(rules)?;
        let tracks = lock(&self.tracks).clone();
        Self::write_export(dest, &tracks, &parsed)
    }

    /// Writes these tracks of the open library, with the saved rules' cues, to
    /// a new rekordbox XML at `dest`. Refuses anything but a new `.xml` file
    /// outside rekordbox's own folders, unless `overwrite` is set.
    pub fn export_tracks(&self, dest: &Path, ids: &[String], overwrite: bool) -> CommandResult<usize> {
        let refuse = |why: &str| Err(CommandError::Refused(format!("{}: {why}", dest.display())));
        if !dest.is_absolute() {
            return refuse("give an absolute path");
        }
        if dest.extension().and_then(|e| e.to_str()).is_none_or(|e| !e.eq_ignore_ascii_case("xml")) {
            return refuse("the file must end in .xml");
        }
        if dest.components().any(|c| c.as_os_str().eq_ignore_ascii_case("Pioneer")) {
            return refuse("that's inside rekordbox's own folders");
        }
        if dest.exists() && !overwrite {
            return refuse("the file exists; set overwrite to replace it");
        }
        if !dest.parent().is_some_and(Path::is_dir) {
            return refuse("its folder doesn't exist");
        }
        let rules = Rules::parse(&self.load_rules())?;
        let tracks = ids.iter().map(|id| self.track_by_id(id).map(|(_, track, _)| track)).collect::<CommandResult<Vec<Track>>>()?;
        Self::write_export(dest, &tracks, &rules)
    }
}
