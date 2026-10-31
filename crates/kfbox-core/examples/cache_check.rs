//! Times loading rekordbox's whole collection twice through a temporary
//! cache, then the list alone. It only reads the library:
//!
//! ```sh
//! cargo run --release -p kfbox-core --example cache_check
//! ```

#![allow(clippy::print_stdout, clippy::unwrap_used, reason = "a command-line check")]

use std::sync::Mutex;
use std::time::Instant;

use kfbox_core::cache::{from_rekordbox, Cache};

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kfbox.db");
    for pass in ["first", "second"] {
        let cache = Mutex::new(Cache::open(&path).unwrap());
        let timer = Instant::now();
        let library = kfbox_core::master::load_installed().unwrap();
        let tracks = library.all_tracks();
        println!("{pass}: list of {} tracks in {:?}", tracks.len(), timer.elapsed());
        let timer = Instant::now();
        let (mut decoded, mut cached, mut failed) = (0, 0, 0);
        for source in tracks {
            match from_rekordbox(&cache, source) {
                Ok((_, true)) => cached += 1,
                Ok((_, false)) => decoded += 1,
                Err(_) => failed += 1,
            }
        }
        println!("{pass}: {decoded} decoded, {cached} from the cache, {failed} failed, in {:?}", timer.elapsed());
    }
    println!("cache file: {} MB", std::fs::metadata(&path).unwrap().len() / 1_000_000);
}
