# kfbox 1.0 plan

kfbox 1.0 replaces rekordbox for everything you do before a gig: browse, listen, cue, tag, import, and export a USB. You can work for weeks without opening rekordbox, and the automatic cues land in your library with one click, every change undoable.

Status on 2026-11-04: milestones 0 to 6 are on `main`, and layout B (the redesign Kristian chose) is on `redesign-b`. Milestone 4 makes the app save to rekordbox's library; nothing has been saved there through it yet. Kristian cut review mode from milestone 4 on 2026-11-04, and added milestones 5 (cleanup) and 6 (LLM quick functions and a rules wizard), so the old 5 onward are now 7 onward. The app still only reads `master.db`: nothing in it calls the safe write path yet.

## What 1.0 includes

| In 1.0 | Not in 1.0 |
| --- | --- |
| Library: search, sort, playlists, folders, intelligent playlists | Two-deck mixing and Performance mode |
| Player: play, seek, scrub, loops, hot cue jumps, queue, zoomable waveform | PRO DJ LINK (rbxport has the crates, so it can come later) |
| Cues: click, drag, and keyboard editing; rules and an LLM write cues straight in, with undo | Cloud sync, stems, lighting, recording |
| Import: drag in files and folders, analysed once | Windows |
| Editing: My Tags, rating, colour, comments, beat grid nudge and BPM | Mood tagging, which you ruled out |
| Automatic My Tags, starting with groove | |
| USB export for CDJs | |
| Backups: history of every change, undo, restore | |

If you perform from a laptop in rekordbox, you'd still use it for that. For USB sets, 1.0 replaces it completely.

## Decisions

1. **Build on rbxport's crates, not a fork of its app (decided).** rbxport's README says it stays small on purpose, and its interface follows rekordbox, which you don't want. Its 49,000-line front end also carries LINK, translations and an updater that 1.0 doesn't need. kfbox adds `rbl-deck`, `rbl-backup`, `rbl-export` and `rbl-devices`, and adapts rbxport's shell code where it fits: `player.rs`, `cues.rs`, `grid.rs`, `file_drop.rs`, `file_journal.rs` and the waveform canvas. The crates are pinned to a revision of `chrisle/rbxport`, so upstream changes arrive when we choose.
2. **`master.db` stays the library.** kfbox doesn't invent a second one, so rekordbox still opens the same collection if kfbox ever fails you. kfbox's own data (cache, settings) lives in its own `kfbox.db`.
3. **React and TypeScript replace the vanilla page.** The current 530-line page can't carry a virtual table, focus handling and a player. React plus TanStack Virtual also lets us adapt rbxport's parts directly. Backend calls go through one typed layer with a mock, so the UI can be tested in a browser without touching your library.
4. **Geist everywhere, bundled with the app.** Geist for text, Geist Mono for BPM, times and bars. It loads from the app, not the network.
5. **Automatic cues write straight in.** Applying the rules, an agent, or the LLM menu writes cues into empty slots in one change, through the safe path, and History undoes it. There's no review step: Kristian judged it overengineering (2026-11-04).

## Why analysis repeats today

Results are held in memory only, so every launch decodes every track again. In 1.0 each result goes into `kfbox.db`, keyed by file size, modified time, a hash of the file, the grid, and the analysis version. A later launch reads the result and decodes nothing. Only new or changed files are analysed, in a background queue that can be cancelled and that resumes after a restart. Changing rules re-places cues instantly, because that step never needed the audio.

## Backups and atomic changes

You asked whether git could run under the hood. It can, and I tested it on a copy of your library with 30 tracks of real cue writes:

| Measure | Result |
| --- | --- |
| Time per commit of `master.db` | About 190 ms |
| Pages changed by four new cues | 10 to 15 of about 2,200 |
| Repository after 30 commits | 96 MB, or 10 MB after `git gc` |

So git works as long as it runs `gc` regularly. It has one weakness: `master.db` is encrypted, so `git diff` shows nothing useful. That's why the design has four layers:

1. **One action, one transaction.** Today `rbl-db`'s `Writer` commits after each cue, so a crash could leave half of a batch. kfbox needs a batch write (a small `rbl-db` addition) so that "accept cues on 40 tracks" lands whole or not at all. Changes to analysis files use `file_journal.rs`, which finishes or rolls back after a crash.
2. **A git commit for every action.** A private local repository holds `master.db`, the changed analysis files, and a readable JSON file per track (cues, tags, rating). That JSON gives real diffs and per-track undo. Commit messages say what happened, such as `Apply rules to 40 tracks`. It's never pushed anywhere, because the JSON is your library in plain text.
3. **Full archives.** `rbl-backup` writes a complete archive of `master.db`, analysis files and artwork before the first write of each day, kept outside the git repository. If git itself breaks, these still restore the library.
4. **Spotting rekordbox's changes.** Before each write, kfbox compares the library on disk with the last commit. If rekordbox changed something, kfbox commits that first as `Changed outside kfbox`, so its own undo can't erase your work.

Every write follows the same order: check rekordbox is closed, snapshot if needed, apply in one transaction, read it back, run `PRAGMA integrity_check`, commit to git. If rekordbox is open, your edits wait in `kfbox.db` under a banner and apply when you quit it.

Cmd-Z undoes the last action. A History panel lists every action and can restore the library to any point. A restore is itself a commit, so it can be undone too.

## The interface

It should read as a music player: calm, type-led, with the track you're hearing always in view.

- **Sidebar:** All Tracks, Recently Added, Missing Files, then your playlists and folders. Drag tracks here to add them to a playlist.
- **Track table:** title, artist, BPM, key, time, tags and cue status. It's virtualised, with sortable columns and instant search.
- **Now playing, always at the bottom:** artwork, title, transport, overview waveform, and hot cue buttons.
- **Track view, opened with Enter or a double click:** large zoomable waveform, beat grid, cue list, tags, and the bars the rules read.
- **Apply rules:** for one track from its view, or for the whole list from the top bar after asking once. Either is one change in History.
- **Quick functions, Cmd-K:** a menu where a short instruction to an LLM does the work, such as "redo this track's cues, but put C on the second drop" (milestone 6).
- **Drag and drop:** drop files or folders anywhere. Progress shows in place, and duplicates are flagged rather than added twice.

Everything works with both mouse and keyboard, and a shortcut sheet lives behind `?`. The first set:

| Keys | Action |
| --- | --- |
| Up, Down | Move through tracks |
| Enter | Open and play |
| Space | Play or pause |
| Left, Right | Seek one beat |
| Shift-Left, Shift-Right | Seek one bar |
| A to H | Jump to hot cue |
| Shift-A to Shift-H | Set hot cue at the playhead |
| M | Add a memory cue |
| `/` | Search |
| Cmd-K | Command palette |
| Cmd-Z, Shift-Cmd-Z | Undo, redo |
| Tab | Move between panes |

## Milestones

Sizes are relative: S, M, L. Each one ends in something you can use or a check that has to pass.

| # | Milestone | Size | Needs | Done when |
| --- | --- | --- | --- | --- |
| 0 | Foundation: first commit, pinned crates, React shell, Geist, typed backend layer, mock | S | Your OK to commit | App opens in the new shell; checks run on every commit |
| 1 | Library and cache: `kfbox.db`, `rbl-index`, table, sidebar, search, keyboard focus | M | 0 | Track list in under a second; a second launch decodes nothing |
| 2 | Player: `rbl-deck`, transport, waveforms, scrub, cue jumps, queue | M | 0 | You can browse and play by keyboard alone without an audio glitch |
| 3 | Safe writes: batch transactions, git history, archives, restore, crash recovery | L | 0 | Passes the checks below on a copy; **a real batch looks right in rekordbox** (done for plain named cues) |
| 3a | Command layer: every action a plain `kfbox-core` function with serde input and output; Tauri becomes a thin wrapper | M | 1, 2, 3 | The app works as before, and each command has a test that runs it without Tauri |
| 3b | MCP server, read and propose: `kfbox-mcp` over stdio | S | 3a | An agent can search, read a track, and see the cues kfbox would place, with no way to write |
| 4 | Cues: editing in the app, the rules writing straight in, undo, History panel | L | 1, 2, 3, 3a | You've cued a playlist in kfbox and every change undoes |
| 4a | Agent writes and live reload: `write_cues` and `undo` over MCP, straight in; the open app shows writes from other processes | M | 3b, 4 | An agent's write lands in the history, undoes, is refused while rekordbox runs, and shows in the open window within a couple of seconds |
| 5 | Code cleanup: rustfmt, clippy pedantic where sensible, Prettier and ESLint, clear names and docs, dead and duplicated code out, idiomatic crates in | M | 4 | No behaviour changes, and every test passes before and after |
| 6 | LLM quick functions: a Cmd-K menu where a model uses kfbox's commands as tools, a rules wizard, API keys in the Keychain | L | 4, 5 | A typed instruction redoes a track's cues, the wizard writes rules that score on your hand cues, and no key ever leaves the Keychain except to the provider |
| 7 | Import: drag and drop, analysis files, duplicates, missing files | M | 3, 4 | A dropped track plays, gets cues, and loads correctly in rekordbox |
| 8 | Library editing: playlists, tags, rating, grid, automatic My Tags | M | 3, 4 | Edits survive a round trip through rekordbox |
| 9 | USB export: `rbl-export`, drive picker, progress | M | 8 | **A USB plays with cues on your CDJs** |
| 9a | The app serves its commands on localhost behind a token, for agents and a browser | M | 4a | A browser page can search, play and write through the app, and an agent can reach the app without its own MCP process |
| 10 | 1.0: performance budgets, signed build, drop finder retuned on your edits | M | All | You go two weeks of prep and a gig without opening rekordbox |

Milestones 1, 2 and 3 can run in parallel after 0, each in its own session and worktree, because they touch separate code once the shell exists. Milestones 4 onward run mostly in sequence. Start with 3 if you want one first: it's the riskiest, and everything that writes depends on it.

### Notes from milestone 4

- **Review mode was built, then cut.** Kristian judged it overengineering, so it's removed rather than left half-wired: no ghost cues, no Needs Review list, no review keys, no review state in `kfbox.db`.
- **Saves go straight in.** A cue edit is saved the moment it's made, one history commit each. **Apply rules** writes the rules' cues into empty slots, for one track or the whole list in one commit. The app opens the safe write path only for the length of each save, so an agent can write in between.
- **With rekordbox open, a save is refused, not queued.** The design above said edits would wait in `kfbox.db` and apply when rekordbox quits. That's left out for now: the refusal says why, nothing changes, and the edit can be made again. Worth adding if it proves annoying.
- **Keys changed:** the queue's next track moved from N to `]`. Only cues in rekordbox are drawn, listed and jumped to; the rules' cues show as a count until they're written.
- **Redo is an undo of an undo**, recorded as `Redo: …`, and only straight after an undo.
- **Tests:** 8 session write tests on fixture libraries; 2 tests that call every page command through Tauri's own IPC and permission checks with a mock runtime, which covers the 3a refactor without a window; interface tests for applying rules, editing, undo and a refused save.

### Notes from milestone 5

- **Formatting and lints are enforced.** `rustfmt.toml` sets the width the code was written to (140); clippy runs with `pedantic`, allowing only `missing_errors_doc`, and the check fails on any warning. Prettier and ESLint, with the React hooks rules, cover the frontend. `pnpm check` runs all of them.
- **Dead code out:** the `write_one` example, which wrote outside the safe path; the page command `remove`, which nothing called; the MCP server's own copy of search; a second test-audio generator; and two hand-built paths to the data folder, now `Session::default_dir` through the `dirs` crate.
- **Shared instead of repeated:** one slot parser (`Slot::parse`) for the rules, the session and the history; one hex helper for the two hashes.
- **`rmcp` stays out.** The hand-written MCP layer is about a hundred lines, has five tests that drive the real binary, and needs no async runtime. `rmcp` would bring tokio and its macros for no change in behaviour. Revisit if MCP needs resources, prompts or streaming.
- **No behaviour changed:** every test passed before and after, with the same counts.

### Milestone 6: LLM quick functions and the rules wizard (added 2026-11-04)

Kristian's brief: an LLM under the hood as "a very intelligent macro/quick fn menu".

**The menu.** Cmd-K opens a box where a short instruction does the work, with the selected track or the current list as context. For example, "redo the cues on this track, but put C on the second drop", or "redo this playlist's cues, ignore tracks with a long intro". The model gets the same tools as `kfbox-mcp` (search, track detail with per-bar levels, propose cues) plus writing and undoing cues. It answers in a line or two, and anything it writes appears in History like any other change.

**Writes go straight in**, as agent writes do: the safe path, refused while rekordbox runs, undoable. The model's steps are limited per instruction, so a confused model stops rather than looping.

**The model is a setting.** Anthropic by default, with Sonnet (`claude-sonnet-5-5`) and Haiku (`claude-haiku-5-5`) as the fast, cheap choice. OpenAI may follow as a second provider. The integration follows the current Messages API tool-use documentation, not memory.

**API keys stay in the macOS Keychain**, through the `keyring` crate, entered on a settings screen. They're never written to `kfbox.db`, config files, logs, git history or the safe-write commits, and never sent to the page: the Rust side makes every API call. The page only sees "key saved", with a remove button.

**The rules wizard.** On first run, and later from settings, the LLM writes the rules file:

1. You describe your cue habits in plain language, such as "A on the first beat, B 8 bars before the drop, C on the drop, D on the last groove". It may ask a few follow-up questions.
2. The model reads your own hand-cued tracks through `track_detail` (cues, bars and levels) and infers the pattern.
3. It writes a rules file in kfbox's real schema, which must pass `Rules::parse` before anything else happens.
4. kfbox scores the draft against your hand-cued tracks the way the `score` example does, and shows the hit rate per cue, A to D.
5. You refine it with more guidance, then save. Saving keeps the previous rules file, so it can be undone.

**Tests mock the API.** No real API call and no real key in any test.

**Built (2026-11-04):**

- **`kfbox-assist`** holds it all: the Keychain store (`keyring`, behind a trait so tests use memory), the model setting (`assistant.json`, model only), the tool loop over the Messages API, quick functions and the wizard.
- **Raw HTTP, not an SDK.** There's no official Rust SDK, so requests follow the API's raw-HTTP documentation: `x-api-key`, `anthropic-version: 2026-10-21`, tools as `input_schema`, the assistant's turn sent back whole (thinking blocks included), every tool result for a turn in one message, no forced `tool_choice` (it 400s on these models). Sonnet requests carry `fallbacks: "default"` with the `server-side-fallback-2026-07-01` beta; Haiku has no server-side fallback, so it goes without. Refusals, a 401 and other API errors come back as plain messages.
- **One tool catalogue** (`kfbox_core::tools`) serves the MCP server and the assistant. The assistant gets reading, proposing, `write_cues`, `write_rule_cues` and `undo`, but not `export_xml`. A run stops after 16 tool steps, a wizard turn after 24.
- **The wizard's tools** add `hand_cued_tracks` and `submit_rules`, which parses a draft with `Rules::parse` and scores it with `Session::score_rules`, the `score` example's method moved into the core. The best-scoring draft is kept. A failed turn leaves the conversation as it was.
- **Saving rules keeps the previous file** (`rules.previous.toml`), and restoring swaps them, so it can be undone either way.
- **Tests:** 9 for the assistant on a scripted API and a fixture library, including a scan of every file it wrote, the history included, for the key; 1 through Tauri's IPC showing no command hands the key back; 2 interface tests for the key, ⌘K and the wizard.
- **Not done:** OpenAI as a second provider, which was optional. **Not tried:** a real request with a real key, and the real Keychain (the tests use the in-memory store), so the first use is Kristian's.

### Layout B (2026-11-04)

Kristian found the menus "really out of control" and chose layout B:

- **The top bar** holds only the search field (with a ⌘K hint), the status line ("✓ Saved", "Saving…" or the error) and Export, the one primary button.
- **The ⌘K palette** is the one place for commands. Typing offers "Run on <track>" (↵) and "Run on all N in <list>" (⌘↵), the quick functions, above a filtered list of every command that left the top bar. A typed command name wins over sending the words to the model.
- **The macOS menu bar** has the same commands. File holds Open XML, Add songs, Reload and Export. Edit holds Undo and Redo, which go to a text field instead when one has focus, plus the standard text items. The kfbox menu holds Settings.
- **The window's own controls** sit in the top bar (an overlay title bar), which also drags the window.
- **The sidebar** has All tracks, Recently added (30 days), No cues, the playlists, and Prep: Rules and History, which replace the table rather than stacking under it.
- **The track view is docked** under the table at a fixed height: its header with Apply rules and zoom, a wide waveform, and the cues on one line. T folds it away, and the choice is kept between launches.
- **Settings and the wizard are sheets**, and the first-run banner is gone: the empty Rules view offers the wizard.
- **Removed with it:** the per-bar levels graph and its legend, which were for tuning the rules rather than prep.
- **Not tried in the real window:** the native menus, the overlay title bar and dragging. The browser mock has no native menu, so its keyboard handles those shortcuts itself.

### Driving kfbox without the window (added 2026-11-04)

Kristian wants kfbox drivable from MCP and later a browser, not only the Tauri window. This is in the plan as milestones 3a, 3b, 4a and 9a.

**Why 3a comes before 4.** Some real work lives in `src-tauri/src/lib.rs` today: `load_playlist`, `analyse_rekordbox`, `track_detail`, `apply_rules_to` and `export_xml`. A second front end can't reach it there. Milestone 4 adds the most commands of any milestone, so moving the work into `kfbox-core` first means those commands are written once, for every front end.

**The command layer (3a).** Each action is a plain function in `kfbox-core` with serde input and output types: load the library or a playlist, search, track detail (cues, grid, the bars the rules read), analyse, propose cues (a dry run), write cues, undo, export XML, and later export USB. Tauri, MCP and HTTP each become a thin wrapper. The player stays in the app, because it owns the audio device.

**MCP first (3b, then 4a).** `kfbox-mcp` is a headless binary that speaks MCP over stdio. It speaks the protocol itself, about a hundred lines of JSON-RPC, rather than through the `rmcp` crate: no dependency, and a test drives the real binary over its pipes. Its tools:

| Tool | Milestone | Writes? |
| --- | --- | --- |
| `list_playlists` | 3b | No |
| `search_tracks` | 3b | No |
| `playlist_tracks` | 3b | No |
| `track_detail` | 3b | No |
| `propose_cues` | 3b | No: a dry run, the default for everything |
| `export_xml` | 3b | A file the agent names, never the library |
| `write_cues` | 4a | Yes, straight into the library |
| `write_rule_cues` | 4a | Yes, into empty slots only |
| `undo` | 4a | Yes |

An agent's writes go straight in, whether or not the app is open, with no approval step: Kristian's decision on 2026-11-04. `propose_cues` is the dry run. `write_cues` writes, through exactly the app's route: refused while rekordbox runs, staged and checked, committed to the history with an archive behind it, and undoable.

**Built (2026-11-04).** 3a moved the work into `kfbox_core::session`, and the Tauri shell is now a wrapper, with 10 tests that run each command with no front end. 3b is `crates/kfbox-mcp`, with 5 tests over its pipes. On the real library, read-only, `list_playlists` takes 0.07 s and a cached `track_detail` 0.05 s, and `master.db` was byte-identical before and after.

**Live reload (4a).** The open app has to show a write it didn't make. The simple version watches the library's history for a new commit, and `master.db` for a change made by rekordbox, then rereads the library and the tracks that changed. This comes in 4a, with agent writes, rather than waiting for 7a.

**Built (4a).** `kfbox-mcp` now offers every tool (`Toolset::Agent`), labelled `Agent:` in the history. The app runs a watcher thread (`src-tauri/src/watch.rs`) that stats `master.db` (modified time, size and inode, since a safe write swaps in a new file) and the history's reflog every 0.7 s. When `master.db` moved, `Session::refresh_if_changed` reads the library again and returns the rows that differ and the IDs that went, and the page updates those tracks in place, or reads the list again if tracks or playlists came or went. kfbox's own writes already update the session, so they aren't reported twice. Tests: four in `crates/kfbox-mcp/tests/agent_writes.rs` drive the real binary with `--fixture` beside an in-process session on the same library (the app sees the agent's write and its undo; the agent sees the app's write and can't take its slot; a held lock makes the agent wait about 2 s and then fail as busy; six interleaved writes from both sides all land), one Tauri test checks that a write from another session reaches the page as exactly one `library-changed` event, and one store test checks the page updates the open track and the history.

**Early drops (2026-11-04).** On the 88 tracks with no hand cues, the drop lands before bar 64 on 24, earlier than any drop Kristian cued (NOTES.md). `Rules::apply` now returns `flags`: an `early_drop` flag when a cue the rules place from the drop (B and C by default) sits on a drop before `[find] doubt_drop_before` (64, from 0). A hand-cued B or C means the drop is Kristian's, so no flag. The sidebar's **Early drops** list and an *early drop?* mark on the row show them, `propose_cues` returns the same `flags` with a sentence of why, and nothing in the library changes.

**Learning from fixes (2026-11-04).** kfbox records each cue the rules, an agent or the assistant writes (`learning.db`, table `placed`), and when a person moves or deletes one, in kfbox or in rekordbox, it records a correction: track, slot, the seconds and bar it was placed at, where it went (none for a delete), who placed it, and where the fix was made. `learning.db` sits beside `kfbox.db` rather than in it, because the cache is deleted and rebuilt when it can't be read, and fixes can't be made again. Every kfbox write brings the record in step while it still holds the library's lock, and the watcher waits while that lock is held, so whatever differs when the library is read again was done outside kfbox. An agent moving a placed cue isn't a fix: the record follows it. An undo forgets cues it removes rather than counting them as deleted. `score_rules` scores each slot on the latest fix per track and slot too (a hit lands in the bar the person chose, or stays out of the bar of one they deleted), and the wizard has a `fixes` tool and shows those hits. One gap: undoing a fix leaves the correction recorded.

**Conflicts stay light.** Kristian expects he and an agent will rarely clash. The library's lock in `kfbox_core::safe` already stops two processes writing at once: a write that finds the other side holding it fails as busy, and the caller retries. For that to work, the app takes the lock only for the length of a write, never for as long as it's open, or a running app would block every agent write. That's a rule for milestone 4 to keep.

**The app as a server (9a).** The running app serves the same commands on localhost behind a token. A browser page can use that endpoint, playing through the app's audio, and an agent can talk to the app directly instead of running its own MCP process. Listening stays inside the app; nothing is reachable from another machine. It comes after USB export, so the features 1.0 needs come first, but before milestone 8, so the two-week test covers it.

### Notes from milestone 2

Playback uses `rbl-deck` without Rubber Band: tempo stays at 1, so kfbox doesn't need it yet. A test runs a real file through the engine with a silent output: loading, sound, seeking, looping and the end of the track. The mock covers the interface. What neither can prove is sound through the real audio device, which needs Kristian's ears. Scrubbing is wired through but has no automated test of how it sounds.

Milestone 2's branch is built on milestone 1's, because both change the shell. Milestone 3's branch comes straight from milestone 0.

### Notes from milestone 1

Sorting and search run in the interface rather than through `rbl-index`'s views, and the table draws its own window of rows rather than using TanStack Virtual: both stay fast well past this library's size, a test covers 5,000 rows, and neither needs a dependency. Revisit if the library reaches tens of thousands of tracks.

### Checks for milestone 3

- Kill the app at each step of a write, then confirm the library comes back whole and intact.
- Restore from every layer, and open the result with `PRAGMA integrity_check`.
- Use `rbl-difftool` to compare kfbox's cue rows with the rows rekordbox writes for the same edit. Any difference is a bug.
- Refuse to write while rekordbox is running, including if it starts during a batch.
- Run all of the above on `rbl-db` fixtures and copies of your library, never the real one, until the final check.

Built on 2026-10-31 as `kfbox_core::safe`, with these checks passing on fixtures and on a temporary copy of the real library (`safe_check` example): the kill tests at 9 write steps and 8 undo steps, restore from every layer, refusal if rekordbox is running or starts mid-write, and a 160-op batch in 4.1 s. Nothing in the app calls it yet.

Three things differ from the design above:

- **A staged copy gives the atomic batch, not an `rbl-db` addition.** Each change goes to a copy of `master.db`, gets read back, and replaces the library in one rename. The crates stay as they are upstream.
- **Archives are plain folders, not `rbl-backup` archives.** That crate only reads archives; writing them lives in rbxport's app. APFS shares the copied files' blocks, so a daily archive costs little.
- **The comparison with rekordbox is partial.** Without driving rekordbox, the check compares column types with the rows rekordbox wrote. They match except `BeatLoopSize`: this library has no hot cue loops, so it's the one thing rekordbox itself has to confirm. `rbl-difftool` needs rekordbox making the same edit.

History needs the `git` command on the machine, which a release build should bundle or replace.

The real check ran on 2026-11-01 (`safe_gate --write`, three tracks, 12 cues). The cues survived rekordbox opening and saving the library, and Kristian found them well placed. He didn't confirm the loop or colours on screen. The default rules place plain cues, so those stay unproven until kfbox needs them.

The check also found the swap had changed `master.db` from `rw-r--r--` to `rw-------`. The code now keeps the file's permissions; the real file still has the changed ones, unless Kristian has restored them.

The first part of the gate has passed. On 2026-10-31, hot cues A to D went onto one real track (`write_one` example, `Writer::add_cue`, rekordbox closed), Kristian confirmed them in rekordbox, and the rows read back unchanged afterwards. That covers plain cues on one track only. Colours, names, loops, batches and USB are still to prove.

## Risks

| Risk | What happens | What we do |
| --- | --- | --- |
| The database format isn't documented, and rekordbox can change it | A rekordbox update breaks writes | Check the schema at launch; fall back to read-only if it's unknown; rely on `rbl-difftool` to catch changes |
| rekordbox rejects or rewrites what kfbox wrote | Cues vanish or a track breaks | The one-track gate in milestone 3, then git restore; test again after each rekordbox update |
| rbxport's crates change | Rebuild work | Pinned revision, upgraded on purpose |
| `rbl-analysis` grids differ from rekordbox's | New imports have weaker grids | Existing tracks keep rekordbox's grid; grid editing ships in 6 |
| Automatic cues stay wrong on some tracks | Fixing them by hand takes longer | The numbers so far: 59 of 72 on tracks used for tuning, nearer 11 of 18 per cue on tracks left out. Editing in kfbox and the LLM menu fix them, and 24 uncued tracks with drops before bar 64 still need a look |
| Scope keeps growing towards all of rekordbox | 1.0 never ships | The "Not in 1.0" column stays out until after the two-week test |

## Not yet known

These haven't been tried:

- How rekordbox shows colours and loops written straight to `master.db`. It kept plain named cues written in a batch on three tracks (2026-11-01), but Kristian didn't check the colours or the loop on screen. Whether such cues reach a USB or CDJ is also untested.
- Whether the real app window behaves as expected. It hasn't been driven end to end, because screen access was declined. The mock and browser tests cover most of the interface, but not the Tauri calls, dialogs or file drops; your own use covers those.
- How `rbl-export` USBs behave on your CDJs.

## Questions for Kristian

Answered on 2026-10-31: build on rbxport's crates, not a fork; dropped files stay where they are; the first commit is made; Kristian uses no cloud or mobile sync, so neither needs testing.

Answered on 2026-11-04: an agent's writes go straight in, with no approval step, and the open app reloads to show them.

Still open:

1. **What are your My Tag categories and values, beyond groove?** Milestone 8 needs them.
2. **Which CDJs or XDJs do you export for?** Milestone 9 tests on them.
