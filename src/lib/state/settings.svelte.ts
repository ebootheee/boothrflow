import type { Style } from "$lib/services/styles";
import { isTauri } from "$lib/services/platform";

type SettingsState = {
  style: Style;
  hotkey: string;
  llmEnabled: boolean;
  privacyMode: boolean;
};

/**
 * Push the current style to the Rust session daemon. Fire-and-forget; the
 * daemon updates an atomic so the next dictation uses the new style.
 */
async function pushStyleToBackend(style: Style) {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_dictation_style", { style });
  } catch (err) {
    console.warn("set_dictation_style failed:", err);
  }
}

function createSettings() {
  const state = $state<SettingsState>({
    style: "casual",
    hotkey: "Ctrl+Win",
    llmEnabled: true,
    privacyMode: false,
  });

  return {
    get style() {
      return state.style;
    },
    set style(value: Style) {
      state.style = value;
      void pushStyleToBackend(value);
    },
    get hotkey() {
      return state.hotkey;
    },
    set hotkey(value: string) {
      state.hotkey = value;
    },
    get llmEnabled() {
      return state.llmEnabled;
    },
    set llmEnabled(value: boolean) {
      state.llmEnabled = value;
    },
    get privacyMode() {
      return state.privacyMode;
    },
    set privacyMode(value: boolean) {
      state.privacyMode = value;
    },
  };
}

export const settings = createSettings();
