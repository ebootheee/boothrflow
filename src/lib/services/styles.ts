export const STYLES = ["raw", "formal", "casual", "excited", "very-casual"] as const;
export type Style = (typeof STYLES)[number];

export function isStyle(value: unknown): value is Style {
  return typeof value === "string" && (STYLES as readonly string[]).includes(value);
}
