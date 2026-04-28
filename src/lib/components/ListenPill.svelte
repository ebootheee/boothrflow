<script lang="ts">
  import Icon from "$lib/components/Icon.svelte";

  type Props = {
    listening?: boolean;
    label?: string;
  };

  let { listening = false, label = "Listening" }: Props = $props();
</script>

<div data-testid="listen-pill" data-listening={listening} class="root" class:listening>
  <div class="mic" aria-hidden="true">
    <Icon name="mic" size={18} strokeWidth={2.4} />
  </div>
  <div class="copy">
    <span class="label">{listening ? label : "Ready"}</span>
    <div class="meter" aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
      <span></span>
      <span></span>
    </div>
  </div>
  <span class="dot" class:pulse={listening}></span>
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
    grid-template-columns: 38px 1fr 12px;
    align-items: center;
    gap: 12px;
    width: calc(100vw - 16px);
    height: calc(100vh - 16px);
    box-sizing: border-box;
    margin: 8px;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 9999px;
    background:
      linear-gradient(135deg, rgba(32, 43, 54, 0.9), rgba(17, 26, 35, 0.92)), rgba(17, 26, 35, 0.92);
    box-shadow:
      0 18px 50px rgba(0, 0, 0, 0.34),
      0 1px 0 rgba(255, 255, 255, 0.12) inset;
    color: white;
    font-family:
      Inter,
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    padding: 9px 14px 9px 10px;
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
  }

  .mic {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border-radius: 999px;
    background: linear-gradient(145deg, #ef6f61, #2f9f8f);
    box-shadow: 0 10px 26px rgba(239, 111, 97, 0.26);
  }

  .copy {
    min-width: 0;
  }

  .label {
    display: block;
    overflow: hidden;
    color: #f7fbf9;
    font-size: 13px;
    font-weight: 780;
    letter-spacing: 0;
    line-height: 1.15;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meter {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 12px;
    margin-top: 4px;
  }

  .meter span {
    display: block;
    width: 4px;
    height: 6px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.58);
  }

  .meter span:nth-child(2),
  .meter span:nth-child(4) {
    height: 10px;
  }

  .meter span:nth-child(3) {
    height: 12px;
  }

  .listening .meter span {
    animation: meter 850ms ease-in-out infinite;
  }

  .meter span:nth-child(2) {
    animation-delay: 80ms;
  }

  .meter span:nth-child(3) {
    animation-delay: 160ms;
  }

  .meter span:nth-child(4) {
    animation-delay: 240ms;
  }

  .meter span:nth-child(5) {
    animation-delay: 320ms;
  }

  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #8da19a;
  }

  .root.listening .dot {
    background: #ff8a72;
  }

  .pulse {
    animation: pulse 1.4s ease-in-out infinite;
    box-shadow: 0 0 0 0 rgba(255, 138, 114, 0.5);
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(255, 138, 114, 0.55);
    }

    70% {
      box-shadow: 0 0 0 10px rgba(255, 138, 114, 0);
    }

    100% {
      box-shadow: 0 0 0 0 rgba(255, 138, 114, 0);
    }
  }

  @keyframes meter {
    0%,
    100% {
      transform: scaleY(0.65);
    }

    50% {
      transform: scaleY(1.18);
    }
  }
</style>
