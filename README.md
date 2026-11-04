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

Analysis runs once per file. Its result goes into `kfbox.db` in the app's data folder, keyed by the file's size, modified time, a hash of its start and end, the grid, and the analysis version. On a 106-track library in a release build, the list takes about 60 ms, the first full analysis 28 s, and every later load 0.4 s with nothing decoded.

### Playing and moving around

kfbox plays through rbxport's `rbl-deck` engine. Double-click a track, or press Enter, to open it and play. The bar along the bottom has the transport, a strip of the whole track to click, and a pad for each hot cue. In the track view, click the waveform to move the playhead, drag to scrub, and click a row of the cue table to jump to it.

| Keys | Action |
| --- | --- |
| Up, Down, PageUp, PageDown, Home, End | Move through the table |
| Enter | Open the track view and play |
| Escape | Close the track view, or clear the search |
| Space | Play or pause |
| Left, Right | Seek one beat, on the track's grid |
| Shift-Left, Shift-Right | Seek one bar |
| A to H | Jump to a hot cue; a cue with an end starts its loop |
| L | Start a 16-beat loop on the beat under the playhead, or end the loop |
| Q | Add the selected track to the queue |
| ] | Go to the next track in the queue |
| + and - | Zoom the track view, up to 64 times |
| `/` | Search, where Up, Down and Enter pick a result |

Column headers sort: ascending, descending, then back to the list's own order.

### Editing cues and applying the rules

kfbox saves cues straight into rekordbox's library, through the safe write path: each save is checked on a copy first, swapped in whole, recorded in the history, and undoable. rekordbox has to be closed to save. If it's open, the save is refused and nothing changes.

The track view lists and draws the cues in rekordbox. **Apply rules** there writes the rules' cues into that track's empty slots, and **Apply rules…** in the top bar does it for every track in the list, in one change, after asking once.

| Keys | Action |
| --- | --- |
| Shift-A to Shift-H | Set a hot cue at the playhead, on the nearest beat, replacing the one in that slot |
| M | Add a memory cue at the playhead |
| Delete | Delete the cue picked in the cue table |
| Cmd-Z, Shift-Cmd-Z | Undo, redo |

Drag a cue's flag along the top of the waveform to move it to another beat, and double-click a cue's name to rename it. **History** lists every change, with undo, redo and restore to any earlier point.

### Cues already in rekordbox

kfbox never moves or replaces a cue that is already there. A rule fills a hot cue slot only when it is empty, and adds a memory cue only where there is none within a beat. 

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

## From an agent, over MCP

`kfbox-mcp` gives an agent the same library and rules over MCP on stdin and stdout. It reads rekordbox's library from a copy, and shares the app's analysis cache and saved rules, so a track the app has analysed answers at once. So far it only reads and proposes. Nothing it does writes to the library, and writing comes in milestone 4a.

| Tool | What it does |
| --- | --- |
| `list_playlists` | The library's size, playlists and folders |
| `search_tracks` | Tracks matching every word, across the library or one playlist |
| `playlist_tracks` | A playlist's tracks in order |
| `track_detail` | A track's grid, its cues with their bars, and optionally every bar's levels |
| `propose_cues` | The cues the rules would place, as a dry run, with the saved rules or rules given |
| `export_xml` | A new rekordbox XML file at a path the agent names, never inside rekordbox's folders |

Build it and add it to Claude Code:

```bash
cargo build --release -p kfbox-mcp
```

```bash
claude mcp add --scope user kfbox -- ~/Developer/kfbox/target/release/kfbox-mcp
```

`kfbox-mcp --xml file.xml` reads a rekordbox XML instead, and `KFBOX_DIR` replaces the app's data folder.

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
