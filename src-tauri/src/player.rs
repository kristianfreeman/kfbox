//! Real playback: rbl-deck's engine, one deck of it, and the tick the page
//! reads it through.
//!
//! The engine opens the audio device when something is first loaded, not at
//! launch. Position comes as one event ten times a second, and the page
//! extrapolates between ticks.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rbl_deck::{ClickVolume, Deck, DeckEvent, Engine, Snapshot};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

const TICK: Duration = Duration::from_millis(100);
const DECK: Deck = Deck::A;

#[derive(Default)]
pub struct Player {
    engine: Mutex<Option<Arc<Engine>>>,
    ticking: AtomicBool,
    next_load: AtomicU64,
    /// The metronome as the page last set it, kept for an engine opened
    /// later: on or off, its level (0 to 2), and the loaded track's grid.
    metronome: AtomicBool,
    metronome_level: AtomicU8,
    grid: Mutex<Vec<(u32, bool)>>,
}

/// An engine on a sink nobody hears, pulled in real time as a device would
/// be: `KFBOX_SILENT`, so the perf run plays tracks without a sound on the
/// machine it runs on, at the same cost.
fn silent_engine(events: &rbl_deck::EventSink) -> rbl_deck::Result<Engine> {
    const FRAMES: usize = 512;
    const RATE: u32 = 44_100;
    Engine::with_sink(
        |render| {
            let sink = Arc::new(rbl_deck::NullSink::new(RATE, render));
            let pulled = Arc::clone(&sink);
            // 512 frames at 44.1 kHz: 11.6 ms.
            let every = Duration::from_micros(11_610);
            // Pulled while the stream runs, as a device would; stopped, it only looks in now and then.
            let _ = std::thread::Builder::new().name("kfbox-silent-audio".into()).spawn(move || loop {
                if pulled.running() {
                    let _ = pulled.pull(FRAMES);
                    std::thread::sleep(every);
                } else {
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
            Ok(sink as Arc<dyn rbl_deck::Sink>)
        },
        events,
    )
}

/// The metronome's three levels, quietest first: rbl-deck's, which are
/// rekordbox's Small, Middle and Large.
fn click_volume(level: u8) -> ClickVolume {
    match level {
        0 => ClickVolume::Small,
        1 => ClickVolume::Middle,
        _ => ClickVolume::Large,
    }
}

/// A track's beats, `(seconds, number in the bar)` as the page has them, as
/// the metronome takes them: milliseconds from the track's start, and
/// whether each is a bar's first. The deck plays on the same timeline as the
/// grid the page draws, so the click lands where the page shows the beat.
pub fn metronome_grid(beats: &[(f64, u8)]) -> Vec<(u32, bool)> {
    let mut grid: Vec<(u32, bool)> = beats
        .iter()
        .filter(|(secs, _)| secs.is_finite() && *secs >= 0.0)
        .map(|&(secs, number)| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a track is far shorter than 49 days")]
            let ms = (secs * 1000.0).round() as u32;
            (ms, number == 1)
        })
        .collect();
    grid.sort_unstable();
    grid
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
        let opened = if std::env::var_os("KFBOX_SILENT").is_some() { silent_engine(&events) } else { Engine::new(&events) };
        let engine = Arc::new(opened.map_err(|e| format!("The audio device could not be opened: {e}"))?);
        self.apply_metronome(&engine);
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

    /// Hands the engine the metronome as the page last set it.
    fn apply_metronome(&self, engine: &Engine) {
        engine.metronome().set_volume(click_volume(self.metronome_level.load(Ordering::Relaxed)));
        if let Ok(grid) = self.grid.lock() {
            engine.set_metronome_grid(DECK, &grid);
        }
        engine.set_metronome(DECK, self.metronome.load(Ordering::Relaxed));
    }

    /// Replaces the loaded track's grid, here and in the engine if it's open.
    fn set_grid(&self, grid: Vec<(u32, bool)>) {
        if let Some(engine) = self.opened() {
            engine.set_metronome_grid(DECK, &grid);
        }
        if let Ok(mut held) = self.grid.lock() {
            *held = grid;
        }
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
    // The last track's grid would click in the wrong places: none until the
    // page sends this one's.
    player.set_grid(Vec::new());
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

/// Switches the metronome on or off and sets its level, 0 to 2. It clicks
/// only while the deck plays, on the grid [`player_metronome_grid`] gave.
#[tauri::command]
pub fn player_metronome(on: bool, level: u8, player: State<'_, Player>) {
    player.metronome.store(on, Ordering::Relaxed);
    player.metronome_level.store(level.min(2), Ordering::Relaxed);
    if let Some(engine) = player.opened() {
        player.apply_metronome(&engine);
    }
}

/// The loaded track's beats for the metronome: `(seconds, number in the bar)`.
/// An empty grid, as for a track with none, means no clicks.
#[tauri::command]
pub fn player_metronome_grid(beats: Vec<(f64, u8)>, player: State<'_, Player>) {
    player.set_grid(metronome_grid(&beats));
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
        write_wav(path, &samples);
    }

    fn write_wav(path: &Path, samples: &[i16]) {
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

    /// An engine on a sink pulled by hand: nothing reaches the audio device.
    fn silent_engine() -> (Engine, Arc<NullSink>) {
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
        (engine, sink)
    }

    /// Plays on until the track reaches `until` frames, and returns where in
    /// the track each click started: a silent track, so any sound is one.
    fn clicks(sink: &NullSink, engine: &Engine, until: u64) -> Vec<u64> {
        const BLOCK: usize = 32;
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut onsets = Vec::new();
        let mut quiet = BLOCK * 64;
        loop {
            let at = engine.snapshot().a.position_frames;
            if at >= until {
                return onsets;
            }
            for (i, sample) in sink.pull(BLOCK).iter().enumerate() {
                if sample.abs() > 1e-3 {
                    if quiet > 2_000 {
                        onsets.push(at + i as u64);
                    }
                    quiet = 0;
                } else {
                    quiet += 1;
                }
            }
            assert!(Instant::now() < deadline, "timed out at {at} frames");
        }
    }

    /// The metronome clicks on every beat of the grid the page sends,
    /// downbeats too and with the same click (rbl-deck's, after rekordbox's),
    /// picks up on the next beat after a seek into the middle of one, and
    /// clicks again on the loop's first beat as it wraps. All on a sink, so no
    /// sound is made.
    #[test]
    fn the_metronome_clicks_on_the_grid_through_a_seek_and_a_loop() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("silence.wav");
        write_wav(&song, &vec![0; RATE as usize * 4]);
        let (engine, sink) = silent_engine();
        engine.load_as(DECK, &song, 1);
        pull(&sink, &engine, |t| t.loaded && t.load_id == 1);
        // 240 BPM: a beat every quarter second, a bar every second.
        let beats: Vec<(f64, u8)> = (0..16).map(|i| (f64::from(i) * 0.25, u8::try_from(i % 4 + 1).unwrap())).collect();
        let grid = metronome_grid(&beats);
        assert_eq!(grid[4], (1000, true));
        assert_eq!(grid[5], (1250, false));
        engine.set_metronome_grid(DECK, &grid);
        engine.set_metronome(DECK, true);
        let beat = u64::from(RATE) / 4;
        let near = |heard: u64, beat_frame: u64| heard.abs_diff(beat_frame) <= 128;

        engine.play(DECK);
        let heard = clicks(&sink, &engine, beat * 4 + beat / 2);
        assert_eq!(heard.len(), 5, "beats 0 to 4: {heard:?}");
        for (n, frame) in heard.iter().enumerate() {
            assert!(near(*frame, beat * n as u64), "beat {n} at {frame}");
        }

        // Into the middle of beat 9: the next click is beat 10, not a click now.
        engine.seek_ms(DECK, 2_375.0);
        pull(&sink, &engine, |t| t.frames >= beat * 9 + beat / 2);
        let heard = clicks(&sink, &engine, beat * 11 - beat / 2);
        assert_eq!(heard.len(), 1, "after the seek: {heard:?}");
        assert!(near(heard[0], beat * 10), "beat 10 at {}", heard[0]);

        // A one-bar loop over beats 4 to 7: beat 4 clicks again on each wrap.
        engine.set_loop_ms(DECK, 1_000.0, 2_000.0);
        engine.set_looping(DECK, true);
        engine.seek_ms(DECK, 1_000.0);
        pull(&sink, &engine, |t| t.frames < beat * 5);
        let mut onsets = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(20);
        while onsets.iter().filter(|f| near(**f, beat * 4)).count() < 2 {
            onsets.extend(clicks(&sink, &engine, engine.snapshot().a.position_frames + 64));
            assert!(Instant::now() < deadline, "no second pass of the loop: {onsets:?}");
        }
        assert!(onsets.iter().all(|f| (4..8).any(|n| near(*f, beat * n))), "only the loop's beats: {onsets:?}");

        // Off: silence, once a click already sounding has rung out.
        engine.set_metronome(DECK, false);
        let _ = clicks(&sink, &engine, engine.snapshot().a.position_frames + u64::from(RATE) / 8);
        let quiet = clicks(&sink, &engine, engine.snapshot().a.position_frames + beat * 2);
        assert!(quiet.is_empty(), "off, yet {quiet:?}");
    }

    #[test]
    fn the_grid_goes_to_the_metronome_in_milliseconds_with_its_downbeats() {
        let grid = metronome_grid(&[(0.5, 2), (0.0, 1), (-0.1, 4), (f64::NAN, 1), (1.0004, 1)]);
        assert_eq!(grid, vec![(0, true), (500, false), (1000, true)]);
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

    /// A song dropped on the window, imported into a fixture library, plays,
    /// and on the same timeline its analysis was made on: the LAME file's
    /// 1,152 frames of lead are in both or neither.
    #[test]
    fn an_imported_mp3_plays_on_the_timeline_its_grid_was_made_on() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        rbl_db::fixture::build(&lib, rbl_db::fixture::Shape { tracks: 2, ..rbl_db::fixture::Shape::default() }).unwrap();
        let session = kfbox_core::session::Session::open(&dir.path().join("app")).unwrap();
        session.open_db(kfbox_core::testing::fixture_location(&lib)).unwrap();
        let song = dir.path().join("click-lame.mp3");
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/kfbox-core/tests/fixtures/click-lame.mp3"), &song).unwrap();
        let report = session.import_files(std::slice::from_ref(&song)).unwrap();
        let row = &report.imported[0];
        let analysed = session.track_summary(&row.id, false).unwrap().analysis;

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
        engine.load_as(DECK, Path::new(&row.path), 3);
        pull(&sink, &engine, |t| t.loaded && t.load_id == 3);
        let ready = Tick::of(&engine.snapshot());
        #[allow(clippy::cast_precision_loss, reason = "a frame count")]
        let played_secs = ready.total_frames as f64 / f64::from(ready.sample_rate);
        assert!((played_secs - analysed.duration_secs).abs() < 0.005, "player {played_secs} s, analysis {} s", analysed.duration_secs);

        engine.play(DECK);
        let loudest = pull(&sink, &engine, |t| t.frames > u64::from(RATE) / 2);
        assert!(loudest > 0.1, "the imported track sounds: {loudest}");
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
