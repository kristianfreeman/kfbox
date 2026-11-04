import { beforeEach, describe, expect, it } from "vitest";

import { mockBackend } from "../ipc/mock";
import { Store } from "./store";

const settle = async (store: Store, done: () => boolean) => {
  for (let i = 0; i < 200 && !done(); i++) await new Promise((r) => setTimeout(r, 5));
  expect(done()).toBe(true);
  return store;
};

const allDone = (store: Store) => store.entries.length > 0 && store.entries.every((e) => !e.busy && e.applied);

const ready = async () => {
  const store = new Store(mockBackend(0));
  await store.start();
  await settle(store, () => allDone(store));
  return store;
};

// The track view's open state is kept between launches; each test starts afresh.
beforeEach(() => localStorage.clear());

describe("Store", () => {
  it("opens on all tracks and places cues on each", async () => {
    const store = await ready();
    expect(store.chosenPath).toBeNull();
    expect(store.entries).toHaveLength(store.total ?? -1);
    expect(store.current?.applied?.cues.map((c) => c.hot)).toEqual([0, 1, 2, 3]);
  });

  it("shows the list before any track is analysed", async () => {
    const store = new Store(mockBackend(50));
    await store.start();
    expect(store.rows).toHaveLength(6);
    expect(store.rows[0]?.row.title).toBe("Tideline");
    expect(store.rows.every((e) => e.analysis === undefined)).toBe(true);
  });

  it("saves rules only on Apply and Defaults, not when a track finishes", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    await store.start();
    await settle(store, () => allDone(store));
    expect(backend.saves).toHaveLength(0);
    await store.applyRules(true);
    expect(backend.saves).toHaveLength(1);
    await store.resetRules();
    expect(backend.saves).toHaveLength(2);
  });

  it("shows a rules error and doesn't save bad rules", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    await store.start();
    store.setRules('[[cue]]\nslot = "Z"\n');
    await store.applyRules(true);
    expect(store.rulesError).toContain("not A to P");
    expect(backend.saves).toHaveLength(0);
  });

  it("drops the old list's analyses when a playlist replaces it", async () => {
    const store = new Store(mockBackend(5));
    await store.start();
    const peak = store.playlists.find((p) => p.path.endsWith("Peak time"));
    await store.choose(peak ?? null);
    await settle(store, () => allDone(store));
    expect(store.chosenPath).toBe(peak?.path);
    expect(store.entries.map((e) => e.row.title)).toEqual(["Tideline", "Low Orbit"]);
    expect(store.entries.every((e) => e.analysis?.path === e.path)).toBe(true);
  });

  it("keeps the chosen playlist across a reload", async () => {
    const store = await ready();
    const warm = store.playlists.find((p) => p.path === "Warm up");
    await store.choose(warm ?? null);
    await store.openLibrary();
    await settle(store, () => allDone(store));
    expect(store.chosenPath).toBe("Warm up");
    expect(store.entries).toHaveLength(warm?.count ?? -1);
  });

  it("adds dropped audio once and ignores other files", async () => {
    const store = new Store(mockBackend(0));
    store.addPaths(["/x/one.mp3", "/x/one.mp3", "/x/notes.txt"]);
    expect(store.entries.map((e) => e.path)).toEqual(["/x/one.mp3"]);
    await settle(store, () => allDone(store));
    expect(store.entries[0]?.row.title).toBe("one.mp3");
  });

  it("searches every word across title, artist and genre", async () => {
    const store = await ready();
    store.setQuery("stand-in flats");
    expect(store.rows.map((e) => e.row.title)).toEqual(["Salt Flats"]);
    expect(store.selected).toBe(store.rows[0]?.key);
    store.setQuery("no such song");
    expect(store.rows).toHaveLength(0);
    store.setQuery("");
    expect(store.rows).toHaveLength(6);
  });

  it("sorts up, then down, then back to the list's order", async () => {
    const store = await ready();
    const titles = () => store.rows.map((e) => e.row.title);
    const original = titles();
    store.sortBy("bpm");
    expect(store.rows.map((e) => e.row.bpm)).toEqual([128, 130, 132, 134, 136, 138]);
    store.sortBy("bpm");
    expect(store.rows.map((e) => e.row.bpm)).toEqual([138, 136, 134, 132, 130, 128]);
    store.sortBy("bpm");
    expect(titles()).toEqual(original);
  });

  it("moves the selection through what's on screen and stops at the ends", async () => {
    const store = await ready();
    store.sortBy("bpm");
    store.selectEdge(false);
    expect(store.current?.row.bpm).toBe(128);
    store.moveSelection(1);
    expect(store.current?.row.bpm).toBe(130);
    store.moveSelection(-5);
    expect(store.current?.row.bpm).toBe(128);
    store.moveSelection(99);
    expect(store.current?.row.bpm).toBe(138);
  });

  it("fetches the whole track and its closer waveform for the deck, following the selection, folded or not", async () => {
    localStorage.setItem("kfbox.trackOpen", "false");
    const store = await ready();
    // Folded, the deck still draws the overview, so it has the track.
    await settle(store, () => store.detail?.tags.title === "Tideline");
    await settle(store, () => store.detailWaveform !== null);
    store.moveSelection(1);
    await settle(store, () => store.detail?.tags.title === "Low Orbit");
    store.openTrack();
    expect(store.detail?.tags.title).toBe("Low Orbit");
  });
});

describe("The header", () => {
  it("names the list and its count, and says what kfbox is busy with in the same place", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    await store.start();
    await settle(store, () => allDone(store));
    expect([store.title, store.activity]).toEqual(["All tracks", null]);

    const event = { done: 12, total: 83, cued: 10, waiting: 0, changed: [] };
    backend.autoCue({ state: "working", ...event });
    await settle(store, () => store.activity !== null);
    expect(store.activity).toEqual({ text: "Cueing 12 of 83", progress: 12 / 83 });
    backend.autoCue({ state: "waiting", ...event, waiting: 71 });
    await settle(store, () => store.activity?.waiting === true);
    expect(store.activity?.text).toBe("Waiting for rekordbox to close");
    backend.autoCue({ state: "done", ...event, done: 83 });
    await settle(store, () => store.activity === null);
    // Done is quiet: cues being saved is normal.
    expect(store.status).toBe("");
  });
});

describe("Live reload", () => {
  it("shows an agent's write on the open track without a reload, and in the history", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    await store.start();
    await settle(store, () => allDone(store));
    store.openTrack();
    await settle(store, () => store.detail !== null);
    const id = store.current?.id ?? "";
    expect(store.current?.row.hot).toBe("D");

    backend.agentWrite("Set B", [{ op: "add", track: id, slot: "B", start_secs: 4, name: "agent" }]);
    await settle(store, () => store.current?.applied?.cues.find((c) => c.hot === 1)?.name === "agent");
    expect(store.current?.row.hot).toBe("BD");
    expect(store.status).toContain("1 track updated");
    // The selection and the list stay where they were.
    expect(store.current?.id).toBe(id);
    expect(store.entries).toHaveLength(6);

    await store.chooseView("history");
    expect(store.history[0]?.summary).toBe("Agent: Set B");
  });
});
