import { describe, expect, it } from "vitest";

import { visibleRows } from "./TrackTable";

describe("the track table's drawn rows", () => {
  it("draws the rows in view and a few either side", () => {
    expect(visibleRows(0, 640, 106)).toEqual({ first: 0, last: 28 });
    expect(visibleRows(32 * 50, 640, 106)).toEqual({ first: 42, last: 78 });
  });

  it("still draws the end of the list when the scroll it holds is past it", () => {
    // Scrolled far down a long list, then a shorter one shows: the browser clamps the scroll, and may not say so.
    const { first, last } = visibleRows(32 * 90, 640, 12);
    expect(first).toBe(0);
    expect(last).toBe(12);
  });

  it("draws nothing only when there's nothing", () => {
    expect(visibleRows(5000, 640, 0)).toEqual({ first: 0, last: 0 });
    expect(visibleRows(-40, 640, 3)).toEqual({ first: 0, last: 3 });
  });
});
