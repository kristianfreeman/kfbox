/** Where the zoomed waveform centres, given the playhead, how many seconds it shows, and `lead`: where a click on
 * the overview started the window, if one did.
 *
 * Normally on the playhead, as a CDJ's. After an overview click the window starts at the click: it holds still
 * with `lead` at its left edge while the playhead runs from there to the middle, then follows the playhead from
 * the moment it gets there, at the playhead's own speed, so there's no jump. A playhead behind `lead` (a seek back)
 * centres at once. `held` says whether the lead is still in force; once it isn't, it's done with. */
export function viewCentre(playhead: number, span: number, lead: number | null): { centre: number; held: boolean } {
  if (lead === null || playhead < lead) return { centre: playhead, held: false };
  const middle = lead + span / 2;
  return playhead < middle ? { centre: middle, held: true } : { centre: playhead, held: false };
}

/** Where an overview click at `secs` starts the window: there, or as near the end as a whole window fits. */
export function leadFor(secs: number, span: number, duration: number): number {
  return Math.max(0, Math.min(secs, duration - span));
}
