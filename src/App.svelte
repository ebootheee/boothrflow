<script lang="ts">
  import { onMount } from "svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import ListenPill from "$lib/components/ListenPill.svelte";
  import {
    dictationHotkeyLabel,
    isMacPlatform,
    isTauri,
    quickPasteHotkeyLabel,
    toggleDictationHotkeyLabel,
  } from "$lib/services/platform";
  import type { Style } from "$lib/services/styles";
  import { dictationStore } from "$lib/state/dictation.svelte";
  import {
    settings,
    type EmbedSettings,
    type HotkeySettings,
    type LlmSettings,
    type ModelOption,
  } from "$lib/state/settings.svelte";

  type HistoryEntry = {
    id: number;
    captured_at: string;
    raw: string;
    formatted: string;
    style: Style;
    app_exe: string | null;
    window_title: string | null;
    duration_ms: number;
    llm_ms: number;
    has_embedding: boolean;
  };

  type HistoryStats = {
    total_entries: number;
    embedded_entries: number;
    db_path: string;
    embed_endpoint: string | null;
    embed_model: string | null;
  };

  const hash = typeof window !== "undefined" ? window.location.hash : "";
  const isPill = hash === "#listen-pill";
  const isQuickPaste = hash === "#quick-paste";
  const inDesktop = isTauri();
  const defaultDictationHotkey = dictationHotkeyLabel();
  const defaultQuickPasteHotkey = quickPasteHotkeyLabel();
  const defaultToggleDictationHotkey = toggleDictationHotkeyLabel();
  const dictationHotkey = $derived(settings.current.hotkeys.ptt || defaultDictationHotkey);
  const quickPasteHotkey = $derived(
    settings.current.hotkeys.quick_paste || defaultQuickPasteHotkey,
  );
  const toggleDictationHotkey = $derived(
    settings.current.hotkeys.toggle || defaultToggleDictationHotkey,
  );

  // The structure-aggressiveness picker — single axis from "leave my
  // words alone" (Raw) to "fully restructure into a memo" (Assertive).
  // Replaced the old tone-based picker (casual / formal / very-casual /
  // excited) in Wave 6. See `docs/waves/wave-6-engine-and-formatting.md`
  // Phase 0. Captain's Log stays available below as a "fun preset" — it
  // doesn't fit the structure axis (it's a tone gimmick).
  const styleOptions: Array<{
    value: Style;
    label: string;
    icon: IconName;
    detail: string;
  }> = [
    {
      value: "raw",
      label: "Raw",
      icon: "audio",
      detail: "No cleanup. Paste verbatim — useful for code dictation or exact-quote capture.",
    },
    {
      value: "light",
      label: "Light",
      icon: "pen",
      detail:
        "Grammar + light punctuation; paragraph kept as-is. Best for short utterances and Slack messages.",
    },
    {
      value: "moderate",
      label: "Moderate",
      icon: "book",
      detail:
        "Light cleanup plus paragraph splits at natural breaks; removes filler words and false starts.",
    },
    {
      value: "assertive",
      label: "Assertive",
      icon: "sparkles",
      detail:
        "LLM has full freedom: bullets when listing, paragraph breaks, code fences, greeting + sign-off in Mail context.",
    },
    {
      value: "captains-log",
      label: "Captain's Log",
      icon: "radio",
      detail: "Star-Trek-style log entry. Stardate prefix + 24th-century rewrite.",
    },
  ];

  // Used by the segmented control in the General settings tab. Captain's
  // Log is shown separately as a "fun preset" below the structure picker.
  const structureStyleOptions = $derived(styleOptions.filter((o) => o.value !== "captains-log"));

  const demoNow = new Date("2026-04-28T15:42:00-06:00").toISOString();
  const demoHistory: HistoryEntry[] = [
    {
      id: 3,
      captured_at: demoNow,
      raw: "okay wow that was pretty impressive let's see if I speak a little faster",
      formatted:
        "Okay wow, that was pretty impressive. Let's see if I speak a little faster, if this still lands cleanly. The cleanup pass kept my tone, tightened the sentence breaks, and pasted it without making me babysit the output.",
      style: "light",
      app_exe: "Notepad.exe",
      window_title: "Notes",
      duration_ms: 7854,
      llm_ms: 509,
      has_embedding: true,
    },
    {
      id: 2,
      captured_at: new Date("2026-04-28T13:08:00-06:00").toISOString(),
      raw: "add connor sophie and max to the dictionary",
      formatted:
        "Add Connor, Sophie, and Max to the dictionary so their names stop getting corrected.",
      style: "moderate",
      app_exe: "Code.exe",
      window_title: "boothrflow",
      duration_ms: 811,
      llm_ms: 164,
      has_embedding: true,
    },
    {
      id: 1,
      captured_at: new Date("2026-04-27T17:22:00-06:00").toISOString(),
      raw: "make this paragraph tighter then paste it into the active document",
      formatted: "Make this paragraph tighter, then paste it into the active document.",
      style: "light",
      app_exe: null,
      window_title: null,
      duration_ms: 704,
      llm_ms: 0,
      has_embedding: false,
    },
  ];

  const demoStats: HistoryStats = {
    total_entries: demoHistory.length,
    embedded_entries: 2,
    db_path: "%APPDATA%/boothrflow/history.db",
    embed_endpoint: "http://localhost:11434/v1/embeddings",
    embed_model: "nomic-embed-text",
  };

  let historyEntries = $state<HistoryEntry[]>([]);
  let historyStats = $state<HistoryStats | null>(null);
  let historyLoading = $state(false);
  let historyError = $state<string | null>(null);
  let selectedHistoryId = $state<number | null>(null);
  // macOS permissions flow. The Info.plist usage strings make prod builds
  // prompt at first capture, but in dev (`tauri dev`) the prompt is
  // attributed to the parent terminal and the user has to relaunch it
  // after granting. We probe the mic on load and surface the State Settings
  // panes on demand so the user isn't hunting through System Preferences.
  const isMac = isMacPlatform();
  let micAvailable = $state<boolean | null>(null);
  let permissionsDismissed = $state(false);
  let settingsOpen = $state(false);
  let settingsExportJson = $state("");
  let settingsImportJson = $state("");
  let capturingHotkey = $state<keyof HotkeySettings | null>(null);

  // Sidebar nav for the Settings modal. Persists across opens so the user
  // doesn't have to re-find their last section. Casper's PR #2 spec'd
  // these five sections; we map our existing fields onto them.
  type SettingsSection = "general" | "llm" | "whisper" | "history" | "benchmarks" | "about";
  let activeSettingsSection = $state<SettingsSection>("general");

  // Benchmarks tab state. Captures (`<stem>.wav` + `<stem>.json`) come from
  // the `BOOTHRFLOW_DEV=1` runtime hook in `session::transcribe_and_emit`.
  // Variants (`<stem>.variants.json`) come from `pnpm bench:replay`. The
  // grading UI here is the consumer half — list captures, audition the wav,
  // grade each config 1-5 with optional notes, save back to disk. The whole
  // tab is hidden in production builds — see `devModeEnabled` below.
  type BenchCapture = {
    wav_filename: string;
    captured_at: string;
    app_exe: string | null;
    audio_seconds: number;
    original_engine: string;
    raw: string;
    formatted: string;
    has_variants: boolean;
    variant_count: number;
    graded_count: number;
  };
  type BenchVariant = {
    config_id: string;
    engine: string;
    llm_model: string;
    style: string;
    raw: string;
    formatted: string;
    stt_ms: number;
    llm_ms: number;
    grade: number | null;
    notes: string | null;
  };
  type BenchVariantsFile = {
    wav: string;
    audio_seconds: number;
    variants: BenchVariant[];
  };
  let benchCaptures = $state<BenchCapture[]>([]);
  let benchLoading = $state(false);
  let benchError = $state<string | null>(null);
  let selectedWav = $state<string | null>(null);
  let selectedVariants = $state<BenchVariantsFile | null>(null);
  let selectedWavSrc = $state<string | null>(null);
  let benchSaving = $state(false);
  let benchSaveStatus = $state<string | null>(null);

  // Developer mode toggle. Set by `BOOTHRFLOW_DEV=1` at app launch — gates
  // the Benchmarks sidebar entry + capture-to-disk in `session.rs`.
  // Probed once on mount; probes stay false in non-Tauri (storybook) mode.
  let devModeEnabled = $state(false);

  // Wave 4b polish surfaces — autostart toggle, LLM connection probe, and
  // app metadata for the About section. Loaded on Settings open.
  let autostartEnabled = $state<boolean | null>(null);
  let autostartPending = $state(false);
  let llmTestResult = $state<{ ok: boolean; latency_ms: number; error: string | null } | null>(
    null,
  );
  let llmTestPending = $state(false);
  let appVersion = $state<string | null>(null);

  const hotkeyRows: Array<{ key: keyof HotkeySettings; label: string }> = [
    { key: "ptt", label: "Push to talk" },
    { key: "toggle", label: "Toggle dictation" },
    { key: "quick_paste", label: "Quick paste" },
  ];

  // LLM endpoint quick-fill chips. Selecting one fills endpoint and
  // suggests a sensible model (current value preserved if non-default).
  // Lifted from Casper's PR #2.
  type LlmPreset = { id: string; label: string; endpoint: string; suggestedModel?: string };
  const llmPresets: LlmPreset[] = [
    {
      id: "ollama",
      label: "Ollama (local)",
      endpoint: "http://localhost:11434/v1/chat/completions",
      suggestedModel: "qwen2.5:7b",
    },
    {
      id: "llama-server",
      label: "llama.cpp server",
      endpoint: "http://localhost:8080/v1/chat/completions",
    },
    {
      id: "lm-studio",
      label: "LM Studio",
      endpoint: "http://localhost:1234/v1/chat/completions",
    },
    {
      id: "openai",
      label: "OpenAI",
      endpoint: "https://api.openai.com/v1/chat/completions",
      suggestedModel: "gpt-4o-mini",
    },
    {
      id: "openrouter",
      label: "OpenRouter",
      endpoint: "https://openrouter.ai/api/v1/chat/completions",
      suggestedModel: "qwen/qwen-2.5-7b-instruct",
    },
  ];

  function applyLlmPreset(preset: LlmPreset) {
    const next: Partial<LlmSettings> = { endpoint: preset.endpoint };
    if (preset.suggestedModel) next.model = preset.suggestedModel;
    updateLlm(next);
  }

  async function refreshAutostart() {
    if (!inDesktop) return;
    try {
      autostartEnabled = await settings.getAutostartEnabled();
    } catch {
      autostartEnabled = null;
    }
  }

  async function toggleAutostart(value: boolean) {
    if (!inDesktop) return;
    autostartPending = true;
    try {
      await settings.setAutostartEnabled(value);
      autostartEnabled = value;
    } catch (e) {
      console.warn("setAutostartEnabled failed:", e);
    } finally {
      autostartPending = false;
    }
  }

  async function runLlmTest() {
    llmTestPending = true;
    llmTestResult = null;
    try {
      llmTestResult = await settings.testLlmConnection();
    } catch (e) {
      llmTestResult = { ok: false, latency_ms: 0, error: String(e) };
    } finally {
      llmTestPending = false;
    }
  }

  /// When the user turns OCR on, persist the setting AND trigger the
  /// macOS Screen Recording permission prompt right away — so the OS
  /// prompt fires from a clear "you just said yes to a feature" UX
  /// moment, not mid-dictation. On non-macOS, only the persist runs.
  async function toggleOcrAndPrompt(checked: boolean) {
    await settings.update({ cleanup_window_ocr: checked });
    if (checked && inDesktop) {
      await settings.requestScreenRecordingPermission();
    }
  }

  async function refreshAppVersion() {
    if (!inDesktop) {
      appVersion = "0.0.0-web";
      return;
    }
    try {
      appVersion = await settings.getAppVersion();
    } catch {
      appVersion = null;
    }
  }

  // When the user opens Settings, populate the polish-section data once.
  // Cheap calls; no spinner-glue needed.
  $effect(() => {
    if (settingsOpen) {
      void refreshAutostart();
      void refreshAppVersion();
    }
  });

  async function probeMicrophone() {
    if (!inDesktop) {
      micAvailable = true;
      return;
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      micAvailable = await invoke<boolean>("microphone_available");
    } catch {
      micAvailable = false;
    }
  }

  const whisperModel = $derived(settings.current.whisper.model);

  type PermissionPane = "microphone" | "accessibility" | "input_monitoring" | "screen_recording";
  async function openPermissionPane(pane: PermissionPane) {
    if (!inDesktop) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_macos_setting", { pane });
    } catch (e) {
      console.warn("open_macos_setting failed:", e);
    }
  }

  function optionFor(options: ModelOption[], value: string): ModelOption | null {
    return options.find((option) => option.value === value) ?? null;
  }

  function modelLabel(options: ModelOption[], value: string): string {
    return optionFor(options, value)?.label ?? value;
  }

  function modelDetail(options: ModelOption[], value: string): string {
    return optionFor(options, value)?.detail ?? "";
  }

  function updateLlm(patch: Partial<LlmSettings>) {
    void settings.update({ llm: { ...settings.current.llm, ...patch } });
  }

  function correctionRows() {
    return settings.current.commonly_misheard ?? [];
  }

  async function addCorrection() {
    const next = [...correctionRows(), { wrong: "", right: "" }];
    await settings.update({ commonly_misheard: next });
  }

  async function updateCorrection(index: number, key: "wrong" | "right", value: string) {
    const next = correctionRows().map((row, i) => (i === index ? { ...row, [key]: value } : row));
    await settings.update({ commonly_misheard: next });
  }

  async function removeCorrection(index: number) {
    const next = correctionRows().filter((_, i) => i !== index);
    await settings.update({ commonly_misheard: next });
  }

  function updateEmbed(patch: Partial<EmbedSettings>) {
    void settings.update({ embed: { ...settings.current.embed, ...patch } });
  }

  function updateHotkey(key: keyof HotkeySettings, value: string) {
    void settings.update({ hotkeys: { ...settings.current.hotkeys, [key]: value } });
  }

  function captureChord(event: KeyboardEvent) {
    if (!capturingHotkey) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      capturingHotkey = null;
      return;
    }
    const chord = chordFromEvent(event);
    if (!chord) return;
    updateHotkey(capturingHotkey, chord);
    capturingHotkey = null;
  }

  function chordFromEvent(event: KeyboardEvent): string | null {
    const parts: string[] = [];
    if (event.ctrlKey) parts.push("Ctrl");
    if (event.altKey) parts.push(isMac ? "Option" : "Alt");
    if (event.shiftKey) parts.push("Shift");
    if (event.metaKey) parts.push(isMac ? "Cmd" : "Win");

    const key = normalizedEventKey(event);
    if (key && !["Ctrl", "Option", "Alt", "Shift", "Cmd", "Win"].includes(key)) {
      parts.push(key);
    }

    return parts.length >= 2 ? parts.join(" + ") : null;
  }

  function normalizedEventKey(event: KeyboardEvent): string | null {
    if (event.key === " ") return "Space";
    if (event.key.length === 1) return event.key.toUpperCase();
    const key = event.key.toLowerCase();
    if (key === "control") return "Ctrl";
    if (key === "meta") return isMac ? "Cmd" : "Win";
    if (key === "alt") return isMac ? "Option" : "Alt";
    if (key === "shift") return "Shift";
    if (key === "escape") return "Escape";
    if (key === "enter") return "Enter";
    if (key === "tab") return "Tab";
    if (key === "spacebar") return "Space";
    return event.key;
  }

  async function exportSettings() {
    try {
      settingsExportJson = await settings.exportJson();
    } catch (error) {
      console.warn("settings export failed:", error);
    }
  }

  async function importSettings() {
    if (!settingsImportJson.trim()) return;
    try {
      await settings.importJson(settingsImportJson);
      settingsImportJson = "";
    } catch (error) {
      console.warn("settings import failed:", error);
    }
  }

  const displayHistory = $derived(
    historyEntries.length ? historyEntries : inDesktop ? [] : demoHistory,
  );
  const selectedEntry = $derived(
    displayHistory.find((entry) => entry.id === selectedHistoryId) ?? displayHistory[0] ?? null,
  );
  const displayStats = $derived(historyStats ?? (inDesktop ? null : demoStats));
  const liveText = $derived(dictationStore.lastResult?.text ?? selectedEntry?.formatted ?? "");
  // Prefer live telemetry from the most recent dictation; fall back to the
  // selected history entry. Without this, the "Current" panel keeps showing
  // an old entry's timings even after a fresh dictation completes (which
  // hadn't yet been pulled into `historyEntries`), so the user saw 0 ms LLM
  // until they hit Refresh.
  const sttMs = $derived(
    dictationStore.lastDone?.stt_ms ??
      selectedEntry?.duration_ms ??
      dictationStore.lastResult?.duration_ms ??
      0,
  );
  const llmMs = $derived(dictationStore.lastDone?.llm_ms ?? selectedEntry?.llm_ms ?? 0);
  const totalMs = $derived(sttMs + llmMs);
  // "0 ms" is ambiguous — distinguish skipped (raw / short / disabled) from
  // failed (Ollama unreachable) so the UI doesn't read like a regression.
  const llmStatus = $derived<
    "ran" | "skipped-raw" | "skipped-short" | "unreachable" | "disabled" | "privacy" | "idle"
  >(
    dictationStore.llmMissing
      ? "unreachable"
      : settings.privacyMode
        ? "privacy"
        : !settings.llmEnabled
          ? "disabled"
          : settings.style === "raw"
            ? "skipped-raw"
            : llmMs > 0
              ? "ran"
              : dictationStore.lastDone
                ? "skipped-short"
                : "idle",
  );
  function llmDisplay(): string {
    switch (llmStatus) {
      case "ran": {
        const tps = dictationStore.lastDone?.llm_tok_per_sec;
        const base = formatMs(llmMs);
        // Show tok/s alongside ms when the backend reported it. Distinct
        // from `null` (Ollama silent) — null is rendered as just the ms.
        return tps != null && tps > 0 ? `${base} · ${tps.toFixed(0)} tok/s` : base;
      }
      case "skipped-raw":
        return "off (raw)";
      case "skipped-short":
        return "skipped";
      case "unreachable":
        return "unreachable";
      case "disabled":
        return "off";
      case "privacy":
        return "privacy";
      case "idle":
      default:
        return llmMs ? formatMs(llmMs) : "0 ms";
    }
  }
  const captureSeconds = $derived(
    dictationStore.lastSummary?.seconds ?? (selectedEntry ? selectedEntry.duration_ms / 1000 : 0),
  );
  const peakLevel = $derived(
    dictationStore.lastSummary ? `${dictationStore.lastSummary.peak_dbfs.toFixed(1)} dBFS` : "n/a",
  );
  const statusLabel = $derived(
    dictationStore.status === "listening"
      ? "Listening"
      : dictationStore.status === "processing"
        ? "Processing"
        : "Ready",
  );

  const cleanupModel = $derived(
    settings.privacyMode || !settings.llmEnabled || settings.style === "raw"
      ? "Bypass"
      : settings.current.llm.model,
  );
  const embeddingModel = $derived(
    settings.current.embed.enabled ? settings.current.embed.model : "Off",
  );

  async function loadHistory() {
    if (!inDesktop) {
      historyEntries = demoHistory;
      historyStats = demoStats;
      selectedHistoryId = demoHistory[0]?.id ?? null;
      return;
    }

    historyLoading = true;
    historyError = null;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const [recent, stats] = await Promise.all([
        invoke<HistoryEntry[]>("history_recent", { limit: 100 }),
        invoke<HistoryStats>("history_stats"),
      ]);
      historyEntries = recent;
      historyStats = stats;
      if (!selectedHistoryId && recent[0]) selectedHistoryId = recent[0].id;
      if (selectedHistoryId && !recent.some((entry) => entry.id === selectedHistoryId)) {
        selectedHistoryId = recent[0]?.id ?? null;
      }
    } catch (error) {
      historyError = String(error);
    } finally {
      historyLoading = false;
    }
  }

  function selectStyle(style: Style) {
    settings.style = style;
  }

  function styleLabel(style: Style): string {
    return styleOptions.find((option) => option.value === style)?.label ?? style;
  }

  function formatDate(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(date);
  }

  function formatMs(ms: number | null | undefined): string {
    if (!ms) return "0 ms";
    return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
  }

  function formatSeconds(seconds: number): string {
    return seconds ? `${seconds.toFixed(1)} s` : "n/a";
  }

  function appLabel(entry: HistoryEntry): string {
    return entry.window_title || entry.app_exe || "Unknown app";
  }

  function preview(text: string): string {
    return text.length > 130 ? `${text.slice(0, 127)}...` : text;
  }

  function embeddingRatio(stats: HistoryStats | null): string {
    if (!stats) return "n/a";
    return `${stats.embedded_entries}/${stats.total_entries}`;
  }

  async function loadBenchCaptures() {
    if (!inDesktop) {
      benchCaptures = [];
      return;
    }
    benchLoading = true;
    benchError = null;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      benchCaptures = await invoke<BenchCapture[]>("bench_list");
      if (selectedWav && !benchCaptures.some((c) => c.wav_filename === selectedWav)) {
        selectedWav = null;
        selectedVariants = null;
        selectedWavSrc = null;
      }
    } catch (error) {
      benchError = String(error);
    } finally {
      benchLoading = false;
    }
  }

  async function selectCapture(wav_filename: string) {
    if (!inDesktop) return;
    selectedWav = wav_filename;
    selectedVariants = null;
    selectedWavSrc = null;
    benchSaveStatus = null;
    try {
      const { invoke, convertFileSrc } = await import("@tauri-apps/api/core");
      const [variants, absPath] = await Promise.all([
        invoke<BenchVariantsFile | null>("bench_load", { wavFilename: wav_filename }),
        invoke<string>("bench_wav_path", { wavFilename: wav_filename }),
      ]);
      selectedVariants = variants;
      selectedWavSrc = convertFileSrc(absPath);
    } catch (error) {
      benchError = String(error);
    }
  }

  function gradeVariant(idx: number, grade: number) {
    if (!selectedVariants) return;
    const next = selectedVariants.variants.slice();
    const cur = next[idx];
    if (!cur) return;
    next[idx] = { ...cur, grade: cur.grade === grade ? null : grade };
    selectedVariants = { ...selectedVariants, variants: next };
    benchSaveStatus = null;
  }

  function noteVariant(idx: number, notes: string) {
    if (!selectedVariants) return;
    const next = selectedVariants.variants.slice();
    const cur = next[idx];
    if (!cur) return;
    next[idx] = { ...cur, notes: notes || null };
    selectedVariants = { ...selectedVariants, variants: next };
    benchSaveStatus = null;
  }

  async function saveBenchGrades() {
    if (!inDesktop || !selectedWav || !selectedVariants) return;
    benchSaving = true;
    benchSaveStatus = null;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("bench_save", {
        wavFilename: selectedWav,
        variants: selectedVariants,
      });
      benchSaveStatus = "Saved";
      void loadBenchCaptures();
    } catch (error) {
      benchSaveStatus = `Error: ${error}`;
    } finally {
      benchSaving = false;
    }
  }

  // Per-config aggregate (mean grade, count). Recomputed when any capture's
  // variants get loaded — note: this only reflects the currently *open*
  // capture's variants, since loading every variants file just to compute a
  // leaderboard would be wasteful. The capture row's `graded_count` shows
  // overall progress; this leaderboard is the within-capture comparison.
  const variantLeaderboard = $derived.by(() => {
    if (!selectedVariants) return [] as { config_id: string; mean: number; count: number }[];
    const groups: Record<string, { sum: number; count: number }> = {};
    for (const v of selectedVariants.variants) {
      if (v.grade == null) continue;
      const cur = groups[v.config_id] ?? { sum: 0, count: 0 };
      cur.sum += v.grade;
      cur.count += 1;
      groups[v.config_id] = cur;
    }
    return Object.entries(groups)
      .map(([config_id, { sum, count }]) => ({ config_id, mean: sum / count, count }))
      .sort((a, b) => b.mean - a.mean);
  });

  async function probeDevMode() {
    if (!inDesktop) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      devModeEnabled = await invoke<boolean>("dev_mode_enabled");
    } catch {
      devModeEnabled = false;
    }
  }

  onMount(() => {
    void settings.load();
    void dictationStore.attach();
    void loadHistory();
    void probeMicrophone();
    void probeDevMode();
  });

  // Refresh history whenever a fresh dictation completes so the new entry
  // surfaces (with correct stt/llm timings from the DB record) without the
  // user having to hit Refresh.
  let lastSeenDoneAt = 0;
  $effect(() => {
    const done = dictationStore.lastDone;
    if (!done || !inDesktop) return;
    // total_ms is monotonic per dictation; use it as a change signal so we
    // don't loop on selectedHistoryId writes.
    if (done.total_ms === lastSeenDoneAt) return;
    lastSeenDoneAt = done.total_ms;
    void loadHistory().then(() => {
      if (historyEntries[0]) selectedHistoryId = historyEntries[0].id;
    });
  });

  // Lazily load the captures list the first time the Benchmarks tab is
  // opened, then refresh on each subsequent open (cheap — just a directory
  // scan + small JSON reads).
  $effect(() => {
    if (settingsOpen && activeSettingsSection === "benchmarks") {
      void loadBenchCaptures();
    }
  });
</script>

<svelte:window onkeydown={captureChord} />

{#if isPill}
  <ListenPill />
{:else if isQuickPaste}
  {#await import("$lib/quickpaste/QuickPasteApp.svelte") then m}
    {@const QuickPasteApp = m.default}
    <QuickPasteApp />
  {/await}
{:else}
  <main class="app-shell">
    <header class="app-topbar">
      <div class="brand-lockup">
        <span class="brand-mark" aria-hidden="true"
          ><Icon name="mic" size={16} strokeWidth={2.3} /></span
        >
        <div>
          <h1>boothrflow</h1>
          <p>Local-first dictation</p>
        </div>
      </div>

      <div class="top-actions">
        <span class="status-pill" data-status={dictationStore.status}>
          <span class="status-dot" aria-hidden="true"></span>
          {statusLabel}
        </span>
        <kbd title="Hold to dictate"><Icon name="command" size={13} /> {dictationHotkey}</kbd>
        <kbd title="Tap to toggle dictation hands-free"
          ><Icon name="zap" size={13} /> {toggleDictationHotkey}</kbd
        >
        <kbd title="Open quick-paste palette"
          ><Icon name="history" size={13} /> {quickPasteHotkey}</kbd
        >
        <button class="quiet-button" type="button" onclick={() => (settingsOpen = true)}>
          <Icon name="settings" size={13} /> Settings
        </button>
      </div>
    </header>

    {#if dictationStore.modelMissing}
      <section class="notice" aria-live="polite">
        <Icon name="lock" size={15} />
        <div>
          <strong>Whisper model missing</strong>
          <pre>{dictationStore.modelMissing}</pre>
        </div>
      </section>
    {/if}

    {#if dictationStore.llmMissing}
      <section class="notice" aria-live="polite">
        <Icon name="brain" size={15} />
        <div>
          <strong>Cleanup model unreachable — using raw transcript</strong>
          <pre>{dictationStore.llmMissing}</pre>
        </div>
      </section>
    {/if}

    {#if isMac && inDesktop && micAvailable === false && !permissionsDismissed}
      <section class="notice" aria-live="polite">
        <Icon name="lock" size={15} />
        <div>
          <strong>Microphone access blocked</strong>
          <pre>boothrflow can't see an input device. Grant Microphone in System Settings, then relaunch the app (or the terminal that started it, in dev mode).</pre>
          <div class="notice-actions">
            <button
              class="quiet-button"
              type="button"
              onclick={() => void openPermissionPane("microphone")}>Open Microphone settings</button
            >
            <button class="quiet-button" type="button" onclick={() => (permissionsDismissed = true)}
              >Dismiss</button
            >
          </div>
        </div>
      </section>
    {/if}

    {#if settingsOpen}
      <div class="settings-backdrop">
        <div
          class="settings-panel"
          role="dialog"
          aria-modal="true"
          aria-labelledby="settings-heading"
        >
          <div class="settings-titlebar">
            <div>
              <span class="section-kicker">Wave 4B</span>
              <h2 id="settings-heading">Settings</h2>
            </div>
            <div class="settings-status">
              {#if settings.saving}
                <span>Saving</span>
              {:else if settings.downloadStatus}
                <span>{settings.downloadStatus}</span>
              {:else}
                <span>Saved</span>
              {/if}
              <button
                class="icon-button"
                type="button"
                aria-label="Close settings"
                onclick={() => (settingsOpen = false)}
              >
                <Icon name="x" size={15} />
              </button>
            </div>
          </div>

          {#if settings.error}
            <div class="inline-error">{settings.error}</div>
          {/if}

          <div class="settings-shell">
            <aside class="settings-sidebar" aria-label="Settings sections">
              <button
                type="button"
                class="settings-nav-item"
                class:active={activeSettingsSection === "general"}
                onclick={() => (activeSettingsSection = "general")}
                ><Icon name="settings" size={14} /> General</button
              >
              <button
                type="button"
                class="settings-nav-item"
                class:active={activeSettingsSection === "llm"}
                onclick={() => (activeSettingsSection = "llm")}
                ><Icon name="brain" size={14} /> LLM</button
              >
              <button
                type="button"
                class="settings-nav-item"
                class:active={activeSettingsSection === "whisper"}
                onclick={() => (activeSettingsSection = "whisper")}
                ><Icon name="mic" size={14} /> Speech</button
              >
              <button
                type="button"
                class="settings-nav-item"
                class:active={activeSettingsSection === "history"}
                onclick={() => (activeSettingsSection = "history")}
                ><Icon name="database" size={14} /> History</button
              >
              {#if devModeEnabled}
                <button
                  type="button"
                  class="settings-nav-item"
                  class:active={activeSettingsSection === "benchmarks"}
                  onclick={() => (activeSettingsSection = "benchmarks")}
                  ><Icon name="bar-chart" size={14} /> Benchmarks</button
                >
              {/if}
              <button
                type="button"
                class="settings-nav-item"
                class:active={activeSettingsSection === "about"}
                onclick={() => (activeSettingsSection = "about")}
                ><Icon name="info" size={14} /> About</button
              >
            </aside>

            <div class="settings-content">
              {#if activeSettingsSection === "general"}
                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="pen" size={14} /></span>
                    <div>
                      <span class="section-kicker">Style</span>
                      <h3>Default cleanup style</h3>
                    </div>
                  </div>
                  <p class="settings-help">
                    How aggressively the LLM may restructure your raw transcript. Pick the level
                    that matches what you usually dictate — Light for short messages, Assertive for
                    long brain dumps you want organized.
                  </p>
                  <div class="style-segmented" role="radiogroup" aria-label="Cleanup style level">
                    {#each structureStyleOptions as option (option.value)}
                      <button
                        type="button"
                        class="style-segment"
                        class:active={settings.style === option.value}
                        role="radio"
                        aria-checked={settings.style === option.value}
                        onclick={() => selectStyle(option.value)}
                      >
                        <Icon name={option.icon} size={13} />
                        <span class="style-segment-label">{option.label}</span>
                      </button>
                    {/each}
                  </div>
                  <p class="settings-help">
                    {styleOptions.find((o) => o.value === settings.style)?.detail ?? ""}
                  </p>
                  <details class="fun-presets">
                    <summary>Fun presets</summary>
                    <button
                      type="button"
                      class="style-segment"
                      class:active={settings.style === "captains-log"}
                      onclick={() => selectStyle("captains-log")}
                    >
                      <Icon name="radio" size={13} />
                      <span class="style-segment-label">Captain's Log</span>
                    </button>
                    <p class="settings-help">
                      Star-Trek-style log entry. Stardate prefix + 24th-century rewrite.
                    </p>
                  </details>
                  <label class="toggle-row">
                    <input
                      type="checkbox"
                      checked={settings.current.privacy_mode}
                      onchange={(event) =>
                        void settings.update({ privacy_mode: event.currentTarget.checked })}
                    />
                    <span>Privacy mode</span>
                  </label>
                  <p class="settings-help">
                    Skips the LLM cleanup pass entirely <em>and</em> suppresses any context the
                    cleanup prompt would otherwise pull in (foreground app name, window title,
                    focused-window OCR). Useful when you're dictating sensitive content you don't
                    want any model (local or cloud) to see. Behaves like selecting <em>Raw</em>
                    style on local-only setups; matters most when a cloud BYOK endpoint is configured.
                    Whisper transcription still runs locally.
                  </p>
                </section>

                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="key" size={14} /></span>
                    <div>
                      <span class="section-kicker">Input</span>
                      <h3>Hotkeys</h3>
                    </div>
                  </div>
                  {#each hotkeyRows as row (row.key)}
                    <label class="settings-field">
                      <span>{row.label}</span>
                      <div class="hotkey-capture">
                        <input
                          value={settings.current.hotkeys[row.key]}
                          oninput={(event) => updateHotkey(row.key, event.currentTarget.value)}
                        />
                        <button
                          class="quiet-button"
                          type="button"
                          onclick={() => (capturingHotkey = row.key)}
                        >
                          {capturingHotkey === row.key ? "Press keys" : "Capture"}
                        </button>
                      </div>
                    </label>
                  {/each}
                </section>

                {#if inDesktop}
                  <section class="settings-section">
                    <div class="settings-section-head">
                      <span class="step-icon"><Icon name="zap" size={14} /></span>
                      <div>
                        <span class="section-kicker">Startup</span>
                        <h3>Launch at login</h3>
                      </div>
                    </div>
                    <label class="toggle-row">
                      <input
                        type="checkbox"
                        checked={autostartEnabled === true}
                        disabled={autostartPending || autostartEnabled === null}
                        onchange={(event) => void toggleAutostart(event.currentTarget.checked)}
                      />
                      <span
                        >Start boothrflow when I log in{autostartEnabled === null
                          ? " (probing)"
                          : ""}</span
                      >
                    </label>
                  </section>
                {/if}

                {#if isMac && inDesktop}
                  <section class="settings-section">
                    <div class="settings-section-head">
                      <span class="step-icon"><Icon name="lock" size={14} /></span>
                      <div>
                        <span class="section-kicker">macOS</span>
                        <h3>Permissions</h3>
                      </div>
                    </div>
                    <p class="settings-help">
                      boothrflow uses up to four permissions on macOS. The first three are required;
                      Screen Recording is optional and only used when the OCR cleanup context toggle
                      is on. Click each to open the relevant pane in System Settings, toggle the
                      switch, then relaunch.
                    </p>
                    <ol class="permission-list">
                      <li>
                        <div>
                          <strong>Microphone</strong>
                          <small
                            >{micAvailable === false
                              ? "Currently blocked — capture will fail"
                              : "Audio capture"}</small
                          >
                        </div>
                        <button
                          class="quiet-button"
                          type="button"
                          onclick={() => void openPermissionPane("microphone")}>Open</button
                        >
                      </li>
                      <li>
                        <div>
                          <strong>Accessibility</strong>
                          <small>Paste into the focused application</small>
                        </div>
                        <button
                          class="quiet-button"
                          type="button"
                          onclick={() => void openPermissionPane("accessibility")}>Open</button
                        >
                      </li>
                      <li>
                        <div>
                          <strong>Input Monitoring</strong>
                          <small>Global hotkey when boothrflow isn't focused</small>
                        </div>
                        <button
                          class="quiet-button"
                          type="button"
                          onclick={() => void openPermissionPane("input_monitoring")}>Open</button
                        >
                      </li>
                      <li>
                        <div>
                          <strong>Screen Recording</strong>
                          <small>Optional — only used when LLM → "focused-window OCR" is on</small>
                        </div>
                        <button
                          class="quiet-button"
                          type="button"
                          onclick={() => void openPermissionPane("screen_recording")}>Open</button
                        >
                      </li>
                    </ol>
                  </section>
                {/if}
              {:else if activeSettingsSection === "llm"}
                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="brain" size={14} /></span>
                    <div>
                      <span class="section-kicker">Cleanup</span>
                      <h3>LLM</h3>
                    </div>
                  </div>

                  <label class="toggle-row">
                    <input
                      type="checkbox"
                      checked={settings.current.llm.enabled}
                      onchange={(event) => updateLlm({ enabled: event.currentTarget.checked })}
                    />
                    <span>LLM cleanup</span>
                  </label>
                  <p class="settings-help">
                    Sends the raw Whisper transcript through a small local LLM (Qwen 2.5 by default
                    via Ollama) that adds punctuation, capitalization, splits run-on sentences,
                    drops disfluencies (<em>uh</em>, <em>um</em>, <em>you know</em>), and corrects
                    context-mismatched words. Disable to paste the raw STT output verbatim — same
                    effect as selecting <em>Raw</em> style.
                  </p>

                  <div class="preset-chips" aria-label="LLM endpoint presets">
                    <span class="preset-chips-label">Presets</span>
                    {#each llmPresets as preset (preset.id)}
                      <button
                        type="button"
                        class="preset-chip"
                        class:active={settings.current.llm.endpoint === preset.endpoint}
                        onclick={() => applyLlmPreset(preset)}
                      >
                        {preset.label}
                      </button>
                    {/each}
                  </div>

                  <label class="settings-field">
                    <span>Endpoint</span>
                    <input
                      value={settings.current.llm.endpoint}
                      oninput={(event) => updateLlm({ endpoint: event.currentTarget.value })}
                    />
                  </label>

                  <label class="settings-field">
                    <span>Model</span>
                    <select
                      value={settings.current.llm.model}
                      onchange={(event) => updateLlm({ model: event.currentTarget.value })}
                    >
                      {#each settings.options.llm_models as option (option.value)}
                        <option value={option.value} disabled={!option.available}
                          >{option.label}{option.value === settings.current.llm.model
                            ? " (active)"
                            : ""}</option
                        >
                      {/each}
                    </select>
                    <small
                      >{modelDetail(settings.options.llm_models, settings.current.llm.model)}</small
                    >
                  </label>

                  <label class="settings-field">
                    <span>API key</span>
                    <input
                      type="password"
                      value={settings.current.llm.api_key ?? ""}
                      placeholder="Stored in OS keychain when available"
                      oninput={(event) => updateLlm({ api_key: event.currentTarget.value || null })}
                    />
                  </label>

                  <div class="settings-actions">
                    <button
                      class="quiet-button"
                      type="button"
                      disabled={llmTestPending}
                      onclick={() => void runLlmTest()}
                    >
                      {llmTestPending ? "Testing…" : "Test connection"}
                    </button>
                    {#if llmTestResult}
                      {#if llmTestResult.ok}
                        <span class="settings-result ok">OK · {llmTestResult.latency_ms} ms</span>
                      {:else}
                        <span class="settings-result fail">{llmTestResult.error ?? "Failed"}</span>
                      {/if}
                    {/if}
                  </div>

                  <label class="toggle-row">
                    <input
                      type="checkbox"
                      checked={settings.current.cleanup_window_ocr ?? false}
                      onchange={(event) => void toggleOcrAndPrompt(event.currentTarget.checked)}
                    />
                    <span>Use focused-window OCR as cleanup context (preview)</span>
                  </label>
                  <p class="settings-help">
                    Captures the visible on-screen text and feeds it to the cleanup prompt as
                    supporting context — helps disambiguate names, models, and jargon that Whisper
                    mishears. Requires Screen Recording permission; turning this on triggers the
                    macOS permission prompt now (rather than mid-dictation). Disabled automatically
                    when <em>Privacy mode</em> is on.
                  </p>
                </section>
              {:else if activeSettingsSection === "whisper"}
                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="mic" size={14} /></span>
                    <div>
                      <span class="section-kicker">Voice</span>
                      <h3>Recognition</h3>
                    </div>
                  </div>

                  <p class="settings-help">
                    Two engine families. <strong>Whisper</strong> shows a live transcript in the
                    pill while you talk and supports 99 languages. <strong>Parakeet</strong>
                    is more accurate on technical jargon and ~3× faster, but English-only and shows the
                    transcript only after you release the hotkey (no live preview). Pick by use case —
                    both are local and private.
                  </p>

                  <label class="settings-field">
                    <span>Speech-to-text model</span>
                    <select
                      value={settings.current.whisper.model}
                      onchange={(event) => void settings.setWhisperModel(event.currentTarget.value)}
                    >
                      {#each settings.options.whisper_models as option (option.value)}
                        <option value={option.value} disabled={!option.available}
                          >{option.label}{option.value === settings.current.whisper.model
                            ? " (active)"
                            : ""}</option
                        >
                      {/each}
                    </select>
                    <small
                      >{modelDetail(
                        settings.options.whisper_models,
                        settings.current.whisper.model,
                      )}</small
                    >
                  </label>

                  <label class="settings-field">
                    <span>Vocabulary</span>
                    <textarea
                      rows="6"
                      value={settings.current.vocabulary}
                      placeholder="kubernetes, terraform, GraphQL — comma-separated proper nouns + jargon."
                      oninput={(event) =>
                        void settings.update({ vocabulary: event.currentTarget.value })}
                    ></textarea>
                    <small
                      >Appended to Whisper's initial prompt — biases recognition toward these terms.
                      Also injected into the LLM cleanup prompt's authoritative-spelling block.</small
                    >
                  </label>

                  <div class="settings-field">
                    <span>Common mishearings</span>
                    {#each correctionRows() as pair, index (index)}
                      <div class="correction-row">
                        <input
                          type="text"
                          placeholder="wrong"
                          value={pair.wrong}
                          oninput={(event) =>
                            void updateCorrection(index, "wrong", event.currentTarget.value)}
                        />
                        <span class="correction-arrow" aria-hidden="true">→</span>
                        <input
                          type="text"
                          placeholder="right"
                          value={pair.right}
                          oninput={(event) =>
                            void updateCorrection(index, "right", event.currentTarget.value)}
                        />
                        <button
                          class="quiet-button"
                          type="button"
                          aria-label="Remove correction"
                          onclick={() => void removeCorrection(index)}
                        >
                          Remove
                        </button>
                      </div>
                    {/each}
                    <button class="quiet-button" type="button" onclick={() => void addCorrection()}>
                      Add correction
                    </button>
                    <small
                      >Wrong → right pairs the LLM applies as authoritative substitutions (e.g.
                      "kwen" → "Qwen"). Empty rows are ignored.</small
                    >
                  </div>

                  <label class="toggle-row">
                    <input
                      type="checkbox"
                      checked={settings.current.auto_learn_corrections ?? false}
                      onchange={(event) =>
                        void settings.update({
                          auto_learn_corrections: event.currentTarget.checked,
                        })}
                    />
                    <span>Auto-learn corrections after paste (preview)</span>
                  </label>
                  <p class="settings-help">
                    After pasting, watches the focused field for ~8 seconds. If you make a small
                    single-word edit (e.g. correcting "kwen" → "qwen"), records it above so the
                    cleanup pass applies it next time. Requires Accessibility permission. Disabled
                    automatically when <em>Privacy mode</em> is on. The macOS accessibility read is
                    being finalized — see
                    <code>docs/waves/wave-5-context-aware-cleanup.md</code>.
                  </p>
                </section>
              {:else if activeSettingsSection === "history"}
                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="database" size={14} /></span>
                    <div>
                      <span class="section-kicker">Memory</span>
                      <h3>Embeddings</h3>
                    </div>
                  </div>

                  <label class="toggle-row">
                    <input
                      type="checkbox"
                      checked={settings.current.embed.enabled}
                      onchange={(event) => updateEmbed({ enabled: event.currentTarget.checked })}
                    />
                    <span>History embeddings</span>
                  </label>

                  <label class="settings-field">
                    <span>Model</span>
                    <select
                      value={settings.current.embed.model}
                      onchange={(event) => updateEmbed({ model: event.currentTarget.value })}
                    >
                      {#each settings.options.embed_models as option (option.value)}
                        <option value={option.value} disabled={!option.available}
                          >{option.label}</option
                        >
                      {/each}
                    </select>
                  </label>

                  <label class="settings-field">
                    <span>Endpoint</span>
                    <input
                      value={settings.current.embed.endpoint}
                      oninput={(event) => updateEmbed({ endpoint: event.currentTarget.value })}
                    />
                  </label>

                  <label class="settings-field">
                    <span>API key</span>
                    <input
                      type="password"
                      value={settings.current.embed.api_key ?? ""}
                      placeholder="Stored in OS keychain when available"
                      oninput={(event) =>
                        updateEmbed({ api_key: event.currentTarget.value || null })}
                    />
                  </label>
                </section>
              {:else if activeSettingsSection === "benchmarks" && devModeEnabled}
                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="bar-chart" size={14} /></span>
                    <div>
                      <span class="section-kicker">Quality</span>
                      <h3>Benchmark grading</h3>
                    </div>
                  </div>
                  <p class="settings-help">
                    Captures saved with <code>BOOTHRFLOW_DEV=1</code>. Run
                    <code>pnpm bench:replay</code> to fan each wav out across every available STT/LLM/style
                    combo, then grade the variants here. 1-5 stars per variant; click a star to set, click
                    the same star again to clear. Notes are free-form.
                  </p>
                  {#if benchError}
                    <div class="inline-error">{benchError}</div>
                  {/if}

                  <div class="bench-shell">
                    <aside class="bench-list" aria-label="Captured wavs">
                      <div class="bench-list-head">
                        <strong
                          >{benchCaptures.length} capture{benchCaptures.length === 1
                            ? ""
                            : "s"}</strong
                        >
                        <button
                          type="button"
                          class="quiet-button"
                          onclick={() => void loadBenchCaptures()}
                          disabled={benchLoading}
                        >
                          {benchLoading ? "Loading..." : "Refresh"}
                        </button>
                      </div>
                      {#if benchCaptures.length === 0 && !benchLoading}
                        <div class="bench-empty">
                          No captures yet. Set
                          <code>BOOTHRFLOW_DEV=1</code> when launching the app and dictate something.
                        </div>
                      {/if}
                      <ul class="bench-row-list">
                        {#each benchCaptures as cap (cap.wav_filename)}
                          <li>
                            <button
                              type="button"
                              class="bench-row"
                              class:selected={selectedWav === cap.wav_filename}
                              onclick={() => void selectCapture(cap.wav_filename)}
                            >
                              <span class="bench-row-top">
                                <strong>{cap.app_exe ?? "Unknown app"}</strong>
                                <small>{formatSeconds(cap.audio_seconds)}</small>
                              </span>
                              <span class="bench-row-mid">{preview(cap.formatted)}</span>
                              <span class="bench-row-foot">
                                <small>{cap.captured_at ? formatDate(cap.captured_at) : ""}</small>
                                {#if cap.has_variants}
                                  <small>{cap.graded_count}/{cap.variant_count} graded</small>
                                {:else}
                                  <small>no variants</small>
                                {/if}
                              </span>
                            </button>
                          </li>
                        {/each}
                      </ul>
                    </aside>

                    <div class="bench-detail">
                      {#if !selectedWav}
                        <div class="bench-empty">Pick a capture on the left.</div>
                      {:else if !selectedVariants}
                        <div class="bench-empty">
                          No <code>{selectedWav.replace(/\.wav$/, ".variants.json")}</code>
                          sidecar found. Run <code>pnpm bench:replay</code> first.
                        </div>
                        {#if selectedWavSrc}
                          <audio
                            class="bench-audio"
                            controls
                            src={selectedWavSrc}
                            preload="metadata"
                          ></audio>
                        {/if}
                      {:else}
                        {#if selectedWavSrc}
                          <audio
                            class="bench-audio"
                            controls
                            src={selectedWavSrc}
                            preload="metadata"
                          ></audio>
                        {/if}

                        {#if variantLeaderboard.length > 0}
                          <div class="bench-leaderboard">
                            <strong>Leaderboard (this capture)</strong>
                            <ol>
                              {#each variantLeaderboard as entry (entry.config_id)}
                                <li>
                                  <span class="lb-config">{entry.config_id}</span>
                                  <span class="lb-mean"
                                    >{entry.mean.toFixed(1)} <small>★</small></span
                                  >
                                  <small class="lb-count">{entry.count}×</small>
                                </li>
                              {/each}
                            </ol>
                          </div>
                        {/if}

                        <ul class="variant-list">
                          {#each selectedVariants.variants as v, i (v.config_id)}
                            <li class="variant-card">
                              <header class="variant-card-head">
                                <strong>{v.config_id}</strong>
                                <small
                                  >{v.engine} • {v.llm_model} • {v.style} • STT {formatMs(v.stt_ms)} •
                                  LLM {formatMs(v.llm_ms)}</small
                                >
                              </header>
                              <div class="variant-text">
                                <label>
                                  <span>Raw</span>
                                  <textarea readonly rows="2" value={v.raw}></textarea>
                                </label>
                                <label>
                                  <span>Formatted</span>
                                  <textarea readonly rows="3" value={v.formatted}></textarea>
                                </label>
                              </div>
                              <div class="variant-grade">
                                <span class="variant-grade-label">Grade</span>
                                <div class="star-row" role="group" aria-label="Grade 1 to 5">
                                  {#each [1, 2, 3, 4, 5] as n (n)}
                                    <button
                                      type="button"
                                      class="star-btn"
                                      class:filled={v.grade != null && v.grade >= n}
                                      aria-label={`${n} star${n === 1 ? "" : "s"}`}
                                      aria-pressed={v.grade === n}
                                      onclick={() => gradeVariant(i, n)}
                                    >
                                      <Icon name="star" size={16} />
                                    </button>
                                  {/each}
                                  {#if v.grade != null}
                                    <small class="grade-readout">{v.grade}/5</small>
                                  {/if}
                                </div>
                                <label class="variant-notes">
                                  <span>Notes</span>
                                  <input
                                    type="text"
                                    value={v.notes ?? ""}
                                    placeholder="optional"
                                    oninput={(e) => noteVariant(i, e.currentTarget.value)}
                                  />
                                </label>
                              </div>
                            </li>
                          {/each}
                        </ul>

                        <div class="settings-actions bench-save-row">
                          <button
                            type="button"
                            class="primary-button"
                            onclick={() => void saveBenchGrades()}
                            disabled={benchSaving}
                          >
                            {benchSaving ? "Saving..." : "Save grades"}
                          </button>
                          {#if benchSaveStatus}
                            <span class="bench-save-status">{benchSaveStatus}</span>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  </div>
                </section>
              {:else if activeSettingsSection === "about"}
                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="info" size={14} /></span>
                    <div>
                      <span class="section-kicker">About</span>
                      <h3>boothrflow</h3>
                    </div>
                  </div>
                  <dl class="about-meta">
                    <div>
                      <dt>Version</dt>
                      <dd>{appVersion ?? "…"}</dd>
                    </div>
                    <div>
                      <dt>Repo</dt>
                      <dd>
                        <a
                          href="https://github.com/ebootheee/boothrflow"
                          target="_blank"
                          rel="noopener">github.com/ebootheee/boothrflow</a
                        >
                      </dd>
                    </div>
                    <div>
                      <dt>License</dt>
                      <dd>Apache-2.0</dd>
                    </div>
                  </dl>
                </section>

                <section class="settings-section">
                  <div class="settings-section-head">
                    <span class="step-icon"><Icon name="server" size={14} /></span>
                    <div>
                      <span class="section-kicker">Portable</span>
                      <h3>Import / Export</h3>
                    </div>
                  </div>

                  <div class="settings-actions">
                    <button
                      class="quiet-button"
                      type="button"
                      onclick={() => void exportSettings()}
                    >
                      <Icon name="download" size={13} /> Export
                    </button>
                    <button
                      class="quiet-button"
                      type="button"
                      onclick={() => void importSettings()}
                    >
                      <Icon name="upload" size={13} /> Import
                    </button>
                  </div>
                  <label class="settings-field">
                    <span>JSON</span>
                    <textarea
                      rows="5"
                      value={settingsExportJson || settingsImportJson}
                      placeholder="boothrflow.settings.json"
                      oninput={(event) => {
                        settingsExportJson = "";
                        settingsImportJson = event.currentTarget.value;
                      }}
                    ></textarea>
                  </label>
                </section>
              {/if}
            </div>
          </div>
        </div>
      </div>
    {/if}

    <section class="toolbar" aria-label="Dictation controls and model status">
      <label class="field compact-field">
        <span>Style</span>
        <select
          value={settings.style}
          onchange={(event) => selectStyle(event.currentTarget.value as Style)}
        >
          {#each styleOptions as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>

      <div
        class="model-chip"
        title={whisperModel === "tiny.en"
          ? "Tiny is fast but error-prone. Switch to Whisper small.en in Settings for better quality."
          : ""}
      >
        <span>STT</span>
        <strong>{modelLabel(settings.options.whisper_models, whisperModel)}</strong>
        <small>{formatMs(sttMs)}</small>
      </div>
      <div class="model-chip">
        <span>Cleanup</span>
        <strong>{cleanupModel}</strong>
        <small>{llmDisplay()}</small>
      </div>
      <div class="model-chip">
        <span>Memory</span>
        <strong>{embeddingModel}</strong>
        <small>{embeddingRatio(displayStats)} embedded</small>
      </div>
    </section>

    <section class="workspace-grid">
      <section class="panel live-panel" aria-labelledby="live-heading">
        <div class="panel-head">
          <div>
            <span class="section-kicker">Current</span>
            <h2 id="live-heading">Transcript</h2>
          </div>
          <span class="subtle-text">Total {formatMs(totalMs)}</span>
        </div>

        {#if dictationStore.lastError}
          <pre class="error-block">{dictationStore.lastError}</pre>
        {:else if liveText}
          <div class="transcript-box">{liveText}</div>
        {:else}
          <div class="empty-panel">
            Hold {dictationHotkey} to dictate, or tap {toggleDictationHotkey} to toggle hands-free.
          </div>
        {/if}

        <dl class="telemetry-row">
          <div>
            <dt>Captured</dt>
            <dd>{formatSeconds(captureSeconds)}</dd>
          </div>
          <div>
            <dt>STT</dt>
            <dd>{formatMs(sttMs)}</dd>
          </div>
          <div>
            <dt>LLM</dt>
            <dd>{llmDisplay()}</dd>
          </div>
          <div>
            <dt>Peak</dt>
            <dd>{peakLevel}</dd>
          </div>
        </dl>
      </section>

      <aside class="panel process-panel" aria-labelledby="process-heading">
        <div class="panel-head">
          <div>
            <span class="section-kicker">Process</span>
            <h2 id="process-heading">Pipeline</h2>
          </div>
        </div>

        <ol class="pipeline-list">
          <li>
            <span class="step-icon"><Icon name="mic" size={14} /></span>
            <div>
              <strong>Capture</strong>
              <small>{formatSeconds(captureSeconds)} audio</small>
            </div>
            <code>{peakLevel}</code>
          </li>
          <li>
            <span class="step-icon"><Icon name="brain" size={14} /></span>
            <div>
              <strong>Clean up</strong>
              <small>{styleLabel(settings.style)} via {cleanupModel}</small>
            </div>
            <code>{llmDisplay()}</code>
          </li>
          <li>
            <span class="step-icon"><Icon name="history" size={14} /></span>
            <div>
              <strong>Index</strong>
              <small>{embeddingModel}</small>
            </div>
            <code>{embeddingRatio(displayStats)}</code>
          </li>
          <li>
            <span class="step-icon"><Icon name="zap" size={14} /></span>
            <div>
              <strong>Paste</strong>
              <small>{selectedEntry ? appLabel(selectedEntry) : "Focused app"}</small>
            </div>
            <code>local</code>
          </li>
        </ol>
      </aside>
    </section>

    <section class="history-grid">
      <section class="panel history-panel" aria-labelledby="history-heading">
        <div class="panel-head history-head">
          <div>
            <span class="section-kicker">History</span>
            <h2 id="history-heading">Recent transcripts</h2>
          </div>
          <button class="quiet-button" type="button" onclick={() => void loadHistory()}>
            {historyLoading ? "Loading" : "Refresh"}
          </button>
        </div>

        {#if historyError}
          <div class="inline-error">{historyError}</div>
        {/if}

        {#if displayHistory.length}
          <div class="history-table" role="list" aria-label="Recent transcript history">
            <div class="history-row table-head" aria-hidden="true">
              <span>Date</span>
              <span>Latency</span>
              <span>Style</span>
              <span>Transcript</span>
            </div>
            {#each displayHistory as entry (entry.id)}
              <button
                class="history-row"
                class:selected={selectedEntry?.id === entry.id}
                type="button"
                onclick={() => (selectedHistoryId = entry.id)}
              >
                <span>{formatDate(entry.captured_at)}</span>
                <span>{formatMs(entry.duration_ms + entry.llm_ms)}</span>
                <span>{styleLabel(entry.style)}</span>
                <span>{preview(entry.formatted)}</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty-panel">
            {historyLoading ? "Loading history..." : "No saved transcripts yet."}
          </div>
        {/if}
      </section>

      <section class="panel detail-panel" aria-labelledby="detail-heading">
        <div class="panel-head">
          <div>
            <span class="section-kicker">Open</span>
            <h2 id="detail-heading">Transcript detail</h2>
          </div>
          {#if selectedEntry}
            <span class="subtle-text">{formatDate(selectedEntry.captured_at)}</span>
          {/if}
        </div>

        {#if selectedEntry}
          <dl class="detail-meta">
            <div>
              <dt>App</dt>
              <dd>{appLabel(selectedEntry)}</dd>
            </div>
            <div>
              <dt>Total</dt>
              <dd>{formatMs(selectedEntry.duration_ms + selectedEntry.llm_ms)}</dd>
            </div>
            <div>
              <dt>STT</dt>
              <dd>{formatMs(selectedEntry.duration_ms)}</dd>
            </div>
            <div>
              <dt>LLM</dt>
              <dd>{formatMs(selectedEntry.llm_ms)}</dd>
            </div>
          </dl>

          <article class="detail-transcript">{selectedEntry.formatted}</article>

          <details class="raw-details">
            <summary>Raw transcript</summary>
            <p>{selectedEntry.raw}</p>
          </details>
        {:else}
          <div class="empty-panel">Select a history row to open a transcript.</div>
        {/if}
      </section>
    </section>
  </main>
{/if}
