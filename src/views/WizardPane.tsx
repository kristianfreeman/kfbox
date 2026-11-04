import { useState, type FormEvent } from "react";

import type { Store } from "../app/store";

/** The rules wizard: say how you cue, see the draft scored on your hand cues, refine, save. */
export function WizardPane({ store }: { store: Store }) {
  const [said, setSaid] = useState("");
  const turn = store.wizardTurn;
  const draft = turn?.draft;
  const noKey = store.assistant !== null && !store.assistant.key_saved;
  const send = (event: FormEvent) => {
    event.preventDefault();
    void store.wizardSay(said).then(() => setSaid(""));
  };
  return (
    <section className="panel" aria-label="Rules wizard">
      <div className="bar">
        <strong>Rules wizard</strong>
        {turn && <button onClick={() => void store.wizardStartOver()}>Start over</button>}
      </div>
      <div className="panel-body">
        {noKey && (
          <p className="error">
            The wizard uses an LLM, which needs an API key.{" "}
            <button className="link" onClick={() => store.openPanel("settings")}>
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
              </tbody>
            </table>
            <p className="faint">Hits: the rule's cue in the same bar as yours, on {draft.score.tracks.length} tracks you cued by hand.</p>
            <textarea className="draft" aria-label="Draft rules" readOnly value={draft.rules} />
            <div className="actions-row">
              <button className="primary" onClick={() => void store.saveWizardRules()}>
                Save these rules
              </button>
              <button onClick={() => void store.restorePreviousRules()}>Undo last save</button>
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
          <button className="primary" disabled={store.wizardBusy || noKey || !said.trim()}>
            {store.wizardBusy ? "Working…" : turn ? "Refine" : "Draft my rules"}
          </button>
        </form>
      </div>
    </section>
  );
}
