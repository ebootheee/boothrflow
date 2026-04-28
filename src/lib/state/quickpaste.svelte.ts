import { isTauri } from "$lib/services/platform";

type Style = "raw" | "formal" | "casual" | "excited" | "very-casual";

export type HistoryEntry = {
  id: number;
  captured_at: string;
  raw: string;
  formatted: string;
  style: Style;
  app_exe: string | null;
  window_title: string | null;
  duration_ms: number;
  llm_ms: number;
  has_embedding: boolean;
};

export type SearchResult = {
  entry: HistoryEntry;
  score: number;
  source: "lexical" | "semantic" | "both";
};

type QuickPasteState = {
  loading: boolean;
  query: string;
  recent: HistoryEntry[];
  results: SearchResult[];
  selected: number;
  error: string | null;
};

function createQuickPaste() {
  const state = $state<QuickPasteState>({
    loading: false,
    query: "",
    recent: [],
    results: [],
    selected: 0,
    error: null,
  });

  let searchToken = 0; // race-guard for concurrent typed queries

  async function loadRecent(limit = 50) {
    if (!isTauri()) return;
    state.loading = true;
    state.error = null;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      state.recent = await invoke<HistoryEntry[]>("history_recent", { limit });
    } catch (e) {
      state.error = `failed to load recent: ${String(e)}`;
    } finally {
      state.loading = false;
    }
  }

  async function search(query: string) {
    state.query = query;
    state.selected = 0;
    if (!isTauri()) return;
    if (!query.trim()) {
      state.results = [];
      return;
    }

    const token = ++searchToken;
    state.loading = true;
    state.error = null;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const results = await invoke<SearchResult[]>("history_search", {
        query,
        limit: 20,
      });
      if (token === searchToken) {
        state.results = results;
      }
    } catch (e) {
      if (token === searchToken) state.error = `search failed: ${String(e)}`;
    } finally {
      if (token === searchToken) state.loading = false;
    }
  }

  function visibleEntries(): HistoryEntry[] {
    if (state.query.trim() && state.results.length > 0) {
      return state.results.map((r) => r.entry);
    }
    return state.recent;
  }

  function moveSelection(delta: number) {
    const list = visibleEntries();
    if (list.length === 0) return;
    const next = (state.selected + delta + list.length) % list.length;
    state.selected = next;
  }

  async function pasteSelected() {
    const list = visibleEntries();
    const entry = list[state.selected];
    if (!entry) return;
    await pasteById(entry.id);
  }

  async function pasteById(id: number) {
    if (!isTauri()) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("quickpaste_paste", { id });
      // Backend hides the palette + restores focus + injects.
    } catch (e) {
      state.error = `paste failed: ${String(e)}`;
    }
  }

  async function close() {
    if (!isTauri()) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("quickpaste_close");
    } catch {
      // best-effort
    }
  }

  function reset() {
    state.query = "";
    state.results = [];
    state.selected = 0;
    state.error = null;
  }

  return {
    get loading() {
      return state.loading;
    },
    get query() {
      return state.query;
    },
    get recent() {
      return state.recent;
    },
    get results() {
      return state.results;
    },
    get selected() {
      return state.selected;
    },
    get error() {
      return state.error;
    },
    visibleEntries,
    loadRecent,
    search,
    moveSelection,
    pasteSelected,
    pasteById,
    close,
    reset,
  };
}

export const quickPaste = createQuickPaste();
