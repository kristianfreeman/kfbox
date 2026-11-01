# kfbox 1.0 plan

kfbox 1.0 replaces rekordbox for everything you do before a gig: browse, listen, cue, tag, import, and export a USB. You can work for weeks without opening rekordbox, and the automatic cues become suggestions you review in a real music player.

Status on 2026-11-01: milestones 0 to 3 are built and merged on `milestone-1-3`, not yet on `main`. The app still only reads `master.db`: nothing in it calls the safe write path yet.

## What 1.0 includes

| In 1.0 | Not in 1.0 |
| --- | --- |
| Library: search, sort, playlists, folders, intelligent playlists | Two-deck mixing and Performance mode |
| Player: play, seek, scrub, loops, hot cue jumps, queue, zoomable waveform | PRO DJ LINK (rbxport has the crates, so it can come later) |
| Cues: click, drag, and keyboard editing; automatic suggestions to accept or reject | Cloud sync, stems, lighting, recording |
| Import: drag in files and folders, analysed once | Windows |
| Editing: My Tags, rating, colour, comments, beat grid nudge and BPM | Mood tagging, which you ruled out |
| Automatic My Tags, starting with groove | |
| USB export for CDJs | |
| Backups: history of every change, undo, restore | |

If you perform from a laptop in rekordbox, you'd still use it for that. For USB sets, 1.0 replaces it completely.

## Decisions

1. **Build on rbxport's crates, not a fork of its app (decided).** rbxport's README says it stays small on purpose, and its interface follows rekordbox, which you don't want. Its 49,000-line front end also carries LINK, translations and an updater that 1.0 doesn't need. kfbox adds `rbl-deck`, `rbl-backup`, `rbl-export` and `rbl-devices`, and adapts rbxport's shell code where it fits: `player.rs`, `cues.rs`, `grid.rs`, `file_drop.rs`, `file_journal.rs` and the waveform canvas. The crates are pinned to a revision of `chrisle/rbxport`, so upstream changes arrive when we choose.
2. **`master.db` stays the library.** kfbox doesn't invent a second one, so rekordbox still opens the same collection if kfbox ever fails you. kfbox's own data (cache, suggestions, settings) lives in its own `kfbox.db`.
3. **React and TypeScript replace the vanilla page.** The current 530-line page can't carry a virtual table, focus handling and a player. React plus TanStack Virtual also lets us adapt rbxport's parts directly. Backend calls go through one typed layer with a mock, so the UI can be tested in a browser without touching your library.
4. **Geist everywhere, bundled with the app.** Geist for text, Geist Mono for BPM, times and bars. It loads from the app, not the network.
5. **Automatic cues are proposals.** Rules never write straight to the library. You see them as ghost cues and accept them. Every accept, move or reject is recorded, which grows the 18 tracks we can score against.

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
2. **A git commit for every action.** A private local repository holds `master.db`, the changed analysis files, and a readable JSON file per track (cues, tags, rating). That JSON gives real diffs and per-track undo. Commit messages say what happened, such as `Accept suggested cues on 40 tracks (rules 3f2a)`. It's never pushed anywhere, because the JSON is your library in plain text.
3. **Full archives.** `rbl-backup` writes a complete archive of `master.db`, analysis files and artwork before the first write of each day, kept outside the git repository. If git itself breaks, these still restore the library.
4. **Spotting rekordbox's changes.** Before each write, kfbox compares the library on disk with the last commit. If rekordbox changed something, kfbox commits that first as `Changed outside kfbox`, so its own undo can't erase your work.

Every write follows the same order: check rekordbox is closed, snapshot if needed, apply in one transaction, read it back, run `PRAGMA integrity_check`, commit to git. If rekordbox is open, your edits wait in `kfbox.db` under a banner and apply when you quit it.

Cmd-Z undoes the last action. A History panel lists every action and can restore the library to any point. A restore is itself a commit, so it can be undone too.

## The interface

It should read as a music player: calm, type-led, with the track you're hearing always in view.

- **Sidebar:** All Tracks, Needs Review, Recently Added, Missing Files, then your playlists and folders. Drag tracks here to add them to a playlist.
- **Track table:** title, artist, BPM, key, time, tags and cue status. It's virtualised, with sortable columns and instant search.
- **Now playing, always at the bottom:** artwork, title, transport, overview waveform, and hot cue buttons.
- **Track view, opened with Enter or a double click:** large zoomable waveform, beat grid, cue list, tags, and the bars the rules read.
- **Review mode:** one track at a time with its ghost cues. Play from each cue, drag it if it's wrong, accept, and move to the next track. This is how the 88 uncued tracks get done.
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
| Y, N | Accept or reject suggestions |
| J, K | Next or previous track to review |
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
| 3 | Safe writes: batch transactions, git history, archives, restore, crash recovery | L | 0 | Passes the checks below on a copy; **a real batch with colours, names and loops looks right in rekordbox** |
| 4 | Cues and review: editing, ghost cues, review mode, undo, History panel | L | 1, 2, 3 | You've reviewed a playlist and every change undoes |
| 5 | Import: drag and drop, analysis files, duplicates, missing files | M | 3, 4 | A dropped track plays, gets suggestions, and loads correctly in rekordbox |
| 6 | Library editing: playlists, tags, rating, grid, automatic My Tags | M | 3, 4 | Edits survive a round trip through rekordbox |
| 7 | USB export: `rbl-export`, drive picker, progress | M | 6 | **A USB plays with cues on your CDJs** |
| 8 | 1.0: performance budgets, signed build, drop finder retuned on your review data | M | All | You go two weeks of prep and a gig without opening rekordbox |

Milestones 1, 2 and 3 can run in parallel after 0, each in its own session and worktree, because they touch separate code once the shell exists. Milestones 4 onward run mostly in sequence. Start with 3 if you want one first: it's the riskiest, and everything that writes depends on it.

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
| Automatic cues stay wrong on some tracks | Review takes longer | The numbers so far: 59 of 72 on tracks used for tuning, nearer 11 of 18 per cue on tracks left out. Review mode is the answer, and 24 uncued tracks with drops before bar 64 still need a look |
| Scope keeps growing towards all of rekordbox | 1.0 never ships | The "Not in 1.0" column stays out until after the two-week test |

## Not yet known

These haven't been tried:

- How rekordbox handles anything beyond plain hot cues on one track. It accepted those. Colours, names, loops and batches written straight to `master.db` are untested, and so is whether such cues reach a USB or CDJ.
- Whether the real app window behaves as expected. It hasn't been driven end to end, because screen access was declined. The mock and browser tests cover most of the interface, but not the Tauri calls, dialogs or file drops; your own use covers those.
- How `rbl-export` USBs behave on your CDJs.

## Questions for Kristian

Answered on 2026-10-31: build on rbxport's crates, not a fork; dropped files stay where they are; the first commit is made; Kristian uses no cloud or mobile sync, so neither needs testing.

Still open:

1. **What are your My Tag categories and values, beyond groove?** Milestone 6 needs them.
2. **Which CDJs or XDJs do you export for?** Milestone 7 tests on them.
