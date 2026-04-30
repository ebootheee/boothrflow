<script lang="ts">
  export type IconName =
    | "audio"
    | "book"
    | "brain"
    | "check"
    | "clock"
    | "command"
    | "history"
    | "languages"
    | "lock"
    | "mic"
    | "pen"
    | "radio"
    | "settings"
    | "sparkles"
    | "zap";

  type Props = {
    name: IconName;
    size?: number;
    strokeWidth?: number;
    class?: string;
  };

  let { name, size = 16, strokeWidth = 2, class: className = "" }: Props = $props();
</script>

<svg
  class={className}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === "audio"}
    <path d="M4 10v4" />
    <path d="M8 7v10" />
    <path d="M12 4v16" />
    <path d="M16 8v8" />
    <path d="M20 11v2" />
  {:else if name === "book"}
    <path d="M5 4h9a4 4 0 0 1 4 4v12H8a3 3 0 0 1-3-3V4z" />
    <path d="M8 4v13" />
    <path d="M10 8h5" />
  {:else if name === "brain"}
    <path d="M8 6a3 3 0 0 0-3 3 3 3 0 0 0 1 5 3 3 0 0 0 4 4" />
    <path d="M16 6a3 3 0 0 1 3 3 3 3 0 0 1-1 5 3 3 0 0 1-4 4" />
    <path d="M12 5v14" />
    <path d="M8 11h8" />
  {:else if name === "check"}
    <path d="m5 12 4 4 10-10" />
  {:else if name === "clock"}
    <circle cx="12" cy="12" r="8" />
    <path d="M12 8v5l3 2" />
  {:else if name === "command"}
    <path d="M8 8h8v8H8z" />
    <path d="M8 8H6a2 2 0 1 1 2-2v12a2 2 0 1 1-2-2h12a2 2 0 1 1-2 2V6a2 2 0 1 1 2 2h-2" />
  {:else if name === "history"}
    <path d="M4 12a8 8 0 1 0 3-6" />
    <path d="M4 5v5h5" />
    <path d="M12 8v5l3 2" />
  {:else if name === "languages"}
    <path d="M4 6h9" />
    <path d="M8 4v2" />
    <path d="M11 6c-.8 4-3 6-6 7" />
    <path d="M6 9c1 2 3 4 6 5" />
    <path d="m15 20 4-9 4 9" />
    <path d="M17 16h4" />
  {:else if name === "lock"}
    <rect x="5" y="10" width="14" height="10" rx="2" />
    <path d="M8 10V8a4 4 0 0 1 8 0v2" />
  {:else if name === "mic"}
    <rect x="8" y="3" width="8" height="12" rx="4" />
    <path d="M5 11a7 7 0 0 0 14 0" />
    <path d="M12 18v3" />
    <path d="M9 21h6" />
  {:else if name === "pen"}
    <path d="M4 20h4l11-11a2.5 2.5 0 0 0-4-4L4 16v4z" />
    <path d="m13 6 5 5" />
  {:else if name === "radio"}
    <circle cx="12" cy="12" r="2" />
    <path d="M8 8a6 6 0 0 0 0 8" />
    <path d="M16 8a6 6 0 0 1 0 8" />
    <path d="M5 5a10 10 0 0 0 0 14" />
    <path d="M19 5a10 10 0 0 1 0 14" />
  {:else if name === "sparkles"}
    <path d="m12 3 1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8L12 3z" />
    <path d="m19 14 .8 1.8 1.7.7-1.7.7L19 19l-.8-1.8-1.7-.7 1.7-.7L19 14z" />
    <path d="m5 14 .7 1.5L7 16l-1.3.5L5 18l-.7-1.5L3 16l1.3-.5L5 14z" />
  {:else if name === "settings"}
    <circle cx="12" cy="12" r="3" />
    <path
      d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
    />
  {:else if name === "zap"}
    <path d="M13 2 4 14h7l-1 8 10-13h-7l1-7z" />
  {/if}
</svg>
