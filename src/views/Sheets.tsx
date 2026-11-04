import { useState, type FormEvent, type ReactNode } from "react";

import { importSummary, type Store } from "../app/store";

/** A dialog over the window, closed by Escape, the backdrop or its button. */
function Sheet({ store, title, children }: { store: Store; title: string; children: ReactNode }) {
  return (
    <div className="overlay" onMouseDown={() => store.openSheet(null)}>
      <section className="sheet" role="dialog" aria-label={title} onMouseDown={(e) => e.stopPropagation()}>
        <header className="sheet-head">
          <h2>{title}</h2>
          <button aria-label="Close" onClick={() => store.openSheet(null)}>
            ✕
          </button>
        </header>
        <div className="sheet-body">{children}</div>
      </section>
    </div>
  );
}

/** The model, and the API key: entered here once, kept in the Keychain, never shown again. */
export function SettingsSheet({ store }: { store: Store }) {
  const [key, setKey] = useState("");
  const settings = store.assistant;
  const save = (event: FormEvent) => {
    event.preventDefault();
    void store.saveApiKey(key).then(() => setKey(""));
  };
  return (
    <Sheet store={store} title="Settings">
      <h3>Cues</h3>
      <label className="field toggle">
        <input type="checkbox" checked={store.autoCue} onChange={(e) => void store.setAutoCue(e.target.checked)} />
        <span>Auto-cue</span>
      </label>
      <p className="faint">
        When kfbox opens, and when songs come in, it puts your rules' cues on every track with an empty slot, as one change in History you
        can undo. It never moves a cue that's there, and a cue you delete or undo stays gone. While rekordbox is open, it waits.
      </p>
      <h3>Assistant</h3>
      <p className="faint">⌘K and the rules wizard use Claude through Anthropic's API.</p>
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
          <button className="quiet-accent" disabled={!key.trim()}>
            Save
          </button>
        </form>
      )}
      <p className="faint">The key is stored in the macOS Keychain. kfbox only reads it to make a request, and never shows it again.</p>
      <h3>Rules</h3>
      <button onClick={() => store.openSheet("wizard")}>Write my rules with the wizard…</button>
    </Sheet>
  );
}

/** The rules wizard: say how you cue, see the draft scored on your hand cues, refine, save. */
export function WizardSheet({ store }: { store: Store }) {
  const [said, setSaid] = useState("");
  const turn = store.wizardTurn;
  const draft = turn?.draft;
  const fixes = draft?.score.slots.reduce((sum, s) => sum + s.fixes, 0) ?? 0;
  const noKey = store.assistant !== null && !store.assistant.key_saved;
  const send = (event: FormEvent) => {
    event.preventDefault();
    void store.wizardSay(said).then(() => setSaid(""));
  };
  return (
    <Sheet store={store} title="Rules wizard">
      {noKey && (
        <p className="error">
          The wizard uses an LLM, which needs an API key.{" "}
          <button className="link" onClick={() => store.openSheet("settings")}>
            Add one in Settings
          </button>
        </p>
      )}
      {!turn && (
        <p className="faint">
          Say how you set your cues, such as "A on the first beat, B 8 bars before the drop, C on the drop, D on the last groove". The
          wizard reads the tracks you cued by hand, writes rules, and scores them on those tracks.
        </p>
      )}
      {turn && <p className="reply">{turn.reply}</p>}
      {draft && (
        <>
          <table className="score" aria-label="Score on your hand cues">
            <tbody>
              <tr>
                {draft.score.slots.map((s) => (
                  <th key={s.slot}>{s.slot}</th>
                ))}
              </tr>
              <tr>
                {draft.score.slots.map((s) => (
                  <td key={s.slot}>
                    {s.hits} of {s.tracks}
                  </td>
                ))}
              </tr>
              {fixes > 0 && (
                <tr aria-label="On your fixes">
                  {draft.score.slots.map((s) => (
                    <td key={s.slot}>{s.fixes ? `${s.fix_hits} of ${s.fixes} fixes` : ""}</td>
                  ))}
                </tr>
              )}
            </tbody>
          </table>
          <p className="faint">
            Hits: the rule's cue in the same bar as yours, on {draft.score.tracks.length} tracks you cued by hand
            {fixes > 0 && `, and where you moved or deleted ${fixes} cues kfbox placed`}.
          </p>
          <textarea className="draft" aria-label="Draft rules" readOnly value={draft.rules} />
          <div className="actions-row">
            <button className="quiet-accent" onClick={() => void store.saveWizardRules()}>
              Save these rules
            </button>
            <button onClick={() => void store.restorePreviousRules()}>Undo last save</button>
            <button onClick={() => void store.wizardStartOver()}>Start over</button>
          </div>
        </>
      )}
      {store.wizardError && (
        <p className="error" role="alert">
          {store.wizardError}
        </p>
      )}
      <form onSubmit={send} className="wizard-say">
        <textarea
          aria-label={turn ? "More guidance" : "How you cue"}
          placeholder={turn ? "More guidance, such as: D is always at least 32 bars from the end" : "How do you set your cues?"}
          value={said}
          disabled={store.wizardBusy || noKey}
          onChange={(e) => setSaid(e.target.value)}
        />
        <button className="quiet-accent" disabled={store.wizardBusy || noKey || !said.trim()}>
          {store.wizardBusy ? "Working…" : turn ? "Refine" : "Draft my rules"}
        </button>
      </form>
    </Sheet>
  );
}

/** What the last import left out or couldn't analyse, and why. */
export function ImportSheet({ store }: { store: Store }) {
  const report = store.importReport;
  if (!report) return null;
  return (
    <Sheet store={store} title="Import">
      <p>{importSummary(report)}.</p>
      {report.duplicates.length > 0 && (
        <>
          <h3>Duplicates</h3>
          <ul className="report" aria-label="Duplicates">
            {report.duplicates.map((d) => (
              <li key={d.path}>
                <span className="path">{d.path}</span>
                <span className="faint">
                  {d.kind === "copy" ? "the same file as" : "imported, but looks like"} {d.of.artist} – {d.of.title}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {report.already.length > 0 && (
        <>
          <h3>Already in the library</h3>
          <ul className="report" aria-label="Already in the library">
            {report.already.map((path) => (
              <li key={path}>
                <span className="path">{path}</span>
              </li>
            ))}
          </ul>
        </>
      )}
      {report.problems.length > 0 && (
        <>
          <h3>Problems</h3>
          <ul className="report" aria-label="Problems">
            {report.problems.map((p) => (
              <li key={p.path}>
                <span className="path">{p.path}</span>
                <span className="faint">{p.why}</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </Sheet>
  );
}

const gigabytes = (bytes: number) => `${(bytes / 1e9).toFixed(1)} GB`;

/** USB export: a drive, the playlists to put on it, and how it's going. */
export function UsbSheet({ store }: { store: Store }) {
  const [drivePath, setDrivePath] = useState<string | null>(null);
  const [chosen, setChosen] = useState<number[]>(() => (store.editablePlaylist ? [store.editablePlaylist.index] : []));
  const drive = store.drives.find((d) => d.path === drivePath) ?? store.drives[0];
  const toggle = (index: number) => setChosen((now) => (now.includes(index) ? now.filter((i) => i !== index) : [...now, index]));
  const progress = store.usbProgress;
  const report = store.usbReport;
  return (
    <Sheet store={store} title="Export to USB">
      <h3>Drive</h3>
      {store.drives.length === 0 ? (
        <p className="faint">No drives found. Plug in a USB stick, then open this again.</p>
      ) : (
        <div className="choices" role="radiogroup" aria-label="Drive">
          {store.drives.map((d) => (
            <label key={d.path} className="choice">
              <input type="radio" name="drive" checked={d === drive} onChange={() => setDrivePath(d.path)} disabled={store.usbBusy} />
              <span>{d.name}</span>
              <span className="faint">
                {d.total_bytes ? `${gigabytes(d.free_bytes)} free of ${gigabytes(d.total_bytes)}` : d.path}
                {d.file_system && ` · ${d.file_system}`}
              </span>
            </label>
          ))}
        </div>
      )}
      <h3>Playlists</h3>
      <div className="choices" role="group" aria-label="Playlists to export">
        {store.playlists.map((p) => (
          <label key={p.path} className="choice" style={{ paddingLeft: `${(p.path.split(" › ").length - 1) * 14}px` }}>
            <input type="checkbox" checked={chosen.includes(p.index)} onChange={() => toggle(p.index)} disabled={store.usbBusy} />
            <span>{p.path.split(" › ").pop()}</span>
            <span className="faint">{p.folder ? "folder" : `${p.count} tracks`}</span>
          </label>
        ))}
      </div>
      {progress && (
        <div className="usb-progress">
          <progress aria-label="Export progress" max={progress.total || 1} value={progress.done} />
          <span className="faint">
            {progress.stage}
            {progress.title && `: ${progress.title}`} {progress.total > 0 && `(${progress.done} of ${progress.total})`}
          </span>
        </div>
      )}
      {store.usbError && (
        <p className="error" role="alert">
          {store.usbError}
        </p>
      )}
      {report && (
        <p role="status">
          {report.tracks} {report.tracks === 1 ? "track" : "tracks"} and {report.playlists}{" "}
          {report.playlists === 1 ? "playlist" : "playlists"} on {drive?.name}
          {report.reused > 0 && `, ${report.reused} already there`}
          {report.removed > 0 && `, ${report.removed} taken off`}
          {report.verified ? ", checked." : ", not checked."}
          {report.failed.length > 0 && ` Left out: ${report.failed.join("; ")}.`}
        </p>
      )}
      <div className="actions-row">
        {store.usbBusy ? (
          <button onClick={() => store.cancelUsb()}>Stop</button>
        ) : (
          <button
            className="quiet-accent"
            disabled={!drive || chosen.length === 0}
            onClick={() => drive && void store.exportUsb(drive, chosen)}
          >
            Export
          </button>
        )}
      </div>
    </Sheet>
  );
}
