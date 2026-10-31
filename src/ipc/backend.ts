import type { Analysis, Applied, Listing, PlayerTick, Track, TrackRow } from "./types";
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
  pickAudio(extensions: string[]): Promise<string[]>;
  pickXml(): Promise<string | null>;
  pickExportPath(defaultPath: string): Promise<string | null>;
  /** Calls back with the paths of files dropped on the window. */
  onFileDrop(listener: (paths: string[]) => void): void;
}

/** The real app inside Tauri; a stand-in library in a browser or a test. */
export function getBackend(): Backend {
  return "__TAURI_INTERNALS__" in window ? tauriBackend : mockBackend();
}
