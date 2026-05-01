import type { Style } from "$lib/services/styles";
import {
  dictationHotkeyLabel,
  isTauri,
  quickPasteHotkeyLabel,
  toggleDictationHotkeyLabel,
} from "$lib/services/platform";

export type WhisperSettings = {
  model: string;
};

export type LlmSettings = {
  enabled: boolean;
  endpoint: string;
  model: string;
  api_key: string | null;
};

export type EmbedSettings = {
  enabled: boolean;
  endpoint: string;
  model: string;
  api_key: string | null;
};

export type HotkeySettings = {
  ptt: string;
  toggle: string;
  quick_paste: string;
};

export type AppStyleOverride = {
  app_id: string;
  style: Style;
};

export type AppSettings = {
  schema_version: number;
  style: Style;
  privacy_mode: boolean;
  whisper: WhisperSettings;
  llm: LlmSettings;
  embed: EmbedSettings;
  hotkeys: HotkeySettings;
  vocabulary: string;
  per_app_styles: AppStyleOverride[];
};

export type SettingsPatch = Partial<
  Pick<
    AppSettings,
    | "style"
    | "privacy_mode"
    | "whisper"
    | "llm"
    | "embed"
    | "hotkeys"
    | "vocabulary"
    | "per_app_styles"
  >
>;

export type ModelOption = {
  value: string;
  label: string;
  detail: string;
  file: string | null;
};

export type SettingsOptions = {
  whisper_models: ModelOption[];
  llm_models: ModelOption[];
  embed_models: ModelOption[];
};

type WhisperDownloadResult = {
  model: string;
  file: string;
  path: string;
  already_present: boolean;
};

const defaultSettings: AppSettings = {
  schema_version: 1,
  style: "casual",
  privacy_mode: false,
  whisper: { model: "tiny.en" },
  llm: {
    enabled: true,
    endpoint: "http://localhost:11434/v1/chat/completions",
    model: "qwen2.5:7b",
    api_key: null,
  },
  embed: {
    enabled: true,
    endpoint: "http://localhost:11434/v1/embeddings",
    model: "nomic-embed-text",
    api_key: null,
  },
  hotkeys: {
    ptt: dictationHotkeyLabel(),
    toggle: toggleDictationHotkeyLabel(),
    quick_paste: quickPasteHotkeyLabel(),
  },
  vocabulary: "",
  per_app_styles: [],
};

const defaultOptions: SettingsOptions = {
  whisper_models: [
    {
      value: "tiny.en",
      label: "Whisper tiny.en (39M, 75MB)",
      detail: "Fastest, lowest accuracy.",
      file: "ggml-tiny.en.bin",
    },
    {
      value: "base.en",
      label: "Whisper base.en (74M, 142MB)",
      detail: "Still quick, noticeably cleaner than tiny.",
      file: "ggml-base.en.bin",
    },
    {
      value: "small.en",
      label: "Whisper small.en (244M, 466MB)",
      detail: "Recommended quality/speed balance.",
      file: "ggml-small.en.bin",
    },
    {
      value: "medium.en",
      label: "Whisper medium.en (769M, 1.5GB)",
      detail: "Better accuracy, higher latency.",
      file: "ggml-medium.en.bin",
    },
    {
      value: "large-v3-turbo",
      label: "Whisper large-v3-turbo (809M, 1.6GB)",
      detail: "Best local quality option for strong Macs.",
      file: "ggml-large-v3-turbo.bin",
    },
  ],
  llm_models: [
    {
      value: "qwen2.5:7b",
      label: "Qwen 2.5 7B Instruct (~5GB, ~80 tok/s on M4)",
      detail: "Higher-quality local cleanup default.",
      file: null,
    },
    {
      value: "qwen2.5:1.5b",
      label: "Qwen 2.5 1.5B Instruct (~1GB, faster)",
      detail: "Lower-latency fallback for slower machines.",
      file: null,
    },
  ],
  embed_models: [
    {
      value: "nomic-embed-text",
      label: "nomic-embed-text v1.5 (137M, 274MB)",
      detail: "Default local embedding model for history search.",
      file: null,
    },
  ],
};

function applyPatch(current: AppSettings, patch: SettingsPatch): AppSettings {
  return {
    ...current,
    ...patch,
    whisper: patch.whisper ?? current.whisper,
    llm: patch.llm ?? current.llm,
    embed: patch.embed ?? current.embed,
    hotkeys: patch.hotkeys ?? current.hotkeys,
    per_app_styles: patch.per_app_styles ?? current.per_app_styles,
  };
}

function createSettings() {
  const state = $state({
    current: defaultSettings,
    options: defaultOptions,
    loaded: false,
    saving: false,
    error: null as string | null,
    downloadStatus: null as string | null,
  });

  async function load() {
    if (!isTauri()) {
      state.loaded = true;
      return state.current;
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const [current, options] = await Promise.all([
        invoke<AppSettings>("settings_get"),
        invoke<SettingsOptions>("settings_options"),
      ]);
      state.current = current;
      state.options = options;
      state.error = null;
    } catch (err) {
      state.error = String(err);
    } finally {
      state.loaded = true;
    }
    return state.current;
  }

  async function update(patch: SettingsPatch) {
    state.current = applyPatch(state.current, patch);
    if (!isTauri()) return state.current;

    state.saving = true;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      state.current = await invoke<AppSettings>("settings_update", { patch });
      state.error = null;
    } catch (err) {
      state.error = String(err);
    } finally {
      state.saving = false;
    }
    return state.current;
  }

  async function setWhisperModel(model: string) {
    await update({ whisper: { ...state.current.whisper, model } });
    if (!isTauri()) return;

    state.downloadStatus = "Checking Whisper model";
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<WhisperDownloadResult>("whisper_download_model", { model });
      state.downloadStatus = result.already_present
        ? `${result.file} ready`
        : `${result.file} downloaded`;
      state.error = null;
    } catch (err) {
      state.downloadStatus = null;
      state.error = String(err);
    }
  }

  async function exportJson() {
    if (!isTauri()) return JSON.stringify(state.current, null, 2);
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("settings_export");
  }

  async function importJson(json: string) {
    if (!isTauri()) {
      state.current = JSON.parse(json) as AppSettings;
      return state.current;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    state.current = await invoke<AppSettings>("settings_import", { json });
    return state.current;
  }

  /** Probe the configured LLM endpoint with a 1-token request. */
  async function testLlmConnection(): Promise<{
    ok: boolean;
    latency_ms: number;
    error: string | null;
  }> {
    if (!isTauri()) {
      return { ok: true, latency_ms: 0, error: null };
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke("llm_test_connection");
  }

  async function getAppVersion(): Promise<string> {
    if (!isTauri()) return "0.0.0-web";
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("app_version");
  }

  async function revealPath(path: string): Promise<void> {
    if (!isTauri()) return;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("reveal_path", { path });
  }

  /**
   * Autostart toggle wraps `tauri-plugin-autostart`. The plugin exposes
   * its own JS module; we call it through `invoke` against the plugin's
   * internal command name to avoid a separate import in the bundle.
   */
  async function getAutostartEnabled(): Promise<boolean> {
    if (!isTauri()) return false;
    try {
      const { isEnabled } = await import("@tauri-apps/plugin-autostart");
      return await isEnabled();
    } catch {
      return false;
    }
  }

  async function setAutostartEnabled(enabled: boolean): Promise<void> {
    if (!isTauri()) return;
    const mod = await import("@tauri-apps/plugin-autostart");
    if (enabled) await mod.enable();
    else await mod.disable();
  }

  return {
    get current() {
      return state.current;
    },
    get options() {
      return state.options;
    },
    get loaded() {
      return state.loaded;
    },
    get saving() {
      return state.saving;
    },
    get error() {
      return state.error;
    },
    get downloadStatus() {
      return state.downloadStatus;
    },
    get style() {
      return state.current.style;
    },
    set style(value: Style) {
      void update({ style: value });
    },
    get hotkey() {
      return state.current.hotkeys.ptt;
    },
    set hotkey(value: string) {
      void update({ hotkeys: { ...state.current.hotkeys, ptt: value } });
    },
    get llmEnabled() {
      return state.current.llm.enabled;
    },
    set llmEnabled(value: boolean) {
      void update({ llm: { ...state.current.llm, enabled: value } });
    },
    get privacyMode() {
      return state.current.privacy_mode;
    },
    set privacyMode(value: boolean) {
      void update({ privacy_mode: value });
    },
    load,
    update,
    setWhisperModel,
    exportJson,
    importJson,
    testLlmConnection,
    getAppVersion,
    revealPath,
    getAutostartEnabled,
    setAutostartEnabled,
  };
}

export const settings = createSettings();
