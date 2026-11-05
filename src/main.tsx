import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import { Crash, noteError } from "./app/Crash";
import { perf } from "./app/perf";
import { smoke } from "./app/smoke";
import { Store } from "./app/store";
import { getBackend } from "./ipc/backend";
import "./styles/app.css";

const backend = getBackend();
const report = (text: string) => void backend.reportError(text).catch(() => undefined);
// Anything that goes wrong outside React's drawing reaches the shell's log too.
window.addEventListener("error", (event) => noteError(event.error ?? event.message, report));
window.addEventListener("unhandledrejection", (event) => noteError(event.reason, report));

const store = new Store(backend);
const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Crash report={report}>
        <App store={store} />
      </Crash>
    </StrictMode>,
  );
}
void store.start().then(() => {
  if (!root) return;
  void smoke(store, root);
  void perf(store, root);
});
