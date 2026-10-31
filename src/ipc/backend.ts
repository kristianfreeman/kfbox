import type { Applied, Listing, PlaylistTrack, Track } from "./types";
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
  loadPlaylist(index: number | null): Promise<PlaylistTrack[]>;
  analyse(path: string): Promise<Track>;
  analyseRekordbox(id: string): Promise<Track>;
  /** Places the cues on every loaded track, keyed by path. Saves the rules only when `save` is set. */
  applyRules(rules: string, save: boolean): Promise<[string, Applied][]>;
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
