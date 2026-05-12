import { defineConfig } from "vite";

// Vite served behind Tauri. Fixed dev port so tauri.conf.json can
// point at it deterministically. HMR works through the Tauri
// `withGlobalTauri` bridge.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Don't reload when the Rust source changes — Tauri's own
      // dev process handles that.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
    rollupOptions: {
      input: {
        main: "./index.html",
      },
    },
  },
});
