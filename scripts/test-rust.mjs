#!/usr/bin/env node
// Runs every Rust test binary at once, then the doctests.
//
// `cargo test` runs its test binaries one after another. Each is parallel
// inside, but the slowest (the safe path's crash tests, about 14 s) set the
// floor only if the rest run beside it: one after another, the workspace's
// tests took 45 s; side by side, about 16 s (2026-11-04, M-series, warm).
// Each binary runs in its own package folder, as `cargo test` runs it.
// Arguments after the script go to every binary, such as a test name filter.
import { spawn, spawnSync } from "node:child_process";
import { dirname } from "node:path";

const passthrough = process.argv.slice(2);
const build = spawnSync("cargo", ["test", "--workspace", "--all-targets", "--no-run", "--message-format=json-render-diagnostics", "-q"], {
  encoding: "utf8",
  stdio: ["ignore", "pipe", "inherit"],
  maxBuffer: 256 * 1024 * 1024,
});
if (build.status !== 0) process.exit(build.status ?? 1);

const binaries = build.stdout
  .split("\n")
  .filter(Boolean)
  .map((line) => JSON.parse(line))
  .filter((m) => m.reason === "compiler-artifact" && m.executable && m.profile?.test)
  .map((m) => ({ path: m.executable, cwd: dirname(m.manifest_path), name: `${m.target.name} (${m.target.kind.join(",")})` }));

const run = ({ path, cwd, name }) =>
  new Promise((resolve) => {
    const child = spawn(path, ["-q", ...passthrough], { cwd, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    child.stdout.on("data", (chunk) => (output += chunk));
    child.stderr.on("data", (chunk) => (output += chunk));
    child.on("close", (code) => resolve({ name, code, output }));
  });

const started = Date.now();
const results = await Promise.all(binaries.map(run));
const failed = results.filter((r) => r.code !== 0);
for (const r of failed) console.error(`\n---- ${r.name} failed ----\n${r.output}`);
const passed = results.reduce((sum, r) => sum + Number(/(\d+) passed/.exec(r.output)?.[1] ?? 0), 0);
console.log(`${binaries.length} test binaries, ${passed} tests passed, ${failed.length} failed, ${((Date.now() - started) / 1000).toFixed(1)} s`);
if (failed.length) process.exit(1);

const docs = spawnSync("cargo", ["test", "--workspace", "--doc", "-q"], { stdio: "inherit" });
process.exit(docs.status ?? 1);
