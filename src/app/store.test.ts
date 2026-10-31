import { describe, expect, it } from "vitest";

import { mockBackend } from "../ipc/mock";
import { Store } from "./store";

const settle = async (store: Store, done: () => boolean) => {
  for (let i = 0; i < 200 && !done(); i++) await new Promise((r) => setTimeout(r, 5));
  expect(done()).toBe(true);
  return store;
};

const allDone = (store: Store) => store.entries.length > 0 && store.entries.every((e) => !e.busy && e.applied);

describe("Store", () => {
  it("opens on all tracks and places cues on each", async () => {
    const store = new Store(mockBackend(0));
    await store.start();
    await settle(store, () => allDone(store));
    expect(store.chosenPath).toBeNull();
    expect(store.entries).toHaveLength(store.total ?? -1);
    expect(store.current?.applied?.cues.map((c) => c.hot)).toEqual([0, 1, 2, 3]);
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
    expect(store.entries.map((e) => e.title)).toEqual(["Example Artist – Tideline", "Example Artist – Low Orbit"]);
    expect(store.entries.every((e) => e.track?.path === e.path)).toBe(true);
  });

  it("keeps the chosen playlist across a reload", async () => {
    const store = new Store(mockBackend(0));
    await store.start();
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
  });
});
