//! Auto-cue: kfbox places the rules' cues on every track with an empty slot
//! the rules fill, by itself, as Apply rules would, in batches that are each
//! one undoable change ("Auto-cue N tracks").
//!
//! It never fights a person: a cue already there is never moved, and a slot
//! where someone deleted a cue kfbox placed, or undid one, stays empty
//! (`learning.db`). Apply rules, asked for by hand, still fills it. A track
//! the rules place nothing new on is passed over; an early drop still gets
//! its cues and keeps its flag. While rekordbox is open, nothing is written
//! and the tracks wait.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::rules::Rules;
use crate::safe::{SafeError, Slot};
use crate::session::{CommandError, CommandResult, CueEdit, Origin, Session, TrackRow};

/// Tracks analysed and written together, so a big library shows progress
/// and an interruption loses little.
pub const BATCH: usize = 25;

/// Where auto-cue has got to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AutoCueProgress {
    /// Tracks looked at so far, and in all.
    pub done: usize,
    pub total: usize,
    /// Tracks that got cues so far.
    pub cued: usize,
    /// The tracks a batch just wrote, as the library now has them; empty
    /// between writes.
    pub changed: Vec<TrackRow>,
}

/// What a run of auto-cue did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AutoCueOutcome {
    /// Tracks that got cues.
    pub cued: usize,
    /// Tracks still to do because rekordbox is open.
    pub waiting: usize,
    /// The history's commits, one a batch.
    pub commits: Vec<String>,
    /// Every track whose cues changed, as the library now has it.
    pub changed: Vec<TrackRow>,
}

fn slot_name(hot: Option<u8>) -> String {
    hot.map_or_else(|| "memory".to_owned(), |h| char::from(b'A' + h).to_string())
}

impl Session {
    /// The tracks auto-cue would look at: in the library, with their file,
    /// and missing one of the hot cues the saved rules place. Cheap: it reads
    /// rows, not audio.
    pub fn auto_cue_candidates(&self) -> CommandResult<Vec<String>> {
        let rules = Rules::parse(&self.load_rules())?;
        let slots: Vec<char> =
            rules.cues.iter().filter_map(|r| Slot::parse(&r.slot).and_then(Slot::hot)).map(|h| char::from(b'A' + h)).collect();
        if slots.is_empty() {
            return Ok(Vec::new());
        }
        let refused = self.refused_slots()?;
        Ok(self
            .playlist_rows(None)?
            .into_iter()
            .filter(|row| !row.path.is_empty() && !row.missing)
            .filter(|row| slots.iter().any(|slot| !row.hot.contains(*slot) && !refused.contains(&(row.id.clone(), slot.to_string()))))
            .map(|row| row.id)
            .collect())
    }

    fn refused_slots(&self) -> CommandResult<HashSet<(String, String)>> {
        self.learning_refused()
    }

    /// Places the saved rules' cues on these tracks' empty slots, `BATCH`
    /// tracks a change. Stops early, with the rest waiting, if rekordbox is
    /// open or `stop` says so. Tracks that can't be read are passed over.
    pub fn auto_cue(
        &self,
        ids: &[String],
        progress: &mut dyn FnMut(&AutoCueProgress),
        stop: &dyn Fn() -> bool,
    ) -> CommandResult<AutoCueOutcome> {
        let rules = Rules::parse(&self.load_rules())?;
        let mut outcome = AutoCueOutcome::default();
        let mut done = 0;
        for (at, batch) in ids.chunks(BATCH).enumerate() {
            if stop() {
                break;
            }
            if self.rekordbox_running() {
                outcome.waiting = ids.len() - at * BATCH;
                return Ok(outcome);
            }
            let refused = self.refused_slots()?;
            let mut edits = Vec::new();
            let mut tracks = 0;
            for id in batch {
                done += 1;
                let Ok((_, track, _)) = self.track_by_id(id) else { continue };
                let before = edits.len();
                for cue in rules.apply(&track).cues.into_iter().filter(|c| !c.existing) {
                    let slot = slot_name(cue.hot);
                    if refused.contains(&(id.clone(), slot.clone())) {
                        continue;
                    }
                    edits.push(CueEdit::Add {
                        track: id.clone(),
                        slot,
                        start_secs: cue.start_secs,
                        end_secs: cue.end_secs,
                        name: cue.name,
                        color: cue.color.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")),
                        replace: false,
                    });
                }
                tracks += usize::from(edits.len() > before);
                progress(&AutoCueProgress { done, total: ids.len(), cued: outcome.cued + tracks, changed: Vec::new() });
            }
            if edits.is_empty() {
                continue;
            }
            let summary = if tracks == 1 { "Auto-cue 1 track".to_owned() } else { format!("Auto-cue {tracks} tracks") };
            match self.save_cues_as(&summary, &edits, Origin::Rules) {
                Ok(saved) => {
                    outcome.cued += tracks;
                    outcome.commits.extend(saved.commit);
                    progress(&AutoCueProgress { done, total: ids.len(), cued: outcome.cued, changed: saved.changed.clone() });
                    outcome.changed.extend(saved.changed);
                }
                // rekordbox opened between the check and the write.
                Err(CommandError::Safe(SafeError::RekordboxRunning)) => {
                    outcome.waiting = ids.len() - at * BATCH;
                    return Ok(outcome);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(outcome)
    }
}
