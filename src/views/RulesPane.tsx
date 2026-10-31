import type { KeyboardEvent } from "react";

import type { Store } from "../app/store";

export function RulesPane({ store }: { store: Store }) {
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void store.applyRules(true);
    }
  };
  return (
    <section className="rules" aria-label="Rules">
      <div className="bar">
        <strong>Rules</strong>
        <button onClick={() => void store.applyRules(true)}>Apply</button>
        <button onClick={() => void store.resetRules()}>Defaults</button>
        <span className="rules-error" role="alert">
          {store.rulesError}
        </span>
      </div>
      <textarea
        aria-label="Rules file"
        spellCheck={false}
        value={store.rulesText}
        onChange={(event) => store.setRules(event.target.value)}
        onKeyDown={onKeyDown}
      />
    </section>
  );
}
