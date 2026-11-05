import { useSyncExternalStore } from "react";

import type { Backend } from "../ipc/backend";
import type {
  Analysis,
  Applied,
  AssistantSettings,
  AutoCueEvent,
  Cue,
  CueEdit,
  Drive,
  GridEdit,
  HistoryEntry,
  ImportReport,
  LibraryChange,
  Listing,
  PlaylistItem,
  QuickResult,
  Saved,
  TagCategory,
  Track,
  TrackRow,
  UsbPlan,
  UsbProgress,
  UsbReport,
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
export type Sheet = "settings" | "wizard" | "import" | "usb";

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
/** What kfbox is busy with, for the header: its words, how far along (0 to 1, or null), and whether it's held up. */
export interface Activity {
  text: string;
  progress: number | null;
  waiting?: boolean;
}

/** The table's columns a person can show or hide. Title is always there. */
export type ColumnId = "artist" | "bpm" | "key" | "time" | "genre" | "rating" | "cues";
export const COLUMN_LABELS: Record<ColumnId, string> = {
  artist: "Artist",
  bpm: "BPM",
  key: "Key",
  time: "Time",
  genre: "Genre",
  rating: "Rating",
  cues: "Cues",
};
const DEFAULT_COLUMNS: ColumnId[] = ["artist", "bpm", "key", "time", "rating", "cues"];
const COLUMNS_KEY = "kfbox.columns";
const FOLDERS_KEY = "kfbox.collapsed";
const SIDEBAR_KEY = "kfbox.sidebar";
/** Bars the deck's waveform shows, and how far it zooms each way. */
export const DEFAULT_BARS = 16;
const FEWEST_BARS = 2;
const MOST_BARS = 64;

function stored<T>(key: string, fallback: T): T {
  try {
    const text = globalThis.localStorage?.getItem(key);
    return text ? (JSON.parse(text) as T) : fallback;
  } catch {
    return fallback;
  }
}

function keep(key: string, value: unknown) {
  try {
    globalThis.localStorage?.setItem(key, JSON.stringify(value));
  } catch {
    // Not kept: the default comes back next time.
  }
}

/** How long a notice, such as an import's summary, stays in the header. */
const NOTICE_MS = 6000;

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

export type SortColumn = "title" | "artist" | "bpm" | "key" | "time" | "genre" | "rating";

const SORT_VALUE: Record<SortColumn, (row: TrackRow) => string | number> = {
  rating: (row) => row.rating,
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
    rating: 0,
    my_tags: [],
  };
}

/** The app's state and everything that changes it. Views read it and call its methods. */
export class Store {
  entries: Entry[] = [];
  /** The entries on screen: filtered by the search, then sorted. */
  rows: Entry[] = [];
  selected: string | null = null;
  /** Goes up each time the selected row should be scrolled into view even if it hasn't moved. */
  revealed = 0;
  status = "";
  total: number | null = null;
  playlists: PlaylistItem[] = [];
  /** The library's My Tag columns: the only values tags can take. */
  myTags: TagCategory[] = [];
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
  /** The table's shown columns. */
  columns: ColumnId[] = stored(COLUMNS_KEY, DEFAULT_COLUMNS);
  /** Playlist folders folded shut in the sidebar, by path. */
  collapsed: string[] = stored(FOLDERS_KEY, []);
  /** How many tracks each kfbox stick is behind the library by (added, or with new cues), by drive path. */
  behind: Record<string, number> = {};
  /** The library is being read. */
  reading = false;
  /** Songs being imported, as said in the header. */
  importing: string | null = null;
  /** Auto-cue's latest news. */
  autoCueState: AutoCueEvent | null = null;
  private noticeTimer: ReturnType<typeof setTimeout> | null = null;
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
  /** Bars the deck's waveform shows around the playhead. */
  zoomBars = DEFAULT_BARS;
  /** The sidebar is showing. */
  sidebarOpen: boolean = stored(SIDEBAR_KEY, true);
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
    this.backend.onAutoCue((event) => void this.autoCueEvent(event));
    this.autoCue = await this.backend.autoCueEnabled().catch(() => true);
    if (!this.sidebarOpen) void this.backend.sidebarShown(false).catch(() => undefined);
    this.watchDrives();
    this.backend.onUsbProgress((progress) => {
      if (!this.usbBusy) return;
      this.usbProgress = progress;
      this.changed();
    });
    this.backend.onHistoryChanged(() => {
      if (this.view === "history") void this.loadHistory();
    });
    this.rulesText = await this.backend.loadRules();
    this.firstRun = !(await this.backend.hasSavedRules().catch(() => true));
    // Not waited for: answering it can mean a Keychain prompt, and the library mustn't wait on one.
    void this.backend
      .assistantSettings()
      .then((settings) => {
        this.assistant = settings;
        this.changed();
      })
      .catch(() => undefined);
    this.changed();
    const named = await this.backend.initialPaths();
    if (named.length) this.addPaths(named);
    else await this.openLibrary();
  }

  /** A notice in the header for a few seconds: what a command did, or what went wrong. */
  private setStatus(text: string) {
    this.status = text;
    if (this.noticeTimer) clearTimeout(this.noticeTimer);
    this.noticeTimer = text
      ? setTimeout(() => {
          this.status = "";
          this.changed();
        }, NOTICE_MS)
      : null;
    this.changed();
  }

  /** What kfbox is busy with, if anything, for the header. Saving isn't announced: it's quick and normal. */
  get activity(): Activity | null {
    const cue = this.autoCueState;
    if (cue?.state === "waiting") return { text: "Waiting for rekordbox to close", progress: null, waiting: true };
    if (this.usbBusy) {
      const drive = this.drives.find((d) => d.path === this.usbDrive);
      const p = this.usbProgress;
      const verb = drive?.stick?.ours ? "Updating" : "Writing";
      return {
        text: p && p.total > 0 ? `${verb} ${drive?.name ?? "USB"}: ${p.done} of ${p.total}` : `${verb} ${drive?.name ?? "USB"}`,
        progress: p && p.total > 0 ? p.done / p.total : null,
      };
    }
    if (this.importing) return { text: `Importing ${this.importing}`, progress: null };
    if (cue?.state === "working") return { text: `Cueing ${cue.done} of ${cue.total}`, progress: cue.total ? cue.done / cue.total : null };
    const busy = this.entries.filter((e) => e.busy).length;
    if (busy > 0) {
      const done = this.entries.length - busy;
      return { text: `Analysing ${done + 1} of ${this.entries.length}`, progress: done / this.entries.length };
    }
    if (this.reading) return { text: "Reading the library", progress: null };
    return null;
  }

  /** The list or view on screen, by name. */
  get title(): string {
    if (this.view === "rules") return "Rules";
    if (this.view === "history") return "History";
    if (this.chosenPath !== null) return this.chosenPath.split(" › ").pop() ?? this.chosenPath;
    return SMART_LABELS[this.smart];
  }

  /** Shows a table column, or hides it. */
  toggleColumn(id: ColumnId) {
    const on = this.columns.includes(id);
    this.columns = on
      ? this.columns.filter((c) => c !== id)
      : (Object.keys(COLUMN_LABELS) as ColumnId[]).filter((c) => c === id || this.columns.includes(c));
    keep(COLUMNS_KEY, this.columns);
    this.changed();
  }

  /** Folds a playlist folder in the sidebar, or opens it. */
  toggleFolder(path: string) {
    this.collapsed = this.collapsed.includes(path) ? this.collapsed.filter((p) => p !== path) : [...this.collapsed, path];
    keep(FOLDERS_KEY, this.collapsed);
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
    this.importing = paths.length === 1 ? fileName(paths[0]!) : `${paths.length} items`;
    this.changed();
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
      this.importing = null;
      this.changed();
      this.stickMayBeBehind();
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

  /** Selects a track of the list on screen, clearing a search that hides it, and scrolls it into view. */
  showTrack(key: string) {
    this.view = "tracks";
    if (this.query && !this.rows.some((e) => e.key === key)) {
      this.query = "";
      this.refreshRows();
    }
    if (!this.rows.some((e) => e.key === key)) return false;
    this.revealed += 1;
    this.select(key);
    return true;
  }

  /** The activity view, clicked while a track is loaded: the loaded track is selected again, scrolled into view and
   * back in the deck. The playback isn't touched. If it isn't in the list on screen, the whole library is shown. */
  async focusPlaying() {
    const key = this.player.key;
    if (!key || this.player.entry?.key !== key) return;
    if (this.showTrack(key)) return;
    if (this.entries.some((e) => e.key === key)) await this.chooseSmart("all");
    else await this.choose(null);
    this.showTrack(key);
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
    this.changed();
  }

  /** Zooms the deck's waveform in by a factor (2 shows half as many bars), between 2 and 64 bars. */
  zoomBy(factor: number) {
    this.zoomBars = Math.max(FEWEST_BARS, Math.min(MOST_BARS, this.zoomBars / factor));
    this.changed();
  }

  /** Shows the sidebar or hides it, as View › Hide Sidebar does, and remembers which. */
  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
    keep(SIDEBAR_KEY, this.sidebarOpen);
    void this.backend.sidebarShown(this.sidebarOpen).catch(() => undefined);
    this.changed();
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

  /** The deck's play button: plays or pauses the selected track, loading it if something else is loaded. */
  playPauseSelected() {
    const entry = this.current;
    if (entry && this.player.key === entry.key) void this.player.toggle();
    else this.playSelected();
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

  /** The selected track whole, for the deck: its overview shows even when the deck is folded. */
  private async loadDetail() {
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
        void this.loadWaveform();
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
    this.changed();
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
    this.reading = true;
    this.changed();
    try {
      const listing = await this.backend.openLibrary();
      this.openedXml = null;
      this.libraryOpen = true;
      this.reading = false;
      await this.showPlaylists(listing);
    } catch (e) {
      this.setStatus(String(e));
    } finally {
      this.reading = false;
      this.changed();
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
  private async showPlaylists({ total, playlists, my_tags }: Listing) {
    this.total = total;
    this.playlists = playlists;
    this.myTags = my_tags ?? [];
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
      if (saved.commit) this.stickMayBeBehind();
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

  async undo() {
    const saved = await this.write(() => this.backend.undo());
    // An undo can bring back a playlist or take one away.
    if (saved && this.libraryOpen) await this.relist();
    return saved;
  }

  async redo() {
    const saved = await this.write(() => this.backend.redo());
    if (saved && this.libraryOpen) await this.relist();
    return saved;
  }

  // ---- auto-cue -----------------------------------------------------------------

  /** kfbox cues tracks with empty slots by itself. */
  autoCue = true;

  async setAutoCue(on: boolean) {
    this.autoCue = await this.backend.setAutoCue(on);
    this.changed();
  }

  /** The worker's news: progress in the status line, and its tracks brought up to date. */
  private async autoCueEvent({ state, done, total, cued, waiting, changed }: AutoCueEvent) {
    this.autoCueState = state === "working" || state === "waiting" ? { state, done, total, cued, waiting, changed: [] } : null;
    this.changed();
    if (state === "done" && cued > 0) this.stickMayBeBehind();
    if (changed.length) {
      await this.refreshChanged(changed);
      if (this.view === "history") await this.loadHistory();
      this.changed();
    }
  }

  // ---- USB export -------------------------------------------------------------

  drives: Drive[] = [];
  usbProgress: UsbProgress | null = null;
  usbReport: UsbReport | null = null;
  usbError = "";
  usbBusy = false;
  /** The drive the USB sheet is for, by path. */
  usbDrive: string | null = null;
  /** The playlists ticked in the USB sheet, by index. */
  usbChosen: number[] = [];
  /** Take off tracks the ticked playlists don't hold. Off unless asked. */
  usbRemove = false;
  usbPlan: UsbPlan | null = null;
  private planAsked = 0;

  /** Looks for drives, for the sidebar's Devices and the Export menu. */
  async refreshDrives() {
    try {
      const drives = await this.backend.usbDrives();
      if (JSON.stringify(drives) !== JSON.stringify(this.drives)) {
        this.drives = drives;
        this.changed();
        await this.refreshBehind();
      }
    } catch {
      // A drive list that can't be read now may be readable next time.
    }
  }

  /** Looks for drives every few seconds in the app, so sticks come and go in the sidebar. */
  watchDrives() {
    void this.refreshDrives();
    if (this.backend.nativeMenu) setInterval(() => void this.refreshDrives(), 3000);
  }

  /** Opens the USB sheet, for `drive` or the first one. A kfbox stick's own playlists are ticked. */
  async openUsb(drive?: Drive) {
    this.usbReport = null;
    this.usbError = "";
    this.usbRemove = false;
    this.openSheet("usb");
    await this.refreshDrives();
    await this.chooseDrive(drive ?? this.drives[0] ?? null);
  }

  async chooseDrive(drive: Drive | null) {
    this.usbDrive = drive?.path ?? null;
    const theirs = drive?.stick?.ours ? await this.backend.stickPlaylists(drive.path).catch(() => []) : [];
    this.usbChosen = theirs.length ? theirs : this.editablePlaylist ? [this.editablePlaylist.index] : [];
    await this.refreshPlan();
  }

  async toggleUsbPlaylist(index: number) {
    this.usbChosen = this.usbChosen.includes(index) ? this.usbChosen.filter((i) => i !== index) : [...this.usbChosen, index];
    await this.refreshPlan();
  }

  async setUsbRemove(remove: boolean) {
    this.usbRemove = remove;
    await this.refreshPlan();
  }

  /** What the export would do, worked out again whenever the choice changes. */
  private async refreshPlan() {
    const asked = ++this.planAsked;
    this.usbPlan = null;
    this.usbError = "";
    this.changed();
    if (!this.usbDrive || this.usbChosen.length === 0) return;
    try {
      const plan = await this.backend.usbPlan(this.usbDrive, this.usbChosen, this.usbRemove);
      if (asked === this.planAsked) this.usbPlan = plan;
    } catch (e) {
      if (asked === this.planAsked) this.usbError = String(e);
    }
    this.changed();
  }

  /** Writes the ticked playlists to the chosen drive. `replace` confirms replacing a rekordbox export. */
  async exportUsb(replace = false) {
    const drive = this.drives.find((d) => d.path === this.usbDrive);
    if (this.usbBusy || !drive || this.usbChosen.length === 0) return;
    this.usbBusy = true;
    this.usbError = "";
    this.usbReport = null;
    this.usbProgress = { stage: "checking", done: 0, total: 0, title: "" };
    this.changed();
    try {
      const options = { file_system: drive.file_system || null, remove: this.usbRemove, replace };
      this.usbReport = await this.backend.exportUsb(drive.path, this.usbChosen, options);
    } catch (e) {
      this.usbError = String(e);
    } finally {
      this.usbBusy = false;
      this.usbProgress = null;
      await this.refreshDrives();
      await this.refreshBehind();
      this.changed();
    }
  }

  private behindTimer: ReturnType<typeof setTimeout> | null = null;

  /** After a write, works out again, a moment later, how far each kfbox stick is behind. */
  private stickMayBeBehind() {
    if (!this.drives.some((d) => d.stick?.ours)) return;
    if (this.behindTimer) clearTimeout(this.behindTimer);
    this.behindTimer = setTimeout(() => void this.refreshBehind(), 1500);
  }

  /** For each kfbox stick, how many of its playlists' tracks are new or have new cues since it was written. */
  async refreshBehind() {
    if (this.usbBusy || !this.libraryOpen) return;
    const behind: Record<string, number> = {};
    for (const drive of this.drives) {
      if (!drive.stick?.ours) continue;
      try {
        const playlists = await this.backend.stickPlaylists(drive.path);
        if (playlists.length === 0) continue;
        const plan = await this.backend.usbPlan(drive.path, playlists, false);
        behind[drive.path] = plan.add + plan.cues_changed;
      } catch {
        // A stick that can't be read now says nothing.
      }
    }
    if (JSON.stringify(behind) !== JSON.stringify(this.behind)) {
      this.behind = behind;
      this.changed();
    }
  }

  cancelUsb() {
    void this.backend.cancelUsb();
  }

  // ---- editing the library ----------------------------------------------------

  /** Reads the playlists again after an edit to them, keeping the list and the selection. */
  private async relist() {
    try {
      const listing = await this.backend.openLibrary();
      this.total = listing.total;
      this.playlists = listing.playlists;
      this.myTags = listing.my_tags ?? [];
      const kept = listing.playlists.find((p) => !p.folder && p.path === this.chosenPath);
      if (this.chosenPath !== null) {
        const selected = this.selected;
        this.chosenPath = kept?.path ?? null;
        await this.loadPlaylist(kept?.index ?? null);
        if (selected && this.entries.some((e) => e.key === selected)) this.select(selected);
      }
      this.changed();
    } catch (e) {
      this.setStatus(String(e));
    }
  }

  /** The selected track's ID, for an edit to the library. */
  private get selectedId(): string | null {
    return this.savable?.id ?? null;
  }

  /** Stars for the selected track, 0 to 5. */
  rate(stars: number) {
    const id = this.selectedId;
    if (id) void this.write(() => this.backend.setRating([id], stars));
  }

  /** Puts a My Tag on the selected track, or takes it off. */
  toggleTag(tag: string) {
    const entry = this.current;
    const id = this.selectedId;
    if (!entry || !id) return;
    const mode = entry.row.my_tags.includes(tag) ? "remove" : "add";
    void this.write(() => this.backend.setMyTags([id], [tag], mode));
  }

  /** The playlist on screen, if it's one whose tracks can be changed. */
  get editablePlaylist(): PlaylistItem | undefined {
    return this.playlists.find((p) => p.path === this.chosenPath && !p.folder && !p.smart);
  }

  /** The playlists a track can be added to. */
  get addablePlaylists(): PlaylistItem[] {
    return this.libraryOpen ? this.playlists.filter((p) => !p.folder && !p.smart) : [];
  }

  async addToPlaylist(playlist: PlaylistItem) {
    const id = this.selectedId;
    if (!id) return;
    if (await this.write(() => this.backend.addToPlaylist(playlist.index, [id]))) await this.relist();
  }

  async removeFromPlaylist() {
    const id = this.selectedId;
    const playlist = this.editablePlaylist;
    if (!id || !playlist) return;
    if (await this.write(() => this.backend.removeFromPlaylist(playlist.index, [id]))) await this.relist();
  }

  /** A new playlist at the top, holding the selected track if there is one. */
  async newPlaylist(name: string) {
    const id = this.current?.id;
    if (await this.write(() => this.backend.createPlaylist(name, null, id ? [id] : []))) await this.relist();
  }

  async renamePlaylist(name: string) {
    const playlist = this.editablePlaylist;
    if (!playlist) return;
    const parts = playlist.path.split(" › ");
    parts[parts.length - 1] = name.trim();
    if (await this.write(() => this.backend.renamePlaylist(playlist.index, name))) {
      this.chosenPath = parts.join(" › ");
      await this.relist();
    }
  }

  async deletePlaylist() {
    const playlist = this.editablePlaylist;
    if (!playlist) return;
    if (await this.write(() => this.backend.deletePlaylist(playlist.index))) await this.relist();
  }

  /** A person's grid edit on the selected track. Cues stay where they are. */
  editGrid(edit: GridEdit) {
    const id = this.selectedId;
    if (id) void this.write(() => this.backend.editGrid(id, edit));
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

  async restore(commit: string) {
    const saved = await this.write(() => this.backend.restore(commit));
    if (saved && this.libraryOpen) await this.relist();
    return saved;
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
      export_usb: () => this.openUsb(),
      settings: () => this.openSheet("settings"),
      view_tracks: () => this.chooseView("tracks"),
      toggle_sidebar: () => this.toggleSidebar(),
      view_rules: () => this.chooseView("rules"),
      view_history: () => this.chooseView("history"),
      wizard: () => this.openSheet("wizard"),
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
