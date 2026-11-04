import { useEffect, useRef, useState, type PointerEvent } from "react";

import type { Store } from "../app/store";
import { drawWave, type View } from "../canvas/waveform";
import type { Cue, Track } from "../ipc/types";
import { barAt, clock, duration, slotName } from "../lib/format";

const WAVE_HEIGHT = 150;
/** How far down the waveform a press picks up a cue's flag rather than seeking. */
const FLAG_BAND = 20;

/** Where a pointer is along an element, in the view's seconds. */
function secsAt(event: PointerEvent<HTMLElement>, view: View): number {
  const box = event.currentTarget.getBoundingClientRect();
  const fraction = Math.max(0, Math.min(1, (event.clientX - box.left) / box.width));
  return view.start + fraction * (view.end - view.start);
}

function Canvases({ store, cues, landmarks, track }: { store: Store; cues: Cue[]; landmarks: number; track: Track }) {
  const wave = useRef<HTMLCanvasElement>(null);
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
      const shown = moving ? cues.map((c) => (c === moving.cue ? { ...c, start_secs: moving.secs } : c)) : cues;
      if (wave.current) drawWave(wave.current, WAVE_HEIGHT, track, shown, landmarks, view, zoom > 1 ? detailWaveform : null, selectedCue);
    };
    draw();
    window.addEventListener("resize", draw);
    return () => window.removeEventListener("resize", draw);
    // `view` is made fresh each render from these.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [track, cues, landmarks, view.start, view.end, detailWaveform, moving, selectedCue]);

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
    return cues.find((c) => c.id !== null && x >= px(c.start_secs) - 2 && x <= px(c.start_secs) + 18);
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
    setHover(
      bar ? `${clock(secs)}  bar ${i + 1}  volume ${bar.volume.toFixed(2)}  bass ${bar.bass.toFixed(2)}  busy ${bar.busy.toFixed(2)}` : "",
    );
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
    <div className="scope" onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp} title={hover}>
      <canvas ref={wave} aria-label="Waveform with cues" />
      <div ref={playhead} className="playhead" />
    </div>
  );
}

/** A cue's name, edited in place by double-clicking it. */
function CueName({ store, cue }: { store: Store; cue: Cue }) {
  const [editing, setEditing] = useState(false);
  if (!editing || !cue.id) {
    return (
      <span title={cue.id ? "Double-click to rename" : undefined} onDoubleClick={() => cue.id && setEditing(true)}>
        {cue.name || <span className="faint">unnamed</span>}
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

/** The selected track, docked under the table: its header, a wide waveform, and its cues on one line. */
export function TrackDock({ store }: { store: Store }) {
  const entry = store.current;
  const track = store.detail;
  if (!entry || !track || track.path !== entry.path) {
    return (
      <section className="dock" aria-label="Track">
        <p className="waiting">{entry ? (entry.error ?? "Analysing…") : "Nothing selected"}</p>
      </section>
    );
  }
  const cues = entry.applied?.cues ?? [];
  const saved = cues.filter((c) => c.existing).sort((a, b) => a.start_secs - b.start_secs);
  const adds = cues.length - saved.length;
  const key = entry.analysis?.camelot ?? entry.row.key;
  const facts = [entry.row.artist, entry.row.bpm ? `${entry.row.bpm.toFixed(2)} BPM` : "", key, duration(track.duration_secs)].filter(
    Boolean,
  );
  const chips = [entry.row.genre, track.grid === "rekordbox" ? "rekordbox grid" : "kfbox grid"].filter(Boolean);
  return (
    <section className="dock" aria-label="Track">
      <header className="dock-head">
        <h2>{entry.row.title}</h2>
        <span className="facts">{facts.join(" · ")}</span>
        <span className="chips">
          {chips.map((chip) => (
            <span key={chip} className="chip">
              {chip}
            </span>
          ))}
          {entry.applied?.flags.map((flag) => (
            <span key={flag.kind} className="flag" title={flag.why}>
              early drop: check {flag.slots.join(" and ")}
            </span>
          ))}
        </span>
        <span className="dock-actions">
          {adds > 0 && entry.id && (
            <button
              disabled={store.saving}
              title={`Fills ${adds} empty ${adds === 1 ? "slot" : "slots"}`}
              onClick={() => void store.applyRulesToSelected()}
            >
              Apply rules
            </button>
          )}
          <span className="zoom" aria-label="Zoom">
            <button aria-label="Zoom out" disabled={store.zoom <= 1} onClick={() => store.zoomBy(0.5)}>
              −
            </button>
            <span>{store.zoom}×</span>
            <button aria-label="Zoom in" disabled={store.zoom >= 64} onClick={() => store.zoomBy(2)}>
              +
            </button>
          </span>
        </span>
      </header>
      <Canvases store={store} cues={saved} landmarks={entry.applied?.landmarks.start ?? 0} track={track} />
      <ul className="cue-line" aria-label="Cues">
        {saved.map((cue) => (
          <li
            key={cue.id ?? `${cue.hot}-${cue.start_secs}`}
            aria-selected={cue.id !== null && cue.id === store.selectedCue}
            onClick={() => {
              store.selectCue(cue.id);
              store.seekInView(cue.start_secs);
            }}
          >
            <span
              className="slot"
              style={
                cue.color
                  ? {
                      background: `rgb(${cue.color.join(",")})`,
                      color: 0.3 * cue.color[0] + 0.59 * cue.color[1] + 0.11 * cue.color[2] > 120 ? "#000" : "#fff",
                    }
                  : undefined
              }
            >
              {slotName(cue)}
            </span>
            <CueName store={store} cue={cue} />
            <span className="bar">bar {barAt(track, cue.start_secs) + 1}</span>
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
          </li>
        ))}
        {saved.length === 0 && <li className="faint">No cues yet.</li>}
      </ul>
    </section>
  );
}
