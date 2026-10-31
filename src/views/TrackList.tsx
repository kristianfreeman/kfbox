import type { Entry, Store } from "../app/store";
import { cueSummary, fileName, trackName } from "../lib/format";

function Meta({ entry }: { entry: Entry }) {
  const track = entry.track;
  if (entry.error) return <span className="meta error">{entry.error}</span>;
  if (entry.busy || !track) return <span className="meta">analysing…</span>;
  const kept = cueSummary(entry.applied, true);
  const added = cueSummary(entry.applied, false);
  return (
    <span className="meta">
      <span>{track.bpm.toFixed(2)}</span>
      <span>{track.camelot ?? track.key ?? "–"}</span>
      {kept && <span>kept {kept}</span>}
      {added && <span className="added">new {added}</span>}
      {entry.id && track.grid === "kfbox" && <span>kfbox grid</span>}
    </span>
  );
}

export function TrackList({ store }: { store: Store }) {
  if (store.entries.length === 0) {
    return (
      <section className="tracks empty" aria-label="Tracks">
        <p>No tracks to show yet. Open a rekordbox XML, add songs, or drop either on the window.</p>
      </section>
    );
  }
  return (
    <section className="tracks" aria-label="Tracks">
      <ol>
        {store.entries.map((entry) => (
          <li key={entry.key}>
            <button aria-current={entry.key === store.selected} onClick={() => store.select(entry.key)}>
              <span className="title">{entry.track ? trackName(entry.track) : (entry.title ?? fileName(entry.path))}</span>
              <Meta entry={entry} />
            </button>
          </li>
        ))}
      </ol>
    </section>
  );
}
