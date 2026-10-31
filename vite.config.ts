import { defineConfig } from "vite";

// Tauri expects a fixed port and loads the page from it in dev.
export default defineConfig({
  clearScreen: false,
  server: { port: 1430, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "safari16", outDir: "dist" },
});
