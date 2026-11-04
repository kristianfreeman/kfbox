import { Check, ChevronDown, Grid3x3, Minus, Plus, Star, TriangleAlert, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";

import type { Store } from "../app/store";
import { drawWave, type View } from "../canvas/waveform";
import type { Cue, Track } from "../ipc/types";
import { barAt, bpmText, camelot, clock, cueColour, duration, onColour, slotName } from "../lib/format";

const WAVE_HEIGHT = 140;
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
    // Redrawn at a new size, and in a new theme's colours.
    const scheme = window.matchMedia?.("(prefers-color-scheme: dark)");
    window.addEventListener("resize", draw);
    scheme?.addEventListener("change", draw);
    return () => {
      window.removeEventListener("resize", draw);
      scheme?.removeEventListener("change", draw);
    };
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

/** What a cue says in the list: its name, or else what it is, and its bar. Never "unnamed". */
export function cueLabel(cue: Cue, track: Track): string {
  const bar = barAt(track, cue.start_secs);
  const loop = cue.end_secs !== null ? `${Math.max(1, barAt(track, cue.end_secs) - bar)}-bar loop` : "";
  const what = cue.name.trim() || loop || (cue.hot === null ? "Memory cue" : "");
  const parts = [what, what === loop ? "" : loop, `bar ${bar + 1}`].filter(Boolean);
  const text = parts.join(" · ");
  // A name is shown as written; a description starts with a capital.
  return cue.name.trim() ? text : text.charAt(0).toUpperCase() + text.slice(1);
}

/** A cue's label, its name edited in place by double-clicking it. */
function CueName({ store, cue, track }: { store: Store; cue: Cue; track: Track }) {
  const [editing, setEditing] = useState(false);
  if (!editing || !cue.id) {
    return (
      <span className="cue-name" title={cue.id ? "Double-click to rename" : undefined} onDoubleClick={() => cue.id && setEditing(true)}>
        {cueLabel(cue, track)}
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

/** Closes a popover on a click elsewhere or Escape. */
function useDismiss(open: boolean, close: () => void) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      if (!box.current?.contains(event.target as Node)) close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        close();
      }
    };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", escape, true);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", escape, true);
    };
  }, [open, close]);
  return box;
}

/** Five stars to click; clicking the rating it has clears it. */
function Rating({ store, rating }: { store: Store; rating: number }) {
  return (
    <span className="rating" role="group" aria-label="Rating">
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          aria-label={`${n} ${n === 1 ? "star" : "stars"}`}
          aria-pressed={n <= rating}
          disabled={store.saving || !store.libraryOpen}
          onClick={() => store.rate(n === rating ? 0 : n)}
        >
          <Star size={13} strokeWidth={1.75} aria-hidden="true" />
        </button>
      ))}
    </span>
  );
}

/** The track's own My Tags as chips to take off, and a picker of the library's tags to add one. */
function Tags({ store }: { store: Store }) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const close = useCallback(() => {
    setOpen(false);
    setFilter("");
  }, []);
  const box = useDismiss(open, close);
  const entry = store.current;
  if (!entry) return null;
  const on = entry.row.my_tags;
  const words = filter.toLowerCase();
  return (
    <div className="tags" role="group" aria-label="My Tags">
      {on.map((tag) => {
        const [column, name] = tag.includes(": ") ? tag.split(": ", 2) : ["", tag];
        return (
          <span key={tag} className="tag-chip" title={column ? `${column}: ${name}` : name}>
            {name}
            <button aria-label={`Remove tag ${name}`} disabled={store.saving} onClick={() => store.toggleTag(tag)}>
              <X size={11} strokeWidth={2} aria-hidden="true" />
            </button>
          </span>
        );
      })}
      {store.myTags.length > 0 && (
        <div className="popover-anchor" ref={box}>
          <button className="add-tag" aria-haspopup="dialog" aria-expanded={open} onClick={() => (open ? close() : setOpen(true))}>
            <Plus size={12} strokeWidth={2} aria-hidden="true" />
            Tag
          </button>
          {open && (
            <div className="popover" role="dialog" aria-label="Add a tag">
              <input
                autoFocus
                placeholder="Find a tag"
                aria-label="Find a tag"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
              />
              <div className="popover-list">
                {store.myTags.map((column) => {
                  const tags = column.tags.filter((t) => t.name.toLowerCase().includes(words) || column.name.toLowerCase().includes(words));
                  if (tags.length === 0) return null;
                  return (
                    <div key={column.name} role="group" aria-label={column.name}>
                      <h4>{column.name}</h4>
                      {tags.map((tag) => {
                        const name = `${column.name}: ${tag.name}`;
                        const has = on.includes(name);
                        return (
                          <button key={tag.id} aria-pressed={has} disabled={store.saving} onClick={() => store.toggleTag(name)}>
                            <span className="tick">{has && <Check size={13} strokeWidth={2} aria-hidden="true" />}</span>
                            {tag.name}
                          </button>
                        );
                      })}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** A person's grid edits, in a small menu: nudge, a downbeat at the playhead, or a BPM. */
function GridMenu({ store, editable, source }: { store: Store; editable: boolean; source: Track["grid"] }) {
  const [open, setOpen] = useState(false);
  const [bpm, setBpm] = useState("");
  const close = useCallback(() => setOpen(false), []);
  const box = useDismiss(open, close);
  const entry = store.current;
  if (!entry) return null;
  return (
    <div className="popover-anchor" ref={box}>
      <button className="tool" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)} title="Beat grid">
        <Grid3x3 size={14} strokeWidth={1.75} aria-hidden="true" />
        Grid
        <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
      </button>
      {open && (
        <div className="popover menu-like" role="group" aria-label="Grid">
          <p className="popover-note">{source === "rekordbox" ? "rekordbox's grid" : "kfbox's grid, from analysis"}</p>
          {editable ? (
            <>
              <div className="nudge">
                <button
                  title="Move the grid 10 ms earlier"
                  disabled={store.saving}
                  onClick={() => store.editGrid({ kind: "shift", ms: -10 })}
                >
                  −10 ms
                </button>
                <button title="Move the grid 10 ms later" disabled={store.saving} onClick={() => store.editGrid({ kind: "shift", ms: 10 })}>
                  +10 ms
                </button>
              </div>
              <button
                className="wide"
                disabled={store.saving || store.player.key !== entry.key}
                onClick={() => store.editGrid({ kind: "downbeat_at", secs: store.player.position() })}
              >
                Downbeat at the playhead
              </button>
              <form
                className="bpm-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  const value = Number(bpm);
                  if (value > 0) store.editGrid({ kind: "bpm", bpm: value });
                  setBpm("");
                }}
              >
                <label>
                  BPM
                  <input
                    type="number"
                    step="0.01"
                    aria-label="Grid BPM"
                    placeholder={entry.row.bpm.toFixed(2)}
                    value={bpm}
                    onChange={(e) => setBpm(e.target.value)}
                  />
                </label>
              </form>
            </>
          ) : (
            <p className="popover-note">Only a grid in the library can be changed.</p>
          )}
        </div>
      )}
    </div>
  );
}

/** The selected track, docked under the table: its header, a wide waveform, and its cues and tags. */
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
  const key = camelot(entry.analysis?.camelot ?? entry.row.key);
  const facts = [
    entry.row.artist,
    entry.row.bpm ? `${bpmText(entry.row.bpm)} BPM` : "",
    key,
    duration(track.duration_secs),
    entry.row.genre,
  ].filter(Boolean);
  const editing = !!entry.id && store.libraryOpen;
  return (
    <section className="dock" aria-label="Track">
      <header className="dock-head">
        <div className="dock-title">
          <h2>{entry.row.title}</h2>
          <span className="facts">{facts.join(" · ")}</span>
        </div>
        {editing && <Rating store={store} rating={entry.row.rating} />}
        <span className="dock-actions">
          {entry.applied?.flags.map((flag) => (
            <span key={flag.kind} className="flag" title={flag.why}>
              <TriangleAlert size={12} strokeWidth={2} aria-hidden="true" />
              Early drop: check {flag.slots.join(" and ")}
            </span>
          ))}
          {adds > 0 && entry.id && (
            <button
              className="tool"
              disabled={store.saving}
              title={`Fills ${adds} empty ${adds === 1 ? "slot" : "slots"}`}
              onClick={() => void store.applyRulesToSelected()}
            >
              Apply rules
            </button>
          )}
          {editing && <GridMenu store={store} editable={track.grid === "rekordbox"} source={track.grid} />}
          <span className="zoom" role="group" aria-label="Zoom">
            <button aria-label="Zoom out" disabled={store.zoom <= 1} onClick={() => store.zoomBy(0.5)}>
              <Minus size={13} strokeWidth={2} aria-hidden="true" />
            </button>
            <span>{store.zoom}×</span>
            <button aria-label="Zoom in" disabled={store.zoom >= 64} onClick={() => store.zoomBy(2)}>
              <Plus size={13} strokeWidth={2} aria-hidden="true" />
            </button>
          </span>
        </span>
      </header>
      <Canvases store={store} cues={saved} landmarks={entry.applied?.landmarks.start ?? 0} track={track} />
      <div className="dock-foot">
        <ul className="cue-line" aria-label="Cues">
          {saved.map((cue) => {
            const colour = cueColour(cue);
            return (
              <li
                key={cue.id ?? `${cue.hot}-${cue.start_secs}`}
                aria-selected={cue.id !== null && cue.id === store.selectedCue}
                onClick={() => {
                  store.selectCue(cue.id);
                  store.seekInView(cue.start_secs);
                }}
              >
                <span className="slot" style={colour ? { background: `rgb(${colour.join(",")})`, color: onColour(colour) } : undefined}>
                  {slotName(cue)}
                </span>
                <CueName store={store} cue={cue} track={track} />
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
                    <X size={12} strokeWidth={2} aria-hidden="true" />
                  </button>
                )}
              </li>
            );
          })}
          {saved.length === 0 && <li className="faint">No cues yet.</li>}
        </ul>
        {editing && <Tags store={store} />}
      </div>
    </section>
  );
}
