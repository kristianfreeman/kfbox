import type { Applied, Cue, Track } from "../ipc/types";

export const fileName = (path: string) => path.split("/").pop() ?? path;

export const slotName = (cue: Cue) => (cue.hot === null ? "M" : String.fromCharCode(65 + cue.hot));

export const trackName = (track: Track) => [track.tags.artist, track.tags.title].filter(Boolean).join(" – ");

export function clock(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
}

export function barAt(track: Track, secs: number): number {
  let index = 0;
  track.bars.forEach((bar, i) => {
    if (bar.start_secs <= secs + 0.01) index = i;
  });
  return index;
}

/** Hot cue letters, and memory cues as a count: a track can have dozens. */
export function cueSummary(applied: Applied | undefined, existing: boolean): string {
  const cues = (applied?.cues ?? []).filter((c) => c.existing === existing);
  const hot = cues.filter((c) => c.hot !== null).map(slotName).join("");
  const memory = cues.length - hot.length;
  return [hot, memory ? `${memory}M` : ""].filter(Boolean).join(" ");
}
