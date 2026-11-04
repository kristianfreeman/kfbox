import { describe, expect, it } from "vitest";

import { bandColour, bpmText, camelot, firstGenre } from "./format";

describe("format", () => {
  it("shows a whole BPM without decimals and a fractional one with one", () => {
    expect(bpmText(128)).toBe("128");
    expect(bpmText(127.999)).toBe("128");
    expect(bpmText(127.5)).toBe("127.5");
    expect(bpmText(0)).toBe("");
  });

  it("names every key in Camelot, whatever spelling rekordbox used", () => {
    expect(camelot("Dbm")).toBe("12A");
    expect(camelot("C#m")).toBe("12A");
    expect(camelot("Am")).toBe("8A");
    expect(camelot("A minor")).toBe("8A");
    expect(camelot("F#")).toBe("2B");
    expect(camelot("Gb")).toBe("2B");
    expect(camelot("C")).toBe("8B");
    expect(camelot("11a")).toBe("11A");
    expect(camelot("Ebm")).toBe("2A");
    expect(camelot("")).toBe("");
    expect(camelot("weird")).toBe("weird");
  });

  it("takes the first of several genres", () => {
    expect(firstGenre("Electro;Techno/House")).toBe("Electro");
    expect(firstGenre(" Techno ")).toBe("Techno");
    expect(firstGenre("")).toBe("");
  });

  it("colours a column by its bands, the strongest at full brightness", () => {
    expect(bandColour(200, 100, 0, 1)).toEqual([255, 128, 0]);
    // Saturated, a bass-led column is red, not orange or pink.
    expect(bandColour(200, 100, 0)).toEqual([255, 32, 0]);
    expect(bandColour(60, 90, 200)).toEqual([7, 23, 255]);
    expect(bandColour(0, 0, 0)).toEqual([0, 0, 0]);
  });
});
