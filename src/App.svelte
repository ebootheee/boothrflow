<script lang="ts">
  import { onMount } from "svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import ListenPill from "$lib/components/ListenPill.svelte";
  import { isTauri } from "$lib/services/platform";
  import type { Style } from "$lib/services/styles";
  import { dictationStore } from "$lib/state/dictation.svelte";
  import { settings } from "$lib/state/settings.svelte";

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

  const styleOptions: Array<{ value: Style; label: string; icon: IconName }> = [
    { value: "casual", label: "Casual", icon: "pen" },
    { value: "formal", label: "Formal", icon: "book" },
    { value: "very-casual", label: "Very casual", icon: "sparkles" },
    { value: "excited", label: "Excited", icon: "zap" },
    { value: "raw", label: "Raw", icon: "audio" },
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

  const displayHistory = $derived(
    historyEntries.length ? historyEntries : inDesktop ? [] : demoHistory,
  );
  const selectedEntry = $derived(
    displayHistory.find((entry) => entry.id === selectedHistoryId) ?? displayHistory[0] ?? null,
  );
  const displayStats = $derived(historyStats ?? (inDesktop ? null : demoStats));
  const liveText = $derived(dictationStore.lastResult?.text ?? selectedEntry?.formatted ?? "");
  const sttMs = $derived(selectedEntry?.duration_ms ?? dictationStore.lastResult?.duration_ms ?? 0);
  const llmMs = $derived(selectedEntry?.llm_ms ?? 0);
  const totalMs = $derived(sttMs + llmMs);
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
    settings.style === "raw" ? "Bypass" : "Qwen 2.5 / OpenAI-compatible",
  );
  const embeddingModel = $derived(displayStats?.embed_model ?? "Off");

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
    void dictationStore.attach();
    void loadHistory();
  });
</script>

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
        <kbd><Icon name="command" size={13} /> Ctrl + Win</kbd>
        <kbd><Icon name="history" size={13} /> Alt + Win + H</kbd>
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

      <div class="model-chip">
        <span>STT</span>
        <strong>Whisper tiny.en</strong>
        <small>{formatMs(sttMs)}</small>
      </div>
      <div class="model-chip">
        <span>Cleanup</span>
        <strong>{cleanupModel}</strong>
        <small>{llmMs ? formatMs(llmMs) : settings.style === "raw" ? "off" : "pending"}</small>
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
          <div class="empty-panel">Hold Ctrl + Win to dictate.</div>
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
            <dd>{llmMs ? formatMs(llmMs) : "0 ms"}</dd>
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
            <code>{llmMs ? formatMs(llmMs) : "0 ms"}</code>
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
