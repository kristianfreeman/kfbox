import { useEffect, useRef, useState, type UIEvent } from "react";

import type { Entry, SortColumn, Store } from "../app/store";
import { cueSummary, duration } from "../lib/format";

const ROW_HEIGHT = 32;
/** Rows drawn beyond the edges, so a fast scroll doesn't show gaps. */
const OVERSCAN = 8;
/** The table's height before it has been measured, as in a test. */
const FALLBACK_HEIGHT = 800;

const COLUMNS: { id: SortColumn; label: string; className: string }[] = [
  { id: "title", label: "Title", className: "c-title" },
  { id: "artist", label: "Artist", className: "c-artist" },
  { id: "bpm", label: "BPM", className: "c-bpm" },
  { id: "key", label: "Key", className: "c-key" },
  { id: "time", label: "Time", className: "c-time" },
  { id: "genre", label: "Genre", className: "c-genre" },
];

function Cues({ entry }: { entry: Entry }) {
  if (entry.error) return <span className="error" title={entry.error}>failed</span>;
  if (entry.busy) return <span className="faint">analysing…</span>;
  const kept = cueSummary(entry.applied, true);
  const added = cueSummary(entry.applied, false);
  return (
    <>
      <span>{kept}</span>
      {added && <span className="added"> +{added}</span>}
    </>
  );
}

export function TrackTable({ store }: { store: Store }) {
  const box = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState({ top: 0, height: FALLBACK_HEIGHT });
  const { rows, selected, sort } = store;

  const measure = (element: HTMLDivElement) => setScroll({ top: element.scrollTop, height: element.clientHeight || FALLBACK_HEIGHT });
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    measure(element);
    const observer = new ResizeObserver(() => measure(element));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Keeps the selected row in view as the keyboard moves it.
  const selectedIndex = rows.findIndex((e) => e.key === selected);
  useEffect(() => {
    const element = box.current;
    if (!element || selectedIndex < 0) return;
    const top = selectedIndex * ROW_HEIGHT;
    const height = element.clientHeight - ROW_HEIGHT; // the sticky header
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + ROW_HEIGHT > element.scrollTop + height) element.scrollTop = top + ROW_HEIGHT - height;
  }, [selectedIndex]);

  if (store.entries.length === 0) {
    return (
      <section className="tracks empty" aria-label="Tracks">
        <p>No tracks to show yet. Open a rekordbox XML, add songs, or drop either on the window.</p>
      </section>
    );
  }

  const first = Math.max(0, Math.floor(scroll.top / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((scroll.top + scroll.height) / ROW_HEIGHT) + OVERSCAN);

  return (
    <div
      ref={box}
      className="tracks"
      role="grid"
      tabIndex={0}
      aria-label="Tracks"
      aria-rowcount={rows.length + 1}
      aria-activedescendant={selected ? `row-${selectedIndex}` : undefined}
      onScroll={(event: UIEvent<HTMLDivElement>) => measure(event.currentTarget)}
    >
      <div className="row head" role="row" aria-rowindex={1}>
        {COLUMNS.map(({ id, label, className }) => (
          <button
            key={id}
            className={className}
            role="columnheader"
            tabIndex={-1}
            aria-sort={sort?.column === id ? (sort.descending ? "descending" : "ascending") : "none"}
            onClick={() => store.sortBy(id)}
          >
            {label}
            {sort?.column === id && (sort.descending ? " ↓" : " ↑")}
          </button>
        ))}
        <span className="c-cues" role="columnheader">
          Cues
        </span>
      </div>
      {rows.length === 0 && <p className="no-match">Nothing matches “{store.query}”.</p>}
      <div className="rows" style={{ height: rows.length * ROW_HEIGHT }}>
        {rows.slice(first, last).map((entry, offset) => {
          const index = first + offset;
          const { row } = entry;
          return (
            <div
              key={entry.key}
              id={`row-${index}`}
              className="row"
              role="row"
              aria-rowindex={index + 2}
              aria-selected={entry.key === selected}
              style={{ top: index * ROW_HEIGHT }}
              onClick={() => store.select(entry.key)}
              onDoubleClick={() => store.openTrack()}
            >
              <span className="c-title" role="gridcell">
                {row.title}
              </span>
              <span className="c-artist" role="gridcell">
                {row.artist}
              </span>
              <span className="c-bpm" role="gridcell">
                {row.bpm ? row.bpm.toFixed(2) : ""}
              </span>
              <span className="c-key" role="gridcell">
                {entry.analysis?.camelot ?? row.key}
              </span>
              <span className="c-time" role="gridcell">
                {row.duration_secs ? duration(row.duration_secs) : ""}
              </span>
              <span className="c-genre" role="gridcell">
                {row.genre}
              </span>
              <span className="c-cues" role="gridcell">
                <Cues entry={entry} />
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
