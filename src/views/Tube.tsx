import { useEffect, useRef } from "react";

import type { Beat } from "../ipc/types";

/** What the tube shows: dark, a dim steady glow, a slow breath while work runs, or a pulse on every beat. */
export type TubeMode = "off" | "dim" | "working" | "playing";

/** The glow at rest in each mode, as an opacity. */
const REST: Record<TubeMode, number> = { off: 0, dim: 0.22, working: 0.3, playing: 0.5 };
/** One breath while work runs, in milliseconds. */
const BREATH_MS = 2400;

/** One bar of beats as keyframes: each beat flares and decays before the next, the downbeat brightest. */
export function barKeyframes(rest: number): Keyframe[] {
  const frames: Keyframe[] = [];
  for (let beat = 0; beat < 4; beat++) {
    const peak = beat === 0 ? 1 : 0.8;
    const stretch = beat === 0 ? 1.06 : 1.035;
    frames.push(
      { offset: beat / 4, opacity: peak, transform: `scale(${stretch}, ${stretch + 0.04})`, easing: "cubic-bezier(0.2, 0.7, 0.3, 1)" },
      { offset: beat / 4 + 0.2, opacity: rest, transform: "scale(1, 1)", easing: "linear" },
    );
  }
  frames.push({ offset: 1, opacity: rest, transform: "scale(1, 1)" });
  return frames;
}

/** How far into its bar the playhead is, in milliseconds: from the grid's first downbeat, or the track's start. */
export function barPhase(secs: number, beats: Beat[], barMs: number): number {
  const downbeat = beats.find((b) => b.number === 1)?.secs ?? 0;
  const into = (secs - downbeat) * 1000;
  return ((into % barMs) + barMs) % barMs;
}

/** A glow behind the activity view, as if a lit tube sat behind it: a halo past its edges and a brighter core
 * inside. While a track plays it pulses on the beat, locked to the playhead; while work runs it breathes; otherwise
 * it rests, dim and still.
 *
 * It costs the page nothing per frame: two pre-drawn gradients, moved only by opacity and transform in Web
 * Animations, which run in the compositor at the display's rate. Each change of state sets up one animation;
 * nothing runs at rest, with Reduce Motion, or while the window is hidden. */
export function Tube({
  mode,
  bpm,
  position,
  beats,
  sync,
}: {
  mode: TubeMode;
  bpm: number;
  /** The playhead now, read when the pulse is set up. */
  position: () => number;
  beats: Beat[];
  /** Changes whenever the pulse should line up with the playhead again: a seek, a loop, a new track. */
  sync: unknown;
}) {
  const halo = useRef<HTMLDivElement>(null);
  const core = useRef<HTMLDivElement>(null);
  const latest = useRef({ position, beats });
  useEffect(() => {
    latest.current = { position, beats };
  });

  useEffect(() => {
    const layers = [halo.current, core.current].filter((l): l is HTMLDivElement => !!l);
    const rest = REST[mode];
    for (const layer of layers) layer.style.opacity = String(rest);
    const still = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (still || typeof Element === "undefined" || !("animate" in Element.prototype)) return;
    let animations: Animation[] = [];
    if (mode === "playing" && bpm > 0) {
      const barMs = (4 * 60_000) / bpm;
      const phase = barPhase(latest.current.position(), latest.current.beats, barMs);
      animations = layers.map((layer) => {
        const animation = layer.animate(barKeyframes(rest), { duration: barMs, iterations: Infinity });
        animation.currentTime = phase;
        return animation;
      });
    } else if (mode === "working") {
      animations = layers.map((layer) =>
        layer.animate([{ opacity: rest * 0.5 }, { opacity: rest * 1.6 }, { opacity: rest * 0.5 }], {
          duration: BREATH_MS,
          iterations: Infinity,
          easing: "ease-in-out",
        }),
      );
    }
    if (animations.length === 0) return;
    // A hidden window shows nothing, so nothing runs; shown again, the pulse finds the beat again.
    const shown = () => {
      const barMs = (4 * 60_000) / bpm;
      for (const animation of animations) {
        if (document.hidden) animation.pause();
        else {
          if (mode === "playing") animation.currentTime = barPhase(latest.current.position(), latest.current.beats, barMs);
          animation.play();
        }
      }
    };
    document.addEventListener("visibilitychange", shown);
    shown();
    return () => {
      document.removeEventListener("visibilitychange", shown);
      for (const animation of animations) animation.cancel();
    };
  }, [mode, bpm, sync]);

  return (
    <>
      <div className={`tube-halo ${mode}`} ref={halo} aria-hidden="true" />
      <div className={`tube-core ${mode}`} ref={core} aria-hidden="true" />
    </>
  );
}
