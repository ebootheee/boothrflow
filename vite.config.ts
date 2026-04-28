import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],

  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
      $ipc: fileURLToPath(new URL("./src/lib/ipc", import.meta.url)),
    },
  },

  // Restrict Vite's dep-scan to our entry only — without this it walks the
  // working tree (including the gitignored `_spike/` reference clones) and
  // chokes on $lib aliases that don't resolve in our project.
  optimizeDeps: {
    entries: ["index.html"],
  },

  // Vite options tailored for Tauri development.
  // See https://v2.tauri.app/start/frontend/vite/
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    ...(host ? { hmr: { protocol: "ws" as const, host, port: 1421 } } : {}),
    watch: {
      // Don't watch src-tauri — Tauri does that itself.
      ignored: ["**/src-tauri/**", "**/_spike/**"],
    },
  },

  // Tauri uses Chromium on Win/Linux and WebKit on Mac/iOS.
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
