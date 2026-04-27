import { Err, Ok, type Result } from "wellcrafted/result";
import { type Style } from "./styles";
import { isTauri } from "$lib/services/platform";

export type DictateResult = {
  raw: string;
  formatted: string;
  durationMs: number;
};

export type DictateError = { kind: "dictation-failed"; message: string };

export type DictationService = {
  dictateOnce(input: { style: Style }): Promise<Result<DictateResult, DictateError>>;
};

/**
 * Web-only fake. Returns canned data after a tiny delay so the UI smoke path
 * exercises the full Result<T, E> flow without hitting Tauri.
 */
function createDictationServiceWeb(): DictationService {
  return {
    async dictateOnce({ style }) {
      await new Promise((r) => setTimeout(r, 250));
      const raw = "uh so basically this is a fake transcript";
      const formatted = formatFake(raw, style);
      return Ok({ raw, formatted, durationMs: 250 });
    },
  };
}

/**
 * Desktop impl — calls the Rust pipeline via Tauri invoke.
 * Lazy-imports @tauri-apps/api to keep the web bundle clean.
 */
function createDictationServiceDesktop(): DictationService {
  return {
    async dictateOnce({ style }) {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const value = await invoke<DictateResult>("dictate_once", { style });
        return Ok(value);
      } catch (error) {
        return Err({ kind: "dictation-failed", message: String(error) });
      }
    },
  };
}

function formatFake(raw: string, style: Style): string {
  // Mimics the LLM cleanup path — strip filler words + apply style-ish casing.
  const cleaned = raw.replace(/\b(uh|um|like|you know|basically)\b\s?/gi, "").trim();
  switch (style) {
    case "raw":
      return raw;
    case "formal":
      return capitalize(cleaned) + ".";
    case "casual":
      return cleaned.toLowerCase();
    case "excited":
      return capitalize(cleaned) + "!";
    case "very-casual":
      return cleaned.toLowerCase();
  }
}

function capitalize(s: string): string {
  return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
}

export const dictationService: DictationService = isTauri()
  ? createDictationServiceDesktop()
  : createDictationServiceWeb();
