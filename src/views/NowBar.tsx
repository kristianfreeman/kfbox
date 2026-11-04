import { Pause, Play, Repeat, SkipBack, SkipForward } from "lucide-react";
import { useEffect, useRef, type MouseEvent } from "react";

import type { Store } from "../app/store";
import { drawStrip } from "../canvas/waveform";
import { bpmText, camelot, clock, cueColour, duration, onColour, slotName } from "../lib/format";

const STRIP_HEIGHT = 28;
const TRANSPORT = { size: 15, strokeWidth: 1.75, "aria-hidden": true } as const;

/** Keeps a click from focusing a transport button, so Space stays with the player. */
const noFocus = (event: MouseEvent) => event.preventDefault();

/** The player, always along the bottom: transport, the track, and its cues. */
export function NowBar({ store }: { store: Store }) {
  const { player } = store;
  const strip = useRef<HTMLCanvasElement>(null);
  const dim = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLDivElement>(null);
  const elapsed = useRef<HTMLSpanElement>(null);
  const entry = player.entry ?? store.current;
  const loaded = player.key !== null;
  const { track } = player;

  useEffect(() => {
    const draw = () => {
      if (strip.current && track) drawStrip(strip.current, STRIP_HEIGHT, track, player.hotCues);
    };
    draw();
    // Redrawn at a new size, and in a new theme's colours.
    const scheme = window.matchMedia?.("(prefers-color-scheme: dark)");
    window.addEventListener("resize", draw);
    scheme?.addEventListener("change", draw);
    return () => {
      window.removeEventListener("resize", draw);
      scheme?.removeEventListener("change", draw);
    };
  }, [track, player.hotCues]);

  // Runs every frame without a re-render.
  useEffect(() => {
    let frame = requestAnimationFrame(function step() {
      const secs = player.position();
      // Full colour up to the playhead, dimmed after it.
      const at = `${player.durationSecs ? (secs / player.durationSecs) * 100 : 0}%`;
      if (dim.current) dim.current.style.left = at;
      if (head.current) head.current.style.left = at;
      if (elapsed.current) elapsed.current.textContent = clock(secs);
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [player]);

  const seekOnStrip = (event: MouseEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    void player.seek(((event.clientX - box.left) / box.width) * player.durationSecs);
  };

  return (
    <footer className="now" aria-label="Player">
      <div className="transport">
        <button aria-label="Back to start" disabled={!loaded} onMouseDown={noFocus} onClick={() => void player.restart()}>
          <SkipBack {...TRANSPORT} fill="currentColor" />
        </button>
        <button
          className="play"
          aria-label={player.playing ? "Pause" : "Play"}
          disabled={!entry?.path}
          onMouseDown={noFocus}
          onClick={() => store.togglePlay()}
        >
          {player.playing ? <Pause {...TRANSPORT} fill="currentColor" /> : <Play {...TRANSPORT} fill="currentColor" />}
        </button>
        <button aria-label="Next in queue" disabled={player.queue.length === 0} onMouseDown={noFocus} onClick={() => void player.next()}>
          <SkipForward {...TRANSPORT} fill="currentColor" />
        </button>
      </div>
      <div className="now-name">
        {entry ? (
          <>
            <strong>{entry.row.title}</strong>
            <span>{player.error ? <span className="error">{player.error}</span> : entry.row.artist}</span>
          </>
        ) : (
          <span>Nothing selected</span>
        )}
      </div>
      <div className="now-position">
        <span className="time" ref={elapsed} />
        <div
          className="strip"
          role="slider"
          aria-label="Position"
          aria-valuenow={0}
          onClick={seekOnStrip}
          data-empty={!loaded || undefined}
        >
          <canvas ref={strip} />
          <div className="dim" ref={dim} />
          {loaded && <div className="strip-head" ref={head} />}
        </div>
        <span className="time">{duration(player.durationSecs || (entry?.row.duration_secs ?? 0))}</span>
      </div>
      <div className="now-cues" aria-label="Hot cues">
        {player.hotCues.map((cue) => {
          const letter = slotName(cue);
          const colour = cueColour(cue);
          return (
            <button
              key={letter}
              className="cue-pad"
              title={cue.name || `Cue ${letter}`}
              style={colour ? { background: `rgb(${colour.join(",")})`, color: onColour(colour) } : undefined}
              onMouseDown={noFocus}
              onClick={() => void player.jumpToCue(letter)}
            >
              {letter}
            </button>
          );
        })}
      </div>
      <div className="now-facts">
        <button
          aria-label="Loop"
          aria-pressed={player.looping}
          disabled={!loaded}
          onMouseDown={noFocus}
          onClick={() => void player.toggleLoop()}
          title="Loop (L)"
        >
          <Repeat size={14} strokeWidth={1.75} aria-hidden="true" />
        </button>
        {entry && entry.row.bpm > 0 && <span>{bpmText(entry.row.bpm)} BPM</span>}
        {entry && <span>{camelot(entry.analysis?.camelot ?? entry.row.key)}</span>}
        {player.queue.length > 0 && <span title={player.queue.map((q) => q.row.title).join(", ")}>+{player.queue.length} next</span>}
      </div>
    </footer>
  );
}
