import { Ellipsis, Minus, Pause, Play, Plus, Repeat, SkipBack, SkipForward, TriangleAlert, X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MouseEvent, type PointerEvent } from "react";

import type { Entry, Store } from "../app/store";
import { drawStrip, drawWave, type View } from "../canvas/waveform";
import type { Cue, Track } from "../ipc/types";
import { bpmText, camelot, clock, cueColour, duration, firstGenre, onColour, slotName } from "../lib/format";
import { CueName, GridControls, Rating, Tags, useDismiss } from "./DeckParts";

const OVERVIEW_HEIGHT = 26;
const WAVE_HEIGHT = 120;
/** How far down the waveform a press picks up a cue's flag rather than scrubbing. */
const FLAG_BAND = 20;
/** How far a press moves before it's a drag rather than a click, in pixels. */
const DRAG_SLOP = 3;
const TRANSPORT = { size: 15, strokeWidth: 1.75, "aria-hidden": true } as const;

/** Keeps a click from focusing a transport button, so Space stays with the player. */
const noFocus = (event: MouseEvent) => event.preventDefault();

/** Redraws on resize and on a change of theme. */
function useRedraw(draw: () => void, deps: unknown[]) {
  useEffect(() => {
    draw();
    const scheme = window.matchMedia?.("(prefers-color-scheme: dark)");
    window.addEventListener("resize", draw);
    scheme?.addEventListener("change", draw);
    return () => {
      window.removeEventListener("resize", draw);
      scheme?.removeEventListener("change", draw);
    };
    // The caller lists what the drawing depends on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

/** Runs `step` every frame while mounted. */
function useFrames(step: () => void, deps: unknown[]) {
  useEffect(() => {
    let frame = requestAnimationFrame(function tick() {
      step();
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

/** Where the deck is looking: the playhead when this track is loaded, otherwise a place of its own. */
interface Focus {
  here: boolean;
  centre: () => number;
  setAnchor: (secs: number) => void;
}

/** The whole track in RGB, with cue ticks, the played part bright, and a box for the zoomed window. Click or drag to seek. */
function Overview({ store, track, cues, focus, span }: { store: Store; track: Track; cues: Cue[]; focus: Focus; span: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const dim = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  useRedraw(() => {
    if (canvas.current) drawStrip(canvas.current, OVERVIEW_HEIGHT, track, cues);
  }, [track, cues]);

  useFrames(() => {
    const centre = focus.centre();
    const at = (secs: number) => `${(Math.max(0, Math.min(track.duration_secs, secs)) / track.duration_secs) * 100}%`;
    if (dim.current) dim.current.style.left = focus.here ? at(centre) : "0%";
    if (head.current) {
      head.current.style.display = focus.here ? "block" : "none";
      head.current.style.left = at(centre);
    }
    if (box.current) {
      box.current.style.left = at(centre - span / 2);
      box.current.style.width = `${(Math.min(span, track.duration_secs) / track.duration_secs) * 100}%`;
    }
  }, [track, focus, span]);

  const seek = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const secs = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)) * track.duration_secs;
    focus.setAnchor(secs);
    store.seekInView(secs);
  };

  return (
    <div
      className="overview"
      role="slider"
      aria-label="Position"
      aria-valuemin={0}
      aria-valuemax={Math.round(track.duration_secs)}
      aria-valuenow={0}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture?.(event.pointerId);
        dragging.current = true;
        seek(event);
      }}
      onPointerMove={(event) => dragging.current && seek(event)}
      onPointerUp={() => (dragging.current = false)}
      onPointerCancel={() => (dragging.current = false)}
    >
      <canvas ref={canvas} />
      <div className="dim" ref={dim} />
      <div className="window" ref={box} />
      <div className="strip-head" ref={head} />
    </div>
  );
}

/** A window of the track around a centred playhead that scrolls as it plays, like a CDJ's: beats, bars and cues.
 * Drag it to scrub, click to jump there, drag a cue's flag to move the cue. */
function Zoomed({
  store,
  track,
  cues,
  landmarks,
  focus,
  span,
}: {
  store: Store;
  track: Track;
  cues: Cue[];
  landmarks: number;
  focus: Focus;
  span: number;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [moving, setMoving] = useState<{ cue: Cue; secs: number } | null>(null);
  const drag = useRef<{ x: number; centre: number; moved: boolean; scrubbing: boolean } | null>(null);
  const view = useRef<View>({ start: 0, end: span });
  const drawn = useRef({ centre: Number.NaN, version: -1 });
  const version = useRef(0);
  const { player, detailWaveform, selectedCue } = store;
  const shown = moving ? cues.map((c) => (c === moving.cue ? { ...c, start_secs: moving.secs } : c)) : cues;
  const state = useRef({ shown, detailWaveform, selectedCue });
  // After each render, the drawing loop picks up what changed.
  useLayoutEffect(() => {
    state.current = { shown, detailWaveform, selectedCue };
    version.current += 1;
  });

  const draw = useCallback(
    (force: boolean) => {
      const element = canvas.current;
      if (!element) return;
      const centre = focus.centre();
      if (!force && centre === drawn.current.centre && drawn.current.version === version.current) return;
      drawn.current = { centre, version: version.current };
      view.current = { start: centre - span / 2, end: centre + span / 2 };
      const { shown: cuesNow, detailWaveform: closer, selectedCue: picked } = state.current;
      drawWave(element, WAVE_HEIGHT, track, cuesNow, landmarks, view.current, closer, picked);
    },
    [focus, span, track, landmarks],
  );
  useRedraw(() => draw(true), [draw]);
  useFrames(() => draw(false), [draw]);

  const secsAt = (event: PointerEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const { start, end } = view.current;
    return start + ((event.clientX - rect.left) / rect.width) * (end - start);
  };

  /** The saved cue whose flag is under the pointer, along the top of the waveform. */
  const flagAt = (event: PointerEvent<HTMLDivElement>): Cue | undefined => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (event.clientY - rect.top > FLAG_BAND) return undefined;
    const x = event.clientX - rect.left;
    const { start, end } = view.current;
    const px = (secs: number) => ((secs - start) / (end - start)) * rect.width;
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
    drag.current = { x: event.clientX, centre: focus.centre(), moved: false, scrubbing: false };
  };
  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    if (moving) {
      setMoving({ cue: moving.cue, secs: store.nearestBeat(secsAt(event)) });
      return;
    }
    const d = drag.current;
    if (!d) return;
    const dx = event.clientX - d.x;
    if (!d.moved && Math.abs(dx) < DRAG_SLOP) return;
    d.moved = true;
    const width = event.currentTarget.getBoundingClientRect().width || 1;
    const secs = Math.max(0, Math.min(track.duration_secs, d.centre - (dx / width) * span));
    if (focus.here) {
      if (!d.scrubbing) {
        d.scrubbing = true;
        void player.scrubBegin();
      }
      void player.scrubTo(secs);
    } else {
      focus.setAnchor(secs);
    }
  };
  const onUp = (event: PointerEvent<HTMLDivElement>) => {
    if (moving) {
      if (Math.abs(moving.secs - moving.cue.start_secs) > 0.001) void store.moveCue(moving.cue, moving.secs);
      setMoving(null);
      return;
    }
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.scrubbing) void player.scrubEnd();
    else if (!d.moved) {
      const secs = Math.max(0, Math.min(track.duration_secs, secsAt(event)));
      focus.setAnchor(secs);
      store.seekInView(secs);
    }
  };

  return (
    <div className="deck-wave" onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
      <canvas ref={canvas} aria-label="Waveform with cues" />
      <div className={focus.here ? "playhead" : "playhead idle"} />
      <div className="zoom" role="group" aria-label="Zoom" onPointerDown={(e) => e.stopPropagation()}>
        <button aria-label="Zoom out" disabled={store.zoomBars >= 64} onMouseDown={noFocus} onClick={() => store.zoomBy(0.5)}>
          <Minus size={12} strokeWidth={2} aria-hidden="true" />
        </button>
        <span>{store.zoomBars} bars</span>
        <button aria-label="Zoom in" disabled={store.zoomBars <= 2} onMouseDown={noFocus} onClick={() => store.zoomBy(2)}>
          <Plus size={12} strokeWidth={2} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}

/** Hot cues A to D (and any later ones the track has) as pads with their name and bar, then Loop and the time. */
function CueRow({ store, entry, track, cues, here }: { store: Store; entry: Entry; track: Track; cues: Cue[]; here: boolean }) {
  const elapsed = useRef<HTMLSpanElement>(null);
  const { player } = store;
  useFrames(() => {
    if (elapsed.current) elapsed.current.textContent = clock(here ? player.position() : 0);
  }, [player, here]);
  const hot = cues.filter((c) => c.hot !== null);
  const slots = [...new Set([0, 1, 2, 3, ...hot.map((c) => c.hot as number)])].sort((a, b) => a - b);
  const memory = cues.length - hot.length;
  return (
    <div className="deck-cues">
      <ul className="cue-line" aria-label="Cues">
        {slots.map((slot) => {
          const cue = hot.find((c) => c.hot === slot);
          const letter = String.fromCharCode(65 + slot);
          const colour = cue ? cueColour(cue) : null;
          return (
            <li
              key={slot}
              className={cue ? "cue-pad" : "cue-pad empty"}
              aria-selected={!!cue && cue.id !== null && cue.id === store.selectedCue}
              title={cue ? undefined : `Empty: Shift-${letter} sets it at the playhead`}
              onClick={() => {
                if (cue) {
                  store.selectCue(cue.id);
                  store.seekInView(cue.start_secs);
                } else if (here) void store.setCueAtPlayhead(letter);
              }}
            >
              <span className="slot" style={colour ? { background: `rgb(${colour.join(",")})`, color: onColour(colour) } : undefined}>
                {letter}
              </span>
              {cue ? <CueName store={store} cue={cue} track={track} /> : <span className="cue-name faint">—</span>}
              {cue?.id && (
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
      </ul>
      {memory > 0 && <span className="memory faint">+{memory} memory</span>}
      <span className="deck-end">
        <button
          className="loop"
          aria-label="Loop"
          title="Loop (L)"
          aria-pressed={here && player.looping}
          disabled={!here}
          onMouseDown={noFocus}
          onClick={() => void player.toggleLoop()}
        >
          <Repeat size={14} strokeWidth={1.75} aria-hidden="true" />
        </button>
        <span className="time">
          <span ref={elapsed}>0:00.0</span> / {duration(track.duration_secs || entry.row.duration_secs)}
        </span>
      </span>
    </div>
  );
}

/** The ⋯ menu: Apply rules, the grid, and the tags when the deck is too narrow to show them. */
function MoreMenu({
  store,
  entry,
  track,
  adds,
  editing,
}: {
  store: Store;
  entry: Entry;
  track: Track | null;
  adds: number;
  editing: boolean;
}) {
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const box = useDismiss(open, close);
  return (
    <div className="popover-anchor" ref={box}>
      <button className="icon-button" aria-label="More" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Ellipsis size={16} strokeWidth={1.75} aria-hidden="true" />
      </button>
      {open && (
        <div className="popover more" role="menu" aria-label="Track">
          {adds > 0 && entry.id && (
            <button
              role="menuitem"
              disabled={store.saving}
              title={`Fills ${adds} empty ${adds === 1 ? "slot" : "slots"}`}
              onClick={() => {
                close();
                void store.applyRulesToSelected();
              }}
            >
              Apply rules
              <kbd>⌘R</kbd>
            </button>
          )}
          {editing && track && <GridControls store={store} editable={track.grid === "rekordbox"} source={track.grid} />}
          {editing && (
            <div className="menu-section more-tags">
              <h4>Tags</h4>
              <Tags store={store} />
            </div>
          )}
          {!editing && adds === 0 && <p className="popover-note">Nothing to change on this track.</p>}
        </div>
      )}
    </div>
  );
}

/** The deck: the selected track's transport, title, rating and tags; its overview; a scrolling waveform around the
 * playhead; and its cues. T folds it down to the top row and the overview. */
export function Deck({ store }: { store: Store }) {
  const entry = store.current;
  const { player } = store;
  const here = !!entry && player.key === entry.key;
  // Where the deck looks when its track isn't loaded: kept per track, so a newly selected one starts at its start.
  const key = entry?.key ?? "";
  const [held, setHeld] = useState({ key, secs: 0 });
  const anchor = held.key === key ? held.secs : 0;
  const focus = useRef<Focus>({ here, centre: () => 0, setAnchor: () => undefined });
  useLayoutEffect(() => {
    focus.current = { here, centre: () => (here ? player.position() : anchor), setAnchor: (secs) => setHeld({ key, secs }) };
  });
  // A stable object the drawing loops read, so a re-render doesn't restart them.
  const [stableFocus] = useState<Focus>(() => ({
    get here() {
      return focus.current.here;
    },
    centre: () => focus.current.centre(),
    setAnchor: (secs: number) => focus.current.setAnchor(secs),
  }));

  if (!entry) {
    return (
      <section className="deck" aria-label="Deck">
        <p className="waiting">Nothing selected</p>
      </section>
    );
  }
  const track = store.detail && store.detail.path === entry.path ? store.detail : null;
  const cues = (entry.applied?.cues ?? []).filter((c) => c.existing).sort((a, b) => a.start_secs - b.start_secs);
  const adds = (entry.applied?.cues.length ?? 0) - cues.length;
  const facts = [
    entry.row.artist,
    entry.row.bpm ? `${bpmText(entry.row.bpm)} BPM` : "",
    camelot(entry.analysis?.camelot ?? entry.row.key),
    firstGenre(entry.row.genre),
  ].filter(Boolean);
  const editing = !!entry.id && store.libraryOpen;
  const barSecs = 240 / (track?.bpm || entry.row.bpm || 120);
  const span = store.zoomBars * barSecs;
  const open = store.trackOpen;
  const playing = here && player.playing;

  return (
    <section className={open ? "deck" : "deck folded"} aria-label={open ? "Track" : "Deck"}>
      <header className="deck-head">
        <div className="transport" role="group" aria-label="Player">
          <button aria-label="Back to start" disabled={!here} onMouseDown={noFocus} onClick={() => void player.restart()}>
            <SkipBack {...TRANSPORT} fill="currentColor" />
          </button>
          <button
            className="play"
            aria-label={playing ? "Pause" : "Play"}
            disabled={!entry.path}
            onMouseDown={noFocus}
            onClick={() => store.playPauseSelected()}
          >
            {playing ? <Pause {...TRANSPORT} fill="currentColor" /> : <Play {...TRANSPORT} fill="currentColor" />}
          </button>
          <button aria-label="Next in queue" disabled={player.queue.length === 0} onMouseDown={noFocus} onClick={() => void player.next()}>
            <SkipForward {...TRANSPORT} fill="currentColor" />
          </button>
        </div>
        <div className="deck-title">
          <h2 title={entry.row.title}>{entry.row.title}</h2>
          <span className="facts" title={facts.join(" · ")}>
            {player.error && here ? <span className="error">{player.error}</span> : facts.join(" · ")}
          </span>
        </div>
        {editing && <Rating store={store} rating={entry.row.rating} />}
        {entry.applied?.flags.map((flag) => (
          <span key={flag.kind} className="flag" title={flag.why}>
            <TriangleAlert size={12} strokeWidth={2} aria-hidden="true" />
            Early drop
          </span>
        ))}
        <div className="deck-tags">{editing && <Tags store={store} />}</div>
        {player.queue.length > 0 && (
          <span className="queue faint" title={player.queue.map((q) => q.row.title).join(", ")}>
            +{player.queue.length} next
          </span>
        )}
        <MoreMenu store={store} entry={entry} track={track} adds={adds} editing={editing} />
      </header>
      {track ? (
        <>
          <Overview store={store} track={track} cues={cues} focus={stableFocus} span={span} />
          {open && (
            <>
              <Zoomed
                store={store}
                track={track}
                cues={cues}
                landmarks={entry.applied?.landmarks.start ?? 0}
                focus={stableFocus}
                span={span}
              />
              <CueRow store={store} entry={entry} track={track} cues={cues} here={here} />
            </>
          )}
        </>
      ) : (
        <p className="waiting">{entry.error ?? (open ? "Analysing…" : "")}</p>
      )}
    </section>
  );
}
