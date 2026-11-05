import { useEffect, useRef, useState, type CSSProperties, type UIEvent } from "react";

import { Check, TriangleAlert } from "lucide-react";

import { COLUMN_LABELS, type ColumnId, type Entry, type SortColumn, type Store } from "../app/store";
import { bpmText, camelot, cueColour, duration, SLOT_COLOURS } from "../lib/format";

const ROW_HEIGHT = 32;
/** The sticky header row's height, part of the scroll. */
const HEAD_HEIGHT = 28;
/** Rows drawn beyond the edges, so a fast scroll doesn't show gaps. */
const OVERSCAN = 8;
/** The table's height before it has been measured, as in a test. */
const FALLBACK_HEIGHT = 800;

/** The rows to draw, first to last (exclusive), for a list scrolled to `top` in a view `height` tall. `top` may be
 * stale: when a shorter list clamps the scroll, WebKit doesn't always say so, so it's held to the list's own end
 * here and the range can never fall past the rows. */
export function visibleRows(top: number, height: number, count: number): { first: number; last: number } {
  const end = Math.max(0, count * ROW_HEIGHT + HEAD_HEIGHT - height);
  const at = Math.min(Math.max(0, top), end);
  const first = Math.max(0, Math.min(count, Math.floor(at / ROW_HEIGHT) - OVERSCAN));
  const last = Math.min(count, Math.ceil((at + height) / ROW_HEIGHT) + OVERSCAN);
  return { first, last };
}

/** Every column, in order; Title always shows, the rest as chosen. */
const COLUMNS: { id: SortColumn; label: string; width: string; column?: ColumnId }[] = [
  { id: "title", label: "Title", width: "minmax(160px, 3fr)" },
  { id: "artist", label: "Artist", width: "minmax(110px, 2fr)", column: "artist" },
  { id: "bpm", label: "BPM", width: "52px", column: "bpm" },
  { id: "key", label: "Key", width: "44px", column: "key" },
  { id: "time", label: "Time", width: "48px", column: "time" },
  { id: "genre", label: "Genre", width: "minmax(80px, 1fr)", column: "genre" },
  { id: "rating", label: "Rating", width: "68px", column: "rating" },
];
const CUES_WIDTH = "92px";

/** Hot cues A to D as pips in their colours, grey when empty, then +N for the rest. */
function Cues({ entry }: { entry: Entry }) {
  const { row } = entry;
  if (row.missing)
    return (
      <span className="error" title={`Not found: ${entry.path}`}>
        Missing
      </span>
    );
  if (entry.error)
    return (
      <span className="error" title={entry.error}>
        Failed
      </span>
    );
  const saved = (entry.applied?.cues ?? []).filter((c) => c.existing);
  const colourOf = (hot: number) => {
    const cue = saved.find((c) => c.hot === hot);
    return cue ? cueColour(cue) : SLOT_COLOURS[hot];
  };
  const letters = row.hot.split("");
  const beyond = letters.filter((l) => l > "D").length;
  const more = row.memory + beyond;
  const doubt = entry.applied?.flags[0];
  const said = [letters.length ? `Hot cues ${letters.join(" ")}` : "No hot cues", row.memory ? `${row.memory} memory` : ""]
    .filter(Boolean)
    .join(", ");
  return (
    <span className="pips" title={said} aria-label={said}>
      {["A", "B", "C", "D"].map((letter, hot) => {
        const on = letters.includes(letter);
        const colour = on ? colourOf(hot) : null;
        return (
          <span key={letter} className={on ? "pip on" : "pip"} style={colour ? { background: `rgb(${colour.join(",")})` } : undefined} />
        );
      })}
      {more > 0 && <span className="more">+{more}</span>}
      {doubt && (
        <span className="flag" title={doubt.why} aria-label="early drop?">
          <TriangleAlert size={12} strokeWidth={2} aria-hidden="true" />
        </span>
      )}
    </span>
  );
}

/** Stars, drawn small. */
function Stars({ rating }: { rating: number }) {
  if (!rating) return null;
  return (
    <span className="stars" aria-label={`${rating} stars`}>
      {"★".repeat(rating)}
      <span className="unlit">{"★".repeat(5 - rating)}</span>
    </span>
  );
}

/** The columns' menu, from right-clicking the header: tick what shows. */
function ColumnMenu({ store, at, close }: { store: Store; at: { x: number; y: number }; close: () => void }) {
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    menu.current?.querySelector("button")?.focus();
    const away = (event: Event) => {
      if (!menu.current?.contains(event.target as Node)) close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [close]);
  return (
    <div ref={menu} className="menu floating" role="menu" aria-label="Columns" style={{ left: at.x, top: at.y }}>
      {(Object.keys(COLUMN_LABELS) as ColumnId[]).map((id) => {
        const on = store.columns.includes(id);
        return (
          <button key={id} role="menuitemcheckbox" aria-checked={on} onClick={() => store.toggleColumn(id)}>
            <span className="tick">{on && <Check size={13} strokeWidth={2} aria-hidden="true" />}</span>
            {COLUMN_LABELS[id]}
          </button>
        );
      })}
    </div>
  );
}

export function TrackTable({ store }: { store: Store }) {
  const box = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState({ top: 0, height: FALLBACK_HEIGHT });
  const [chooser, setChooser] = useState<{ x: number; y: number } | null>(null);
  const { rows, selected, sort, player } = store;
  // Rating shows once any track in the list has one: an empty column is noise.
  const rated = rows.some((e) => e.row.rating > 0);
  const shown = COLUMNS.filter((c) => !c.column || (store.columns.includes(c.column) && (c.id !== "rating" || rated)));
  const cues = store.columns.includes("cues");
  const template = [...shown.map((c) => c.width), ...(cues ? [CUES_WIDTH] : [])].join(" ");

  const measure = (element: HTMLDivElement) => setScroll({ top: element.scrollTop, height: element.clientHeight || FALLBACK_HEIGHT });
  // The grid only exists once there are tracks, so the observers attach then, not on first mount. Watching the
  // rows' own height too re-reads the scroll whenever the list changes length, since a clamped scroll may not
  // send an event.
  const showing = store.entries.length > 0;
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    measure(element);
    const observer = new ResizeObserver(() => measure(element));
    observer.observe(element);
    const body = element.querySelector(".rows");
    if (body) observer.observe(body);
    return () => observer.disconnect();
  }, [showing]);

  // Keeps the selected row in view as the keyboard moves it.
  const selectedIndex = rows.findIndex((e) => e.key === selected);
  const { revealed } = store;
  useEffect(() => {
    const element = box.current;
    if (!element || selectedIndex < 0) return;
    const top = selectedIndex * ROW_HEIGHT;
    const height = element.clientHeight - ROW_HEIGHT; // the sticky header
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + ROW_HEIGHT > element.scrollTop + height) element.scrollTop = top + ROW_HEIGHT - height;
  }, [selectedIndex, revealed]);

  if (store.entries.length === 0) {
    return (
      <section className="tracks empty" aria-label="Tracks">
        <p>No tracks to show yet. Open a rekordbox XML, add songs, or drop either on the window.</p>
      </section>
    );
  }

  const { first, last } = visibleRows(scroll.top, scroll.height, rows.length);

  const cell = (entry: Entry, id: SortColumn) => {
    const { row } = entry;
    switch (id) {
      case "title":
        return row.title;
      case "artist":
        return row.artist;
      case "bpm":
        return bpmText(row.bpm);
      case "key":
        return camelot(entry.analysis?.camelot ?? row.key);
      case "time":
        return row.duration_secs ? duration(row.duration_secs) : "";
      case "genre":
        return row.genre;
      case "rating":
        return <Stars rating={row.rating} />;
    }
  };

  return (
    <div
      ref={box}
      className="tracks"
      role="grid"
      tabIndex={0}
      aria-label="Tracks"
      aria-rowcount={rows.length + 1}
      aria-activedescendant={selected ? `row-${selectedIndex}` : undefined}
      style={{ "--columns": template } as CSSProperties}
      onScroll={(event: UIEvent<HTMLDivElement>) => measure(event.currentTarget)}
    >
      <div
        className="row head"
        role="row"
        aria-rowindex={1}
        onContextMenu={(event) => {
          event.preventDefault();
          setChooser({ x: event.clientX, y: event.clientY });
        }}
      >
        {shown.map(({ id, label }) => (
          <button
            key={id}
            className={`c-${id}`}
            role="columnheader"
            tabIndex={-1}
            aria-sort={sort?.column === id ? (sort.descending ? "descending" : "ascending") : "none"}
            onClick={() => store.sortBy(id)}
          >
            {label}
            {sort?.column === id && (sort.descending ? " ↓" : " ↑")}
          </button>
        ))}
        {cues && (
          <span className="c-cues" role="columnheader">
            Cues
          </span>
        )}
      </div>
      {chooser && <ColumnMenu store={store} at={chooser} close={() => setChooser(null)} />}
      {rows.length === 0 && <p className="no-match">{store.query ? `Nothing matches “${store.query}”.` : "Nothing here."}</p>}
      <div className="rows" style={{ height: rows.length * ROW_HEIGHT }}>
        {rows.slice(first, last).map((entry, offset) => {
          const index = first + offset;
          return (
            <div
              key={entry.key}
              id={`row-${index}`}
              className="row"
              role="row"
              aria-rowindex={index + 2}
              aria-selected={entry.key === selected}
              data-playing={entry.key === player.key || undefined}
              style={{ top: index * ROW_HEIGHT }}
              onClick={() => store.select(entry.key)}
              onDoubleClick={() => {
                store.openTrack();
                store.playSelected();
              }}
            >
              {shown.map(({ id }) => (
                <span key={id} className={`c-${id}`} role="gridcell">
                  {cell(entry, id)}
                </span>
              ))}
              {cues && (
                <span className="c-cues" role="gridcell">
                  <Cues entry={entry} />
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
