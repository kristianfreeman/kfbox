#!/usr/bin/env node
// Boots the real app (a debug build, with the page built in) against a
// fixture library, and checks the page mounts and works: the page reports
// over IPC and the app quits. A page that never mounts, or a shell whose
// main thread hangs, never reports, and this fails after a timeout.
//
// The window is hidden, and HOME is a temporary folder, so the run never
// touches the real rekordbox library, kfbox's settings or the Keychain's
// items. Slow (it builds the app), so it isn't part of `pnpm check`:
// `pnpm smoke`, or `pnpm smoke --no-build` to reuse the last build.
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const TIMEOUT_MS = 90_000;
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], ...options });
  if (result.status !== 0) {
    console.error(`${command} ${args.join(" ")} failed`);
    process.exit(result.status ?? 1);
  }
  return (result.stdout ?? "").trim();
};

if (!process.argv.includes("--no-build")) run("pnpm", ["-s", "tauri", "build", "--debug", "--no-bundle"], { stdio: "inherit" });

const scratch = mkdtempSync(join(tmpdir(), "kfbox-smoke-"));
const library = run("cargo", ["run", "-q", "-p", "kfbox-core", "--example", "fixture", "--", join(scratch, "fixture")]);
const home = join(scratch, "home");

const started = Date.now();
const app = spawn("target/debug/kfbox", [], {
  env: { ...process.env, HOME: home, KFBOX_SMOKE: "1", KFBOX_FIXTURE: library },
  stdio: ["ignore", "pipe", "pipe"],
});
let output = "";
app.stdout.on("data", (chunk) => (output += chunk));
app.stderr.on("data", (chunk) => (output += chunk));
const timer = setTimeout(() => {
  console.error(`kfbox smoke: no report from the page in ${TIMEOUT_MS / 1000} s: it didn't mount, or the app hung`);
  app.kill("SIGKILL");
}, TIMEOUT_MS);
app.on("close", (code) => {
  clearTimeout(timer);
  rmSync(scratch, { recursive: true, force: true });
  const report = output.split("\n").find((line) => line.startsWith("kfbox smoke:"));
  console.log(report ?? output.trim());
  console.log(`${((Date.now() - started) / 1000).toFixed(1)} s`);
  process.exit(code === 0 && report?.startsWith("kfbox smoke: ok") ? 0 : 1);
});
