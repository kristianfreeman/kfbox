// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import { mockBackend } from "../ipc/mock";
import { App } from "./App";
import { Store } from "./store";

beforeAll(() => {
  // rAF never fires usefully here, and pending frames outlive the test.
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

afterEach(cleanup);

const grid = () => within(screen.getByRole("grid"));

async function open() {
  const store = new Store(mockBackend(0));
  render(<App store={store} />);
  await store.start();
  await waitFor(() => grid().getByText("Tideline"));
  await waitFor(() => expect(store.entries.every((e) => !e.busy && e.applied)).toBe(true));
  return store;
}

const selectedRow = () => document.querySelector('[role="row"][aria-selected="true"]');

describe("App", () => {
  it("lists tracks, switches playlist, and opens the rules", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: /Warm up/ }));
    await waitFor(() => grid().getByText("Salt Flats"));
    expect(grid().queryByText("Tideline")).toBeNull();

    expect(screen.queryByLabelText("Rules file")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Rules" }));
    expect((screen.getByLabelText("Rules file") as HTMLTextAreaElement).value).toContain("[[cue]]");
  });

  it("walks the table by keyboard and opens the track view", async () => {
    const store = await open();
    expect(selectedRow()?.textContent).toContain("Tideline");
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(selectedRow()?.textContent).toContain("Low Orbit");
    fireEvent.keyDown(window, { key: "End" });
    expect(selectedRow()?.textContent).toContain("Night Shift");
    fireEvent.keyDown(window, { key: "Home" });
    expect(selectedRow()?.textContent).toContain("Tideline");

    fireEvent.keyDown(window, { key: "Enter" });
    expect(store.trackOpen).toBe(true);
    await screen.findByLabelText("Waveform with cues");
    expect(store.player.key).toBe(store.current?.key);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(store.trackOpen).toBe(false);
  });

  it("plays by keyboard: Space, beat and bar seeks, cues, loop, queue and zoom", async () => {
    const store = await open();
    const { player } = store;
    fireEvent.keyDown(window, { key: " " });
    await waitFor(() => expect(player.playing).toBe(true));
    await waitFor(() => expect(player.track).not.toBeNull());
    fireEvent.keyDown(window, { key: " " });
    await waitFor(() => expect(player.playing).toBe(false));

    const beat = 60 / 132;
    await player.seek(0);
    fireEvent.keyDown(window, { key: "ArrowRight" });
    await waitFor(() => expect(player.position()).toBeCloseTo(beat, 3));
    fireEvent.keyDown(window, { key: "ArrowRight", shiftKey: true });
    await waitFor(() => expect(player.position()).toBeCloseTo(5 * beat, 3));
    fireEvent.keyDown(window, { key: "ArrowLeft" });
    await waitFor(() => expect(player.position()).toBeCloseTo(4 * beat, 3));

    fireEvent.keyDown(window, { key: "c" });
    const drop = player.entry?.applied?.cues.find((c) => c.hot === 2)?.start_secs ?? -1;
    await waitFor(() => expect(player.position()).toBeCloseTo(drop, 3));

    fireEvent.keyDown(window, { key: "l" });
    await waitFor(() => expect(player.looping).toBe(true));

    fireEvent.keyDown(window, { key: "ArrowDown" });
    fireEvent.keyDown(window, { key: "q" });
    expect(player.queue.map((e) => e.row.title)).toEqual(["Low Orbit"]);
    fireEvent.keyDown(window, { key: "]" });
    await waitFor(() => expect(player.key).toBe("/music/Low Orbit.mp3"));

    fireEvent.keyDown(window, { key: "+" });
    expect(store.zoom).toBe(2);
    fireEvent.keyDown(window, { key: "-" });
    expect(store.zoom).toBe(1);
  });

  it("reviews by keyboard: J opens the next, Y accepts, N skips, Cmd-Z undoes", async () => {
    const store = await open();
    fireEvent.click(screen.getByRole("button", { name: /Needs Review/ }));
    await waitFor(() => expect(store.reviewing).toBe(true));
    expect(screen.getByRole("region", { name: "Review" }).textContent).toContain("6 tracks have suggested cues");

    fireEvent.keyDown(window, { key: "j" });
    expect(store.trackOpen).toBe(true);
    const first = store.current?.row.title;
    fireEvent.keyDown(window, { key: "y" });
    await waitFor(() => expect(store.toReview).toHaveLength(5));
    expect(store.entries.find((e) => e.row.title === first)?.row.hot).toBe("ABCD");
    expect(store.current?.row.title).not.toBe(first);

    fireEvent.keyDown(window, { key: "n" });
    await waitFor(() => expect(store.toReview).toHaveLength(4));
    expect(screen.getByRole("button", { name: /Bring back 1 skipped/ })).toBeTruthy();

    fireEvent.keyDown(window, { key: "z", metaKey: true });
    await waitFor(() => expect(store.entries.find((e) => e.row.title === first)?.row.hot).toBe("D"));
    fireEvent.keyDown(window, { key: "z", metaKey: true, shiftKey: true });
    await waitFor(() => expect(store.entries.find((e) => e.row.title === first)?.row.hot).toBe("ABCD"));
  });

  it("accepting all asks once, then saves every track in one change", async () => {
    const store = await open();
    await store.chooseReview();
    const bar = within(await screen.findByRole("region", { name: "Review" }));
    fireEvent.click(bar.getByRole("button", { name: "Accept all 6…" }));
    expect(store.toReview).toHaveLength(6);
    fireEvent.click(bar.getByRole("button", { name: "Accept all" }));
    await waitFor(() => expect(store.toReview).toHaveLength(0));
    await store.toggleHistory();
    expect(store.history[0]?.summary).toBe("Accept suggested cues on 6 tracks");
  });

  it("edits cues in the track view: rename, delete, and set at the playhead", async () => {
    const store = await open();
    store.openTrack();
    const table = await screen.findByRole("table");
    // The D every made-up track starts with is in rekordbox, so it can be edited.
    fireEvent.doubleClick(within(table).getByText("mine"));
    const input = within(table).getByLabelText("Name of cue D");
    fireEvent.change(input, { target: { value: "groove" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(store.current?.applied?.cues.find((c) => c.hot === 3)?.name).toBe("groove"));

    fireEvent.click(within(table).getByRole("button", { name: "Delete cue D" }));
    await waitFor(() => expect(store.current?.row.hot).toBe(""));

    fireEvent.keyDown(window, { key: " " });
    await waitFor(() => expect(store.player.track).not.toBeNull());
    await store.player.seek(10);
    fireEvent.keyDown(window, { key: "B", shiftKey: true });
    await waitFor(() => expect(store.current?.row.hot).toBe("B"));
    const b = store.current?.applied?.cues.find((c) => c.hot === 1);
    const beat = 60 / 132;
    expect(Math.abs((b?.start_secs ?? 0) / beat - Math.round((b?.start_secs ?? 0) / beat))).toBeLessThan(1e-6);
  });

  it("shows why a save failed and changes nothing", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    render(<App store={store} />);
    await store.start();
    await waitFor(() => expect(store.entries.every((e) => !e.busy && e.applied)).toBe(true));
    backend.rekordboxOpen = true;
    fireEvent.keyDown(window, { key: "y" });
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("rekordbox is running"));
    expect(store.current?.row.hot).toBe("D");
  });

  it("doesn't treat typing as player keys", async () => {
    const store = await open();
    const search = screen.getByLabelText("Search tracks");
    search.focus();
    fireEvent.keyDown(search, { key: " " });
    fireEvent.keyDown(search, { key: "c" });
    expect(store.player.key).toBeNull();
  });

  it("the transport plays and the bar shows the track", async () => {
    const store = await open();
    const bar = within(screen.getByLabelText("Player"));
    fireEvent.click(bar.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(store.player.playing).toBe(true));
    await waitFor(() => bar.getByRole("button", { name: "Pause" }));
    await waitFor(() => bar.getByRole("button", { name: "A" }));
    fireEvent.click(bar.getByRole("button", { name: "C" }));
    const drop = store.player.entry?.applied?.cues.find((c) => c.hot === 2)?.start_secs ?? -1;
    await waitFor(() => expect(store.player.position()).toBeGreaterThanOrEqual(drop));
  });

  it("slash searches, arrows pick a result, and Escape clears", async () => {
    const store = await open();
    const search = screen.getByLabelText("Search tracks") as HTMLInputElement;
    fireEvent.keyDown(window, { key: "/" });
    expect(document.activeElement).toBe(search);
    fireEvent.change(search, { target: { value: "placeholder" } });
    expect(store.rows).toHaveLength(2);
    fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(selectedRow()?.textContent).toContain("Glasshouse");
    fireEvent.keyDown(search, { key: "Escape" });
    expect(search.value).toBe("");
    expect(store.rows).toHaveLength(6);
  });

  it("leaves keys alone while typing rules", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Rules" }));
    const rules = screen.getByLabelText("Rules file");
    rules.focus();
    fireEvent.keyDown(rules, { key: "ArrowDown" });
    expect(selectedRow()?.textContent).toContain("Tideline");
  });

  it("draws only the rows on screen in a large library, and follows the selection", async () => {
    const backend = mockBackend(0);
    const row = (i: number) => ({ id: `${i}`, path: `/m/${i}.mp3`, title: `Track ${i}`, artist: "A", album: "", genre: "", bpm: 120, key: "", duration_secs: 60, hot: "", memory: 0 });
    backend.loadPlaylist = async () => Array.from({ length: 5000 }, (_, i) => row(i));
    const store = new Store(backend);
    render(<App store={store} />);
    await store.openLibrary();
    await waitFor(() => grid().getByText("Track 0"));
    const drawn = () => document.querySelectorAll(".rows .row").length;
    expect(drawn()).toBeLessThan(60);

    fireEvent.keyDown(window, { key: "End" });
    fireEvent.scroll(screen.getByRole("grid"));
    await waitFor(() => grid().getByText("Track 4999"));
    expect(drawn()).toBeLessThan(60);
    expect(grid().queryByText("Track 0")).toBeNull();
  });

  it("clicks select, double clicks open, and headers sort", async () => {
    const store = await open();
    fireEvent.click(grid().getByText("Paper Engine"));
    expect(store.current?.row.title).toBe("Paper Engine");
    fireEvent.doubleClick(grid().getByText("Paper Engine"));
    expect(store.trackOpen).toBe(true);
    fireEvent.click(screen.getByRole("columnheader", { name: "BPM" }));
    expect(store.rows[0]?.row.bpm).toBe(128);
    expect(screen.getByRole("columnheader", { name: "BPM ↑" }).getAttribute("aria-sort")).toBe("ascending");
  });
});
