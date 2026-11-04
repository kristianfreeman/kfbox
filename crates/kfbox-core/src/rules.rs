//! The rules file: where each cue goes, written once and applied to every
//! track.
//!
//! A rule names a landmark — the start, the drop, or the groove near the
//! end — and a slot, and can move the cue by whole bars or make it a loop.
//! The landmarks are found from the per-bar levels in [`Track::bars`]; the
//! `[find]` table tunes how. `rules.default.toml` is the reference.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::rekordbox::{Attributes, Mark};
use crate::safe::Slot;
use crate::track::Track;

/// The rules every new install starts from.
pub const DEFAULT_RULES: &str = include_str!("../rules.default.toml");

/// Bars either side of a candidate drop that are compared.
const DROP_WINDOW_BARS: usize = 4;

/// Bars either side of a candidate groove that are compared.
const GROOVE_WINDOW_BARS: usize = 8;

/// The shortest bass-out that marks the last section.
const GROOVE_MIN_BREAK_BARS: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("{0}")]
    Parse(#[from] toml::de::Error),
    #[error("cue {index}: slot {slot:?} is not A to P or \"memory\"")]
    BadSlot { index: usize, slot: String },
    #[error("cue {index}: slot {slot} is used twice")]
    DuplicateSlot { index: usize, slot: String },
    #[error("cue {index}: colour {colour:?} is not a name or #rrggbb")]
    BadColour { index: usize, colour: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default)]
    pub find: Find,
    #[serde(default, rename = "cue")]
    pub cues: Vec<CueRule>,
}

/// How the landmarks are found.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Find {
    /// Drops and grooves are looked for on this many-bar phrase grid,
    /// counted from the start. 0 allows any bar.
    pub phrase_bars: usize,
    /// The start is the first bar at least this loud (0..=1).
    pub start_level: f32,
    /// Which drop: the end of the longest `break`, the `biggest` rise in
    /// bass, or the `first` one that clears `drop_rise`.
    pub drop: DropPick,
    /// How much the bass must rise (0..=1) over four bars to be a drop.
    /// With `drop = "break"`, used only for a track with no break.
    pub drop_rise: f32,
    /// A break is a stretch with the bass below this (0..=1).
    pub break_bass: f32,
    /// The break before the drop ends within this fraction of the track.
    pub break_before: f32,
    /// The groove is looked for after this fraction of the track.
    pub groove_after: f32,
    /// How much the mids and highs must fall (0..=1) for the groove to
    /// start there rather than after the last break.
    pub groove_step: f32,
    /// On a track with neither, every bar of the groove has at least this
    /// much bass: the kick runs right through it.
    pub groove_bass: f32,
    /// The groove starts at least this many bars before the end, leaving
    /// room to mix out from it.
    pub groove_end_bars: usize,
    /// A drop found before this bar (from 0) is flagged for a look. None of
    /// Kristian's hand-cued drops is earlier than bar 64. 0 flags nothing.
    pub doubt_drop_before: usize,
}

impl Default for Find {
    fn default() -> Self {
        Self {
            phrase_bars: 8,
            start_level: 0.05,
            drop: DropPick::Break,
            drop_rise: 0.25,
            break_bass: 0.7,
            break_before: 0.55,
            groove_after: 0.6,
            groove_step: 0.15,
            groove_bass: 0.5,
            groove_end_bars: 20,
            doubt_drop_before: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DropPick {
    Break,
    Biggest,
    First,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Landmark {
    Start,
    Drop,
    Groove,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CueRule {
    /// `A` to `P` for a hot cue, `memory` for a memory cue.
    pub slot: String,
    pub at: Landmark,
    /// Whole bars to move the cue by: -8 is eight bars earlier.
    #[serde(default)]
    pub offset_bars: i32,
    /// Makes the cue a loop this many bars long.
    pub loop_bars: Option<u32>,
    /// A name (`green`, `red`, …) or `#rrggbb`.
    pub color: Option<String>,
    /// The cue's comment in rekordbox.
    pub name: Option<String>,
}

/// A cue on one track: one the rules placed, or one already in rekordbox.
#[derive(Debug, Clone, Serialize)]
pub struct Cue {
    /// The cue's ID in rekordbox's library, for an existing cue read from it.
    pub id: Option<String>,
    /// Hot cue slot from 0 (A); `None` for a memory cue.
    pub hot: Option<u8>,
    pub start_secs: f64,
    /// Set for a loop.
    pub end_secs: Option<f64>,
    pub color: Option<[u8; 3]>,
    pub name: String,
    /// Already in the rekordbox XML, and so never moved or replaced.
    pub existing: bool,
    /// An existing cue's `POSITION_MARK` as read, written back unchanged.
    #[serde(skip)]
    pub attributes: Option<Attributes>,
}

impl Cue {
    fn from_mark(mark: &Mark) -> Self {
        Self {
            id: mark.id.clone(),
            hot: u8::try_from(mark.num).ok(),
            start_secs: mark.start_secs,
            end_secs: mark.end_secs,
            color: mark.color,
            name: mark.name.clone(),
            existing: true,
            attributes: Some(mark.attributes.clone()),
        }
    }
}

/// The bars each landmark was found at, for drawing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landmarks {
    pub start: Option<usize>,
    pub drop: Option<usize>,
    pub groove: Option<usize>,
}

/// What applying the rules to one track gave.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub landmarks: Landmarks,
    pub cues: Vec<Cue>,
    /// Rules that placed nothing, and why.
    pub notes: Vec<String>,
    /// Rules that left a cue already there alone.
    pub kept: Vec<String>,
    /// Cues the rules placed but aren't sure of, for a person to check.
    pub flags: Vec<Flag>,
}

/// Why a placed cue is worth a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagKind {
    /// The drop is earlier than any drop cued by hand.
    EarlyDrop,
}

/// A doubt about cues the rules placed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    pub kind: FlagKind,
    /// The slots of the new cues it's about, such as `["B", "C"]`.
    pub slots: Vec<String>,
    /// The bar the doubt is about, from 0.
    pub bar: usize,
    /// One sentence for a person or an agent.
    pub why: String,
}

/// rekordbox's device palette entries for the names a rules file can use
/// (`rbl-anlz`'s `MEASURED_CUE_COLOURS`).
const COLOURS: [(&str, [u8; 3]); 8] = [
    ("pink", [0xFF, 0x00, 0x8A]),
    ("red", [0xFF, 0x00, 0x00]),
    ("orange", [0xFF, 0x8C, 0x00]),
    ("yellow", [0xFF, 0xFF, 0x00]),
    ("green", [0x00, 0xFF, 0x00]),
    ("aqua", [0x00, 0xFF, 0xFF]),
    ("blue", [0x00, 0x00, 0xFF]),
    ("purple", [0x99, 0x00, 0xFF]),
];

impl Rules {
    /// Parses and checks a rules file.
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        let rules: Self = toml::from_str(text)?;
        let mut seen: Vec<Option<u8>> = Vec::new();
        for (index, rule) in rules.cues.iter().enumerate() {
            let index = index + 1;
            let slot = Slot::parse(&rule.slot).map(Slot::hot).ok_or_else(|| RulesError::BadSlot { index, slot: rule.slot.clone() })?;
            if slot.is_some() && seen.contains(&slot) {
                return Err(RulesError::DuplicateSlot { index, slot: rule.slot.to_uppercase() });
            }
            seen.push(slot);
            if let Some(colour) = &rule.color {
                parse_colour(colour).ok_or_else(|| RulesError::BadColour { index, colour: colour.clone() })?;
            }
        }
        Ok(rules)
    }

    /// Finds the landmarks in a track and places every cue. A track from
    /// rekordbox keeps the cues it has: a rule only fills a hot cue slot
    /// that is empty, and adds a memory cue only where there is none
    /// within a beat.
    pub fn apply(&self, track: &Track) -> Applied {
        let landmarks = find_landmarks(&self.find, track);
        let mut cues: Vec<Cue> = track.rekordbox.iter().flat_map(|rb| rb.marks.iter().map(Cue::from_mark)).collect();
        let mut notes = Vec::new();
        let mut kept = Vec::new();
        // New cues placed from the drop, for the early-drop flag.
        let mut from_drop = Vec::new();
        let beat_secs = 60.0 / track.bpm.max(1.0);
        for rule in &self.cues {
            let hot = Slot::parse(&rule.slot).and_then(Slot::hot);
            let label = hot.map_or_else(|| "memory cue".to_owned(), |h| format!("cue {}", char::from(b'A' + h)));
            if hot.is_some() && cues.iter().any(|c| c.existing && c.hot == hot) {
                kept.push(format!("{label} was already set"));
                continue;
            }
            let at = match rule.at {
                Landmark::Start => landmarks.start,
                Landmark::Drop => landmarks.drop,
                Landmark::Groove => landmarks.groove,
            };
            let Some(at) = at else {
                notes.push(format!("{label}: no {:?} found", rule.at).to_lowercase());
                continue;
            };
            let Some(bar) =
                usize::try_from(i64::try_from(at).unwrap_or(0) + i64::from(rule.offset_bars)).ok().filter(|&b| b < track.bars.len())
            else {
                notes.push(format!("{label}: {} bars from the {:?} is off the track", rule.offset_bars, rule.at).to_lowercase());
                continue;
            };
            let Some(start_secs) = track.beat_secs(bar, 0) else { continue };
            if hot.is_none() && cues.iter().any(|c| c.existing && c.hot.is_none() && (c.start_secs - start_secs).abs() < beat_secs) {
                kept.push(format!("{label} at {start_secs:.1}s was already set"));
                continue;
            }
            let end_secs = rule.loop_bars.and_then(|bars| track.beat_secs(bar, i64::from(bars) * 4));
            if rule.at == Landmark::Drop {
                from_drop.push(hot.map_or_else(|| "memory".to_owned(), |h| char::from(b'A' + h).to_string()));
            }
            cues.push(Cue {
                id: None,
                hot,
                start_secs,
                end_secs,
                color: rule.color.as_deref().and_then(parse_colour),
                name: rule.name.clone().unwrap_or_default(),
                existing: false,
                attributes: None,
            });
        }
        cues.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
        let mut flags = Vec::new();
        if let Some(drop) = landmarks.drop.filter(|&d| d < self.find.doubt_drop_before && !from_drop.is_empty()) {
            flags.push(Flag {
                kind: FlagKind::EarlyDrop,
                why: format!(
                    "The drop was found at bar {}, earlier than any drop cued by hand (bar {} or later), so check {} by ear.",
                    drop + 1,
                    self.find.doubt_drop_before + 1,
                    from_drop.join(" and ")
                ),
                slots: from_drop,
                bar: drop,
            });
        }
        Applied { landmarks, cues, notes, kept, flags }
    }
}

fn parse_colour(colour: &str) -> Option<[u8; 3]> {
    if let Some(hex) = colour.strip_prefix('#') {
        let value = u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6)?;
        let [_, r, g, b] = value.to_be_bytes();
        return Some([r, g, b]);
    }
    COLOURS.iter().find(|(name, _)| name.eq_ignore_ascii_case(colour)).map(|&(_, rgb)| rgb)
}

fn find_landmarks(find: &Find, track: &Track) -> Landmarks {
    let start = track.bars.iter().position(|b| b.volume >= find.start_level);
    let drop = start.and_then(|start| find_drop(find, track, start));
    let groove = start.and_then(|start| find_groove(find, track, start));
    Landmarks { start, drop, groove }
}

/// Whether `bar` is on the phrase grid counted from `start`.
fn on_phrase(find: &Find, start: usize, bar: usize) -> bool {
    find.phrase_bars == 0 || bar.checked_sub(start).is_some_and(|d| d % find.phrase_bars == 0)
}

fn mean(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = values.fold((0.0, 0_u16), |(s, n), v| (s + v, n.saturating_add(1)));
    if n == 0 {
        0.0
    } else {
        sum / f32::from(n)
    }
}

/// The stretches with the bass below `level`, as bar ranges. One still
/// going at the end of the track is left out: nothing comes back from it.
fn bass_outs(track: &Track, level: f32) -> Vec<Range<usize>> {
    let mut outs = Vec::new();
    let mut from = None;
    for (i, bar) in track.bars.iter().enumerate() {
        match from {
            None if bar.bass < level => from = Some(i),
            Some(f) if bar.bass >= level => {
                outs.push(f..i);
                from = None;
            }
            _ => {}
        }
    }
    outs
}

/// The drop, as `find.drop` asks. A track with no break falls back on the
/// first rise in bass.
fn find_drop(find: &Find, track: &Track, start: usize) -> Option<usize> {
    match find.drop {
        DropPick::Break => find_break(find, track, start).or_else(|| find_rise(find, track, start, DropPick::First)),
        pick => find_rise(find, track, start, pick),
    }
}

/// Where the longest break in the first part of the track ends, and the
/// later one if two are as long. A short fill comes and goes; the big
/// breakdown is what the track builds towards. A bass-less intro is not a
/// break, and one ending on the phrase grid beats any other.
fn find_break(find: &Find, track: &Track, start: usize) -> Option<usize> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, reason = "a bar index")]
    let limit = (track.bars.len() as f32 * find.break_before.clamp(0.0, 1.0)) as usize;
    let outs: Vec<Range<usize>> =
        bass_outs(track, find.break_bass).into_iter().filter(|out| out.start > start && out.end <= limit).collect();
    let longest = |phrase_only: bool| {
        outs.iter().filter(|out| !phrase_only || on_phrase(find, start, out.end)).max_by_key(|out| (out.len(), out.end)).map(|out| out.end)
    };
    longest(true).or_else(|| longest(false))
}

/// The bar where the bass comes back hardest: the mean bass of the four
/// bars from it, less that of the four before it. Looked for on the phrase
/// grid first, and on any bar if nothing there clears `drop_rise`.
fn find_rise(find: &Find, track: &Track, start: usize, which: DropPick) -> Option<usize> {
    let bass: Vec<f32> = track.bars.iter().map(|b| b.bass).collect();
    let w = DROP_WINDOW_BARS;
    let rises: Vec<(usize, f32)> = (start + w..=bass.len().saturating_sub(w))
        .map(|i| {
            let before = mean(bass.get(i - w..i).unwrap_or(&[]).iter().copied());
            let after = mean(bass.get(i..i + w).unwrap_or(&[]).iter().copied());
            (i, after - before)
        })
        .collect();
    let pick = |candidates: &mut dyn Iterator<Item = &(usize, f32)>| -> Option<usize> {
        let mut clearing = candidates.filter(|(_, rise)| *rise >= find.drop_rise);
        match which {
            DropPick::Biggest => clearing.max_by(|a, b| a.1.total_cmp(&b.1)).map(|&(i, _)| i),
            DropPick::First | DropPick::Break => clearing.next().map(|&(i, _)| i),
        }
    };
    pick(&mut rises.iter().filter(|(i, _)| on_phrase(find, start, *i))).or_else(|| pick(&mut rises.iter()))
}

/// Where the last section late in the track starts: the later of the last
/// return of the bass and the last phrase where the mids and highs fall
/// away over a steady kick. A track with neither gets its most minimal
/// steady stretch.
fn find_groove(find: &Find, track: &Track, start: usize) -> Option<usize> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, reason = "a bar index")]
    let from = ((track.bars.len() as f32 * find.groove_after.clamp(0.0, 1.0)) as usize).max(start);
    let last = track.bars.len().saturating_sub(find.groove_end_bars);
    let returned = bass_outs(track, find.break_bass)
        .into_iter()
        .filter(|out| out.len() >= GROOVE_MIN_BREAK_BARS && (from..=last).contains(&out.end))
        .map(|out| out.end)
        .max();
    let stepped = (from..=last).rev().find(|&i| on_phrase(find, start, i) && busy_falls(find, track, i));
    returned.max(stepped).or_else(|| find_minimal(find, track, start, from))
}

/// Whether the mids and highs fall by `groove_step` at `bar`, with the kick
/// running on after it. Bars with the bass out don't count towards the level
/// before, since a breakdown's mids say little about the section.
fn busy_falls(find: &Find, track: &Track, bar: usize) -> bool {
    let w = GROOVE_WINDOW_BARS;
    let bars = &track.bars;
    let after = bars.get(bar..bars.len().min(bar + w)).unwrap_or(&[]);
    if after.iter().any(|b| b.bass < find.break_bass) {
        return false;
    }
    let before = bars.get(bar.saturating_sub(w)..bar).unwrap_or(&[]);
    let level = mean(before.iter().filter(|b| b.bass >= find.break_bass).map(|b| b.busy));
    level - mean(after.iter().map(|b| b.busy)) >= find.groove_step
}

/// The most minimal steady stretch from `from`: a phrase where the kick
/// never stops, the level hardly moves, and the mids and highs are quietest.
fn find_minimal(find: &Find, track: &Track, start: usize, from: usize) -> Option<usize> {
    let bars = &track.bars;
    let w = if find.phrase_bars == 0 { GROOVE_WINDOW_BARS } else { find.phrase_bars };
    let score = |i: usize| -> Option<f32> {
        let span = bars.get(i..i + w)?;
        if span.iter().any(|b| b.bass < find.groove_bass) {
            return None;
        }
        let level = mean(span.iter().map(|b| b.volume));
        let spread = mean(span.iter().map(|b| (b.volume - level).abs()));
        Some(mean(span.iter().map(|b| b.busy)) + 2.0 * spread)
    };
    let last = bars.len().saturating_sub(find.groove_end_bars.max(w));
    let best = |phrase_only: bool| {
        (from..=last)
            .filter(|&i| !phrase_only || on_phrase(find, start, i))
            .filter_map(|i| Some((i, score(i)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    best(true).or_else(|| best(false))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::float_cmp, reason = "made-up tracks of small sizes")]
mod tests {
    use super::*;
    use crate::rekordbox::RbTrack;
    use crate::track::{Bar, Beat, GridSource, Tags};

    /// A track at 120 BPM from bars of (volume, bass, busy).
    fn track(levels: &[(f32, f32, f32)]) -> Track {
        let beats: Vec<Beat> =
            (0..levels.len() * 4).map(|i| Beat { secs: i as f64 * 0.5, number: (i % 4 + 1) as u8, bpm: 120.0 }).collect();
        let bars = levels
            .iter()
            .enumerate()
            .map(|(i, &(volume, bass, busy))| Bar {
                start_secs: i as f64 * 2.0,
                end_secs: (i + 1) as f64 * 2.0,
                first_beat: i * 4,
                volume,
                bass,
                busy,
            })
            .collect();
        Track {
            path: "/x.mp3".into(),
            tags: Tags::default(),
            duration_secs: levels.len() as f64 * 2.0,
            bpm: 120.0,
            key: None,
            camelot: None,
            beats,
            bars,
            overview: Vec::new(),
            grid: GridSource::Kfbox,
            rekordbox: None,
        }
    }

    /// 80 bars: a kick intro, a full section, an eight-bar breakdown, the
    /// drop at bar 24, and a 32-bar kick-and-bass outro from bar 48.
    fn shaped() -> Track {
        let mut levels = vec![(0.6, 0.7, 0.2); 8];
        levels.extend(vec![(0.9, 0.9, 0.7); 8]);
        levels.extend(vec![(0.5, 0.05, 0.6); 8]);
        levels.extend(vec![(1.0, 1.0, 0.9); 24]);
        levels.extend(vec![(0.7, 0.8, 0.15); 32]);
        track(&levels)
    }

    #[test]
    fn an_early_drop_is_flagged_only_when_the_rules_placed_it() {
        let rules = Rules::parse(DEFAULT_RULES).unwrap();
        let applied = rules.apply(&shaped());
        assert_eq!(applied.flags.len(), 1);
        let flag = &applied.flags[0];
        assert_eq!((flag.kind, flag.bar, flag.slots.clone()), (FlagKind::EarlyDrop, 24, vec!["B".to_owned(), "C".to_owned()]));
        assert!(flag.why.contains("bar 25"), "{}", flag.why);

        let later = Rules::parse(&DEFAULT_RULES.replace("doubt_drop_before = 64", "doubt_drop_before = 24")).unwrap();
        assert_eq!(later.apply(&shaped()).flags, [], "a drop at the threshold isn't early");

        // Hand-cued B and C: the drop is Kristian's, so there's nothing to doubt.
        let mut track = shaped();
        let mark =
            |num: i32| Mark { id: None, num, start_secs: 48.0, end_secs: None, name: String::new(), color: None, attributes: Vec::new() };
        track.rekordbox = Some(RbTrack {
            id: "1".into(),
            path: None,
            attributes: Vec::new(),
            tempos: Vec::new(),
            marks: vec![mark(1), mark(2)],
            anlz: None,
        });
        assert_eq!(rules.apply(&track).flags, []);
    }

    #[test]
    fn the_default_rules_place_a_to_d_on_a_shaped_track() {
        let rules = Rules::parse(DEFAULT_RULES).unwrap();
        let applied = rules.apply(&shaped());
        assert_eq!(applied.landmarks.start, Some(0));
        assert_eq!(applied.landmarks.drop, Some(24));
        assert_eq!(applied.landmarks.groove, Some(48));
        let at: Vec<(Option<u8>, f64)> = applied.cues.iter().map(|c| (c.hot, c.start_secs)).collect();
        assert_eq!(at, vec![(Some(0), 0.0), (Some(1), 32.0), (Some(2), 48.0), (Some(3), 96.0)]);
        assert!(applied.notes.is_empty(), "{:?}", applied.notes);
    }

    #[test]
    fn the_drop_is_the_longest_break_not_the_first_fill() {
        let mut levels = vec![(0.9, 0.9, 0.5); 12];
        levels.extend(vec![(0.4, 0.2, 0.3); 4]); // a fill, ending at 16
        levels.extend(vec![(0.9, 0.9, 0.5); 16]);
        levels.extend(vec![(0.4, 0.2, 0.3); 8]); // the breakdown, ending at 40
        levels.extend(vec![(1.0, 1.0, 0.8); 60]);
        let applied = Rules::parse(DEFAULT_RULES).unwrap().apply(&track(&levels));
        assert_eq!(applied.landmarks.drop, Some(40));
    }

    #[test]
    fn a_bass_less_intro_and_a_late_break_are_not_the_drop() {
        let mut levels = vec![(0.3, 0.1, 0.2); 16]; // the intro, ending at 16
        levels.extend(vec![(0.9, 0.9, 0.5); 8]);
        levels.extend(vec![(0.4, 0.2, 0.3); 8]); // ending at 32
        levels.extend(vec![(0.9, 0.9, 0.5); 28]);
        levels.extend(vec![(0.4, 0.2, 0.3); 16]); // too late, past 55%
        levels.extend(vec![(0.9, 0.9, 0.5); 24]);
        let applied = Rules::parse(DEFAULT_RULES).unwrap().apply(&track(&levels));
        assert_eq!(applied.landmarks.drop, Some(32));
    }

    #[test]
    fn a_track_with_no_break_falls_back_on_the_first_rise() {
        let mut levels = vec![(0.8, 0.72, 0.5); 16];
        levels.extend(vec![(1.0, 1.0, 0.8); 64]);
        let applied = Rules::parse(DEFAULT_RULES).unwrap().apply(&track(&levels));
        assert_eq!(applied.landmarks.drop, Some(16));
    }

    #[test]
    fn the_groove_starts_after_the_last_break_when_that_is_latest() {
        let mut levels = vec![(1.0, 1.0, 0.8); 70];
        levels.extend(vec![(0.4, 0.2, 0.3); 2]); // comes back at 72
        levels.extend(vec![(1.0, 1.0, 0.8); 28]);
        let applied = Rules::parse(DEFAULT_RULES).unwrap().apply(&track(&levels));
        assert_eq!(applied.landmarks.groove, Some(72));
    }

    #[test]
    fn cues_already_in_rekordbox_are_kept_and_only_empty_slots_filled() {
        let mut track = shaped();
        let mark = |num: i32, start: f64| Mark {
            id: None,
            num,
            start_secs: start,
            end_secs: None,
            name: "mine".into(),
            color: None,
            attributes: vec![("Num".into(), num.to_string()), ("Start".into(), start.to_string())],
        };
        track.rekordbox = Some(RbTrack {
            id: "1".into(),
            path: None,
            attributes: Vec::new(),
            tempos: Vec::new(),
            marks: vec![mark(2, 50.0), mark(-1, 10.0)],
            anlz: None,
        });
        let rules = format!("{DEFAULT_RULES}\n[[cue]]\nslot = \"memory\"\nat = \"start\"\noffset_bars = 5\n");
        let applied = Rules::parse(&rules).unwrap().apply(&track);
        let c: Vec<&Cue> = applied.cues.iter().filter(|c| c.hot == Some(2)).collect();
        assert_eq!(c.len(), 1, "C stays the hand-placed one");
        assert!(c[0].existing && c[0].start_secs == 50.0);
        let new: Vec<Option<u8>> = applied.cues.iter().filter(|c| !c.existing).map(|c| c.hot).collect();
        assert_eq!(new, vec![Some(0), Some(1), Some(3)]);
        assert_eq!(applied.kept.len(), 2, "C, and the memory cue a beat from the one at 10 s: {:?}", applied.kept);
        assert_eq!(applied.cues.iter().filter(|c| c.existing).count(), 2);
    }

    #[test]
    fn a_cue_off_the_track_is_noted_not_placed() {
        let rules = Rules::parse("[[cue]]\nslot = \"A\"\nat = \"start\"\noffset_bars = -2\n").unwrap();
        let applied = rules.apply(&shaped());
        assert!(applied.cues.is_empty());
        assert_eq!(applied.notes.len(), 1);
    }

    #[test]
    fn bad_rules_say_what_is_wrong() {
        let twice = "[[cue]]\nslot = \"A\"\nat = \"start\"\n[[cue]]\nslot = \"a\"\nat = \"drop\"\n";
        assert!(matches!(Rules::parse(twice), Err(RulesError::DuplicateSlot { index: 2, .. })));
        assert!(matches!(Rules::parse("[[cue]]\nslot = \"Z\"\nat = \"start\"\n"), Err(RulesError::BadSlot { .. })));
        assert!(matches!(Rules::parse("[[cue]]\nslot = \"A\"\nat = \"start\"\ncolor = \"mauve\"\n"), Err(RulesError::BadColour { .. })));
        assert!(Rules::parse("[[cue]]\nslot = \"A\"\nat = \"somewhere\"\n").is_err());
    }

    #[test]
    fn colours_are_names_or_hex() {
        assert_eq!(parse_colour("Green"), Some([0, 255, 0]));
        assert_eq!(parse_colour("#102030"), Some([0x10, 0x20, 0x30]));
        assert_eq!(parse_colour("#1020"), None);
    }
}
