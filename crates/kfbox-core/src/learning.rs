//! What kfbox learns from Kristian's fixes: which cues the rules or an agent
//! placed, and where he moved them or that he deleted them. Kept in
//! `learning.db` beside `kfbox.db`, not in it: the cache is thrown away and
//! rebuilt when it can't be read, and these can't be made again.
//!
//! Scoring a rules file counts the fixes as extra evidence beside the
//! hand-cued tracks, and the record is there for retuning the drop finder.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A cue the rules or an agent put in the library, until a person changes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    /// Its `djmdCue.ID`.
    pub cue: String,
    pub track: String,
    /// `A` to `P`, or `memory`.
    pub slot: String,
    pub start_secs: f64,
    /// Who placed it: `rules`, `agent` or `assistant`.
    pub source: String,
}

/// A person moving or deleting a cue the rules or an agent placed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub track: String,
    pub slot: String,
    pub from_secs: f64,
    /// Where it went; `None` for a deleted cue.
    pub to_secs: Option<f64>,
    /// The bars, from 1, when the track's analysis was at hand.
    pub from_bar: Option<usize>,
    pub to_bar: Option<usize>,
    /// Who had placed it: `rules`, `agent` or `assistant`.
    pub source: String,
    /// Where the fix was made: `kfbox`, or `rekordbox` for one found in the
    /// library afterwards.
    pub via: String,
    pub unix_secs: i64,
}

pub struct Learning {
    connection: Connection,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

fn bar_column(bar: Option<usize>) -> Option<i64> {
    bar.and_then(|b| i64::try_from(b).ok())
}

impl Learning {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 2000;
             CREATE TABLE IF NOT EXISTS placed (
                 cue TEXT PRIMARY KEY, track TEXT NOT NULL, slot TEXT NOT NULL,
                 start_secs REAL NOT NULL, source TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS placed_track ON placed (track);
             CREATE TABLE IF NOT EXISTS corrections (
                 id INTEGER PRIMARY KEY, track TEXT NOT NULL, slot TEXT NOT NULL,
                 from_secs REAL NOT NULL, to_secs REAL, from_bar INTEGER, to_bar INTEGER,
                 source TEXT NOT NULL, via TEXT NOT NULL, unix_secs INTEGER NOT NULL);",
        )?;
        Ok(Self { connection })
    }

    pub fn remember(&self, placed: &Placed) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO placed (cue, track, slot, start_secs, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![placed.cue, placed.track, placed.slot, placed.start_secs, placed.source],
        )?;
        Ok(())
    }

    pub fn forget(&self, cue: &str) -> rusqlite::Result<()> {
        self.connection.execute("DELETE FROM placed WHERE cue = ?1", [cue])?;
        Ok(())
    }

    /// The placed cues on these tracks that no person has changed yet.
    pub fn placed_on(&self, tracks: &[&str]) -> rusqlite::Result<Vec<Placed>> {
        let mut statement = self.connection.prepare("SELECT cue, track, slot, start_secs, source FROM placed WHERE track = ?1")?;
        let mut found = Vec::new();
        for track in tracks {
            let rows = statement.query_map([track], |r| {
                Ok(Placed { cue: r.get(0)?, track: r.get(1)?, slot: r.get(2)?, start_secs: r.get(3)?, source: r.get(4)? })
            })?;
            for row in rows {
                found.push(row?);
            }
        }
        Ok(found)
    }

    pub fn record(&self, correction: &Correction) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO corrections (track, slot, from_secs, to_secs, from_bar, to_bar, source, via, unix_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                correction.track,
                correction.slot,
                correction.from_secs,
                correction.to_secs,
                bar_column(correction.from_bar),
                bar_column(correction.to_bar),
                correction.source,
                correction.via,
                if correction.unix_secs == 0 { now() } else { correction.unix_secs },
            ],
        )?;
        Ok(())
    }

    /// Every correction, oldest first.
    pub fn corrections(&self) -> rusqlite::Result<Vec<Correction>> {
        let mut statement = self
            .connection
            .prepare("SELECT track, slot, from_secs, to_secs, from_bar, to_bar, source, via, unix_secs FROM corrections ORDER BY id")?;
        let bar = |value: Option<i64>| value.and_then(|b| usize::try_from(b).ok());
        let rows = statement.query_map([], |r| {
            Ok(Correction {
                track: r.get(0)?,
                slot: r.get(1)?,
                from_secs: r.get(2)?,
                to_secs: r.get(3)?,
                from_bar: bar(r.get(4)?),
                to_bar: bar(r.get(5)?),
                source: r.get(6)?,
                via: r.get(7)?,
                unix_secs: r.get(8)?,
            })
        })?;
        rows.collect()
    }
}
