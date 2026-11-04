import type { KeyboardEvent } from "react";

import type { Store } from "../app/store";

/** The rules file, edited in place, with the wizard on offer until rules are saved. */
export function RulesView({ store }: { store: Store }) {
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void store.applyRules(true);
    }
  };
  return (
    <section className="view rules" aria-label="Rules">
      <header className="view-head">
        <span className="rules-error" role="alert">
          {store.rulesError}
        </span>
        <div className="view-actions">
          <button onClick={() => store.openSheet("wizard")}>Rules wizard…</button>
          <button onClick={() => void store.resetRules()}>Defaults</button>
          <button onClick={() => void store.applyRules(true)}>Save</button>
        </div>
      </header>
      {store.firstRun && (
        <div className="offer" role="note">
          <p>
            <strong>Let the wizard write your rules.</strong> Say how you cue, and it reads the tracks you've cued by hand, drafts rules,
            and shows how well they match.
          </p>
          <button className="quiet-accent" onClick={() => store.openSheet("wizard")}>
            Open the rules wizard
          </button>
        </div>
      )}
      <textarea
        aria-label="Rules file"
        spellCheck={false}
        value={store.rulesText}
        onChange={(e) => store.setRules(e.target.value)}
        onKeyDown={onKeyDown}
      />
    </section>
  );
}
