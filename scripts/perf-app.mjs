#!/usr/bin/env node
// The perf run: the release app on a fixture library, its window on screen,
// playing on a silent sink so nothing is heard. The page plays a scenario in
// phases (src/app/perf.ts) and marks each over IPC; this samples every
// process's CPU and memory once a second, lines the samples up with the
// marks, and prints a table, and writes target/perf/report.json.
//
// Slow and it needs the display awake, so it isn't part of `pnpm check`:
// `pnpm perf` (about 15 minutes), `pnpm perf --quick` (the track-switching
// phase 2 minutes, not 10), `--no-build` to reuse the last release build,
// `--big` to time startup and a scroll on 5,000 tracks too. HOME is a
// temporary folder: the real library, settings and Keychain are never
// touched.
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
const quick = args.includes("--quick");
const leakSecs = quick ? 120 : 600;
const binary = "target/release/kfbox";

const asleep = execFileSync("system_profiler", ["SPDisplaysDataType"], { encoding: "utf8" }).includes("Display Asleep: Yes");
if (asleep) {
  console.error("kfbox perf: the display is asleep, so the window would draw nothing. Wake it and run again.");
  process.exit(2);
}

const run = (command, argv, options = {}) => {
  const result = spawnSync(command, argv, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], ...options });
  if (result.status !== 0) {
    console.error(`${command} ${argv.join(" ")} failed`);
    process.exit(result.status ?? 1);
  }
  return (result.stdout ?? "").trim();
};
if (!args.includes("--no-build")) run("pnpm", ["-s", "tauri", "build", "--no-bundle"], { stdio: "inherit" });

/** The WebKit helper processes running now: the app's are the ones that appear after it starts. */
const webkit = () =>
  new Set(
    execFileSync("ps", ["-A", "-o", "pid=,comm="], { encoding: "utf8" })
      .split("\n")
      .filter((line) => line.includes("com.apple.WebKit"))
      .map((line) => Number(line.trim().split(/\s+/)[0])),
  );

/** One second's CPU (percent of a core) and memory (MB) for each of `pids`, named by process. */
function sample(pids) {
  if (pids.length === 0) return {};
  let out = "";
  try {
    out = execFileSync("ps", ["-o", "pid=,pcpu=,rss=,comm=", "-p", pids.join(",")], { encoding: "utf8" });
  } catch {
    return {};
  }
  const rows = {};
  for (const line of out.split("\n").filter(Boolean)) {
    const [pid, cpu, rss, ...comm] = line.trim().split(/\s+/);
    const name = comm.join(" ").split("/").pop().replace("com.apple.WebKit.", "WebKit ");
    rows[`${name} ${pid}`] = { cpu: Number(cpu), mb: Number(rss) / 1024 };
  }
  return rows;
}

/** A process's memory by `footprint`: its total and its graphics (IOSurfaces, GPU buffers), in MB. */
function footprint(pid) {
  const result = spawnSync("footprint", ["-p", String(pid)], { encoding: "utf8", timeout: 15_000 });
  const text = result.stdout ?? "";
  const mb = (value, unit) => Number(value) * ({ B: 1 / 1048576, KB: 1 / 1024, MB: 1, GB: 1024 }[unit] ?? 0);
  const total = /Footprint: ([\d.]+) (B|KB|MB|GB)/.exec(text);
  let graphics = 0;
  for (const line of text.split("\n")) {
    const row = /^\s*([\d.]+) (B|KB|MB|GB)\s.*\(graphics\)/.exec(line);
    if (row) graphics += mb(row[1], row[2]);
  }
  return { total_mb: total ? round(mb(total[1], total[2])) : null, graphics_mb: round(graphics) };
}

/** Runs the app on `tracks` fixture tracks and returns the marks and the samples. */
async function boot(tracks, leak) {
  const scratch = mkdtempSync(join(tmpdir(), "kfbox-perf-"));
  const library = run("cargo", ["run", "-q", "-p", "kfbox-core", "--example", "fixture", "--", join(scratch, "fixture"), String(tracks)]);
  const before = webkit();
  const started = Date.now();
  const app = spawn(binary, [], {
    env: { ...process.env, HOME: join(scratch, "home"), KFBOX_FIXTURE: library, KFBOX_PERF: String(leak), KFBOX_SILENT: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const marks = [];
  const samples = [];
  let phase = "startup";
  let buffer = "";
  app.stdout.on("data", (chunk) => {
    buffer += chunk;
    let at;
    while ((at = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, at);
      buffer = buffer.slice(at + 1);
      if (!line.startsWith("kfbox perf: ")) continue;
      const mark = { ...JSON.parse(line.slice("kfbox perf: ".length)), at_ms: Date.now() - started };
      marks.push(mark);
      if (mark.name.endsWith(" start")) phase = mark.name.slice(0, -" start".length);
      else if (mark.name.endsWith(" end")) phase = "between";
      if (mark.name === "switch" || mark.name.endsWith(" end")) {
        // What the WebKit processes hold, by footprint: total and graphics.
        const ours = [...webkit()].filter((pid) => !before.has(pid));
        mark.footprint = Object.fromEntries(
          ours.map((pid) => [
            execFileSync("ps", ["-o", "comm=", "-p", String(pid)], { encoding: "utf8" })
              .trim()
              .split(".")
              .pop(),
            footprint(pid),
          ]),
        );
      }
      console.log(
        `  ${(mark.at_ms / 1000).toFixed(0)} s: ${mark.name} ${JSON.stringify(mark.data ?? {})} ${JSON.stringify(mark.footprint ?? {})}`,
      );
    }
  });
  app.stderr.on("data", (chunk) => process.stderr.write(chunk));
  const timer = setInterval(() => {
    const pids = [app.pid, ...[...webkit()].filter((pid) => !before.has(pid))];
    samples.push({ at_ms: Date.now() - started, phase, processes: sample(pids) });
  }, 1000);
  const limit = setTimeout(() => app.kill("SIGKILL"), (leak + 400) * 1000);
  await new Promise((resolve) => app.on("close", resolve));
  clearInterval(timer);
  clearTimeout(limit);
  rmSync(scratch, { recursive: true, force: true });
  return { tracks, marks, samples };
}

const round = (n, places = 1) => Math.round(n * 10 ** places) / 10 ** places;

/** Each phase's mean CPU per process, and its memory at the end. */
function byPhase({ samples }) {
  const phases = {};
  for (const { phase, processes } of samples) {
    if (phase === "between") continue;
    for (const [name, { cpu, mb }] of Object.entries(processes)) {
      const key = name.replace(/ \d+$/, "");
      const p = ((phases[phase] ??= {})[key] ??= { cpu: [], mb: [] });
      p.cpu.push(cpu);
      p.mb.push(mb);
    }
  }
  return phases;
}

function report(result) {
  const phases = byPhase(result);
  console.log(`\n## ${result.tracks} tracks\n`);
  const startup = result.marks.find((m) => m.name === "startup");
  if (startup) {
    console.log(
      `Startup: launch to page marks ${startup.at_ms} ms; in the page, rows drawn at ${startup.data.rows_ms} ms and the deck at ${startup.data.deck_ms} ms.\n`,
    );
  }
  console.log("| Phase | Process | CPU mean % | CPU max % | Memory start → end MB |");
  console.log("|---|---|---|---|---|");
  for (const [phase, processes] of Object.entries(phases)) {
    for (const [name, { cpu, mb }] of Object.entries(processes)) {
      const mean = cpu.reduce((a, b) => a + b, 0) / cpu.length;
      console.log(
        `| ${phase} | ${name} | ${round(mean)} | ${round(Math.max(...cpu))} | ${round(mb[0], 0)} → ${round(mb[mb.length - 1], 0)} |`,
      );
    }
  }
  console.log("\n| Mark | Page | Main thread (worst late ms, stalls > 50 ms) |");
  console.log("|---|---|---|");
  for (const mark of result.marks.filter((m) => m.name !== "switch")) {
    const main = mark.main_thread ? `${round(mark.main_thread.worst_ms)}, ${mark.main_thread.stalls_over_50ms}` : "";
    console.log(`| ${mark.name} | ${JSON.stringify(mark.data ?? {})} | ${main} |`);
  }
  const switches = result.marks.filter((m) => m.name === "switch");
  if (switches.length) {
    console.log(
      `\nWhile switching tracks, animations ${switches.map((s) => s.data.animations).join(" → ")}; intervals ${switches.map((s) => s.data.intervals).join(" → ")}.`,
    );
    for (const name of Object.keys(switches[0].footprint ?? {})) {
      const series = switches.map((s) => s.footprint?.[name]).filter(Boolean);
      console.log(`${name} footprint MB (graphics): ${series.map((f) => `${f.total_mb} (${f.graphics_mb})`).join(" → ")}`);
    }
  }
}

// --only-big skips the 120-track run, for a look at a large library alone.
const results = args.includes("--only-big") ? [] : [await boot(120, leakSecs)];
if (args.includes("--big") || args.includes("--only-big")) results.push(await boot(5000, 30));
mkdirSync("target/perf", { recursive: true });
writeFileSync("target/perf/report.json", JSON.stringify(results, null, 2));
for (const result of results) report(result);
console.log("\nThe whole run: target/perf/report.json");
