<script lang="ts">
  import { onMount } from "svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { dictationStore } from "$lib/state/dictation.svelte";
  import ListenPill from "$lib/components/ListenPill.svelte";

  // Hash-based window routing. The Rust side opens a second window with
  // url=index.html#listen-pill; everything else is the main settings UI.
  const isPill = typeof window !== "undefined" && window.location.hash === "#listen-pill";

  onMount(() => {
    void dictationStore.attach();
  });
</script>

{#if isPill}
  <ListenPill listening={true} label="Listening" />
{:else}
  <main class="mx-auto flex h-full max-w-2xl flex-col gap-6 p-8">
    <header>
      <h1 class="text-2xl font-semibold tracking-tight">boothrflow</h1>
      <p class="text-muted-foreground text-sm">
        Local-first voice dictation. Hold <kbd>Ctrl + Win</kbd> to dictate.
      </p>
    </header>

    {#if dictationStore.modelMissing}
      <section class="rounded-xl border border-amber-400 bg-amber-50 p-4 text-amber-900">
        <h2 class="mb-1 text-sm font-semibold">Whisper model not loaded</h2>
        <pre class="text-xs whitespace-pre-wrap">{dictationStore.modelMissing}</pre>
      </section>
    {/if}

    <section class="rounded-xl border p-4">
      <h2 class="mb-2 text-base font-medium">Style</h2>
      <select class="bg-muted rounded px-2 py-1 text-sm" bind:value={settings.style}>
        <option value="raw">Raw (passthrough)</option>
        <option value="formal">Formal</option>
        <option value="casual">Casual</option>
        <option value="excited">Excited</option>
        <option value="very-casual">Very casual</option>
      </select>
    </section>

    <section class="rounded-xl border p-4">
      <h2 class="mb-2 text-base font-medium">
        Live transcript
        <span class="text-muted-foreground ml-2 text-xs font-normal">
          status: <span data-testid="status">{dictationStore.status}</span>
        </span>
      </h2>

      {#if dictationStore.lastError}
        <pre class="bg-muted mt-2 rounded p-3 text-xs whitespace-pre-wrap text-red-700">
{dictationStore.lastError}</pre>
      {:else if dictationStore.lastResult}
        <pre class="bg-muted mt-2 rounded p-3 text-sm whitespace-pre-wrap">{dictationStore
            .lastResult.text || "<empty transcript>"}</pre>
        <p class="text-muted-foreground mt-2 text-xs">
          Whisper {dictationStore.lastResult.duration_ms}ms
          {#if dictationStore.lastSummary}
            · captured {dictationStore.lastSummary.seconds.toFixed(2)}s · peak
            {dictationStore.lastSummary.peak_dbfs.toFixed(1)} dBFS
          {/if}
        </p>
      {:else}
        <p class="text-muted-foreground mt-2 text-sm">
          Hold <kbd>Ctrl + Win</kbd>, speak, release. Transcripts appear here while we wire
          paste-anywhere (W3).
        </p>
      {/if}
    </section>

    {#if dictationStore.history.length > 1}
      <section class="rounded-xl border p-4">
        <h2 class="mb-2 text-base font-medium">Recent transcripts</h2>
        <ul class="text-muted-foreground space-y-2 text-sm">
          {#each dictationStore.history.slice(1) as entry, i (i)}
            <li class="border-l-2 pl-2">
              <span class="text-foreground">{entry.text || "<empty>"}</span>
              <span class="text-xs">— {entry.duration_ms}ms</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </main>
{/if}
