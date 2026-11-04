//! `kfbox-mcp`: kfbox for an agent, over MCP on stdin and stdout.
//!
//! It reads rekordbox's library from a copy, as the app does, and shares the
//! app's analysis cache, saved rules and history. The tools search, read,
//! propose cues as a dry run, export a rekordbox XML to a file the agent
//! names, and write cues and undo through the same safe path as the app,
//! which refuses while rekordbox is open. An open kfbox window sees an
//! agent's writes within a second or two.
//!
//! ```sh
//! kfbox-mcp                         # rekordbox's own library
//! kfbox-mcp --xml collection.xml    # a rekordbox XML instead, read only
//! kfbox-mcp --fixture path/to/lib   # a test library from rbl-db's fixtures
//! ```
//!
//! `KFBOX_DIR` replaces the app's data folder, which holds the cache and rules.

mod server;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use kfbox_core::session::Session;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let xml = value("--xml");
    let Some(dir) = Session::default_dir() else {
        eprintln!("kfbox-mcp: no data folder, and no KFBOX_DIR");
        return ExitCode::FAILURE;
    };
    let session = match Session::open(&dir) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("kfbox-mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Only opens a library made by rbl-db's fixture builder: its passphrase
    // can't open a real one, so this can't reach around the safety checks.
    if let Some(dir) = value("--fixture") {
        if let Err(e) = session.open_db(kfbox_core::testing::fixture_location(&dir)) {
            eprintln!("kfbox-mcp: {e}");
            return ExitCode::FAILURE;
        }
    }
    let mut server = server::Server::new(session, xml);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            // stdout carries the protocol and nothing else.
            if writeln!(stdout, "{reply}").and_then(|()| stdout.flush()).is_err() {
                break;
            }
        }
    }
    ExitCode::SUCCESS
}
