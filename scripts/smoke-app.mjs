#!/usr/bin/env node
// Boots the real app (a debug build, with the page built in) against a
// fixture library, and checks the page mounts and works: the page reports
// over IPC and the app quits. A page that never mounts, or a shell whose
// main thread hangs, never reports, and this fails after a timeout.
//
// The window is hidden, HOME is a temporary folder and the API key store
// is a stand-in, so the run never touches the real rekordbox library,
// kfbox's settings or the Keychain. A second run makes the key store as slow
// as a Keychain prompt nobody answers, and the window must still come up. Slow (it builds the app), so it isn't part of `pnpm check`:
// `pnpm smoke`, `pnpm smoke --no-build` to reuse the last build, or
// `pnpm smoke --release` for the signed bundle.
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

// --release checks the signed bundle `pnpm app` made (hardened runtime and all) instead of a debug build.
const release = process.argv.includes("--release");
const binary = release ? "target/release/bundle/macos/kfbox.app/Contents/MacOS/kfbox" : "target/debug/kfbox";
if (!release && !process.argv.includes("--no-build")) run("pnpm", ["-s", "tauri", "build", "--debug", "--no-bundle"], { stdio: "inherit" });

/** Boots the app once and waits for the page's report, within `limitMs`. */
function boot(name, extraEnv, limitMs) {
  const scratch = mkdtempSync(join(tmpdir(), "kfbox-smoke-"));
  const library = run("cargo", ["run", "-q", "-p", "kfbox-core", "--example", "fixture", "--", join(scratch, "fixture")]);
  const started = Date.now();
  const app = spawn(binary, [], {
    env: { ...process.env, HOME: join(scratch, "home"), KFBOX_SMOKE: "1", KFBOX_FIXTURE: library, ...extraEnv },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  app.stdout.on("data", (chunk) => (output += chunk));
  app.stderr.on("data", (chunk) => (output += chunk));
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      output += `kfbox smoke: no report from the page in ${limitMs / 1000} s: it didn't mount, or the app hung\n`;
      app.kill("SIGKILL");
    }, limitMs);
    app.on("close", (code) => {
      clearTimeout(timer);
      rmSync(scratch, { recursive: true, force: true });
      const report = output.split("\n").filter((line) => line.startsWith("kfbox smoke:") || line.startsWith("kfbox page error")).join("\n");
      const ok = code === 0 && report.startsWith("kfbox smoke: ok");
      console.log(`${name}: ${report || output.trim()} (${((Date.now() - started) / 1000).toFixed(1)} s)`);
      resolve(ok);
    });
  });
}

const results = [
  await boot("fixture library", {}, TIMEOUT_MS),
  // A Keychain prompt nobody answers: the key store takes 30 s, and the page
  // must still come up and report well before then.
  await boot("Keychain waiting 30 s", { KFBOX_SMOKE_SLOW_KEYS: "30" }, 20_000),
];
process.exit(results.every(Boolean) ? 0 : 1);
