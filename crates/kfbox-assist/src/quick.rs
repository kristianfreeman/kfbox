//! Quick functions: a short instruction, with the selected track or the
//! current playlist as context, done by the model with kfbox's tools.

use kfbox_core::session::{Saved, Session, TrackRow};
use kfbox_core::tools::{self, Toolset};
use serde::{Deserialize, Serialize};

use crate::client::{self, AssistError, Step, Transport};
use crate::settings::Model;

/// Tool calls one instruction may make before it's stopped.
const MAX_STEPS: usize = 16;

/// The label on the history's message for a change made here.
pub const LABEL: &str = "Assistant";

const SYSTEM: &str = "You are the assistant inside kfbox, a DJ prep app working on the user's rekordbox library. \
You act through tools that read tracks and write cues. Hot cues are slots A to P; a memory cue has no slot. \
The user's rules place cues automatically (propose_cues shows what they would do). Writes go straight into the library \
as one undoable change each, so make the change you're asked for without asking first. Put cues on downbeats: \
track_detail gives each bar's start, and its levels (volume, bass, busy, 0 to 1 of the track's loud end) show the \
structure: a breakdown is a stretch of low bass, a drop is where the bass comes back. When you're done, say in one or \
two short sentences what you did, or why you couldn't. Don't list every cue.";

/// What the instruction is about.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Context {
    /// The selected track's ID.
    pub track: Option<String>,
    /// The playlist on screen, by its path; `None` for the whole collection.
    pub playlist: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuickResult {
    pub reply: String,
    pub steps: Vec<Step>,
    /// Every track whose cues changed, as the library now has it.
    pub changed: Vec<TrackRow>,
}

fn context_line(session: &Session, context: &Context) -> String {
    let mut parts = Vec::new();
    if let Some(id) = &context.track {
        let title = session
            .search("", usize::MAX)
            .ok()
            .and_then(|rows| rows.into_iter().find(|r| &r.id == id))
            .map(|r| format!("{} – {}", r.artist, r.title));
        parts.push(format!("The selected track is {id}{}.", title.map(|t| format!(" ({t})")).unwrap_or_default()));
    }
    match &context.playlist {
        Some(path) => parts.push(format!("The playlist on screen is \"{path}\".")),
        None => parts.push("The whole collection is on screen.".to_owned()),
    }
    parts.join(" ")
}

/// Runs one instruction. The session must have rekordbox's library open.
pub fn run(
    session: &Session,
    transport: &dyn Transport,
    key: &str,
    model: Model,
    instruction: &str,
    context: &Context,
) -> Result<QuickResult, AssistError> {
    let instruction = instruction.trim();
    if instruction.is_empty() {
        return Err(AssistError::Other("there's no instruction".to_owned()));
    }
    let tools = client::api_tools(&tools::definitions(Toolset::Write));
    let mut messages = vec![serde_json::json!({
        "role": "user",
        "content": format!("{}\n\n{instruction}", context_line(session, context)),
    })];
    let mut changed: Vec<TrackRow> = Vec::new();
    let mut run_tool = |name: &str, input: &serde_json::Value| {
        let answer = tools::call(session, Toolset::Write, name, input, LABEL).unwrap_or_else(|| Err(format!("there's no tool {name}")))?;
        if let Ok(saved) = serde_json::from_value::<Saved>(answer.clone()) {
            for row in saved.changed {
                changed.retain(|r| r.id != row.id);
                changed.push(row);
            }
        }
        Ok(answer)
    };
    let call = client::Call { transport, key, model, system: SYSTEM, tools: &tools, max_steps: MAX_STEPS };
    let outcome = client::run(&call, &mut messages, &mut run_tool)?;
    Ok(QuickResult { reply: outcome.text, steps: outcome.steps, changed })
}
