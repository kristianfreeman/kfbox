import type { Applied, Track } from "../ipc/types";
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

export function drawWave(canvas: HTMLCanvasElement, height: number, track: Track, applied: Applied | undefined) {
  const ctx = fit(canvas, height);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const h = height;
  const x = (secs: number) => (secs / track.duration_secs) * w;
  const top = 20; // room for the cue letters
  const mid = top + (h - top) / 2;
  const half = (h - top) / 2 - 4;
  const ink = token("--ink");

  // Bar lines, stronger on each phrase from the start.
  const start = applied?.landmarks.start ?? 0;
  const barWidth = w / Math.max(track.bars.length, 1);
  track.bars.forEach((bar, i) => {
    const phrase = (i - start) % 8 === 0 && i >= start;
    if (!phrase && barWidth < 4) return;
    ctx.fillStyle = token(phrase ? "--grid-phrase" : "--grid-bar");
    ctx.fillRect(Math.round(x(bar.start_secs)), top, 1, h - top);
  });

  // The waveform: the whole signal soft, the bass strong over it.
  const column = w / track.overview.length;
  const wave = token("--wave");
  const bass = token("--bass");
  track.overview.forEach(([low, , , peak], i) => {
    const px = i * column;
    const ph = (peak / 255) * half;
    ctx.fillStyle = wave;
    ctx.fillRect(px, mid - ph, Math.max(column, 1), ph * 2);
    const lh = (low / 255) * half;
    ctx.fillStyle = bass;
    ctx.fillRect(px, mid - lh, Math.max(column, 1), lh * 2);
  });

  // Cues: a loop as a band, every cue as a line with its letter on top. A new
  // cue has a solid flag and line; one kept from rekordbox an open flag in its
  // colour and a dashed line.
  ctx.font = `600 10px ${token("--mono")}`;
  for (const cue of applied?.cues ?? []) {
    const colour = cue.color ? `rgb(${cue.color.join(",")})` : ink;
    const cx = Math.round(x(cue.start_secs));
    if (cue.end_secs !== null) {
      ctx.globalAlpha = cue.existing ? 0.12 : 0.22;
      ctx.fillStyle = colour;
      ctx.fillRect(cx, top, Math.max(x(cue.end_secs) - cx, 2), h - top);
      ctx.globalAlpha = 1;
    }
    if (cue.existing) {
      ctx.strokeStyle = ink;
      ctx.lineWidth = 1;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(cx + 0.5, top);
      ctx.lineTo(cx + 0.5, h);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.strokeStyle = colour;
      ctx.lineWidth = 2;
      ctx.strokeRect(cx + 1, 3, 14, 13);
      ctx.fillStyle = ink;
    } else {
      ctx.fillStyle = ink;
      ctx.fillRect(cx, top, 1.5, h - top);
      ctx.fillStyle = colour;
      ctx.fillRect(cx, 2, 16, 15);
      // Dark letters on bright flags, light ones on deep colours.
      const [r, g, b] = cue.color ?? [255, 255, 255];
      ctx.fillStyle = 0.3 * r + 0.59 * g + 0.11 * b > 120 ? "#000" : "#fff";
    }
    ctx.fillText(slotName(cue), cx + 5, 13.5);
  }
}

export function drawLevels(canvas: HTMLCanvasElement, height: number, track: Track) {
  const ctx = fit(canvas, height);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const x = (secs: number) => (secs / track.duration_secs) * w;
  const y = (v: number) => height - 6 - v * (height - 12);
  const line = (key: "volume" | "bass", colour: string) => {
    ctx.beginPath();
    track.bars.forEach((bar, i) => {
      const yy = y(bar[key]);
      if (i === 0) ctx.moveTo(x(bar.start_secs), yy);
      else ctx.lineTo(x(bar.start_secs), yy);
      ctx.lineTo(x(bar.end_secs), yy);
    });
    ctx.strokeStyle = colour;
    ctx.lineWidth = 1.5;
    ctx.stroke();
  };
  line("volume", token("--volume"));
  line("bass", token("--bass"));
}
