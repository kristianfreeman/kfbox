/** How long the zoomed view takes to glide from where an overview click started it to centring the playhead. */
export const GLIDE_MS = 1000;

/** Where an overview click started the zoomed window, and when playback set off from it (null while it hasn't). */
export interface Lead {
  start: number;
  since: number | null;
}

/** Fast at first and settling: the view arrives on the playhead at the playhead's own speed. */
const easeOut = (t: number) => 1 - (1 - t) ** 3;

/** Where the zoomed waveform centres, given the playhead, how many seconds the view shows, the overview click's
 * `lead` if there was one, and the time now in milliseconds.
 *
 * Normally on the playhead, as a CDJ's. After an overview click the window starts at the click, the playhead at its
 * left edge, for as long as playback hasn't started. Once it plays, the view glides over GLIDE_MS, easing out, until
 * the playhead is centred, and follows it from there; the ease ends at the playhead's speed, so there's no jolt.
 * `done` says the lead has served its purpose. */
export function viewCentre(playhead: number, span: number, lead: Lead | null, now: number): { centre: number; done: boolean } {
  if (lead === null) return { centre: playhead, done: true };
  const start = lead.start + span / 2;
  if (lead.since === null) return { centre: start, done: false };
  const t = Math.max(0, Math.min(1, (now - lead.since) / GLIDE_MS));
  if (t >= 1) return { centre: playhead, done: true };
  return { centre: start + (playhead - start) * easeOut(t), done: false };
}

/** Where the playhead line sits across the view, 0 at its left edge to 1 at its right. */
export function playheadAcross(playhead: number, centre: number, span: number): number {
  return (playhead - (centre - span / 2)) / span;
}

/** Where an overview click at `secs` starts the window: there, or as near the end as a whole window fits. */
export function leadFor(secs: number, span: number, duration: number): number {
  return Math.max(0, Math.min(secs, duration - span));
}
