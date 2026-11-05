import type { Store } from "./store";

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** What the page counts while the perf run goes on. */
interface Counts {
  /** Store changes, each a render of the page. */
  commits: number;
  /** Intervals and timeouts the page has running. */
  intervals: Set<number>;
}

/** Counts the page's intervals from here on, so one that's never cleared shows as a climbing number. */
function countIntervals(counts: Counts) {
  const set = window.setInterval.bind(window);
  const clear = window.clearInterval.bind(window);
  window.setInterval = ((handler: TimerHandler, ms?: number, ...rest: unknown[]) => {
    const id = set(handler, ms, ...rest);
    counts.intervals.add(id);
    return id;
  }) as typeof window.setInterval;
  window.clearInterval = ((id?: number) => {
    if (id !== undefined) counts.intervals.delete(id);
    clear(id);
  }) as typeof window.clearInterval;
}

/** One thing the page did, for lining up with long frames: a backend call from ask to answer (with its reply's
 * size), or a store change. */
interface Traced {
  name: string;
  start: number;
  end: number;
  bytes?: number;
}

/** Every backend call and store change from here on, timed. */
function trace(store: Store): Traced[] {
  const log: Traced[] = [];
  const backend = store.backend as unknown as Record<string, unknown>;
  for (const [name, value] of Object.entries(backend)) {
    if (typeof value !== "function" || name.startsWith("on") || name.startsWith("perf")) continue;
    const call = value as (...args: unknown[]) => unknown;
    backend[name] = (...args: unknown[]) => {
      const result = call(...args);
      if (!(result instanceof Promise)) return result;
      const start = performance.now();
      return result.then((reply: unknown) => {
        let bytes = 0;
        try {
          bytes = JSON.stringify(reply ?? null).length;
        } catch {
          // Not data.
        }
        log.push({ name, start, end: performance.now(), bytes });
        return reply;
      });
    };
  }
  store.subscribe(() => {
    const now = performance.now();
    log.push({ name: "page update", start: now, end: now });
  });
  return log;
}

/** For each frame over 50 ms, what the page did during it: the calls answered and the updates made, counted by name
 * over all the long frames, with the replies' sizes. */
function blame(long: { start: number; end: number }[], log: Traced[], since: number) {
  const counts: Record<string, { frames: number; replies: number; bytes: number }> = {};
  let explained = 0;
  for (const frame of long) {
    const during = log.filter((t) => t.end >= frame.start && t.end <= frame.end);
    if (during.length) explained += 1;
    const names = new Set<string>();
    for (const t of during) {
      const c = (counts[t.name] ??= { frames: 0, replies: 0, bytes: 0 });
      c.replies += 1;
      c.bytes += t.bytes ?? 0;
      names.add(t.name);
    }
    for (const name of names) counts[name]!.frames += 1;
  }
  const first = long.slice(0, 8).map((f) => ({
    at_ms: Math.round(f.start - since),
    ms: Math.round(f.end - f.start),
    answered: log.filter((t) => t.end >= f.start && t.end <= f.end).map((t) => t.name),
  }));
  return { long_frames: long.length, with_something_answered: explained, during_long_frames: counts, first };
}

/** What the page did over a phase, by name: calls answered, updates made, and the replies' bytes. */
function tally(log: Traced[], from: number, to: number) {
  const counts: Record<string, { n: number; bytes: number }> = {};
  for (const t of log.filter((x) => x.end >= from && x.end <= to)) {
    const c = (counts[t.name] ??= { n: 0, bytes: 0 });
    c.n += 1;
    c.bytes += t.bytes ?? 0;
  }
  return counts;
}

/** Frame times while `run` goes on: how many frames, the median, and how many took over 50 ms. `long` collects the
 * long frames' spans. */
async function frames(run: () => Promise<void>, long: { start: number; end: number }[] = []) {
  const gaps: number[] = [];
  let going = true;
  let last = performance.now();
  const step = (now: number) => {
    if (now - last > 50) long.push({ start: last, end: now });
    gaps.push(now - last);
    last = now;
    if (going) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
  await run();
  going = false;
  gaps.shift();
  gaps.sort((a, b) => a - b);
  const median = gaps[Math.floor(gaps.length / 2)] ?? 0;
  return {
    frames: gaps.length,
    median_ms: Math.round(median * 10) / 10,
    p99_ms: Math.round((gaps[Math.floor(gaps.length * 0.99)] ?? 0) * 10) / 10,
    over_50ms: gaps.filter((g) => g > 50).length,
    worst_ms: Math.round(gaps[gaps.length - 1] ?? 0),
  };
}

/** The perf run (`pnpm perf`, `scripts/perf-app.mjs`): the app on a fixture library, a scenario played through
 * phases, each marked to the runner with what the page measured over it. The runner samples each process's CPU
 * and memory and lines them up with the marks; the shell adds how the main thread kept up. Tracks play on a silent
 * sink. The shell says how long the track-switching phase runs, in seconds; 0 is no perf run. */
export async function perf(store: Store, root: HTMLElement) {
  const leakSecs = await store.backend.perfMode().catch(() => 0);
  if (!leakSecs) return;
  const counts: Counts = { commits: 0, intervals: new Set() };
  countIntervals(counts);
  const log = trace(store);
  store.subscribe(() => (counts.commits += 1));
  const mark = (name: string, data: Record<string, unknown> = {}) => store.backend.perfMark(name, data).catch(() => undefined);
  const phase = async (name: string, ms: number, run: () => Promise<void> = () => wait(ms)) => {
    const before = counts.commits;
    await mark(`${name} start`);
    const start = performance.now();
    await run();
    const secs = (performance.now() - start) / 1000;
    await mark(`${name} end`, {
      commits_per_sec: Math.round(((counts.commits - before) / secs) * 10) / 10,
      animations: document.getAnimations().length,
      intervals: counts.intervals.size,
    });
  };
  const leakMs = leakSecs * 1000;

  // Startup: when the table first drew rows, and when the deck was ready.
  const rowsAt = await until(() => root.querySelector(".rows .row") !== null);
  const deckAt = await until(() => root.querySelector(".deck-wave canvas, .deck-wave.pending") !== null);
  await mark("startup", { rows_ms: rowsAt, deck_ms: deckAt, tracks: store.entries.length });

  // The fixture's three tracks with audio, once their analysis is in: it arrives after startup.
  const playableNow = () => store.entries.filter((e) => e.path.endsWith(".wav") && e.analysis);
  for (let i = 0; i < 240 && playableNow().length < 3; i++) await wait(250);
  const playable = playableNow();
  const first = playable[0];
  await mark("analysed", { playable: playable.length });
  if (!first) {
    const wavs = store.entries.filter((e) => e.path.endsWith(".wav"));
    await mark("error", {
      reason: `no analysed fixture track: ${wavs.length} with audio among ${store.entries.length}`,
      paths: wavs.map((e) => e.path),
    });
    await store.backend.perfDone();
    return;
  }

  await phase("idle, nothing loaded", 30_000);

  store.select(first.key);
  await store.player.load(first, false);
  await wait(2_000);
  await phase("idle, loaded and paused", 30_000);

  if (!store.metronome) store.toggleMetronome();
  store.openTrack();
  let played: Awaited<ReturnType<typeof frames>> | undefined;
  const long: { start: number; end: number }[] = [];
  const playStart = performance.now();
  await phase("playing, visible", 60_000, async () => {
    played = await frames(() => playFor(store, 60_000), long);
  });
  await mark("playing frames", played ?? {});
  await mark("playing: what the page did", tally(log, playStart, performance.now()));
  await mark("playing: during long frames", blame(long, log, playStart));

  await store.backend.perfWindow(false);
  await phase("playing, hidden", 30_000, () => playFor(store, 30_000));
  await store.backend.perfWindow(true);
  await wait(1_000);

  // Interaction: the table scrolled top to bottom fast, the overview dragged past both ends, search typed into.
  const table = root.querySelector<HTMLElement>(".tracks[role=grid]");
  const scroll = await frames(async () => {
    if (!table) return;
    for (let at = 0; at <= table.scrollHeight; at += 400) {
      table.scrollTop = at;
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    table.scrollTop = 0;
  });
  await mark("interaction: table scroll", scroll);
  const drag = await frames(() => dragOverview(root));
  await mark("interaction: overview drag past both ends", drag);
  const search = await frames(async () => {
    store.openQuick();
    await wait(200);
    const input = root.ownerDocument.querySelector<HTMLInputElement>('[aria-label="Command or instruction"]');
    for (const letter of "track 1") {
      if (input) {
        const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
        set?.call(input, input.value + letter);
        input.dispatchEvent(new Event("input", { bubbles: true }));
      }
      await wait(80);
    }
    store.closeQuick();
  });
  await mark("interaction: search typed", search);

  // Leaks: playing, switching tracks every 20 s; the counts must not climb.
  await phase("switching tracks", leakMs, async () => {
    const end = performance.now() + leakMs;
    let n = 0;
    while (performance.now() < end) {
      const entry = playable[n % playable.length]!;
      store.select(entry.key);
      store.playSelected();
      await wait(Math.min(20_000, end - performance.now()));
      n += 1;
      await mark("switch", { animations: document.getAnimations().length, intervals: counts.intervals.size, commits: counts.commits });
    }
  });
  if (store.player.playing) store.togglePlay();
  await store.backend.perfDone();
}

/** Milliseconds since the page started when `ready` first holds, checked every frame or so. */
async function until(ready: () => boolean): Promise<number> {
  for (let i = 0; i < 1_000 && !ready(); i++) await wait(16);
  return Math.round(performance.now());
}

/** Plays for `ms`, from the start again every 25 s: the fixture's tracks are 30 s long. */
async function playFor(store: Store, ms: number) {
  if (!store.player.playing) store.playSelected();
  const end = performance.now() + ms;
  while (performance.now() < end) {
    await wait(Math.min(25_000, end - performance.now()));
    if (performance.now() < end) await store.player.seek(0);
  }
}

/** Drags the overview's window from the middle past its start, then past its end, a step a frame. */
async function dragOverview(root: HTMLElement) {
  const strip = root.querySelector<HTMLElement>(".overview");
  if (!strip) return;
  const box = strip.getBoundingClientRect();
  const y = box.top + box.height - 6;
  const send = (type: string, x: number) =>
    strip.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, pointerId: 7, buttons: 1 }));
  send("pointerdown", box.left + box.width / 2);
  for (let x = box.left + box.width / 2; x > box.left - 200; x -= 12) {
    send("pointermove", x);
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }
  for (let x = box.left - 200; x < box.right + 200; x += 24) {
    send("pointermove", x);
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }
  send("pointerup", box.right + 200);
}
