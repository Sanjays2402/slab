import type { SplitRange } from "./splitPlan";

/** Blank means all pages. Every nonblank token must be an exact page or range. */
export function parseMergeRanges(text: string): SplitRange[] {
  if (!text.trim()) return [];
  const page = (value: string): number => {
    const n = Number(value);
    if (!Number.isSafeInteger(n) || n < 1 || n > 0xffffffff) {
      throw new Error("Page numbers must be positive 32-bit integers.");
    }
    return n;
  };
  return text.split(",").map((token) => {
    const part = token.trim();
    const match = /^(\d+)(?:\s*-\s*(\d+))?$/.exec(part);
    if (!match) throw new Error(`"${part}" isn't a page number or range.`);
    const a = page(match[1]);
    const b = match[2] === undefined ? a : page(match[2]);
    return { start: Math.min(a, b), end: Math.max(a, b) };
  });
}
