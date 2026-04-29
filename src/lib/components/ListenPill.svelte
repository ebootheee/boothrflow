<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import {
    dictationStore,
    type DictationLifecycle,
    type PartialPayload,
  } from "$lib/state/dictation.svelte";
  import { dictationHotkeyLabel } from "$lib/services/platform";

  type Props = {
    lifecycle?: DictationLifecycle;
    partial?: PartialPayload | null;
    atMs?: number;
    hotkey?: string;
  };

  let {
    lifecycle = undefined,
    partial = undefined,
    atMs = undefined,
    hotkey = dictationHotkeyLabel(),
  }: Props = $props();

  const activeLifecycle = $derived(lifecycle ?? dictationStore.lifecycle);
  const activePartial = $derived(partial ?? dictationStore.lastPartial);
  const propAtMs = $derived(atMs);
  const meta = $derived(stageMeta(activeLifecycle));
  const hasPartial = $derived(Boolean(activePartial?.committed || activePartial?.tentative));

  // Live elapsed clock. The backend's `dictation:state` events only fire on
  // stage transitions, so its at_ms freezes during "listening". We tick a
  // local clock every 100ms so the user sees the pill counting up while
  // they hold the talk key.
  let listenStartedAt = $state<number | null>(null);
  let nowMs = $state<number>(0);
  const displayMs = $derived(
    propAtMs ??
      (activeLifecycle === "listening" && listenStartedAt != null
        ? Math.max(0, nowMs - listenStartedAt)
        : dictationStore.atMs),
  );

  $effect(() => {
    if (activeLifecycle === "listening") {
      // Reset on each fresh press; idempotent if the effect re-runs.
      if (listenStartedAt == null) {
        listenStartedAt = Date.now();
        nowMs = listenStartedAt;
      }
      const id = setInterval(() => {
        nowMs = Date.now();
      }, 100);
      return () => clearInterval(id);
    }
    listenStartedAt = null;
  });

  // Auto-scroll the partial row so the newest tokens stay visible as the
  // user keeps talking. Now wraps to two lines and scrolls vertically —
  // gives roughly 2× the visible context vs the prior horizontal-scroll
  // single-line layout, which mattered for longer dictations where you
  // want to see what you just said, not just the trailing words.
  let partialEl = $state<HTMLDivElement | null>(null);
  $effect(() => {
    void activePartial; // track changes
    if (partialEl) {
      void tick().then(() => {
        if (partialEl) partialEl.scrollTop = partialEl.scrollHeight;
      });
    }
  });

  onMount(() => {
    void dictationStore.attach();
    // Pill window styling (transparent background, no scrollbars, no select)
    // applied imperatively so it stays scoped to this window. Earlier we
    // had these as :global(html)/:global(body) rules in the component CSS
    // block, but Svelte 5 emits :global rules into the shared bundle CSS,
    // which meant the main window inherited overflow:hidden and lost its
    // scroll. Setting the styles here on mount keeps them scoped.
    const html = document.documentElement;
    const body = document.body;
    const root = document.getElementById("app");
    const original: Array<[HTMLElement, Record<string, string>]> = [];
    for (const el of [html, body, root]) {
      if (!el) continue;
      original.push([
        el,
        {
          background: el.style.background,
          margin: el.style.margin,
          padding: el.style.padding,
          overflow: el.style.overflow,
          userSelect: el.style.userSelect,
        },
      ]);
      el.style.background = "transparent";
      el.style.margin = "0";
      el.style.padding = "0";
      el.style.overflow = "hidden";
      el.style.userSelect = "none";
    }
    return () => {
      for (const [el, prev] of original) {
        for (const [k, v] of Object.entries(prev)) {
          (el.style as unknown as Record<string, string>)[k] = v;
        }
      }
    };
  });

  function stageMeta(state: DictationLifecycle): {
    label: string;
    icon: IconName;
    helper: string;
    busy: boolean;
  } {
    switch (state) {
      case "listening":
        return { label: "Listening", icon: "mic", helper: "Speak now", busy: true };
      case "transcribing":
        return {
          label: "Transcribing",
          icon: "radio",
          helper: "Turning audio into text",
          busy: true,
        };
      case "cleaning":
        return { label: "Cleaning", icon: "sparkles", helper: "Applying style", busy: true };
      case "pasting":
        return { label: "Pasting", icon: "zap", helper: "Sending to focused app", busy: true };
      case "idle":
      default:
        return { label: "Ready", icon: "check", helper: "Ready", busy: false };
    }
  }

  function formatElapsed(ms: number): string {
    if (!Number.isFinite(ms) || ms <= 0) return "0.0s";
    return `${(ms / 1000).toFixed(1)}s`;
  }
</script>

<div
  data-testid="listen-pill"
  data-lifecycle={activeLifecycle}
  class="root"
  class:busy={meta.busy}
  aria-live="polite"
>
  <div class="status-row">
    <span class="dot" class:pulse={activeLifecycle === "listening"} aria-hidden="true"></span>
    <span class="icon-wrap" class:spin={activeLifecycle === "transcribing"} aria-hidden="true">
      <Icon name={meta.icon} size={16} strokeWidth={2.3} />
    </span>
    <span class="label">{meta.label}</span>
    <span class="elapsed">{formatElapsed(displayMs)}</span>
    <span class="hint">{hotkey}</span>
  </div>

  <div class="partial-row" class:empty={!hasPartial} bind:this={partialEl}>
    {#if activePartial?.committed}
      <span class="committed">{activePartial.committed}</span>
    {/if}
    {#if activePartial?.tentative}
      <span class="tentative">{activePartial.tentative}</span>
    {/if}
    {#if !hasPartial}
      <span>{meta.helper}</span>
    {/if}
  </div>
</div>

<style>
  /* No :global() rules here on purpose. Earlier this block had
     `:global(html), :global(body) { background: transparent; overflow: hidden }`
     which Svelte 5 emits into the shared bundle CSS — that broke the main
     window (transparent body / no scroll). Pill-only styles are now applied
     imperatively in onMount and reverted on unmount. */

  .root {
    display: grid;
    grid-template-rows: 22px minmax(0, 1fr);
    gap: 4px;
    width: 100vw;
    height: 100vh;
    box-sizing: border-box;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 16px;
    background: rgba(24, 34, 40, 0.94);
    box-shadow:
      0 12px 32px rgba(0, 0, 0, 0.34),
      0 1px 0 rgba(255, 255, 255, 0.1) inset;
    color: #f7fbf9;
    font-family:
      Inter,
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    padding: 7px 13px 8px;
    backdrop-filter: blur(14px);
    -webkit-backdrop-filter: blur(14px);
  }

  .status-row {
    display: grid;
    grid-template-columns: 10px 20px minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: #8da19a;
  }

  .root.busy .dot {
    background: #ff7f6e;
  }

  .icon-wrap {
    display: grid;
    place-items: center;
    color: rgba(247, 251, 249, 0.86);
  }

  .label {
    overflow: hidden;
    color: #fff;
    font-size: 13px;
    font-weight: 780;
    letter-spacing: 0;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .elapsed {
    color: rgba(247, 251, 249, 0.56);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    font-weight: 740;
    line-height: 1;
  }

  .hint {
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    color: rgba(247, 251, 249, 0.72);
    font-size: 11px;
    font-weight: 720;
    line-height: 1;
    padding: 6px 8px;
    white-space: nowrap;
  }

  .partial-row {
    display: block;
    min-width: 0;
    /* Two-line wrap with vertical scroll. The auto-scroll-to-bottom keeps
       the newest sentence in view; older lines fall up off the top as the
       user keeps speaking. Word-break covers very long unbroken tokens
       (URLs, file paths) so they wrap instead of forcing a horizontal
       scrollbar. */
    overflow-x: hidden;
    overflow-y: auto;
    color: rgba(247, 251, 249, 0.92);
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 1.3;
    white-space: normal;
    word-break: break-word;
    scrollbar-width: none;
  }

  .partial-row::-webkit-scrollbar {
    display: none;
  }

  .partial-row.empty {
    color: rgba(247, 251, 249, 0.5);
    font-weight: 680;
  }

  .committed {
    color: #fff;
  }

  .tentative {
    color: rgba(247, 251, 249, 0.5);
  }

  .committed + .tentative::before {
    content: " ";
  }

  .pulse {
    animation: pulse 1.35s ease-in-out infinite;
    box-shadow: 0 0 0 0 rgba(255, 127, 110, 0.45);
  }

  .spin {
    animation: spin 0.85s linear infinite;
    transform-origin: center;
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(255, 127, 110, 0.5);
    }

    70% {
      box-shadow: 0 0 0 9px rgba(255, 127, 110, 0);
    }

    100% {
      box-shadow: 0 0 0 0 rgba(255, 127, 110, 0);
    }
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
