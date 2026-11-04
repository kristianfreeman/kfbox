import { useState } from "react";

import type { Store } from "../app/store";

const KIND: Record<string, string> = { first: "start", action: "", restore: "restore", outside: "rekordbox" };

function when(unix: number): string {
  const date = new Date(unix * 1000);
  const today = new Date().toDateString() === date.toDateString();
  return today
    ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : date.toLocaleDateString([], { month: "short", day: "numeric" });
}

/** Every change kfbox made or found, newest first, with undo, redo and restore. */
export function HistoryView({ store }: { store: Store }) {
  const [confirm, setConfirm] = useState<string | null>(null);
  const latest = store.history[0];
  return (
    <section className="view history" aria-label="History">
      <header className="view-head">
        <h1>History</h1>
        <div className="view-actions">
          <button disabled={store.saving || !latest || latest.kind === "first"} onClick={() => void store.undo()}>
            Undo
          </button>
          <button disabled={store.saving || !latest?.summary.startsWith("Undo: ")} onClick={() => void store.redo()}>
            Redo
          </button>
        </div>
      </header>
      <ol>
        {store.history.map((entry, i) => (
          <li key={entry.commit}>
            <span className="time">{when(entry.unix_secs)}</span>
            <span className="summary">{entry.summary}</span>
            {KIND[entry.kind] && <span className="tag">{KIND[entry.kind]}</span>}
            {i > 0 &&
              (confirm === entry.commit ? (
                <span className="confirm">
                  <button
                    className="quiet-accent"
                    disabled={store.saving}
                    onClick={() => void store.restore(entry.commit).then(() => setConfirm(null))}
                  >
                    Restore
                  </button>
                  <button onClick={() => setConfirm(null)}>Cancel</button>
                </span>
              ) : (
                <button className="link" onClick={() => setConfirm(entry.commit)}>
                  Restore to here
                </button>
              ))}
          </li>
        ))}
        {store.history.length === 0 && <li className="faint">Nothing saved yet.</li>}
      </ol>
    </section>
  );
}
