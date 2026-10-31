import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import { Store } from "./app/store";
import { getBackend } from "./ipc/backend";
import "./styles/app.css";

const store = new Store(getBackend());
const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App store={store} />
    </StrictMode>,
  );
}
void store.start();
