import { describe, expect, it } from "vitest";

import { mockBackend } from "../ipc/mock";
import { Store } from "./store";

const wait = (ms = 5) => new Promise((r) => setTimeout(r, ms));

async function ready() {
  const backend = mockBackend(0);
  const store = new Store(backend);
  await store.start();
  for (let i = 0; i < 200 && store.entries.some((e) => e.busy || !e.applied); i++) await wait();
  return { backend, store, player: store.player };
}

describe("Player", () => {
  it("plays the selected track as soon as it has loaded", async () => {
    const { backend, store, player } = await ready();
    store.playSelected();
    await wait(20);
    expect(backend.calls).toEqual(["load /music/Tideline.mp3", "metronome grid 384", "play"]);
    expect(player.playing).toBe(true);
    expect(player.key).toBe("/music/Tideline.mp3");
    expect(player.durationSecs).toBeCloseTo(174.55, 1);
  });

  it("toggles with Space, and starts the selected track if nothing is loaded", async () => {
    const { backend, store, player } = await ready();
    store.togglePlay();
    await wait(20);
    expect(player.playing).toBe(true);
    store.togglePlay();
    await wait();
    expect(player.playing).toBe(false);
    expect(backend.calls.at(-1)).toBe("pause");
  });

  it("keeps playing when the selection moves, until another is played", async () => {
    const { backend, store, player } = await ready();
    store.playSelected();
    await wait(20);
    store.moveSelection(1);
    expect(player.key).toBe("/music/Tideline.mp3");
    store.playSelected();
    await wait(20);
    expect(player.key).toBe("/music/Low Orbit.mp3");
    expect(backend.calls.filter((c) => c.startsWith("load"))).toHaveLength(2);
  });

  it("leans towards a tick that's a little off rather than jumping, and snaps to one that's far off", async () => {
    const { store, player, backend } = await ready();
    store.playSelected();
    await wait(30);
    const rate = 44100;
    backend.tickNow({ frames: 10 * rate, playing: true });
    let renders = 0;
    store.subscribe(() => (renders += 1));
    const before = player.position();
    // 40 ms ahead of where it has run to: jitter. It moves a fifth of the way.
    backend.tickNow({ frames: Math.round((before + 0.04) * rate), playing: true });
    expect(player.position() - before).toBeGreaterThan(0.004);
    expect(player.position() - before).toBeLessThan(0.02);
    // Jitter changes nothing on the page; the playhead is read each frame.
    expect(renders).toBe(0);
    // A seek: far off, so it snaps, and what keeps time with the playhead hears of it.
    const jumps = player.jumps;
    backend.tickNow({ frames: 60 * rate, playing: true });
    expect(player.position()).toBeCloseTo(60, 2);
    expect(player.jumps).toBe(jumps + 1);
    expect(renders).toBe(1);
  });

  it("starts the zoomed window at an overview click, and centres it for a click on the zoomed waveform", async () => {
    const { store, player } = await ready();
    store.playSelected();
    await wait(30);
    store.seekInView(60, 60);
    expect(player.lead).toEqual({ key: player.key, secs: 60 });
    // A click on the zoomed waveform, a cue pad or the keys move the playhead and centre the window.
    store.seekInView(70);
    expect(player.lead).toBeNull();
    store.seekInView(60, 60);
    await player.seekBy(1);
    expect(player.lead).toBeNull();
    // A track not yet loaded takes the lead with it.
    store.moveSelection(1);
    store.seekInView(20, 20);
    expect(player.lead).toEqual({ key: store.current?.key, secs: 20 });
  });

  it("seeks by beats on the track's own grid, stopping at the ends", async () => {
    const { store, player, backend } = await ready();
    store.playSelected();
    await wait(30);
    expect(player.track).not.toBeNull();
    backend.tickNow({ frames: 0, playing: false });
    const beat = 60 / 132;
    await player.seekBy(1);
    expect(player.position()).toBeCloseTo(beat, 3);
    await player.seekBy(4);
    expect(player.position()).toBeCloseTo(5 * beat, 3);
    await player.seekBy(-99);
    expect(player.position()).toBe(0);
    // A playhead between beats goes to the next one up, and back to its own.
    await player.seek(beat * 1.5);
    await player.seekBy(1);
    expect(player.position()).toBeCloseTo(2 * beat, 3);
  });

  it("jumps to hot cues in rekordbox, and a cue with an end starts its loop", async () => {
    const { store, player, backend } = await ready();
    await store.applyRulesToSelected();
    store.playSelected();
    await wait(30);
    backend.tickNow({ playing: false });
    await player.jumpToCue("c");
    const drop = player.entry?.applied?.cues.find((c) => c.hot === 2)?.start_secs ?? -1;
    expect(player.position()).toBeCloseTo(drop, 3);

    const entry = player.entry;
    const cue = entry?.applied?.cues.find((c) => c.hot === 3);
    if (cue) cue.end_secs = cue.start_secs + 8;
    await player.jumpToCue("d");
    expect(backend.calls.at(-1)).toContain("loop");
    expect(player.looping).toBe(true);

    await player.jumpToCue("h");
    expect(player.position()).toBeCloseTo(cue?.start_secs ?? -1, 3);
  });

  it("toggles a 16-beat loop from the beat under the playhead", async () => {
    const { store, player, backend } = await ready();
    store.playSelected();
    await wait(30);
    backend.tickNow({ frames: 0, playing: false });
    await player.toggleLoop();
    const beat = 60 / 132;
    expect(backend.calls.at(-1)).toBe(`loop 0.000 ${(16 * beat).toFixed(3)}`);
    await player.toggleLoop();
    expect(backend.calls.at(-1)).toBe("clear loop");
    expect(player.looping).toBe(false);
  });

  it("goes through the queue when a track ends, and stops after the last", async () => {
    const { store, player, backend } = await ready();
    store.playSelected();
    await wait(20);
    store.select("/music/Glasshouse.mp3");
    store.enqueueSelected();
    expect(player.queue).toHaveLength(1);
    backend.tickNow({ ended: true, playing: false });
    await wait(20);
    expect(player.key).toBe("/music/Glasshouse.mp3");
    expect(player.playing).toBe(true);
    expect(player.queue).toHaveLength(0);
    backend.tickNow({ ended: true, playing: false });
    await wait(20);
    expect(player.key).toBe("/music/Glasshouse.mp3");
    expect(player.playing).toBe(false);
  });

  it("clicking a waveform seeks, loading that track first if it isn't the one playing", async () => {
    const { store, player, backend } = await ready();
    store.seekInView(30);
    await wait(20);
    expect(player.key).toBe("/music/Tideline.mp3");
    expect(player.playing).toBe(false);
    expect(backend.calls).toEqual(["load /music/Tideline.mp3", "metronome grid 384", "seek 30.000"]);
    store.seekInView(45);
    await wait();
    expect(backend.calls.at(-1)).toBe("seek 45.000");
  });

  it("shows a file that can't be opened and stops", async () => {
    const { store, player } = await ready();
    const entry = store.entries[0];
    if (!entry) throw new Error("no entry");
    await player.load({ ...entry, key: "missing", path: "/music/missing.mp3" }, true);
    await wait(20);
    expect(player.error).toContain("can't open");
    expect(player.playing).toBe(false);
  });

  it("ignores ticks from the track it just replaced", async () => {
    const { store, player, backend } = await ready();
    store.playSelected();
    await wait(20);
    store.moveSelection(1);
    store.playSelected();
    await wait(20);
    const before = player.position();
    backend.tickNow({ load_id: 1, frames: 9999999 });
    expect(player.position()).toBeLessThan(before + 1);
  });

  it("zooms the deck between 2 and 64 bars around the playhead, from 16", async () => {
    const { store } = await ready();
    expect(store.zoomBars).toBe(16);
    store.zoomBy(2);
    expect(store.zoomBars).toBe(8);
    for (let i = 0; i < 9; i++) store.zoomBy(2);
    expect(store.zoomBars).toBe(2);
    for (let i = 0; i < 9; i++) store.zoomBy(0.5);
    expect(store.zoomBars).toBe(64);
  });
});
