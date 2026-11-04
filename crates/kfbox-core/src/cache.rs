//! `kfbox.db`: analysis results, so a track is decoded once.
//!
//! A result is keyed by what it depends on: the file's size and modified time,
//! a hash of its start and end, the grid, and the analysis version. Moving a
//! file keeps its result; changing its audio or its grid doesn't.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::rekordbox::RbTrack;
use crate::track::{with_grid, Track, TrackError};

/// Bump when analysis or [`Track`]'s shape changes: old results then miss.
const ANALYSIS_VERSION: u32 = 1;

/// How fine the zoomed waveform is.
pub const WAVEFORM_COLUMNS_PER_SEC: f64 = 40.0;

/// How much of each end of a file goes into its key.
const SAMPLE_BYTES: u64 = 1 << 20;

pub struct Cache {
    connection: Connection,
}

impl Cache {
    /// Opens the cache, starting a new one if the file can't be read: it only
    /// holds what analysis can make again.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Ok(cache) = Self::open_file(path) {
            Ok(cache)
        } else {
            for suffix in ["", "-wal", "-shm"] {
                let mut name = path.as_os_str().to_owned();
                name.push(suffix);
                let _ = std::fs::remove_file(name);
            }
            Self::open_file(path)
        }
    }

    fn open_file(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS analysis (key TEXT PRIMARY KEY, track BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS waveform (key TEXT PRIMARY KEY, columns BLOB NOT NULL);",
        )?;
        Ok(Self { connection })
    }

    fn get(&self, key: &str) -> Option<Track> {
        let bytes: Vec<u8> =
            self.connection.query_row("SELECT track FROM analysis WHERE key = ?1", [key], |r| r.get(0)).optional().ok()??;
        serde_json::from_slice(&bytes).ok()
    }

    fn put(&self, key: &str, track: &Track) {
        // A failure here costs a decode next time, and nothing else.
        if let Ok(bytes) = serde_json::to_vec(track) {
            let _ = self.connection.execute("INSERT OR REPLACE INTO analysis (key, track) VALUES (?1, ?2)", params![key, bytes]);
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        let count: i64 = self.connection.query_row("SELECT COUNT(*) FROM analysis", [], |r| r.get(0)).unwrap_or(0);
        usize::try_from(count).unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn key(path: &Path, grid: &[u8]) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let mut hash = Sha256::new();
    hash.update(ANALYSIS_VERSION.to_le_bytes());
    hash.update(meta.len().to_le_bytes());
    let modified = meta.modified()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    hash.update(modified.to_le_bytes());
    let mut sample = Vec::new();
    (&mut file).take(SAMPLE_BYTES).read_to_end(&mut sample)?;
    hash.update(&sample);
    if meta.len() > SAMPLE_BYTES {
        file.seek(SeekFrom::End(-i64::try_from(SAMPLE_BYTES).unwrap_or(i64::MAX)))?;
        sample.clear();
        file.read_to_end(&mut sample)?;
        hash.update(&sample);
    }
    hash.update(grid);
    Ok(crate::hex(&hash.finalize()))
}

fn grid_bytes(source: &RbTrack) -> Vec<u8> {
    if source.tempos.is_empty() {
        return source.anlz.as_ref().and_then(|dat| std::fs::read(dat).ok()).unwrap_or_default();
    }
    source.tempos.iter().flat_map(|t| format!("{} {} {};", t.inizio, t.bpm, t.battito).into_bytes()).collect()
}

fn get(cache: &Mutex<Cache>, key: &str) -> Option<Track> {
    cache.lock().ok()?.get(key)
}

fn put(cache: &Mutex<Cache>, key: &str, track: &Track) {
    if let Ok(cache) = cache.lock() {
        cache.put(key, track);
    }
}

/// The closer waveform a zoomed view draws: [`WAVEFORM_COLUMNS_PER_SEC`]
/// columns of `[low, mid, high, peak]`, flat. Made on first asking.
pub fn waveform(cache: &Mutex<Cache>, path: &Path) -> Result<Vec<u8>, TrackError> {
    let key = key(path, b"waveform").map_err(TrackError::Io)?;
    if let Ok(held) = cache.lock() {
        let found: Option<Vec<u8>> =
            held.connection.query_row("SELECT columns FROM waveform WHERE key = ?1", [&key], |r| r.get(0)).optional().ok().flatten();
        if let Some(columns) = found {
            return Ok(columns);
        }
    }
    let columns = crate::track::waveform_columns(path, WAVEFORM_COLUMNS_PER_SEC)?;
    if let Ok(held) = cache.lock() {
        let _ = held.connection.execute("INSERT OR REPLACE INTO waveform (key, columns) VALUES (?1, ?2)", params![key, columns]);
    }
    Ok(columns)
}

/// A song on kfbox's grid. The flag says whether it came from the cache.
pub fn analyse(cache: &Mutex<Cache>, path: &Path) -> Result<(Track, bool), TrackError> {
    let key = key(path, b"kfbox").map_err(TrackError::Io)?;
    if let Some(mut track) = get(cache, &key) {
        track.path = path.to_path_buf();
        return Ok((track, true));
    }
    let track = Track::analyse(path)?;
    put(cache, &key, &track);
    Ok((track, false))
}

/// A track of a rekordbox collection. Tags and cues always come from the
/// collection: only the analysis is cached.
pub fn from_rekordbox(cache: &Mutex<Cache>, source: &RbTrack) -> Result<(Track, bool), TrackError> {
    let path = source.path.as_deref().ok_or(TrackError::NoFile)?;
    let key = key(path, &grid_bytes(source)).map_err(TrackError::Io)?;
    if let Some(mut track) = get(cache, &key) {
        track.path = path.to_path_buf();
        track.attach(with_grid(source));
        return Ok((track, true));
    }
    let track = Track::from_rekordbox(source)?;
    let mut stored = track.clone();
    stored.rekordbox = None;
    put(cache, &key, &stored);
    Ok((track, false))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(clippy::cast_precision_loss, reason = "a column count")]
mod tests {
    use super::*;

    fn wav(path: &Path, seed: u8) {
        crate::testing::wav(path, seed).unwrap();
    }

    fn cache(dir: &Path) -> Mutex<Cache> {
        Mutex::new(Cache::open(&dir.join("kfbox.db")).unwrap())
    }

    #[test]
    fn a_second_load_comes_from_the_cache_even_across_opens() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("a.wav");
        wav(&song, 1);
        let (first, cached) = analyse(&cache(dir.path()), &song).unwrap();
        assert!(!cached);
        let (second, cached) = analyse(&cache(dir.path()), &song).unwrap();
        assert!(cached, "a new Cache on the same file still hits");
        assert_eq!(first.bars.len(), second.bars.len());
        assert_eq!(first.overview, second.overview);
    }

    #[test]
    fn changed_audio_misses_and_a_moved_file_hits() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let song = dir.path().join("a.wav");
        wav(&song, 1);
        analyse(&cache, &song).unwrap();

        let moved = dir.path().join("moved.wav");
        std::fs::rename(&song, &moved).unwrap();
        let (track, cached) = analyse(&cache, &moved).unwrap();
        assert!(cached);
        assert_eq!(track.path, moved);

        wav(&moved, 9);
        assert!(!analyse(&cache, &moved).unwrap().1);
    }

    #[test]
    fn a_changed_grid_misses_and_cues_come_from_the_collection() {
        use crate::rekordbox::{Mark, Tempo};
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let song = dir.path().join("a.wav");
        wav(&song, 1);
        let mark = Mark { id: None, num: 0, start_secs: 1.0, end_secs: None, name: "mine".into(), color: None, attributes: Vec::new() };
        let tempo = |bpm: f64| Tempo { inizio: 0.0, bpm, battito: 1, attributes: Vec::new() };
        let mut source = RbTrack {
            id: "1".into(),
            path: Some(song),
            attributes: vec![("Name".into(), "Old".into())],
            tempos: vec![tempo(120.0)],
            marks: Vec::new(),
            anlz: None,
        };
        assert!(!from_rekordbox(&cache, &source).unwrap().1);

        // New cues and a new title don't need a new analysis, and do show.
        source.marks.push(mark);
        source.attributes = vec![("Name".into(), "New".into())];
        let (track, cached) = from_rekordbox(&cache, &source).unwrap();
        assert!(cached);
        assert_eq!(track.tags.title, "New");
        assert_eq!(track.rekordbox.unwrap().marks.len(), 1);

        source.tempos = vec![tempo(121.0)];
        assert!(!from_rekordbox(&cache, &source).unwrap().1);
    }

    #[test]
    fn the_zoomed_waveform_is_made_once_at_its_fineness() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path());
        let song = dir.path().join("a.wav");
        wav(&song, 1);
        let first = waveform(&cache, &song).unwrap();
        assert_eq!(first.len() % 4, 0);
        let seconds = first.len() as f64 / 4.0 / WAVEFORM_COLUMNS_PER_SEC;
        assert!((seconds - 30.0).abs() < 0.5, "{seconds}");
        // It isn't decoded again: the cached copy survives the bytes being
        // unreadable as audio only if the key still matches, so check the row.
        let rows: i64 = cache.lock().unwrap().connection.query_row("SELECT COUNT(*) FROM waveform", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1);
        assert_eq!(waveform(&cache, &song).unwrap(), first);
    }

    #[test]
    fn a_broken_cache_file_starts_again() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("kfbox.db"), b"not a database at all, not even close").unwrap();
        let cache = Cache::open(&dir.path().join("kfbox.db")).unwrap();
        assert!(cache.is_empty());
    }
}
