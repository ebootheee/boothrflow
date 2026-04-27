import type { Style } from "$lib/services/styles";

type SettingsState = {
  style: Style;
  hotkey: string;
  llmEnabled: boolean;
  privacyMode: boolean;
};

function createSettings() {
  let state = $state<SettingsState>({
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
