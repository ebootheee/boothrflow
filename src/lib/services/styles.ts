// Cleanup style — single axis: how aggressively the LLM may
// restructure the raw transcript. See
// `docs/waves/wave-6-engine-and-formatting.md` Phase 0 for the
// rationale (tone variation turned out to be noise; users actually
// switch between "leave my words alone" and "organize this for me").
//
// Order matters — the picker renders these in array order.
export const STYLES = ["raw", "light", "moderate", "assertive", "captains-log"] as const;
export type Style = (typeof STYLES)[number];

// Subset shown in the main structure-aggressiveness picker.
// Captain's Log is rendered separately as a "fun preset."
export const STRUCTURE_STYLES = ["raw", "light", "moderate", "assertive"] as const;
export type StructureStyle = (typeof STRUCTURE_STYLES)[number];

export function isStyle(value: unknown): value is Style {
  return typeof value === "string" && (STYLES as readonly string[]).includes(value);
}
