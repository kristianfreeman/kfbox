//! The page's commands, called through Tauri's IPC with a mock runtime, so
//! every argument name and type is checked without a window.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kfbox_assist::client::{AssistError, Transport};
use kfbox_assist::keys::MemoryKeys;
use kfbox_core::session::Session;
use kfbox_lib::assistant::Assistant;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow};

fn app(dir: &std::path::Path) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    // The app's own configuration, so its capabilities decide what the page may call.
    let app = kfbox_lib::commands(mock_builder()).build(kfbox_lib::context()).unwrap();
    app.manage(Session::open(&dir.join("app")).unwrap());
    app.manage(Assistant::new(dir.join("app"), Box::new(MemoryKeys::default()), Box::new(Answers)));
    let window = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default()).build().unwrap();
    (app, window)
}

/// The API, standing in: every request gets the same short answer.
struct Answers;

impl Transport for Answers {
    fn post(&self, headers: &[(&str, &str)], _body: &Value) -> Result<(u16, Value), AssistError> {
        let key = headers.iter().find(|(name, _)| *name == "x-api-key").map(|(_, v)| *v).unwrap_or_default();
        Ok((
            200,
            json!({ "stop_reason": "end_turn", "content": [{ "type": "text", "text": format!("ok, key of {} characters", key.len()) }] }),
        ))
    }
}

/// Calls a command as the page does, with camelCase arguments.
fn call(window: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // The page is served from the app itself, as `tauri://localhost` on macOS.
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize::<Value>().unwrap())
}

#[test]
fn the_page_s_reading_commands_answer_as_it_expects() {
    let dir = tempfile::tempdir().unwrap();
    let xml = kfbox_core::testing::collection(dir.path()).unwrap();
    let (_app, window) = app(dir.path());

    assert!(call(&window, "default_rules", json!({})).unwrap().as_str().unwrap().contains("[[cue]]"));
    assert!(call(&window, "load_rules", json!({})).unwrap().as_str().unwrap().contains("[[cue]]"));
    let listing = call(&window, "open_rekordbox", json!({ "path": xml })).unwrap();
    assert_eq!(listing["total"], 3);
    let rows = call(&window, "load_playlist", json!({ "index": null })).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert_eq!(rows[0]["title"], "Alpha");

    let analysis = call(&window, "analyse_rekordbox", json!({ "id": "1" })).unwrap();
    let path = analysis["path"].as_str().unwrap().to_owned();
    assert_eq!(analysis["grid"], "rekordbox");
    let detail = call(&window, "track_detail", json!({ "path": path })).unwrap();
    assert!(detail["bars"].as_array().unwrap().len() > 10);
    let rules = call(&window, "default_rules", json!({})).unwrap();
    let applied = call(&window, "apply_rules_to", json!({ "path": path, "rules": rules })).unwrap();
    assert_eq!(applied["cues"][0]["existing"], true);
    let all = call(&window, "apply_rules", json!({ "rules": rules, "save": false })).unwrap();
    assert_eq!(all.as_array().unwrap().len(), 1);
    let columns = call(&window, "waveform", json!({ "path": path })).unwrap();
    assert!(columns.as_array().unwrap().len() > 100);

    let out = dir.path().join("out.xml");
    assert_eq!(call(&window, "export_xml", json!({ "dest": out, "rules": rules })).unwrap(), 1);
    assert!(out.exists());
    assert!(call(&window, "track_detail", json!({ "path": "/not/loaded.wav" })).is_err());

    // An error comes back as text the page can show.
    let refused = call(&window, "apply_rules", json!({ "rules": "[[cue]]\nslot = \"Z\"\nat = \"start\"\n", "save": true })).unwrap_err();
    assert!(refused.as_str().unwrap().contains("A to P"));
}

#[test]
fn the_page_s_writing_commands_save_undo_and_record() {
    use rbl_db::fixture::{self, Shape};
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let location = fixture::build(&lib, Shape::default()).unwrap();
    let song = dir.path().join("song.wav");
    kfbox_core::testing::wav(&song, 1).unwrap();
    fixture::point_at_audio(&location, 0, &song.display().to_string(), 30).unwrap();
    let (app, window) = app(dir.path());
    app.state::<Session>().open_db(location).unwrap();
    let track = fixture::track_id(0);

    let edit = json!({ "op": "add", "track": track, "slot": "A", "start_secs": 1.5, "name": "start", "color": "#00ff00" });
    let saved = call(&window, "save_cues", json!({ "summary": "Set A", "edits": [edit] })).unwrap();
    assert_eq!(saved["changed"][0]["hot"], "A");
    let rules = "[[cue]]\nslot = \"memory\"\nat = \"start\"\noffset_bars = 2\n";
    let applied = call(&window, "write_rule_cues", json!({ "ids": [track], "rules": rules })).unwrap();
    assert_eq!(applied["added"].as_array().unwrap().len(), 1);
    let history = call(&window, "history", json!({ "limit": 5 })).unwrap();
    assert_eq!(history[0]["summary"], "Apply rules to 1 track");
    assert_eq!(history[0]["kind"], "action");

    call(&window, "undo", json!({})).unwrap();
    call(&window, "redo", json!({})).unwrap();
    let first = call(&window, "history", json!({ "limit": 50 })).unwrap().as_array().unwrap().last().unwrap()["commit"].clone();
    let restored = call(&window, "restore", json!({ "commit": first })).unwrap();
    assert_eq!(restored["changed"][0]["hot"], "");
}

#[test]
fn the_assistant_s_commands_never_hand_the_key_back() {
    let dir = tempfile::tempdir().unwrap();
    let xml = kfbox_core::testing::collection(dir.path()).unwrap();
    let (_app, window) = app(dir.path());
    call(&window, "open_rekordbox", json!({ "path": xml })).unwrap();
    let key = "sk-ant-test-NOT-A-REAL-KEY";

    let fresh = call(&window, "assistant_settings", json!({})).unwrap();
    assert_eq!((fresh["model"].as_str(), fresh["key_saved"].as_bool()), (Some("claude-sonnet-5-5"), Some(false)));
    assert!(call(&window, "run_quick", json!({ "instruction": "hi", "track": null, "playlist": null }))
        .unwrap_err()
        .as_str()
        .unwrap()
        .contains("no API key"));

    let saved = call(&window, "save_api_key", json!({ "key": key })).unwrap();
    assert_eq!(saved["key_saved"], true);
    assert!(!saved.to_string().contains(key), "the page never gets the key back");
    let haiku = call(&window, "set_assistant_model", json!({ "model": "claude-haiku-5-5" })).unwrap();
    assert_eq!(haiku["model"], "claude-haiku-5-5");
    assert!(call(&window, "set_assistant_model", json!({ "model": "gpt-9" })).is_err());

    let quick = call(&window, "run_quick", json!({ "instruction": "hi", "track": "1", "playlist": null })).unwrap();
    assert_eq!(quick["reply"], format!("ok, key of {} characters", key.len()), "the key reached the request");
    assert!(!quick.to_string().contains(key));
    let wizard = call(&window, "wizard_turn", json!({ "said": "A on the first beat" })).unwrap();
    assert!(wizard["draft"].is_null());
    call(&window, "wizard_reset", json!({})).unwrap();

    assert_eq!(call(&window, "has_saved_rules", json!({})).unwrap(), false);
    call(&window, "save_rules_keeping_previous", json!({ "rules": "[[cue]]\nslot = \"A\"\nat = \"start\"\n" })).unwrap();
    assert_eq!(call(&window, "has_saved_rules", json!({})).unwrap(), true);
    let previous = call(&window, "restore_previous_rules", json!({})).unwrap();
    assert!(previous.as_str().unwrap().contains("[find]"), "back to the defaults");
    let score = call(&window, "score_rules", json!({ "rules": previous })).unwrap();
    assert_eq!(score["tracks"].as_array().unwrap().len(), 0, "the made-up collection has no hand-cued tracks");

    let removed = call(&window, "remove_api_key", json!({})).unwrap();
    assert_eq!(removed["key_saved"], false);
}

#[test]
fn a_write_from_another_process_reaches_the_page_as_one_event() {
    use std::sync::{Arc, Mutex};
    use tauri::Listener;

    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("lib");
    std::fs::create_dir_all(&library).unwrap();
    rbl_db::fixture::build(&library, rbl_db::fixture::Shape::default()).unwrap();
    let (app, _window) = app(dir.path());
    app.state::<Session>().open_db(kfbox_core::testing::fixture_location(&library)).unwrap();

    let heard: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
    for event in ["library-changed", "history-changed"] {
        let heard = Arc::clone(&heard);
        app.listen(event, move |e| heard.lock().unwrap().push((event.to_owned(), serde_json::from_str(e.payload()).unwrap())));
    }
    let handle = app.handle().clone();
    let mut history = app.state::<Session>().history_marker();
    assert!(!kfbox_lib::watch::check(&handle, &mut history), "nothing to report yet");

    // Another process, such as kfbox-mcp, sharing the data folder.
    let agent = Session::open(&dir.path().join("app")).unwrap();
    agent.open_db(kfbox_core::testing::fixture_location(&library)).unwrap();
    let edit = kfbox_core::session::CueEdit::Add {
        track: rbl_db::fixture::track_id(2),
        slot: "C".into(),
        start_secs: 3.0,
        end_secs: None,
        name: String::new(),
        color: None,
        replace: false,
    };
    agent.save_cues("Agent: Set C", &[edit]).unwrap();

    assert!(kfbox_lib::watch::check(&handle, &mut history));
    assert!(!kfbox_lib::watch::check(&handle, &mut history), "reported once");
    let heard = heard.lock().unwrap();
    assert_eq!(heard.len(), 1);
    let (event, payload) = &heard[0];
    assert_eq!(event, "library-changed");
    assert_eq!(payload["changed"][0]["id"], json!(rbl_db::fixture::track_id(2)));
    assert_eq!(payload["changed"][0]["hot"], "C");
    assert_eq!(payload["listing"]["total"], 40);
}

#[test]
fn the_page_imports_a_dropped_song_and_finds_a_missing_one() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("lib");
    std::fs::create_dir_all(&library).unwrap();
    rbl_db::fixture::build(&library, rbl_db::fixture::Shape { tracks: 3, ..rbl_db::fixture::Shape::default() }).unwrap();
    let (app, window) = app(dir.path());
    app.state::<Session>().open_db(kfbox_core::testing::fixture_location(&library)).unwrap();
    let song = dir.path().join("music/dropped.wav");
    std::fs::create_dir_all(song.parent().unwrap()).unwrap();
    kfbox_core::testing::wav(&song, 4).unwrap();

    let report = call(&window, "import_files", json!({ "paths": [dir.path().join("music")] })).unwrap();
    assert_eq!(report["imported"].as_array().unwrap().len(), 1, "{report}");
    assert_eq!(report["imported"][0]["path"], json!(song));
    assert_eq!(report["imported"][0]["missing"], false);

    std::fs::create_dir_all(dir.path().join("moved")).unwrap();
    std::fs::rename(&song, dir.path().join("moved/dropped.wav")).unwrap();
    let found = call(&window, "relocate_missing", json!({ "folder": dir.path().join("moved") })).unwrap();
    assert!(found["commit"].is_string(), "{found}");
    let row = found["changed"].as_array().unwrap().iter().find(|r| r["title"] == report["imported"][0]["title"]).unwrap().clone();
    assert_eq!(row["missing"], false);
}

#[test]
fn the_page_lists_drives_and_exports_a_playlist_to_one_with_progress() {
    use std::sync::{Arc, Mutex};
    use tauri::Listener;

    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("lib");
    std::fs::create_dir_all(&library).unwrap();
    rbl_db::fixture::build(&library, rbl_db::fixture::Shape { tracks: 2, ..rbl_db::fixture::Shape::default() }).unwrap();
    let (app, window) = app(dir.path());
    let session = app.state::<Session>();
    session.open_db(kfbox_core::testing::fixture_location(&library)).unwrap();
    let song = dir.path().join("music/gig.wav");
    std::fs::create_dir_all(song.parent().unwrap()).unwrap();
    kfbox_core::testing::wav(&song, 5).unwrap();
    let id = session.import_files(std::slice::from_ref(&song)).unwrap().imported.remove(0).id;
    session.create_playlist("Gig", None, &[id]).unwrap();
    let index = session.fresh_listing().unwrap().playlists.iter().find(|p| p.path == "Gig").unwrap().index;

    // A folder standing in for a stick: never a real drive in a test.
    let stick = dir.path().join("FAKE STICK");
    std::fs::create_dir_all(&stick).unwrap();
    std::env::set_var(rbl_devices::FAKE_VOLUMES, &stick);
    let drives = call(&window, "usb_drives", json!({})).unwrap();
    assert_eq!(drives[0]["path"], json!(stick));

    let heard: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = Arc::clone(&heard);
    app.listen("usb-progress", move |e| sink.lock().unwrap().push(serde_json::from_str(e.payload()).unwrap()));
    let report = call(&window, "export_usb", json!({ "destination": stick, "playlists": [index], "fileSystem": null })).unwrap();
    assert_eq!((report["tracks"].as_u64(), report["playlists"].as_u64()), (Some(1), Some(1)), "{report}");
    assert!(stick.join("PIONEER/rekordbox/export.pdb").is_file());
    assert!(!heard.lock().unwrap().is_empty(), "progress reached the page");
    call(&window, "cancel_usb", json!({})).unwrap();
}
