import { useState } from "react";

import type { Store } from "../app/store";

/** Writes the rules' cues into the empty slots of every track in the list, after asking once. */
export function ApplyRules({ store }: { store: Store }) {
  const [confirming, setConfirming] = useState(false);
  const targets = store.ruleTargets.length;
  if (confirming) {
    return (
      <span className="confirm">
        Write cues to {targets} {targets === 1 ? "track" : "tracks"}?
        <button className="primary" disabled={store.saving} onClick={() => void store.applyRulesToList().then(() => setConfirming(false))}>
          Write
        </button>
        <button onClick={() => setConfirming(false)}>Cancel</button>
      </span>
    );
  }
  return (
    <button
      disabled={targets === 0 || store.saving}
      title="Write the rules' cues into empty slots, with undo"
      onClick={() => setConfirming(true)}
    >
      Apply rules…
    </button>
  );
}
