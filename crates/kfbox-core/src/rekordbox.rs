//! Reading the XML rekordbox writes with `File › Export Collection in xml
//! format`: the collection's tracks with their grids and cues, and the
//! playlist tree.
//!
//! The fields follow rbxport's `rbl-db/src/xml.rs`, copied rather than
//! depended on because that importer is tied to rbxport's database writer.
//! Every element's attributes are kept as read, so a track written back out
//! says exactly what rekordbox said about it, plus kfbox's new cues.

use std::path::PathBuf;

use rbl_core::xml::{attribute, tags, Tag};

#[derive(Debug, thiserror::Error)]
pub enum RekordboxError {
    #[error("not a rekordbox XML file: there is no DJ_PLAYLISTS element")]
    NotRekordbox,
}

/// Attributes in document order, unescaped.
pub type Attributes = Vec<(String, String)>;

/// One `TEMPO`: from `inizio` on, a beat every 60/`bpm` seconds, and the
/// beat at `inizio` is beat `battito` of the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Tempo {
    pub inizio: f64,
    pub bpm: f64,
    pub battito: u8,
    pub attributes: Attributes,
}

/// One `POSITION_MARK`.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// The cue's `djmdCue.ID`, for one read from rekordbox's own library.
    /// An XML has none, and its cues can't be edited in place.
    pub id: Option<String>,
    /// -1 a memory cue, 0 and up a hot cue A, B, C …
    pub num: i32,
    pub start_secs: f64,
    /// Set for a loop.
    pub end_secs: Option<f64>,
    pub name: String,
    pub color: Option<[u8; 3]>,
    pub attributes: Attributes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RbTrack {
    pub id: String,
    /// The file, decoded from `Location`; `None` when it is not a file.
    pub path: Option<PathBuf>,
    pub attributes: Attributes,
    pub tempos: Vec<Tempo>,
    pub marks: Vec<Mark>,
    /// The analysis file holding the beat grid, for a track read from
    /// rekordbox's own library, whose grid is not in the database.
    pub anlz: Option<PathBuf>,
}

impl RbTrack {
    /// Whether the file is in rekordbox's own `rekordbox/Sampler` folder.
    fn is_sampler_loop(&self) -> bool {
        self.path.as_deref().is_some_and(|path| {
            let parts: Vec<_> = path.components().map(std::path::Component::as_os_str).collect();
            parts.windows(2).any(|pair| pair[0] == "rekordbox" && pair[1] == "Sampler")
        })
    }

    /// An attribute of the `TRACK`, empty when absent.
    #[must_use]
    pub fn attribute(&self, name: &str) -> String {
        attribute(&self.attributes, name)
    }
}

/// A folder or playlist below `ROOT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub name: String,
    /// The folders above it and its name, joined by ` › `.
    pub path: String,
    pub folder: bool,
    /// The `TrackID`s it lists, in order.
    pub track_ids: Vec<String>,
    /// Its `djmdPlaylist.ID`, for one read from rekordbox's own library.
    pub id: Option<String>,
    /// An intelligent playlist, whose tracks its rule picks.
    pub smart: bool,
}

/// Between a track's My Tags in its `MyTags` attribute: a character no tag name has.
pub const TAG_SEPARATOR: &str = "\u{1f}";

/// One My Tag column, as rekordbox calls a category, and the tags in it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TagCategory {
    pub name: String,
    pub tags: Vec<MyTag>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MyTag {
    /// Its `djmdMyTag.ID`.
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RbLibrary {
    pub tracks: Vec<RbTrack>,
    /// The tree in document order, folders included, `ROOT` left out.
    pub playlists: Vec<Playlist>,
    /// The library's My Tag columns, in its order. An XML has none.
    pub my_tags: Vec<TagCategory>,
}

impl RbLibrary {
    /// A My Tag by its ID, as `Category: Tag`.
    #[must_use]
    pub fn tag_name(&self, id: &str) -> Option<String> {
        self.my_tags.iter().find_map(|c| c.tags.iter().find(|t| t.id == id).map(|t| format!("{}: {}", c.name, t.name)))
    }

    /// A My Tag's ID by `Category: Tag`, any case, or by ID.
    #[must_use]
    pub fn tag_id(&self, name: &str) -> Option<String> {
        let wanted = name.trim().to_lowercase();
        self.my_tags.iter().find_map(|c| {
            c.tags.iter().find(|t| t.id == name.trim() || format!("{}: {}", c.name, t.name).to_lowercase() == wanted).map(|t| t.id.clone())
        })
    }

    pub fn parse(text: &str) -> Result<Self, RekordboxError> {
        let mut library = Self::default();
        let mut seen_root = false;
        let mut in_collection = false;
        let mut in_playlists = false;
        let mut track: Option<RbTrack> = None;
        // The names of the open nodes, ROOT first, and which playlist (if
        // any) each is.
        let mut open: Vec<(String, Option<usize>)> = Vec::new();
        for tag in tags(text) {
            match tag {
                Tag::Open { name, attributes, closed } => match name.as_str() {
                    "DJ_PLAYLISTS" => seen_root = true,
                    "COLLECTION" => in_collection = !closed,
                    "PLAYLISTS" => in_playlists = !closed,
                    "TRACK" if in_collection => {
                        let parsed = RbTrack {
                            id: attribute(&attributes, "TrackID"),
                            path: file_path(&attribute(&attributes, "Location")),
                            attributes,
                            tempos: Vec::new(),
                            marks: Vec::new(),
                            anlz: None,
                        };
                        if closed {
                            library.tracks.push(parsed);
                        } else {
                            track = Some(parsed);
                        }
                    }
                    "TEMPO" => {
                        if let Some(current) = track.as_mut() {
                            current.tempos.push(Tempo {
                                inizio: number(&attributes, "Inizio").unwrap_or(0.0),
                                bpm: number(&attributes, "Bpm").unwrap_or(0.0),
                                battito: attribute(&attributes, "Battito").trim().parse().unwrap_or(1),
                                attributes,
                            });
                        }
                    }
                    "POSITION_MARK" => {
                        if let Some(current) = track.as_mut() {
                            current.marks.push(mark(attributes));
                        }
                    }
                    "NODE" if in_playlists => {
                        let node_name = attribute(&attributes, "Name");
                        let index = if open.is_empty() {
                            None
                        } else {
                            let mut path: Vec<&str> = open.iter().skip(1).map(|(n, _)| n.as_str()).collect();
                            path.push(&node_name);
                            library.playlists.push(Playlist {
                                path: path.join(" › "),
                                name: node_name.clone(),
                                folder: attribute(&attributes, "Type").trim() != "1",
                                track_ids: Vec::new(),
                                id: None,
                                smart: false,
                            });
                            Some(library.playlists.len() - 1)
                        };
                        if !closed {
                            open.push((node_name, index));
                        }
                    }
                    "TRACK" if in_playlists => {
                        if let Some(playlist) = open.last().and_then(|(_, i)| *i).and_then(|i| library.playlists.get_mut(i)) {
                            playlist.track_ids.push(attribute(&attributes, "Key"));
                        }
                    }
                    _ => {}
                },
                Tag::Close { name } => match name.as_str() {
                    "COLLECTION" => in_collection = false,
                    "PLAYLISTS" => in_playlists = false,
                    "TRACK" if in_collection => {
                        if let Some(done) = track.take() {
                            library.tracks.push(done);
                        }
                    }
                    "NODE" if in_playlists => {
                        open.pop();
                    }
                    _ => {}
                },
            }
        }
        if seen_root {
            Ok(library)
        } else {
            Err(RekordboxError::NotRekordbox)
        }
    }

    #[must_use]
    pub fn track(&self, id: &str) -> Option<&RbTrack> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// The whole collection, by artist and then title, ignoring case. The
    /// loops rekordbox installs for its sampler are in the collection too,
    /// but they are not songs to cue.
    #[must_use]
    pub fn all_tracks(&self) -> Vec<&RbTrack> {
        let mut tracks: Vec<&RbTrack> = self.tracks.iter().filter(|t| !t.is_sampler_loop()).collect();
        tracks.sort_by_cached_key(|t| (t.attribute("Artist").to_lowercase(), t.attribute("Name").to_lowercase()));
        tracks
    }

    /// A playlist's tracks in its order, skipping IDs the collection lacks.
    pub fn playlist_tracks(&self, playlist: usize) -> Vec<&RbTrack> {
        self.playlists.get(playlist).map_or_else(Vec::new, |p| p.track_ids.iter().filter_map(|id| self.track(id)).collect())
    }
}

fn number(attributes: &Attributes, name: &str) -> Option<f64> {
    attribute(attributes, name).trim().parse().ok()
}

fn mark(attributes: Attributes) -> Mark {
    let channel = |name: &str| attribute(&attributes, name).trim().parse::<u8>().ok();
    let color = match (channel("Red"), channel("Green"), channel("Blue")) {
        (Some(r), Some(g), Some(b)) => Some([r, g, b]),
        _ => None,
    };
    Mark {
        id: None,
        num: attribute(&attributes, "Num").trim().parse().unwrap_or(-1),
        start_secs: number(&attributes, "Start").unwrap_or(0.0),
        end_secs: number(&attributes, "End"),
        name: attribute(&attributes, "Name"),
        color,
        attributes,
    }
}

/// The file a `file://` URL names, percent-decoded; `None` for another
/// scheme or nothing. `file://localhost/x` and `file:///x` both mean `/x`.
#[must_use]
pub fn file_path(location: &str) -> Option<PathBuf> {
    let rest = location.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(path);
    if decoded.is_empty() {
        return None;
    }
    // A Windows path arrives as `/C:/Users/…`.
    let decoded = if decoded.len() > 2 && decoded.as_bytes()[0] == b'/' && decoded.as_bytes()[2] == b':' {
        decoded.get(1..).unwrap_or("").to_owned()
    } else {
        decoded
    };
    Some(PathBuf::from(decoded))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'%' && i + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(text.get(i + 1..i + 3).unwrap_or(""), 16) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(byte);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="7.2.8" Company="AlphaTheta"/>
  <COLLECTION Entries="2">
    <TRACK TrackID="11" Name="All U Need" Artist="A &amp; B" Tonality="Fm" AverageBpm="128.00"
           Location="file://localhost/Users/me/Music/All%20U%20Need.mp3">
      <TEMPO Inizio="0.025" Bpm="128.00" Metro="4/4" Battito="1"/>
      <TEMPO Inizio="60.025" Bpm="130.00" Metro="4/4" Battito="3"/>
      <POSITION_MARK Name="" Type="0" Start="12.5" Num="-1"/>
      <POSITION_MARK Name="in" Type="0" Start="0.025" Num="0" Red="40" Green="226" Blue="20"/>
      <POSITION_MARK Name="" Type="4" Start="60" End="63.75" Num="2"/>
    </TRACK>
    <TRACK TrackID="12" Name="Bare" Location="file://localhost/Users/me/Music/Bare.wav"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="2">
      <NODE Name="Gigs" Type="0" Count="1">
        <NODE Name="Friday" Type="1" KeyType="0" Entries="2">
          <TRACK Key="12"/>
          <TRACK Key="11"/>
          <TRACK Key="99"/>
        </NODE>
      </NODE>
      <NODE Name="Empty" Type="1" KeyType="0" Entries="0"/>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>
"#;

    #[test]
    fn a_document_parses_into_tracks_grids_cues_and_a_tree() {
        let library = RbLibrary::parse(DOC).unwrap();
        assert_eq!(library.tracks.len(), 2);
        let track = library.track("11").unwrap();
        assert_eq!(track.attribute("Artist"), "A & B");
        assert_eq!(track.path, Some(PathBuf::from("/Users/me/Music/All U Need.mp3")));
        assert_eq!(track.tempos.len(), 2);
        assert_eq!((track.tempos[1].inizio, track.tempos[1].bpm, track.tempos[1].battito), (60.025, 130.0, 3));
        assert_eq!(track.marks.iter().map(|m| m.num).collect::<Vec<_>>(), vec![-1, 0, 2]);
        assert_eq!(track.marks[1].color, Some([40, 226, 20]));
        assert_eq!(track.marks[2].end_secs, Some(63.75));
        assert_eq!(library.track("12").unwrap().tempos, Vec::new());

        let paths: Vec<(&str, bool)> = library.playlists.iter().map(|p| (p.path.as_str(), p.folder)).collect();
        assert_eq!(paths, vec![("Gigs", true), ("Gigs › Friday", false), ("Empty", false)]);
        let friday: Vec<&str> = library.playlist_tracks(1).iter().map(|t| t.id.as_str()).collect();
        assert_eq!(friday, vec!["12", "11"], "in playlist order, the missing 99 skipped");
    }

    #[test]
    fn all_tracks_is_the_whole_collection_in_name_order() {
        let library = RbLibrary::parse(DOC).unwrap();
        let ids: Vec<&str> = library.all_tracks().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["12", "11"], "no artist sorts before A & B");

        let sampler = DOC.replace("/Users/me/Music/Bare.wav", "/Users/me/Music/rekordbox/Sampler/OSC_SAMPLER/Bare.wav");
        let library = RbLibrary::parse(&sampler).unwrap();
        let ids: Vec<&str> = library.all_tracks().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["11"], "rekordbox's sampler loops are not songs");
    }

    #[test]
    fn other_documents_are_refused() {
        assert!(RbLibrary::parse("<plist><dict/></plist>").is_err());
    }

    #[test]
    fn locations_decode_as_paths() {
        assert_eq!(file_path("file:///Music/a%20b.mp3"), Some(PathBuf::from("/Music/a b.mp3")));
        assert_eq!(file_path("file://localhost/M%C3%BCsic/%E2%99%AA.mp3"), Some(PathBuf::from("/Müsic/♪.mp3")));
        assert_eq!(file_path("http://x/y"), None);
    }
}
