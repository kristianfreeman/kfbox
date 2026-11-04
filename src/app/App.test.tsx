// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

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
// The track view's open state is kept between launches; each test starts afresh.
beforeEach(() => localStorage.clear());

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
  it("lists tracks, switches playlist, and shows Rules and History in place of the table", async () => {
    await open();
    const sidebar = within(screen.getByRole("navigation", { name: "Library" }));
    fireEvent.click(sidebar.getByRole("button", { name: /Warm up/ }));
    await waitFor(() => grid().getByText("Salt Flats"));
    expect(grid().queryByText("Tideline")).toBeNull();

    expect(screen.queryByLabelText("Rules file")).toBeNull();
    fireEvent.click(sidebar.getByRole("button", { name: "Rules" }));
    expect((screen.getByLabelText("Rules file") as HTMLTextAreaElement).value).toContain("[[cue]]");
    expect(screen.queryByRole("grid")).toBeNull();
    fireEvent.click(sidebar.getByRole("button", { name: "History" }));
    await screen.findByRole("region", { name: "History" });
    fireEvent.click(sidebar.getByRole("button", { name: /Warm up/ }));
    await waitFor(() => grid().getByText("Salt Flats"));
  });

  it("the sidebar's own lists: recently added and no cues, with counts", async () => {
    const store = await open();
    const sidebar = within(screen.getByRole("navigation", { name: "Library" }));
    expect(sidebar.getByRole("button", { name: /All tracks/ }).textContent).toContain("6");
    expect(sidebar.getByRole("button", { name: /Recently added/ }).textContent).toContain("2");
    expect(sidebar.getByRole("button", { name: /No cues/ }).textContent).toContain("0");
    fireEvent.click(sidebar.getByRole("button", { name: /Recently added/ }));
    expect(store.rows.map((e) => e.row.title)).toEqual(["Tideline", "Low Orbit"]);

    await store.deleteCue(store.current!.applied!.cues.find((c) => c.existing)!);
    fireEvent.click(sidebar.getByRole("button", { name: /No cues/ }));
    await waitFor(() => expect(store.rows.map((e) => e.row.title)).toEqual(["Tideline"]));
    expect(sidebar.getByRole("button", { name: /No cues/ }).textContent).toContain("1");
  });

  it("imports dropped songs into the library, shows them under Recently added, and lists what it left out", async () => {
    const backend = mockBackend(0);
    let dropped: (paths: string[]) => void = () => {};
    backend.onFileDrop = (listener) => {
      dropped = listener;
    };
    const store = new Store(backend);
    render(<App store={store} />);
    await store.start();
    await waitFor(() => grid().getByText("Tideline"));

    dropped(["/new/Fresh Cut.mp3", "/new/Tideline.mp3", "/new/cover.jpg"]);
    await waitFor(() => expect(store.rows.map((e) => e.row.title)).toContain("Fresh Cut"));
    expect(store.smart).toBe("recent");
    expect(store.current?.row.title).toBe("Fresh Cut");
    expect(store.current?.row.hot).toBe("A");
    const sheet = within(screen.getByRole("dialog", { name: "Import" }));
    expect(sheet.getByText(/Imported 1 track · 1 copy of tracks you have skipped/)).toBeTruthy();
    expect(sheet.getByRole("list", { name: "Duplicates" }).textContent).toContain("the same file as Example Artist – Tideline");
    expect(backend.calls).not.toContain("analyse /new/cover.jpg");
  });

  it("lists missing files and finds them again in a folder", async () => {
    const backend = mockBackend(0);
    backend.missing.add("2");
    const store = new Store(backend);
    render(<App store={store} />);
    await store.start();
    const sidebar = within(screen.getByRole("navigation", { name: "Library" }));
    await waitFor(() => expect(sidebar.getByRole("button", { name: /Missing files/ }).textContent).toContain("1"));
    fireEvent.click(sidebar.getByRole("button", { name: /Missing files/ }));
    expect(store.rows.map((e) => e.row.title)).toEqual(["Low Orbit"]);
    expect(within(grid().getAllByRole("row")[1]!).getByText("missing")).toBeTruthy();
    await waitFor(() => expect(store.entries.every((e) => !e.busy)).toBe(true));

    await store.findMissing();
    await waitFor(() => expect(store.rows).toHaveLength(0));
    expect(sidebar.getByRole("button", { name: /Missing files/ }).textContent).toContain("0");
    expect(store.status).toContain("Found 1 missing files");
  });

  it("rates with 0 to 5, toggles the library's own My Tags, and edits playlists from the palette", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    render(<App store={store} />);
    await store.start();
    await waitFor(() => expect(store.entries.every((e) => !e.busy)).toBe(true));

    fireEvent.keyDown(window, { key: "4" });
    await waitFor(() => expect(store.current?.row.rating).toBe(4));
    expect(within(selectedRow() as HTMLElement).getByLabelText("4 stars").textContent).toBe("★★★★");

    const situation = within(screen.getByRole("group", { name: "Situation" }));
    fireEvent.click(situation.getByRole("button", { name: "Peak" }));
    await waitFor(() => expect(store.current?.row.my_tags).toEqual(["Situation: Peak"]));
    expect(situation.getByRole("button", { name: "Peak" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(situation.getByRole("button", { name: "Peak" }));
    await waitFor(() => expect(store.current?.row.my_tags).toEqual([]));

    const palette = () => within(screen.getByRole("dialog", { name: "Command palette" }));
    const type = (text: string) => fireEvent.change(palette().getByLabelText("Command or instruction"), { target: { value: text } });
    const pick = (label: string) => fireEvent.click(palette().getByRole("option", { name: label }));
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    type("Saturday");
    pick("New playlist “Saturday” with Tideline");
    await waitFor(() => expect(store.playlists.map((p) => p.path)).toContain("Saturday"));
    expect(store.playlists.find((p) => p.path === "Saturday")?.count).toBe(1);

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    type("add warm");
    pick("Add Tideline to Warm up");
    await waitFor(() => expect(store.playlists.find((p) => p.path === "Warm up")?.count).toBe(4));

    await store.choose(store.playlists.find((p) => p.path === "Saturday")!);
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    type("Sunday");
    pick("Rename Saturday to “Sunday”");
    await waitFor(() => expect(store.chosenPath).toBe("Sunday"));
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    type("delete playlist");
    pick("Delete playlist Sunday");
    await waitFor(() => expect(store.playlists.map((p) => p.path)).not.toContain("Sunday"));
  });

  it("edits the grid only from the track view, by hand", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    render(<App store={store} />);
    await store.start();
    await waitFor(() => expect(store.entries.every((e) => !e.busy)).toBe(true));
    const grid = within(screen.getByRole("group", { name: "Grid" }));
    fireEvent.click(grid.getByRole("button", { name: "+10 ms" }));
    await waitFor(() => expect(backend.calls).toContain('grid 1 {"kind":"shift","ms":10}'));
    fireEvent.change(grid.getByLabelText("Grid BPM"), { target: { value: "131" } });
    fireEvent.submit(grid.getByLabelText("Grid BPM"));
    await waitFor(() => expect(store.current?.row.bpm).toBe(131));
    // Nothing in the palette writes a grid.
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    fireEvent.change(screen.getByLabelText("Command or instruction"), { target: { value: "grid" } });
    const gridCommands = screen
      .queryAllByRole("option")
      .map((o) => o.textContent ?? "")
      .filter((text) => /grid/i.test(text) && !text.includes("“grid”"));
    expect(gridCommands).toEqual([]);
  });

  it("exports chosen playlists to a USB drive with progress, from the palette", async () => {
    const backend = mockBackend(0);
    const store = new Store(backend);
    render(<App store={store} />);
    await store.start();
    await waitFor(() => grid().getByText("Tideline"));
    await store.openUsb();
    const sheet = within(screen.getByRole("dialog", { name: "Export to USB" }));
    expect(sheet.getByRole("radio", { name: /KF-USB/ })).toHaveProperty("checked", true);
    expect(sheet.getByText("20.0 GB free of 32.0 GB · msdos")).toBeTruthy();
    const playlists = within(sheet.getByRole("group", { name: "Playlists to export" }));
    const exportButton = sheet.getByRole("button", { name: "Export" });
    expect(exportButton).toHaveProperty("disabled", true);
    fireEvent.click(playlists.getByRole("checkbox", { name: /Warm up/ }));
    fireEvent.click(exportButton);
    await waitFor(() => expect(sheet.getByRole("status").textContent).toContain("3 tracks and 1 playlist on KF-USB"));
    expect(backend.calls).toContain("usb /Volumes/KF-USB 2");
  });

  it("lists the tracks whose drop the rules found early, and says why on the row", async () => {
    const store = await open();
    const sidebar = within(screen.getByRole("navigation", { name: "Library" }));
    expect(sidebar.getByRole("button", { name: /Early drops/ }).textContent).toContain("2");
    fireEvent.click(sidebar.getByRole("button", { name: /Early drops/ }));
    expect(store.rows.map((e) => e.row.title)).toEqual(["Glasshouse", "Night Shift"]);
    const flag = within(grid().getAllByRole("row")[1]!).getByText("early drop?");
    expect(flag.getAttribute("title")).toContain("earlier than any drop cued by hand");

    // Once B and C are set by hand, the drop is no longer the rules' to doubt.
    const id = store.current!.id!;
    await store.saveCues("Set B and C", [
      { op: "add", track: id, slot: "B", start_secs: 30, name: "" },
      { op: "add", track: id, slot: "C", start_secs: 40, name: "" },
    ]);
    await waitFor(() => expect(store.rows.map((e) => e.row.title)).toEqual(["Night Shift"]));
  });

  it("walks the table by keyboard, and the docked track view follows and folds away with T", async () => {
    const store = await open();
    expect(selectedRow()?.textContent).toContain("Tideline");
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(selectedRow()?.textContent).toContain("Low Orbit");
    fireEvent.keyDown(window, { key: "End" });
    expect(selectedRow()?.textContent).toContain("Night Shift");
    fireEvent.keyDown(window, { key: "Home" });
    expect(selectedRow()?.textContent).toContain("Tideline");

    // Docked and open from the start: it follows the selection.
    expect(store.trackOpen).toBe(true);
    await waitFor(() => expect(within(screen.getByRole("region", { name: "Track" })).getByRole("heading").textContent).toBe("Tideline"));
    fireEvent.keyDown(window, { key: "ArrowDown" });
    await waitFor(() => expect(within(screen.getByRole("region", { name: "Track" })).getByRole("heading").textContent).toBe("Low Orbit"));

    fireEvent.keyDown(window, { key: "Enter" });
    await screen.findByLabelText("Waveform with cues");
    expect(store.player.key).toBe(store.current?.key);
    fireEvent.keyDown(window, { key: "t" });
    expect(screen.queryByRole("region", { name: "Track" })).toBeNull();
    expect(localStorage.getItem("kfbox.trackOpen")).toBe("false");
    expect(new Store(mockBackend(0)).trackOpen).toBe(false);
    fireEvent.keyDown(window, { key: "t" });
    await screen.findByRole("region", { name: "Track" });
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

    fireEvent.keyDown(window, { key: "d" });
    const groove = player.entry?.applied?.cues.find((c) => c.hot === 3)?.start_secs ?? -1;
    await waitFor(() => expect(player.position()).toBeCloseTo(groove, 3));
    // C is only what the rules would place, not a cue in rekordbox, so it isn't jumped to.
    fireEvent.keyDown(window, { key: "c" });
    expect(player.position()).toBeCloseTo(groove, 3);

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

  it("Apply rules writes a track's cues straight in, and Cmd-Z undoes it", async () => {
    const store = await open();
    const dock = within(await screen.findByRole("region", { name: "Track" }));
    // Only cues in rekordbox are drawn and listed: the D every made-up track has.
    await waitFor(() => expect(within(dock.getByRole("list", { name: "Cues" })).getAllByRole("listitem")).toHaveLength(1));
    expect(dock.getByRole("button", { name: "Apply rules" }).getAttribute("title")).toBe("Fills 3 empty slots");
    fireEvent.click(dock.getByRole("button", { name: "Apply rules" }));
    await waitFor(() => expect(store.current?.row.hot).toBe("ABCD"));
    await waitFor(() => expect(dock.queryByRole("button", { name: "Apply rules" })).toBeNull());
    expect(within(dock.getByRole("list", { name: "Cues" })).getAllByRole("listitem")).toHaveLength(4);
    expect(screen.getByRole("status").textContent).toContain("Saved");

    fireEvent.keyDown(window, { key: "z", metaKey: true });
    await waitFor(() => expect(store.current?.row.hot).toBe("D"));
    fireEvent.keyDown(window, { key: "z", metaKey: true, shiftKey: true });
    await waitFor(() => expect(store.current?.row.hot).toBe("ABCD"));
  });

  it("the palette's commands filter as you type: apply rules to the list, then to the selection with ⌘R", async () => {
    const store = await open();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    const palette = within(screen.getByRole("dialog", { name: "Command palette" }));
    const commands = () =>
      within(palette.getByRole("group", { name: "Commands" }))
        .getAllByRole("option")
        .map((o) => o.textContent);
    expect(commands()).toContain("Settings…⌘,");
    fireEvent.change(palette.getByLabelText("Command or instruction"), { target: { value: "apply all" } });
    // What's typed could also name a new playlist; that comes after a command it matches.
    expect(commands()).toEqual(["Apply rules to all 6 with empty slots", "New playlist “apply all” with Tideline"]);
    // A typed command comes before sending the words to the model, so Enter runs it.
    fireEvent.keyDown(palette.getByLabelText("Command or instruction"), { key: "Enter" });
    await waitFor(() => expect(store.entries.every((e) => e.row.hot === "ABCD")).toBe(true));
    expect(screen.queryByRole("dialog")).toBeNull();
    await store.chooseView("history");
    expect(store.history[0]?.summary).toBe("Apply rules to 6 tracks");

    await store.undo();
    await store.chooseView("tracks");
    fireEvent.keyDown(window, { key: "r", metaKey: true });
    await waitFor(() => expect(store.current?.row.hot).toBe("ABCD"));
    expect(store.entries.filter((e) => e.row.hot === "ABCD")).toHaveLength(1);
  });

  it("edits cues in the track view: rename, delete, and set at the playhead", async () => {
    const store = await open();
    const cues = within(await screen.findByRole("list", { name: "Cues" }));
    // The D every made-up track starts with is in rekordbox, so it can be edited.
    fireEvent.doubleClick(cues.getByText("mine"));
    const input = cues.getByLabelText("Name of cue D");
    fireEvent.change(input, { target: { value: "groove" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(store.current?.applied?.cues.find((c) => c.hot === 3)?.name).toBe("groove"));

    fireEvent.click(cues.getByRole("button", { name: "Delete cue D" }));
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
    const dock = within(await screen.findByRole("region", { name: "Track" }));
    fireEvent.click(await dock.findByRole("button", { name: "Apply rules" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("rekordbox is running"));
    expect(store.current?.row.hot).toBe("D");
  });

  it("the API key goes in once, is only ever shown masked, and unlocks asking in the palette", async () => {
    const store = await open();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    let palette = within(screen.getByRole("dialog", { name: "Command palette" }));
    const field = () => palette.getByLabelText("Command or instruction");
    fireEvent.change(field(), { target: { value: "redo the cues on this track" } });
    fireEvent.click(palette.getByRole("button", { name: "Add one in Settings" }));
    expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull();

    const settings = within(screen.getByRole("dialog", { name: "Settings" }));
    const key = "sk-ant-test-never-shown";
    fireEvent.change(settings.getByLabelText("API key"), { target: { value: key } });
    fireEvent.click(settings.getByRole("button", { name: "Save" }));
    await waitFor(() => settings.getByText(/Saved in the Keychain/));
    expect(document.body.innerHTML).not.toContain(key);
    fireEvent.change(settings.getByLabelText("Model"), { target: { value: "claude-haiku-5-5" } });
    await waitFor(() => expect(store.assistant?.model).toBe("claude-haiku-5-5"));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    palette = within(screen.getByRole("dialog", { name: "Command palette" }));
    fireEvent.change(field(), { target: { value: "redo the cues on this track" } });
    const ask = within(palette.getByRole("group", { name: "Ask kfbox" }));
    expect(ask.getAllByRole("option").map((o) => o.textContent)).toEqual(["Run on Tideline↵", "Run on all 6 in All tracks⌘↵"]);
    fireEvent.keyDown(field(), { key: "Enter" });
    await waitFor(() => expect(palette.getByRole("status").textContent).toContain("Redid the cues on track 1"));
    expect(palette.getByRole("status").textContent).toContain("Applied the rules");
    await waitFor(() => expect(store.current?.row.hot).toBe("ABCD"));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    // It undoes like any other change.
    fireEvent.keyDown(window, { key: "z", metaKey: true });
    await waitFor(() => expect(store.current?.row.hot).toBe("D"));

    // ⌘↵ sends it for the whole list instead, with no single track as the context.
    const backend = (store as unknown as { backend: { asked: string[] } }).backend;
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    palette = within(screen.getByRole("dialog", { name: "Command palette" }));
    fireEvent.change(field(), { target: { value: "tidy the names" } });
    fireEvent.keyDown(field(), { key: "Enter", metaKey: true });
    await waitFor(() => expect(backend.asked.at(-1)).toContain("every track in the list on screen"));
  });

  it("the empty Rules view offers the wizard, which drafts, scores, saves, and undoes the save", async () => {
    const store = await open();
    await store.saveApiKey("sk-ant-test");
    expect(screen.queryByRole("note")).toBeNull();
    fireEvent.click(within(screen.getByRole("navigation", { name: "Library" })).getByRole("button", { name: "Rules" }));
    const offer = await screen.findByRole("note");
    fireEvent.click(within(offer).getByRole("button", { name: "Open the rules wizard" }));
    const wizard = within(screen.getByRole("dialog", { name: "Rules wizard" }));
    fireEvent.change(wizard.getByLabelText("How you cue"), { target: { value: "A on the first beat, C on the drop" } });
    fireEvent.click(wizard.getByRole("button", { name: "Draft my rules" }));
    const score = await wizard.findByRole("table", { name: "Score on your hand cues" });
    expect(score.textContent).toContain("A");
    expect(score.textContent).toContain("2 of 2");

    fireEvent.change(wizard.getByLabelText("More guidance"), { target: { value: "D at least 32 bars from the end" } });
    fireEvent.click(wizard.getByRole("button", { name: "Refine" }));
    await waitFor(() => expect(store.wizardBusy).toBe(false));

    store.setRules("# my old rules\n");
    fireEvent.click(wizard.getByRole("button", { name: "Save these rules" }));
    await waitFor(() => expect(store.rulesText).toContain("[[cue]]"));
    expect(screen.queryByRole("note")).toBeNull();
    fireEvent.click(wizard.getByRole("button", { name: "Undo last save" }));
    await waitFor(() => expect(store.rulesText).not.toBe(store.wizardTurn?.draft?.rules));
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
    await waitFor(() => bar.getByRole("button", { name: "D" }));
    expect(bar.queryByRole("button", { name: "A" })).toBeNull();
    fireEvent.click(bar.getByRole("button", { name: "D" }));
    const groove = store.player.entry?.applied?.cues.find((c) => c.hot === 3)?.start_secs ?? -1;
    await waitFor(() => expect(store.player.position()).toBeGreaterThanOrEqual(groove));
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
    const store = await open();
    fireEvent.click(within(screen.getByRole("navigation", { name: "Library" })).getByRole("button", { name: "Rules" }));
    const rules = screen.getByLabelText("Rules file");
    rules.focus();
    fireEvent.keyDown(rules, { key: "ArrowDown" });
    fireEvent.keyDown(rules, { key: "t" });
    expect(store.current?.row.title).toBe("Tideline");
    expect(store.trackOpen).toBe(true);
  });

  it("draws only the rows on screen in a large library, and follows the selection", async () => {
    const backend = mockBackend(0);
    const row = (i: number) => ({
      id: `${i}`,
      path: `/m/${i}.mp3`,
      title: `Track ${i}`,
      artist: "A",
      album: "",
      genre: "",
      bpm: 120,
      key: "",
      duration_secs: 60,
      added: "",
      hot: "",
      memory: 0,
      missing: false,
      rating: 0,
      my_tags: [],
    });
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
