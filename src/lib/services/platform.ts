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
  // `navigator.platform` is deprecated in modern WebKit and may
  // return an empty string in some Tauri WKWebView contexts. Fall
  // back to `userAgent`, which reliably contains "Macintosh" on
  // every Mac browser engine. Also accept the modern userAgentData
  // when present.
  if (typeof navigator === "undefined") return false;
  const platform = navigator.platform ?? "";
  if (/Mac|iPhone|iPad|iPod/.test(platform)) return true;
  const ua = navigator.userAgent ?? "";
  if (/Macintosh|Mac OS X/i.test(ua)) return true;
  type UserAgentData = { platform?: string };
  const uaData = (navigator as unknown as { userAgentData?: UserAgentData }).userAgentData;
  if (uaData?.platform && /macOS|Mac/.test(uaData.platform)) return true;
  return false;
}

export function dictationHotkeyLabel(): string {
  return isMacPlatform() ? "Ctrl + Cmd" : "Ctrl + Win";
}

export function quickPasteHotkeyLabel(): string {
  return isMacPlatform() ? "Option + Cmd + H" : "Alt + Win + H";
}

/**
 * Tap-to-toggle dictation. Different modifier set from the hold-PTT chord
 * (Ctrl + Alt instead of Ctrl + Meta) so the rising edges don't collide
 * — user can tap to start a hands-free session that lasts as long as
 * they want, then tap again to end.
 */
export function toggleDictationHotkeyLabel(): string {
  return isMacPlatform() ? "Ctrl + Option + Space" : "Ctrl + Alt + Space";
}
