<script lang="ts">
  import { onMount } from "svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import ListenPill from "$lib/components/ListenPill.svelte";
  import {
    dictationHotkeyLabel,
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

  const styleOptions: Array<{ value: Style; label: string; icon: IconName }> = [
    { value: "casual", label: "Casual", icon: "pen" },
    { value: "formal", label: "Formal", icon: "book" },
    { value: "very-casual", label: "Very casual", icon: "sparkles" },
    { value: "excited", label: "Excited", icon: "zap" },
    { value: "raw", label: "Raw", icon: "audio" },
    // Captain's Log: Star-Trek-style log entry (computed stardate prefix +
    // formal 24th-century rewrite). Same code path as the other styles —
    // just a different prompt branch in the cleanup backend.
    { value: "captains-log", label: "Captain's Log", icon: "radio" },
  ];

  const demoNow = new Date("2026-04-28T15:42:00-06:00").toISOString();
  const demoHistory: HistoryEntry[] = [
    {
      id: 3,
      captured_at: demoNow,
      raw: "okay wow that was pretty impressive let's see if I speak a little faster",
      formatted:
        "Okay wow, that was pretty impressive. Let's see if I speak a little faster, if this still lands cleanly. The cleanup pass kept my tone, tightened the sentence breaks, and pasted it without making me babysit the output.",
      style: "casual",
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
      style: "formal",
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
      style: "casual",
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
  const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  let micAvailable = $state<boolean | null>(null);
  let permissionsDismissed = $state(false);
  let settingsOpen = $state(false);
  let settingsExportJson = $state("");
  let settingsImportJson = $state("");
  let capturingHotkey = $state<keyof HotkeySettings | null>(null);

  // Sidebar nav for the Settings modal. Persists across opens so the user
  // doesn't have to re-find their last section. Casper's PR #2 spec'd
  // these five sections; we map our existing fields onto them.
  type SettingsSection = "general" | "llm" | "whisper" | "history" | "about";
  let activeSettingsSection = $state<SettingsSection>("general");

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

  type PermissionPane = "microphone" | "accessibility" | "input_monitoring";
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

  onMount(() => {
    void settings.load();
    void dictationStore.attach();
    void loadHistory();
    void probeMicrophone();
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
                ><Icon name="mic" size={14} /> Whisper</button
              >
              <button
                type="button"
                class="settings-nav-item"
                class:active={activeSettingsSection === "history"}
                onclick={() => (activeSettingsSection = "history")}
                ><Icon name="database" size={14} /> History</button
              >
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
                      <h3>Default style</h3>
                    </div>
                  </div>
                  <label class="settings-field">
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
                    Skips the LLM cleanup pass entirely — useful when you're dictating sensitive
                    content you don't want any model (local or cloud) to see. Behaves like selecting <em
                      >Raw</em
                    > style on local-only setups; matters most when a cloud BYOK endpoint is configured.
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
                      boothrflow needs three permissions on macOS. Click each to open the relevant
                      pane in System Settings, toggle the switch, then relaunch.
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

                  <label class="settings-field">
                    <span>Whisper model</span>
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
                      >Appended to Whisper's initial prompt — biases recognition toward these terms.</small
                    >
                  </label>
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
