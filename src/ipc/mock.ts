import defaultRules from "../../crates/kfbox-core/rules.default.toml?raw";

import type { Backend } from "./backend";
import type { Analysis, Applied, Bar, Beat, Cue, CueEdit, HistoryEntry, Listing, PlayerTick, Saved, Track, TrackRow } from "./types";

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

const letter = (hot: number) => String.fromCharCode(65 + hot);

const asRow = ([title, artist, bpm]: [string, string, number], i: number, saved: Cue[]): TrackRow => ({
  id: String(i + 1),
  path: `/music/${title}.mp3`,
  title,
  artist,
  album: "",
  genre: "Techno",
  bpm,
  key: "Am",
  duration_secs: (96 * 240) / bpm,
  hot: saved.filter((c) => c.hot !== null).sort((a, b) => (a.hot ?? 0) - (b.hot ?? 0)).map((c) => letter(c.hot ?? 0)).join(""),
  memory: saved.filter((c) => c.hot === null).length,
});

const idOfPath = (path: string) => String(SONGS.findIndex(([title]) => path.endsWith(`/${title}.mp3`)) + 1);

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

/** The rules' cues in the slots the track's saved cues leave free, beside those. */
function applied(track: Track, saved: Cue[]): Applied {
  const cue = (hot: number, bar: number, color: Cue["color"], name: string): Cue => ({
    id: null,
    hot,
    start_secs: track.bars[bar]?.start_secs ?? 0,
    end_secs: null,
    color,
    name,
    existing: false,
  });
  const rules = [cue(0, 0, [0, 255, 0], "start"), cue(1, 24, [255, 255, 0], "8 to drop"), cue(2, 32, [255, 0, 0], "drop"), cue(3, 72, [0, 0, 255], "groove")];
  const taken = new Set(saved.map((c) => c.hot));
  const cues = [...saved, ...rules.filter((c) => !taken.has(c.hot))].sort((a, b) => a.start_secs - b.start_secs);
  return {
    landmarks: { start: 0, drop: 32, groove: 72 },
    cues,
    notes: [],
    kept: rules.filter((c) => taken.has(c.hot)).map((c) => `cue ${letter(c.hot ?? 0)} was already set`),
  };
}

export interface MockBackend extends Backend {
  /** The rules each save wrote, for tests. */
  saves: string[];
  /** Sends a player tick from the stand-in engine, for tests. */
  tickNow(change?: Partial<PlayerTick>): void;
  /** What the stand-in engine was asked, for tests. */
  calls: string[];
  /** Makes writes fail as if rekordbox were open, for tests. */
  rekordboxOpen: boolean;
}

const RATE = 44100;

/** A small invented library, so the interface runs and is tested without touching a real one. */
export function mockBackend(delayMs = 150): MockBackend {
  const loaded = new Map<string, Track>();
  // The library's cues, by track ID: every song starts with a D, as if set by hand.
  let library = new Map<string, Cue[]>(
    SONGS.map(([, , bpm], i) => [String(i + 1), [{ id: `c${i + 1}-d`, hot: 3, start_secs: (72 * 240) / bpm, end_secs: null, color: [0, 0, 255], name: "mine", existing: true }]]),
  );
  let nextCue = 1;
  const snapshot = () => new Map([...library].map(([id, cues]) => [id, cues.map((c) => ({ ...c }))]));
  const history: (HistoryEntry & { state: Map<string, Cue[]> })[] = [
    { commit: "c0", unix_secs: 1, summary: "Library as kfbox first saw it", kind: "first", state: snapshot() },
  ];
  const rowsOf = (ids: string[]) => ids.map((id) => asRow(SONGS[Number(id) - 1] ?? SONGS[0]!, Number(id) - 1, library.get(id) ?? []));
  const commit = (summary: string, kind: HistoryEntry["kind"], before: Map<string, Cue[]>, added: string[] = []): Saved => {
    const changed = [...library.keys()].filter((id) => JSON.stringify(library.get(id)) !== JSON.stringify(before.get(id)));
    const entry = { commit: `c${history.length}`, unix_secs: history.length + 1, summary, kind, state: snapshot() };
    history.unshift(entry);
    return { commit: entry.commit, added, changed: rowsOf(changed) };
  };
  const goBackTo = (state: Map<string, Cue[]>, summary: string) => {
    const before = snapshot();
    library = new Map([...state].map(([id, cues]) => [id, cues.map((c) => ({ ...c }))]));
    return commit(summary, "restore", before);
  };
  const refuseIfOpen = () => {
    if (backend.rekordboxOpen) throw new Error("rekordbox is running. Quit it and try again.");
  };
  const findCue = (id: string) => {
    for (const cues of library.values()) {
      const cue = cues.find((c) => c.id === id);
      if (cue) return { cue, cues };
    }
    throw new Error(`no cue ${id}`);
  };
  const colour = (hex: string | null | undefined): Cue["color"] =>
    hex ? [Number.parseInt(hex.slice(1, 3), 16), Number.parseInt(hex.slice(3, 5), 16), Number.parseInt(hex.slice(5, 7), 16)] : null;
  const edit = (change: CueEdit, added: string[]) => {
    if (change.op === "add") {
      const cues = library.get(change.track) ?? [];
      const hot = change.slot === "memory" ? null : change.slot.charCodeAt(0) - 65;
      const taken = hot !== null && cues.some((c) => c.hot === hot);
      if (taken && !change.replace) throw new Error(`hot cue slot is taken on track ${change.track}`);
      const id = `n${nextCue++}`;
      const kept = cues.filter((c) => !(hot !== null && c.hot === hot));
      kept.push({ id, hot, start_secs: change.start_secs, end_secs: change.end_secs ?? null, color: colour(change.color), name: change.name ?? "", existing: true });
      library.set(change.track, kept);
      added.push(id);
    } else if (change.op === "delete") {
      const { cues } = findCue(change.cue);
      cues.splice(cues.findIndex((c) => c.id === change.cue), 1);
    } else {
      const { cue } = findCue(change.cue);
      if (change.op === "move") cue.start_secs = change.start_secs;
      if (change.op === "rename") cue.name = change.name;
      if (change.op === "recolour") cue.color = colour(change.color);
    }
  };
  const saves: string[] = [];
  const calls: string[] = [];
  const tickListeners: ((tick: PlayerTick) => void)[] = [];
  const errorListeners: ((error: { load_id: number; message: string }) => void)[] = [];
  let tick: PlayerTick = { frames: 0, total_frames: 0, sample_rate: RATE, generation: 0, playing: false, loaded: false, load_id: 0, looping: false, ended: false };
  let loads = 0;
  const send = (change: Partial<PlayerTick> = {}) => {
    tick = { ...tick, ...change };
    for (const listener of tickListeners) listener(tick);
  };
  // The engine's clock, for a browser. A test sends its own ticks instead.
  if (delayMs > 0 && typeof window !== "undefined") {
    setInterval(() => {
      if (!tick.playing) return;
      const frames = tick.frames + RATE / 10;
      if (frames >= tick.total_frames) send({ frames: tick.total_frames, playing: false, ended: true });
      else send({ frames });
    }, 100);
  }
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
  const backend: MockBackend = {
    rekordboxOpen: false,
    saves,
    calls,
    tickNow: send,
    waveform: async (path) => {
      const track = loaded.get(path);
      const columns = Math.round((track?.duration_secs ?? 60) * 40);
      return Array.from({ length: columns * 4 }, (_, i) => Math.round(120 + 100 * Math.sin(i * 0.03)));
    },
    playerLoad: async (path) => {
      loads += 1;
      calls.push(`load ${path}`);
      const id = loads;
      const song = SONGS.find(([title]) => path.endsWith(`/${title}.mp3`));
      const missing = path.includes("missing");
      setTimeout(() => {
        if (missing) for (const listener of errorListeners) listener({ load_id: id, message: "can't open the file" });
        else send({ load_id: id, loaded: true, frames: 0, playing: false, ended: false, looping: false, generation: tick.generation + 1, total_frames: Math.round(((96 * 240) / (song?.[2] ?? 130)) * RATE) });
      }, delayMs);
      return id;
    },
    playerPlay: async () => {
      calls.push("play");
      send({ playing: true, ended: false });
    },
    playerPause: async () => {
      calls.push("pause");
      send({ playing: false });
    },
    playerSeek: async (secs) => {
      calls.push(`seek ${secs.toFixed(3)}`);
      send({ frames: secs * RATE, generation: tick.generation + 1, ended: false });
    },
    playerSetLoop: async (from, to) => {
      calls.push(`loop ${from.toFixed(3)} ${to.toFixed(3)}`);
      send({ looping: true });
    },
    playerClearLoop: async () => {
      calls.push("clear loop");
      send({ looping: false });
    },
    playerScrubBegin: async () => {
      calls.push("scrub begin");
    },
    playerScrubTo: async (secs) => {
      calls.push(`scrub ${secs.toFixed(3)}`);
      send({ frames: secs * RATE });
    },
    playerScrubEnd: async () => {
      calls.push("scrub end");
    },
    onPlayerTick: (listener) => {
      tickListeners.push(listener);
    },
    onPlayerError: (listener) => {
      errorListeners.push(listener);
    },
    loadRules: async () => defaultRules,
    defaultRules: async () => defaultRules,
    initialPaths: async () => [],
    openLibrary: async () => LISTING,
    openXml: async () => LISTING,
    loadPlaylist: async (index) => {
      loaded.clear();
      const songs = SONGS.map((song, i) => asRow(song, i, library.get(String(i + 1)) ?? []));
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
      return applied(track, library.get(idOfPath(path)) ?? []);
    },
    applyRules: async (rules, save) => {
      if (rules.includes('slot = "Z"')) throw new Error('cue 1: slot "Z" is not A to P or "memory"');
      if (save) saves.push(rules);
      return [...loaded.values()].map((track): [string, Applied] => [track.path, applied(track, library.get(idOfPath(track.path)) ?? [])]);
    },
    exportXml: async () => loaded.size,
    saveCues: async (summary, edits) => {
      refuseIfOpen();
      if (edits.length === 0) throw new Error("there's nothing to save");
      const before = snapshot();
      const added: string[] = [];
      try {
        for (const change of edits) edit(change, added);
      } catch (e) {
        library = before;
        throw e;
      }
      return commit(summary, "action", before, added);
    },
    writeRuleCues: async (ids) => {
      refuseIfOpen();
      const before = snapshot();
      const added: string[] = [];
      for (const id of ids) {
        const path = `/music/${SONGS[Number(id) - 1]?.[0]}.mp3`;
        const track = loaded.get(path) ?? makeTrack(path, "", "", SONGS[Number(id) - 1]?.[2] ?? 130);
        for (const cue of applied(track, library.get(id) ?? []).cues.filter((c) => !c.existing)) {
          edit({ op: "add", track: id, slot: letter(cue.hot ?? 0), start_secs: cue.start_secs, name: cue.name, color: null }, added);
        }
      }
      if (added.length === 0) return { commit: null, added, changed: [] };
      return commit(ids.length === 1 ? "Apply rules to 1 track" : `Apply rules to ${ids.length} tracks`, "action", before, added);
    },
    undo: async () => {
      refuseIfOpen();
      const [latest, previous] = history;
      if (!latest || !previous || latest.kind === "first") throw new Error("there's nothing to undo");
      return goBackTo(previous.state, `Undo: ${latest.summary}`);
    },
    redo: async () => {
      refuseIfOpen();
      const [latest, previous] = history;
      if (!latest?.summary.startsWith("Undo: ") || !previous) throw new Error("there's nothing to redo");
      return goBackTo(previous.state, `Redo: ${latest.summary.slice(6)}`);
    },
    history: async (limit) => history.slice(0, limit).map(({ commit, unix_secs, summary, kind }) => ({ commit, unix_secs, summary, kind })),
    restore: async (id) => {
      refuseIfOpen();
      const entry = history.find((h) => h.commit === id);
      if (!entry) throw new Error(`${id} isn't a commit in the history`);
      return goBackTo(entry.state, `Restore to ${id}`);
    },
    pickAudio: async () => [],
    pickXml: async () => null,
    pickExportPath: async (defaultPath) => defaultPath,
    onFileDrop: () => {},
  };
  return backend;
}
