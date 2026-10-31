import type { Backend } from "../ipc/backend";
import type { Cue, PlayerTick, Track } from "../ipc/types";
import type { Entry } from "./store";

/** How long a loop set from the keyboard is. */
const LOOP_BEATS = 16;
/** The last known position may be this stale before the playhead stops running on. */
const EXTRAPOLATE_SECS = 1;

/** The player: what's loaded, where it is, and what comes next. */
export class Player {
  /** The loaded entry's key, while it's loaded or loading. */
  key: string | null = null;
  entry: Entry | null = null;
  /** The whole of the loaded track once it has been analysed, for its grid. */
  track: Track | null = null;
  playing = false;
  looping = false;
  durationSecs = 0;
  error = "";
  queue: Entry[] = [];

  private loadId = 0;
  private anchorSecs = 0;
  private anchorAt = 0;
  private rate = 44100;
  // What to do as soon as the file is ready.
  private pending: { play: boolean; seek: number | null } | null = null;
  private wasEnded = false;

  constructor(
    private backend: Backend,
    private changed: () => void,
  ) {
    backend.onPlayerTick((tick) => this.onTick(tick));
    backend.onPlayerError(({ load_id, message }) => {
      if (load_id !== this.loadId) return;
      this.error = message;
      this.playing = false;
      this.pending = null;
      this.changed();
    });
  }

  /** The playhead in seconds, run on from the last tick while playing. */
  position(): number {
    if (!this.playing) return this.anchorSecs;
    const run = Math.min((performance.now() - this.anchorAt) / 1000, EXTRAPOLATE_SECS);
    return Math.min(this.anchorSecs + run, this.durationSecs || Infinity);
  }

  private place(secs: number) {
    this.anchorSecs = secs;
    this.anchorAt = performance.now();
  }

  private onTick(tick: PlayerTick) {
    if (tick.load_id !== this.loadId) return;
    if (tick.sample_rate) this.rate = tick.sample_rate;
    const secs = tick.frames / this.rate;
    this.place(secs);
    this.playing = tick.playing;
    this.looping = tick.looping;
    if (tick.total_frames) this.durationSecs = tick.total_frames / this.rate;
    if (tick.loaded && this.pending) {
      const { play, seek } = this.pending;
      this.pending = null;
      if (seek !== null) void this.backend.playerSeek(seek);
      if (play) void this.backend.playerPlay();
    }
    const ended = tick.ended && !this.wasEnded;
    this.wasEnded = tick.ended;
    this.changed();
    if (ended) void this.next();
  }

  /** Loads an entry, and plays it or moves to `seek` as soon as it's ready. */
  async load(entry: Entry, play: boolean, seek: number | null = null) {
    this.key = entry.key;
    this.entry = entry;
    this.track = null;
    this.error = "";
    this.playing = false;
    this.looping = false;
    this.wasEnded = false;
    this.durationSecs = entry.row.duration_secs;
    this.place(seek ?? 0);
    this.pending = { play, seek };
    this.changed();
    try {
      this.loadId = await this.backend.playerLoad(entry.path);
    } catch (e) {
      this.error = String(e);
      this.pending = null;
      this.changed();
      return;
    }
    await this.fetchTrack(entry);
  }

  private async fetchTrack(entry: Entry) {
    if (!entry.analysis || this.key !== entry.key || this.track) return;
    try {
      const track = await this.backend.trackDetail(entry.path);
      if (this.key === entry.key) {
        this.track = track;
        this.changed();
      }
    } catch {
      // A track from another list isn't kept: the row's BPM stands in for its grid.
    }
  }

  /** The store calls this when an entry's analysis or cues arrive. */
  entryChanged(entry: Entry) {
    if (entry.key === this.key) {
      this.entry = entry;
      void this.fetchTrack(entry);
      this.changed();
    }
  }

  async toggle() {
    if (!this.key) return;
    if (this.playing) await this.backend.playerPause();
    else await this.backend.playerPlay();
  }

  async seek(secs: number) {
    if (!this.key) return;
    const clamped = Math.max(0, Math.min(secs, this.durationSecs || secs));
    this.place(clamped);
    this.changed();
    await this.backend.playerSeek(clamped);
  }

  /** The beat `beats` from the playhead, snapped to the grid. */
  beatFrom(secs: number, beats: number): number {
    const grid = this.track?.beats;
    if (grid?.length) {
      // The nearest beat at or before the playhead, within a hair.
      let index = 0;
      for (let i = 0; i < grid.length; i++) {
        if ((grid[i]?.secs ?? 0) <= secs + 0.01) index = i;
        else break;
      }
      const target = Math.max(0, Math.min(grid.length - 1, index + beats));
      return grid[target]?.secs ?? secs;
    }
    const bpm = this.entry?.row.bpm || 120;
    return secs + (beats * 60) / bpm;
  }

  async seekBy(beats: number) {
    if (this.key) await this.seek(this.beatFrom(this.position(), beats));
  }

  private cue(letter: string): Cue | undefined {
    const hot = letter.toLowerCase().charCodeAt(0) - 97;
    return this.entry?.applied?.cues.find((c) => c.hot === hot);
  }

  get hotCues(): Cue[] {
    return (this.entry?.applied?.cues ?? []).filter((c) => c.hot !== null).sort((a, b) => (a.hot ?? 0) - (b.hot ?? 0));
  }

  async jumpToCue(letter: string) {
    const cue = this.cue(letter);
    if (!cue) return;
    await this.seek(cue.start_secs);
    if (cue.end_secs !== null) await this.backend.playerSetLoop(cue.start_secs, cue.end_secs);
  }

  async toggleLoop() {
    if (!this.key) return;
    if (this.looping) {
      await this.backend.playerClearLoop();
      return;
    }
    const from = this.beatFrom(this.position(), 0);
    await this.backend.playerSetLoop(from, this.beatFrom(from, LOOP_BEATS));
  }

  enqueue(entry: Entry) {
    if (!this.key) {
      void this.load(entry, true);
      return;
    }
    this.queue.push(entry);
    this.changed();
  }

  /** Goes on to the next queued track, or stops if there's none. */
  async next() {
    const next = this.queue.shift();
    if (next) await this.load(next, true);
    else this.changed();
  }

  /** Back to the start of the track. */
  async restart() {
    await this.seek(0);
  }

  async scrubBegin() {
    await this.backend.playerScrubBegin();
  }

  async scrubTo(secs: number) {
    this.place(secs);
    await this.backend.playerScrubTo(secs);
  }

  async scrubEnd() {
    await this.backend.playerScrubEnd();
  }
}
