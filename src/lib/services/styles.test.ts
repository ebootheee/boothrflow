import { describe, expect, it } from "vitest";
import { isStyle, STYLES } from "./styles";

describe("isStyle", () => {
  it("accepts every value in STYLES", () => {
    for (const s of STYLES) {
      expect(isStyle(s)).toBe(true);
    }
  });

  it("rejects unknown strings", () => {
    expect(isStyle("nope")).toBe(false);
    expect(isStyle("")).toBe(false);
  });

  it("rejects non-strings", () => {
    expect(isStyle(undefined)).toBe(false);
    expect(isStyle(null)).toBe(false);
    expect(isStyle(42)).toBe(false);
    expect(isStyle({})).toBe(false);
  });
});
