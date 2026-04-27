import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
      $ipc: fileURLToPath(new URL("./src/lib/ipc", import.meta.url)),
    },
    // Svelte 5 + Vitest browser mode requires browser conditions
    // when running component tests; node project uses default.
    conditions: ["browser"],
  },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          environment: "node",
          include: ["src/**/*.test.ts"],
          exclude: ["src/**/*.svelte.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "component",
          include: ["src/**/*.svelte.test.ts"],
          browser: {
            enabled: true,
            provider: "playwright",
            instances: [{ browser: "chromium" }],
            headless: true,
          },
        },
      },
    ],
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov"],
      include: ["src/**/*.{ts,svelte}"],
      exclude: ["src/**/*.test.ts", "src/**/*.svelte.test.ts", "src/lib/ipc/**"],
    },
  },
});
