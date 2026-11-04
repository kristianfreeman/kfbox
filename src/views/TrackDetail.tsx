import { useEffect, useRef, useState, type PointerEvent } from "react";

import type { Store } from "../app/store";
import { drawLevels, drawWave, type View } from "../canvas/waveform";
import type { Applied, Cue, Track } from "../ipc/types";
import { barAt, clock, slotName } from "../lib/format";

const WAVE_HEIGHT = 190;
/** How far down the waveform a press picks up a cue's flag rather than seeking. */
const FLAG_BAND = 20;
const LEVELS_HEIGHT = 84;

/** Where a pointer is along an element, in the view's seconds. */
function secsAt(event: PointerEvent<HTMLElement>, view: View): number {
  const box = event.currentTarget.getBoundingClientRect();
  const fraction = Math.max(0, Math.min(1, (event.clientX - box.left) / box.width));
  return view.start + fraction * (view.end - view.start);
}

function Canvases({ store, applied, track }: { store: Store; applied: Applied | undefined; track: Track }) {
  const wave = useRef<HTMLCanvasElement>(null);
  const levels = useRef<HTMLCanvasElement>(null);
  const playhead = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState("");
  const [anchor, setAnchor] = useState(0);
  const dragging = useRef<"down" | "scrubbing" | null>(null);
  // A cue being dragged by its flag, and where it is now.
  const [moving, setMoving] = useState<{ cue: Cue; secs: number } | null>(null);
  const { player, zoom, detailWaveform, selectedCue } = store;
  const here = player.key === store.current?.key;

  const span = track.duration_secs / zoom;
  const start = Math.max(0, Math.min(anchor, track.duration_secs - span));
  const view: View = { start, end: start + span };

  useEffect(() => {
    const draw = () => {
      // A cue being dragged draws where it's going.
      const shown = moving && applied ? { ...applied, cues: applied.cues.map((c) => (c === moving.cue ? { ...c, start_secs: moving.secs } : c)) } : applied;
      if (wave.current) drawWave(wave.current, WAVE_HEIGHT, track, shown, view, zoom > 1 ? detailWaveform : null, selectedCue);
      if (levels.current) drawLevels(levels.current, LEVELS_HEIGHT, track, view);
    };
    draw();
    window.addEventListener("resize", draw);
    return () => window.removeEventListener("resize", draw);
    // `view` is made fresh each render from these.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [track, applied, view.start, view.end, detailWaveform, moving, selectedCue]);

  // The playhead moves every frame without a re-render, and a zoomed view turns
  // the page when it runs off the edge.
  useEffect(() => {
    let frame = requestAnimationFrame(function step() {
      const line = playhead.current;
      if (line) {
        const secs = player.position();
        const visible = here && secs >= view.start && secs <= view.end;
        line.style.display = visible ? "block" : "none";
        if (visible) line.style.left = `${((secs - view.start) / (view.end - view.start)) * 100}%`;
        if (here && zoom > 1 && player.playing && (secs < view.start || secs > view.end)) setAnchor(secs);
      }
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [player, here, zoom, view.start, view.end]);

  /** The saved cue whose flag is under the pointer, along the top of the waveform. */
  const flagAt = (event: PointerEvent<HTMLDivElement>): Cue | undefined => {
    const box = event.currentTarget.getBoundingClientRect();
    if (event.clientY - box.top > FLAG_BAND) return undefined;
    const x = event.clientX - box.left;
    const px = (secs: number) => ((secs - view.start) / (view.end - view.start)) * box.width;
    return applied?.cues.find((c) => c.existing && c.id !== null && x >= px(c.start_secs) - 2 && x <= px(c.start_secs) + 18);
  };

  const onDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const flag = flagAt(event);
    if (flag) {
      store.selectCue(flag.id);
      setMoving({ cue: flag, secs: flag.start_secs });
      return;
    }
    dragging.current = "down";
    store.seekInView(secsAt(event, view));
  };
  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    const secs = secsAt(event, view);
    if (moving) {
      setMoving({ cue: moving.cue, secs: store.nearestBeat(secs) });
      return;
    }
    const i = barAt(track, secs);
    const bar = track.bars[i];
    setHover(bar ? `${clock(secs)}  bar ${i + 1}  volume ${bar.volume.toFixed(2)}  bass ${bar.bass.toFixed(2)}  busy ${bar.busy.toFixed(2)}` : "");
    if (!dragging.current || !here) return;
    if (dragging.current === "down") {
      dragging.current = "scrubbing";
      void player.scrubBegin();
    }
    void player.scrubTo(secs);
  };
  const onUp = () => {
    if (moving) {
      if (Math.abs(moving.secs - moving.cue.start_secs) > 0.001) void store.moveCue(moving.cue, moving.secs);
      setMoving(null);
      return;
    }
    if (dragging.current === "scrubbing") void player.scrubEnd();
    dragging.current = null;
  };

  return (
    <>
      <div className="scope" onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
        <canvas ref={wave} aria-label="Waveform with cues" />
        <canvas ref={levels} aria-label="Volume and bass per bar" />
        <div ref={playhead} className="playhead" />
      </div>
      <p className="legend">
        <span className="swatch volume" />
        volume per bar
        <span className="swatch bass" />
        bass per bar
        <span className="flag new" />
        in rekordbox
        <span className="flag kept" />
        suggested
        {zoom > 1 && <span className="zoom">{zoom}×</span>}
        <span className="hover">{hover}</span>
      </p>
    </>
  );
}

/** A cue's name, edited in place by double-clicking it. */
function CueName({ store, cue }: { store: Store; cue: Cue }) {
  const [editing, setEditing] = useState(false);
  if (!editing || !cue.id) {
    return (
      <span title={cue.id ? "Double-click to rename" : undefined} onDoubleClick={() => cue.id && setEditing(true)}>
        {cue.name || (cue.id ? <span className="faint">unnamed</span> : "")}
      </span>
    );
  }
  const finish = (name: string | null) => {
    setEditing(false);
    if (name !== null) void store.renameCue(cue, name);
  };
  return (
    <input
      className="rename"
      aria-label={`Name of cue ${slotName(cue)}`}
      autoFocus
      defaultValue={cue.name}
      onClick={(e) => e.stopPropagation()}
      onBlur={(e) => finish(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(e.currentTarget.value);
        if (e.key === "Escape") finish(null);
      }}
    />
  );
}

export function TrackDetail({ store }: { store: Store }) {
  const entry = store.current;
  const track = store.detail;
  if (!entry || !track || track.path !== entry.path) {
    return (
      <section className="detail" aria-label="Track">
        <p className="waiting">{entry?.error ?? "Analysing…"}</p>
      </section>
    );
  }
  const cues = entry.applied?.cues ?? [];
  const suggested = cues.some((c) => !c.existing);
  return (
    <section className="detail" aria-label="Track">
      <Canvases store={store} applied={entry.applied} track={track} />
      {suggested && entry.id && (
        <p className="suggestion-bar">
          The rules suggest {cues.filter((c) => !c.existing).length} cues here.
          <button className="primary" disabled={store.saving} onClick={() => void store.acceptSelected()}>
            Accept <kbd>Y</kbd>
          </button>
          <button disabled={store.saving} onClick={() => void store.skipSelected()}>
            Skip <kbd>N</kbd>
          </button>
        </p>
      )}
      <table className="cues">
        <tbody>
          {cues.map((cue) => (
            <tr
              key={cue.id ?? `${cue.hot}-${cue.start_secs}`}
              className={cue.existing ? "saved" : "suggested"}
              aria-selected={cue.id !== null && cue.id === store.selectedCue}
              onClick={() => {
                store.selectCue(cue.id);
                store.seekInView(cue.start_secs);
              }}
            >
              <td>
                <span className="dot" style={{ background: cue.color ? `rgb(${cue.color.join(",")})` : "transparent" }} /> {slotName(cue)}
              </td>
              <td>{cue.existing ? "in rekordbox" : "suggested"}</td>
              <td>{clock(cue.start_secs)}</td>
              <td>bar {barAt(track, cue.start_secs) + 1}</td>
              <td>{cue.end_secs === null ? "" : `loop ${Math.round((cue.end_secs - cue.start_secs) / (240 / track.bpm))} bars`}</td>
              <td className="name">
                <CueName store={store} cue={cue} />
              </td>
              <td>
                {cue.id && (
                  <button
                    className="delete"
                    aria-label={`Delete cue ${slotName(cue)}`}
                    disabled={store.saving}
                    onClick={(e) => {
                      e.stopPropagation();
                      void store.deleteCue(cue);
                    }}
                  >
                    ×
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <ul className="notes">
        {entry.applied?.notes.map((note) => (
          <li className="warn" key={note}>
            {note}
          </li>
        ))}
      </ul>
    </section>
  );
}
