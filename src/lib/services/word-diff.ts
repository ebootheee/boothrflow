// Word-level diff for the bench grading UI. Highlights tokens in
// `formatted` that don't have a matching counterpart in `raw`,
// surfacing LLM hallucinations and trailing-fragment completions
// (the failure modes that drove the 2026-05-09 grades down).
//
// Multiset comparison (case-insensitive, punctuation-stripped):
// every word in `raw` "spends" one matching occurrence in `formatted`
// from left-to-right. The first appearance of a word in `formatted`
// matches the first appearance in `raw`; additional appearances that
// have no counterpart get flagged as added. Not position-aware, so a
// word that appears in both but moved positions still counts as
// preserved — adequate for grading-aid purposes; would need an
// LCS-based diff for editor-grade accuracy.

export type DiffSegment = { text: string; added: boolean };

const WORD_RE = /\p{L}[\p{L}\p{N}']*/gu;

function normalize(token: string): string {
  return token.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
}

export function wordDiff(raw: string, formatted: string): DiffSegment[] {
  const counts = new Map<string, number>();
  for (const m of raw.matchAll(WORD_RE)) {
    const k = normalize(m[0]);
    if (k) counts.set(k, (counts.get(k) ?? 0) + 1);
  }

  const segments: DiffSegment[] = [];
  let cursor = 0;
  for (const m of formatted.matchAll(WORD_RE)) {
    const start = m.index ?? 0;
    const end = start + m[0].length;
    if (start > cursor) {
      segments.push({ text: formatted.slice(cursor, start), added: false });
    }
    const k = normalize(m[0]);
    const remaining = counts.get(k) ?? 0;
    if (remaining > 0) {
      counts.set(k, remaining - 1);
      segments.push({ text: m[0], added: false });
    } else {
      segments.push({ text: m[0], added: true });
    }
    cursor = end;
  }
  if (cursor < formatted.length) {
    segments.push({ text: formatted.slice(cursor), added: false });
  }
  return segments;
}
