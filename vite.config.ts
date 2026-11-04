import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const root = fileURLToPath(new URL(".", import.meta.url));
// Build output and other checkouts reload the page if watched. Matched under
// this checkout only: a worktree's own path has .worktrees in it.
const skipped = ["src-tauri/", ".worktrees/", "target/"].map((dir) => root + dir);

// Tauri expects a fixed port and loads the page from it in dev.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1430, strictPort: true, watch: { ignored: (path: string) => skipped.some((dir) => path.startsWith(dir)) } },
  build: { target: "safari16", outDir: "dist" },
  test: { environment: "jsdom", include: ["src/**/*.test.{ts,tsx}"], setupFiles: ["src/test-setup.ts"] },
});
