import { useEffect, useRef, useState, type PointerEvent } from "react";

import type { Store } from "../app/store";
import { drawLevels, drawWave, type View } from "../canvas/waveform";
import type { Applied, Track } from "../ipc/types";
import { barAt, clock, slotName } from "../lib/format";

const WAVE_HEIGHT = 190;
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
  const { player, zoom, detailWaveform } = store;
  const here = player.key === store.current?.key;

  const span = track.duration_secs / zoom;
  const start = Math.max(0, Math.min(anchor, track.duration_secs - span));
  const view: View = { start, end: start + span };

  useEffect(() => {
    const draw = () => {
      if (wave.current) drawWave(wave.current, WAVE_HEIGHT, track, applied, view, zoom > 1 ? detailWaveform : null);
      if (levels.current) drawLevels(levels.current, LEVELS_HEIGHT, track, view);
    };
    draw();
    window.addEventListener("resize", draw);
    return () => window.removeEventListener("resize", draw);
    // `view` is made fresh each render from these.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [track, applied, view.start, view.end, detailWaveform]);

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

  const onDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    dragging.current = "down";
    store.seekInView(secsAt(event, view));
  };
  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    const secs = secsAt(event, view);
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
        new cue
        <span className="flag kept" />
        kept from rekordbox
        {zoom > 1 && <span className="zoom">{zoom}×</span>}
        <span className="hover">{hover}</span>
      </p>
    </>
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
  return (
    <section className="detail" aria-label="Track">
      <Canvases store={store} applied={entry.applied} track={track} />
      <table className="cues">
        <tbody>
          {cues.map((cue) => (
            <tr key={`${cue.hot}-${cue.start_secs}`} className={cue.existing ? "kept" : "new"} onClick={() => store.seekInView(cue.start_secs)}>
              <td>
                <span className="dot" style={{ background: cue.color ? `rgb(${cue.color.join(",")})` : "transparent" }} /> {slotName(cue)}
              </td>
              <td>{cue.existing ? "kept" : "new"}</td>
              <td>{clock(cue.start_secs)}</td>
              <td>bar {barAt(track, cue.start_secs) + 1}</td>
              <td>{cue.end_secs === null ? "" : `loop ${Math.round((cue.end_secs - cue.start_secs) / (240 / track.bpm))} bars`}</td>
              <td className="name">{cue.name}</td>
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
        {entry.applied?.kept.map((note) => (
          <li key={note}>{note}: left as it is</li>
        ))}
      </ul>
    </section>
  );
}
