import { describe, expect, it } from "vitest";

import type { Track } from "../ipc/types";
import { barPhase, energyAt, loudness } from "./Glow";

/** A minute of track at 10 columns a second: a quiet break for the first 30 s, then a drop. */
const track = {
  duration_secs: 60,
  overview: Array.from({ length: 600 }, (_, i) => (i < 300 ? [20, 20, 20, 40] : [220, 120, 90, 240])),
} as unknown as Track;

describe("the activity view's glow", () => {
  it("judges energy against the track's own loud parts, over the stretch before the playhead", () => {
    const loud = loudness(track);
    expect(energyAt(track, 20, loud)).toBeLessThan(0.2);
    expect(energyAt(track, 45, loud)).toBeCloseTo(1, 5);
    // Just into the drop, part of the stretch is still the break: it swells rather than jumps.
    const into = energyAt(track, 30.5, loud);
    expect(into).toBeGreaterThan(energyAt(track, 20, loud));
    expect(into).toBeLessThan(energyAt(track, 31.5, loud));
  });

  it("gives no energy for a track with no waveform, and stays within 0 and 1 at the ends", () => {
    const loud = loudness(track);
    expect(energyAt({ ...track, overview: [] }, 10, 1)).toBe(0);
    expect(energyAt(track, 0, loud)).toBeGreaterThanOrEqual(0);
    expect(energyAt(track, 999, loud)).toBeLessThanOrEqual(1);
  });

  it("finds the playhead's place in its bar from the grid's first downbeat", () => {
    const beats = [
      { secs: 0.2, number: 4, bpm: 120 },
      { secs: 0.7, number: 1, bpm: 120 },
    ];
    expect(barPhase(0.7, beats, 2000)).toBe(0);
    expect(barPhase(1.7, beats, 2000)).toBeCloseTo(1000, 6);
    expect(barPhase(0.2, beats, 2000)).toBeCloseTo(1500, 6);
  });
});
