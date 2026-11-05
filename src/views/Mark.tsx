import { useEffect, useRef } from "react";

import type { Store } from "../app/store";

/** The mark's size, in CSS pixels, unless asked for another. */
const SIZE = 20;
/** The figure's radius as a share of the box: the app icon's figure is 0.27 of its canvas inside a squircle that
 * fills about 0.8 of it, so 0.27 / 0.8 of the box keeps the icon's proportions with a little margin. */
const RADIUS = 0.34;
/** The stroke, in CSS pixels whatever the size: a hairline, as toolbar symbols are. */
const STROKE = 1.25;
/** Points along the figure: enough that its curves stay smooth at 2x. */
const STEPS = 240;
/** The locked shape: the app icon's figure. */
const LOCKED = Math.PI / 2;
/** How fast the idle figure turns, in radians a second. */
const DRIFT = 0.22;
/** How quickly the shown phase follows where it should be, a second. */
const FOLLOW = 4;
/** How long a beat's pulse takes to fade, in seconds. */
const PULSE_FADE = 0.12;

/** How the mark moves: turning by itself, locked to the beat, easing in with progress, or held. */
export type MarkState = { kind: "idle" } | { kind: "playing" } | { kind: "working"; progress: number } | { kind: "waiting" };

/** The figure x = sin(3t + phase), y = sin(2t): at phase π/2 it's the app icon's. */
export function lissajous(phase: number, steps = STEPS): [number, number][] {
  const points: [number, number][] = [];
  for (let i = 0; i <= steps; i++) {
    const t = (i / steps) * Math.PI * 2;
    points.push([Math.sin(3 * t + phase), Math.sin(2 * t)]);
  }
  return points;
}

/** How strong the beat is now, 0 to 1: 1 on a beat, fading until the next. A downbeat is the strongest. */
export function beatPulse(secs: number, beats: { secs: number; number: number }[], bpm: number): number {
  if (beats.length > 0) {
    let low = 0;
    let high = beats.length - 1;
    if (secs < beats[0]!.secs) return 0;
    while (low < high) {
      const mid = (low + high + 1) >> 1;
      if (beats[mid]!.secs <= secs) low = mid;
      else high = mid - 1;
    }
    const beat = beats[low]!;
    return Math.exp(-(secs - beat.secs) / PULSE_FADE) * (beat.number === 1 ? 1 : 0.6);
  }
  if (bpm <= 0) return 0;
  const period = 60 / bpm;
  return Math.exp(-(secs % period) / PULSE_FADE) * 0.6;
}

const ease = (x: number) => (x < 0.5 ? 2 * x * x : 1 - (-2 * x + 2) ** 2 / 2);

/** The shortest way round from one phase to another. */
const turn = (from: number, to: number) => {
  const d = (((to - from) % (Math.PI * 2)) + Math.PI * 3) % (Math.PI * 2);
  return d - Math.PI;
};

/** The figure as an SVG path in a `size`-pixel square, a little larger on the beat. Equal radii on both axes, as
 * the icon's: never scaled more one way than the other. */
export function figurePath(size: number, phase: number, pulse = 0): string {
  const radius = size * RADIUS * (1 + 0.06 * pulse);
  const centre = size / 2;
  const points = lissajous(phase).map(([x, y]) => `${(centre + x * radius).toFixed(2)} ${(centre - y * radius).toFixed(2)}`);
  return `M${points.join("L")}Z`;
}

function stateOf(store: Store): MarkState {
  const activity = store.activity;
  if (activity?.waiting) return { kind: "waiting" };
  if (store.player.playing) return { kind: "playing" };
  if (activity) return { kind: "working", progress: activity.progress ?? 0 };
  return { kind: "idle" };
}

/** The header's mark: the app icon's Lissajous figure, alive. It turns slowly when idle, locks and pulses
 * on the beat when a track plays, eases into its locked shape as work goes on, and goes grey and still
 * while kfbox waits for rekordbox. With Reduce Motion it stays still. */
export function Mark({ store, size = SIZE }: { store: Store; size?: number }) {
  const path = useRef<SVGPathElement>(null);
  const state = useRef<MarkState>(stateOf(store));
  state.current = stateOf(store);
  const kind = state.current.kind;
  // Kept across state changes, so the figure never jumps.
  const motion = useRef({ drift: LOCKED, phase: LOCKED });

  useEffect(() => {
    const element = path.current;
    if (!element) return;
    const still = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (still) {
      element.setAttribute("d", figurePath(size, LOCKED));
      return;
    }
    const { player } = store;
    const m = motion.current;
    let last = performance.now();
    let frame = requestAnimationFrame(function step(now) {
      const dt = Math.min((now - last) / 1000, 0.1);
      last = now;
      const current = state.current;
      let target = m.phase;
      let pulse = 0;
      if (current.kind === "idle") {
        m.drift += DRIFT * dt;
        target = m.drift;
      } else if (current.kind === "playing") {
        target = LOCKED;
        m.drift = m.phase;
        pulse = beatPulse(player.position(), player.track?.beats ?? [], player.entry?.row.bpm ?? 0);
      } else if (current.kind === "working") {
        m.drift += DRIFT * dt * (1 - current.progress);
        target = m.drift + turn(m.drift, LOCKED) * ease(Math.min(1, current.progress));
      }
      if (current.kind !== "waiting") m.phase += turn(m.phase, target) * Math.min(1, FOLLOW * dt);
      element.setAttribute("d", figurePath(size, m.phase, pulse));
      element.style.strokeWidth = `${STROKE + 0.5 * pulse}px`;
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [store, kind, size]);

  return (
    <svg
      className={`mark ${kind}`}
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      preserveAspectRatio="xMidYMid meet"
      aria-hidden="true"
    >
      <path
        ref={path}
        d={figurePath(size, LOCKED)}
        fill="none"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
