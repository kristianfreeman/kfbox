//! How well a rules file matches the cues someone set by hand: on each track
//! with hot cues A to D all set, the cues are hidden, the rules placed, and
//! each rule cue counts as a hit if it lands in the same bar. Fixes count too:
//! where a person moved a cue the rules or an agent placed, a hit lands in the
//! bar they moved it to; where they deleted one, a hit stays out of that bar.
//! The `score` example prints the same for the whole library.

use serde::{Deserialize, Serialize};

use crate::learning::Correction;
use crate::rules::Rules;
use crate::session::{CommandResult, Session};
use crate::track::Track;

const SLOTS: [&str; 4] = ["A", "B", "C", "D"];

/// One slot's hits, out of the tracks scored, and out of the fixes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlotScore {
    pub slot: String,
    pub hits: usize,
    pub tracks: usize,
    /// Fixes a person made to a placed cue in this slot, on tracks not
    /// already scored, and how many the rules now agree with.
    pub fixes: usize,
    pub fix_hits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Score {
    /// The track IDs scored: every one with A to D set by hand.
    pub tracks: Vec<String>,
    pub slots: Vec<SlotScore>,
}

impl Score {
    /// Hits on hand-cued tracks and on fixes together.
    #[must_use]
    pub fn hits(&self) -> usize {
        self.slots.iter().map(|s| s.hits + s.fix_hits).sum()
    }
}

/// Where a time falls in bars, a part-way bar as a fraction.
fn bar_of(track: &Track, secs: f64) -> f64 {
    let next = track.bars.iter().position(|b| b.start_secs > secs + 0.02).unwrap_or(track.bars.len());
    let Some(index) = next.checked_sub(1) else { return 0.0 };
    let Some(bar) = track.bars.get(index) else { return 0.0 };
    #[allow(clippy::cast_precision_loss, reason = "a bar index")]
    let whole = index as f64;
    whole + ((secs - bar.start_secs) / (bar.end_secs - bar.start_secs)).max(0.0)
}

impl Session {
    /// The tracks of the open library with hot cues A to D all set, which are
    /// the ones a rules file can be scored on.
    pub fn hand_cued(&self) -> CommandResult<Vec<String>> {
        self.with_library(|library| {
            Ok(library
                .all_tracks()
                .into_iter()
                .filter(|t| (0..4).all(|num| t.marks.iter().any(|m| m.num == num)))
                .map(|t| t.id.clone())
                .collect())
        })
    }

    /// Scores `rules` against the hand-cued tracks. Their analysis is cached,
    /// so only the first scoring decodes anything.
    pub fn score_rules(&self, rules: &str) -> CommandResult<Score> {
        let parsed = Rules::parse(rules)?;
        let tracks = self.hand_cued()?;
        let mut hits = [0_usize; 4];
        let mut scored = Vec::new();
        for id in &tracks {
            let source = self.source(id)?;
            let Ok((_, mut track, _)) = self.track_by_id(id) else { continue };
            let wanted: Vec<f64> =
                (0..4).filter_map(|num| source.marks.iter().find(|m| m.num == num).map(|m| bar_of(&track, m.start_secs))).collect();
            if let Some(rb) = track.rekordbox.as_mut() {
                rb.marks.clear();
            }
            let placed = parsed.apply(&track);
            for (slot, want) in wanted.iter().enumerate() {
                let hit = placed
                    .cues
                    .iter()
                    .find(|c| c.hot.map(usize::from) == Some(slot))
                    .is_some_and(|c| (bar_of(&track, c.start_secs) - want).abs() < 0.5);
                if hit {
                    if let Some(count) = hits.get_mut(slot) {
                        *count += 1;
                    }
                }
            }
            scored.push(id.clone());
        }
        let (fixes, fix_hits) = self.score_fixes(&parsed, &scored)?;
        let slots = SLOTS
            .iter()
            .enumerate()
            .map(|(i, slot)| SlotScore {
                slot: (*slot).to_owned(),
                hits: hits[i],
                tracks: scored.len(),
                fixes: fixes[i],
                fix_hits: fix_hits[i],
            })
            .collect();
        Ok(Score { tracks: scored, slots })
    }

    /// Each slot's fixes and how many `rules` agree with, the latest fix per
    /// track and slot, on tracks not in `scored`.
    fn score_fixes(&self, rules: &Rules, scored: &[String]) -> CommandResult<([usize; 4], [usize; 4])> {
        let mut latest: Vec<Correction> = Vec::new();
        for fix in self.corrections()? {
            latest.retain(|f| !(f.track == fix.track && f.slot == fix.slot));
            latest.push(fix);
        }
        let mut fixes = [0_usize; 4];
        let mut hits = [0_usize; 4];
        for fix in latest.iter().filter(|f| !scored.contains(&f.track)) {
            let Some(slot) = SLOTS.iter().position(|s| *s == fix.slot) else { continue };
            let Ok((_, mut track, _)) = self.track_by_id(&fix.track) else { continue };
            if let Some(rb) = track.rekordbox.as_mut() {
                rb.marks.clear();
            }
            let placed = rules.apply(&track);
            let cue = placed.cues.iter().find(|c| c.hot.map(usize::from) == Some(slot)).map(|c| bar_of(&track, c.start_secs));
            let same = |a: f64, b: f64| (a - b).abs() < 0.5;
            let hit = match fix.to_secs {
                Some(to) => cue.is_some_and(|bar| same(bar, bar_of(&track, to))),
                None => cue.is_none_or(|bar| !same(bar, bar_of(&track, fix.from_secs))),
            };
            fixes[slot] += 1;
            hits[slot] += usize::from(hit);
        }
        Ok((fixes, hits))
    }
}
