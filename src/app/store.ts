import { useSyncExternalStore } from "react";

import type { Backend } from "../ipc/backend";
import type { Applied, Listing, PlaylistItem, Track } from "../ipc/types";
import { fileName } from "../lib/format";

const AUDIO = ["mp3", "wav", "aif", "aiff", "flac", "m4a", "mp4", "aac"];

/** A song in the list: waiting, being analysed, done, or failed. */
export interface Entry {
  /** The path, or `rb:` and the TrackID for a collection entry with none. */
  key: string;
  path: string;
  /** The TrackID, for a song from rekordbox. */
  id?: string;
  title?: string;
  track?: Track;
  applied?: Applied;
  error?: string;
  busy?: boolean;
}

/** The app's state and everything that changes it. Views read it and call its methods. */
export class Store {
  entries: Entry[] = [];
  selected: string | null = null;
  status = "";
  total: number | null = null;
  playlists: PlaylistItem[] = [];
  /** The sidebar's choice, by path so a reload can find it again. `null` is the whole collection. */
  chosenPath: string | null = null;
  rulesText = "";
  rulesError = "";

  private version = 0;
  private listeners = new Set<() => void>();
  private queue: Promise<void> = Promise.resolve();
  // Bumped when a playlist replaces the list, so queued analyses of the old
  // list are dropped.
  private generation = 0;
  // The rekordbox XML last opened: the export is offered next to it.
  private openedXml: string | null = null;

  constructor(private backend: Backend) {}

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
    return this.entries.some((e) => e.track);
  }

  /** Loads the rules, then the songs named at launch or else the library. */
  async start() {
    this.backend.onFileDrop((paths) => this.drop(paths));
    this.rulesText = await this.backend.loadRules();
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
    else this.addPaths(paths);
  }

  addPaths(paths: string[]) {
    const fresh = [...new Set(paths)].filter(
      (p) => AUDIO.includes(p.split(".").pop()?.toLowerCase() ?? "") && !this.entries.some((e) => e.path === p),
    );
    for (const path of fresh) this.entries.push({ key: path, path, busy: true });
    this.selected ??= fresh[0] ?? null;
    this.changed();
    this.queueAnalysis(fresh);
  }

  async pickSongs() {
    this.addPaths(await this.backend.pickAudio(AUDIO));
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
    this.setStatus(`Analysing ${entry.title ?? fileName(entry.path)}…`);
    try {
      entry.track = entry.id ? await this.backend.analyseRekordbox(entry.id) : await this.backend.analyse(entry.path);
    } catch (e) {
      entry.error = String(e);
    }
    entry.busy = false;
    this.status = this.entries.some((e) => e.busy) ? "" : `${this.entries.filter((e) => e.track).length} tracks analysed`;
    await this.applyRules();
  }

  /** Loads rekordbox's own library and offers its playlists. */
  async openLibrary() {
    this.setStatus("Reading the rekordbox library…");
    try {
      const listing = await this.backend.openLibrary();
      this.openedXml = null;
      await this.showPlaylists(listing);
    } catch (e) {
      this.setStatus(String(e));
    }
  }

  async openXml(path: string) {
    try {
      const listing = await this.backend.openXml(path);
      this.openedXml = path;
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

  choose(playlist: PlaylistItem | null) {
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
      this.entries = tracks.map((t) =>
        t.path
          ? { key: t.path, path: t.path, id: t.id, title: t.title, busy: true }
          : { key: `rb:${t.id}`, path: "", id: t.id, title: t.title, error: "no file location in the XML" },
      );
      this.selected = this.entries[0]?.key ?? null;
      this.changed();
      this.queueAnalysis(this.entries.filter((e) => e.busy).map((e) => e.key));
    } catch (e) {
      this.setStatus(String(e));
    }
  }

  select(key: string) {
    this.selected = key;
    this.changed();
  }

  setRules(text: string) {
    this.rulesText = text;
    this.changed();
  }

  /** Places the cues. Only pressing Apply or Defaults saves the rules. */
  async applyRules(save = false) {
    try {
      const results = await this.backend.applyRules(this.rulesText, save);
      for (const [path, applied] of results) {
        const entry = this.entries.find((e) => e.track?.path === path);
        if (entry) entry.applied = applied;
      }
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
