import defaultRules from "../../crates/kfbox-core/rules.default.toml?raw";

import type { Backend } from "./backend";
import type { Analysis, Applied, Bar, Beat, Cue, Listing, Track, TrackRow } from "./types";

/** A made-up song: 96 bars with an intro, a break before the drop, and a stripped-back end. */
function makeTrack(path: string, title: string, artist: string, bpm: number): Track {
  const barSecs = 240 / bpm;
  const dropBar = 32;
  const grooveBar = 72;
  const level = (i: number) => {
    if (i >= 24 && i < dropBar) return { volume: 0.5, bass: 0.1, busy: 0.6 };
    if (i >= grooveBar) return { volume: 0.7, bass: 0.9, busy: 0.2 };
    if (i >= dropBar) return { volume: 1, bass: 1, busy: 0.85 };
    return { volume: 0.7, bass: 0.9, busy: 0.3 };
  };
  const bars: Bar[] = Array.from({ length: 96 }, (_, i) => ({
    start_secs: i * barSecs,
    end_secs: (i + 1) * barSecs,
    first_beat: i * 4,
    ...level(i),
  }));
  const beats: Beat[] = Array.from({ length: 96 * 4 }, (_, i) => ({ secs: (i * barSecs) / 4, number: (i % 4) + 1, bpm }));
  const overview = Array.from({ length: 1200 }, (_, i): [number, number, number, number] => {
    const bar = level(Math.floor((i / 1200) * 96));
    const wobble = 0.75 + 0.25 * Math.sin(i * 0.9);
    return [bar.bass * 220 * wobble, bar.busy * 160, bar.busy * 120, bar.volume * 250 * wobble];
  });
  return {
    path,
    tags: { title, artist, album: "", genre: "Techno" },
    duration_secs: 96 * barSecs,
    bpm,
    key: "Am",
    camelot: "8A",
    beats,
    bars,
    overview,
    grid: "rekordbox",
  };
}

const SONGS: [string, string, number][] = [
  ["Tideline", "Example Artist", 132],
  ["Low Orbit", "Example Artist", 134],
  ["Paper Engine", "Placeholder", 128],
  ["Glasshouse", "Placeholder", 136],
  ["Salt Flats", "Stand-In", 130],
  ["Night Shift", "Stand-In", 138],
];

const LISTING: Listing = {
  total: SONGS.length,
  playlists: [
    { index: 0, path: "Sets", folder: true, count: 0 },
    { index: 1, path: "Sets › Peak time", folder: false, count: 2 },
    { index: 2, path: "Warm up", folder: false, count: 3 },
  ],
};

const asRow = ([title, artist, bpm]: [string, string, number], i: number): TrackRow => ({
  id: String(i + 1),
  path: `/music/${title}.mp3`,
  title,
  artist,
  album: "",
  genre: "Techno",
  bpm,
  key: "Am",
  duration_secs: (96 * 240) / bpm,
  hot: "D",
  memory: 0,
});

const summary = (track: Track): Analysis => ({
  path: track.path,
  title: track.tags.title,
  artist: track.tags.artist,
  bpm: track.bpm,
  key: track.key,
  camelot: track.camelot,
  duration_secs: track.duration_secs,
  grid: track.grid,
  cached: false,
});

function applied(track: Track): Applied {
  const cue = (hot: number, bar: number, color: Cue["color"], name: string): Cue => ({
    hot,
    start_secs: track.bars[bar]?.start_secs ?? 0,
    end_secs: null,
    color,
    name,
    existing: false,
  });
  return {
    landmarks: { start: 0, drop: 32, groove: 72 },
    cues: [
      cue(0, 0, [0, 255, 0], "start"),
      cue(1, 24, [255, 255, 0], "8 to drop"),
      cue(2, 32, [255, 0, 0], "drop"),
      { ...cue(3, 72, [0, 0, 255], "groove"), existing: true },
    ],
    notes: [],
    kept: ["cue D was already set"],
  };
}

export interface MockBackend extends Backend {
  /** The rules each save wrote, for tests. */
  saves: string[];
}

/** A small invented library, so the interface runs and is tested without touching a real one. */
export function mockBackend(delayMs = 150): MockBackend {
  const loaded = new Map<string, Track>();
  const saves: string[] = [];
  const wait = () => new Promise((resolve) => setTimeout(resolve, delayMs));
  const analyse = async (path: string) => {
    await wait();
    const song = SONGS.find(([title]) => path.endsWith(`/${title}.mp3`));
    const track = song
      ? makeTrack(path, song[0], song[1], song[2])
      : makeTrack(path, path.split("/").pop() ?? path, "", 130);
    loaded.set(path, track);
    return summary(track);
  };
  return {
    saves,
    loadRules: async () => defaultRules,
    defaultRules: async () => defaultRules,
    initialPaths: async () => [],
    openLibrary: async () => LISTING,
    openXml: async () => LISTING,
    loadPlaylist: async (index) => {
      loaded.clear();
      const songs = SONGS.map(asRow);
      if (index === 1) return songs.slice(0, 2);
      if (index === 2) return songs.slice(2, 5);
      return songs;
    },
    analyse,
    analyseRekordbox: (id) => {
      const song = SONGS[Number(id) - 1];
      return analyse(`/music/${song?.[0] ?? id}.mp3`);
    },
    trackDetail: async (path) => {
      const track = loaded.get(path);
      if (!track) throw new Error("the track isn't loaded");
      return track;
    },
    applyRulesTo: async (path, rules) => {
      if (rules.includes('slot = "Z"')) throw new Error('cue 1: slot "Z" is not A to P or "memory"');
      const track = loaded.get(path);
      if (!track) throw new Error("the track isn't loaded");
      return applied(track);
    },
    applyRules: async (rules, save) => {
      if (rules.includes('slot = "Z"')) throw new Error('cue 1: slot "Z" is not A to P or "memory"');
      if (save) saves.push(rules);
      return [...loaded.values()].map((track): [string, Applied] => [track.path, applied(track)]);
    },
    exportXml: async () => loaded.size,
    pickAudio: async () => [],
    pickXml: async () => null,
    pickExportPath: async (defaultPath) => defaultPath,
    onFileDrop: () => {},
  };
}
