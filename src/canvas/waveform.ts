import type { Cue, Track } from "../ipc/types";
import { slotName } from "../lib/format";

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

/** The part of a track on screen, in seconds. */
export interface View {
  start: number;
  end: number;
}

/** The whole track as one strip, for the bar along the bottom, with a line at each cue. */
export function drawStrip(canvas: HTMLCanvasElement, height: number, track: Track, cues: Cue[] = []) {
  const ctx = fit(canvas, height);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const column = w / track.overview.length;
  ctx.fillStyle = token("--strip");
  track.overview.forEach(([, , , peak], i) => {
    const ph = Math.max((peak / 255) * height, 1);
    ctx.fillRect(i * column, (height - ph) / 2, Math.max(column, 1), ph);
  });
  for (const cue of cues) {
    ctx.fillStyle = cue.color ? `rgb(${cue.color.join(",")})` : token("--ink");
    ctx.fillRect(Math.round((cue.start_secs / track.duration_secs) * w), 0, 1.5, height);
  }
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

  // The waveform: the whole signal soft, the bass strong over it.
  const wave = token("--wave");
  const bass = token("--bass");
  const columns = closer ? closer.length / 4 : track.overview.length;
  const perSec = columns / track.duration_secs;
  const first = Math.max(0, Math.floor(view.start * perSec));
  const last = Math.min(columns, Math.ceil(view.end * perSec));
  const width = Math.max(w / Math.max(last - first, 1), 1);
  for (let i = first; i < last; i++) {
    const low = closer ? (closer[i * 4] ?? 0) : (track.overview[i]?.[0] ?? 0);
    const peak = closer ? (closer[i * 4 + 3] ?? 0) : (track.overview[i]?.[3] ?? 0);
    const px = x(i / perSec);
    const ph = (peak / 255) * half;
    ctx.fillStyle = wave;
    ctx.fillRect(px, mid - ph, width, ph * 2);
    const lh = (low / 255) * half;
    ctx.fillStyle = bass;
    ctx.fillRect(px, mid - lh, width, lh * 2);
  }

  // Cues: a loop as a band, every cue as a line with its letter on a flag.
  // The picked cue's line is the accent colour.
  ctx.font = `600 10px ${token("--mono")}`;
  for (const cue of cues) {
    const colour = cue.color ? `rgb(${cue.color.join(",")})` : ink;
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
    ctx.fillStyle = colour;
    ctx.fillRect(cx, 2, 16, 15);
    // Dark letters on bright flags, light ones on deep colours.
    const [r, g, b] = cue.color ?? [255, 255, 255];
    ctx.fillStyle = 0.3 * r + 0.59 * g + 0.11 * b > 120 ? "#000" : "#fff";
    ctx.fillText(slotName(cue), cx + 5, 13.5);
  }
}
