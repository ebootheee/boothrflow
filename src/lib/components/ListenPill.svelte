<script lang="ts">
  import Icon from "$lib/components/Icon.svelte";

  type Props = {
    listening?: boolean;
    label?: string;
  };

  let { listening = false, label = "Listening" }: Props = $props();
</script>

<div data-testid="listen-pill" data-listening={listening} class="root" class:listening>
  <span class="dot" class:pulse={listening} aria-hidden="true"></span>
  <Icon name="mic" size={16} strokeWidth={2.3} />
  <span class="label">{listening ? label : "Ready"}</span>
  <span class="hint">Ctrl + Win</span>
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
    grid-template-columns: 10px 20px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    width: 100vw;
    height: 100vh;
    box-sizing: border-box;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 9999px;
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
    padding: 0 16px;
    backdrop-filter: blur(14px);
    -webkit-backdrop-filter: blur(14px);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: #8da19a;
  }

  .root.listening .dot {
    background: #ff7f6e;
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

  .pulse {
    animation: pulse 1.35s ease-in-out infinite;
    box-shadow: 0 0 0 0 rgba(255, 127, 110, 0.45);
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
</style>
