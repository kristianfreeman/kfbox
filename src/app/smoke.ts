import { pageErrors } from "./Crash";
import type { Store } from "./store";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** In the app's smoke test (`scripts/smoke-app.mjs`), checks the page is up and
 * working against the real shell, then reports. The shell prints the report
 * and quits; a page that never mounts, or a shell that hangs, never reports,
 * and the test times out.
 *
 * It goes through what launch does: the window shows the library's list, the
 * deck has its track, and the sidebar folds and comes back, which renames the
 * menu bar's item. */
export async function smoke(store: Store, root: HTMLElement) {
  if (!(await store.backend.smokeMode().catch(() => false))) return;
  try {
    for (let i = 0; i < 200 && !(store.entries.length > 0 && store.detail); i++) await wait(100);
    // The deck draws in effects and frames after the track arrives: give them a moment to fail.
    await wait(1500);
    if (pageErrors.length) throw new Error(pageErrors[0]);
    const heading = root.querySelector(".heading")?.textContent ?? "";
    if (!heading) throw new Error("the page didn't mount");
    if (store.entries.length === 0) throw new Error(`no tracks listed: ${store.status || store.writeError}`);
    if (!root.querySelector('[aria-label="Track"]')) throw new Error("the deck has no track");
    if (!root.querySelector(".deck-wave canvas")) throw new Error("the deck didn't draw its waveform");
    store.toggleSidebar();
    await store.backend.sidebarShown(store.sidebarOpen);
    store.toggleSidebar();
    await store.backend.sidebarShown(store.sidebarOpen);
    if (pageErrors.length) throw new Error(pageErrors[0]);
    await store.backend.smokeDone(true, `${heading}, ${store.entries.length} tracks, deck and menu answer`);
  } catch (e) {
    await store.backend.smokeDone(false, String(e));
  }
}
