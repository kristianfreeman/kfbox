import type { Entry } from "../app/store";
import { duration } from "../lib/format";

/** What's selected. Playback joins it in milestone 2. */
export function NowBar({ entry }: { entry: Entry | undefined }) {
  const analysis = entry?.analysis;
  return (
    <footer className="now" aria-label="Selected track">
      {entry ? (
        <>
          <div className="now-name">
            <strong>{entry.row.title}</strong>
            <span>{entry.row.artist}</span>
          </div>
          <div className="now-facts">
            {entry.row.bpm > 0 && <span>{entry.row.bpm.toFixed(2)} BPM</span>}
            {(analysis?.key ?? entry.row.key) && (
              <span>
                {analysis?.key ?? entry.row.key}
                {analysis?.camelot ? ` · ${analysis.camelot}` : ""}
              </span>
            )}
            {entry.row.duration_secs > 0 && <span>{duration(entry.row.duration_secs)}</span>}
            {analysis && <span>{analysis.grid === "rekordbox" ? "rekordbox grid" : "kfbox grid"}</span>}
          </div>
        </>
      ) : (
        <div className="now-name">
          <span>Nothing selected</span>
        </div>
      )}
    </footer>
  );
}
