//! Real playback: rbl-deck's engine, one deck of it, and the tick the page
//! reads it through.
//!
//! The engine opens the audio device when something is first loaded, not at
//! launch. Position comes as one event ten times a second, and the page
//! extrapolates between ticks.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rbl_deck::{Deck, DeckEvent, Engine, Snapshot};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

const TICK: Duration = Duration::from_millis(100);
const DECK: Deck = Deck::A;

#[derive(Default)]
pub struct Player {
    engine: Mutex<Option<Arc<Engine>>>,
    ticking: AtomicBool,
    next_load: AtomicU64,
}

/// One tick, in the terms the page needs.
#[allow(clippy::struct_excessive_bools, reason = "the deck's flags, sent together")]
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Tick {
    frames: u64,
    total_frames: u64,
    sample_rate: u32,
    /// Bumped on every load and seek, so the page snaps its playhead.
    generation: u32,
    playing: bool,
    loaded: bool,
    load_id: u64,
    looping: bool,
    /// Stopped because the track ran out, rather than paused.
    ended: bool,
}

impl Tick {
    fn of(snapshot: &Snapshot) -> Self {
        let deck = snapshot.a;
        Self {
            frames: deck.position_frames,
            total_frames: deck.total_frames,
            sample_rate: deck.sample_rate,
            generation: deck.generation,
            playing: deck.playing,
            loaded: deck.loaded,
            load_id: deck.load_id,
            looping: deck.looping,
            ended: deck.played_out,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct PlayerError {
    load_id: u64,
    message: String,
}

impl Player {
    fn engine<R: Runtime>(&self, app: &AppHandle<R>) -> Result<Arc<Engine>, String> {
        let mut held = self.engine.lock().map_err(|e| e.to_string())?;
        if let Some(engine) = held.as_ref() {
            return Ok(Arc::clone(engine));
        }
        let handle = app.clone();
        let events: rbl_deck::EventSink = Arc::new(move |event: DeckEvent| {
            if let DeckEvent::Error { load_id, message, .. } = event {
                let _ = handle.emit("player-error", PlayerError { load_id, message });
            }
        });
        let engine = Arc::new(Engine::new(&events).map_err(|e| format!("The audio device could not be opened: {e}"))?);
        *held = Some(Arc::clone(&engine));
        drop(held);
        self.start_ticking(app, &engine);
        Ok(engine)
    }

    fn start_ticking<R: Runtime>(&self, app: &AppHandle<R>, engine: &Arc<Engine>) {
        if self.ticking.swap(true, Ordering::SeqCst) {
            return;
        }
        let app = app.clone();
        let engine = Arc::clone(engine);
        std::thread::spawn(move || {
            let mut last = None;
            loop {
                let tick = Tick::of(&engine.snapshot());
                // Quiet while nothing changes, as when paused.
                if last != Some(tick) {
                    let _ = app.emit("player-tick", tick);
                    last = Some(tick);
                }
                std::thread::sleep(TICK);
            }
        });
    }

    /// The engine if something has opened it: controls have nothing to do before that.
    fn opened(&self) -> Option<Arc<Engine>> {
        self.engine.lock().ok()?.as_ref().map(Arc::clone)
    }
}

/// Loads a file. The tick says when it's ready, with the ID this returns.
#[tauri::command]
pub fn player_load<R: Runtime>(path: String, app: AppHandle<R>) -> Result<u64, String> {
    let player = app.state::<Player>();
    let engine = player.engine(&app)?;
    let load_id = player.next_load.fetch_add(1, Ordering::SeqCst) + 1;
    engine.load_as(DECK, Path::new(&path), load_id);
    Ok(load_id)
}

#[tauri::command]
pub fn player_play(player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.play(DECK);
    }
}

#[tauri::command]
pub fn player_pause(player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.pause(DECK);
    }
}

#[tauri::command]
pub fn player_seek(secs: f64, player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.seek_ms(DECK, secs * 1000.0);
    }
}

/// Loops from `from` to `to` seconds, starting now.
#[tauri::command]
pub fn player_set_loop(from: f64, to: f64, player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.set_loop_ms(DECK, from * 1000.0, to * 1000.0);
        engine.set_looping(DECK, true);
    }
}

#[tauri::command]
pub fn player_clear_loop(player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.clear_loop(DECK);
    }
}

#[tauri::command]
pub fn player_scrub_begin(player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.scrub_begin(DECK);
    }
}

#[tauri::command]
pub fn player_scrub_to(secs: f64, player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.scrub_to_ms(DECK, secs * 1000.0);
    }
}

#[tauri::command]
pub fn player_scrub_end(player: State<'_, Player>) {
    if let Some(engine) = player.opened() {
        engine.scrub_end(DECK);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use rbl_deck::{NullSink, Sink};
    use std::time::Instant;

    const RATE: u32 = 44_100;

    /// One second of a tone at half scale, as a 16-bit mono WAV.
    fn wav(path: &Path) {
        let samples: Vec<i16> = (0..RATE).map(|i| if (i / 50) % 2 == 0 { 16000 } else { -16000 }).collect();
        let bytes = u32::try_from(samples.len() * 2).unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + bytes).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2_u16.to_le_bytes());
        out.extend_from_slice(&16_u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&bytes.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, out).unwrap();
    }

    /// Pulls audio by hand until `done`, returning the loudest sample heard.
    fn pull(sink: &NullSink, engine: &Engine, done: impl Fn(&Tick) -> bool) -> f32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut loudest = 0.0_f32;
        loop {
            let block = sink.pull(1024);
            loudest = block.iter().fold(loudest, |m, s| m.max(s.abs()));
            if done(&Tick::of(&engine.snapshot())) {
                return loudest;
            }
            assert!(Instant::now() < deadline, "timed out at {:?}", Tick::of(&engine.snapshot()));
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The whole path a track takes, with a sink in place of the audio device.
    #[test]
    fn a_file_loads_sounds_seeks_loops_and_reports_its_end_in_ticks() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("tone.wav");
        wav(&song);
        let slot: Arc<Mutex<Option<Arc<NullSink>>>> = Arc::default();
        let held = Arc::clone(&slot);
        let events: rbl_deck::EventSink = Arc::new(|_| {});
        let engine = Engine::with_sink(
            move |render| {
                let sink = Arc::new(NullSink::new(RATE, render));
                *held.lock().unwrap() = Some(Arc::clone(&sink));
                Ok(sink as Arc<dyn Sink>)
            },
            &events,
        )
        .unwrap();
        let sink = slot.lock().unwrap().clone().unwrap();

        engine.load_as(DECK, &song, 7);
        pull(&sink, &engine, |t| t.loaded && t.load_id == 7);
        let ready = Tick::of(&engine.snapshot());
        assert_eq!(ready.total_frames, u64::from(RATE));
        assert!(!ready.playing);

        engine.play(DECK);
        let loudest = pull(&sink, &engine, |t| t.frames > u64::from(RATE) / 10);
        assert!(loudest > 0.1, "playing makes sound: {loudest}");
        assert!(Tick::of(&engine.snapshot()).playing);

        let before = Tick::of(&engine.snapshot()).generation;
        engine.seek_ms(DECK, 500.0);
        pull(&sink, &engine, |t| t.generation != before && t.frames >= u64::from(RATE) / 2);

        engine.set_loop_ms(DECK, 100.0, 200.0);
        engine.set_looping(DECK, true);
        assert!(Tick::of(&engine.snapshot()).looping);
        engine.clear_loop(DECK);

        engine.pause(DECK);
        pull(&sink, &engine, |t| !t.playing);

        engine.seek_ms(DECK, 900.0);
        engine.play(DECK);
        pull(&sink, &engine, |t| t.ended);
        assert!(!Tick::of(&engine.snapshot()).playing);
    }
}
