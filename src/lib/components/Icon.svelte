<script lang="ts">
  export type IconName =
    | "audio"
    | "book"
    | "brain"
    | "check"
    | "clock"
    | "command"
    | "database"
    | "download"
    | "history"
    | "key"
    | "languages"
    | "lock"
    | "mic"
    | "pen"
    | "radio"
    | "server"
    | "settings"
    | "shield"
    | "sparkles"
    | "upload"
    | "x"
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
  {:else if name === "database"}
    <ellipse cx="12" cy="5" rx="8" ry="3" />
    <path d="M4 5v6c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
    <path d="M4 11v6c0 1.7 3.6 3 8 3s8-1.3 8-3v-6" />
  {:else if name === "download"}
    <path d="M12 3v12" />
    <path d="m7 10 5 5 5-5" />
    <path d="M5 21h14" />
  {:else if name === "history"}
    <path d="M4 12a8 8 0 1 0 3-6" />
    <path d="M4 5v5h5" />
    <path d="M12 8v5l3 2" />
  {:else if name === "key"}
    <circle cx="7.5" cy="14.5" r="4.5" />
    <path d="m11 11 8-8" />
    <path d="m16 6 2 2" />
    <path d="m14 8 2 2" />
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
  {:else if name === "server"}
    <rect x="4" y="4" width="16" height="6" rx="2" />
    <rect x="4" y="14" width="16" height="6" rx="2" />
    <path d="M8 7h.01" />
    <path d="M8 17h.01" />
  {:else if name === "settings"}
    <path d="M4 6h16" />
    <path d="M4 12h16" />
    <path d="M4 18h16" />
    <path d="M8 4v4" />
    <path d="M16 10v4" />
    <path d="M11 16v4" />
  {:else if name === "shield"}
    <path d="M12 3 5 6v5c0 4.4 3 8.4 7 10 4-1.6 7-5.6 7-10V6l-7-3z" />
    <path d="m9 12 2 2 4-4" />
  {:else if name === "sparkles"}
    <path d="m12 3 1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8L12 3z" />
    <path d="m19 14 .8 1.8 1.7.7-1.7.7L19 19l-.8-1.8-1.7-.7 1.7-.7L19 14z" />
    <path d="m5 14 .7 1.5L7 16l-1.3.5L5 18l-.7-1.5L3 16l1.3-.5L5 14z" />
  {:else if name === "upload"}
    <path d="M12 15V3" />
    <path d="m17 8-5-5-5 5" />
    <path d="M5 21h14" />
  {:else if name === "x"}
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  {:else if name === "zap"}
    <path d="M13 2 4 14h7l-1 8 10-13h-7l1-7z" />
  {/if}
</svg>
