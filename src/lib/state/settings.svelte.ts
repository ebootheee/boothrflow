import type { Style } from "$lib/services/styles";
import {
  dictationHotkeyLabel,
  isTauri,
  quickPasteHotkeyLabel,
  toggleDictationHotkeyLabel,
} from "$lib/services/platform";

// Types re-exported from the auto-generated tauri-specta bindings so the
// FE consumes the same shapes the Rust commands return. Replaces the
// manual mirrors we maintained pre-Specta — adding a new field on the
// Rust side flows through `pnpm gen` → here → every consumer.
export type {
  AppSettings,
  AppStyleOverride,
  EmbedSettings,
  HotkeySettings,
  LlmSettings,
  ModelOption,
  SettingsOptions,
  SettingsPatch,
  WhisperSettings,
} from "$lib/ipc/bindings";
import type {
  AppSettings,
  SettingsOptions,
  SettingsPatch,
  WhisperDownloadResult,
} from "$lib/ipc/bindings";

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
  commonly_misheard: [],
  cleanup_window_ocr: false,
  auto_learn_corrections: false,
};

const defaultOptions: SettingsOptions = {
  whisper_models: [
    {
      value: "tiny.en",
      label: "Whisper tiny.en — live preview (39M, 75MB)",
      detail:
        "Live transcript appears as you talk. Fastest, lowest accuracy of the Whisper variants.",
      file: "ggml-tiny.en.bin",
      available: true,
    },
    {
      value: "base.en",
      label: "Whisper base.en — live preview (74M, 142MB)",
      detail: "Live transcript appears as you talk. Quick, noticeably cleaner than tiny.",
      file: "ggml-base.en.bin",
      available: true,
    },
    {
      value: "small.en",
      label: "Whisper small.en — live preview (244M, 466MB)",
      detail:
        "Live transcript appears as you talk. Recommended Whisper balance of quality and speed.",
      file: "ggml-small.en.bin",
      available: true,
    },
    {
      value: "medium.en",
      label: "Whisper medium.en — live preview (769M, 1.5GB)",
      detail: "Live transcript appears as you talk. Better accuracy, higher latency.",
      file: "ggml-medium.en.bin",
      available: true,
    },
    {
      value: "large-v3-turbo",
      label: "Whisper large-v3-turbo — live preview (809M, 1.6GB)",
      detail:
        "Live transcript appears as you talk. Highest-quality Whisper variant; best for M-series Macs.",
      file: "ggml-large-v3-turbo.bin",
      available: true,
    },
    {
      value: "parakeet-tdt-0.6b-v3",
      label: "NVIDIA Parakeet TDT 0.6B — final transcript only (preview)",
      detail:
        "Highest accuracy on technical jargon (Qwen, OpenAI, file paths, etc). No live preview while talking — transcript appears on release. English only.",
      file: "parakeet-tdt-0.6b-v3",
      available: false,
    },
  ],
  llm_models: [
    {
      value: "qwen2.5:7b",
      label: "Qwen 2.5 7B Instruct (~5GB, ~80 tok/s on M4)",
      detail: "Higher-quality local cleanup default.",
      file: null,
      available: true,
    },
    {
      value: "qwen2.5:1.5b",
      label: "Qwen 2.5 1.5B Instruct (~1GB, faster)",
      detail: "Lower-latency fallback for slower machines.",
      file: null,
      available: true,
    },
  ],
  embed_models: [
    {
      value: "nomic-embed-text",
      label: "nomic-embed-text v1.5 (137M, 274MB)",
      detail: "Default local embedding model for history search.",
      file: null,
      available: true,
    },
  ],
};

function applyPatch(current: AppSettings, patch: SettingsPatch): AppSettings {
  // Field-by-field merge rather than `{...current, ...patch}` because the
  // generated `SettingsPatch` types every field as `T | null` (the wire
  // shape — Rust `Option<T>`). Spread would clobber e.g. `style` with
  // `null` when the patch only set `vocabulary`.
  return {
    schema_version: current.schema_version,
    style: patch.style ?? current.style,
    privacy_mode: patch.privacy_mode ?? current.privacy_mode,
    whisper: patch.whisper ?? current.whisper,
    llm: patch.llm ?? current.llm,
    embed: patch.embed ?? current.embed,
    hotkeys: patch.hotkeys ?? current.hotkeys,
    vocabulary: patch.vocabulary ?? current.vocabulary,
    per_app_styles: patch.per_app_styles ?? current.per_app_styles,
    commonly_misheard: patch.commonly_misheard ?? current.commonly_misheard ?? [],
    cleanup_window_ocr: patch.cleanup_window_ocr ?? current.cleanup_window_ocr ?? false,
    auto_learn_corrections: patch.auto_learn_corrections ?? current.auto_learn_corrections ?? false,
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

  /// Trigger the macOS Screen Recording permission prompt and return
  /// whether access is currently granted. No-op on non-macOS. Also
  /// has the side-effect of registering the app in System Settings →
  /// Privacy & Security → Screen Recording (without this call, the
  /// app doesn't appear in that list at all).
  async function requestScreenRecordingPermission(): Promise<boolean> {
    if (!isTauri()) return true;
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      return await invoke<boolean>("request_screen_recording_permission");
    } catch {
      return false;
    }
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
    requestScreenRecordingPermission,
    getAppVersion,
    revealPath,
    getAutostartEnabled,
    setAutostartEnabled,
  };
}

export const settings = createSettings();
