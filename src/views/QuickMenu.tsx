import { useEffect, useRef, useState, type FormEvent } from "react";

import type { Store } from "../app/store";

/** Ready-made instructions, as a start. The selected track or the list on screen is the context. */
const IDEAS = [
  "Redo the cues on this track, but put C on the second drop",
  "Redo this playlist's cues, skipping tracks with a long intro",
  "Move D to the last stripped-back groove on this track",
  "Name every hot cue on this track after what happens there",
];

const TOOL: Record<string, string> = {
  list_playlists: "Listed playlists",
  search_tracks: "Searched",
  playlist_tracks: "Read a playlist",
  track_detail: "Read a track",
  propose_cues: "Tried the rules",
  write_cues: "Saved cues",
  write_rule_cues: "Applied the rules",
  undo: "Undid a change",
};

/** Cmd-K: a short instruction the model carries out with kfbox's own commands. */
export function QuickMenu({ store }: { store: Store }) {
  const [text, setText] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const noKey = store.assistant !== null && !store.assistant.key_saved;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void store.runQuick(text);
  };
  const result = store.quickResult;
  return (
    <div className="overlay" onMouseDown={() => store.closeQuick()}>
      <div className="quick" role="dialog" aria-label="Quick functions" onMouseDown={(e) => e.stopPropagation()}>
        <form onSubmit={submit}>
          <input
            ref={input}
            aria-label="Instruction"
            placeholder={store.current ? `Ask about ${store.current.row.title}, or the list…` : "Ask kfbox to do something…"}
            value={text}
            disabled={store.quickBusy || noKey}
            onChange={(e) => setText(e.target.value)}
          />
        </form>
        {noKey ? (
          <p className="quick-note">
            Quick functions use an LLM, which needs an API key.{" "}
            <button
              className="link"
              onClick={() => {
                store.closeQuick();
                store.openPanel("settings");
              }}
            >
              Add one in Settings
            </button>
          </p>
        ) : store.quickBusy ? (
          <p className="quick-note">Working… changes are saved as they're made, and undo from History.</p>
        ) : store.quickError ? (
          <p className="quick-note error" role="alert">
            {store.quickError}
          </p>
        ) : result ? (
          <div className="quick-result" role="status">
            <p>{result.reply}</p>
            <ul>
              {result.steps.map((step, i) => (
                <li key={i} className={step.ok ? undefined : "error"} title={step.error ?? undefined}>
                  {TOOL[step.tool] ?? step.tool}
                  {step.ok ? "" : " (failed)"}
                </li>
              ))}
            </ul>
          </div>
        ) : (
          <ul className="ideas">
            {IDEAS.map((idea) => (
              <li key={idea}>
                <button
                  onClick={() => {
                    setText(idea);
                    input.current?.focus();
                  }}
                >
                  {idea}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
