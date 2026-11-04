import { useSyncExternalStore } from "react";

import type { Backend } from "../ipc/backend";
import type {
  Analysis,
  Applied,
  AssistantSettings,
  Cue,
  CueEdit,
  HistoryEntry,
  ImportReport,
  LibraryChange,
  Listing,
  PlaylistItem,
  QuickResult,
  Saved,
  Track,
  TrackRow,
  WizardTurn,
} from "../ipc/types";
import { fileName } from "../lib/format";
import { Player } from "./player";

const AUDIO = ["mp3", "wav", "aif", "aiff", "flac", "m4a", "mp4", "aac"];

/** How many cues the rules would add to an entry: slots they fill that are empty now. */
export const ruleCuesFor = (entry: Entry) => entry.applied?.cues.filter((c) => !c.existing).length ?? 0;

const slotOf = (cue: Cue) => (cue.hot === null ? "memory" : String.fromCharCode(65 + cue.hot));

/** A song in the list: waiting, being analysed, done, or failed. */
export interface Entry {
  /** The path, or `rb:` and the TrackID for a collection entry with none. */
  key: string;
  path: string;
  /** The TrackID, for a song from rekordbox. */
  id?: string;
  row: TrackRow;
  analysis?: Analysis;
  applied?: Applied;
  error?: string;
  busy?: boolean;
}

/** What the main area shows: the track table, the rules, or the history. */
export type View = "tracks" | "rules" | "history";

/** The library's own lists, beside the playlists. */
/** The dialogs over the window. */
export type Sheet = "settings" | "wizard" | "import";

export type Smart = "all" | "recent" | "nocues" | "early" | "missing";

/** The library's own lists, as the sidebar names them. */
export const SMART_LABELS: Record<Smart, string> = {
  all: "All tracks",
  recent: "Recently added",
  nocues: "No cues",
  early: "Early drops",
  missing: "Missing files",
};

/** How far back "Recently added" looks. */
const RECENT_DAYS = 30;

const TRACK_OPEN_KEY = "kfbox.trackOpen";

/** The track view's open or closed state, kept between launches where storage allows. */
function storedTrackOpen(): boolean {
  try {
    return globalThis.localStorage?.getItem(TRACK_OPEN_KEY) !== "false";
  } catch {
    return true;
  }
}

function storeTrackOpen(open: boolean) {
  try {
    globalThis.localStorage?.setItem(TRACK_OPEN_KEY, String(open));
  } catch {
    // Not kept: it opens next time, which is the default anyway.
  }
}

/** The rules placed a drop earlier than any cued by hand on this track. */
export const hasEarlyDrop = (entry: Entry) => entry.applied?.flags.some((f) => f.kind === "early_drop") ?? false;

/** One line on what an import did. */
export function importSummary({ imported, already, duplicates, problems }: ImportReport): string {
  const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
  const copies = duplicates.filter((d) => d.kind === "copy").length;
  const likely = duplicates.length - copies;
  const parts = [`Imported ${plural(imported.length, "track")}`];
  if (already.length) parts.push(`${already.length} already in the library`);
  if (copies) parts.push(`${plural(copies, "copy")} of tracks you have skipped`.replace("copys", "copies"));
  if (likely) parts.push(`${plural(likely, "likely duplicate")} to check`);
  if (problems.length) parts.push(`${plural(problems.length, "problem")}`);
  return parts.join(" · ");
}

const isRecent = (row: TrackRow, now: number) => row.added !== "" && now - Date.parse(row.added) <= RECENT_DAYS * 86_400_000;

export type SortColumn = "title" | "artist" | "bpm" | "key" | "time" | "genre";

const SORT_VALUE: Record<SortColumn, (row: TrackRow) => string | number> = {
  title: (row) => row.title.toLowerCase(),
  artist: (row) => row.artist.toLowerCase(),
  bpm: (row) => row.bpm,
  key: (row) => row.key,
  time: (row) => row.duration_secs,
  genre: (row) => row.genre.toLowerCase(),
};

const haystack = ({ title, artist, album, genre, key }: TrackRow) => `${title} ${artist} ${album} ${genre} ${key}`.toLowerCase();

function handAdded(path: string): TrackRow {
  return {
    id: "",
    path,
    title: fileName(path),
    artist: "",
    album: "",
    genre: "",
    bpm: 0,
    key: "",
    duration_secs: 0,
    added: "",
    hot: "",
    memory: 0,
    missing: false,
  };
}

/** The app's state and everything that changes it. Views read it and call its methods. */
export class Store {
  entries: Entry[] = [];
  /** The entries on screen: filtered by the search, then sorted. */
  rows: Entry[] = [];
  selected: string | null = null;
  status = "";
  total: number | null = null;
  playlists: PlaylistItem[] = [];
  /** The sidebar's choice, by path so a reload can find it again. `null` is the whole collection. */
  chosenPath: string | null = null;
  /** The cue picked in the track view, by ID. */
  selectedCue: string | null = null;
  /** A write is under way. */
  saving = false;
  /** Why the last write failed, until the next one. */
  writeError = "";
  history: HistoryEntry[] = [];
  /** What the main area shows. */
  view: View = "tracks";
  /** Which of the library's own lists is chosen, when no playlist is. */
  smart: Smart = "all";
  /** How many tracks each of the library's own lists holds. */
  counts = { all: 0, recent: 0, nocues: 0, early: 0, missing: 0 };
  /** Which sheet is open over the window, if any. */
  sheet: Sheet | null = null;
  /** The last write landed, and nothing has happened since. */
  justSaved = false;
  assistant: AssistantSettings | null = null;
  /** Rules have never been saved, so the Rules view offers the wizard. */
  firstRun = false;
  quickOpen = false;
  quickBusy = false;
  quickResult: QuickResult | null = null;
  quickError = "";
  wizardBusy = false;
  wizardTurn: WizardTurn | null = null;
  wizardError = "";
  query = "";
  sort: { column: SortColumn; descending: boolean } | null = null;
  /** Whether the track view is docked open under the table. */
  trackOpen = storedTrackOpen();
  /** The whole of the selected track, while the track view is open. */
  detail: Track | null = null;
  /** The closer waveform of [`detail`], once it has arrived. */
  detailWaveform: number[] | null = null;
  /** How far the track view is zoomed in: 1 shows the whole track. */
  zoom = 1;
  readonly player: Player;
  rulesText = "";
  rulesError = "";

  private version = 0;
  private listeners = new Set<() => void>();
  private queue: Promise<void> = Promise.resolve();
  // Bumped when a playlist replaces the list, so queued analyses of the old
  // list are dropped.
  private generation = 0;
  /** rekordbox's own library is open, so dropped songs are imported into it. */
  libraryOpen = false;
  /** The last import's report, for the skipped and the problems. */
  importReport: ImportReport | null = null;
  // The rekordbox XML last opened: the export is offered next to it.
  private openedXml: string | null = null;

  constructor(readonly backend: Backend) {
    this.player = new Player(backend, () => this.changed());
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getVersion = () => this.version;

  private changed() {
    this.version += 1;
    for (const listener of this.listeners) listener();
  }

  get current(): Entry | undefined {
    return this.entries.find((e) => e.key === this.selected);
  }

  get canExport(): boolean {
    return this.entries.some((e) => e.analysis);
  }

  /** Loads the rules, then the songs named at launch or else the library. */
  async start() {
    this.backend.onFileDrop((paths) => this.drop(paths));
    this.backend.onMenu((id) => void this.menu(id));
    this.backend.onLibraryChanged((change) => void this.libraryChanged(change));
    this.backend.onHistoryChanged(() => {
      if (this.view === "history") void this.loadHistory();
    });
    this.rulesText = await this.backend.loadRules();
    this.firstRun = !(await this.backend.hasSavedRules().catch(() => true));
    this.assistant = await this.backend.assistantSettings().catch(() => null);
    this.changed();
    const named = await this.backend.initialPaths();
    if (named.length) this.addPaths(named);
    else await this.openLibrary();
  }

  private setStatus(text: string) {
    this.status = text;
    this.changed();
  }

  private drop(paths: string[]) {
    const xml = paths.find((p) => p.toLowerCase().endsWith(".xml"));
    if (xml) void this.openXml(xml);
    else if (this.libraryOpen) void this.importPaths(paths);
    else this.addPaths(paths);
  }

  /**
   * Adds songs, or folders of them, to rekordbox's library where they are,
   * analysed and cued, then shows them under Recently added.
   */
  async importPaths(paths: string[]) {
    if (this.saving || paths.length === 0) return;
    this.saving = true;
    this.justSaved = false;
    this.writeError = "";
    this.setStatus(`Importing ${paths.length === 1 ? fileName(paths[0]!) : `${paths.length} items`}…`);
    try {
      const report = await this.backend.importFiles(paths);
      this.importReport = report;
      // The summary says more than "Saved" would.
      this.setStatus(importSummary(report));
      // Anything left out gets a list, with why.
      if (report.already.length || report.duplicates.length || report.problems.length) this.sheet = "import";
      const listing = await this.backend.openLibrary();
      this.view = "tracks";
      this.smart = "recent";
      this.chosenPath = null;
      await this.showPlaylists(listing);
      const first = report.imported[0];
      if (first) this.select(this.entries.find((e) => e.id === first.id)?.key ?? this.selected ?? "");
    } catch (e) {
      this.writeError = String(e);
    } finally {
      this.saving = false;
      this.changed();
    }
  }

  /** Asks for a folder and points missing tracks at the files of the same name in it. */
  async findMissing() {
    const folder = await this.backend.pickFolder();
    if (!folder) return;
    const saved = await this.write(() => this.backend.relocateMissing(folder));
    if (saved)
      this.setStatus(saved.changed.length ? `Found ${saved.changed.length} missing files` : "None of the missing files are in that folder");
  }

  // ---- what's on screen ---------------------------------------------------

  private refreshRows() {
    const words = this.query.toLowerCase().split(/\s+/).filter(Boolean);
    let rows = this.entries;
    if (this.chosenPath === null) {
      const now = Date.now();
      this.counts = {
        all: this.entries.length,
        recent: this.entries.filter((e) => isRecent(e.row, now)).length,
        nocues: this.entries.filter((e) => e.row.hot === "").length,
        early: this.entries.filter(hasEarlyDrop).length,
        missing: this.entries.filter((e) => e.row.missing).length,
      };
      if (this.smart === "recent") rows = rows.filter((e) => isRecent(e.row, now)).sort((a, b) => b.row.added.localeCompare(a.row.added));
      if (this.smart === "nocues") rows = rows.filter((e) => e.row.hot === "");
      if (this.smart === "early") rows = rows.filter(hasEarlyDrop);
      if (this.smart === "missing") rows = rows.filter((e) => e.row.missing);
    }
    if (words.length) {
      rows = rows.filter((entry) => {
        const text = haystack(entry.row);
        return words.every((word) => text.includes(word));
      });
    }
    if (this.sort) {
      const value = SORT_VALUE[this.sort.column];
      const direction = this.sort.descending ? -1 : 1;
      rows = [...rows].sort((a, b) => {
        const [x, y] = [value(a.row), value(b.row)];
        return (x < y ? -1 : x > y ? 1 : 0) * direction;
      });
    }
    this.rows = rows;
    // The selection follows what's on screen.
    if (!rows.some((e) => e.key === this.selected) && rows[0]) {
      this.selected = rows[0].key;
      void this.loadDetail();
    }
  }

  setQuery(query: string) {
    this.query = query;
    this.refreshRows();
    this.changed();
  }

  /** Sorts by a column: ascending, then descending, then back to the list's own order. */
  sortBy(column: SortColumn) {
    if (this.sort?.column !== column) this.sort = { column, descending: false };
    else if (!this.sort.descending) this.sort = { column, descending: true };
    else this.sort = null;
    this.refreshRows();
    this.changed();
  }

  select(key: string) {
    if (key !== this.selected) this.selectedCue = null;
    this.selected = key;
    this.changed();
    void this.loadDetail();
  }

  /** Moves the selection by whole rows, stopping at either end. */
  moveSelection(delta: number) {
    const index = this.rows.findIndex((e) => e.key === this.selected);
    const next = this.rows[Math.max(0, Math.min(this.rows.length - 1, index + delta))];
    if (next) this.select(next.key);
  }

  selectEdge(last: boolean) {
    const next = this.rows[last ? this.rows.length - 1 : 0];
    if (next) this.select(next.key);
  }

  openTrack() {
    this.trackOpen = true;
    storeTrackOpen(true);
    this.changed();
    void this.loadDetail();
  }

  /** Shows or hides the track view, so the table can have the full height. */
  toggleTrack() {
    if (this.trackOpen) this.closeTrack();
    else this.openTrack();
  }

  closeTrack() {
    this.trackOpen = false;
    storeTrackOpen(false);
    this.detail = null;
    this.detailWaveform = null;
    this.changed();
  }

  /** Zooms the track view in by a factor, between the whole track and 64 times closer. */
  zoomBy(factor: number) {
    this.zoom = Math.max(1, Math.min(64, this.zoom * factor));
    this.changed();
    if (this.zoom > 1) void this.loadWaveform();
  }

  private async loadWaveform() {
    const track = this.detail;
    if (!track || this.detailWaveform) return;
    try {
      const columns = await this.backend.waveform(track.path);
      if (this.detail?.path === track.path) {
        this.detailWaveform = columns;
        this.changed();
      }
    } catch {
      // The overview still draws, just coarser.
    }
  }

  /** Plays the selected track from its start, or goes on with it if it's already loaded. */
  playSelected() {
    const entry = this.current;
    if (!entry?.path) return;
    if (this.player.key === entry.key) {
      if (!this.player.playing) void this.player.toggle();
      return;
    }
    void this.player.load(entry, true);
  }

  /** Space: plays or pauses what's loaded, or starts the selected track. */
  togglePlay() {
    if (this.player.key) void this.player.toggle();
    else this.playSelected();
  }

  enqueueSelected() {
    const entry = this.current;
    if (entry?.path) this.player.enqueue(entry);
  }

  /** Clicking a waveform: moves the playhead, loading that track first if needed. */
  seekInView(secs: number) {
    const entry = this.current;
    if (!entry?.path) return;
    if (this.player.key === entry.key) void this.player.seek(secs);
    else void this.player.load(entry, false, secs);
  }

  private async loadDetail() {
    if (!this.trackOpen) return;
    const entry = this.current;
    if (!entry?.analysis) {
      this.detail = null;
      this.changed();
      return;
    }
    const key = entry.key;
    try {
      const track = await this.backend.trackDetail(entry.path);
      // The selection may have moved while this was on its way.
      if (this.selected === key) {
        if (this.detail?.path !== track.path) this.detailWaveform = null;
        this.detail = track;
        this.changed();
        if (this.zoom > 1) void this.loadWaveform();
      }
    } catch (e) {
      entry.error = String(e);
      this.changed();
    }
  }

  // ---- loading ------------------------------------------------------------

  addPaths(paths: string[]) {
    const fresh = [...new Set(paths)].filter(
      (p) => AUDIO.includes(p.split(".").pop()?.toLowerCase() ?? "") && !this.entries.some((e) => e.path === p),
    );
    for (const path of fresh) this.entries.push({ key: path, path, row: handAdded(path), busy: true });
    this.selected ??= fresh[0] ?? null;
    this.refreshRows();
    this.changed();
    this.queueAnalysis(fresh);
  }

  async pickSongs() {
    const paths = await this.backend.pickAudio(AUDIO);
    if (this.libraryOpen) await this.importPaths(paths);
    else this.addPaths(paths);
  }

  async pickXml() {
    const path = await this.backend.pickXml();
    if (path) await this.openXml(path);
  }

  private queueAnalysis(keys: string[]) {
    const mine = this.generation;
    for (const key of keys) {
      // One at a time: analysis already uses every core.
      this.queue = this.queue.then(() => (mine === this.generation ? this.analyse(key) : undefined));
    }
  }

  private async analyse(key: string) {
    const entry = this.entries.find((e) => e.key === key);
    if (!entry) return;
    const done = this.entries.filter((e) => !e.busy).length;
    this.status = `Analysing ${done + 1} of ${this.entries.length}: ${entry.row.title}`;
    try {
      entry.analysis = entry.id ? await this.backend.analyseRekordbox(entry.id) : await this.backend.analyse(entry.path);
      if (!entry.id) {
        const { title, artist, bpm, key: musicalKey, duration_secs } = entry.analysis;
        entry.row = { ...entry.row, title, artist, bpm, key: musicalKey ?? "", duration_secs };
      }
      entry.applied = await this.backend.applyRulesTo(entry.path, this.rulesText).catch(() => entry.applied);
    } catch (e) {
      entry.error = String(e);
    }
    entry.busy = false;
    if (!this.entries.some((e) => e.busy)) {
      this.status = `${this.entries.filter((e) => e.analysis).length} tracks ready`;
    }
    if (this.selected === key) void this.loadDetail();
    this.player.entryChanged(entry);
    this.flagsChanged();
    this.changed();
  }

  /** The early-drop count follows analysis as it goes, and so does that list when it's on screen. */
  private flagsChanged() {
    this.counts = { ...this.counts, early: this.entries.filter(hasEarlyDrop).length };
    if (this.chosenPath === null && this.smart === "early") this.refreshRows();
  }

  /** Loads rekordbox's own library and offers its playlists. */
  async openLibrary() {
    this.setStatus("Reading the rekordbox library…");
    try {
      const listing = await this.backend.openLibrary();
      this.openedXml = null;
      this.libraryOpen = true;
      await this.showPlaylists(listing);
    } catch (e) {
      this.setStatus(String(e));
    }
  }

  async openXml(path: string) {
    try {
      const listing = await this.backend.openXml(path);
      this.openedXml = path;
      this.libraryOpen = false;
      await this.showPlaylists(listing);
    } catch (e) {
      this.setStatus(String(e));
    }
  }

  /** Lists the playlists and loads the choice it had, or the whole collection. */
  private async showPlaylists({ total, playlists }: Listing) {
    this.total = total;
    this.playlists = playlists;
    const kept = playlists.find((p) => !p.folder && p.path === this.chosenPath);
    this.chosenPath = kept?.path ?? null;
    this.changed();
    await this.loadPlaylist(kept?.index ?? null);
  }

  /** One of the library's own lists: everything, the recently added, or the tracks with no hot cues. */
  async chooseSmart(smart: Smart) {
    this.view = "tracks";
    this.smart = smart;
    if (this.chosenPath !== null) {
      await this.choose(null, smart);
      return;
    }
    this.refreshRows();
    this.changed();
  }

  /** Shows the rules or the history in the main area, or the tracks again. */
  async chooseView(view: View) {
    this.view = view;
    this.changed();
    if (view === "history") await this.loadHistory();
  }

  choose(playlist: PlaylistItem | null, smart: Smart = "all") {
    this.view = "tracks";
    this.smart = smart;
    this.chosenPath = playlist?.path ?? null;
    this.changed();
    return this.loadPlaylist(playlist?.index ?? null);
  }

  private async loadPlaylist(index: number | null) {
    this.generation += 1;
    const mine = this.generation;
    // Queued analyses of the old list now skip themselves. The one under way
    // has to finish first, or its track lands in the new list's export.
    await this.queue;
    if (mine !== this.generation) return;
    try {
      const tracks = await this.backend.loadPlaylist(index);
      this.entries = tracks.map((row) =>
        !row.path
          ? { key: `rb:${row.id}`, path: "", id: row.id, row, error: "no file location in the XML" }
          : row.missing
            ? { key: row.path, path: row.path, id: row.id, row, error: "the file isn't where the library says" }
            : { key: row.path, path: row.path, id: row.id, row, busy: true },
      );
      this.detail = null;
      this.selected = this.entries[0]?.key ?? null;
      this.refreshRows();
      this.changed();
      this.queueAnalysis(this.entries.filter((e) => e.busy).map((e) => e.key));
    } catch (e) {
      this.setStatus(String(e));
    }
  }

  // ---- writing -----------------------------------------------------------

  /** Runs a write, then brings the tracks it changed up to date. */
  private async write(action: () => Promise<Saved>): Promise<Saved | null> {
    if (this.saving) return null;
    this.saving = true;
    this.justSaved = false;
    this.writeError = "";
    this.changed();
    try {
      const saved = await action();
      this.justSaved = saved.commit !== null;
      await this.refreshChanged(saved.changed);
      if (this.view === "history") await this.loadHistory();
      return saved;
    } catch (e) {
      this.writeError = String(e);
      return null;
    } finally {
      this.saving = false;
      this.changed();
    }
  }

  /** Takes a write's changed tracks: their rows, and their cues placed again. */
  private async refreshChanged(rows: TrackRow[]) {
    for (const row of rows) {
      const entry = this.entries.find((e) => e.id === row.id);
      if (!entry) continue;
      entry.row = row;
      try {
        entry.analysis = await this.backend.analyseRekordbox(row.id);
        entry.applied = await this.backend.applyRulesTo(entry.path, this.rulesText);
        // A missing file that was found again now analyses like any other.
        entry.error = undefined;
      } catch (e) {
        entry.error = String(e);
      }
      this.player.entryChanged(entry);
      if (entry.key === this.selected) void this.loadDetail();
    }
    this.refreshRows();
  }

  /**
   * Something else wrote to the library, such as an agent: the changed tracks
   * are brought up to date in place, or the list is read again when tracks or
   * playlists came or went.
   */
  private async libraryChanged({ listing, changed, removed }: LibraryChange) {
    const known = new Set(this.entries.map((e) => e.id));
    const shape = (items: PlaylistItem[]) => JSON.stringify(items.map((p) => [p.path, p.folder, p.count]));
    const reshaped =
      listing.total !== this.total ||
      shape(listing.playlists) !== shape(this.playlists) ||
      removed.some((id) => known.has(id)) ||
      (this.chosenPath === null && changed.some((row) => !known.has(row.id)));
    const count = changed.length;
    this.setStatus(`The library changed outside kfbox: ${count} ${count === 1 ? "track" : "tracks"} updated`);
    if (reshaped) await this.showPlaylists(listing);
    else await this.refreshChanged(changed);
    if (this.view === "history") await this.loadHistory();
    this.changed();
  }

  /** The selected entry, if it's a track of rekordbox's library, which is all that can be saved to. */
  private get savable(): Entry | undefined {
    const entry = this.current;
    if (!entry?.id) {
      this.writeError = "Only tracks of rekordbox's library can be saved to.";
      this.changed();
      return undefined;
    }
    return entry;
  }

  saveCues(summary: string, edits: CueEdit[]) {
    return this.write(() => this.backend.saveCues(summary, edits));
  }

  /** Tracks of rekordbox's library in the list whose empty slots the rules would fill. */
  get ruleTargets(): Entry[] {
    return this.entries.filter((e) => e.id && ruleCuesFor(e) > 0);
  }

  private writeRuleCues(entries: Entry[]) {
    const ids = entries.map((e) => e.id).filter((id): id is string => Boolean(id));
    if (ids.length === 0) return Promise.resolve(null);
    return this.write(() => this.backend.writeRuleCues(ids, this.rulesText));
  }

  /** Writes the rules' cues on the selected track, into its empty slots. */
  applyRulesToSelected() {
    const entry = this.savable;
    if (!entry) return Promise.resolve(null);
    return this.writeRuleCues([entry]);
  }

  /** Writes the rules' cues on every track in the list, in one change. */
  applyRulesToList() {
    return this.writeRuleCues(this.ruleTargets);
  }

  /** Sets a hot cue at the playhead of the track playing, snapped to the nearest beat, replacing the one in that slot. */
  setCueAtPlayhead(slot: string) {
    const entry = this.player.entry;
    if (!entry?.id) {
      this.writeError = entry ? "Only tracks of rekordbox's library can be saved to." : "Play a track first.";
      this.changed();
      return;
    }
    const secs = this.nearestBeat(this.player.position());
    const name = slot === "memory" ? "Memory cue" : `Cue ${slot}`;
    return this.saveCues(`Set ${name.toLowerCase()} on ${entry.row.title}`, [
      { op: "add", track: entry.id, slot, start_secs: secs, replace: slot !== "memory" },
    ]);
  }

  /** The beat nearest a time, on the playing or open track's grid. */
  nearestBeat(secs: number): number {
    const beats = (this.player.track ?? this.detail)?.beats;
    if (!beats?.length) return secs;
    let best = beats[0]?.secs ?? secs;
    for (const beat of beats) {
      if (Math.abs(beat.secs - secs) < Math.abs(best - secs)) best = beat.secs;
    }
    return best;
  }

  selectCue(id: string | null) {
    this.selectedCue = id;
    this.changed();
  }

  moveCue(cue: Cue, secs: number) {
    const entry = this.savable;
    if (!entry || !cue.id) return;
    return this.saveCues(`Move cue ${slotOf(cue)} on ${entry.row.title}`, [{ op: "move", cue: cue.id, start_secs: secs }]);
  }

  renameCue(cue: Cue, name: string) {
    const entry = this.savable;
    if (!entry || !cue.id || name === cue.name) return;
    return this.saveCues(`Rename cue ${slotOf(cue)} on ${entry.row.title}`, [{ op: "rename", cue: cue.id, name }]);
  }

  async deleteCue(cue: Cue) {
    const entry = this.savable;
    if (!entry || !cue.id) return;
    await this.saveCues(`Delete cue ${slotOf(cue)} on ${entry.row.title}`, [{ op: "delete", cue: cue.id }]);
    if (this.selectedCue === cue.id) this.selectedCue = null;
    this.changed();
  }

  /** Deletes the cue picked in the track view. */
  deleteSelectedCue() {
    const cue = this.current?.applied?.cues.find((c) => c.id !== null && c.id === this.selectedCue);
    if (cue) return this.deleteCue(cue);
  }

  undo() {
    return this.write(() => this.backend.undo());
  }

  redo() {
    return this.write(() => this.backend.redo());
  }

  async loadHistory() {
    try {
      this.history = await this.backend.history(100);
    } catch (e) {
      this.history = [];
      this.writeError = String(e);
    }
    this.changed();
  }

  restore(commit: string) {
    return this.write(() => this.backend.restore(commit));
  }

  // ---- the assistant -------------------------------------------------------

  openSheet(sheet: Sheet | null) {
    this.sheet = sheet;
    if (sheet) this.quickOpen = false;
    this.changed();
  }

  /** Runs a kfbox item chosen from the macOS menu bar. Undo and redo go to a
   * text field instead when one has focus, as the menu's Undo would elsewhere. */
  async menu(id: string) {
    const focused = globalThis.document?.activeElement;
    const editing = focused?.tagName === "INPUT" || focused?.tagName === "TEXTAREA";
    const actions: Record<string, () => unknown> = {
      open_xml: () => this.pickXml(),
      add_songs: () => this.pickSongs(),
      reload: () => this.openLibrary(),
      export: () => this.exportXml(),
      settings: () => this.openSheet("settings"),
      undo: () => (editing ? globalThis.document.execCommand("undo") : this.undo()),
      redo: () => (editing ? globalThis.document.execCommand("redo") : this.redo()),
    };
    await actions[id]?.();
  }

  private async settingsCall(action: () => Promise<AssistantSettings>) {
    try {
      this.assistant = await action();
      this.writeError = "";
    } catch (e) {
      this.writeError = String(e);
    }
    this.changed();
  }

  saveApiKey(key: string) {
    return this.settingsCall(() => this.backend.saveApiKey(key));
  }

  removeApiKey() {
    return this.settingsCall(() => this.backend.removeApiKey());
  }

  setModel(model: string) {
    return this.settingsCall(() => this.backend.setAssistantModel(model));
  }

  openQuick() {
    this.quickOpen = true;
    this.quickError = "";
    this.changed();
  }

  closeQuick() {
    this.quickOpen = false;
    this.changed();
  }

  /** Runs an instruction on the selected track, or on every track in the list on screen, then shows what changed. */
  async runQuick(instruction: string, scope: "track" | "list" = "track") {
    if (this.quickBusy || !instruction.trim()) return;
    this.quickBusy = true;
    this.quickError = "";
    this.quickResult = null;
    this.changed();
    try {
      const track = scope === "track" ? (this.current?.id ?? null) : null;
      const text = scope === "list" ? `${instruction}\n\nDo this for every track in the list on screen.` : instruction;
      const result = await this.backend.runQuick(text, track, this.chosenPath);
      this.quickResult = result;
      await this.refreshChanged(result.changed);
      if (this.view === "history") await this.loadHistory();
    } catch (e) {
      this.quickError = String(e);
    } finally {
      this.quickBusy = false;
      this.changed();
    }
  }

  /** A turn of the rules wizard: how you cue, or more guidance on the draft. */
  async wizardSay(said: string) {
    if (this.wizardBusy || !said.trim()) return;
    this.wizardBusy = true;
    this.wizardError = "";
    this.changed();
    try {
      this.wizardTurn = await this.backend.wizardTurn(said);
    } catch (e) {
      this.wizardError = String(e);
    } finally {
      this.wizardBusy = false;
      this.changed();
    }
  }

  async wizardStartOver() {
    await this.backend.wizardReset();
    this.wizardTurn = null;
    this.wizardError = "";
    this.changed();
  }

  /** Saves the wizard's draft as the rules, keeping the old ones, and places the cues again. */
  async saveWizardRules() {
    const draft = this.wizardTurn?.draft;
    if (!draft) return;
    try {
      await this.backend.saveRulesKeepingPrevious(draft.rules);
      this.rulesText = draft.rules;
      this.firstRun = false;
      await this.applyRules();
    } catch (e) {
      this.wizardError = String(e);
      this.changed();
    }
  }

  /** Puts back the rules the last save replaced. */
  async restorePreviousRules() {
    try {
      this.rulesText = await this.backend.restorePreviousRules();
      await this.applyRules();
    } catch (e) {
      this.wizardError = String(e);
      this.changed();
    }
  }

  // ---- rules --------------------------------------------------------------

  setRules(text: string) {
    this.rulesText = text;
    this.changed();
  }

  /** Places the cues on every loaded track. Pressing Apply or Defaults also saves the rules. */
  async applyRules(save = false) {
    try {
      const results = await this.backend.applyRules(this.rulesText, save);
      for (const [path, applied] of results) {
        const entry = this.entries.find((e) => e.analysis?.path === path);
        if (entry) {
          entry.applied = applied;
          this.player.entryChanged(entry);
        }
      }
      this.flagsChanged();
      this.rulesError = "";
    } catch (e) {
      this.rulesError = String(e);
    }
    this.changed();
  }

  async resetRules() {
    this.rulesText = await this.backend.defaultRules();
    await this.applyRules(true);
  }

  async exportXml() {
    const folder = this.openedXml ? this.openedXml.slice(0, this.openedXml.lastIndexOf("/") + 1) : "";
    const dest = await this.backend.pickExportPath(`${folder}kfbox.xml`);
    if (!dest) return;
    try {
      const count = await this.backend.exportXml(dest, this.rulesText);
      this.setStatus(`Wrote ${count} songs to ${fileName(dest)}`);
    } catch (e) {
      this.setStatus(`Export failed: ${e}`);
    }
  }
}

/** Re-renders the calling view whenever the store changes. */
export function useStore(store: Store): Store {
  useSyncExternalStore(store.subscribe, store.getVersion);
  return store;
}
