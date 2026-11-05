import { describe, expect, it } from "vitest";

import { GLIDE_MS, leadFor, playheadAcross, viewCentre } from "./view";

// A 16-bar window at 128 BPM: 30 s.
const span = 30;

describe("the zoomed waveform's view", () => {
  it("centres on the playhead with no overview click", () => {
    expect(viewCentre(100, span, null, 0)).toEqual({ centre: 100, done: true });
    expect(playheadAcross(100, 100, span)).toBe(0.5);
  });

  it("starts the window at an overview click, the playhead at its left edge, while it hasn't played", () => {
    const { centre } = viewCentre(100, span, { start: 100, since: null }, 5_000);
    expect(centre - span / 2).toBe(100);
    expect(playheadAcross(100, centre, span)).toBe(0);
  });

  it("moves the playhead line across the view on every frame as it plays, then holds it centred", () => {
    // The click at 100 s, playback from there at 1 s a second, drawn 60 times a second for 3 s.
    const lead = { start: 100, since: 0 };
    let last = -1;
    let lastCentre: number | null = null;
    for (let frame = 0; frame <= 180; frame++) {
      const now = (frame / 60) * 1000;
      const playhead = 100 + now / 1000;
      const { centre, done } = viewCentre(playhead, span, lead, now);
      const across = playheadAcross(playhead, centre, span);
      if (now < GLIDE_MS) {
        // Gliding: the line moves on across the view, every frame.
        expect(across).toBeGreaterThan(last);
        expect(done).toBe(false);
      } else {
        expect(across).toBeCloseTo(0.5, 9);
        expect(done).toBe(true);
      }
      // The view slides back half a window to bring the playhead to the middle, smoothly: never more than a
      // twentieth of the window between frames.
      if (lastCentre !== null) expect(Math.abs(centre - lastCentre)).toBeLessThan(span / 20);
      last = across;
      lastCentre = centre;
    }
  });

  it("ends the glide at the playhead's speed, with no jump", () => {
    const lead = { start: 100, since: 0 };
    const step = 1000 / 60;
    const at = (now: number) => viewCentre(100 + now / 1000, span, lead, now).centre;
    const before = at(GLIDE_MS - step) - at(GLIDE_MS - 2 * step);
    const after = at(GLIDE_MS + step) - at(GLIDE_MS);
    expect(Math.abs(at(GLIDE_MS) - at(GLIDE_MS - 1e-6))).toBeLessThan(1e-3);
    expect(after).toBeCloseTo(step / 1000, 6);
    expect(Math.abs(before - after)).toBeLessThan(0.002);
  });

  it("keeps the window inside the track near its end", () => {
    expect(leadFor(100, span, 300)).toBe(100);
    expect(leadFor(290, span, 300)).toBe(270);
    expect(leadFor(10, 400, 300)).toBe(0);
  });
});
