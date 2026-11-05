/** Where a click on a waveform lands with snapping on: on a cue near it, else on the nearest beat of the grid. */

/** How near a cue a click must be to land on it, in points on screen, in the waveform's own scale. */
export const CUE_PULL_PT = 8;

export interface Snapped {
  secs: number;
  /** What it landed on, if anything. */
  to: "cue" | "beat" | null;
}

/** The value in `sorted` nearest `secs`, by binary search. */
function nearest(sorted: number[], secs: number): number | undefined {
  if (sorted.length === 0) return undefined;
  let low = 0;
  let high = sorted.length - 1;
  while (low < high) {
    const middle = (low + high) >> 1;
    if ((sorted[middle] ?? 0) < secs) low = middle + 1;
    else high = middle;
  }
  const after = sorted[low] ?? secs;
  const before = sorted[low - 1];
  return before !== undefined && secs - before <= after - secs ? before : after;
}

/** Snaps `secs`: to the nearest cue within `pullSecs` (a cue always wins over a beat), else to the nearest beat,
 * else nowhere. `cues` and `beats` are times in seconds, in order. */
export function snapTime(secs: number, cues: number[], beats: number[], pullSecs: number): Snapped {
  const cue = nearest(cues, secs);
  if (cue !== undefined && Math.abs(cue - secs) <= pullSecs) return { secs: cue, to: "cue" };
  const beat = nearest(beats, secs);
  if (beat !== undefined) return { secs: beat, to: "beat" };
  return { secs, to: null };
}
