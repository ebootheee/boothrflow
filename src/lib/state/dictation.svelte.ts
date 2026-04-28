import { isTauri } from "$lib/services/platform";

type DictationStatus = "idle" | "listening" | "processing";

type SttResultPayload = {
  text: string;
  language: string | null;
  duration_ms: number;
};

type SummaryPayload = {
  frames: number;
  samples: number;
  seconds: number;
  peak_dbfs: number;
};

type DictationState = {
  status: DictationStatus;
  lastResult: SttResultPayload | null;
  lastSummary: SummaryPayload | null;
  lastError: string | null;
  modelMissing: string | null;
  history: SttResultPayload[];
};

function createDictationStore() {
  const state = $state<DictationState>({
    status: "idle",
    lastResult: null,
    lastSummary: null,
    lastError: null,
    modelMissing: null,
    history: [],
  });

  let started = false;

  async function attach() {
    if (started || !isTauri()) return;
    started = true;

    const { listen } = await import("@tauri-apps/api/event");

    await listen("dictation:start", () => {
      state.status = "listening";
      state.lastError = null;
    });

    await listen<SummaryPayload>("dictation:summary", (e) => {
      state.status = "processing";
      state.lastSummary = e.payload;
    });

    await listen<SttResultPayload>("dictation:result", (e) => {
      state.status = "idle";
      state.lastResult = e.payload;
      state.history = [e.payload, ...state.history].slice(0, 20);
    });

    await listen<string>("dictation:error", (e) => {
      state.status = "idle";
      state.lastError = e.payload;
    });

    await listen<string>("dictation:model-missing", (e) => {
      state.modelMissing = e.payload;
    });
  }

  return {
    get status() {
      return state.status;
    },
    get lastResult() {
      return state.lastResult;
    },
    get lastSummary() {
      return state.lastSummary;
    },
    get lastError() {
      return state.lastError;
    },
    get modelMissing() {
      return state.modelMissing;
    },
    get history() {
      return state.history;
    },
    attach,
  };
}

export const dictationStore = createDictationStore();
