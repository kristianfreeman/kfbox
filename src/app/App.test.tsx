// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import { mockBackend } from "../ipc/mock";
import { App } from "./App";
import { Store } from "./store";

beforeAll(() => {
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
  await waitFor(() => expect(store.entries.every((e) => !e.busy)).toBe(true));
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
    fireEvent.keyDown(window, { key: "Escape" });
    expect(store.trackOpen).toBe(false);
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
