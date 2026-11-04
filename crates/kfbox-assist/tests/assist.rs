//! The assistant against a scripted stand-in for the API and a throwaway
//! fixture library: no network, and no real key.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use kfbox_assist::client::{AssistError, Transport};
use kfbox_assist::keys::{check_key, KeyStore, MemoryKeys};
use kfbox_assist::quick::{self, Context};
use kfbox_assist::settings::{Model, Settings};
use kfbox_assist::wizard::Wizard;
use kfbox_core::session::{CueEdit, Session};
use kfbox_core::testing;
use rbl_db::fixture::{self, Shape};
use serde_json::{json, Value};

/// A key no test would ever send anywhere real.
const KEY: &str = "sk-ant-test-NOT-A-REAL-KEY-42";

/// A request as sent: its headers and its body.
type Sent = (Vec<(String, String)>, Value);

/// Answers each request with the next scripted reply, and keeps what was sent.
#[derive(Default)]
struct Scripted {
    replies: Mutex<VecDeque<(u16, Value)>>,
    sent: Mutex<Vec<Sent>>,
}

impl Scripted {
    fn new(replies: Vec<(u16, Value)>) -> Self {
        Self { replies: Mutex::new(replies.into()), sent: Mutex::default() }
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }
}

impl Transport for Scripted {
    fn post(&self, headers: &[(&str, &str)], body: &Value) -> Result<(u16, Value), AssistError> {
        self.sent.lock().unwrap().push((headers.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), body.clone()));
        self.replies.lock().unwrap().pop_front().ok_or_else(|| AssistError::Http("no more scripted replies".to_owned()))
    }
}

fn tool_use(id: &str, name: &str, input: &Value) -> (u16, Value) {
    (
        200,
        json!({
            "stop_reason": "tool_use",
            "content": [
                { "type": "thinking", "thinking": "", "signature": "sig-1" },
                { "type": "tool_use", "id": id, "name": name, "input": input }
            ]
        }),
    )
}

fn done(text: &str) -> (u16, Value) {
    (200, json!({ "stop_reason": "end_turn", "content": [{ "type": "text", "text": text }] }))
}

struct Bench {
    dir: tempfile::TempDir,
    session: Session,
}

/// A fixture library with three made-up songs on its first three tracks.
fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let location = fixture::build(&lib, Shape::default()).unwrap();
    for i in 0..3 {
        let song = dir.path().join(format!("song{i}.wav"));
        testing::wav(&song, u8::try_from(i).unwrap()).unwrap();
        fixture::point_at_audio(&location, i, &song.display().to_string(), 30).unwrap();
    }
    let session = Session::open(&dir.path().join("app")).unwrap();
    session.open_db(location).unwrap();
    Bench { dir, session }
}

fn id(i: usize) -> String {
    fixture::track_id(i)
}

#[test]
fn an_instruction_runs_tools_writes_cues_and_says_what_it_did() {
    let b = bench();
    let transport = Scripted::new(vec![
        tool_use("t1", "track_detail", &json!({ "id": id(0) })),
        tool_use(
            "t2",
            "write_cues",
            &json!({ "summary": "Put C on bar 9", "edits": [{ "op": "add", "track": id(0), "slot": "C", "start_secs": 16.0, "name": "drop" }] }),
        ),
        done("Put C on bar 9, where the drop is."),
    ]);
    let context = Context { track: Some(id(0)), playlist: None };
    let result = quick::run(&b.session, &transport, KEY, Model::Sonnet, "put C on the drop", &context).unwrap();

    assert_eq!(result.reply, "Put C on bar 9, where the drop is.");
    assert_eq!(
        result.steps.iter().map(|s| (s.tool.as_str(), s.ok)).collect::<Vec<_>>(),
        vec![("track_detail", true), ("write_cues", true)]
    );
    assert_eq!(result.changed.iter().map(|r| r.hot.as_str()).collect::<Vec<_>>(), vec!["C"]);
    assert_eq!(b.session.history(1).unwrap()[0].summary, "Assistant: Put C on bar 9");

    let sent = transport.sent();
    assert_eq!(sent.len(), 3);
    let (headers, first) = &sent[0];
    assert!(headers.contains(&("x-api-key".to_owned(), KEY.to_owned())));
    assert!(headers.contains(&("anthropic-version".to_owned(), "2026-10-21".to_owned())));
    assert!(headers.contains(&("anthropic-beta".to_owned(), "server-side-fallback-2026-07-01".to_owned())));
    assert_eq!(first["model"], "claude-sonnet-5-5");
    assert_eq!(first["fallbacks"], "default");
    assert!(first["messages"][0]["content"].as_str().unwrap().contains(&format!("The selected track is {}", id(0))));
    let names: Vec<&str> = first["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"write_cues") && names.contains(&"undo") && !names.contains(&"export_xml"));
    assert!(first["tools"][0]["input_schema"].is_object(), "the API's tool shape");

    // The assistant's turn goes back whole, thinking block and all, and every
    // tool result for a turn is in one user message.
    let (_, second) = &sent[1];
    let messages = second["messages"].as_array().unwrap();
    assert_eq!(messages[1]["content"][0]["type"], "thinking");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(messages[2]["content"][0]["tool_use_id"], "t1");
    assert_eq!(messages[2]["content"][0]["is_error"], false);
}

#[test]
fn haiku_runs_without_the_fallback_it_does_not_have() {
    let b = bench();
    let transport = Scripted::new(vec![done("Nothing to do.")]);
    quick::run(&b.session, &transport, KEY, Model::Haiku, "anything", &Context::default()).unwrap();
    let (headers, body) = &transport.sent()[0];
    assert_eq!(body["model"], "claude-haiku-5-5");
    assert!(body.get("fallbacks").is_none());
    assert!(!headers.iter().any(|(k, _)| k == "anthropic-beta"));
}

#[test]
fn a_failing_tool_is_reported_to_the_model_and_the_run_goes_on() {
    let b = bench();
    let transport =
        Scripted::new(vec![tool_use("t1", "track_detail", &json!({ "id": "999999" })), done("That track isn't in the library.")]);
    let result = quick::run(&b.session, &transport, KEY, Model::Sonnet, "look at it", &Context::default()).unwrap();
    assert!(!result.steps[0].ok);
    let (_, second) = &transport.sent()[1];
    assert_eq!(second["messages"][2]["content"][0]["is_error"], true);
    assert!(second["messages"][2]["content"][0]["content"].as_str().unwrap().contains("no track 999999"));
}

#[test]
fn an_assistant_write_undoes_like_any_other() {
    let b = bench();
    let transport = Scripted::new(vec![
        tool_use("t1", "write_rule_cues", &json!({ "ids": [id(1)], "rules": "[[cue]]\nslot = \"A\"\nat = \"start\"\n" })),
        tool_use("t2", "undo", &json!({})),
        done("Wrote A, then undid it."),
    ]);
    let result = quick::run(&b.session, &transport, KEY, Model::Sonnet, "try and undo", &Context::default()).unwrap();
    assert!(result.steps.iter().all(|s| s.ok));
    let history: Vec<String> = b.session.history(3).unwrap().into_iter().map(|e| e.summary).collect();
    assert_eq!(history[0], "Undo: Assistant: Apply rules to 1 track");
    assert_eq!(history[1], "Assistant: Apply rules to 1 track");
    assert_eq!(result.changed.last().map(|r| r.hot.as_str()), Some(""));
}

#[test]
fn api_errors_refusals_and_runaways_come_back_as_errors() {
    let b = bench();
    let ctx = Context::default();
    let bad_key =
        Scripted::new(vec![(401, json!({ "type": "error", "error": { "type": "authentication_error", "message": "invalid x-api-key" } }))]);
    assert!(matches!(quick::run(&b.session, &bad_key, KEY, Model::Sonnet, "x", &ctx), Err(AssistError::BadKey(_))));

    let overloaded =
        Scripted::new(vec![(529, json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } }))]);
    assert!(matches!(quick::run(&b.session, &overloaded, KEY, Model::Sonnet, "x", &ctx), Err(AssistError::Api { status: 529, .. })));

    let refused = Scripted::new(vec![(200, json!({ "stop_reason": "refusal", "stop_details": { "category": "cyber" }, "content": [] }))]);
    assert!(matches!(quick::run(&b.session, &refused, KEY, Model::Sonnet, "x", &ctx), Err(AssistError::Refused(c)) if c == "cyber"));

    let looping = Scripted::new((0..40).map(|i| tool_use(&format!("t{i}"), "list_playlists", &json!({}))).collect());
    assert!(matches!(quick::run(&b.session, &looping, KEY, Model::Sonnet, "x", &ctx), Err(AssistError::TooManySteps(_))));

    assert!(matches!(quick::run(&b.session, &looping, KEY, Model::Sonnet, "   ", &ctx), Err(AssistError::Other(_))));
}

/// Every file under `dir`, read as bytes.
fn every_file(dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            every_file(&path, out);
        } else {
            out.push((path.display().to_string(), std::fs::read(&path).unwrap()));
        }
    }
}

#[test]
fn the_key_is_never_written_anywhere_on_disk() {
    let b = bench();
    let keys = MemoryKeys::default();
    keys.set(&check_key(&format!("  {KEY}\n")).unwrap()).unwrap();
    Settings { model: Model::Haiku }.save(&b.dir.path().join("app")).unwrap();
    let transport = Scripted::new(vec![
        tool_use(
            "t1",
            "write_cues",
            &json!({ "summary": "Set A", "edits": [{ "op": "add", "track": id(0), "slot": "A", "start_secs": 0.0 }] }),
        ),
        done("Done."),
    ]);
    let key = keys.get().unwrap().unwrap();
    quick::run(&b.session, &transport, &key, Model::Haiku, "set A", &Context::default()).unwrap();

    let mut files = Vec::new();
    every_file(b.dir.path(), &mut files);
    assert!(files.iter().any(|(p, _)| p.contains("/.git/")), "the history was written, so this checks it too");
    for (path, bytes) in &files {
        assert!(!bytes.windows(KEY.len()).any(|w| w == KEY.as_bytes()), "{path} holds the key");
    }
    assert_eq!(Settings::load(&b.dir.path().join("app")).model, Model::Haiku);
}

#[test]
fn keys_are_checked_and_removable() {
    assert!(matches!(check_key("   "), Err(AssistError::BadKey(_))));
    assert!(matches!(check_key("sk ant"), Err(AssistError::BadKey(_))));
    let keys = MemoryKeys::default();
    keys.set("sk-ant-x").unwrap();
    keys.delete().unwrap();
    assert_eq!(keys.get().unwrap(), None);
}

/// Sets A to D on the first two tracks, as someone cueing by hand would.
fn hand_cue(b: &Bench) {
    let mut edits = Vec::new();
    for track in 0..2 {
        for (slot, secs) in [("A", 0.0), ("B", 8.0), ("C", 16.0), ("D", 24.0)] {
            edits.push(CueEdit::Add {
                track: id(track),
                slot: slot.into(),
                start_secs: secs,
                end_secs: None,
                name: String::new(),
                color: None,
                replace: false,
            });
        }
    }
    b.session.save_cues("By hand", &edits).unwrap();
}

#[test]
fn the_wizard_reads_the_hand_cues_scores_its_drafts_and_keeps_the_best() {
    let b = bench();
    hand_cue(&b);
    let bad = "[[cue]]\nslot = \"Z\"\nat = \"start\"\n";
    // Only A lands: B, C and D need a drop and a groove a click track hasn't got.
    let start_only = "[[cue]]\nslot = \"A\"\nat = \"start\"\n";
    let transport = Scripted::new(vec![
        tool_use("t1", "hand_cued_tracks", &json!({})),
        tool_use("t2", "track_detail", &json!({ "id": id(0), "levels": true })),
        tool_use("t3", "submit_rules", &json!({ "rules": bad })),
        tool_use("t4", "submit_rules", &json!({ "rules": start_only })),
        done("A goes on the first downbeat. It matched your A on both tracks."),
    ]);
    let mut wizard = Wizard::default();
    let turn = wizard.turn(&b.session, &transport, KEY, Model::Sonnet, "A on the first beat, C on the drop").unwrap();

    assert_eq!(
        turn.steps.iter().map(|s| (s.tool.as_str(), s.ok)).collect::<Vec<_>>(),
        vec![("hand_cued_tracks", true), ("track_detail", true), ("submit_rules", false), ("submit_rules", true)]
    );
    let draft = turn.draft.unwrap();
    assert_eq!(draft.rules, start_only);
    assert_eq!(draft.score.tracks.len(), 2);
    let hits: Vec<(String, usize)> = draft.score.slots.iter().map(|s| (s.slot.clone(), s.hits)).collect();
    assert_eq!(hits, vec![("A".into(), 2), ("B".into(), 0), ("C".into(), 0), ("D".into(), 0)]);

    let sent = transport.sent();
    let hand_cued = sent[1].1["messages"][2]["content"][0]["content"].as_str().unwrap();
    assert!(hand_cued.contains(&id(0)) && hand_cued.contains("\"slot\":\"C\""));
    assert!(sent[3].1["messages"][6]["content"][0]["content"].as_str().unwrap().contains("don't parse"));
    assert!(sent[0].1["system"].as_str().unwrap().contains("[find]"), "the schema is in the prompt");

    // Refining sends the current draft with the guidance, in the same conversation.
    let more = Scripted::new(vec![done("Kept it as is.")]);
    wizard.turn(&b.session, &more, KEY, Model::Sonnet, "make B 8 bars before C").unwrap();
    let refine = &more.sent()[0].1["messages"];
    let last = refine.as_array().unwrap().last().unwrap()["content"].as_str().unwrap();
    assert!(last.starts_with("make B 8 bars before C") && last.contains(start_only));

    // A failed turn keeps the conversation as it was: the refining turn's
    // messages and its answer, and nothing from the failure.
    let before = refine.as_array().unwrap().len() + 1;
    let failing = Scripted::new(vec![(500, json!({ "error": { "message": "boom" } }))]);
    assert!(wizard.turn(&b.session, &failing, KEY, Model::Sonnet, "again").is_err());
    let retry = Scripted::new(vec![done("Fine.")]);
    wizard.turn(&b.session, &retry, KEY, Model::Sonnet, "again").unwrap();
    assert_eq!(retry.sent()[0].1["messages"].as_array().unwrap().len(), before + 1);
}

#[test]
fn saving_the_wizard_s_rules_keeps_the_previous_ones() {
    let b = bench();
    let first = "[[cue]]\nslot = \"A\"\nat = \"start\"\n";
    let second = "[[cue]]\nslot = \"B\"\nat = \"start\"\n";
    assert!(!b.session.has_saved_rules());
    b.session.save_rules_keeping_previous(first).unwrap();
    b.session.save_rules_keeping_previous(second).unwrap();
    assert_eq!(b.session.load_rules(), second);
    assert_eq!(b.session.restore_previous_rules().unwrap(), first);
    assert_eq!(b.session.load_rules(), first);
    assert!(b.session.save_rules_keeping_previous("slot = [").is_err());
    assert_eq!(b.session.load_rules(), first, "bad rules aren't saved");
}
