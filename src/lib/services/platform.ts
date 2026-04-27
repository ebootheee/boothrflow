/**
 * Build-time detection: are we running inside the Tauri webview,
 * or in a plain browser (e.g. `vite dev` without `tauri dev`)?
 *
 * Used by service-layer factory functions to pick desktop vs web impls
 * without dragging Tauri imports into the web bundle.
 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
