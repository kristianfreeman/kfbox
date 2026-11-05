import type { Cue, Track } from "../ipc/types";
import { bandColour, cueColour, onColour, slotName } from "../lib/format";

const token = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** Sizes a canvas to its box at the screen's pixel density. Its CSS height is
 * passed in: the `height` attribute is the backing store's. */
function fit(canvas: HTMLCanvasElement, height: number): CanvasRenderingContext2D | null {
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.round(canvas.clientWidth * ratio);
  canvas.height = Math.round(height * ratio);
  canvas.style.height = `${height}px`;
  const ctx = canvas.getContext("2d");
  ctx?.setTransform(ratio, 0, 0, ratio, 0, 0);
  return ctx;
}

/** A column's colour as CSS: its bands as red, green and blue, scaled by the theme's gain so light mode isn't garish. */
function columnColour(low: number, mid: number, high: number, gain: number): string {
  const [r, g, b] = bandColour(low, mid, high);
  return `rgb(${Math.round(r * gain)},${Math.round(g * gain)},${Math.round(b * gain)})`;
}

const gainOf = () => Number(token("--wave-gain")) || 1;

/** Draws one column centred on `centre`, as kfbox always drew it: the whole signal soft, the bass strong over it,
 * so breaks, builds and drops show in the shape. The bass is coloured by the column's band balance: red where the
 * lows lead, green the mids, blue the highs. `half` is the height a full-scale value reaches either way. */
function drawColumn(
  ctx: CanvasRenderingContext2D,
  px: number,
  width: number,
  centre: number,
  half: number,
  column: number[],
  gain: number,
  soft: string,
) {
  const [low = 0, middle = 0, high = 0, peak = 0] = column;
  const bar = (value: number) => {
    const h = Math.max((value / 255) * half, 0.5);
    ctx.fillRect(px, centre - h, width, h * 2);
  };
  // The whole signal, soft, as context.
  ctx.fillStyle = soft;
  bar(peak);
  ctx.fillStyle = columnColour(low, middle, high, gain);
  bar(low);
}

/** The part of a track on screen, in seconds. */
export interface View {
  start: number;
  end: number;
}

/** The whole track as one strip, with its cues: a hot cue as a 2 px line in its colour, outlined dark so it reads
 * over any waveform, with its lettered flag in the `band` pixels above the strip; a memory cue as a small triangle
 * there. */
export function drawStrip(canvas: HTMLCanvasElement, height: number, track: Track, cues: Cue[] = [], band = 0) {
  const ctx = fit(canvas, height);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const strip = height - band;
  if (band > 0) {
    ctx.fillStyle = token("--scope");
    ctx.beginPath();
    ctx.roundRect(0, band, w, strip, 4);
    ctx.fill();
  }
  const column = w / track.overview.length;
  const gain = gainOf();
  const soft = token("--wave-soft");
  track.overview.forEach((values, i) => drawColumn(ctx, i * column, Math.max(column, 1), band + strip / 2, strip / 2, values, gain, soft));

  ctx.font = `700 9px ${token("--sans")}`;
  ctx.textAlign = "center";
  const at = (cue: Cue) => Math.round((cue.start_secs / track.duration_secs) * w);
  // Memory cues first, so hot cues' flags sit over them.
  for (const cue of cues.filter((c) => c.hot === null)) {
    const x = at(cue);
    ctx.fillStyle = token("--muted");
    ctx.beginPath();
    ctx.moveTo(x - 3, Math.max(band - 6, 0));
    ctx.lineTo(x + 3, Math.max(band - 6, 0));
    ctx.lineTo(x, Math.max(band - 1, 5));
    ctx.closePath();
    ctx.fill();
  }
  for (const cue of cues.filter((c) => c.hot !== null)) {
    const x = at(cue);
    const rgb = cueColour(cue) ?? [255, 255, 255];
    const colour = `rgb(${rgb.join(",")})`;
    ctx.fillStyle = "rgb(0 0 0 / 70%)";
    ctx.fillRect(x - 2, band, 4, strip);
    ctx.fillStyle = colour;
    ctx.fillRect(x - 1, band, 2, strip);
    if (band > 0) {
      ctx.beginPath();
      ctx.roundRect(x - 1, 0, 11, band - 1, [2, 2, 2, 0]);
      ctx.fill();
      ctx.fillStyle = onColour(rgb);
      ctx.fillText(slotName(cue), x + 4.5, band - 3.5);
    }
  }
  ctx.textAlign = "start";
}

/** Device pixels per tile of the zoomed waveform's tape. */
const TILE = 1024;

/** What the zoomed waveform draws with, read from the theme once rather than every frame. */
interface Look {
  soft: string;
  gain: number;
  ink: string;
  accent: string;
  bg: string;
  phrase: string;
  bar: string;
  sans: string;
}

const lookNow = (): Look => ({
  soft: token("--wave-soft"),
  gain: gainOf(),
  ink: token("--ink"),
  accent: token("--accent"),
  bg: token("--bg"),
  phrase: token("--grid-phrase"),
  bar: token("--grid-bar"),
  sans: token("--sans"),
});

/** What the zoomed waveform shows: a track, its cues and grid, and the finer waveform once it has arrived. */
export interface Scene {
  track: Track;
  cues: Cue[];
  /** The bar phrases count from. */
  start: number;
  closer: number[] | null;
  selected: string | null;
}

/** The zoomed waveform, a window `span` seconds wide that scrolls as the track plays.
 *
 * The waveform is drawn once, at device pixels, onto a tape of tiles: each device pixel column holds the loudest of
 * the analyser's columns that fall in it, so nothing lands on half pixels. Each frame copies the tiles in view at a
 * whole-pixel offset and draws the grid and cues over them at the same offset, so as it scrolls every pixel moves
 * together and nothing shimmers or bands. The tape is kept until the track, the zoom, the width or the theme change. */
export class ZoomedWave {
  private look: Look = lookNow();
  private tiles = new Map<number, HTMLCanvasElement | null>();
  private tape = "";
  private size = { width: 0, ratio: 0 };

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly height: number,
  ) {}

  /** After a change of theme: the colours are read again and the tape redrawn. */
  restyle() {
    this.look = lookNow();
    this.tiles.clear();
  }

  /** Draws the window around `centre`, and returns the part of the track it shows, snapped to whole device pixels. */
  draw(scene: Scene, centre: number, span: number): View {
    const ratio = window.devicePixelRatio || 1;
    const width = Math.round(this.canvas.clientWidth * ratio);
    const height = Math.round(this.height * ratio);
    if (width !== this.size.width || ratio !== this.size.ratio) {
      this.canvas.width = width;
      this.canvas.height = height;
      this.canvas.style.height = `${this.height}px`;
      this.size = { width, ratio };
      this.tiles.clear();
    }
    const { track, closer } = scene;
    const pps = width / span; // device pixels per second
    const tape = `${track.path}|${track.duration_secs}|${pps}|${height}|${closer?.length ?? 0}`;
    if (tape !== this.tape) {
      this.tape = tape;
      this.tiles.clear();
    }
    const offset = Math.round((centre - span / 2) * pps);
    const view = { start: offset / pps, end: (offset + width) / pps };
    const ctx = this.canvas.getContext("2d");
    if (!ctx || width === 0) return view;

    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width, height);
    // Every position in device pixels, whole, from the same offset as the tape.
    const at = (secs: number) => Math.round(secs * pps) - offset;
    const look = this.look;
    const top = Math.round(20 * ratio); // room for the cue letters
    const line = Math.max(1, Math.round(ratio));

    // Bar lines, stronger on each phrase from the start.
    const barPx = pps * (240 / (track.bpm || 120));
    // Each line is kept or culled by its pixel, not its time, so it moves exactly as the tape does.
    const inView = (px: number) => px > -line && px < width;
    track.bars.forEach((bar, i) => {
      const px = at(bar.start_secs);
      if (!inView(px)) return;
      const phrase = (i - scene.start) % 8 === 0 && i >= scene.start;
      if (!phrase && barPx < 4 * ratio) return;
      ctx.fillStyle = phrase ? look.phrase : look.bar;
      ctx.fillRect(px, top, line, height - top);
    });

    // Beat ticks along the top and bottom edges, once beats are far enough apart to read.
    if (pps * (60 / (track.bpm || 120)) > 6 * ratio) {
      ctx.fillStyle = look.phrase;
      for (const beat of track.beats) {
        const px = at(beat.secs);
        if (!inView(px)) continue;
        const tick = Math.round((beat.number === 1 ? 6 : 3) * ratio);
        ctx.fillRect(px, top, line, tick);
        ctx.fillRect(px, height - tick, line, tick);
      }
    }

    // The waveform, from the tape.
    for (let k = Math.floor(offset / TILE); k * TILE < offset + width; k++) {
      const tile = this.tile(k, scene, pps, height, top);
      if (tile) ctx.drawImage(tile, k * TILE - offset, 0);
    }

    // Cues: a loop as a band, every cue as a line with its letter on a flag. The picked cue's line is the accent.
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.font = `600 10px ${look.sans}`;
    ctx.textAlign = "center";
    const css = (secs: number) => at(secs) / ratio;
    const cssHeight = height / ratio;
    for (const cue of scene.cues) {
      if (cue.start_secs > view.end || (cue.end_secs ?? cue.start_secs) < view.start) continue;
      const rgb = cueColour(cue);
      const colour = rgb ? `rgb(${rgb.join(",")})` : look.ink;
      const cx = css(cue.start_secs);
      const picked = cue.id !== null && cue.id === scene.selected;
      if (cue.end_secs !== null) {
        ctx.globalAlpha = 0.22;
        ctx.fillStyle = colour;
        ctx.fillRect(cx, 20, Math.max(css(cue.end_secs) - cx, 2), cssHeight - 20);
        ctx.globalAlpha = 1;
      }
      ctx.fillStyle = picked ? look.accent : look.ink;
      ctx.fillRect(cx, 20, picked ? 2.5 : 1.5, cssHeight - 20);
      // A flag in the cue's colour, its letter dark or light to read on it.
      ctx.fillStyle = colour;
      ctx.beginPath();
      ctx.roundRect(cx, 2, 16, 15, [0, 3, 3, 0]);
      ctx.fill();
      ctx.fillStyle = rgb ? onColour(rgb) : look.bg;
      ctx.fillText(slotName(cue), cx + 8, 13);
    }
    ctx.textAlign = "start";
    return view;
  }

  /** Tile `k` of the tape, drawn the first time it's needed. Before the track's start or past its end, nothing. */
  private tile(k: number, scene: Scene, pps: number, height: number, top: number): HTMLCanvasElement | null {
    const cached = this.tiles.get(k);
    if (cached !== undefined) return cached;
    const { track, closer } = scene;
    const from = k * TILE;
    const total = Math.ceil(track.duration_secs * pps);
    let tile: HTMLCanvasElement | null = null;
    const ctx = from + TILE > 0 && from < total ? (tile = document.createElement("canvas")).getContext("2d") : null;
    if (tile && ctx) {
      tile.width = TILE;
      tile.height = height;
      const columns = closer ? closer.length / 4 : track.overview.length;
      const perSec = columns / track.duration_secs;
      const value = (i: number, band: number) => (closer ? (closer[i * 4 + band] ?? 0) : (track.overview[i]?.[band] ?? 0));
      const mid = top + (height - top) / 2;
      const half = (height - top) / 2 - 4 * this.size.ratio;
      const bar = (v: number) => {
        const h = Math.max((v / 255) * half, 0.5);
        const y = Math.round(mid - h);
        return [y, Math.max(1, Math.round(mid + h) - y)] as const;
      };
      for (let p = Math.max(0, -from); p < TILE && from + p < total; p++) {
        // The analyser's columns under this pixel: the loudest of each band.
        const first = Math.floor(((from + p) / pps) * perSec);
        const last = Math.max(first + 1, Math.floor(((from + p + 1) / pps) * perSec));
        let low = 0;
        let middle = 0;
        let high = 0;
        let peak = 0;
        for (let i = first; i < last && i < columns; i++) {
          low = Math.max(low, value(i, 0));
          middle = Math.max(middle, value(i, 1));
          high = Math.max(high, value(i, 2));
          peak = Math.max(peak, value(i, 3));
        }
        // The whole signal soft, as context; the bass strong over it, coloured by the band balance.
        const [softY, softH] = bar(peak);
        ctx.fillStyle = this.look.soft;
        ctx.fillRect(p, softY, 1, softH);
        const [lowY, lowH] = bar(low);
        ctx.fillStyle = columnColour(low, middle, high, this.look.gain);
        ctx.fillRect(p, lowY, 1, lowH);
      }
    } else {
      tile = null;
    }
    this.tiles.set(k, tile);
    return tile;
  }
}
