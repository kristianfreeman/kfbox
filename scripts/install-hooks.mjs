// Points this checkout's pre-commit hook at the one in the repository, so
// every commit runs `pnpm check`. Also covers worktrees, which share the hooks.
import { execSync } from "node:child_process";
import { chmodSync, existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";

let hooks;
try {
  hooks = execSync("git rev-parse --git-path hooks", { encoding: "utf8" }).trim();
} catch {
  process.exit(0); // not a git checkout, such as an unpacked archive
}
if (existsSync(hooks)) {
  const hook = join(hooks, "pre-commit");
  writeFileSync(hook, '#!/bin/sh\nexec "$(git rev-parse --show-toplevel)/.githooks/pre-commit" "$@"\n');
  chmodSync(hook, 0o755);
}
