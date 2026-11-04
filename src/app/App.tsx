import { useEffect, useRef, useState } from "react";

import { HistoryPane } from "../views/HistoryPane";
import { NowBar } from "../views/NowBar";
import { ApplyRules } from "../views/ApplyRules";
import { QuickMenu } from "../views/QuickMenu";
import { SettingsPane } from "../views/SettingsPane";
import { WizardPane } from "../views/WizardPane";
import { RulesPane } from "../views/RulesPane";
import { Sidebar } from "../views/Sidebar";
import { TrackDetail } from "../views/TrackDetail";
import { TrackTable } from "../views/TrackTable";
import { useStore, type Store } from "./store";

/** Rows PageUp and PageDown move by. */
const PAGE_ROWS = 12;

export function App({ store: source }: { store: Store }) {
  const store = useStore(source);
  const [rulesOpen, setRulesOpen] = useState(false);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (source.quickOpen) source.closeQuick();
        else source.openQuick();
        return;
      }
      if (source.quickOpen) {
        if (event.key === "Escape") source.closeQuick();
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
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z" && !typing) {
        event.preventDefault();
        void (event.shiftKey ? source.redo() : source.undo());
        return;
      }
      if (event.metaKey || event.ctrlKey || event.altKey) return;
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
    <div className="app">
      <header className="top">
        <strong className="brand">kfbox</strong>
        <input
          ref={search}
          className="search"
          type="search"
          placeholder="Search  /"
          aria-label="Search tracks"
          value={store.query}
          onChange={(event) => store.setQuery(event.target.value)}
        />
        <span className={store.writeError ? "status error" : "status"} role="status">
          {store.saving ? "Saving…" : store.writeError || store.status}
        </span>
        <div className="actions">
          <button onClick={() => void store.openLibrary()} title="Read rekordbox's library again">
            Reload
          </button>
          <button onClick={() => void store.pickXml()}>Open XML…</button>
          <button onClick={() => void store.pickSongs()}>Add songs…</button>
          <ApplyRules store={store} />
          <button aria-pressed={rulesOpen} onClick={() => setRulesOpen(!rulesOpen)}>
            Rules
          </button>
          <button aria-pressed={store.historyOpen} onClick={() => void store.toggleHistory()}>
            History
          </button>
          <button title="Quick functions" onClick={() => store.openQuick()}>
            ⌘K
          </button>
          <button aria-pressed={store.panel === "settings"} onClick={() => store.openPanel("settings")}>
            Settings
          </button>
          <button className="primary" disabled={!store.canExport} onClick={() => void store.exportXml()}>
            Export XML…
          </button>
        </div>
      </header>
      <Sidebar store={store} />
      <main className="main">
        {store.firstRun && (
          <p className="first-run" role="note">
            Let the rules wizard write your cue rules from how you cue and the tracks you've cued by hand.
            <button onClick={() => store.openPanel("wizard")}>Open the wizard</button>
            <button className="link" onClick={() => store.dismissFirstRun()}>
              Not now
            </button>
          </p>
        )}
        <TrackTable store={store} />
        {store.trackOpen && <TrackDetail store={store} />}
        {rulesOpen && <RulesPane store={store} />}
        {store.historyOpen && <HistoryPane store={store} />}
        {store.panel === "settings" && <SettingsPane store={store} />}
        {store.panel === "wizard" && <WizardPane store={store} />}
      </main>
      <NowBar store={store} />
      {store.quickOpen && <QuickMenu store={store} />}
    </div>
  );
}
