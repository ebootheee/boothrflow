<script lang="ts">
  import { isOk } from "wellcrafted/result";
  import { settings } from "$lib/state/settings.svelte";
  import { dictationService } from "$lib/services/dictation";

  let lastTranscript = $state("");
  let pending = $state(false);

  async function dictateOnce() {
    pending = true;
    const result = await dictationService.dictateOnce({
      style: settings.style,
    });
    pending = false;
    lastTranscript = isOk(result) ? result.data.formatted : `Error: ${result.error.message}`;
  }
</script>

<main class="mx-auto flex h-full max-w-2xl flex-col gap-6 p-8">
  <header>
    <h1 class="text-2xl font-semibold tracking-tight">boothrflow</h1>
    <p class="text-muted-foreground text-sm">
      Local-first voice dictation. Hot path not yet wired — this is a scaffold.
    </p>
  </header>

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
    <h2 class="mb-2 text-base font-medium">Smoke test</h2>
    <p class="text-muted-foreground mb-3 text-sm">
      Click to invoke the (stubbed) dictation pipeline. Behind the scenes this calls a fake STT +
      fake LLM until real engines are wired.
    </p>
    <button
      class="bg-foreground text-background rounded px-3 py-1.5 text-sm disabled:opacity-50"
      disabled={pending}
      onclick={dictateOnce}
    >
      {pending ? "Dictating…" : "Dictate (fake)"}
    </button>

    {#if lastTranscript}
      <pre class="bg-muted mt-3 rounded p-3 text-sm whitespace-pre-wrap">{lastTranscript}</pre>
    {/if}
  </section>
</main>
