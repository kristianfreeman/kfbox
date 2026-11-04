import { useState } from "react";

import type { Store } from "../app/store";

/** Above the table in review: how many are left, the keys, and accepting them all. */
export function ReviewBar({ store }: { store: Store }) {
  const [confirming, setConfirming] = useState(false);
  const left = store.toReview.length;
  const skipped = Object.values(store.reviewStates).filter((s) => s === "skipped").length;
  return (
    <div className="review-bar" role="region" aria-label="Review">
      <strong>{left === 0 ? "Nothing left to review" : `${left} ${left === 1 ? "track has" : "tracks have"} suggested cues`}</strong>
      <span className="keys">
        <kbd>Y</kbd> accept <kbd>N</kbd> skip <kbd>J</kbd> <kbd>K</kbd> next and previous
      </span>
      {skipped > 0 && (
        <button className="link" onClick={() => void store.resetSkipped()}>
          Bring back {skipped} skipped
        </button>
      )}
      {left > 0 &&
        (confirming ? (
          <span className="confirm">
            Save cues on all {left}?
            <button className="primary" disabled={store.saving} onClick={() => void store.acceptAll().then(() => setConfirming(false))}>
              Accept all
            </button>
            <button onClick={() => setConfirming(false)}>Cancel</button>
          </span>
        ) : (
          <button onClick={() => setConfirming(true)}>Accept all {left}…</button>
        ))}
    </div>
  );
}
