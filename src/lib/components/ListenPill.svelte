<script lang="ts">
  type Props = {
    listening: boolean;
    label?: string;
  };

  let { listening, label = "Listening" }: Props = $props();
</script>

<div data-testid="listen-pill" data-listening={listening} class="root" class:listening>
  <span class="dot" class:pulse={listening}></span>
  <span class="label">{listening ? label : "Idle"}</span>
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
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 18px;
    margin: 8px;
    border-radius: 9999px;
    background: rgba(20, 20, 24, 0.85);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    color: white;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    font-size: 13px;
    font-weight: 500;
    box-shadow:
      0 8px 24px rgba(0, 0, 0, 0.35),
      0 1px 0 rgba(255, 255, 255, 0.08) inset;
    border: 1px solid rgba(255, 255, 255, 0.06);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #6b7280;
  }

  .root.listening .dot {
    background: #ef4444;
  }

  .pulse {
    animation: pulse 1.4s ease-in-out infinite;
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.5);
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.55);
    }
    70% {
      box-shadow: 0 0 0 12px rgba(239, 68, 68, 0);
    }
    100% {
      box-shadow: 0 0 0 0 rgba(239, 68, 68, 0);
    }
  }
</style>
