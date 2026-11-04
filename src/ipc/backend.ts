import type {
  Analysis,
  Applied,
  AssistantSettings,
  AutoCueEvent,
  CueEdit,
  Drive,
  GridEdit,
  HistoryEntry,
  ImportReport,
  LibraryChange,
  Listing,
  PlayerTick,
  QuickResult,
  Saved,
  Score,
  Track,
  TrackRow,
  UsbOptions,
  UsbPlan,
  UsbProgress,
  UsbReport,
  WizardTurn,
} from "./types";
import { mockBackend } from "./mock";
import { tauriBackend } from "./tauri";

/** Everything the interface can ask of the app. Only `src/ipc` talks to Tauri. */
export interface Backend {
  loadRules(): Promise<string>;
  defaultRules(): Promise<string>;
  initialPaths(): Promise<string[]>;
  openLibrary(): Promise<Listing>;
  openXml(path: string): Promise<Listing>;
  /** `null` is the whole collection. */
  loadPlaylist(index: number | null): Promise<TrackRow[]>;
  analyse(path: string): Promise<Analysis>;
  analyseRekordbox(id: string): Promise<Analysis>;
  /** The whole of a loaded track, for the track view. */
  trackDetail(path: string): Promise<Track>;
  /** Places the cues on every loaded track, keyed by path. Saves the rules only when `save` is set. */
  applyRules(rules: string, save: boolean): Promise<[string, Applied][]>;
  /** The closer waveform for a zoomed view: flat `[low, mid, high, peak]` columns, 40 a second. */
  waveform(path: string): Promise<number[]>;
  /** Starts loading a file into the player and returns its load ID. */
  playerLoad(path: string): Promise<number>;
  playerPlay(): Promise<void>;
  playerPause(): Promise<void>;
  playerSeek(secs: number): Promise<void>;
  playerSetLoop(from: number, to: number): Promise<void>;
  playerClearLoop(): Promise<void>;
  playerScrubBegin(): Promise<void>;
  playerScrubTo(secs: number): Promise<void>;
  playerScrubEnd(): Promise<void>;
  onPlayerTick(listener: (tick: PlayerTick) => void): void;
  onPlayerError(listener: (error: { load_id: number; message: string }) => void): void;
  /** Places the cues on one loaded track. */
  applyRulesTo(path: string, rules: string): Promise<Applied>;
  exportXml(dest: string, rules: string): Promise<number>;
  /** Saves cue edits to rekordbox's library, all together or not at all. */
  saveCues(summary: string, edits: CueEdit[]): Promise<Saved>;
  /** Writes the cues the rules place on these tracks, in one change. */
  writeRuleCues(ids: string[], rules: string): Promise<Saved>;
  /** Adds songs, and the songs in folders, to the library where they are, analysed and cued. */
  importFiles(paths: string[]): Promise<ImportReport>;
  /** Points missing tracks at files of the same name in a folder. */
  relocateMissing(folder: string): Promise<Saved>;
  setRating(ids: string[], stars: number): Promise<Saved>;
  /** `tags` as `Column: Tag`, from the library's own. */
  setMyTags(ids: string[], tags: string[], mode: "add" | "remove" | "set"): Promise<Saved>;
  /** `folder` is a folder's index, or `null` for the top. */
  createPlaylist(name: string, folder: number | null, ids: string[]): Promise<Saved>;
  addToPlaylist(index: number, ids: string[]): Promise<Saved>;
  removeFromPlaylist(index: number, ids: string[]): Promise<Saved>;
  renamePlaylist(index: number, name: string): Promise<Saved>;
  deletePlaylist(index: number): Promise<Saved>;
  /** A person's grid edit. Nothing automatic calls this. */
  editGrid(id: string, edit: GridEdit): Promise<Saved>;
  /** Whether kfbox cues tracks with empty slots by itself. On by default. */
  autoCueEnabled(): Promise<boolean>;
  setAutoCue(on: boolean): Promise<boolean>;
  onAutoCue(listener: (event: AutoCueEvent) => void): void;
  usbDrives(): Promise<Drive[]>;
  /** Writes these playlists (a folder brings what's in it) to the drive at `destination`. */
  exportUsb(destination: string, playlists: number[], options: UsbOptions): Promise<UsbReport>;
  /** What that export would do, before anything is written. */
  usbPlan(destination: string, playlists: number[], remove: boolean): Promise<UsbPlan>;
  /** The library's playlists a kfbox stick already holds. */
  stickPlaylists(destination: string): Promise<number[]>;
  cancelUsb(): Promise<void>;
  onUsbProgress(listener: (progress: UsbProgress) => void): void;
  undo(): Promise<Saved>;
  redo(): Promise<Saved>;
  history(limit: number): Promise<HistoryEntry[]>;
  restore(commit: string): Promise<Saved>;

  assistantSettings(): Promise<AssistantSettings>;
  setAssistantModel(model: string): Promise<AssistantSettings>;
  /** Puts the API key in the Keychain. It never comes back to the page. */
  saveApiKey(key: string): Promise<AssistantSettings>;
  removeApiKey(): Promise<AssistantSettings>;
  /** An instruction the model carries out with kfbox's tools. */
  runQuick(instruction: string, track: string | null, playlist: string | null): Promise<QuickResult>;
  wizardTurn(said: string): Promise<WizardTurn>;
  wizardReset(): Promise<void>;
  hasSavedRules(): Promise<boolean>;
  /** Saves rules, keeping the ones they replace. */
  saveRulesKeepingPrevious(rules: string): Promise<void>;
  /** Swaps the saved rules with the ones they replaced; returns the rules now in use. */
  restorePreviousRules(): Promise<string>;
  scoreRules(rules: string): Promise<Score>;
  pickAudio(extensions: string[]): Promise<string[]>;
  pickXml(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
  pickExportPath(defaultPath: string): Promise<string | null>;
  /** Calls back with the paths of files dropped on the window. */
  onFileDrop(listener: (paths: string[]) => void): void;
  /** Calls back when something else, such as an agent, wrote to the open library. */
  onLibraryChanged(listener: (change: LibraryChange) => void): void;
  /** Calls back when only the history moved. */
  onHistoryChanged(listener: () => void): void;
  /** Whether the app has a native menu bar, which then owns the shortcuts its items show. */
  readonly nativeMenu: boolean;
  /** Writes a page error to the shell's log (stderr, so Console and a terminal show it). */
  reportError(text: string): Promise<void>;
  /** Whether this is the app's smoke test, whose page reports that it works. */
  smokeMode(): Promise<boolean>;
  /** The smoke test's report: the app prints it and quits. */
  smokeDone(ok: boolean, detail: string): Promise<void>;
  /** Tells the menu bar whether the sidebar shows, so View says Hide or Show Sidebar. */
  sidebarShown(shown: boolean): Promise<void>;
  /** Calls back with the ID of each kfbox item chosen from the menu bar. */
  onMenu(listener: (id: string) => void): void;
}

/** The real app inside Tauri; a stand-in library in a browser or a test. */
export function getBackend(): Backend {
  return "__TAURI_INTERNALS__" in window ? tauriBackend : mockBackend();
}
