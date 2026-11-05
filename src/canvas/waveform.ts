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

/** Draws what's in `view`. `closer` is the finer waveform, flat `[low, mid, high, peak]` columns, once it has arrived. */
export function drawWave(
  canvas: HTMLCanvasElement,
  height: number,
  track: Track,
  cues: Cue[],
  start: number,
  view: View = { start: 0, end: track.duration_secs },
  closer: number[] | null = null,
  selected: string | null = null,
) {
  const ctx = fit(canvas, height);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const h = height;
  const x = (secs: number) => ((secs - view.start) / (view.end - view.start)) * w;
  const top = 20; // room for the cue letters
  const mid = top + (h - top) / 2;
  const half = (h - top) / 2 - 4;
  const ink = token("--ink");

  // Bar lines, stronger on each phrase from the start.
  const barWidth = (w / Math.max(track.bars.length, 1)) * (track.duration_secs / (view.end - view.start));
  track.bars.forEach((bar, i) => {
    if (bar.end_secs < view.start || bar.start_secs > view.end) return;
    const phrase = (i - start) % 8 === 0 && i >= start;
    if (!phrase && barWidth < 4) return;
    ctx.fillStyle = token(phrase ? "--grid-phrase" : "--grid-bar");
    ctx.fillRect(Math.round(x(bar.start_secs)), top, 1, h - top);
  });

  // Beat ticks along the top and bottom edges, once beats are far enough apart to read.
  const beatPx = (w / (view.end - view.start)) * (60 / (track.bpm || 120));
  if (beatPx > 6) {
    ctx.fillStyle = token("--grid-phrase");
    for (const beat of track.beats) {
      if (beat.secs < view.start || beat.secs > view.end) continue;
      const bx = Math.round(x(beat.secs));
      const tick = beat.number === 1 ? 6 : 3;
      ctx.fillRect(bx, top, 1, tick);
      ctx.fillRect(bx, h - tick, 1, tick);
    }
  }

  // The waveform in RGB: each column's lows, mids and highs as red, green and
  // blue, scaled to the brightest; its height is the overall level.
  const gain = gainOf();
  const soft = token("--wave-soft");
  const columns = closer ? closer.length / 4 : track.overview.length;
  const perSec = columns / track.duration_secs;
  const first = Math.max(0, Math.floor(view.start * perSec));
  const last = Math.min(columns, Math.ceil(view.end * perSec));
  const width = Math.max(w / Math.max(last - first, 1), 1);
  for (let i = first; i < last; i++) {
    const [low, middle, high, peak] = closer
      ? [closer[i * 4] ?? 0, closer[i * 4 + 1] ?? 0, closer[i * 4 + 2] ?? 0, closer[i * 4 + 3] ?? 0]
      : (track.overview[i] ?? [0, 0, 0, 0]);
    drawColumn(ctx, x(i / perSec), width, mid, half, [low, middle, high, peak], gain, soft);
  }

  // Cues: a loop as a band, every cue as a line with its letter on a flag.
  // The picked cue's line is the accent colour.
  ctx.font = `600 10px ${token("--sans")}`;
  for (const cue of cues) {
    const rgb = cueColour(cue);
    const colour = rgb ? `rgb(${rgb.join(",")})` : ink;
    const cx = Math.round(x(cue.start_secs));
    if (cue.start_secs > view.end || (cue.end_secs ?? cue.start_secs) < view.start) continue;
    const picked = cue.id !== null && cue.id === selected;
    if (cue.end_secs !== null) {
      ctx.globalAlpha = 0.22;
      ctx.fillStyle = colour;
      ctx.fillRect(cx, top, Math.max(x(cue.end_secs) - cx, 2), h - top);
      ctx.globalAlpha = 1;
    }
    ctx.fillStyle = picked ? token("--accent") : ink;
    ctx.fillRect(cx, top, picked ? 2.5 : 1.5, h - top);
    // A flag in the cue's colour, its letter dark or light to read on it.
    ctx.fillStyle = colour;
    ctx.beginPath();
    ctx.roundRect(cx, 2, 16, 15, [0, 3, 3, 0]);
    ctx.fill();
    ctx.fillStyle = rgb ? onColour(rgb) : token("--bg");
    ctx.textAlign = "center";
    ctx.fillText(slotName(cue), cx + 8, 13);
    ctx.textAlign = "start";
  }
}
