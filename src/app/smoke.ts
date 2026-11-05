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
 * menu bar's item. The table is scrolled to its end, narrowed to one track by a
 * search and widened again, and must still draw rows where it's looking. */
export async function smoke(store: Store, root: HTMLElement) {
  if (!(await store.backend.smokeMode().catch(() => false))) return;
  try {
    for (let i = 0; i < 200 && !(store.entries.length > 0 && store.detail); i++) await wait(100);
    // The deck draws in effects and frames after the track arrives: give them a moment to fail.
    await wait(1500);
    if (pageErrors.length) throw new Error(pageErrors[0]);
    const status = root.querySelector('.activity [role="status"]')?.textContent ?? "";
    if (!status) throw new Error("the page didn't mount");
    if (store.entries.length === 0) throw new Error(`no tracks listed: ${store.status || store.writeError}`);
    if (!root.querySelector('[aria-label="Track"]')) throw new Error("the deck has no track");
    if (!root.querySelector(".deck-wave canvas")) throw new Error("the deck didn't draw its waveform");
    store.toggleSidebar();
    await store.backend.sidebarShown(store.sidebarOpen);
    store.toggleSidebar();
    await store.backend.sidebarShown(store.sidebarOpen);
    await rowsStayDrawn(store, root);
    if (pageErrors.length) throw new Error(pageErrors[0]);
    await store.backend.smokeDone(true, `${store.title}, ${store.entries.length} tracks, deck and menu answer`);
  } catch (e) {
    await store.backend.smokeDone(false, String(e));
  }
}

/** Rows the table draws inside its own view: none means the list has gone blank. */
function rowsInView(table: HTMLElement): number {
  const view = table.getBoundingClientRect();
  return [...table.querySelectorAll(".rows .row")].filter((row) => {
    const box = row.getBoundingClientRect();
    return box.bottom > view.top && box.top < view.bottom;
  }).length;
}

/** Scrolls the table to its end, shows one track then all again, and folds the sidebar: rows must still draw. */
async function rowsStayDrawn(store: Store, root: HTMLElement) {
  const table = () => root.querySelector<HTMLElement>(".tracks[role=grid]");
  // Timers, not frames: a hidden window may not draw frames at all.
  const frames = () => wait(250);
  const first = table();
  if (!first) throw new Error("the track table didn't draw");
  first.scrollTop = first.scrollHeight;
  await frames();
  if (first.scrollTop === 0) throw new Error(`the table doesn't scroll: ${store.rows.length} rows in ${first.clientHeight} px`);
  store.setQuery(store.rows[0]?.row.title ?? "");
  await frames();
  store.setQuery("");
  await frames();
  store.toggleSidebar();
  await frames();
  store.toggleSidebar();
  await frames();
  const now = table();
  if (!now || rowsInView(now) === 0) throw new Error(`the table went blank: ${store.rows.length} rows, scrolled to ${now?.scrollTop}`);
}
