import { describe, expect, it } from "vitest";

import { leadFor, viewCentre } from "./view";

// A 16-bar window at 128 BPM: 30 s.
const span = 30;

describe("the zoomed waveform's view", () => {
  it("centres on the playhead with no overview click", () => {
    expect(viewCentre(100, span, null)).toEqual({ centre: 100, held: false });
  });

  it("starts the window at an overview click: the playhead at its left edge", () => {
    const view = viewCentre(100, span, 100);
    expect(view.centre - span / 2).toBe(100);
    expect(view.held).toBe(true);
  });

  it("holds still while the playhead is left of the middle", () => {
    for (const playhead of [100, 104, 110, 114.99]) expect(viewCentre(playhead, span, 100).centre).toBe(115);
  });

  it("hands off at the middle without a jump, then follows the playhead", () => {
    const before = viewCentre(114.999, span, 100).centre;
    const at = viewCentre(115, span, 100);
    const after = viewCentre(115.001, span, 100);
    expect(Math.abs(at.centre - before)).toBeLessThan(0.002);
    expect(at).toEqual({ centre: 115, held: false });
    expect(after.centre).toBeCloseTo(115.001, 6);
  });

  it("centres at once on a playhead behind the click, as after a seek back", () => {
    expect(viewCentre(90, span, 100)).toEqual({ centre: 90, held: false });
  });

  it("keeps the window inside the track near its end", () => {
    expect(leadFor(100, span, 300)).toBe(100);
    expect(leadFor(290, span, 300)).toBe(270);
    expect(leadFor(10, 400, 300)).toBe(0);
  });
});
