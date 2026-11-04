import { useState, type FormEvent } from "react";

import type { Store } from "../app/store";

/** The model, and the API key: entered here once, kept in the Keychain, never shown again. */
export function SettingsPane({ store }: { store: Store }) {
  const [key, setKey] = useState("");
  const settings = store.assistant;
  const save = (event: FormEvent) => {
    event.preventDefault();
    void store.saveApiKey(key).then(() => setKey(""));
  };
  return (
    <section className="panel" aria-label="Settings">
      <div className="bar">
        <strong>Settings</strong>
      </div>
      <div className="panel-body">
        <h3>Assistant</h3>
        <p className="faint">Quick functions (⌘K) and the rules wizard use Claude through Anthropic's API.</p>
        <label className="field">
          <span>Model</span>
          <select aria-label="Model" value={settings?.model ?? ""} onChange={(e) => void store.setModel(e.target.value)}>
            {settings?.models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
        </label>
        {settings?.key_saved ? (
          <div className="field">
            <span>API key</span>
            <span className="saved-key">Saved in the Keychain ••••••••</span>
            <button onClick={() => void store.removeApiKey()}>Remove</button>
          </div>
        ) : (
          <form className="field" onSubmit={save}>
            <span>API key</span>
            <input
              type="password"
              aria-label="API key"
              autoComplete="off"
              spellCheck={false}
              placeholder="sk-ant-…"
              value={key}
              onChange={(e) => setKey(e.target.value)}
            />
            <button className="primary" disabled={!key.trim()}>
              Save
            </button>
          </form>
        )}
        <p className="faint">The key is stored in the macOS Keychain. kfbox only reads it to make a request, and never shows it again.</p>
        <h3>Rules</h3>
        <button onClick={() => store.openPanel("wizard")}>Write my rules with the wizard…</button>
      </div>
    </section>
  );
}
