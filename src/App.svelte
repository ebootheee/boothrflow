<script lang="ts">
  import { onMount } from "svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import ListenPill from "$lib/components/ListenPill.svelte";
  import { isTauri } from "$lib/services/platform";
  import type { Style } from "$lib/services/styles";
  import { dictationStore } from "$lib/state/dictation.svelte";
  import { settings } from "$lib/state/settings.svelte";

  // Hash-based window routing. The Rust side opens secondary windows with
  // url=index.html#<label>:
  //   #listen-pill -> ListenPill (always-on-top dictation indicator)
  //   #quick-paste -> QuickPasteApp (Alt+Meta+H history palette)
  // Everything else is the main settings UI.
  const hash = typeof window !== "undefined" ? window.location.hash : "";
  const isPill = hash === "#listen-pill";
  const isQuickPaste = hash === "#quick-paste";
  const inDesktop = isTauri();

  const styleOptions: Array<{
    value: Style;
    label: string;
    tone: string;
    icon: IconName;
  }> = [
    { value: "casual", label: "Casual", tone: "Natural", icon: "pen" },
    { value: "formal", label: "Formal", tone: "Polished", icon: "book" },
    { value: "very-casual", label: "Very casual", tone: "Warm", icon: "sparkles" },
    { value: "excited", label: "Excited", tone: "Bright", icon: "zap" },
    { value: "raw", label: "Raw", tone: "Untouched", icon: "audio" },
  ];

  const demoResult = {
    text: "Okay wow, that was pretty impressive. Let's see if I speak a little faster, if this still lands cleanly. The cleanup pass kept my tone, tightened the sentence breaks, and pasted it without making me babysit the output.",
    language: "en",
    duration_ms: 7854,
  };

  const demoSummary = {
    frames: 995,
    samples: 509_440,
    seconds: 31.84,
    peak_dbfs: -56.9,
  };

  const demoHistory = [
    {
      text: "Draft the launch note in a casual voice and keep the technical claims crisp.",
      language: "en",
      duration_ms: 642,
    },
    {
      text: "Add Connor, Sophie, and Max to the dictionary so their names stop getting corrected.",
      language: "en",
      duration_ms: 811,
    },
    {
      text: "Make this paragraph tighter, then paste it into the active document.",
      language: "en",
      duration_ms: 704,
    },
  ];

  const displayResult = $derived(dictationStore.lastResult ?? (inDesktop ? null : demoResult));
  const displaySummary = $derived(dictationStore.lastSummary ?? (inDesktop ? null : demoSummary));
  const displayHistory = $derived(
    dictationStore.history.length > 1
      ? dictationStore.history.slice(1)
      : inDesktop
        ? []
        : demoHistory,
  );

  const statusLabel = $derived(
    dictationStore.status === "listening"
      ? "Listening"
      : dictationStore.status === "processing"
        ? "Cleaning"
        : "Ready",
  );

  const statusDetail = $derived(
    dictationStore.status === "listening"
      ? "Voice capture active"
      : dictationStore.status === "processing"
        ? "Formatting locally"
        : "Hold shortcut in any app",
  );

  const whisperDuration = $derived(
    displayResult ? `${(displayResult.duration_ms / 1000).toFixed(2)}s` : "Idle",
  );

  const capturedDuration = $derived(
    displaySummary ? `${displaySummary.seconds.toFixed(1)}s` : "Awaiting audio",
  );

  const peakLevel = $derived(
    displaySummary ? `${displaySummary.peak_dbfs.toFixed(1)} dBFS` : "No signal",
  );

  function selectStyle(style: Style) {
    settings.style = style;
  }

  onMount(() => {
    void dictationStore.attach();
  });
</script>

{#if isPill}
  <ListenPill listening={true} label="Listening" />
{:else if isQuickPaste}
  {#await import("$lib/quickpaste/QuickPasteApp.svelte") then m}
    {@const QuickPasteApp = m.default}
    <QuickPasteApp />
  {/await}
{:else}
  <main class="app-shell">
    <section class="topbar" aria-label="Application status">
      <div class="brand-lockup">
        <div class="brand-mark" aria-hidden="true">
          <Icon name="mic" size={20} strokeWidth={2.4} />
        </div>
        <div>
          <h1>boothrflow</h1>
          <p>Local-first voice dictation</p>
        </div>
      </div>

      <div class="status-cluster" data-status={dictationStore.status}>
        <span class="status-dot" aria-hidden="true"></span>
        <div>
          <strong>{statusLabel}</strong>
          <span>{statusDetail}</span>
        </div>
      </div>
    </section>

    {#if dictationStore.modelMissing}
      <section class="model-alert" aria-live="polite">
        <Icon name="lock" size={18} />
        <div>
          <h2>Whisper model not loaded</h2>
          <pre>{dictationStore.modelMissing}</pre>
        </div>
      </section>
    {/if}

    <section class="hero-panel" aria-label="Dictation workspace">
      <div class="hero-copy">
        <span class="eyebrow">Desktop dictation</span>
        <h2>Speak anywhere. Paste cleanly.</h2>
        <p>
          Fast local capture, lightweight cleanup, and a focused review surface for every dictation.
        </p>

        <div class="shortcut-card" aria-label="Current dictation shortcut">
          <span>Shortcut</span>
          <kbd><Icon name="command" size={15} /> Ctrl + Win</kbd>
        </div>
      </div>

      <div class="capture-panel" data-status={dictationStore.status}>
        <div class="orb-wrap" aria-hidden="true">
          <div class="capture-orb">
            <Icon name="mic" size={38} strokeWidth={2.2} />
          </div>
          <span class="ring ring-one"></span>
          <span class="ring ring-two"></span>
        </div>
        <div class="capture-copy">
          <strong>{statusLabel}</strong>
          <span>{settings.style.replace("-", " ")} style</span>
        </div>
        <div class="waveform" aria-hidden="true">
          <span></span>
          <span></span>
          <span></span>
          <span></span>
          <span></span>
          <span></span>
          <span></span>
        </div>
      </div>
    </section>

    <section class="content-grid">
      <div class="primary-column">
        <section class="surface style-surface" aria-labelledby="style-heading">
          <div class="section-heading">
            <div>
              <span class="eyebrow">Tone</span>
              <h2 id="style-heading">Style preset</h2>
            </div>
            <span class="local-badge"><Icon name="lock" size={14} /> Local</span>
          </div>

          <div class="style-grid" role="radiogroup" aria-label="Dictation style preset">
            {#each styleOptions as option (option.value)}
              <button
                class:active={settings.style === option.value}
                type="button"
                role="radio"
                aria-checked={settings.style === option.value}
                onclick={() => selectStyle(option.value)}
              >
                <Icon name={option.icon} size={17} strokeWidth={2.2} />
                <span>
                  <strong>{option.label}</strong>
                  <small>{option.tone}</small>
                </span>
                {#if settings.style === option.value}
                  <Icon class="style-check" name="check" size={16} strokeWidth={2.4} />
                {/if}
              </button>
            {/each}
          </div>
        </section>

        <section class="surface transcript-surface" aria-labelledby="transcript-heading">
          <div class="section-heading">
            <div>
              <span class="eyebrow">Output</span>
              <h2 id="transcript-heading">Live transcript</h2>
            </div>
            <span class="status-chip" data-status={dictationStore.status}>
              <Icon name="radio" size={14} />
              {dictationStore.status}
            </span>
          </div>

          {#if dictationStore.lastError}
            <pre class="error-block">{dictationStore.lastError}</pre>
          {:else if displayResult}
            <article class="transcript-card">
              <p>{displayResult.text || "<empty transcript>"}</p>
            </article>
            <dl class="metric-strip">
              <div>
                <dt><Icon name="clock" size={14} /> Whisper</dt>
                <dd>{whisperDuration}</dd>
              </div>
              <div>
                <dt><Icon name="audio" size={14} /> Captured</dt>
                <dd>{capturedDuration}</dd>
              </div>
              <div>
                <dt><Icon name="radio" size={14} /> Peak</dt>
                <dd>{peakLevel}</dd>
              </div>
            </dl>
          {:else}
            <div class="empty-state">
              <Icon name="mic" size={26} />
              <p>No transcript yet</p>
            </div>
          {/if}
        </section>
      </div>

      <aside class="side-column" aria-label="Voice workflow">
        <section class="surface pipeline-surface">
          <span class="eyebrow">Flow</span>
          <h2>Pipeline</h2>
          <ol class="pipeline-list">
            <li>
              <span><Icon name="mic" size={15} /></span>
              <div>
                <strong>Capture</strong>
                <small>{capturedDuration}</small>
              </div>
            </li>
            <li>
              <span><Icon name="brain" size={15} /></span>
              <div>
                <strong>Clean up</strong>
                <small>{settings.style.replace("-", " ")}</small>
              </div>
            </li>
            <li>
              <span><Icon name="zap" size={15} /></span>
              <div>
                <strong>Paste</strong>
                <small>Focused app</small>
              </div>
            </li>
          </ol>
        </section>

        <section class="surface feature-surface">
          <span class="eyebrow">Next layer</span>
          <h2>Workspace memory</h2>
          <div class="feature-list">
            <div>
              <Icon name="book" size={16} />
              <span>Dictionary</span>
            </div>
            <div>
              <Icon name="sparkles" size={16} />
              <span>Snippets</span>
            </div>
            <div>
              <Icon name="languages" size={16} />
              <span>Languages</span>
            </div>
          </div>
        </section>

        <section class="surface history-surface" aria-labelledby="history-heading">
          <div class="section-heading compact">
            <div>
              <span class="eyebrow">Recent</span>
              <h2 id="history-heading">History</h2>
            </div>
            <Icon name="history" size={17} />
          </div>

          {#if displayHistory.length}
            <ul>
              {#each displayHistory as entry, i (i)}
                <li>
                  <p>{entry.text || "<empty>"}</p>
                  <span>{entry.duration_ms}ms</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="muted-line">Recent dictations appear here.</p>
          {/if}
        </section>
      </aside>
    </section>
  </main>
{/if}
