import { useEffect, useRef } from "react";

import { HistoryView } from "../views/HistoryView";
import { NowBar } from "../views/NowBar";
import { Palette } from "../views/Palette";
import { RulesView } from "../views/RulesView";
import { SettingsSheet, WizardSheet } from "../views/Sheets";
import { Sidebar } from "../views/Sidebar";
import { TrackDock } from "../views/TrackDock";
import { TrackTable } from "../views/TrackTable";
import { useStore, type Store } from "./store";

/** Rows PageUp and PageDown move by. */
const PAGE_ROWS = 12;

/** The status line: a write in progress, its error, that it landed, or what analysis is doing. */
function Status({ store }: { store: Store }) {
  if (store.saving) return <span className="status">Saving…</span>;
  if (store.writeError)
    return (
      <span className="status error" title={store.writeError}>
        {store.writeError}
      </span>
    );
  if (store.justSaved) return <span className="status saved">✓ Saved</span>;
  return (
    <span className="status" title={store.status}>
      {store.status}
    </span>
  );
}

export function App({ store: source }: { store: Store }) {
  const store = useStore(source);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";
      const command = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      if (command && key === "k") {
        event.preventDefault();
        if (source.quickOpen) source.closeQuick();
        else source.openQuick();
        return;
      }
      if (source.quickOpen || source.sheet) {
        if (event.key === "Escape") {
          if (source.quickOpen) source.closeQuick();
          else source.openSheet(null);
        }
        return;
      }
      // With a menu bar, its items own their shortcuts and arrive as menu events.
      if (command && !source.backend.nativeMenu) {
        const menu: Record<string, () => unknown> = {
          o: () => (event.shiftKey ? source.pickSongs() : source.pickXml()),
          e: () => source.exportXml(),
          ",": () => source.openSheet("settings"),
        };
        if (!typing) menu.z = () => (event.shiftKey ? source.redo() : source.undo());
        const action = menu[key];
        if (action) {
          event.preventDefault();
          void action();
          return;
        }
      }
      if (command && key === "r") {
        event.preventDefault();
        void source.applyRulesToSelected();
        return;
      }
      if (event.key === "Escape") {
        if (target === search.current) {
          source.setQuery("");
          search.current?.blur();
        } else if (source.trackOpen) {
          source.closeTrack();
        }
        return;
      }
      // Up, Down and Enter also work from the search box, to pick a result.
      const fromSearch = target === search.current;
      if (typing && !fromSearch) return;
      if (command || event.altKey) return;
      const { player } = source;
      const actions: Record<string, () => void> = {
        ArrowUp: () => source.moveSelection(-1),
        ArrowDown: () => source.moveSelection(1),
        Enter: () => {
          source.openTrack();
          source.playSelected();
        },
      };
      if (!typing) {
        // A to H jump to hot cues; Shift-A to Shift-H set them at the playhead.
        if (/^[a-h]$/.test(event.key)) actions[event.key] = () => void player.jumpToCue(event.key);
        if (/^[A-H]$/.test(event.key) && event.shiftKey) actions[event.key] = () => void source.setCueAtPlayhead(event.key);
        Object.assign(actions, {
          " ": () => source.togglePlay(),
          ArrowLeft: () => void player.seekBy(event.shiftKey ? -4 : -1),
          ArrowRight: () => void player.seekBy(event.shiftKey ? 4 : 1),
          l: () => void player.toggleLoop(),
          q: () => source.enqueueSelected(),
          t: () => source.toggleTrack(),
          "]": () => void player.next(),
          m: () => void source.setCueAtPlayhead("memory"),
          Delete: () => void source.deleteSelectedCue(),
          Backspace: () => void source.deleteSelectedCue(),
          "+": () => source.zoomBy(2),
          "=": () => source.zoomBy(2),
          "-": () => source.zoomBy(0.5),
          PageUp: () => source.moveSelection(-PAGE_ROWS),
          PageDown: () => source.moveSelection(PAGE_ROWS),
          Home: () => source.selectEdge(false),
          End: () => source.selectEdge(true),
          "/": () => search.current?.focus(),
        });
      }
      const action = actions[event.key];
      // A focused button handles its own Enter. Space always plays, so the last
      // button clicked doesn't get pressed again.
      if (!action || (event.key === "Enter" && target?.tagName === "BUTTON")) return;
      event.preventDefault();
      action();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [source]);

  return (
    <div className={store.backend.nativeMenu ? "app native" : "app"}>
      <header className="top" data-tauri-drag-region>
        <strong className="brand" data-tauri-drag-region>
          kfbox
        </strong>
        <div className="search">
          <input
            ref={search}
            type="search"
            placeholder="Search tracks, or type a command"
            aria-label="Search tracks"
            value={store.query}
            onChange={(event) => store.setQuery(event.target.value)}
          />
          <button className="hint" aria-label="Command palette" onClick={() => store.openQuick()}>
            <kbd>⌘K</kbd>
          </button>
        </div>
        <span className="status-slot" role="status" data-tauri-drag-region>
          <Status store={store} />
        </span>
        <button className="primary" disabled={!store.canExport} onClick={() => void store.exportXml()}>
          Export
        </button>
      </header>
      <Sidebar store={store} />
      <main className="main">
        {store.view === "rules" && <RulesView store={store} />}
        {store.view === "history" && <HistoryView store={store} />}
        {store.view === "tracks" && (
          <>
            <TrackTable store={store} />
            {store.trackOpen && <TrackDock store={store} />}
          </>
        )}
      </main>
      <NowBar store={store} />
      {store.quickOpen && <Palette store={store} />}
      {store.sheet === "settings" && <SettingsSheet store={store} />}
      {store.sheet === "wizard" && <WizardSheet store={store} />}
    </div>
  );
}
