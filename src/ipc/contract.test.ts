import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const files = (dir: string): string[] =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });

describe("the backend boundary", () => {
  it("only calls commands the Tauri shell registers", () => {
    const shell = readFileSync("src-tauri/src/lib.rs", "utf8");
    const handler = /generate_handler!\[([^\]]*)\]/.exec(shell)?.[1] ?? "";
    const registered = handler
      .split(",")
      .map((s) => s.trim().split("::").pop() ?? "")
      .filter(Boolean);
    const called = [...readFileSync("src/ipc/tauri.ts", "utf8").matchAll(/invoke<[^(]*>\("([a-z_]+)"/g)].map((m) => m[1]);
    expect(called.length).toBeGreaterThan(0);
    expect(called.filter((name) => !registered.includes(name ?? ""))).toEqual([]);
  });

  it("keeps Tauri imports inside src/ipc", () => {
    const outside = files("src").filter((f) => /\.tsx?$/.test(f) && !f.startsWith("src/ipc/"));
    expect(outside.filter((f) => readFileSync(f, "utf8").includes("@tauri-apps"))).toEqual([]);
  });
});
