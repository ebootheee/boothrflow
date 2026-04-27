import { vi } from "vitest";

/**
 * Helper for unit tests that need to stand in for `@tauri-apps/api/core`.
 *
 * Usage:
 *   import { mockTauriInvoke } from "$lib/test/tauri-mocks";
 *   mockTauriInvoke({ dictate_once: () => ({ raw: "x", formatted: "x", durationMs: 1 }) });
 */
export function mockTauriInvoke(handlers: Record<string, (args?: unknown) => unknown>) {
  vi.mock("@tauri-apps/api/core", () => ({
    invoke: vi.fn(async (cmd: string, args?: unknown) => {
      const handler = handlers[cmd];
      if (!handler) throw new Error(`Unmocked Tauri command: ${cmd}`);
      return handler(args);
    }),
  }));
}

/**
 * Pretend we're inside the Tauri webview so `isTauri()` returns true.
 * Call inside `beforeEach` and pair with `delete (window as any).__TAURI_INTERNALS__`
 * in `afterEach`.
 */
export function fakeTauriEnv() {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}

export function clearTauriEnv() {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
}
