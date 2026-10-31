import type { Entry } from "../app/store";
import { clock } from "../lib/format";

/** What's selected. Playback joins it in milestone 2. */
export function NowBar({ entry }: { entry: Entry | undefined }) {
  const track = entry?.track;
  return (
    <footer className="now" aria-label="Selected track">
      {track ? (
        <>
          <div className="now-name">
            <strong>{track.tags.title}</strong>
            <span>{track.tags.artist}</span>
          </div>
          <div className="now-facts">
            <span>{track.bpm.toFixed(2)} BPM</span>
            <span>
              {track.key ?? "?"}
              {track.camelot ? ` · ${track.camelot}` : ""}
            </span>
            <span>{clock(track.duration_secs)}</span>
            <span>{track.bars.length} bars</span>
            <span>{track.grid === "rekordbox" ? "rekordbox grid" : "kfbox grid"}</span>
          </div>
        </>
      ) : (
        <div className="now-name">
          <span>{entry?.title ?? "Nothing selected"}</span>
        </div>
      )}
    </footer>
  );
}
