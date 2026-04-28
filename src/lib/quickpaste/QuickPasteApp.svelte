<script lang="ts">
  import { onMount } from "svelte";
  import { quickPaste } from "$lib/state/quickpaste.svelte";

  let inputEl: HTMLInputElement | undefined = $state();

  onMount(() => {
    quickPaste.loadRecent();
    queueMicrotask(() => inputEl?.focus());
    // The Tauri-side window listener fires when this window loses focus
    // (e.g. user clicks elsewhere) — close so we don't hang invisibly.
    const onBlur = () => {
      // small delay so click-to-paste lands first
      setTimeout(() => quickPaste.close(), 60);
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  });

  function onInput(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    quickPaste.search(v);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      quickPaste.close();
    } else if (e.key === "Enter") {
      e.preventDefault();
      quickPaste.pasteSelected();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      quickPaste.moveSelection(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      quickPaste.moveSelection(-1);
    }
  }

  function relativeTime(iso: string): string {
    const t = new Date(iso).getTime();
    if (!Number.isFinite(t)) return iso;
    const delta = (Date.now() - t) / 1000;
    if (delta < 60) return `${Math.round(delta)}s ago`;
    if (delta < 3600) return `${Math.round(delta / 60)}m ago`;
    if (delta < 86400) return `${Math.round(delta / 3600)}h ago`;
    return `${Math.round(delta / 86400)}d ago`;
  }
</script>

<div class="qp-root" onkeydown={onKeydown} role="dialog" tabindex="-1">
  <input
    bind:this={inputEl}
    class="qp-search"
    placeholder="Search history… (Esc to close, ↑↓ to navigate, Enter to paste)"
    value={quickPaste.query}
    oninput={onInput}
    autocomplete="off"
    spellcheck="false"
  />

  {#if quickPaste.error}
    <div class="qp-error">{quickPaste.error}</div>
  {/if}

  <ul class="qp-list" role="listbox" aria-label="History entries">
    {#each quickPaste.visibleEntries() as entry, i (entry.id)}
      <li
        class="qp-item"
        class:selected={i === quickPaste.selected}
        role="option"
        aria-selected={i === quickPaste.selected}
        onclick={() => quickPaste.pasteById(entry.id)}
      >
        <div class="qp-text">{entry.formatted}</div>
        <div class="qp-meta">
          <span class="qp-style">{entry.style}</span>
          <span>{relativeTime(entry.captured_at)}</span>
          {#if entry.has_embedding}
            <span class="qp-emb" title="embedded — semantic search available">●</span>
          {/if}
        </div>
      </li>
    {/each}

    {#if quickPaste.visibleEntries().length === 0 && !quickPaste.loading}
      <li class="qp-empty">
        {quickPaste.query.trim() ? "no matches" : "no history yet — dictate something first"}
      </li>
    {/if}
  </ul>
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent !important;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  .qp-root {
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    background: rgba(20, 20, 24, 0.94);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    color: #e8e8ea;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    overflow: hidden;
  }

  .qp-search {
    flex: 0 0 auto;
    padding: 14px 18px;
    border: 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    background: transparent;
    color: inherit;
    font-size: 15px;
    outline: none;
  }
  .qp-search::placeholder {
    color: rgba(232, 232, 234, 0.4);
  }

  .qp-error {
    padding: 8px 18px;
    background: rgba(220, 60, 60, 0.18);
    color: #ffd6d6;
    font-size: 12px;
  }

  .qp-list {
    flex: 1 1 auto;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }

  .qp-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 18px;
    cursor: pointer;
    border-left: 2px solid transparent;
  }
  .qp-item:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .qp-item.selected {
    background: rgba(255, 255, 255, 0.08);
    border-left-color: #ef6f61;
  }

  .qp-text {
    font-size: 13px;
    line-height: 1.35;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .qp-meta {
    display: flex;
    gap: 8px;
    font-size: 11px;
    color: rgba(232, 232, 234, 0.5);
  }

  .qp-style {
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-weight: 600;
  }

  .qp-emb {
    color: #2f9f8f;
  }

  .qp-empty {
    padding: 22px 18px;
    color: rgba(232, 232, 234, 0.45);
    font-size: 13px;
    text-align: center;
  }
</style>
