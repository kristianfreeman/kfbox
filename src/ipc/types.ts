// Mirrors of kfbox-core's and the Tauri shell's types, as serde writes them.

export interface Beat {
  secs: number;
  number: number;
  bpm: number;
}

export interface Bar {
  start_secs: number;
  end_secs: number;
  first_beat: number;
  volume: number;
  bass: number;
  busy: number;
}

export interface Track {
  path: string;
  tags: { title: string; artist: string; album: string; genre: string };
  duration_secs: number;
  bpm: number;
  key: string | null;
  camelot: string | null;
  beats: Beat[];
  bars: Bar[];
  overview: [number, number, number, number][];
  /** Where the beats came from: rekordbox's grid, or kfbox's. */
  grid: "rekordbox" | "kfbox";
}

export interface Cue {
  /** Its ID in rekordbox's library, for a cue already there. */
  id: string | null;
  hot: number | null;
  start_secs: number;
  end_secs: number | null;
  color: [number, number, number] | null;
  name: string;
  /** Already in rekordbox: kept as it is. */
  existing: boolean;
}

/** A doubt about cues the rules placed, for a person to check. */
export interface Flag {
  /** `early_drop`: the drop is earlier than any drop cued by hand. */
  kind: "early_drop";
  /** The slots of the new cues it's about, such as `["B", "C"]`. */
  slots: string[];
  /** The bar, from 0. */
  bar: number;
  why: string;
}

export interface Applied {
  landmarks: { start: number | null; drop: number | null; groove: number | null };
  cues: Cue[];
  notes: string[];
  kept: string[];
  /** New cues the rules aren't sure of. */
  flags: Flag[];
}

export interface PlaylistItem {
  index: number;
  path: string;
  folder: boolean;
  count: number;
}

export interface Listing {
  total: number;
  playlists: PlaylistItem[];
}

/** A row of the track table, before the track is analysed. */
export interface TrackRow {
  id: string;
  /** Empty when the collection entry is not a file. */
  path: string;
  title: string;
  artist: string;
  album: string;
  genre: string;
  bpm: number;
  key: string;
  duration_secs: number;
  /** When it joined the collection, as `YYYY-MM-DD`; empty when unknown. */
  added: string;
  /** The letters of the hot cues already in rekordbox. */
  hot: string;
  memory: number;
}

/** What the table shows of an analysed track. */
export interface Analysis {
  path: string;
  title: string;
  artist: string;
  bpm: number;
  key: string | null;
  camelot: string | null;
  duration_secs: number;
  grid: "rekordbox" | "kfbox";
  /** Read from kfbox.db rather than decoded. */
  cached: boolean;
}

/** One tick of the player. */
export interface PlayerTick {
  frames: number;
  total_frames: number;
  sample_rate: number;
  /** Bumped on every load and seek. */
  generation: number;
  playing: boolean;
  loaded: boolean;
  load_id: number;
  looping: boolean;
  /** Stopped because the track ran out, rather than paused. */
  ended: boolean;
}

/** One change to a track's cues. */
export type CueEdit =
  | {
      op: "add";
      track: string;
      slot: string;
      start_secs: number;
      end_secs?: number | null;
      name?: string;
      color?: string | null;
      replace?: boolean;
    }
  | { op: "move"; cue: string; start_secs: number }
  | { op: "rename"; cue: string; name: string }
  | { op: "recolour"; cue: string; color: string | null }
  | { op: "delete"; cue: string };

/** What a write did. */
export interface Saved {
  commit: string | null;
  added: string[];
  /** Every track whose cues changed, as the library now has it. */
  changed: TrackRow[];
}

/** What changed in the library when something else wrote to it, such as an agent. */
export interface LibraryChange {
  listing: Listing;
  /** Tracks that are new or whose row differs, as the library now has them. */
  changed: TrackRow[];
  /** The IDs of tracks no longer in the library. */
  removed: string[];
}

/** A change in the library's history. */
export interface HistoryEntry {
  commit: string;
  unix_secs: number;
  summary: string;
  kind: "first" | "action" | "restore" | "outside";
}

/** The assistant's settings as the page sees them: never the key itself. */
export interface AssistantSettings {
  model: string;
  models: { id: string; label: string }[];
  key_saved: boolean;
}

/** A tool the model called, and how it went. */
export interface Step {
  tool: string;
  ok: boolean;
  error: string | null;
}

export interface QuickResult {
  reply: string;
  steps: Step[];
  /** Every track whose cues changed. */
  changed: TrackRow[];
}

/** How a rules file did against the tracks cued by hand. */
export interface Score {
  tracks: string[];
  /** Each slot's hits on the hand-cued tracks, and on the fixes made to placed cues. */
  slots: { slot: string; hits: number; tracks: number; fixes: number; fix_hits: number }[];
}

export interface WizardTurn {
  reply: string;
  /** The best checked draft so far. */
  draft: { rules: string; score: Score } | null;
  steps: Step[];
}
