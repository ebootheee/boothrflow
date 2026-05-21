import { describe, it, expect } from "vitest";
import { wordDiff } from "./word-diff";

function added(segments: ReturnType<typeof wordDiff>): string[] {
  return segments.filter((s) => s.added).map((s) => s.text);
}

describe("wordDiff", () => {
  it("returns no added segments when formatted is a punctuation-cleaned subset of raw", () => {
    const segments = wordDiff("hello world this is great", "Hello world. This is great.");
    expect(added(segments)).toEqual([]);
  });

  it("flags hallucinated words that don't appear in raw", () => {
    const segments = wordDiff(
      "feel free to use your, let's just draft this into a markdown file",
      "feel free to use your email tool to draft this into a markdown file",
    );
    // The 2026-05-09 trailing-fragment completion failure mode: "email tool to"
    // bridges the trail-off; raw has no "email" or "tool", so both flag.
    expect(added(segments)).toContain("email");
    expect(added(segments)).toContain("tool");
  });

  it("flags only extra occurrences when a word repeats more in formatted than raw", () => {
    const segments = wordDiff(
      "Use your email tool to read the email",
      "Use your email tool to read the email then use your email tool again",
    );
    // Raw has email_tool x1+x1=2 each; formatted has email x3, tool x2.
    // Extra: email (1), tool (1), then(1), use(1), your(1), again(1).
    expect(added(segments)).toContain("again");
    // The very first "email" in formatted should NOT be flagged (position-naive
    // multiset consumes from raw left-to-right; the second occurrence triggers).
    const firstEmail = segments.find((s) => s.text === "email" && !s.added);
    expect(firstEmail).toBeDefined();
  });

  it("ignores case + punctuation when matching", () => {
    const segments = wordDiff("hello WORLD", "Hello, world!");
    expect(added(segments)).toEqual([]);
  });

  it("preserves whitespace and punctuation as non-added segments", () => {
    const segments = wordDiff("a b c", "a b c");
    const reconstructed = segments.map((s) => s.text).join("");
    expect(reconstructed).toBe("a b c");
    expect(added(segments)).toEqual([]);
  });

  it("handles empty raw — every word in formatted is added", () => {
    const segments = wordDiff("", "fully made up");
    expect(added(segments)).toEqual(["fully", "made", "up"]);
  });

  it("handles empty formatted — produces no segments", () => {
    const segments = wordDiff("anything", "");
    expect(segments).toEqual([]);
  });

  it("treats apostrophes inside words as part of the token", () => {
    const segments = wordDiff("we're going there", "We're going there now");
    expect(added(segments)).toEqual(["now"]);
  });
});
