//! `kfbox-mcp`: kfbox for an agent, over MCP on stdin and stdout.
//!
//! It reads rekordbox's library from a copy, as the app does, and shares the
//! app's analysis cache and saved rules. Nothing it does writes to the
//! library yet: the tools search, read, propose cues as a dry run, and export
//! a rekordbox XML to a file the agent names.
//!
//! ```sh
//! kfbox-mcp                      # rekordbox's own library
//! kfbox-mcp --xml collection.xml # a rekordbox XML instead
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
    let xml = args.iter().position(|a| a == "--xml").and_then(|i| args.get(i + 1)).map(PathBuf::from);
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
