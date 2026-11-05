import { describe, expect, it } from "vitest";

import { snapTime } from "./snap";

// 120 BPM: a beat every half second.
const beats = Array.from({ length: 40 }, (_, i) => i * 0.5);

describe("snapping a click on a waveform", () => {
  it("lands on the nearest beat", () => {
    expect(snapTime(3.2, [], beats, 0.1)).toEqual({ secs: 3, to: "beat" });
    expect(snapTime(3.3, [], beats, 0.1)).toEqual({ secs: 3.5, to: "beat" });
    expect(snapTime(-1, [], beats, 0.1)).toEqual({ secs: 0, to: "beat" });
  });

  it("lands on a cue within its pull, even with a beat nearer", () => {
    // The drop's cue sits a little off the grid; a click near it hits the drop, not the beat by it.
    expect(snapTime(10.02, [10.08], beats, 0.1)).toEqual({ secs: 10.08, to: "cue" });
  });

  it("falls back to the beat outside a cue's pull", () => {
    expect(snapTime(9.8, [10.08], beats, 0.1)).toEqual({ secs: 10, to: "beat" });
  });

  it("with no grid, lands only on cues, and otherwise where it was clicked", () => {
    expect(snapTime(5.03, [5], [], 0.1)).toEqual({ secs: 5, to: "cue" });
    expect(snapTime(7.3, [5], [], 0.1)).toEqual({ secs: 7.3, to: null });
  });
});
