/** One-based, inclusive ranges shared by the preview and split command. */
export interface SplitRange { start: number; end: number }
export type SplitMode = "ranges" | "every" | "parts";

export interface OddEvenOutput {
  parity: "odd" | "even";
  count: number;
  preview: string;
}

function validateTotal(total: number | null): number {
  if (total === null) throw new Error("Choose a readable PDF to preview its pages.");
  if (!Number.isSafeInteger(total) || total < 1 || total > 0xffffffff)
    throw new Error("The PDF has no valid pages.");
  return total;
}

/** Uses page positions, starting at 1, rather than printed page labels. */
export function oddEvenSplitPlan(total: number | null): OddEvenOutput[] {
  const n = validateTotal(total);
  return (["odd", "even"] as const).flatMap((parity, i) => {
    const first = i + 1;
    const count = Math.floor((n - first) / 2) + 1;
    if (count === 0) return [];
    const last = first + (count - 1) * 2;
    const preview = count <= 3
      ? Array.from({ length: count }, (_, j) => first + j * 2).join(", ")
      : `${first}, ${first + 2}, …, ${last}`;
    return [{ parity, count, preview }];
  });
}

export function splitPlan(
  total: number | null,
  mode: SplitMode,
  rangeText: string,
  size: number,
  parts: number,
): SplitRange[] {
  total = validateTotal(total);

  if (mode === "ranges") {
    const tokens = rangeText.split(",").map(s => s.trim());
    if (tokens.some(s => !s)) throw new Error("Add complete ranges, e.g. 1-3, 5, 7-9.");
    const ranges = tokens.map(token => {
      const match = /^(\d+)(?:-(\d+))?$/.exec(token);
      if (!match) throw new Error(`Invalid range: "${token}".`);
      const start = Number(match[1]);
      const end = Number(match[2] ?? match[1]);
      if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 1 || end < start || end > total)
        throw new Error(`Range "${token}" must be within pages 1–${total}.`);
      return { start, end };
    });
    const sorted = [...ranges].sort((a, b) => a.start - b.start);
    if (sorted.some((r, i) => i > 0 && r.start <= sorted[i - 1].end))
      throw new Error("Ranges overlap — a page may only appear in one output file.");
    return ranges;
  }

  const value = mode === "parts" ? parts : size;
  if (!Number.isSafeInteger(value) || value < 1 || (mode === "parts" && value > total))
    throw new Error(mode === "parts"
      ? `Choose a whole number of files from 1 to ${total}.`
      : "Pages per chunk must be a positive whole number.");
  const count = mode === "parts" ? parts : Math.ceil(total / size);
  const base = mode === "parts" ? Math.floor(total / parts) : size;
  const extra = mode === "parts" ? total % parts : 0;
  const ranges: SplitRange[] = [];
  let start = 1;
  for (let i = 0; i < count; i++) {
    const end = Math.min(total, start + base + (i < extra ? 1 : 0) - 1);
    ranges.push({ start, end });
    start = end + 1;
  }
  return ranges;
}
