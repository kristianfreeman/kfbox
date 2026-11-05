import { describe, expect, it } from "vitest";

import { columnColour, loudestColumn } from "./waveform";

const rgb = (css: string) => css.match(/\d+/g)!.map(Number) as [number, number, number];

describe("the waveforms' colours", () => {
  it("draws a column of highs blue, and a mixed column as a mix rather than its leading band", () => {
    const [r, g, b] = rgb(columnColour(10, 20, 200, 1));
    expect(b).toBe(255);
    expect(Math.max(r, g)).toBeLessThan(80);
    // A kick with mids and hats under it: still led by red, but not pure red.
    const [kr, kg, kb] = rgb(columnColour(220, 150, 110, 1));
    expect(kr).toBe(255);
    expect(kg).toBeGreaterThan(100);
    expect(kb).toBeGreaterThan(60);
  });

  it("colours the zoomed waveform's pixels as the overview colours the same column", () => {
    // Four columns: a kick, a hat, a quiet pad, a full-band hit; [low, mid, high, peak] each.
    const columns = [
      [230, 90, 40, 240],
      [20, 70, 210, 215],
      [40, 60, 50, 70],
      [200, 180, 170, 250],
    ];
    const value = (i: number, band: number) => columns[i]?.[band] ?? 0;
    for (let i = 0; i < columns.length; i++) {
      const [low, mid, high] = loudestColumn(value, i, i + 1);
      const [ol, om, oh] = columns[i]!;
      expect(columnColour(low, mid, high, 1)).toBe(columnColour(ol!, om!, oh!, 1));
    }
    // One pixel over several columns draws the loudest of them whole, not a blend of their bands.
    expect(loudestColumn(value, 0, 4)).toEqual([200, 180, 170, 250]);
    expect(loudestColumn(value, 0, 2)).toEqual([230, 90, 40, 240]);
  });
});
