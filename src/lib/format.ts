import type { Applied, Cue, Track } from "../ipc/types";

export const fileName = (path: string) => path.split("/").pop() ?? path;

export const slotName = (cue: Cue) => (cue.hot === null ? "M" : String.fromCharCode(65 + cue.hot));

/** Minutes and seconds, for a table. */
export function duration(secs: number): string {
  const whole = Math.round(secs);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

export function clock(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
}

export function barAt(track: Track, secs: number): number {
  let index = 0;
  track.bars.forEach((bar, i) => {
    if (bar.start_secs <= secs + 0.01) index = i;
  });
  return index;
}

/** The cues in rekordbox: hot cue letters, and memory cues as a count, since a track can have dozens. */
export function cueSummary(applied: Applied | undefined): string {
  const cues = (applied?.cues ?? []).filter((c) => c.existing);
  const hot = cues
    .filter((c) => c.hot !== null)
    .map(slotName)
    .join("");
  const memory = cues.length - hot.length;
  return [hot, memory ? `${memory}M` : ""].filter(Boolean).join(" ");
}

/** BPM as a whole number, or with one decimal when it has a fraction. */
export function bpmText(bpm: number): string {
  if (!bpm) return "";
  return Math.abs(bpm - Math.round(bpm)) < 0.005 ? String(Math.round(bpm)) : bpm.toFixed(1);
}

/** Each pitch class's Camelot number, minor (A) and major (B). */
const CAMELOT_MINOR: Record<string, number> = { Ab: 1, Eb: 2, Bb: 3, F: 4, C: 5, G: 6, D: 7, A: 8, E: 9, B: 10, "F#": 11, Db: 12 };
const CAMELOT_MAJOR: Record<string, number> = { B: 1, "F#": 2, Db: 3, Ab: 4, Eb: 5, Bb: 6, F: 7, C: 8, G: 9, D: 10, A: 11, E: 12 };
/** Sharps and flats spelled the way the tables above do. */
const SPELLING: Record<string, string> = {
  "C#": "Db",
  "D#": "Eb",
  Gb: "F#",
  "G#": "Ab",
  "A#": "Bb",
  Cb: "B",
  "E#": "F",
  Fb: "E",
  "B#": "C",
};

/** A key in Camelot ("11A"), from rekordbox's names ("Dbm", "F#", "A minor") or Camelot itself. */
export function camelot(key: string | null | undefined): string {
  const text = (key ?? "").trim();
  if (!text) return "";
  const already = /^(1[0-2]|[1-9])\s*([AB])$/i.exec(text);
  if (already) return `${already[1]}${already[2]!.toUpperCase()}`;
  const named = /^([A-G])\s*([#♯b♭]?)\s*(m|min|minor|maj|major)?$/i.exec(text);
  if (!named) return text;
  const accidental = named[2] === "♯" ? "#" : named[2] === "♭" ? "b" : (named[2] ?? "");
  const note = named[1]!.toUpperCase() + accidental;
  const minor = /^m(in(or)?)?$/i.test(named[3] ?? "") && named[3] !== "M";
  const number = (minor ? CAMELOT_MINOR : CAMELOT_MAJOR)[SPELLING[note] ?? note];
  return number ? `${number}${minor ? "A" : "B"}` : text;
}

/** How strongly a column's leading band shows over the others. At 3 every column came out as its leading band
 * alone, so bass-led music drew solid red; at 1.5 the bands mix, as an RGB waveform's do: kicks red to orange, hats
 * blue, a full-band hit pale, and the bass still leads where it should. */
export const SATURATION = 1.5;

/** A waveform column's colour: its lows, mids and highs as red, green and blue, scaled to the brightest,
 * with the weaker bands pushed down so the balance shows. */
export function bandColour(low: number, mid: number, high: number, saturation = SATURATION): [number, number, number] {
  const top = Math.max(low, mid, high, 1);
  const channel = (v: number) => Math.round((v / top) ** saturation * 255);
  return [channel(low), channel(mid), channel(high)];
}

/** The rules' default colour for each hot cue, A to H, for a cue whose own colour isn't known. */
export const SLOT_COLOURS: [number, number, number][] = [
  [0, 255, 0],
  [255, 255, 0],
  [255, 0, 0],
  [0, 0, 255],
  [255, 140, 0],
  [0, 255, 255],
  [255, 0, 138],
  [153, 0, 255],
];

/** A cue's colour, or its slot's default. */
export function cueColour(cue: Cue): [number, number, number] | null {
  return cue.color ?? (cue.hot !== null ? (SLOT_COLOURS[cue.hot] ?? null) : null);
}

/** Black or white, whichever reads on this colour. */
export function onColour([r, g, b]: [number, number, number]): string {
  return 0.3 * r + 0.59 * g + 0.11 * b > 120 ? "#000" : "#fff";
}

/** The first of a genre field that holds several, such as "Electro;Techno/House". */
export function firstGenre(genre: string): string {
  return genre.split(/[;,/|]/)[0]?.trim() ?? "";
}
