<script lang="ts">
  import { onMount } from "svelte";
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
  const activeAtMs = $derived(atMs ?? dictationStore.atMs);
  const meta = $derived(stageMeta(activeLifecycle));
  const hasPartial = $derived(Boolean(activePartial?.committed || activePartial?.tentative));

  onMount(() => {
    void dictationStore.attach();
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
    <span class="elapsed">{formatElapsed(activeAtMs)}</span>
    <span class="hint">{hotkey}</span>
  </div>

  <div class="partial-row" class:empty={!hasPartial}>
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
  :global(html),
  :global(body) {
    background: transparent !important;
    margin: 0;
    padding: 0;
    overflow: hidden;
    user-select: none;
  }

  .root {
    display: grid;
    grid-template-rows: 28px minmax(0, 1fr);
    gap: 3px;
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
    padding: 9px 14px 10px;
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
    overflow: hidden;
    color: rgba(247, 251, 249, 0.92);
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
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
