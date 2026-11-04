import { describe, expect, it } from "vitest";

import { beatPulse, lissajous } from "./Mark";

describe("the header mark", () => {
  it("draws one closed figure, the app icon's at phase π/2", () => {
    const points = lissajous(Math.PI / 2, 120);
    const [first, last] = [points[0]!, points[points.length - 1]!];
    expect(Math.hypot(first[0] - last[0], first[1] - last[1])).toBeLessThan(1e-9);
    // x = sin(3t + π/2) = cos(3t): at t = 0 it's at the right edge, halfway up.
    expect(first[0]).toBeCloseTo(1, 9);
    expect(first[1]).toBeCloseTo(0, 9);
  });

  it("pulses on each beat of the grid, strongest on the downbeat, fading before the next", () => {
    const beats = [0, 1, 2, 3, 4].map((n) => ({ secs: 1 + n * 0.5, number: (n % 4) + 1 }));
    expect(beatPulse(0.5, beats, 120)).toBe(0);
    expect(beatPulse(1, beats, 120)).toBeCloseTo(1, 6);
    expect(beatPulse(1.5, beats, 120)).toBeCloseTo(0.6, 6);
    expect(beatPulse(1.4, beats, 120)).toBeLessThan(0.05);
    expect(beatPulse(3.01, beats, 120)).toBeGreaterThan(0.9);
  });

  it("pulses from the BPM when there's no grid", () => {
    expect(beatPulse(2, [], 120)).toBeCloseTo(0.6, 6);
    expect(beatPulse(2.25, [], 120)).toBeLessThan(0.1);
    expect(beatPulse(2, [], 0)).toBe(0);
  });
});
