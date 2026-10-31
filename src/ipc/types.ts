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
  hot: number | null;
  start_secs: number;
  end_secs: number | null;
  color: [number, number, number] | null;
  name: string;
  /** Already in rekordbox: kept as it is. */
  existing: boolean;
}

export interface Applied {
  landmarks: { start: number | null; drop: number | null; groove: number | null };
  cues: Cue[];
  notes: string[];
  kept: string[];
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

export interface PlaylistTrack {
  id: string;
  path: string;
  title: string;
}
