import { useState } from "react";

import { NowBar } from "../views/NowBar";
import { RulesPane } from "../views/RulesPane";
import { Sidebar } from "../views/Sidebar";
import { TrackDetail } from "../views/TrackDetail";
import { TrackList } from "../views/TrackList";
import { useStore, type Store } from "./store";

export function App({ store: source }: { store: Store }) {
  const store = useStore(source);
  const [rulesOpen, setRulesOpen] = useState(false);
  return (
    <div className="app">
      <header className="top">
        <strong className="brand">kfbox</strong>
        <span className="status" role="status">
          {store.status}
        </span>
        <div className="actions">
          <button onClick={() => void store.openLibrary()} title="Read rekordbox's library again">
            Reload rekordbox
          </button>
          <button onClick={() => void store.pickXml()}>Open XML…</button>
          <button onClick={() => void store.pickSongs()}>Add songs…</button>
          <button aria-pressed={rulesOpen} onClick={() => setRulesOpen(!rulesOpen)}>
            Rules
          </button>
          <button className="primary" disabled={!store.canExport} onClick={() => void store.exportXml()}>
            Export rekordbox XML…
          </button>
        </div>
      </header>
      <Sidebar store={store} />
      <TrackList store={store} />
      <div className="right">
        <TrackDetail entry={store.current} />
        {rulesOpen && <RulesPane store={store} />}
      </div>
      <NowBar entry={store.current} />
    </div>
  );
}
