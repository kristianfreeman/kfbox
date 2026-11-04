//! Reading rekordbox's own library: the encrypted `master.db` and the beat
//! grids in its analysis files.
//!
//! kfbox never touches the database rekordbox uses. It copies `master.db`
//! and its journal files to a temporary folder, opens the copy read-only
//! with rbxport's `rbl-db`, loads it with `rbl-index`, and lets the copy go.
//! The result is the same [`RbLibrary`] a rekordbox XML gives, so the rest
//! of kfbox does not care where a track came from.
//!
//! A track's grid is not read here: there is one analysis file per track,
//! and reading all of them for a playlist of twenty would be waste. Each
//! [`RbTrack`] carries its analysis file instead, and [`grid_from_anlz`]
//! reads it when the track is loaded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rbl_db::{LibraryLocation, OpenMode};
use rbl_index::{Cue as DbCue, Library, Row, TrackSource};

use crate::rekordbox::{Attributes, Mark, MyTag, Playlist, RbLibrary, RbTrack, TagCategory, Tempo};

#[derive(Debug, thiserror::Error)]
pub enum MasterError {
    #[error("rekordbox's library: {0}")]
    Db(#[from] rbl_db::DbError),
    #[error("reading rekordbox's library: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("copying rekordbox's library: {0}")]
    Io(#[from] std::io::Error),
}

/// Where rekordbox's library is on this machine, from its agent's settings.
pub fn locate() -> Result<PathBuf, MasterError> {
    Ok(rbl_db::detect()?.master_db)
}

/// Loads the library rekordbox is set up with, from a copy of it.
pub fn load_installed() -> Result<RbLibrary, MasterError> {
    load(&rbl_db::detect()?)
}

/// Runs `f` on a read-only copy of the library at `location`, journal and
/// all, which is thrown away after. kfbox never opens the file rekordbox uses.
pub fn with_copy<T>(location: &LibraryLocation, f: impl FnOnce(&rbl_db::Library) -> Result<T, MasterError>) -> Result<T, MasterError> {
    let scratch = tempfile::tempdir()?;
    let copy = scratch.path().join("master.db");
    std::fs::copy(&location.master_db, &copy)?;
    // An uncommitted write lives in these; without them the copy can be a
    // step behind what rekordbox shows, or unreadable.
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = location.master_db.clone().into_os_string();
        name.push(suffix);
        let journal = PathBuf::from(name);
        if journal.exists() {
            std::fs::copy(&journal, scratch.path().join(format!("master.db{suffix}")))?;
        }
    }
    // A copy: nothing here is the user's library.
    let copied = LibraryLocation { master_db: copy, is_real_install: false, ..location.clone() };
    let db = rbl_db::Library::open(copied, OpenMode::ReadOnly)?;
    f(&db)
}

/// Loads the library at `location`, from a copy of it.
pub fn load(location: &LibraryLocation) -> Result<RbLibrary, MasterError> {
    with_copy(location, |db| {
        let (library, _) = rbl_index::load(db)?;
        let comments = cue_comments(db.connection())?;
        let mut converted = convert(&library, &location.share_root, &comments);
        converted.my_tags = my_tag_categories(db.connection())?;
        let tagged = song_tags(db.connection())?;
        let names: HashMap<&str, String> =
            converted.my_tags.iter().flat_map(|c| c.tags.iter().map(move |t| (t.id.as_str(), format!("{}: {}", c.name, t.name)))).collect();
        for track in &mut converted.tracks {
            if let Some(ids) = tagged.get(&track.id) {
                let shown: Vec<&str> = ids.iter().filter_map(|id| names.get(id.as_str()).map(String::as_str)).collect();
                track.attributes.push(("MyTags".into(), shown.join(crate::rekordbox::TAG_SEPARATOR)));
            }
        }
        Ok(converted)
    })
}

/// The My Tag columns and the tags in each, in rekordbox's order.
fn my_tag_categories(conn: &rusqlite::Connection) -> Result<Vec<TagCategory>, MasterError> {
    let rows = rbl_db::export_info::my_tags(conn)?;
    Ok(rows
        .iter()
        .filter(|r| r.attribute == 1)
        .map(|category| TagCategory {
            name: category.name.clone(),
            tags: rows
                .iter()
                .filter(|r| r.attribute == 0 && r.parent == category.id)
                .map(|t| MyTag { id: t.id.clone(), name: t.name.clone() })
                .collect(),
        })
        .collect())
}

/// Each track's My Tag IDs.
fn song_tags(conn: &rusqlite::Connection) -> Result<HashMap<String, Vec<String>>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT s.ContentID, s.MyTagID FROM djmdSongMyTag s
         JOIN djmdMyTag t ON t.ID = s.MyTagID AND t.rb_local_deleted = 0
         WHERE s.rb_local_deleted = 0 ORDER BY s.ContentID, s.TrackNo, s.MyTagID",
    )?;
    let mut tags: HashMap<String, Vec<String>> = HashMap::new();
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (track, tag) = row?;
        tags.entry(track).or_default().push(tag);
    }
    Ok(tags)
}

/// `djmdCue.Comment` by track, slot kind and position: the one cue field
/// `rbl-index` leaves out, and the cue's name in rekordbox.
fn cue_comments(conn: &rusqlite::Connection) -> Result<HashMap<(String, u8, u32), String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT ContentID, Kind, InMsec, Comment FROM djmdCue
         WHERE rb_local_deleted = 0 AND Comment IS NOT NULL AND Comment <> ''",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((
            (r.get::<_, String>(0)?, u8::try_from(r.get::<_, i64>(1)?).unwrap_or(0), u32::try_from(r.get::<_, i64>(2)?).unwrap_or(0)),
            r.get::<_, String>(3)?,
        ))
    })?;
    rows.collect()
}

fn convert(library: &Library, share_root: &Path, comments: &HashMap<(String, u8, u32), String>) -> RbLibrary {
    let count = u32::try_from(library.len()).unwrap_or(u32::MAX);
    let tracks = (0..count).map(|row| track(library, row, share_root, comments)).collect();
    RbLibrary { tracks, playlists: playlists(library), my_tags: Vec::new() }
}

fn track(library: &Library, row: Row, share_root: &Path, comments: &HashMap<(String, u8, u32), String>) -> RbTrack {
    let i = row as usize;
    let id = library.ids.get(i).copied().unwrap_or(0).to_string();
    let path = library.folder_path.get(i).to_owned();
    let mut attributes: Attributes = vec![
        ("TrackID".into(), id.clone()),
        ("Name".into(), library.title.get(i).to_owned()),
        ("Artist".into(), library.artist_name(row).to_owned()),
        ("Album".into(), library.album_name(row).to_owned()),
        ("Genre".into(), library.genre_name(row).to_owned()),
        ("Kind".into(), kind(library.file_type.get(i).copied().unwrap_or(0)).to_owned()),
        ("TotalTime".into(), library.length_sec.get(i).copied().unwrap_or(0).to_string()),
        ("DateAdded".into(), library.date_added.get(i).to_owned()),
        ("AverageBpm".into(), format!("{:.2}", f64::from(library.bpm_x100.get(i).copied().unwrap_or(0)) / 100.0)),
        ("Tonality".into(), library.key_name(row).to_owned()),
        ("Comments".into(), library.comment.get(i).to_owned()),
        ("Rating".into(), library.rating.get(i).copied().unwrap_or(0).to_string()),
        ("Location".into(), crate::xml::location(Path::new(&path))),
    ];
    attributes.retain(|(_, value)| !value.is_empty());
    let analysis = library.analysis_path.get(i);
    let marks = library
        .cues_of(row)
        .iter()
        .map(|cue| {
            let name = comments.get(&(id.clone(), cue.kind, cue.position_ms)).cloned().unwrap_or_default();
            mark(cue, name)
        })
        .collect();
    RbTrack {
        id,
        path: (!path.is_empty()).then(|| PathBuf::from(path)),
        attributes,
        tempos: Vec::new(),
        marks,
        anlz: (!analysis.is_empty()).then(|| rbl_anlz::resolve(share_root, analysis)),
    }
}

/// A database cue as the `POSITION_MARK` rekordbox's own XML export writes.
fn mark(cue: &DbCue, name: String) -> Mark {
    let num = cue.hot_letter().map_or(-1, |letter| i32::from(letter as u8) - i32::from(b'A'));
    let start_secs = f64::from(cue.position_ms) / 1000.0;
    let end_secs = (cue.out_ms > cue.position_ms).then(|| f64::from(cue.out_ms) / 1000.0);
    // Index 0 is no colour; a memory cue's colour is kept elsewhere and not
    // carried over.
    let color = (cue.hot_letter().is_some() && cue.colour != 0).then(|| rbl_anlz::cue_colour(cue.colour)).flatten();
    let mut attributes: Attributes = vec![
        ("Name".into(), name.clone()),
        ("Type".into(), if end_secs.is_some() { "4" } else { "0" }.into()),
        ("Start".into(), format!("{start_secs:.3}")),
    ];
    if let Some(end) = end_secs {
        attributes.push(("End".into(), format!("{end:.3}")));
    }
    attributes.push(("Num".into(), num.to_string()));
    if let Some([r, g, b]) = color {
        attributes.extend([("Red".into(), r.to_string()), ("Green".into(), g.to_string()), ("Blue".into(), b.to_string())]);
    }
    Mark { id: (cue.id != 0).then(|| cue.id.to_string()), num, start_secs, end_secs, name, color, attributes }
}

/// rekordbox's `Kind` for its `FileType` code.
fn kind(file_type: u8) -> &'static str {
    match file_type {
        1 => "MP3 File",
        4 => "M4A File",
        5 => "FLAC File",
        11 => "WAV File",
        12 => "AIFF File",
        _ => "",
    }
}

/// The playlist tree, depth-first, with each intelligent playlist as the
/// tracks its rule admits now.
fn playlists(library: &Library) -> Vec<Playlist> {
    let tree = library.playlists();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); tree.len()];
    let mut roots = Vec::new();
    for index in 0..tree.len() {
        match tree.parent.get(index).copied() {
            Some(parent) if parent != rbl_index::NO_ID && (parent as usize) < tree.len() => {
                if let Some(bucket) = children.get_mut(parent as usize) {
                    bucket.push(index);
                }
            }
            _ => roots.push(index),
        }
    }
    let mut out = Vec::new();
    // Iterative with a visited set: a corrupt parent cycle must not loop.
    let mut visited = vec![false; tree.len()];
    let mut stack: Vec<(usize, String)> = roots.iter().rev().map(|&i| (i, String::new())).collect();
    while let Some((index, above)) = stack.pop() {
        if visited.get(index).copied().unwrap_or(true) {
            continue;
        }
        if let Some(seen) = visited.get_mut(index) {
            *seen = true;
        }
        let name = tree.name(index).to_owned();
        let path = if above.is_empty() { name.clone() } else { format!("{above} › {name}") };
        if tree.is_folder(index) {
            let id = Some(tree.ids.get(index).copied().unwrap_or(0).to_string());
            out.push(Playlist { name, path: path.clone(), folder: true, track_ids: Vec::new(), id, smart: false });
            for &child in children.get(index).map_or(&[][..], Vec::as_slice).iter().rev() {
                stack.push((child, path.clone()));
            }
        } else {
            let source = if tree.is_smart(index) { TrackSource::SmartPlaylist(index) } else { TrackSource::Playlist(index) };
            let track_ids = library
                .source_rows_unlocked(&tree, &source)
                .into_iter()
                .map(|row| library.ids.get(row as usize).copied().unwrap_or(0).to_string())
                .collect();
            let id = Some(tree.ids.get(index).copied().unwrap_or(0).to_string());
            out.push(Playlist { name, path, folder: false, track_ids, id, smart: tree.is_smart(index) });
        }
    }
    out
}

/// A track's beat grid from its analysis file, as the `TEMPO` entries
/// rekordbox's XML export writes for it: one where the grid starts and one
/// at each tempo change, with the beat's place in the bar. Empty when the
/// file is missing or has no grid.
#[must_use]
pub fn grid_from_anlz(dat: &Path) -> Vec<Tempo> {
    let Ok(parsed) = rbl_anlz::Anlz::read(dat) else { return Vec::new() };
    let beats = parsed.beat_grid().unwrap_or_default();
    // rekordbox can store a correction for the whole grid apart from the
    // beats; rbxport's grid editor applies it the same way.
    let offset = i64::from(parsed.grid_offset().unwrap_or(0));
    let mut tempos: Vec<Tempo> = Vec::new();
    for beat in beats {
        #[allow(clippy::cast_possible_truncation, reason = "a BPM times 100 fits easily")]
        if tempos.last().is_some_and(|t| (t.bpm * 100.0).round() as i64 == i64::from(beat.tempo_x100)) {
            continue;
        }
        #[allow(clippy::cast_precision_loss, reason = "milliseconds of a track")]
        let inizio = (i64::from(beat.time_ms) + offset) as f64 / 1000.0;
        let bpm = f64::from(beat.tempo_x100) / 100.0;
        let battito = u8::try_from(beat.beat_number).unwrap_or(1);
        tempos.push(Tempo {
            inizio,
            bpm,
            battito,
            attributes: vec![
                ("Inizio".into(), format!("{inizio:.3}")),
                ("Bpm".into(), format!("{bpm:.2}")),
                ("Metro".into(), "4/4".into()),
                ("Battito".into(), battito.to_string()),
            ],
        });
    }
    tempos
}
