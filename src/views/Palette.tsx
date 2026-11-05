import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import { commands, matching, named } from "../app/commands";
import { SMART_LABELS, type Store } from "../app/store";

const TOOL: Record<string, string> = {
  list_playlists: "Listed playlists",
  search_tracks: "Searched",
  playlist_tracks: "Read a playlist",
  track_detail: "Read a track",
  propose_cues: "Tried the rules",
  write_cues: "Saved cues",
  write_rule_cues: "Applied the rules",
  undo: "Undid a change",
};

interface Item {
  key: string;
  label: string;
  keys?: string;
  disabled?: boolean;
  run: () => unknown;
}

/** How many tracks the palette's search shows: those of the list on screen whose title, artist or genre hold
 * every word typed. */
const TRACKS_SHOWN = 6;

/** ⌘K, and the toolbar's search: one palette to find a track, run any command, or ask kfbox in plain words. */
export function Palette({ store }: { store: Store }) {
  const [text, setText] = useState("");
  const [picked, setPicked] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const entry = store.current;
  const listName = store.chosenPath?.split(" › ").pop() ?? SMART_LABELS[store.smart];
  const asking = text.trim() !== "";
  const ask: Item[] = asking
    ? [
        ...(entry?.id
          ? [{ key: "ask-track", label: `Run on ${entry.row.title}`, keys: "↵", run: () => store.runQuick(text, "track") }]
          : []),
        { key: "ask-list", label: `Run on all ${store.rows.length} in ${listName}`, keys: "⌘↵", run: () => store.runQuick(text, "list") },
      ]
    : [];
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  const tracks: Item[] = asking
    ? store.entries
        .filter((e) => {
          const said = `${e.row.title} ${e.row.artist} ${e.row.genre}`.toLowerCase();
          return words.every((w) => said.includes(w));
        })
        .slice(0, TRACKS_SHOWN)
        .map((e) => ({
          key: `track-${e.key}`,
          label: e.row.artist ? `${e.row.title} · ${e.row.artist}` : e.row.title,
          run: () => store.showTrack(e.key),
        }))
    : [];
  const found = matching(commands(store), text).map((c) => ({ ...c, key: c.id }));
  // Commands that use the words as a name come last: they always match.
  const naming = named(store, text).map((c) => ({ ...c, key: c.id }));
  // A track found comes first, then a typed command name, then sending the words to the model.
  const items: Item[] = [...tracks, ...(found.length > 0 && asking ? [...found, ...naming, ...ask] : [...ask, ...found, ...naming])];
  const index = Math.min(picked, Math.max(items.length - 1, 0));

  const run = (item: Item | undefined) => {
    if (!item || item.disabled) return;
    if (item.key.startsWith("ask-")) {
      void item.run();
      return;
    }
    store.closeQuick();
    void item.run();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setPicked((index + step + items.length) % Math.max(items.length, 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if ((event.metaKey || event.ctrlKey) && asking) void store.runQuick(text, "list");
      else run(items[index]);
    }
  };

  const noKey = store.assistant !== null && !store.assistant.key_saved;
  const result = store.quickResult;
  const section = (title: string, list: Item[]) =>
    list.length > 0 && (
      <div className="palette-section" role="group" aria-label={title}>
        <h4>{title}</h4>
        {list.map((item) => (
          <button
            key={item.key}
            className="palette-item"
            role="option"
            aria-selected={items[index] === item}
            disabled={item.disabled || (item.key.startsWith("ask-") && (noKey || store.quickBusy))}
            onMouseEnter={() => setPicked(items.indexOf(item))}
            onClick={() => run(item)}
          >
            <span>{item.label}</span>
            {item.keys && <kbd>{item.keys}</kbd>}
          </button>
        ))}
      </div>
    );

  return (
    <div className="overlay" onMouseDown={() => store.closeQuick()}>
      <div className="palette" role="dialog" aria-label="Command palette" onMouseDown={(e) => e.stopPropagation()}>
        <input
          ref={input}
          aria-label="Command or instruction"
          placeholder="Find a track, type a command, or ask kfbox to do something…"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setPicked(0);
          }}
          onKeyDown={onKeyDown}
        />
        <div className="palette-list" role="listbox" aria-label="Results">
          {asking && noKey && (
            <p className="palette-note">
              Asking kfbox uses an LLM, which needs an API key.{" "}
              <button className="link" onClick={() => store.openSheet("settings")}>
                Add one in Settings
              </button>
            </p>
          )}
          {store.quickBusy && <p className="palette-note">Working… changes are saved as they're made, and undo from History.</p>}
          {store.quickError && (
            <p className="palette-note error" role="alert">
              {store.quickError}
            </p>
          )}
          {result && !store.quickBusy && (
            <div className="palette-note" role="status">
              <p>{result.reply}</p>
              <p className="steps">{result.steps.map((s) => (TOOL[s.tool] ?? s.tool) + (s.ok ? "" : " (failed)")).join(" · ")}</p>
            </div>
          )}
          {section("Tracks", tracks)}
          {section(
            "Ask kfbox",
            items.filter((i) => i.key.startsWith("ask-")),
          )}
          {section(
            "Commands",
            items.filter((i) => !i.key.startsWith("ask-") && !i.key.startsWith("track-")),
          )}
          {items.length === 0 && <p className="palette-note">No commands match.</p>}
        </div>
      </div>
    </div>
  );
}
