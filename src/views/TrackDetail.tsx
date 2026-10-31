import { useEffect, useRef, useState, type MouseEvent } from "react";

import type { Entry } from "../app/store";
import { drawLevels, drawWave } from "../canvas/waveform";
import type { Track } from "../ipc/types";
import { barAt, clock, slotName } from "../lib/format";

const WAVE_HEIGHT = 190;
const LEVELS_HEIGHT = 84;

function Canvases({ entry, track }: { entry: Entry; track: Track }) {
  const wave = useRef<HTMLCanvasElement>(null);
  const levels = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState("");

  useEffect(() => {
    const draw = () => {
      if (wave.current) drawWave(wave.current, WAVE_HEIGHT, track, entry.applied);
      if (levels.current) drawLevels(levels.current, LEVELS_HEIGHT, track);
    };
    draw();
    window.addEventListener("resize", draw);
    return () => window.removeEventListener("resize", draw);
  }, [track, entry.applied]);

  const onMove = (event: MouseEvent<HTMLCanvasElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const secs = ((event.clientX - box.left) / box.width) * track.duration_secs;
    const i = barAt(track, secs);
    const bar = track.bars[i];
    setHover(
      bar ? `${clock(secs)}  bar ${i + 1}  volume ${bar.volume.toFixed(2)}  bass ${bar.bass.toFixed(2)}  busy ${bar.busy.toFixed(2)}` : "",
    );
  };

  return (
    <>
      <div className="scope">
        <canvas ref={wave} onMouseMove={onMove} aria-label="Waveform with cues" />
        <canvas ref={levels} onMouseMove={onMove} aria-label="Volume and bass per bar" />
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
        <span className="hover">{hover}</span>
      </p>
    </>
  );
}

export function TrackDetail({ entry }: { entry: Entry | undefined }) {
  const track = entry?.track;
  if (!entry || !track) return <section className="detail" aria-label="Track" />;
  const cues = entry.applied?.cues ?? [];
  return (
    <section className="detail" aria-label="Track">
      <Canvases entry={entry} track={track} />
      <table className="cues">
        <tbody>
          {cues.map((cue) => (
            <tr key={`${cue.hot}-${cue.start_secs}`} className={cue.existing ? "kept" : "new"}>
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
