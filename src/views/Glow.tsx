import { useEffect, useMemo, useRef, type CSSProperties } from "react";

import type { Beat, Track } from "../ipc/types";

/** Every number the activity view's glow is built from, in one place to tune. Lengths are in points. */
export const GLOW = {
  /** The pill while a track is loaded: a little warmer, its words white with a faint shadow over the brightest lava. */
  pill: { background: "#140f0d", border: "#3e3430", ink: "#ffffff", detail: "#d0c4bc", shadow: "0 1px 2px rgb(0 0 0 / 55%)" },
  lava: {
    /** Each blob's size, larger than the pill so they fill it. */
    width: 120,
    height: 80,
    /** Colour, where it starts across the pill (0 to 1), and how long one drift takes, in milliseconds. */
    blobs: [
      { colour: "rgba(255, 110, 30, 0.9)", at: 0.05, ms: 6000 },
      { colour: "rgba(255, 180, 70, 0.8)", at: 0.28, ms: 7300 },
      { colour: "rgba(240, 60, 50, 0.85)", at: 0.72, ms: 8200 },
      { colour: "rgba(255, 140, 200, 0.55)", at: 0.95, ms: 9000 },
    ],
    /** How far a blob travels across, and up and down, at most. */
    travel: 100,
    rise: 14,
    /** A blob's scale at either end of its drift. */
    scale: [0.8, 1.25],
  },
  energy: {
    /** The lava's opacity and overall scale from a break (0) to a drop (1). */
    opacity: [0.3, 0.65],
    scale: [0.9, 1.15],
    /** How long a change of energy takes to show, in milliseconds: a drop swells, a break cools. */
    ease: 1000,
    /** The stretch of track before the playhead the energy is taken over, in seconds. */
    window: 1.5,
    /** How often the energy is read while playing, in milliseconds. */
    every: 250,
  },
  /** A soft swell from below the pill, squashed on every beat; its opacity is the energy. */
  thump: { scaleY: [1.15, 0.8] },
  /** A soft dark veil behind the words, over the lava and the thump: the colour shows mostly toward the pill's
   * ends, and the words read at 4.5:1 or better over the brightest frame (Glow.test.ts checks). */
  scrim: { colour: "12, 8, 6", alpha: 0.5 },
  filament: {
    height: 2,
    /** In from each end, as a fraction of the pill. */
    inset: 0.06,
    core: "#fff0d0",
    halo: "rgba(255, 110, 30, 0.85)",
    /** Its opacity on the beat (the downbeat brighter still) and where it decays to before the next. */
    beat: 0.85,
    downbeat: 1,
    rest: 0.45,
  },
  paused: { lava: 0.18, filament: 0.3 },
} as const;

export type GlowMode = "playing" | "paused";

const lerp = ([from, to]: readonly [number, number], t: number) => from + (to - from) * t;

/** How far into its bar the playhead is, in milliseconds: from the grid's first downbeat, or the track's start. */
export function barPhase(secs: number, beats: Beat[], barMs: number): number {
  const downbeat = beats.find((b) => b.number === 1)?.secs ?? 0;
  const into = (secs - downbeat) * 1000;
  return ((into % barMs) + barMs) % barMs;
}

/** A bar of four beats as keyframes: each one jumps to `peak` (the downbeat to `downbeat`) and decays to `rest`
 * a fifth of the way to the next. `at` turns a level into a keyframe's properties. */
function beatFrames(peak: number, downbeat: number, rest: number, at: (level: number) => Keyframe): Keyframe[] {
  const frames: Keyframe[] = [];
  for (let beat = 0; beat < 4; beat++) {
    frames.push(
      { ...at(beat === 0 ? downbeat : peak), offset: beat / 4, easing: "cubic-bezier(0.2, 0.7, 0.3, 1)" },
      { ...at(rest), offset: beat / 4 + 0.2, easing: "linear" },
    );
  }
  frames.push({ ...at(rest), offset: 1 });
  return frames;
}

/** A track's loudness for the lava: the bass, with the whole signal under it, as `[low, mid, high, peak]` columns
 * go. */
const level = ([low = 0, , , peak = 0]: readonly number[]) => 0.6 * low + 0.4 * peak;

/** The level the track's loud parts reach: its 90th percentile, so the lava is judged against the track itself. */
export function loudness(track: Track): number {
  const levels = track.overview.map(level).sort((a, b) => a - b);
  return levels[Math.floor(levels.length * 0.9)] || 1;
}

/** The track's energy just before `secs`, 0 to 1: its level over the last stretch, against its own loud parts. */
export function energyAt(track: Track, secs: number, loud: number, window: number = GLOW.energy.window): number {
  const columns = track.overview;
  if (!columns.length || !track.duration_secs) return 0;
  const perSec = columns.length / track.duration_secs;
  const last = Math.min(columns.length, Math.max(1, Math.ceil(secs * perSec)));
  const first = Math.max(0, Math.min(last - 1, Math.floor((secs - window) * perSec)));
  let sum = 0;
  for (let i = first; i < last; i++) sum += level(columns[i] ?? []);
  return Math.max(0, Math.min(1, sum / (last - first) / loud));
}

/** Plays `animations` while the window is shown and pauses them while it's hidden; `shown` runs on each return. */
function whileShown(animations: Animation[], shown: () => void) {
  const update = () => {
    for (const animation of animations) {
      if (document.hidden) animation.pause();
      else animation.play();
    }
    if (!document.hidden) shown();
  };
  document.addEventListener("visibilitychange", update);
  update();
  return () => {
    document.removeEventListener("visibilitychange", update);
    for (const animation of animations) animation.cancel();
  };
}

const animates = () =>
  typeof Element !== "undefined" &&
  "animate" in Element.prototype &&
  !(globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false);

/** The glow inside the activity view while a track is loaded: lava drifting in the pill, a filament along its
 * bottom edge that flares on every beat, and a swell from below that thumps with it. Clipped to the pill, under the
 * words.
 *
 * The lava drifts at its own steady pace; the track's energy only sets how lit and how large it is, easing over a
 * second, so a drop swells it and a break cools it but nothing ever jumps. Paused, the lava freezes and dims and the
 * filament holds still. Hidden, everything stops. With Reduce Motion only a still, dim filament shows.
 *
 * It costs the page next to nothing: pre-drawn gradients moved by transform and opacity in Web Animations, which
 * the compositor runs, and the energy read four times a second while playing. */
export function Glow({
  mode,
  track,
  bpm,
  position,
  beats,
  sync,
}: {
  mode: GlowMode;
  track: Track | null;
  bpm: number;
  position: () => number;
  beats: Beat[];
  /** Changes whenever the beat should line up with the playhead again: a seek, a loop, a new track. */
  sync: unknown;
}) {
  const lava = useRef<HTMLDivElement>(null);
  const thump = useRef<HTMLDivElement>(null);
  const swell = useRef<HTMLDivElement>(null);
  const filament = useRef<HTMLDivElement>(null);
  const blobs = useRef<(HTMLDivElement | null)[]>([]);
  const latest = useRef({ position, beats, track });
  useEffect(() => {
    latest.current = { position, beats, track };
  });
  const loud = useMemo(() => (track ? loudness(track) : 1), [track]);
  const moving = animates();

  // The drift: always the same pace, out of phase. It's set up once and only paused and played after, so the lava
  // freezes where it is and picks up from there.
  const drifts = useRef<Animation[]>([]);
  const playing = mode === "playing";
  const running = useRef(playing);
  useEffect(() => {
    running.current = playing;
    for (const drift of drifts.current) {
      if (playing && !document.hidden) drift.play();
      else drift.pause();
    }
  }, [playing]);
  useEffect(() => {
    if (!moving) return;
    const { travel, rise, scale } = GLOW.lava;
    drifts.current = blobs.current.flatMap((blob, i) => {
      if (!blob) return [];
      const way = i % 2 === 0 ? 1 : -1;
      const ms = GLOW.lava.blobs[i]?.ms ?? 7000;
      const animation = blob.animate(
        [
          { transform: `translate(${(-travel / 2) * way}px, ${-rise}px) scale(${scale[0]})` },
          { transform: `translate(${(travel / 2) * way}px, ${rise}px) scale(${scale[1]})` },
        ],
        { duration: ms, iterations: Infinity, direction: "alternate", easing: "ease-in-out" },
      );
      animation.currentTime = (i * 2300) % ms;
      if (!running.current) animation.pause();
      return [animation];
    });
    const update = () => {
      for (const drift of drifts.current) {
        if (running.current && !document.hidden) drift.play();
        else drift.pause();
      }
    };
    document.addEventListener("visibilitychange", update);
    update();
    return () => {
      document.removeEventListener("visibilitychange", update);
      for (const drift of drifts.current) drift.cancel();
      drifts.current = [];
    };
  }, [moving]);

  // The energy: how lit and how large the lava is, and how hard the beat thumps.
  useEffect(() => {
    const set = (energy: number | null) => {
      const lit = energy === null ? GLOW.paused.lava : lerp(GLOW.energy.opacity, energy);
      const size = energy === null ? GLOW.energy.scale[0] : lerp(GLOW.energy.scale, energy);
      if (lava.current) {
        lava.current.style.opacity = String(lit);
        lava.current.style.transform = `scale(${size})`;
      }
      if (thump.current) thump.current.style.opacity = String(energy ?? 0);
    };
    if (mode !== "playing" || !moving) {
      set(null);
      return;
    }
    let timer = 0;
    const read = () => {
      const { track: now, position: at } = latest.current;
      set(now ? energyAt(now, at(), loud) : 0.5);
    };
    const update = () => {
      window.clearInterval(timer);
      if (document.hidden) return;
      read();
      timer = window.setInterval(read, GLOW.energy.every);
    };
    document.addEventListener("visibilitychange", update);
    update();
    return () => {
      document.removeEventListener("visibilitychange", update);
      window.clearInterval(timer);
    };
  }, [mode, moving, loud]);

  // The beat: the filament flares and the swell squashes on every one, locked to the playhead.
  useEffect(() => {
    const line = filament.current;
    const below = swell.current;
    const { beat, downbeat, rest } = GLOW.filament;
    if (line) line.style.opacity = String(mode === "playing" && moving ? rest : GLOW.paused.filament);
    if (mode !== "playing" || !moving || bpm <= 0 || !line || !below) return;
    const barMs = (4 * 60_000) / bpm;
    const [high, low] = GLOW.thump.scaleY;
    const beats = [
      line.animate(
        beatFrames(beat, downbeat, rest, (level) => ({ opacity: level })),
        { duration: barMs, iterations: Infinity },
      ),
      below.animate(
        beatFrames(1, 1, 0, (level) => ({ transform: `scaleY(${low + (high - low) * level})` })),
        { duration: barMs, iterations: Infinity },
      ),
    ];
    const align = () => {
      const phase = barPhase(latest.current.position(), latest.current.beats, barMs);
      for (const animation of beats) animation.currentTime = phase;
    };
    return whileShown(beats, align);
  }, [mode, moving, bpm, sync]);

  const ease = `${GLOW.energy.ease}ms ease`;
  const { lava: blob, filament: line } = GLOW;
  return (
    <div className="glow" aria-hidden="true">
      {moving && (
        <>
          <div className="lava" ref={lava} style={{ transition: `opacity ${ease}, transform ${ease}` }}>
            {blob.blobs.map((b, i) => (
              <div
                key={b.colour}
                ref={(element) => {
                  blobs.current[i] = element;
                }}
                className="blob"
                style={{
                  left: `calc(${b.at * 100}% - ${blob.width / 2}px)`,
                  width: blob.width,
                  height: blob.height,
                  marginTop: -blob.height / 2,
                  background: `radial-gradient(closest-side, ${b.colour}, transparent)`,
                }}
              />
            ))}
          </div>
          <div className="thump" ref={thump} style={{ transition: `opacity ${ease}` }}>
            <div className="swell" ref={swell} style={{ background: `radial-gradient(50% 100% at 50% 100%, ${line.halo}, transparent)` }} />
          </div>
          <div
            className="scrim"
            style={{
              background: `radial-gradient(closest-side, rgba(${GLOW.scrim.colour}, ${GLOW.scrim.alpha}) 0%, rgba(${GLOW.scrim.colour}, ${GLOW.scrim.alpha}) 55%, transparent 100%)`,
            }}
          />
        </>
      )}
      <div
        className="filament"
        ref={filament}
        style={
          {
            left: `${line.inset * 100}%`,
            right: `${line.inset * 100}%`,
            height: line.height,
            background: `linear-gradient(to right, transparent, ${line.halo} 18%, ${line.core} 50%, ${line.halo} 82%, transparent)`,
            boxShadow: `0 0 6px 1px ${line.halo}`,
          } as CSSProperties
        }
      />
    </div>
  );
}

type Rgb = [number, number, number];

const parse = (css: string): { rgb: Rgb; alpha: number } => {
  const n = css.match(/[\d.]+/g)?.map(Number) ?? [];
  if (css.startsWith("#")) {
    const hex = css.slice(1);
    return { rgb: [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb, alpha: 1 };
  }
  return { rgb: [n[0] ?? 0, n[1] ?? 0, n[2] ?? 0], alpha: n[3] ?? 1 };
};

const over = (under: Rgb, top: Rgb, alpha: number): Rgb => under.map((u, i) => u * (1 - alpha) + (top[i] ?? 0) * alpha) as Rgb;

/** WCAG's relative luminance. */
const luminance = (rgb: Rgb) => {
  const [r, g, b] = rgb.map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
};

/** The words' contrast over the brightest frame the glow can make behind them: every blob stacked at its peak, at
 * the most energy, the thump at full, then the scrim. Harsher than any real frame, where blobs drift apart. */
export function worstContrast(ink: string): number {
  let behind = parse(GLOW.pill.background).rgb;
  const lit = GLOW.energy.opacity[1];
  for (const blob of GLOW.lava.blobs) {
    const { rgb, alpha } = parse(blob.colour);
    behind = over(behind, rgb, alpha * lit);
  }
  const swell = parse(GLOW.filament.halo);
  behind = over(behind, swell.rgb, swell.alpha);
  behind = over(behind, parse(`rgb(${GLOW.scrim.colour})`).rgb, GLOW.scrim.alpha);
  const [a, b] = [luminance(parse(ink).rgb), luminance(behind)];
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** The pill's own colours while the glow shows, as custom properties for app.css. */
export const litPill = {
  "--pill": GLOW.pill.background,
  "--pill-line": GLOW.pill.border,
  "--pill-ink": GLOW.pill.ink,
  "--pill-detail": GLOW.pill.detail,
  "--pill-shadow": GLOW.pill.shadow,
} as CSSProperties;
