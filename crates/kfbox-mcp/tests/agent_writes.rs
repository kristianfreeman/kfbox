//! An agent writing through the real `kfbox-mcp` binary while the app has the
//! same library open, as two processes do on Kristian's Mac. Fixture libraries
//! in temp folders only.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use kfbox_core::safe::SafeLibrary;
use kfbox_core::session::{CueEdit, Session};
use kfbox_core::testing;
use rbl_db::fixture::{self, Shape};
use serde_json::{json, Value};

struct Agent {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Agent {
    fn start(app_dir: &Path, library: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kfbox-mcp"))
            .args(["--fixture", &library.display().to_string()])
            .env("KFBOX_DIR", app_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut agent = Self { child, input, output, next: 1 };
        agent.request(
            "initialize",
            &json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }),
        );
        agent
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(self.input, "{}", json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn tool(&mut self, name: &str, arguments: &Value) -> Result<Value, String> {
        let reply = self.request("tools/call", &json!({ "name": name, "arguments": arguments }));
        let text = reply["result"]["content"][0]["text"].as_str().unwrap().to_owned();
        if reply["result"]["isError"] == json!(true) {
            Err(text)
        } else {
            Ok(serde_json::from_str(&text).unwrap())
        }
    }

    fn add_cue(&mut self, track: usize, slot: &str, secs: f64) -> Result<Value, String> {
        let edit = json!({ "op": "add", "track": fixture::track_id(track), "slot": slot, "start_secs": secs, "name": "from agent" });
        self.tool("write_cues", &json!({ "summary": format!("Set {slot}"), "edits": [edit] }))
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A fixture library, the app's session on it, and the shared data folder.
struct Bench {
    dir: tempfile::TempDir,
    app: Session,
}

impl Bench {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("lib");
        std::fs::create_dir_all(&library).unwrap();
        let location = fixture::build(&library, Shape::default()).unwrap();
        let song = dir.path().join("song.wav");
        testing::wav(&song, 1).unwrap();
        fixture::point_at_audio(&location, 0, &song.display().to_string(), 30).unwrap();
        let app = Session::open(&dir.path().join("app")).unwrap();
        app.open_db(testing::fixture_location(&library)).unwrap();
        Self { dir, app }
    }

    fn library(&self) -> std::path::PathBuf {
        self.dir.path().join("lib")
    }

    fn agent(&self) -> Agent {
        Agent::start(&self.dir.path().join("app"), &self.library())
    }

    fn hot(&self, track: usize) -> String {
        self.app.search("", 100).unwrap().into_iter().find(|r| r.id == fixture::track_id(track)).unwrap().hot
    }
}

#[test]
fn the_open_app_sees_an_agents_write_and_its_undo() {
    let bench = Bench::new();
    let mut agent = bench.agent();
    assert!(bench.app.refresh_if_changed().unwrap().is_none(), "nothing has changed yet");

    agent.add_cue(0, "B", 4.0).unwrap();
    let change = bench.app.refresh_if_changed().unwrap().expect("the app notices the agent's write");
    assert_eq!(change.changed.iter().map(|r| (r.id.clone(), r.hot.clone())).collect::<Vec<_>>(), vec![(fixture::track_id(0), "B".into())]);
    assert_eq!(change.removed, Vec::<String>::new());
    assert_eq!(bench.hot(0), "B");
    assert!(bench.app.refresh_if_changed().unwrap().is_none(), "one change is reported once");

    let history = bench.app.history(5).unwrap();
    assert_eq!(history[0].summary, "Agent: Set B", "History says an agent made it");

    agent.tool("undo", &json!({})).unwrap();
    let change = bench.app.refresh_if_changed().unwrap().unwrap();
    assert_eq!(change.changed[0].hot, "");
    assert_eq!(bench.app.history(1).unwrap()[0].summary, "Undo: Agent: Set B");
}

#[test]
fn the_agent_sees_the_apps_write_on_its_next_call() {
    let bench = Bench::new();
    let mut agent = bench.agent();
    let id = fixture::track_id(0);
    let edit = CueEdit::Add {
        track: id.clone(),
        slot: "D".into(),
        start_secs: 2.0,
        end_secs: None,
        name: String::new(),
        color: None,
        replace: false,
    };
    bench.app.save_cues("Set D", &[edit]).unwrap();
    let found = agent.tool("search_tracks", &json!({ "query": "" , "limit": 500 })).unwrap();
    let row = found.as_array().unwrap().iter().find(|r| r["id"] == json!(id)).unwrap();
    assert_eq!(row["hot"], "D");
    // The agent's slot is checked against the library as it is now, not as
    // the agent first read it.
    let taken = agent.add_cue(0, "D", 9.0).unwrap_err();
    assert!(taken.contains("taken"), "{taken}");
}

#[test]
fn a_writer_holding_the_library_makes_the_other_wait_then_give_up() {
    let bench = Bench::new();
    let mut agent = bench.agent();
    let root = bench.dir.path().join("app/safe");
    let held = SafeLibrary::open(testing::fixture_location(&bench.library()), &root).unwrap();
    let started = Instant::now();
    let busy = agent.add_cue(0, "A", 1.0).unwrap_err();
    assert!(busy.contains("another kfbox is writing"), "{busy}");
    assert!(started.elapsed() >= Duration::from_secs(14), "it waited for the lock before giving up");
    drop(held);
    agent.add_cue(0, "A", 1.0).unwrap();
}

#[test]
fn the_app_and_an_agent_writing_at_once_both_land() {
    let bench = Bench::new();
    let mut agent = bench.agent();
    let id = fixture::track_id(0);
    std::thread::scope(|scope| {
        let app = scope.spawn(|| {
            let edits: Vec<CueEdit> = ["C", "E", "G"]
                .iter()
                .enumerate()
                .map(|(i, slot)| CueEdit::Add {
                    track: id.clone(),
                    slot: (*slot).into(),
                    start_secs: 1.0 + f64::from(u8::try_from(i).unwrap()),
                    end_secs: None,
                    name: String::new(),
                    color: None,
                    replace: false,
                })
                .collect();
            for edit in edits {
                bench.app.save_cues("App", &[edit]).unwrap();
            }
        });
        for (i, slot) in ["B", "D", "F"].iter().enumerate() {
            agent.add_cue(0, slot, 10.0 + f64::from(u8::try_from(i).unwrap())).unwrap();
        }
        app.join().unwrap();
    });
    bench.app.refresh_if_changed().unwrap();
    let mut hot: Vec<char> = bench.hot(0).chars().collect();
    hot.sort_unstable();
    assert_eq!(hot.into_iter().collect::<String>(), "BCDEFG", "neither writer lost the other's cues");
    let actions = bench.app.history(20).unwrap().iter().filter(|e| e.summary == "App" || e.summary.starts_with("Agent:")).count();
    assert_eq!(actions, 6);
}
