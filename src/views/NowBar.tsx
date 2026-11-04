import { useEffect, useRef, type MouseEvent } from "react";

import type { Store } from "../app/store";
import { drawStrip } from "../canvas/waveform";
import { clock, duration, slotName } from "../lib/format";

const STRIP_HEIGHT = 30;

/** Keeps a click from focusing a transport button, so Space stays with the player. */
const noFocus = (event: MouseEvent) => event.preventDefault();

function Icon({ path }: { path: string }) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <path d={path} fill="currentColor" />
    </svg>
  );
}

const PLAY = "M4 2.5v11l9-5.5z";
const PAUSE = "M4 2.5h3v11H4zM9 2.5h3v11H9z";
const START = "M3 3h2v10H3zM13 3v10L6 8z";
const NEXT = "M11 3h2v10h-2zM3 3v10l7-5z";

/** The player, always along the bottom: transport, the track, and its cues. */
export function NowBar({ store }: { store: Store }) {
  const { player } = store;
  const strip = useRef<HTMLCanvasElement>(null);
  const progress = useRef<HTMLDivElement>(null);
  const elapsed = useRef<HTMLSpanElement>(null);
  const entry = player.entry ?? store.current;
  const loaded = player.key !== null;
  const { track } = player;

  useEffect(() => {
    const draw = () => {
      if (strip.current && track) drawStrip(strip.current, STRIP_HEIGHT, track);
    };
    draw();
    window.addEventListener("resize", draw);
    return () => window.removeEventListener("resize", draw);
  }, [track]);

  // Runs every frame without a re-render.
  useEffect(() => {
    let frame = requestAnimationFrame(function step() {
      const secs = player.position();
      if (progress.current) progress.current.style.width = `${player.durationSecs ? (secs / player.durationSecs) * 100 : 0}%`;
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
          <Icon path={START} />
        </button>
        <button
          className="play"
          aria-label={player.playing ? "Pause" : "Play"}
          disabled={!entry?.path}
          onMouseDown={noFocus}
          onClick={() => store.togglePlay()}
        >
          <Icon path={player.playing ? PAUSE : PLAY} />
        </button>
        <button aria-label="Next in queue" disabled={player.queue.length === 0} onMouseDown={noFocus} onClick={() => void player.next()}>
          <Icon path={NEXT} />
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
          <div className="progress" ref={progress} />
        </div>
        <span className="time">{duration(player.durationSecs || (entry?.row.duration_secs ?? 0))}</span>
      </div>
      <div className="now-cues" aria-label="Hot cues">
        {player.hotCues.map((cue) => {
          const letter = slotName(cue);
          return (
            <button
              key={letter}
              className="cue-pad"
              title={cue.name || `Cue ${letter}`}
              style={
                cue.color
                  ? {
                      background: `rgb(${cue.color.join(",")})`,
                      color: 0.3 * cue.color[0] + 0.59 * cue.color[1] + 0.11 * cue.color[2] > 120 ? "#000" : "#fff",
                    }
                  : undefined
              }
              onMouseDown={noFocus}
              onClick={() => void player.jumpToCue(letter)}
            >
              {letter}
            </button>
          );
        })}
      </div>
      <div className="now-facts">
        <button aria-pressed={player.looping} disabled={!loaded} onMouseDown={noFocus} onClick={() => void player.toggleLoop()}>
          Loop
        </button>
        {entry && entry.row.bpm > 0 && <span>{entry.row.bpm.toFixed(2)}</span>}
        {entry && <span>{entry.analysis?.camelot ?? entry.row.key}</span>}
        {player.queue.length > 0 && <span title={player.queue.map((q) => q.row.title).join(", ")}>+{player.queue.length} next</span>}
      </div>
    </footer>
  );
}
