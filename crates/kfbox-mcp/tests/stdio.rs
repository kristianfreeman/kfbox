//! The real binary, spoken to over stdin and stdout, on a made-up collection.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(dir: &std::path::Path, xml: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kfbox-mcp"))
            .args(["--xml", &xml.display().to_string()])
            .env("KFBOX_DIR", dir.join("app"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self { child, input, output, next: 1 }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], json!(id));
        reply
    }

    /// Calls a tool and returns its JSON answer, or its error text.
    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": arguments }));
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap().to_owned();
        if result["isError"] == json!(true) {
            Err(text)
        } else {
            Ok(serde_json::from_str(&text).unwrap())
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn client() -> (tempfile::TempDir, std::path::PathBuf, Client) {
    let dir = tempfile::tempdir().unwrap();
    let xml = kfbox_core::testing::collection(dir.path()).unwrap();
    let mut client = Client::start(dir.path(), &xml);
    let init = client.request("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "kfbox");
    client.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    (dir, xml, client)
}

#[test]
fn it_lists_six_tools_all_read_only_but_the_export() {
    let (_dir, _xml, mut client) = client();
    let tools = client.request("tools/list", json!({}))["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["list_playlists", "search_tracks", "playlist_tracks", "track_detail", "propose_cues", "export_xml"]);
    for tool in &tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], json!(tool["name"] != "export_xml"), "{}", tool["name"]);
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

#[test]
fn an_agent_finds_reads_and_proposes_without_changing_anything() {
    let (dir, xml, mut client) = client();
    let before = std::fs::read(&xml).unwrap();

    let listing = client.tool("list_playlists", json!({})).unwrap();
    assert_eq!(listing["total"], 3);

    let found = client.tool("search_tracks", json!({ "query": "second" })).unwrap();
    let titles: Vec<&str> = found.as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Beta", "Gamma"]);
    let in_playlist = client.tool("search_tracks", json!({ "query": "", "playlist": "warm up", "limit": 1 })).unwrap();
    assert_eq!(in_playlist[0]["title"], "Beta");

    let warm = client.tool("playlist_tracks", json!({ "playlist": "Sets › Warm up" })).unwrap();
    assert_eq!(warm.as_array().unwrap().len(), 2);

    let detail = client.tool("track_detail", json!({ "id": "1", "levels": true })).unwrap();
    assert_eq!(detail["row"]["title"], "Alpha");
    assert_eq!(detail["cues"][0]["slot"], "A");
    assert!(detail["levels"].as_array().unwrap().len() > 10);

    let rules = "[[cue]]\nslot = \"memory\"\nat = \"start\"\noffset_bars = 4\n";
    let proposal = client.tool("propose_cues", json!({ "id": "1", "rules": rules })).unwrap();
    let new: Vec<&Value> = proposal["cues"].as_array().unwrap().iter().filter(|c| c["existing"] == json!(false)).collect();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0]["bar"], 5);

    assert_eq!(std::fs::read(&xml).unwrap(), before, "nothing in the library changed");
    assert!(!dir.path().join("app/rules.toml").exists(), "trying rules doesn't save them");
}

#[test]
fn an_export_writes_only_where_asked() {
    let (dir, _xml, mut client) = client();
    let out = dir.path().join("for-rekordbox.xml");
    let done = client.tool("export_xml", json!({ "path": out, "playlist": "Warm up", "ids": ["3"] })).unwrap();
    assert_eq!(done["tracks"], 3);
    assert!(std::fs::read_to_string(&out).unwrap().contains("DJ_PLAYLISTS"));
    let again = client.tool("export_xml", json!({ "path": out, "ids": ["3"] }));
    assert!(again.unwrap_err().contains("exists"));
    assert!(client.tool("export_xml", json!({ "path": out })).unwrap_err().contains("give track IDs"));
}

#[test]
fn mistakes_come_back_as_errors_not_crashes() {
    let (_dir, _xml, mut client) = client();
    assert!(client.tool("track_detail", json!({ "id": "99" })).unwrap_err().contains("no track 99"));
    assert!(client.tool("track_detail", json!({})).unwrap_err().contains("id is required"));
    assert!(client.tool("playlist_tracks", json!({ "playlist": "Nope" })).unwrap_err().contains("no playlist"));
    assert!(client.tool("propose_cues", json!({ "id": "1", "rules": "slot = [" })).is_err());
    assert_eq!(client.request("tools/call", json!({ "name": "write_cues", "arguments": {} }))["error"]["code"], -32602);
    assert_eq!(client.request("resources/list", json!({}))["error"]["code"], -32601);
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    // Still answering after all that.
    assert_eq!(client.tool("list_playlists", json!({})).unwrap()["total"], 3);
}

#[test]
fn an_edit_to_the_library_shows_on_the_next_call() {
    let (_dir, xml, mut client) = client();
    assert_eq!(client.tool("search_tracks", json!({ "query": "renamed" })).unwrap().as_array().unwrap().len(), 0);
    let text = std::fs::read_to_string(&xml).unwrap().replace("Name=\"Gamma\"", "Name=\"Renamed\"");
    std::fs::write(&xml, text).unwrap();
    assert_eq!(client.tool("search_tracks", json!({ "query": "renamed" })).unwrap().as_array().unwrap().len(), 1);
}
