<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "$lib/services/platform";

  type LlmSettings = {
    endpoint: string;
    model: string;
    apiKey: string;
    disabled: boolean;
  };
  type AppSettings = { llm: LlmSettings };

  type LlmTestResult = {
    ok: boolean;
    latencyMs: number;
    status: number | null;
    error: string | null;
  };

  type Section = "general" | "llm" | "whisper" | "history" | "about";

  type HistoryStats = {
    total_entries: number;
    embedded_entries: number;
    db_path: string;
    embed_endpoint: string | null;
    embed_model: string | null;
  };

  const { onClose }: { onClose: () => void } = $props();

  const DEFAULT_ENDPOINT = "http://localhost:11434/v1/chat/completions";
  const DEFAULT_MODEL = "qwen2.5:1.5b";

  const PRESETS: Array<{ label: string; endpoint: string; hint: string }> = [
    {
      label: "Ollama",
      endpoint: "http://localhost:11434/v1/chat/completions",
      hint: "Local. ollama pull <model>",
    },
    {
      label: "llama.cpp",
      endpoint: "http://localhost:8080/v1/chat/completions",
      hint: "Local llama-server. Bring your own GGUF",
    },
    {
      label: "OpenAI",
      endpoint: "https://api.openai.com/v1/chat/completions",
      hint: "Cloud. Set API key. e.g. gpt-4o-mini",
    },
    {
      label: "OpenRouter",
      endpoint: "https://openrouter.ai/api/v1/chat/completions",
      hint: "Cloud. Set API key. e.g. anthropic/claude-haiku-4-5",
    },
  ];

  let section = $state<Section>("llm");
  let settings = $state<AppSettings>({
    llm: { endpoint: "", model: "", apiKey: "", disabled: false },
  });
  let loaded = $state(false);
  let saving = $state(false);
  let savedAt = $state<number | null>(null);
  let testing = $state(false);
  let testResult = $state<LlmTestResult | null>(null);

  // Status read-outs for the General/Whisper/History sections.
  let whisperModel = $state<string | null>(null);
  let historyStats = $state<HistoryStats | null>(null);
  let historyClearing = $state(false);
  let autostartEnabled = $state<boolean | null>(null);
  let autostartBusy = $state(false);
  const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  // We mirror what the daemon will pick at startup so users see real values,
  // not blanks: empty endpoint/model fall back to defaults on the Rust side.
  let effectiveEndpoint = $derived(settings.llm.endpoint.trim() || DEFAULT_ENDPOINT);
  let effectiveModel = $derived(settings.llm.model.trim() || DEFAULT_MODEL);

  onMount(async () => {
    if (!isTauri()) {
      loaded = true;
      return;
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const got = await invoke<AppSettings>("app_settings_get");
      // Backend uses serde camelCase, but missing fields default to "".
      settings = {
        llm: {
          endpoint: got?.llm?.endpoint ?? "",
          model: got?.llm?.model ?? "",
          apiKey: got?.llm?.apiKey ?? "",
          disabled: got?.llm?.disabled ?? false,
        },
      };
    } catch (e) {
      console.warn("app_settings_get failed:", e);
    } finally {
      loaded = true;
    }

    // Best-effort status readouts for other sections. Each is independent;
    // a failure in one shouldn't block the others.
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      whisperModel = await invoke<string>("whisper_model_name");
    } catch (e) {
      console.warn("whisper_model_name failed:", e);
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      historyStats = await invoke<HistoryStats>("history_stats");
    } catch (e) {
      console.warn("history_stats failed:", e);
    }
    try {
      const { isEnabled } = await import("@tauri-apps/plugin-autostart");
      autostartEnabled = await isEnabled();
    } catch (e) {
      console.warn("autostart isEnabled failed:", e);
    }
  });

  async function toggleAutostart() {
    if (!isTauri()) return;
    autostartBusy = true;
    try {
      const { enable, disable, isEnabled } = await import("@tauri-apps/plugin-autostart");
      if (autostartEnabled) {
        await disable();
      } else {
        await enable();
      }
      autostartEnabled = await isEnabled();
    } catch (e) {
      alert(`Autostart failed: ${e}`);
    } finally {
      autostartBusy = false;
    }
  }

  async function clearHistory() {
    if (!isTauri()) return;
    if (!confirm("Clear all dictation history? This can't be undone.")) return;
    historyClearing = true;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("history_clear");
      historyStats = await invoke<HistoryStats>("history_stats");
    } catch (e) {
      alert(`Clear failed: ${e}`);
    } finally {
      historyClearing = false;
    }
  }

  async function revealDataDir(path: string | null) {
    if (!isTauri() || !path) return;
    try {
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      await revealItemInDir(path);
    } catch (e) {
      console.warn("reveal failed:", e);
    }
  }

  async function restartApp() {
    if (!isTauri()) return;
    if (!confirm("Restart boothrflow now to apply settings?")) return;
    try {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      alert(`Restart failed: ${e}`);
    }
  }

  async function save() {
    if (!isTauri()) return;
    saving = true;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("app_settings_save", { settings });
      savedAt = Date.now();
    } catch (e) {
      console.error("app_settings_save failed:", e);
      alert(`Save failed: ${e}`);
    } finally {
      saving = false;
    }
  }

  async function testConnection() {
    if (!isTauri()) return;
    testing = true;
    testResult = null;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<{
        ok: boolean;
        latency_ms: number;
        status: number | null;
        error: string | null;
      }>("llm_test_connection", {
        endpoint: effectiveEndpoint,
        model: effectiveModel,
        apiKey: settings.llm.apiKey || null,
      });
      testResult = {
        ok: result.ok,
        latencyMs: result.latency_ms,
        status: result.status,
        error: result.error,
      };
    } catch (e) {
      testResult = {
        ok: false,
        latencyMs: 0,
        status: null,
        error: String(e),
      };
    } finally {
      testing = false;
    }
  }

  function applyPreset(endpoint: string) {
    settings.llm.endpoint = endpoint;
    testResult = null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="overlay" onclick={onClose} role="presentation">
  <div
    class="panel"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-label="Settings"
  >
    <aside class="rail">
      <div class="rail-title">Settings</div>
      <nav>
        <button class:active={section === "general"} onclick={() => (section = "general")}
          >General</button
        >
        <button class:active={section === "llm"} onclick={() => (section = "llm")}>LLM</button>
        <button class:active={section === "whisper"} onclick={() => (section = "whisper")}
          >Whisper</button
        >
        <button class:active={section === "history"} onclick={() => (section = "history")}
          >History</button
        >
        <button class:active={section === "about"} onclick={() => (section = "about")}>About</button
        >
      </nav>
      <button class="close" onclick={onClose} aria-label="Close settings">×</button>
    </aside>

    <main class="content">
      {#if section === "llm"}
        <header>
          <h2>LLM cleanup</h2>
          <p class="sub">
            boothrflow sends the raw transcript to an OpenAI-compatible chat endpoint for
            punctuation + capitalization. Any server that speaks
            <code>/v1/chat/completions</code> works.
          </p>
        </header>

        {#if !loaded}
          <div class="hint">Loading…</div>
        {:else}
          <section class="card">
            <div class="row">
              <label>
                <span>Endpoint URL</span>
                <input
                  type="url"
                  bind:value={settings.llm.endpoint}
                  placeholder={DEFAULT_ENDPOINT}
                  spellcheck="false"
                  autocomplete="off"
                />
              </label>
              <div class="presets">
                {#each PRESETS as p (p.label)}
                  <button
                    type="button"
                    class="preset"
                    onclick={() => applyPreset(p.endpoint)}
                    title={p.hint}>{p.label}</button
                  >
                {/each}
              </div>
            </div>

            <div class="row">
              <label>
                <span>Model</span>
                <input
                  type="text"
                  bind:value={settings.llm.model}
                  placeholder={DEFAULT_MODEL}
                  spellcheck="false"
                  autocomplete="off"
                />
              </label>
            </div>

            <div class="row">
              <label>
                <span>API key (optional)</span>
                <input
                  type="password"
                  bind:value={settings.llm.apiKey}
                  placeholder="sk-… (only for cloud providers)"
                  autocomplete="off"
                />
              </label>
            </div>

            <div class="row toggle">
              <label class="toggle-label">
                <input type="checkbox" bind:checked={settings.llm.disabled} />
                <span>Disable LLM cleanup (paste raw transcript)</span>
              </label>
            </div>

            <div class="effective">
              Effective: <code>{effectiveEndpoint}</code> ·
              <code>{effectiveModel}</code>
              {#if settings.llm.apiKey}· <span class="key-set">key set</span>{/if}
            </div>
          </section>

          <div class="actions">
            <button
              class="secondary"
              onclick={testConnection}
              disabled={testing || settings.llm.disabled}
              title="Ping the endpoint with a 1-token request"
            >
              {testing ? "Testing…" : "Test connection"}
            </button>
            <button class="primary" onclick={save} disabled={saving}>
              {saving ? "Saving…" : "Save"}
            </button>
          </div>

          {#if testResult}
            <div class="test-result" class:ok={testResult.ok} class:fail={!testResult.ok}>
              {#if testResult.ok}
                ✓ Reached endpoint in {testResult.latencyMs}ms
                {#if testResult.status}(HTTP {testResult.status}){/if}
              {:else}
                ✗ Failed{#if testResult.status}
                  (HTTP {testResult.status}){/if}:
                {testResult.error ?? "unknown error"}
              {/if}
            </div>
          {/if}

          {#if savedAt}
            <div class="restart-banner">
              Saved. <strong>Restart boothrflow</strong> to apply LLM changes — the session daemon reads
              settings once at startup.
            </div>
          {/if}
        {/if}
      {:else if section === "general"}
        <header>
          <h2>General</h2>
          <p class="sub">App-wide preferences.</p>
        </header>

        <section class="card">
          <div class="kv">
            <span class="k">Push-to-talk</span>
            <span class="v"><kbd>Ctrl + Cmd</kbd></span>
          </div>
          <div class="kv">
            <span class="k">Tap-to-toggle</span>
            <span class="v"><kbd>Ctrl + Option + Space</kbd></span>
          </div>
          <div class="kv">
            <span class="k">Quick-paste palette</span>
            <span class="v"><kbd>Option + Cmd + H</kbd></span>
          </div>
          <div class="kv-note">
            Hotkey rebinding lands in a follow-up. For now, edit
            <code>src-tauri/src/hotkey/</code> and rebuild.
          </div>
        </section>

        <section class="card">
          <div class="row toggle">
            <label class="toggle-label">
              <input
                type="checkbox"
                checked={autostartEnabled === true}
                disabled={autostartBusy || autostartEnabled === null}
                onchange={toggleAutostart}
              />
              <span>
                Launch boothrflow at login
                {#if autostartEnabled === null}
                  <small>· checking…</small>
                {/if}
              </span>
            </label>
          </div>
        </section>

        <div class="actions">
          <button class="secondary" onclick={restartApp}>Restart app</button>
        </div>
      {:else if section === "whisper"}
        <header>
          <h2>Whisper (speech-to-text)</h2>
          <p class="sub">Local STT runs on-device. No audio leaves your machine.</p>
        </header>

        <section class="card">
          <div class="kv">
            <span class="k">Active model</span>
            <span class="v">
              <code>{whisperModel ?? "loading…"}</code>
            </span>
          </div>
          <div class="kv">
            <span class="k">Backend</span>
            <span class="v">
              {#if isMac}
                <span class="badge ok">Metal (Apple Silicon GPU)</span>
              {:else}
                <span class="badge">CPU</span>
              {/if}
            </span>
          </div>
          <div class="kv-note">
            Switch models by setting
            <code>BOOTHRFLOW_WHISPER_MODEL_FILE=ggml-small.en.bin</code>
            (or <code>base</code>/<code>medium</code>) and restarting. In-app picker + auto-download
            lands in a follow-up.
          </div>
        </section>
      {:else if section === "history"}
        <header>
          <h2>History</h2>
          <p class="sub">
            Every dictation is stored locally in SQLite for the quick-paste palette and (soon)
            semantic search.
          </p>
        </header>

        <section class="card">
          <div class="kv">
            <span class="k">Total entries</span>
            <span class="v">
              {historyStats?.total_entries ?? "—"}
            </span>
          </div>
          <div class="kv">
            <span class="k">Embedded</span>
            <span class="v">
              {historyStats?.embedded_entries ?? "—"}
              {#if historyStats && historyStats.total_entries > 0}
                <small>
                  ({Math.round(
                    (historyStats.embedded_entries / historyStats.total_entries) * 100,
                  )}%)
                </small>
              {/if}
            </span>
          </div>
          <div class="kv">
            <span class="k">Embedding model</span>
            <span class="v">
              <code>{historyStats?.embed_model ?? "—"}</code>
            </span>
          </div>
          <div class="kv">
            <span class="k">Database</span>
            <span class="v path" title={historyStats?.db_path ?? ""}>
              <code>{historyStats?.db_path ?? "—"}</code>
            </span>
          </div>
        </section>

        <div class="actions">
          <button
            class="secondary"
            disabled={!historyStats?.db_path}
            onclick={() => revealDataDir(historyStats?.db_path ?? null)}
          >
            Reveal in Finder
          </button>
          <button class="danger" disabled={historyClearing || !historyStats} onclick={clearHistory}>
            {historyClearing ? "Clearing…" : "Clear history"}
          </button>
        </div>
      {:else if section === "about"}
        <header>
          <h2>About</h2>
          <p class="sub">Local-first voice dictation.</p>
        </header>

        <section class="card">
          <div class="kv">
            <span class="k">Version</span>
            <span class="v"><code>0.0.0</code> · pre-alpha</span>
          </div>
          <div class="kv">
            <span class="k">Repository</span>
            <span class="v">
              <a href="https://github.com/ebootheee/boothrflow" target="_blank" rel="noreferrer"
                >github.com/ebootheee/boothrflow</a
              >
            </span>
          </div>
          <div class="kv">
            <span class="k">License</span>
            <span class="v">Apache-2.0</span>
          </div>
          <div class="kv-note">
            Built with Tauri 2 + Rust + Svelte 5. Whisper.cpp for STT, OpenAI-compatible chat for
            cleanup. No telemetry.
          </div>
        </section>
      {/if}
    </main>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(23, 32, 36, 0.45);
    display: grid;
    place-items: center;
    z-index: 1000;
  }
  .panel {
    width: min(1040px, 96vw);
    height: min(820px, 94vh);
    background: var(--paper, #fff);
    border-radius: 14px;
    box-shadow:
      0 20px 60px rgba(23, 32, 36, 0.25),
      0 4px 12px rgba(23, 32, 36, 0.08);
    display: grid;
    grid-template-columns: 168px 1fr;
    overflow: hidden;
    color: var(--ink, #172024);
  }
  .rail {
    background: var(--muted, #eef3f1);
    border-right: 1px solid var(--line, #d7e0dd);
    display: flex;
    flex-direction: column;
    padding: 18px 14px 14px;
    position: relative;
  }
  .rail-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--subtle, #637176);
    margin-bottom: 12px;
  }
  .rail nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .rail nav button {
    text-align: left;
    background: transparent;
    border: 0;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 14px;
    color: var(--ink, #172024);
    cursor: pointer;
    font-weight: 500;
  }
  .rail nav button:hover {
    background: rgba(23, 32, 36, 0.05);
  }
  .rail nav button.active {
    background: var(--paper, #fff);
    box-shadow: 0 1px 0 rgba(23, 32, 36, 0.06);
  }
  .close {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    border: 0;
    background: transparent;
    font-size: 18px;
    line-height: 1;
    color: var(--subtle, #637176);
    cursor: pointer;
  }
  .close:hover {
    background: rgba(23, 32, 36, 0.06);
  }
  .content {
    padding: 22px 24px 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  .content > header {
    margin-bottom: 2px;
  }
  header h2 {
    margin: 0 0 2px 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .sub {
    margin: 0;
    color: var(--subtle, #637176);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .sub code,
  .empty code,
  .effective code {
    background: var(--muted, #eef3f1);
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 12px;
  }
  .card {
    border: 1px solid var(--line, #d7e0dd);
    border-radius: 10px;
    padding: 14px 14px 10px;
    background: var(--paper, #fff);
  }
  .row {
    margin-bottom: 12px;
  }
  .row:last-child {
    margin-bottom: 0;
  }
  .row label {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .row label > span {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--subtle, #637176);
    letter-spacing: 0.01em;
  }
  .row input[type="text"],
  .row input[type="url"],
  .row input[type="password"] {
    border: 1px solid var(--line, #d7e0dd);
    background: var(--paper, #fff);
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 13px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    color: var(--ink, #172024);
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .row input:focus {
    border-color: var(--brand, #d95e54);
    box-shadow: 0 0 0 3px rgba(217, 94, 84, 0.15);
  }
  .presets {
    display: flex;
    flex-wrap: nowrap;
    gap: 6px;
    margin-top: 8px;
    overflow-x: auto;
    scrollbar-width: thin;
  }
  .preset {
    border: 1px solid var(--line, #d7e0dd);
    background: var(--paper, #fff);
    border-radius: 999px;
    padding: 4px 10px;
    font-size: 12px;
    color: var(--ink, #172024);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .preset:hover {
    border-color: var(--brand, #d95e54);
    color: var(--brand, #d95e54);
  }
  .toggle {
    margin-bottom: 4px;
  }
  .toggle-label {
    flex-direction: row !important;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--ink, #172024);
  }
  .toggle-label input {
    width: auto !important;
  }
  .effective {
    font-size: 12px;
    color: var(--subtle, #637176);
    border-top: 1px solid var(--line-soft, #e8eeec);
    padding-top: 10px;
    margin-top: 4px;
  }
  .key-set {
    color: var(--mint, #208f83);
    font-weight: 600;
  }
  .actions {
    display: flex;
    flex-wrap: nowrap;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 4px;
  }
  .actions button {
    padding: 7px 12px;
    border-radius: 8px;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
    white-space: nowrap;
    line-height: 1.2;
  }
  .actions .primary {
    background: var(--brand, #d95e54);
    color: #fff;
    border-color: var(--brand, #d95e54);
  }
  .actions .primary:hover {
    filter: brightness(0.95);
  }
  .actions .primary:disabled,
  .actions .secondary:disabled,
  .actions .danger:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .actions .secondary {
    background: var(--paper, #fff);
    color: var(--ink, #172024);
    border-color: var(--line, #d7e0dd);
  }
  .actions .secondary:hover:not(:disabled) {
    background: var(--muted, #eef3f1);
  }
  .actions .danger {
    background: var(--paper, #fff);
    color: #b03028;
    border-color: rgba(176, 48, 40, 0.35);
  }
  .actions .danger:hover:not(:disabled) {
    background: #fdecea;
  }
  /* Key-value rows used by General/Whisper/History/About sections. */
  .kv {
    display: grid;
    grid-template-columns: 140px 1fr;
    align-items: center;
    gap: 12px;
    padding: 7px 0;
    border-bottom: 1px solid var(--line-soft, #e8eeec);
    font-size: 13px;
    min-width: 0;
  }
  .kv:last-of-type {
    border-bottom: 0;
  }
  .kv .k {
    color: var(--subtle, #637176);
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.01em;
    text-transform: uppercase;
  }
  .kv .v {
    color: var(--ink, #172024);
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .kv .v code {
    background: var(--muted, #eef3f1);
    padding: 2px 6px;
    border-radius: 5px;
    font-size: 12px;
  }
  .kv .v small {
    color: var(--subtle, #637176);
    font-size: 11.5px;
  }
  .kv .v.path code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
    display: inline-block;
  }
  .kv .v kbd {
    background: var(--paper, #fff);
    border: 1px solid var(--line, #d7e0dd);
    border-bottom-width: 2px;
    border-radius: 5px;
    padding: 2px 6px;
    font-size: 11.5px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    color: var(--ink, #172024);
  }
  .kv-note {
    font-size: 12px;
    color: var(--subtle, #637176);
    line-height: 1.5;
    padding: 10px 0 2px;
    border-top: 1px dashed var(--line, #d7e0dd);
    margin-top: 4px;
  }
  .kv-note code {
    background: var(--muted, #eef3f1);
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 11.5px;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
    background: var(--muted, #eef3f1);
    color: var(--ink, #172024);
  }
  .badge.ok {
    background: var(--mint-soft, #e5f5f1);
    color: var(--mint, #208f83);
  }
  a {
    color: var(--brand, #d95e54);
    text-decoration: none;
  }
  a:hover {
    text-decoration: underline;
  }
  .test-result {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: 8px;
    font-size: 13px;
    line-height: 1.45;
    word-break: break-word;
  }
  .test-result.ok {
    background: var(--mint-soft, #e5f5f1);
    color: var(--mint, #208f83);
  }
  .test-result.fail {
    background: #fdecea;
    color: #b03028;
  }
  .restart-banner {
    margin-top: 12px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--brand-soft, #fff3ef);
    color: var(--ink, #172024);
    border: 1px solid rgba(217, 94, 84, 0.25);
    font-size: 13px;
  }
  .empty {
    color: var(--subtle, #637176);
    font-size: 13px;
    border: 1px dashed var(--line, #d7e0dd);
    border-radius: 10px;
    padding: 18px;
    line-height: 1.55;
  }
  .hint {
    color: var(--subtle, #637176);
    font-size: 13px;
  }
</style>
