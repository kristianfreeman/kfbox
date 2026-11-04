import defaultRules from "../../crates/kfbox-core/rules.default.toml?raw";

import type { Backend } from "./backend";
import type {
  Analysis,
  Applied,
  AssistantSettings,
  Bar,
  Beat,
  Cue,
  CueEdit,
  HistoryEntry,
  ImportReport,
  LibraryChange,
  Listing,
  PlayerTick,
  Saved,
  Score,
  TagCategory,
  Track,
  TrackRow,
  UsbProgress,
} from "./types";

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

/** The stand-in's playlists, with their tracks by ID. */
interface MockPlaylist {
  path: string;
  folder: boolean;
  members: string[];
}

const PLAYLISTS: MockPlaylist[] = [
  { path: "Sets", folder: true, members: [] },
  { path: "Sets › Peak time", folder: false, members: ["1", "2"] },
  { path: "Warm up", folder: false, members: ["3", "4", "5"] },
];

/** The stand-in's one My Tag column, like rbl-db's fixture. */
const MY_TAGS: TagCategory[] = [
  {
    name: "Situation",
    tags: [
      { id: "t1", name: "Peak" },
      { id: "t2", name: "Warm-up" },
    ],
  },
];

const listingOf = (total: number, playlists: MockPlaylist[]): Listing => ({
  total,
  playlists: playlists.map((p, index) => ({ index, path: p.path, folder: p.folder, count: p.members.length })),
  my_tags: MY_TAGS,
});

/** A date this many days before today, as `YYYY-MM-DD`. */
const daysAgo = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString().slice(0, 10);

const letter = (hot: number) => String.fromCharCode(65 + hot);

const asRow = ([title, artist, bpm]: [string, string, number], i: number, saved: Cue[], missing = false): TrackRow => ({
  id: String(i + 1),
  path: `/music/${title}.mp3`,
  title,
  artist,
  album: "",
  genre: "Techno",
  bpm,
  key: "Am",
  duration_secs: (96 * 240) / bpm,
  // The first two were added lately, anything imported today.
  added: daysAgo(i < 2 ? i * 3 : i >= SONGS.length ? 0 : 90 + i * 20),
  hot: saved
    .filter((c) => c.hot !== null)
    .sort((a, b) => (a.hot ?? 0) - (b.hot ?? 0))
    .map((c) => letter(c.hot ?? 0))
    .join(""),
  memory: saved.filter((c) => c.hot === null).length,
  missing,
  rating: 0,
  my_tags: [],
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
  const rules = [
    cue(0, 0, [0, 255, 0], "start"),
    cue(1, 24, [255, 255, 0], "8 to drop"),
    cue(2, 32, [255, 0, 0], "drop"),
    cue(3, 72, [0, 0, 255], "groove"),
  ];
  const taken = new Set(saved.map((c) => c.hot));
  const cues = [...saved, ...rules.filter((c) => !taken.has(c.hot))].sort((a, b) => a.start_secs - b.start_secs);
  // The stand-in's fastest songs have their drop found early, so the flag shows.
  const placed = rules.filter((c) => !taken.has(c.hot) && (c.hot === 1 || c.hot === 2)).map((c) => letter(c.hot ?? 0));
  const early = track.bpm >= 136 && placed.length > 0;
  return {
    landmarks: { start: 0, drop: 32, groove: 72 },
    cues,
    notes: [],
    kept: rules.filter((c) => taken.has(c.hot)).map((c) => `cue ${letter(c.hot ?? 0)} was already set`),
    flags: early
      ? [
          {
            kind: "early_drop",
            slots: placed,
            bar: 32,
            why: `The drop was found at bar 33, earlier than any drop cued by hand (bar 65 or later), so check ${placed.join(" and ")} by ear.`,
          },
        ]
      : [],
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
  /** Every instruction and wizard message sent to the stand-in model, for tests. */
  asked: string[];
  /** Writes as an agent would from another process, and tells the page, for tests. */
  agentWrite(summary: string, edits: CueEdit[]): void;
  /** Tracks whose files have gone, by ID, for tests. */
  missing: Set<string>;
}

const RATE = 44100;

/** A small invented library, so the interface runs and is tested without touching a real one. */
export function mockBackend(delayMs = 150): MockBackend {
  // Its own copy, so an import in one test doesn't show in the next.
  const songs: [string, string, number][] = [...SONGS];
  let playlists: MockPlaylist[] = PLAYLISTS.map((p) => ({ ...p, members: [...p.members] }));
  const listing = () => listingOf(songs.length, playlists);
  const ratings = new Map<string, number>();
  const tagsOf = new Map<string, string[]>();
  const grids = new Map<string, { shift: number; bpm: number | null }>();
  const idOf = (path: string) => String(songs.findIndex(([title]) => path.endsWith(`/${title}.mp3`)) + 1);
  // Tracks whose files have gone, by ID, for tests.
  const missing = new Set<string>();
  const loaded = new Map<string, Track>();
  // The library's cues, by track ID: every song starts with a D, as if set by hand.
  let library = new Map<string, Cue[]>(
    songs.map(([, , bpm], i) => [
      String(i + 1),
      [{ id: `c${i + 1}-d`, hot: 3, start_secs: (72 * 240) / bpm, end_secs: null, color: [0, 0, 255], name: "mine", existing: true }],
    ]),
  );
  let nextCue = 1;
  let keySaved = false;
  let model = "claude-sonnet-5-5";
  let rules: string | null = null;
  let previousRules: string | null = null;
  const settings = (): AssistantSettings => ({
    model,
    models: [
      { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5" },
      { id: "claude-haiku-5-5", label: "Claude Haiku 5.5 (faster, cheaper)" },
    ],
    key_saved: keySaved,
  });
  const needKey = () => {
    if (!keySaved) throw new Error("no API key is saved: add one in Settings");
  };
  const score = (): Score => ({
    tracks: ["1", "2"],
    slots: ["A", "B", "C", "D"].map((slot, i) => ({
      slot,
      hits: 2 - Math.min(i, 2),
      tracks: 2,
      fixes: i === 2 ? 3 : 0,
      fix_hits: i === 2 ? 1 : 0,
    })),
  });
  const snapshot = () => new Map([...library].map(([id, cues]) => [id, cues.map((c) => ({ ...c }))]));
  const history: (HistoryEntry & { state: Map<string, Cue[]> })[] = [
    { commit: "c0", unix_secs: 1, summary: "Library as kfbox first saw it", kind: "first", state: snapshot() },
  ];
  const rowsOf = (ids: string[]) =>
    ids.map((id) => ({
      ...asRow(songs[Number(id) - 1] ?? songs[0]!, Number(id) - 1, library.get(id) ?? [], missing.has(id)),
      rating: ratings.get(id) ?? 0,
      my_tags: tagsOf.get(id) ?? [],
      ...(grids.get(id)?.bpm ? { bpm: grids.get(id)!.bpm! } : {}),
    }));
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
      kept.push({
        id,
        hot,
        start_secs: change.start_secs,
        end_secs: change.end_secs ?? null,
        color: colour(change.color),
        name: change.name ?? "",
        existing: true,
      });
      library.set(change.track, kept);
      added.push(id);
    } else if (change.op === "delete") {
      const { cues } = findCue(change.cue);
      cues.splice(
        cues.findIndex((c) => c.id === change.cue),
        1,
      );
    } else {
      const { cue } = findCue(change.cue);
      if (change.op === "move") cue.start_secs = change.start_secs;
      if (change.op === "rename") cue.name = change.name;
      if (change.op === "recolour") cue.color = colour(change.color);
    }
  };
  const saves: string[] = [];
  const calls: string[] = [];
  const libraryListeners: ((change: LibraryChange) => void)[] = [];
  const usbListeners: ((progress: UsbProgress) => void)[] = [];
  const tickListeners: ((tick: PlayerTick) => void)[] = [];
  const errorListeners: ((error: { load_id: number; message: string }) => void)[] = [];
  let tick: PlayerTick = {
    frames: 0,
    total_frames: 0,
    sample_rate: RATE,
    generation: 0,
    playing: false,
    loaded: false,
    load_id: 0,
    looping: false,
    ended: false,
  };
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
    const song = songs.find(([title]) => path.endsWith(`/${title}.mp3`));
    const track = song ? makeTrack(path, song[0], song[1], song[2]) : makeTrack(path, path.split("/").pop() ?? path, "", 130);
    loaded.set(path, track);
    return summary(track);
  };
  const backend: MockBackend = {
    rekordboxOpen: false,
    asked: [],
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
      const song = songs.find(([title]) => path.endsWith(`/${title}.mp3`));
      const missing = path.includes("missing");
      setTimeout(() => {
        if (missing) for (const listener of errorListeners) listener({ load_id: id, message: "can't open the file" });
        else
          send({
            load_id: id,
            loaded: true,
            frames: 0,
            playing: false,
            ended: false,
            looping: false,
            generation: tick.generation + 1,
            total_frames: Math.round(((96 * 240) / (song?.[2] ?? 130)) * RATE),
          });
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
    openLibrary: async () => listing(),
    openXml: async () => listing(),
    loadPlaylist: async (index) => {
      loaded.clear();
      if (index !== null) return rowsOf(playlists[index]?.members ?? []);
      return rowsOf(songs.map((_, i) => String(i + 1)));
    },
    analyse,
    analyseRekordbox: (id) => {
      const song = songs[Number(id) - 1];
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
      return applied(track, library.get(idOf(path)) ?? []);
    },
    applyRules: async (rules, save) => {
      if (rules.includes('slot = "Z"')) throw new Error('cue 1: slot "Z" is not A to P or "memory"');
      if (save) saves.push(rules);
      return [...loaded.values()].map((track): [string, Applied] => [track.path, applied(track, library.get(idOf(track.path)) ?? [])]);
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
        const path = `/music/${songs[Number(id) - 1]?.[0]}.mp3`;
        const track = loaded.get(path) ?? makeTrack(path, "", "", songs[Number(id) - 1]?.[2] ?? 130);
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
    assistantSettings: async () => settings(),
    setAssistantModel: async (id) => {
      model = id;
      return settings();
    },
    saveApiKey: async (key) => {
      if (!key.trim()) throw new Error("the API key was refused: the key is empty");
      keySaved = true;
      return settings();
    },
    removeApiKey: async () => {
      keySaved = false;
      return settings();
    },
    runQuick: async (instruction, track) => {
      needKey();
      backend.asked.push(instruction);
      // The stand-in model applies the rules to the selected track when asked to redo its cues.
      if (track && /cue/i.test(instruction)) {
        const saved = await backend.writeRuleCues([track], "");
        return {
          reply: `Redid the cues on track ${track}.`,
          steps: [{ tool: "write_rule_cues", ok: true, error: null }],
          changed: saved.changed,
        };
      }
      return { reply: "Nothing to change.", steps: [{ tool: "track_detail", ok: true, error: null }], changed: [] };
    },
    wizardTurn: async (said) => {
      needKey();
      backend.asked.push(said);
      return {
        reply: "A on the first beat, B 8 bars before the drop, C on the drop. It matched most of your hand cues.",
        draft: { rules: `${defaultRules}\n# Drafted by the wizard.\n`, score: score() },
        steps: [
          { tool: "hand_cued_tracks", ok: true, error: null },
          { tool: "submit_rules", ok: true, error: null },
        ],
      };
    },
    wizardReset: async () => {},
    hasSavedRules: async () => rules !== null,
    saveRulesKeepingPrevious: async (text) => {
      previousRules = rules ?? defaultRules;
      rules = text;
    },
    restorePreviousRules: async () => {
      if (previousRules === null) throw new Error("there are no earlier rules to go back to");
      [rules, previousRules] = [previousRules, rules ?? defaultRules];
      return rules;
    },
    scoreRules: async () => score(),
    pickAudio: async () => [],
    pickXml: async () => null,
    pickExportPath: async (defaultPath) => defaultPath,
    onFileDrop: () => {},
    onLibraryChanged: (listener) => libraryListeners.push(listener),
    missing,
    importFiles: async (paths) => {
      refuseIfOpen();
      const report: ImportReport = { imported: [], already: [], duplicates: [], problems: [], commits: [] };
      const audio = paths.filter((p) => /\.(mp3|m4a|aac|flac|wav|aiff?|ogg|opus)$/i.test(p));
      if (audio.length === 0) throw new Error("there are no song files there to import");
      const before = snapshot();
      for (const path of audio) {
        const title = path
          .split("/")
          .pop()!
          .replace(/\.[^.]+$/, "");
        const at = songs.findIndex(([known]) => known === title);
        if (at >= 0) {
          report.duplicates.push({ path, of: rowsOf([String(at + 1)])[0]!, kind: "copy" });
          continue;
        }
        songs.push([title, "Imported", 128]);
        const id = String(songs.length);
        library.set(id, [
          { id: `n${nextCue++}`, hot: 0, start_secs: 0, end_secs: null, color: [0, 255, 0], name: "start", existing: true },
        ]);
        report.imported.push(rowsOf([id])[0]!);
      }
      if (report.imported.length) {
        report.commits.push(commit(`Import ${report.imported.length} tracks`, "action", before).commit ?? "");
        report.commits.push(commit(`Apply rules to ${report.imported.length} tracks`, "action", before).commit ?? "");
      }
      return report;
    },
    relocateMissing: async () => {
      refuseIfOpen();
      const found = [...missing];
      missing.clear();
      if (found.length === 0) return { commit: null, added: [], changed: [] };
      const saved = commit(`Find ${found.length} missing files`, "action", snapshot());
      return { ...saved, changed: rowsOf(found) };
    },
    pickFolder: async () => "/elsewhere",
    usbDrives: async () => [
      { name: "KF-USB", path: "/Volumes/KF-USB", total_bytes: 32e9, free_bytes: 20e9, file_system: "msdos", removable: true },
    ],
    exportUsb: async (destination, chosen) => {
      if (backend.rekordboxOpen) throw new Error("the export stopped");
      const members = new Set(chosen.flatMap((i) => playlists[i]?.members ?? []));
      const total = members.size;
      let done = 0;
      for (const id of members) {
        done += 1;
        for (const listener of usbListeners) listener({ stage: "copying", done, total, title: songs[Number(id) - 1]?.[0] ?? id });
        await new Promise((resolve) => setTimeout(resolve, delayMs / 10));
      }
      calls.push(`usb ${destination} ${chosen.join(",")}`);
      return {
        tracks: total,
        playlists: chosen.length,
        bytes_copied: total * 9e6,
        reused: 0,
        removed: 0,
        failed: [],
        verified: true,
        elapsed_ms: 900,
      };
    },
    cancelUsb: async () => {},
    onUsbProgress: (listener) => usbListeners.push(listener),
    setRating: async (ids, stars) => {
      refuseIfOpen();
      for (const id of ids) ratings.set(id, stars);
      return { ...commit(`Rate ${ids.length} tracks ${"★".repeat(stars)}`, "action", snapshot()), changed: rowsOf(ids) };
    },
    setMyTags: async (ids, tags, mode) => {
      refuseIfOpen();
      const known = MY_TAGS.flatMap((c) => c.tags.map((t) => `${c.name}: ${t.name}`));
      const unknown = tags.find((t) => !known.includes(t));
      if (unknown) throw new Error(`there's no My Tag "${unknown}"; the library has ${known.join(", ")}`);
      for (const id of ids) {
        const on = tagsOf.get(id) ?? [];
        tagsOf.set(
          id,
          mode === "set" ? tags : mode === "add" ? [...on, ...tags.filter((t) => !on.includes(t))] : on.filter((t) => !tags.includes(t)),
        );
      }
      return { ...commit(`Tag ${ids.length} tracks`, "action", snapshot()), changed: rowsOf(ids) };
    },
    createPlaylist: async (name, _folder, ids) => {
      refuseIfOpen();
      playlists = [...playlists, { path: name, folder: false, members: [...ids] }];
      return commit(`New playlist ${name}`, "action", snapshot());
    },
    addToPlaylist: async (index, ids) => {
      refuseIfOpen();
      const playlist = playlists[index]!;
      playlist.members = [...playlist.members, ...ids];
      return commit(`Add ${ids.length} tracks to ${playlist.path}`, "action", snapshot());
    },
    removeFromPlaylist: async (index, ids) => {
      refuseIfOpen();
      const playlist = playlists[index]!;
      playlist.members = playlist.members.filter((m) => !ids.includes(m));
      return commit(`Remove ${ids.length} tracks from ${playlist.path}`, "action", snapshot());
    },
    renamePlaylist: async (index, name) => {
      refuseIfOpen();
      const playlist = playlists[index]!;
      const parts = playlist.path.split(" › ");
      parts[parts.length - 1] = name;
      playlist.path = parts.join(" › ");
      return commit(`Rename to ${name}`, "action", snapshot());
    },
    deletePlaylist: async (index) => {
      refuseIfOpen();
      const gone = playlists[index]!;
      playlists = playlists.filter((p) => p !== gone);
      return commit(`Delete playlist ${gone.path}`, "action", snapshot());
    },
    editGrid: async (id, edit) => {
      refuseIfOpen();
      const grid = grids.get(id) ?? { shift: 0, bpm: null };
      if (edit.kind === "shift") grid.shift += edit.ms;
      if (edit.kind === "bpm") grid.bpm = edit.bpm;
      grids.set(id, grid);
      calls.push(`grid ${id} ${JSON.stringify(edit)}`);
      return { ...commit("Grid edit", "action", snapshot()), changed: rowsOf([id]) };
    },
    onHistoryChanged: () => {},
    agentWrite: (summary, edits) => {
      const before = snapshot();
      const added: string[] = [];
      for (const change of edits) edit(change, added);
      const { changed } = commit(`Agent: ${summary}`, "action", before, added);
      for (const listener of libraryListeners) listener({ listing: listing(), changed, removed: [] });
    },
    nativeMenu: false,
    onMenu: () => {},
  };
  return backend;
}
