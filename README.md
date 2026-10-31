# kfbox

kfbox cues DJ tracks by rules. You load songs, it finds the beat grid and the shape of each track, places your cues where your rules say, and writes a rekordbox XML file you import.

Decoding and analysis come from rbxport's `rbl-audio` and `rbl-analysis` crates, and rekordbox's library is read with its `rbl-db`, `rbl-index` and `rbl-anlz`, pinned to one revision in the root `Cargo.toml`. `rbl-db` builds SQLCipher and OpenSSL, so the first build takes a few minutes.

## Run it

```bash
pnpm install
```

```bash
pnpm dev
```

To work on the interface without the app or your library, run `pnpm dev:web`. In a browser, kfbox uses an invented library from `src/ipc/mock.ts`.

There are three ways in:

- **Your rekordbox library** loads when kfbox starts. The list shows **All tracks**, your whole collection by artist, analysed one at a time in the background. The picker at the top narrows it to a playlist (intelligent ones included). Tracks use the beat grids from rekordbox's analysis files. kfbox reads a copy of `master.db`, made fresh each time, and never writes to rekordbox's database. **Reload rekordbox** reads it again after you change something in rekordbox.
- **Open XML…** reads the file rekordbox writes with **File › Export Collection in xml format**, and lists its playlists. Picking one loads its tracks in order. Each track is decoded from its `Location`, and kfbox uses rekordbox's own grid from the track's `TEMPO` entries, so cues land on the beats rekordbox shows. A track rekordbox never analysed has no `TEMPO`, gets kfbox's grid, and is marked **kfbox grid** in the list.
- **Add songs…** adds single files on kfbox's grid. Songs can also be named on the command line of the built app. Dropping files on the window does either: an `.xml` opens as a collection, audio files are added.

A track takes well under a second to analyse in a release build.

### Cues already in rekordbox

kfbox never moves or replaces a cue that is already there. A rule fills a hot cue slot only when it is empty, and adds a memory cue only where there is none within a beat. In the preview, a kept cue has an open flag and a dashed line, and a new one a solid flag and line. The cue table marks each **kept** or **new**.

## Rules

The rules live in the app's bottom pane. **Apply** (or Cmd-Return) places the cues and saves the rules to `~/Library/Application Support/com.kristianfreeman.kfbox/rules.toml`. **Defaults** puts back [`rules.default.toml`](crates/kfbox-core/rules.default.toml), which is also the reference for every setting, and saves that. Tracks finishing their analysis use the rules in the pane without saving them.

Each `[[cue]]` sits on a landmark and can be moved by whole bars or turned into a loop:

```toml
[[cue]]
slot = "B"          # A to P, or "memory"
at = "drop"         # start, drop or groove
offset_bars = -8    # eight bars before the drop
color = "yellow"    # pink red orange yellow green aqua blue purple, or "#rrggbb"
name = "8 to drop"  # the cue's comment in rekordbox
```

The landmarks come from the per-bar volume, bass, and "busy" (mids and highs) levels, each scaled so 1.0 is the track's own loud end:

- **start** is the first downbeat at least `start_level` loud.
- **drop** is where the bass comes back after the longest break that ends within `break_before` of the track, where a break is a stretch with the bass below `break_bass`. A bass-less intro doesn't count, and a break ending on an 8-bar phrase from the start beats one that doesn't. A track with no break uses the first place the bass rises by `drop_rise` over four bars, compared with the four before. `drop = "first"` always uses that rise, and `drop = "biggest"` the largest one.
- **groove** is where the last section starts, after `groove_after` of the track and at least `groove_end_bars` before the end. That is the later of the last return of the bass and the last phrase where the mids and highs fall by `groove_step` over a steady kick. A track with neither gets its most stripped-back steady stretch, with the bass above `groove_bass`.

To see how a change compares with your own cues, run the `score` example. It hides the cues on every track with A to D set by hand, places the rule cues, and prints the difference in bars.

## Into rekordbox

**Export rekordbox XML…** writes every track that has finished analysing in the current list, and a `kfbox` playlist. It offers to save `kfbox.xml` next to the rekordbox XML that was opened. A track from a rekordbox XML is written back exactly as it was read (its `TrackID`, attributes, `TEMPO`s and cues) with the new cues added. A song added by hand gets kfbox's grid as `TEMPO` entries, so its cues land on the beats they were placed on.

In rekordbox, set **Preferences › Advanced › Database › rekordbox xml** to the file, then open **rekordbox xml › Playlists › kfbox** in the tree and import the tracks to the collection.

Not yet tried: what rekordbox does when an imported track is already in the collection, and whether it updates that track's cues or leaves them alone. Try it on a copy of one track before relying on it.

## Without the GUI

```bash
cargo run --release -p kfbox-core --example cue -- - out.xml song.mp3
```

That applies the default rules (or a rules file in place of `-`), prints each track's landmarks and cues, and writes the XML. The `playlist` example does the same for a playlist of a rekordbox XML, or of your rekordbox library when given `rekordbox` in place of the XML; run it with just that to list the playlists. The `dump` example prints a track's analysis and cues as JSON, and `score` compares the rules with your hand-placed cues.

## Checks

`pnpm check` runs the staged-file guard, the typecheck, the interface tests, clippy and the Rust tests. `pnpm install` sets it up as the pre-commit hook. The guard refuses rekordbox databases, backups, analysis files, XML exports, audio, secrets and anything over 2 MB, even with `git add -f`.

Only `src/ipc` may talk to Tauri. Everything else goes through the `Backend` interface in `src/ipc/backend.ts`, and a test enforces that.

## Licence

GPL-2.0-or-later, because the rbxport crates it links are. See [LICENSE](LICENSE) and [COPYING](COPYING). The Geist fonts in `src/styles/fonts` are under the SIL Open Font License.
