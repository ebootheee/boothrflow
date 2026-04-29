import { isTauri } from "$lib/services/platform";

/** Coarse status for legacy UI surfaces (the settings page status pill). */
type DictationStatus = "idle" | "listening" | "processing";

/** Full lifecycle from the session daemon. Drives the floating pill. */
export type DictationLifecycle = "idle" | "listening" | "transcribing" | "cleaning" | "pasting";

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

type StatePayload = {
  state: DictationLifecycle;
  at_ms: number;
};

export type DonePayload = {
  formatted: string;
  capture_ms: number;
  stt_ms: number;
  llm_ms: number;
  paste_ms: number;
  total_ms: number;
};

/**
 * Streaming partial fired while the user is still holding push-to-talk.
 * `committed` is stable text the FE can render solid; `tentative` is the
 * latest pass's tail and may still revise on the next tick.
 */
export type PartialPayload = {
  committed: string;
  tentative: string;
  at_ms: number;
};

type DictationState = {
  lifecycle: DictationLifecycle;
  /** Monotonic ms since the current dictation began (resets on each press). */
  at_ms: number;
  lastResult: SttResultPayload | null;
  lastSummary: SummaryPayload | null;
  lastDone: DonePayload | null;
  lastPartial: PartialPayload | null;
  lastError: string | null;
  modelMissing: string | null;
  history: SttResultPayload[];
};

function statusFor(lifecycle: DictationLifecycle): DictationStatus {
  if (lifecycle === "idle") return "idle";
  if (lifecycle === "listening") return "listening";
  return "processing";
}

function createDictationStore() {
  const state = $state<DictationState>({
    lifecycle: "idle",
    at_ms: 0,
    lastResult: null,
    lastSummary: null,
    lastDone: null,
    lastPartial: null,
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
      state.lifecycle = "listening";
      state.at_ms = 0;
      state.lastError = null;
      state.lastPartial = null;
    });

    await listen<PartialPayload>("dictation:partial", (e) => {
      // Drop stale partials if a newer one already landed (events can
      // arrive out of order if the press loop is busy).
      if (state.lastPartial && state.lastPartial.at_ms > e.payload.at_ms) return;
      state.lastPartial = e.payload;
    });

    await listen<StatePayload>("dictation:state", (e) => {
      state.lifecycle = e.payload.state;
      state.at_ms = e.payload.at_ms;
    });

    await listen<SummaryPayload>("dictation:summary", (e) => {
      state.lastSummary = e.payload;
    });

    await listen<SttResultPayload>("dictation:result", (e) => {
      state.lastResult = e.payload;
      state.history = [e.payload, ...state.history].slice(0, 20);
    });

    await listen<DonePayload>("dictation:done", (e) => {
      state.lastDone = e.payload;
    });

    await listen<string>("dictation:error", (e) => {
      state.lifecycle = "idle";
      state.lastError = e.payload;
    });

    await listen<string>("dictation:model-missing", (e) => {
      state.modelMissing = e.payload;
    });
  }

  return {
    get lifecycle() {
      return state.lifecycle;
    },
    get atMs() {
      return state.at_ms;
    },
    get status(): DictationStatus {
      return statusFor(state.lifecycle);
    },
    get lastResult() {
      return state.lastResult;
    },
    get lastSummary() {
      return state.lastSummary;
    },
    get lastDone() {
      return state.lastDone;
    },
    get lastPartial() {
      return state.lastPartial;
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
