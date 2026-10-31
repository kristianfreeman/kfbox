# kfbox notes

kfbox is a very simple GUI app for DJ prep:

1. Load a song.
2. Analyse its waveform, especially volume changes across the track.
3. Place cue points from rules Kristian writes, because Kristian places cues the same way every time.
4. Tag tracks automatically.
5. Get the results into rekordbox: write a rekordbox XML file to import, or make a rekordbox USB directly.

It should not try to look like rekordbox. Keep the UI plain and the features few.

## Decisions (2026-10-29)

- **The cue pattern.** A at the start of the song, B eight bars before the drop, C on the drop, and D on something loopable at the end, like a stripped-back groove. The default rules (`crates/kfbox-core/rules.default.toml`) encode this, with D as a plain hot cue. Kristian named no colours or memory cues, so the colours are placeholders and there are no memory cues by default.
- **Automatic tagging** means Kristian's own rekordbox My Tag categories (groove and others). Mood is too complicated: skip it. Not built yet.
- **The GUI stack** is Tauri, with a plain Vite and TypeScript page (no framework).

## Reading rekordbox's library (2026-10-29)

kfbox reads `master.db` directly, which Kristian asked for. It copies `master.db` and its journal files to a temporary folder and opens the copy read-only, so it never touches rekordbox's own database. Grids come from each track's analysis file, read only when the track is loaded.

### How the rules compare with Kristian's own cues

18 tracks in the collection have A to D cued by hand. Run `cargo run --release -p kfbox-core --example score` to hide those cues, place the rule cues, and compare:

| Cue | First rule (`drop = "first"`) | Tuned (2026-10-31) | Tuned, leaving each track out of the fit |
| --- | --- | --- | --- |
| A | 17 of 18 | 17 of 18 | not tuned |
| B | 9 of 18 | 15 of 18 | follows C |
| C | 9 of 18 | 15 of 18 | 11 of 18, before the fallback for tracks with no break |
| D | 6 of 18 | 12 of 18 | 11 of 18 |

`score` now reports 63 of 76 over 19 tracks. That isn't an improvement: the 19th is Benjamin Mull's Cursed, which holds the rule-written cues from the one-track write test, so it matches itself. The hand-cued result is still 59 of 72.

The tuned figures come from the same 18 tracks the settings were chosen on, so expect the real rate to be nearer the last column. The settings sit on a flat stretch for `break_bass` (0.5 to 0.9) and `break_before` (0.5 to 0.6), and `groove_step` peaks at 0.15.

What the cues showed: Kristian's drop is the end of the longest bass-out in roughly the first half, not the first bass rise, and D is the start of the last section. B is 8 bars before C on 16 of 18, and D is always a plain cue, 17 to 65 bars from the end.

The misses that remain:

- **Ben Klock, CVNSUMED (C):** the drop is after a second, later break. On CVNSUMED that break ends at 58% of the track, just past `break_before`. Kuss has the same shape and wants the earlier drop, so the levels alone don't separate them.
- **Mothu (B, C):** Kristian's own track, cued differently: B on the first kick and C inside the break.
- **D on Amotik, Dextro, Evil Can Rest, CVNSUMED, Alberto Tolo, Aidan Rudd:** a smaller step late in the track, or a later dip, wins or loses against the one Kristian chose.

On the 88 tracks with no hand cues, the drop lands between bars 64 and 128 on 62, the same range as Kristian's own drops. It lands before bar 64 on 24, earlier than any drop Kristian cued, so those are the first to doubt. It finds none on 2 (Hertz, Planetary Assault Systems), which get no B or C.

## Still open

- **What rekordbox does when it imports a track that is already in its collection.** kfbox writes such a track back with its own `TrackID`, attributes, grid and cues, plus the new cues, but whether rekordbox then updates that track's cues or leaves them alone has not been tried. Kristian needs to try it by hand on one track.

- Which My Tag categories and values, and the rule for each. rekordbox XML has no My Tag field, so tags need the direct USB export or `master.db`.

## Reusable code in rbxport

rbxport lives at `~/Developer/rbxport`. Its Rust crates are in `crates/` and are independent of its Tauri app. Use them as git or path dependencies rather than copying them where you can.

**Licence:** rbxport is GPL-2.0-or-later, and the analysis crates are Chris Le's work. Linking them makes kfbox GPL-2.0-or-later too.

| Need | Take | Notes |
| --- | --- | --- |
| Load a song | `rbl-audio`: `decode_mono(path, max_secs)` | Decodes MP3, AAC, ALAC, FLAC, WAV and AIFF to mono `f32` samples. Light dependencies (symphonia, rubato). |
| Volume changes, grid, key | `rbl-analysis`: `analyse(&samples, sample_rate)` / `analyse_with(..)` | Only depends on `realfft`, and the same audio gives the same result. Returns the tempo and beat grid (4/4, downbeats numbered), the key, the waveform, the peak and the RMS. |
| Per-band levels | `rbl-analysis/src/waveform.rs` | Bass (200 Hz low-pass RMS), mids (200 Hz–2 kHz) and highs (above 2 kHz). This is the starting point for a per-bar volume and bass curve. |
| Kick present or absent | `rbl-analysis/src/attack.rs` | Finds the kick's attack near each predicted beat. Useful as a per-bar "kick in or out" feature. |
| Section starts | `rbl-analysis/src/downbeat.rs`: `phrase_starts(..)` and `GridPhase::phrase_starts` | About six per track, on downbeats, where the music changes most over four bars either side. They are not in the public `Analysis` result yet. Either call `downbeat` directly or send a small upstream change that exposes them. |
| rekordbox's own phrase labels | `rbl-anlz/src/phrase.rs`: `SongStructure`, `Phrase` | Decodes rekordbox's `PSSI` tag from a track's analysis file: mood, labels like `INTRO 2` / `UP 3` / `CHORUS 1`, start beats, and fill-ins. It handles rekordbox 6's masking. Good for testing your own section labels, or for rules on tracks rekordbox already analysed. |
| rekordbox XML | Write: `rbxport/crates/rbl-index/src/xml_export.rs`. Read: `rbxport/crates/rbl-db/src/xml.rs` | The writer is tied to rbxport's `Library` type, so copy the layout rather than depend on it. The layout is a `DJ_PLAYLISTS` document holding `TRACK` elements with `POSITION_MARK` cues (`Num` -1 is a memory cue, 0 and up are hot cues A, B, C…) and the `COLOURS` table. |
| Make the USB directly | `rbl-export`: `export(..)` with a list of `SourceTrack` | Works from plain track data. `id: 0` means "not in a rekordbox library", so rekordbox's database is not needed. It's heavy, and the hardware-tested part you least want to write yourself. Do this after XML. |
| Analysis files | `rbl-anlz` (`write.rs`, `encode.rs`, `cues.rs`, `grid.rs`) | Parses and writes `.DAT`, `.EXT` and `.2EX`. Desktop analysis files hold empty cue lists, and cues come from the library or XML. |

### Facts worth knowing

- Cue kinds in rekordbox's database: 0 is a memory cue, 1–3 are hot cues A–C, 5 is D, 6–9 are E–H, 10–17 are I–P, and 4 is unused (`rbl-db/src/write.rs`, `add_cue`).
- A loop's `BeatLoopSize` is `(beats << 16) | 1`, and 0 means the length comes from the In and Out points (`rbl-db/src/write.rs`, module docs).
- Default colours: a hot cue uses `ColorTableIndex` 21 and `Color` -1. A memory cue uses `Color` 255 (no colour) and `ColorTableIndex` 0.
- How rbxport measured itself against rekordbox: `rbl-analysis/examples/golden.rs` treats rekordbox's own results as answers and scores against them, without disassembling rekordbox. Use the same method to test cue and section rules against tracks you've already cued by hand.

### Skip

- rbxport's frontend, deck playback (`rbl-deck`), and the PRO DJ LINK and network crates.

## Suggested shape

```
song → decode → analyse (grid + per-bar volume / bass / kick) → your rules → cues + tags
     → preview on a plain waveform → write rekordbox XML (first), or export a USB (later)
```

Version one: load a folder of songs, analyse them, apply a rules file, preview the cues on a simple waveform, and write rekordbox XML.
