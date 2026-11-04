//! Editing the library beyond cues: ratings, My Tags, playlists and a
//! track's beat grid. Every edit goes through the safe path as one change in
//! the history, so it undoes like any other.
//!
//! My Tags are only ever the ones the library has: kfbox doesn't invent
//! columns or values, and refuses a name it doesn't know, listing the ones
//! it does. The grid is a person's edit only: no rule, agent or assistant
//! tool writes one.

use serde::{Deserialize, Serialize};

use crate::rekordbox::TagCategory;
use crate::safe::Op;
use crate::session::{CommandError, CommandResult, Origin, Saved, Session};

/// How a list of My Tags combines with a track's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagMode {
    /// Adds these to the track's tags.
    #[default]
    Add,
    /// Takes these off.
    Remove,
    /// Makes these the track's tags, and only these.
    Set,
}

/// A person's change to a track's beat grid. The result is a grid of one
/// tempo, counted in bars from the downbeat it keeps or is given.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GridEdit {
    /// Moves every beat by this many milliseconds, later when positive.
    Shift { ms: f64 },
    /// A new tempo, keeping the first downbeat where it is.
    Bpm { bpm: f64 },
    /// Puts a downbeat here, keeping the tempo.
    DownbeatAt { secs: f64 },
}

/// A grid of one tempo through `anchor`, a downbeat, from the start of the
/// track to its end, as an analysis file holds it.
fn constant_grid(anchor_secs: f64, bpm: f64, duration_secs: f64) -> Vec<rbl_anlz::Beat> {
    let period = 60.0 / bpm;
    #[allow(clippy::cast_possible_truncation, reason = "a beat count within a track")]
    let before = (anchor_secs / period).floor() as i64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a tempo of a few hundred BPM")]
    let tempo_x100 = (bpm * 100.0).round() as u16;
    let mut beats = Vec::new();
    let mut k = -before;
    loop {
        #[allow(clippy::cast_precision_loss, reason = "a beat count within a track")]
        let secs = anchor_secs + k as f64 * period;
        if secs > duration_secs {
            break;
        }
        if secs >= 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a time within a track")]
            let time_ms = (secs * 1000.0).round() as u32;
            let beat_number = u16::try_from(k.rem_euclid(4) + 1).unwrap_or(1);
            beats.push(rbl_anlz::Beat { beat_number, tempo_x100, time_ms });
        }
        k += 1;
    }
    beats
}

fn stars(n: u8) -> String {
    if n == 0 {
        "no stars".to_owned()
    } else {
        "★".repeat(usize::from(n))
    }
}

fn tracks_word(n: usize) -> String {
    if n == 1 {
        "1 track".to_owned()
    } else {
        format!("{n} tracks")
    }
}

impl Session {
    /// The library's My Tag columns and the tags in each.
    pub fn my_tags(&self) -> CommandResult<Vec<TagCategory>> {
        self.with_library(|library| Ok(library.my_tags.clone()))
    }

    /// Rates tracks, 0 to 5 stars.
    pub fn set_rating(&self, ids: &[String], stars_given: u8) -> CommandResult<Saved> {
        if stars_given > 5 {
            return Err(CommandError::Refused(format!("{stars_given} stars: a rating is 0 to 5")));
        }
        let ops: Vec<Op> = ids.iter().map(|id| Op::SetRating { track: id.clone(), stars: stars_given }).collect();
        self.write(&format!("Rate {} {}", tracks_word(ids.len()), stars(stars_given)), ops, Origin::Hand)
    }

    /// Adds, takes off or sets My Tags on tracks. `tags` are `Category: Tag`
    /// as [`Session::my_tags`] lists them (or their IDs), and only those: a
    /// name the library doesn't have is refused, with the ones it does.
    pub fn set_my_tags(&self, ids: &[String], tags: &[String], mode: TagMode, origin: Origin) -> CommandResult<Saved> {
        let (wanted, current) = self.with_library(|library| {
            let mut wanted = Vec::new();
            for name in tags {
                let id = library.tag_id(name).ok_or_else(|| {
                    let known: Vec<String> =
                        library.my_tags.iter().flat_map(|c| c.tags.iter().map(move |t| format!("{}: {}", c.name, t.name))).collect();
                    CommandError::Refused(format!("there's no My Tag {name:?}; the library has {}", known.join(", ")))
                })?;
                wanted.push(id);
            }
            let mut current = Vec::new();
            for id in ids {
                let track = library.track(id).ok_or_else(|| CommandError::NoTrack(id.clone()))?;
                let on: Vec<String> =
                    track.attribute("MyTags").split(crate::rekordbox::TAG_SEPARATOR).filter_map(|n| library.tag_id(n)).collect();
                current.push(on);
            }
            Ok((wanted, current))
        })?;
        let mut ops = Vec::new();
        for (id, on) in ids.iter().zip(current) {
            let next: Vec<String> = match mode {
                TagMode::Set => wanted.clone(),
                TagMode::Add => on.iter().chain(wanted.iter().filter(|t| !on.contains(t))).cloned().collect(),
                TagMode::Remove => on.iter().filter(|t| !wanted.contains(t)).cloned().collect(),
            };
            if next != on {
                ops.push(Op::SetMyTags { track: id.clone(), tags: next });
            }
        }
        if ops.is_empty() {
            return Ok(Saved::default());
        }
        let what = format!("Tag {}", tracks_word(ops.len()));
        let summary = match origin {
            Origin::Agent => format!("Agent: {what}"),
            Origin::Assistant => format!("Assistant: {what}"),
            Origin::Hand | Origin::Rules => what,
        };
        self.write(&summary, ops, origin)
    }

    /// The `djmdPlaylist.ID` of the playlist at `index`, if it can be edited.
    fn editable_playlist(&self, index: usize) -> CommandResult<(String, String)> {
        self.with_library(|library| {
            let playlist = library.playlists.get(index).ok_or_else(|| CommandError::NoPlaylist(index.to_string()))?;
            let refuse = |why: &str| Err(CommandError::Refused(format!("{}: {why}", playlist.path)));
            if playlist.smart {
                return refuse("an intelligent playlist's rule picks its tracks");
            }
            match &playlist.id {
                Some(id) => Ok((id.clone(), playlist.name.clone())),
                None => refuse("a rekordbox XML's playlists can't be changed; open rekordbox's library"),
            }
        })
    }

    /// A new playlist named `name` with these tracks, inside the folder at
    /// `folder`, or at the top.
    pub fn create_playlist(&self, name: &str, folder: Option<usize>, ids: &[String]) -> CommandResult<Saved> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CommandError::Refused("a playlist needs a name".to_owned()));
        }
        let parent = match folder {
            Some(index) => Some(self.with_library(|library| match library.playlists.get(index) {
                Some(p) if p.folder => p.id.clone().ok_or_else(|| CommandError::Refused("an XML's folders can't be changed".to_owned())),
                _ => Err(CommandError::Refused("that isn't a folder".to_owned())),
            })?),
            None => None,
        };
        let op = Op::CreatePlaylist { name: name.to_owned(), parent, tracks: ids.to_vec() };
        self.write(&format!("New playlist {name}"), vec![op], Origin::Hand)
    }

    pub fn add_to_playlist(&self, index: usize, ids: &[String]) -> CommandResult<Saved> {
        let (playlist, name) = self.editable_playlist(index)?;
        let op = Op::AddToPlaylist { playlist, tracks: ids.to_vec() };
        self.write(&format!("Add {} to {name}", tracks_word(ids.len())), vec![op], Origin::Hand)
    }

    pub fn remove_from_playlist(&self, index: usize, ids: &[String]) -> CommandResult<Saved> {
        let (playlist, name) = self.editable_playlist(index)?;
        let op = Op::RemoveFromPlaylist { playlist, tracks: ids.to_vec() };
        self.write(&format!("Remove {} from {name}", tracks_word(ids.len())), vec![op], Origin::Hand)
    }

    pub fn rename_playlist(&self, index: usize, name: &str) -> CommandResult<Saved> {
        let (playlist, old) = self.editable_playlist(index)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(CommandError::Refused("a playlist needs a name".to_owned()));
        }
        self.write(&format!("Rename {old} to {name}"), vec![Op::RenamePlaylist { playlist, name: name.to_owned() }], Origin::Hand)
    }

    pub fn delete_playlist(&self, index: usize) -> CommandResult<Saved> {
        let (playlist, name) = self.editable_playlist(index)?;
        self.write(&format!("Delete playlist {name}"), vec![Op::DeletePlaylist { playlist }], Origin::Hand)
    }

    /// A person's edit to a track's beat grid, written to its analysis file
    /// with its BPM. Cues stay where they are in time.
    pub fn edit_grid(&self, id: &str, edit: GridEdit) -> CommandResult<Saved> {
        let (row, track, _) = self.track_by_id(id)?;
        if track.grid != crate::track::GridSource::Rekordbox {
            return Err(CommandError::Refused(format!("{} has no grid in the library yet to edit", row.title)));
        }
        let anchor = track.bars.first().map_or(0.0, |b| b.start_secs);
        let (anchor, bpm, what) = match edit {
            GridEdit::Shift { ms } => (anchor + ms / 1000.0, track.bpm, format!("shift {ms:+.0} ms")),
            GridEdit::Bpm { bpm } => (anchor, bpm, format!("{bpm:.2} BPM")),
            GridEdit::DownbeatAt { secs } => (secs, track.bpm, format!("downbeat at {secs:.3} s")),
        };
        if !(40.0..=300.0).contains(&bpm) {
            return Err(CommandError::Refused(format!("{bpm:.2} BPM is outside 40 to 300")));
        }
        let beats = constant_grid(anchor, bpm, track.duration_secs);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a tempo of a few hundred BPM")]
        let bpm_x100 = (bpm * 100.0).round() as u32;
        self.write(&format!("Grid of {}: {what}", row.title), vec![Op::SetGrid { track: id.to_owned(), beats, bpm_x100 }], Origin::Hand)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn a_constant_grid_counts_bars_from_its_downbeat_both_ways() {
        let beats = constant_grid(1.0, 120.0, 3.0);
        let at: Vec<(u16, u32)> = beats.iter().map(|b| (b.beat_number, b.time_ms)).collect();
        // Two beats before the downbeat are the bar before's third and fourth.
        assert_eq!(at, vec![(3, 0), (4, 500), (1, 1000), (2, 1500), (3, 2000), (4, 2500), (1, 3000)]);
        assert!(beats.iter().all(|b| b.tempo_x100 == 12000));
    }
}
