import { Check, Plus, Star, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { Store } from "../app/store";
import type { Cue, Track } from "../ipc/types";
import { barAt, slotName } from "../lib/format";

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
export function CueName({ store, cue, track }: { store: Store; cue: Cue; track: Track }) {
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
export function useDismiss(open: boolean, close: () => void) {
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
export function Rating({ store, rating }: { store: Store; rating: number }) {
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
export function Tags({ store }: { store: Store }) {
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

/** A person's grid edits, for the deck's ⋯ menu: nudge, a downbeat at the playhead, or a BPM. */
export function GridControls({ store, editable, source }: { store: Store; editable: boolean; source: Track["grid"] }) {
  const [bpm, setBpm] = useState("");
  const entry = store.current;
  if (!entry) return null;
  return (
    <div className="menu-section" role="group" aria-label="Grid">
      <h4>{source === "rekordbox" ? "Grid · rekordbox's" : "Grid · kfbox's, from analysis"}</h4>
      {editable ? (
        <>
          <div className="nudge">
            <button title="Move the grid 10 ms earlier" disabled={store.saving} onClick={() => store.editGrid({ kind: "shift", ms: -10 })}>
              −10 ms
            </button>
            <button title="Move the grid 10 ms later" disabled={store.saving} onClick={() => store.editGrid({ kind: "shift", ms: 10 })}>
              +10 ms
            </button>
          </div>
          <button
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
  );
}
