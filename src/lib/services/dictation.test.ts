import { describe, expect, it } from "vitest";
import { isErr, isOk } from "wellcrafted/result";
import { dictationService } from "./dictation";

describe("dictationService (web fake)", () => {
  it("returns Ok with stripped fillers when style=casual", async () => {
    const result = await dictationService.dictateOnce({ style: "casual" });
    expect(isOk(result)).toBe(true);
    if (isErr(result)) return;
    expect(result.data.formatted).not.toMatch(/\buh\b/i);
    expect(result.data.formatted).not.toMatch(/\bbasically\b/i);
    expect(result.data.raw).toContain("uh");
  });

  it("preserves the raw transcript when style=raw", async () => {
    const result = await dictationService.dictateOnce({ style: "raw" });
    expect(isOk(result)).toBe(true);
    if (isErr(result)) return;
    expect(result.data.formatted).toBe(result.data.raw);
  });

  it("ends formal output with a period", async () => {
    const result = await dictationService.dictateOnce({ style: "formal" });
    expect(isOk(result)).toBe(true);
    if (isErr(result)) return;
    expect(result.data.formatted.endsWith(".")).toBe(true);
  });

  it("ends excited output with an exclamation", async () => {
    const result = await dictationService.dictateOnce({ style: "excited" });
    expect(isOk(result)).toBe(true);
    if (isErr(result)) return;
    expect(result.data.formatted.endsWith("!")).toBe(true);
  });
});
