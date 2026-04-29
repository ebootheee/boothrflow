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

export function isMacPlatform(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform);
}

export function dictationHotkeyLabel(): string {
  return isMacPlatform() ? "Ctrl + Cmd" : "Ctrl + Win";
}

export function quickPasteHotkeyLabel(): string {
  return isMacPlatform() ? "Option + Cmd + H" : "Alt + Win + H";
}
