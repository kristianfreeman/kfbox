//! The rules wizard: you describe how you cue, the model reads your own
//! hand-cued tracks, writes a rules file in kfbox's schema, and kfbox scores
//! each draft against those tracks. You refine it with more guidance, then save.

use kfbox_core::scoring::Score;
use kfbox_core::session::Session;
use kfbox_core::tools::{self, Toolset};
use kfbox_core::{Rules, DEFAULT_RULES};
use serde::Serialize;
use serde_json::{json, Value};

use crate::client::{self, AssistError, Step, Transport};
use crate::settings::Model;

/// Tool calls one turn of the wizard may make.
const MAX_STEPS: usize = 24;

fn system() -> String {
    format!(
        "You write cue rules for kfbox, a DJ prep app. The user describes how they set cues; you turn that into a rules \
file in kfbox's TOML schema, shown here with every setting explained:\n\n{DEFAULT_RULES}\n\n\
Work like this: call hand_cued_tracks to see the tracks the user cued by hand, read a few with track_detail \
(levels on) to see where their cues sit against the bass and volume, then call submit_rules with a complete rules file. \
It's checked and scored against every hand-cued track: a hit is a rule cue in the same bar as the user's. Improve and \
submit again while it helps; stop after a few tries. Keep the schema exactly: only the [find] settings and [[cue]] \
fields shown. If something essential is unclear, ask one short question instead. End with two or three sentences on \
what the rules do and how they scored."
    )
}

/// A checked draft and its score.
#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    pub rules: String,
    pub score: Score,
}

/// One turn of the wizard.
#[derive(Debug, Clone, Serialize)]
pub struct WizardTurn {
    pub reply: String,
    /// The best draft so far, if any passed.
    pub draft: Option<Draft>,
    pub steps: Vec<Step>,
}

/// The wizard's conversation, kept between turns.
#[derive(Debug, Default)]
pub struct Wizard {
    messages: Vec<Value>,
    best: Option<Draft>,
}

fn wizard_tools() -> Vec<Value> {
    let mut defs: Vec<Value> = tools::definitions(Toolset::Read).into_iter().filter(|t| t["name"] == "track_detail").collect();
    defs.push(json!({
        "name": "hand_cued_tracks",
        "description": "The tracks the user cued by hand (hot cues A to D all set), with their IDs, titles and those cues.",
        "inputSchema": { "type": "object", "properties": {} }
    }));
    defs.push(json!({
        "name": "submit_rules",
        "description": "Checks a complete rules file and scores it against every hand-cued track. Returns the hits per cue, or what's wrong with the file.",
        "inputSchema": {
            "type": "object",
            "properties": { "rules": { "type": "string", "description": "The whole rules file, in TOML." } },
            "required": ["rules"]
        }
    }));
    client::api_tools(&defs)
}

impl Wizard {
    /// Starts again, forgetting the conversation and the drafts.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn tool(&mut self, session: &Session, name: &str, input: &Value) -> Result<Value, String> {
        match name {
            "hand_cued_tracks" => {
                let ids = session.hand_cued().map_err(|e| e.to_string())?;
                let mut tracks = Vec::new();
                for id in ids {
                    let summary = session.track_summary(&id, false).map_err(|e| e.to_string())?;
                    let cues: Vec<Value> =
                        summary.cues.iter().map(|c| json!({ "slot": c.slot, "bar": c.bar, "secs": c.start_secs })).collect();
                    tracks.push(json!({ "id": id, "title": format!("{} – {}", summary.row.artist, summary.row.title), "bars": summary.bars, "cues": cues }));
                }
                Ok(Value::Array(tracks))
            }
            "submit_rules" => {
                let rules = input["rules"].as_str().ok_or("rules is required")?;
                Rules::parse(rules).map_err(|e| format!("the rules don't parse: {e}"))?;
                let score = session.score_rules(rules).map_err(|e| e.to_string())?;
                let better = self.best.as_ref().is_none_or(|best| score.hits() >= best.score.hits());
                if better {
                    self.best = Some(Draft { rules: rules.to_owned(), score: score.clone() });
                }
                Ok(json!({ "score": score, "best_so_far": better }))
            }
            _ => tools::call(session, Toolset::Read, name, input, "").unwrap_or_else(|| Err(format!("there's no tool {name}"))),
        }
    }

    /// Takes the user's description, or more guidance on the draft, and runs a turn.
    pub fn turn(
        &mut self,
        session: &Session,
        transport: &dyn Transport,
        key: &str,
        model: Model,
        said: &str,
    ) -> Result<WizardTurn, AssistError> {
        let said = said.trim();
        if said.is_empty() {
            return Err(AssistError::Other("say how you cue first".to_owned()));
        }
        let text = match &self.best {
            Some(draft) => format!("{said}\n\nThe current draft:\n\n{}", draft.rules),
            None => said.to_owned(),
        };
        let tools = wizard_tools();
        let system = system();
        let call = client::Call { transport, key, model, system: &system, tools: &tools, max_steps: MAX_STEPS };
        let mut messages = self.messages.clone();
        messages.push(json!({ "role": "user", "content": text }));
        // The conversation only keeps a turn that finished: a failed one leaves
        // no half-finished exchange behind, and can be asked again.
        let outcome = client::run(&call, &mut messages, &mut |name, input| self.tool(session, name, input))?;
        self.messages = messages;
        Ok(WizardTurn { reply: outcome.text, draft: self.best.clone(), steps: outcome.steps })
    }
}
