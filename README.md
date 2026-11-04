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

The window has one search field, one Export button, and the sidebar; everything else is in the **⌘K** palette and the macOS menu bar. The sidebar has the library's own lists (**All tracks**, **Recently added** in the last 30 days, **No cues** for tracks with no hot cues, and **Early drops** for tracks where the rules found the drop before bar 65, earlier than any drop you've cued by hand), your playlists, and **Rules** and **History**, which replace the table when chosen. The selected track's view is docked under the table, and **T** folds it away and back; kfbox remembers which between launches. An early drop shows as *early drop?* on the row, with why on hover, and in the track view; agents see the same as a `flags` entry in `propose_cues`. The threshold is `doubt_drop_before` in the rules' `[find]` table.

There are three ways in:

- **Your rekordbox library** loads when kfbox starts. The list shows **All tracks**, your whole collection by artist, analysed one at a time in the background. The sidebar narrows it to a playlist (intelligent ones included). Tracks use the beat grids from rekordbox's analysis files. kfbox reads a copy of `master.db`, made fresh each time, and never writes to rekordbox's database. **Reload library** (in the palette and the File menu) reads it again after you change something in rekordbox.
- **Open rekordbox XML** (⌘O) reads the file rekordbox writes with **File › Export Collection in xml format**, and lists its playlists. Picking one loads its tracks in order. Each track is decoded from its `Location`, and kfbox uses rekordbox's own grid from the track's `TEMPO` entries, so cues land on the beats rekordbox shows. A track rekordbox never analysed has no `TEMPO`, gets kfbox's grid, and is marked **kfbox grid** in the list.
- **Add songs** (⇧⌘O) adds single files on kfbox's grid. Songs can also be named on the command line of the built app. Dropping files on the window does either: an `.xml` opens as a collection, audio files are added.

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

The track view lists and draws the cues in rekordbox. **Apply rules** there, or ⌘R, writes the rules' cues into that track's empty slots, and **Apply rules to all** in the palette does it for every track in the list, in one change.

| Keys | Action |
| --- | --- |
| Shift-A to Shift-H | Set a hot cue at the playhead, on the nearest beat, replacing the one in that slot |
| M | Add a memory cue at the playhead |
| Delete | Delete the cue picked in the track view's cue line |
| T | Fold the track view away, or back |
| ⌘K | The palette: every command, and asking kfbox |
| Cmd-Z, Shift-Cmd-Z | Undo, redo |

Drag a cue's flag along the top of the waveform to move it to another beat, and double-click a cue's name to rename it. **History** in the sidebar lists every change, with undo, redo and restore to any earlier point.

### Cues already in rekordbox

kfbox never moves or replaces a cue that is already there. A rule fills a hot cue slot only when it is empty, and adds a memory cue only where there is none within a beat. 

## Rules

The rules are under **Rules** in the sidebar. **Save** (or Cmd-Return) places the cues and saves the rules to `~/Library/Application Support/com.kristianfreeman.kfbox/rules.toml`. **Defaults** puts back [`rules.default.toml`](crates/kfbox-core/rules.default.toml), which is also the reference for every setting, and saves that. Tracks finishing their analysis use the rules on screen without saving them.

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

**Export** writes every track that has finished analysing in the current list, and a `kfbox` playlist. It offers to save `kfbox.xml` next to the rekordbox XML that was opened. A track from a rekordbox XML is written back exactly as it was read (its `TrackID`, attributes, `TEMPO`s and cues) with the new cues added. A song added by hand gets kfbox's grid as `TEMPO` entries, so its cues land on the beats they were placed on.

In rekordbox, set **Preferences › Advanced › Database › rekordbox xml** to the file, then open **rekordbox xml › Playlists › kfbox** in the tree and import the tracks to the collection.

Not yet tried: what rekordbox does when an imported track is already in the collection, and whether it updates that track's cues or leaves them alone. Try it on a copy of one track before relying on it.

## Quick functions and the rules wizard

kfbox can hand a short instruction to Claude, which carries it out with kfbox's own commands as tools: the same ones the MCP server offers, plus writing cues and undoing. Press **⌘K** and type, for example, "redo the cues on this track, but put C on the second drop", or "redo this playlist's cues, ignore tracks with a long intro". **↵** runs it on the selected track, **⌘↵** on every track in the list on screen. What it writes goes straight in, through the safe path, refused while rekordbox is open, and shows in **History** as `Assistant: …`, where it undoes like any other change.

The **rules wizard**, offered in **Rules** until rules are saved, in the palette and in **Settings** (⌘,), writes your rules file for you. Say how you cue; it reads the tracks you cued by hand (A to D all set), drafts rules in kfbox's schema, checks each draft with the real parser, and scores it on those tracks: how many of your A, B, C and D cues it puts in the same bar. Refine it with more guidance, then **Save these rules**. Saving keeps the rules it replaces, and **Undo last save** puts them back.

**Settings** picks the model, Claude Sonnet 5.5 (the default) or Claude Haiku 5.5 (faster and cheaper), and takes your Anthropic API key. The key is stored in the macOS Keychain and read only by the app's Rust side to make a request. It never goes to the page, `kfbox.db`, a settings file, a log or the library's history; the page only shows that a key is saved, with a button to remove it. Requests go straight to `api.anthropic.com`. On Sonnet they include Anthropic's server-side refusal fallback.

## From an agent, over MCP

`kfbox-mcp` gives an agent the same library and rules over MCP on stdin and stdout. It reads rekordbox's library from a copy, and shares the app's analysis cache, saved rules and history, so a track the app has analysed answers at once. Its writes take the app's safe path: refused while rekordbox is open, one undoable change each, shown in **History** as `Agent: …`. An open kfbox window picks up an agent's write within a second or so, without a reload: the changed tracks update in place, and the list is read again only if tracks or playlists came or went.

| Tool | What it does |
| --- | --- |
| `list_playlists` | The library's size, playlists and folders |
| `search_tracks` | Tracks matching every word, across the library or one playlist |
| `playlist_tracks` | A playlist's tracks in order |
| `track_detail` | A track's grid, its cues with their bars, and optionally every bar's levels |
| `propose_cues` | The cues the rules would place, as a dry run, with the saved rules or rules given |
| `export_xml` | A new rekordbox XML file at a path the agent names, never inside rekordbox's folders |
| `write_cues` | Adds, moves, renames, recolours or deletes cues, all together or not at all |
| `write_rule_cues` | Writes the rules' cues into the empty slots of the tracks named |
| `undo` | Undoes the latest change, if nothing has changed since |

Build it and add it to Claude Code:

```bash
cargo build --release -p kfbox-mcp
```

```bash
claude mcp add --scope user kfbox -- ~/Developer/kfbox/target/release/kfbox-mcp
```

`kfbox-mcp --xml file.xml` reads a rekordbox XML instead, which it can't write to, and `KFBOX_DIR` replaces the app's data folder. `--fixture <folder>` opens a test library made by rbl-db's fixture builder; its passphrase can't open a real library.

## Without the GUI

```bash
cargo run --release -p kfbox-core --example cue -- - out.xml song.mp3
```

That applies the default rules (or a rules file in place of `-`), prints each track's landmarks and cues, and writes the XML. The `playlist` example does the same for a playlist of a rekordbox XML, or of your rekordbox library when given `rekordbox` in place of the XML; run it with just that to list the playlists. The `dump` example prints a track's analysis and cues as JSON, and `score` compares the rules with your hand-placed cues.

## Checks

`pnpm check` runs the staged-file guard, the typecheck, ESLint and Prettier, the interface tests, rustfmt, pedantic clippy with warnings as errors, and the Rust tests. `pnpm install` sets it up as the pre-commit hook. The guard refuses rekordbox databases, backups, analysis files, XML exports, audio, secrets and anything over 2 MB, even with `git add -f`.

Only `src/ipc` may talk to Tauri. Everything else goes through the `Backend` interface in `src/ipc/backend.ts`, and a test enforces that.

## Licence

GPL-2.0-or-later, because the rbxport crates it links are. See [LICENSE](LICENSE) and [COPYING](COPYING). The Geist fonts in `src/styles/fonts` are under the SIL Open Font License.
