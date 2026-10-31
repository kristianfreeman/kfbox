import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";

// Mirrors of kfbox-core's types, as serde writes them.
interface Beat {
  secs: number;
  number: number;
  bpm: number;
}
interface Bar {
  start_secs: number;
  end_secs: number;
  first_beat: number;
  volume: number;
  bass: number;
  busy: number;
}
interface Track {
  path: string;
  tags: { title: string; artist: string; album: string; genre: string };
  duration_secs: number;
  bpm: number;
  key: string | null;
  camelot: string | null;
  beats: Beat[];
  bars: Bar[];
  overview: [number, number, number, number][];
  /** Where the beats came from: the rekordbox XML's TEMPO entries, or kfbox. */
  grid: "rekordbox" | "kfbox";
}
interface Cue {
  hot: number | null;
  start_secs: number;
  end_secs: number | null;
  color: [number, number, number] | null;
  name: string;
  /** Already in rekordbox: kept as it is. */
  existing: boolean;
}
interface Applied {
  landmarks: { start: number | null; drop: number | null; groove: number | null };
  cues: Cue[];
  notes: string[];
  kept: string[];
}
interface PlaylistItem {
  index: number;
  path: string;
  folder: boolean;
  count: number;
}
interface Listing {
  total: number;
  playlists: PlaylistItem[];
}
interface PlaylistTrack {
  id: string;
  path: string;
  title: string;
}

/** A song in the list: waiting, being analysed, done, or failed. */
interface Entry {
  /** The path, or `rb:` and the TrackID for a collection entry with none. */
  key: string;
  path: string;
  /** The TrackID, for a song from the rekordbox XML. */
  id?: string;
  title?: string;
  track?: Track;
  applied?: Applied;
  error?: string;
  busy?: boolean;
}

const ALL_TRACKS = "all";
const AUDIO = ["mp3", "wav", "aif", "aiff", "flac", "m4a", "mp4", "aac"];

const entries: Entry[] = [];
let selected: string | null = null;
let queue: Promise<void> = Promise.resolve();
// Bumped when a playlist replaces the list, so queued analyses of the old
// list are dropped.
let generation = 0;
// The rekordbox XML last opened: the export is offered next to it.
let openedXml: string | null = null;
// The picker's choice, by path so a reload can find it again. `null` is the
// whole collection.
let chosenPath: string | null = null;
let offered: PlaylistItem[] = [];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const rulesText = $<HTMLTextAreaElement>("rules-text");
const wave = $<HTMLCanvasElement>("wave");
const levels = $<HTMLCanvasElement>("levels");

const fileName = (path: string) => path.split("/").pop() ?? path;
const slotName = (cue: Cue) => (cue.hot === null ? "M" : String.fromCharCode(65 + cue.hot));
const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

function clock(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
}

function setStatus(text: string) {
  $("status").textContent = text;
}

// ---- loading ------------------------------------------------------------

function addPaths(paths: string[]) {
  const fresh = paths.filter(
    (p) => AUDIO.includes(p.split(".").pop()?.toLowerCase() ?? "") && !entries.some((e) => e.path === p),
  );
  for (const path of fresh) {
    entries.push({ key: path, path, busy: true });
  }
  selected ??= fresh[0] ?? null;
  renderList();
  queueAnalysis(fresh);
}

function queueAnalysis(keys: string[]) {
  const mine = generation;
  for (const key of keys) {
    // One at a time: analysis already uses every core.
    queue = queue.then(() => (mine === generation ? analyse(key) : undefined));
  }
}

async function analyse(key: string) {
  const entry = entries.find((e) => e.key === key);
  if (!entry) return;
  setStatus(`Analysing ${entry.title ?? fileName(entry.path)}…`);
  try {
    entry.track = entry.id
      ? await invoke<Track>("analyse_rekordbox", { id: entry.id })
      : await invoke<Track>("analyse", { path: entry.path });
  } catch (e) {
    entry.error = String(e);
  }
  entry.busy = false;
  setStatus(entries.some((e) => e.busy) ? "" : `${entries.filter((e) => e.track).length} tracks analysed`);
  await applyRules();
}

/** Loads rekordbox's own library and offers its playlists. */
async function openLibrary() {
  setStatus("Reading the rekordbox library…");
  try {
    const listing = await invoke<Listing>("open_rekordbox_library");
    openedXml = null;
    await showPlaylists(listing);
  } catch (e) {
    setStatus(String(e));
  }
}

async function openRekordbox(path: string) {
  try {
    const listing = await invoke<Listing>("open_rekordbox", { path });
    openedXml = path;
    await showPlaylists(listing);
  } catch (e) {
    setStatus(String(e));
  }
}

/** Fills the picker and loads the choice it had, or the whole collection. */
async function showPlaylists({ total, playlists }: Listing) {
  const picker = $<HTMLSelectElement>("playlist");
  const depth = (p: PlaylistItem) => p.path.split(" › ").length - 1;
  offered = playlists;
  picker.replaceChildren(
    new Option(`All tracks  (${total})`, ALL_TRACKS),
    ...playlists.map((p) => {
      const name = p.path.split(" › ").pop() ?? p.path;
      const option = new Option(`${"\u2003".repeat(depth(p))}${name}${p.folder ? "" : `  (${p.count})`}`, String(p.index));
      option.disabled = p.folder;
      return option;
    }),
  );
  const kept = playlists.find((p) => !p.folder && p.path === chosenPath);
  picker.value = kept ? String(kept.index) : ALL_TRACKS;
  picker.hidden = false;
  await loadPlaylist(kept?.index ?? null);
}

async function loadPlaylist(index: number | null) {
  generation += 1;
  const mine = generation;
  // Queued analyses of the old list now skip themselves. The one under way
  // has to finish first, or its track lands in the new list's export.
  await queue;
  if (mine !== generation) return;
  try {
    const tracks = await invoke<PlaylistTrack[]>("load_playlist", { index });
    entries.length = 0;
    for (const t of tracks) {
      entries.push(
        t.path
          ? { key: t.path, path: t.path, id: t.id, title: t.title, busy: true }
          : { key: `rb:${t.id}`, path: "", id: t.id, title: t.title, error: "no file location in the XML" },
      );
    }
    selected = entries[0]?.key ?? null;
    renderList();
    renderDetail();
    queueAnalysis(entries.filter((e) => e.busy).map((e) => e.key));
  } catch (e) {
    setStatus(String(e));
  }
}

async function applyRules(save = false) {
  try {
    const results = await invoke<[string, Applied][]>("apply_rules", { rules: rulesText.value, save });
    for (const [path, applied] of results) {
      const entry = entries.find((e) => e.track?.path === path);
      if (entry) entry.applied = applied;
    }
    $("rules-error").textContent = "";
  } catch (e) {
    $("rules-error").textContent = String(e);
  }
  renderList();
  renderDetail();
}

// ---- list ---------------------------------------------------------------

function renderList() {
  const list = $("tracks");
  list.replaceChildren(
    ...entries.map((entry) => {
      const li = document.createElement("li");
      li.classList.toggle("selected", entry.key === selected);
      const title = document.createElement("span");
      title.className = "title";
      const t = entry.track;
      title.textContent = t ? [t.tags.artist, t.tags.title].filter(Boolean).join(" – ") : (entry.title ?? fileName(entry.path));
      const meta = document.createElement("span");
      meta.className = "meta";
      if (entry.error) {
        meta.classList.add("error");
        meta.textContent = entry.error;
      } else if (entry.busy || !t) {
        meta.textContent = "analysing…";
      } else {
        const cues = entry.applied?.cues ?? [];
        // Hot cue letters, and memory cues as a count: a track can have dozens.
        const summary = (existing: boolean) => {
          const mine = cues.filter((c) => c.existing === existing);
          const hot = mine.filter((c) => c.hot !== null).map(slotName).join("");
          const memory = mine.length - hot.length;
          return [hot, memory ? `${memory}M` : ""].filter(Boolean).join(" ");
        };
        const kept = summary(true);
        const added = summary(false);
        const parts = [t.bpm.toFixed(2), t.camelot ?? t.key ?? "–"];
        if (kept) parts.push(`kept ${kept}`);
        if (added) parts.push(`new ${added}`);
        if (entry.id && t.grid === "kfbox") parts.push("kfbox grid");
        meta.textContent = parts.join("  ");
      }
      li.append(title, meta);
      li.onclick = () => {
        selected = entry.key;
        renderList();
        renderDetail();
      };
      return li;
    }),
  );
  $("empty").hidden = entries.length > 0;
  $<HTMLButtonElement>("export").disabled = !entries.some((e) => e.track);
}

// ---- detail -------------------------------------------------------------

function current(): Entry | undefined {
  return entries.find((e) => e.key === selected);
}

function renderDetail() {
  const entry = current();
  const t = entry?.track;
  $("detail").hidden = !t;
  if (!entry || !t) return;

  const heading = $("heading");
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = [t.tags.artist, t.tags.title].filter(Boolean).join(" – ");
  const meta = document.createElement("span");
  meta.className = "meta";
  const grid = t.grid === "rekordbox" ? "rekordbox grid" : "kfbox grid";
  meta.textContent = `${t.bpm.toFixed(2)} BPM  ${t.key ?? "?"} ${t.camelot ? `(${t.camelot})` : ""}  ${clock(t.duration_secs)}  ${t.bars.length} bars  ${grid}`;
  heading.replaceChildren(name, meta);

  const cues = entry.applied?.cues ?? [];
  $("cues").replaceChildren(
    ...cues.map((cue) => {
      const row = document.createElement("tr");
      const dot = `<span class="dot" style="background:${cue.color ? `rgb(${cue.color.join(",")})` : "transparent"}"></span>`;
      const bar = barAt(t, cue.start_secs) + 1;
      const length = cue.end_secs === null ? "" : `loop ${Math.round((cue.end_secs - cue.start_secs) / (240 / t.bpm))} bars`;
      row.className = cue.existing ? "kept" : "new";
      row.innerHTML = `<td>${dot} ${slotName(cue)}</td><td>${cue.existing ? "kept" : "new"}</td><td>${clock(cue.start_secs)}</td><td>bar ${bar}</td><td>${length}</td><td></td>`;
      row.lastElementChild!.textContent = cue.name;
      return row;
    }),
  );
  const line = (text: string, className: string) => {
    const li = document.createElement("li");
    li.className = className;
    li.textContent = text;
    return li;
  };
  $("notes").replaceChildren(
    ...(entry.applied?.notes ?? []).map((note) => line(note, "warn")),
    ...(entry.applied?.kept ?? []).map((note) => line(`${note}: left as it is`, "info")),
  );
  draw();
}

function barAt(t: Track, secs: number): number {
  let index = 0;
  t.bars.forEach((bar, i) => {
    if (bar.start_secs <= secs + 0.01) index = i;
  });
  return index;
}

/** Sizes a canvas to its box at the screen's pixel density. Its CSS height
 * is `data-height`: the `height` attribute is the backing store's. */
function fit(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  const ratio = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = Number(canvas.dataset.height);
  canvas.width = Math.round(width * ratio);
  canvas.height = Math.round(height * ratio);
  canvas.style.height = `${height}px`;
  const ctx = canvas.getContext("2d")!;
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  return ctx;
}

function draw() {
  const entry = current();
  const t = entry?.track;
  if (!t) return;
  drawWave(t, entry.applied);
  drawLevels(t);
}

function drawWave(t: Track, applied: Applied | undefined) {
  const ctx = fit(wave);
  const w = wave.clientWidth;
  const h = Number(wave.dataset.height);
  const x = (secs: number) => (secs / t.duration_secs) * w;
  const top = 18; // room for the cue letters
  const mid = top + (h - top) / 2;
  const half = (h - top) / 2 - 4;

  // Bar lines, darker on each phrase from the start.
  const start = applied?.landmarks.start ?? 0;
  const barWidth = w / Math.max(t.bars.length, 1);
  t.bars.forEach((bar, i) => {
    const phrase = (i - start) % 8 === 0 && i >= start;
    if (!phrase && barWidth < 4) return;
    ctx.fillStyle = phrase ? css("--rule") : "#ece8de";
    ctx.fillRect(Math.round(x(bar.start_secs)), top, 1, h - top);
  });

  // The waveform: the whole signal light, the bass dark over it.
  const n = t.overview.length;
  const column = w / n;
  t.overview.forEach(([low, , , peak], i) => {
    const px = i * column;
    const ph = (peak / 255) * half;
    ctx.fillStyle = css("--wave");
    ctx.fillRect(px, mid - ph, Math.max(column, 1), ph * 2);
    const lh = (low / 255) * half;
    ctx.fillStyle = css("--bass");
    ctx.fillRect(px, mid - lh, Math.max(column, 1), lh * 2);
  });

  // Cues: a loop as a band, every cue as a line with its letter on top.
  // A new cue has a solid flag and line; one kept from rekordbox an open
  // flag in its colour and a dashed line.
  ctx.font = "bold 10px ui-monospace, Menlo, monospace";
  for (const cue of applied?.cues ?? []) {
    const colour = cue.color ? `rgb(${cue.color.join(",")})` : css("--ink");
    const cx = Math.round(x(cue.start_secs));
    if (cue.end_secs !== null) {
      ctx.globalAlpha = cue.existing ? 0.1 : 0.18;
      ctx.fillStyle = colour;
      ctx.fillRect(cx, top, Math.max(x(cue.end_secs) - cx, 2), h - top);
      ctx.globalAlpha = 1;
    }
    if (cue.existing) {
      ctx.strokeStyle = css("--ink");
      ctx.lineWidth = 1;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(cx + 0.5, top);
      ctx.lineTo(cx + 0.5, h);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.fillStyle = css("--card");
      ctx.fillRect(cx, 2, 16, 14);
      ctx.strokeStyle = cue.color ? colour : css("--ink");
      ctx.lineWidth = 2;
      ctx.strokeRect(cx + 1, 3, 14, 12);
      ctx.fillStyle = css("--ink");
    } else {
      ctx.fillStyle = css("--ink");
      ctx.fillRect(cx, top, 1.5, h - top);
      ctx.fillStyle = colour;
      ctx.fillRect(cx, 2, 16, 14);
      ctx.strokeStyle = css("--ink");
      ctx.lineWidth = 1;
      ctx.strokeRect(cx + 0.5, 2.5, 15, 13);
      ctx.fillStyle = cue.color && cue.color[0] + cue.color[1] < 200 ? "#fff" : css("--ink");
    }
    ctx.fillText(slotName(cue), cx + 5, 13);
  }
}

function drawLevels(t: Track) {
  const ctx = fit(levels);
  const w = levels.clientWidth;
  const h = Number(levels.dataset.height);
  const x = (secs: number) => (secs / t.duration_secs) * w;
  const y = (v: number) => h - 6 - v * (h - 12);
  const line = (key: "volume" | "bass", colour: string, width: number) => {
    ctx.beginPath();
    t.bars.forEach((bar, i) => {
      const yy = y(bar[key]);
      if (i === 0) ctx.moveTo(x(bar.start_secs), yy);
      else ctx.lineTo(x(bar.start_secs), yy);
      ctx.lineTo(x(bar.end_secs), yy);
    });
    ctx.strokeStyle = colour;
    ctx.lineWidth = width;
    ctx.stroke();
  };
  line("volume", css("--volume"), 1.5);
  line("bass", css("--bass"), 1.5);
}

function hover(event: MouseEvent) {
  const t = current()?.track;
  if (!t) return;
  const box = (event.currentTarget as HTMLCanvasElement).getBoundingClientRect();
  const secs = ((event.clientX - box.left) / box.width) * t.duration_secs;
  const i = barAt(t, secs);
  const bar = t.bars[i];
  $("hover").textContent = bar
    ? `${clock(secs)}  bar ${i + 1}  volume ${bar.volume.toFixed(2)}  bass ${bar.bass.toFixed(2)}  busy ${bar.busy.toFixed(2)}`
    : "";
}

// ---- wiring -------------------------------------------------------------

$("add").onclick = async () => {
  const picked = await open({ multiple: true, filters: [{ name: "Audio", extensions: AUDIO }] });
  if (picked) addPaths(Array.isArray(picked) ? picked : [picked]);
};

$("open-library").onclick = () => openLibrary();

$("open-xml").onclick = async () => {
  const picked = await open({ filters: [{ name: "rekordbox XML", extensions: ["xml"] }] });
  if (typeof picked === "string") await openRekordbox(picked);
};

$<HTMLSelectElement>("playlist").onchange = (event) => {
  const value = (event.target as HTMLSelectElement).value;
  const index = value === ALL_TRACKS ? null : Number(value);
  chosenPath = offered.find((p) => p.index === index)?.path ?? null;
  loadPlaylist(index);
};

$("export").onclick = async () => {
  const folder = openedXml ? openedXml.slice(0, openedXml.lastIndexOf("/") + 1) : "";
  const dest = await save({ defaultPath: `${folder}kfbox.xml`, filters: [{ name: "rekordbox XML", extensions: ["xml"] }] });
  if (!dest) return;
  try {
    const count = await invoke<number>("export_xml", { dest, rules: rulesText.value });
    setStatus(`Wrote ${count} songs to ${fileName(dest)}`);
  } catch (e) {
    setStatus(`Export failed: ${e}`);
  }
};

$("apply").onclick = () => applyRules(true);
$("reset").onclick = async () => {
  rulesText.value = await invoke<string>("default_rules");
  await applyRules(true);
};
rulesText.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    applyRules(true);
  }
});

for (const canvas of [wave, levels]) {
  canvas.addEventListener("mousemove", hover);
}
window.addEventListener("resize", draw);

getCurrentWebview().onDragDropEvent((event) => {
  if (event.payload.type !== "drop") return;
  const xml = event.payload.paths.find((p) => p.toLowerCase().endsWith(".xml"));
  if (xml) openRekordbox(xml);
  else addPaths(event.payload.paths);
});

rulesText.value = await invoke<string>("load_rules");
renderList();
renderDetail();
const named = await invoke<string[]>("initial_paths");
if (named.length) addPaths(named);
else await openLibrary();
