//! One song: decoded, analysed, and boiled down to what the rules read.
//!
//! The rules think in bars, so the analysis is reduced to one row per bar:
//! how loud it is, how much bass it has, and how busy the mids and highs
//! are. Each is scaled so 1.0 is the track's own loud end, which lets one
//! rules file work on a quiet master and a loud one alike.

use std::path::{Path, PathBuf};

use rbl_analysis::waveform::{Waveform, WaveformColumn};
use serde::{Deserialize, Serialize};

use crate::rekordbox::{RbTrack, Tempo};

/// Columns kept for drawing the whole track.
pub const OVERVIEW_COLUMNS: usize = 2400;

/// The longest stretch decoded. A DJ track is well inside it; a two-hour
/// mix dropped in by mistake does not fill the memory.
const MAX_DECODE_SECS: f64 = 30.0 * 60.0;

#[derive(Debug, thiserror::Error)]
pub enum TrackError {
    #[error(transparent)]
    Audio(#[from] rbl_audio::AudioError),
    #[error("no beat grid found: the track may be silent or too short")]
    NoGrid,
    #[error("the collection entry has no file location")]
    NoFile,
    #[error("can't read the file: {0}")]
    Io(std::io::Error),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Beat {
    pub secs: f64,
    /// 1..=4, where 1 is the downbeat.
    pub number: u8,
    pub bpm: f64,
}

/// One bar, from a downbeat to the next.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bar {
    pub start_secs: f64,
    pub end_secs: f64,
    /// Index of the bar's downbeat in [`Track::beats`].
    pub first_beat: usize,
    /// Overall level, 0..=1 of the track's loud end.
    pub volume: f32,
    /// Below 200 Hz: the kick and the bassline.
    pub bass: f32,
    /// Mids and highs together: melody, vocals, hats.
    pub busy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub path: PathBuf,
    pub tags: Tags,
    pub duration_secs: f64,
    pub bpm: f64,
    /// e.g. `Ebm`.
    pub key: Option<String>,
    /// e.g. `2A`.
    pub camelot: Option<String>,
    pub beats: Vec<Beat>,
    pub bars: Vec<Bar>,
    /// `[low, mid, high, peak]` per column, [`OVERVIEW_COLUMNS`] across the track.
    pub overview: Vec<[u8; 4]>,
    /// Where [`Track::beats`] came from.
    pub grid: GridSource,
    /// The collection entry the track was loaded from, cues and all.
    #[serde(skip)]
    pub rekordbox: Option<RbTrack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GridSource {
    /// The `TEMPO` entries of the rekordbox XML.
    Rekordbox,
    /// rbl-analysis's own grid.
    Kfbox,
}

impl Track {
    /// Decodes and analyses a file, grid included. Under a second per
    /// track in a release build.
    pub fn analyse(path: &Path) -> Result<Self, TrackError> {
        let audio = rbl_audio::decode_mono(path, Some(MAX_DECODE_SECS))?;
        let analysis = rbl_analysis::analyse(&audio.samples, audio.sample_rate);
        let beats: Vec<Beat> = analysis
            .tempo
            .beats
            .iter()
            .map(|b| Beat {
                secs: f64::from(b.time_ms) / 1000.0,
                number: u8::try_from(b.beat_number).unwrap_or(1),
                bpm: f64::from(b.tempo_x100) / 100.0,
            })
            .collect();
        let key = analysis.key.as_ref().map(|k| k.name.clone());
        let camelot = analysis.key.as_ref().map(rbl_analysis::MusicalKey::camelot);
        let mut track = Self::measure(path, &audio, beats, &analysis.waveform, GridSource::Kfbox)?;
        track.bpm = analysis.tempo.bpm;
        track.key = key;
        track.camelot = camelot;
        track.tags = read_tags(path);
        Ok(track)
    }

    /// A track from a rekordbox collection. Its own `TEMPO` grid is used
    /// so cues land on the beats rekordbox shows; a track rekordbox has not
    /// analysed, with no `TEMPO`, gets kfbox's grid instead.
    pub fn from_rekordbox(source: &RbTrack) -> Result<Self, TrackError> {
        let path = source.path.as_deref().ok_or(TrackError::NoFile)?;
        let source = with_grid(source);
        let mut track = if source.tempos.is_empty() {
            Self::analyse(path)?
        } else {
            let audio = rbl_audio::decode_mono(path, Some(MAX_DECODE_SECS))?;
            let waveform = rbl_analysis::waveform::compute(&audio.samples, audio.sample_rate);
            let beats = grid_beats(&source.tempos, audio.duration_secs());
            let mut track = Self::measure(path, &audio, beats, &waveform, GridSource::Rekordbox)?;
            track.bpm = source.tempos.first().map_or(0.0, |t| t.bpm);
            track.key = Some(source.attribute("Tonality")).filter(|k| !k.is_empty());
            track
        };
        track.attach(source);
        Ok(track)
    }

    /// Takes the tags and cues from the collection entry the track came from.
    pub(crate) fn attach(&mut self, source: RbTrack) {
        let named = |name: &str, fallback: &str| {
            let value = source.attribute(name);
            if value.is_empty() { fallback.to_owned() } else { value }
        };
        self.tags = Tags {
            title: named("Name", &self.tags.title),
            artist: named("Artist", &self.tags.artist),
            album: named("Album", &self.tags.album),
            genre: named("Genre", &self.tags.genre),
        };
        self.rekordbox = Some(source);
    }

    /// Bars and the overview from a grid and a waveform.
    fn measure(path: &Path, audio: &rbl_audio::Audio, beats: Vec<Beat>, waveform: &Waveform, grid: GridSource) -> Result<Self, TrackError> {
        let duration_secs = audio.duration_secs();
        let bars = bars(&beats, &waveform.columns, waveform.columns_per_sec, duration_secs);
        if bars.is_empty() {
            return Err(TrackError::NoGrid);
        }
        let overview = waveform.reduced(OVERVIEW_COLUMNS).iter().map(|c| [c.low, c.mid, c.high, c.peak]).collect();
        Ok(Self {
            path: path.to_path_buf(),
            tags: Tags::default(),
            duration_secs,
            bpm: 0.0,
            key: None,
            camelot: None,
            grid,
            beats,
            bars,
            overview,
            rekordbox: None,
        })
    }

    /// The time of the beat `offset` beats after bar `bar`'s downbeat. Past
    /// either end of the grid the last known beat length carries on, so a
    /// loop that runs off the end still has an end.
    pub fn beat_secs(&self, bar: usize, offset: i64) -> Option<f64> {
        let first = self.bars.get(bar)?.first_beat;
        let index = i64::try_from(first).ok()? + offset;
        let last = self.beats.len().checked_sub(1)?;
        if let Some(beat) = usize::try_from(index).ok().and_then(|i| self.beats.get(i)) {
            return Some(beat.secs);
        }
        let (edge, period) = if index < 0 {
            let period = self.beats.get(1).map_or(0.5, |b| b.secs) - self.beats.first()?.secs;
            (0_usize, period)
        } else {
            let period = self.beats.get(last)?.secs - self.beats.get(last.checked_sub(1)?)?.secs;
            (last, period)
        };
        let from = self.beats.get(edge)?.secs;
        let steps = index - i64::try_from(edge).ok()?;
        #[allow(clippy::cast_precision_loss, reason = "a beat count")]
        Some(from + steps as f64 * period)
    }
}

/// A song's waveform at a given fineness, as flat `[low, mid, high, peak]` columns.
pub(crate) fn waveform_columns(path: &Path, per_sec: f64) -> Result<Vec<u8>, TrackError> {
    let audio = rbl_audio::decode_mono(path, Some(MAX_DECODE_SECS))?;
    let waveform = rbl_analysis::waveform::compute(&audio.samples, audio.sample_rate);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a column count")]
    let columns = (audio.duration_secs() * per_sec).round() as usize;
    Ok(waveform.reduced(columns).iter().flat_map(|c| [c.low, c.mid, c.high, c.peak]).collect())
}

/// A track from rekordbox's own library keeps its grid in its analysis file
/// rather than in `TEMPO` entries.
pub(crate) fn with_grid(source: &RbTrack) -> RbTrack {
    let mut source = source.clone();
    if source.tempos.is_empty() {
        if let Some(dat) = &source.anlz {
            source.tempos = crate::master::grid_from_anlz(dat);
        }
    }
    source
}

/// Every beat of a rekordbox grid. Each `TEMPO` runs to the next, or to
/// the end; the grid also runs back from the first one to the start of the
/// file, as rekordbox draws it.
fn grid_beats(tempos: &[Tempo], duration_secs: f64) -> Vec<Beat> {
    let mut tempos: Vec<&Tempo> = tempos.iter().filter(|t| t.bpm > 0.0).collect();
    tempos.sort_by(|a, b| a.inizio.total_cmp(&b.inizio));
    let number = |battito: u8, k: i64| -> u8 { u8::try_from((i64::from(battito.clamp(1, 4)) - 1 + k).rem_euclid(4) + 1).unwrap_or(1) };
    let mut beats = Vec::new();
    if let Some(first) = tempos.first() {
        let period = 60.0 / first.bpm;
        let mut k = 1_i64;
        #[allow(clippy::cast_precision_loss, reason = "a beat count")]
        while first.inizio - k as f64 * period >= 0.0 {
            beats.push(Beat { secs: first.inizio - k as f64 * period, number: number(first.battito, -k), bpm: first.bpm });
            k += 1;
        }
        beats.reverse();
    }
    for (i, tempo) in tempos.iter().enumerate() {
        let end = tempos.get(i + 1).map_or(duration_secs, |next| next.inizio);
        let period = 60.0 / tempo.bpm;
        let mut k = 0_i64;
        loop {
            #[allow(clippy::cast_precision_loss, reason = "a beat count")]
            let secs = tempo.inizio + k as f64 * period;
            if secs >= end - 1e-6 {
                break;
            }
            beats.push(Beat { secs, number: number(tempo.battito, k), bpm: tempo.bpm });
            k += 1;
        }
    }
    beats
}

/// Splits the grid into bars and measures each.
fn bars(beats: &[Beat], columns: &[WaveformColumn], columns_per_sec: f64, duration_secs: f64) -> Vec<Bar> {
    let downbeats: Vec<usize> = beats.iter().enumerate().filter(|(_, b)| b.number == 1).map(|(i, _)| i).collect();
    let mut bars: Vec<Bar> = downbeats
        .iter()
        .enumerate()
        .filter_map(|(n, &first_beat)| {
            let start_secs = beats.get(first_beat)?.secs;
            let end_secs = match downbeats.get(n + 1) {
                Some(&next) => beats.get(next)?.secs,
                None => (start_secs + 4.0 * 60.0 / beats.get(first_beat)?.bpm.max(1.0)).min(duration_secs),
            };
            (end_secs > start_secs).then_some(Bar { start_secs, end_secs, first_beat, volume: 0.0, bass: 0.0, busy: 0.0 })
        })
        .collect();

    let mut raw: Vec<[f32; 3]> = bars
        .iter()
        .map(|bar| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "column indices of a track")]
            let range = |secs: f64| ((secs * columns_per_sec) as usize).min(columns.len());
            let span = columns.get(range(bar.start_secs)..range(bar.end_secs)).unwrap_or(&[]);
            if span.is_empty() {
                return [0.0; 3];
            }
            #[allow(clippy::cast_precision_loss, reason = "a column count")]
            let n = span.len() as f32;
            let mean = |f: fn(&WaveformColumn) -> f32| span.iter().map(f).sum::<f32>() / n;
            [
                mean(|c| f32::from(c.peak)),
                mean(|c| f32::from(c.low)),
                mean(|c| f32::from(c.mid) + f32::from(c.high)),
            ]
        })
        .collect();
    for feature in 0..3 {
        let loud_end = percentile(raw.iter().map(|r| r[feature]), 0.95);
        for values in &mut raw {
            values[feature] = if loud_end > 0.0 { (values[feature] / loud_end).min(1.0) } else { 0.0 };
        }
    }
    for (bar, [volume, bass, busy]) in bars.iter_mut().zip(raw) {
        bar.volume = volume;
        bar.bass = bass;
        bar.busy = busy;
    }
    bars
}

fn percentile(values: impl Iterator<Item = f32>, p: f32) -> f32 {
    let mut sorted: Vec<f32> = values.filter(|v| *v > 0.0).collect();
    sorted.sort_by(f32::total_cmp);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, reason = "an index into the list")]
    let index = ((sorted.len().saturating_sub(1)) as f32 * p).round() as usize;
    sorted.get(index).copied().unwrap_or(0.0)
}

fn read_tags(path: &Path) -> Tags {
    use lofty::prelude::*;

    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let Ok(file) = lofty::read_from_path(path) else {
        return Tags { title: stem, ..Tags::default() };
    };
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Tags { title: stem, ..Tags::default() };
    };
    let text = |value: Option<std::borrow::Cow<'_, str>>| value.map(std::borrow::Cow::into_owned).unwrap_or_default();
    let title = text(tag.title());
    Tags {
        title: if title.is_empty() { stem } else { title },
        artist: text(tag.artist()),
        album: text(tag.album()),
        genre: text(tag.genre()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn tempo(inizio: f64, bpm: f64, battito: u8) -> Tempo {
        Tempo { inizio, bpm, battito, attributes: Vec::new() }
    }

    #[test]
    fn a_rekordbox_grid_runs_back_to_the_start_and_through_each_change() {
        // 120 BPM from 1.25 s on beat 1, 60 BPM from 5.25 s on beat 3.
        let beats = grid_beats(&[tempo(5.25, 60.0, 3), tempo(1.25, 120.0, 1)], 8.0);
        let got: Vec<(f64, u8)> = beats.iter().map(|b| (b.secs, b.number)).collect();
        assert_eq!(
            got,
            vec![
                (0.25, 3),
                (0.75, 4),
                (1.25, 1),
                (1.75, 2),
                (2.25, 3),
                (2.75, 4),
                (3.25, 1),
                (3.75, 2),
                (4.25, 3),
                (4.75, 4),
                (5.25, 3),
                (6.25, 4),
                (7.25, 1),
            ]
        );
        assert_eq!(beats.last().unwrap().bpm, 60.0);
    }
}
