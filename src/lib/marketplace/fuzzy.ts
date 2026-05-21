/**
 * Tiny in-house fuzzy matcher for the Workshop Marketplace search box.
 *
 * ## Why not Fuse.js?
 *
 * The v2.0.2 plan called for Fuse.js. We pivoted to a homemade matcher
 * during implementation for three reasons:
 *
 *   1. **Offline-first ideology.** Slab's pitch is "no network, no
 *      surveillance." Pulling in a 100kB+ JS dep with regex + transitive
 *      packages goes against the bundle-size and audit-surface goals.
 *   2. **Score-aware highlights.** We need character-range highlights
 *      to ship the WOW moment (live-highlighting matched substrings in
 *      result cards). Fuse exposes match positions but the integration
 *      story for Svelte 5 is awkward; rolling our own gives us byte-
 *      exact ranges per haystack field for free.
 *   3. **Surface area is small.** Browse fields = name + description
 *      + author + id + categories + tags. ~6 strings per entry, a few
 *      dozen entries. A naive O(n·m) matcher returns < 1ms even on
 *      slow hardware. Fuse's Bitap is overkill.
 *
 * The algorithm is deliberately simple: case-insensitive substring +
 * weighted fuzzy subsequence with tightness bonus. See `scoreMatch`.
 *
 * If the matcher ever proves inadequate (large indexes, multilingual
 * queries, typo tolerance demands), we'll graduate to MiniSearch or
 * Fuse — but only after a benchmark shows we actually need it.
 */

/**
 * One contiguous character range inside a haystack string that was
 * matched by the query — emitted by `scoreMatch` so the UI can render
 * <mark> tags around the matched chars.
 */
export interface MatchRange {
  /** Inclusive start char index into the original haystack. */
  start: number;
  /** Exclusive end char index. */
  end: number;
}

/**
 * Result of fuzzy-matching a query against one field of one entry.
 * Higher score = better match. Score 0 means no match at all.
 */
export interface FieldMatch {
  /** Raw match score. 0 = no match. */
  score: number;
  /** Character ranges to highlight. Empty when score == 0. */
  ranges: MatchRange[];
}

/**
 * Score a query against a haystack string.
 *
 * Scoring (additive):
 *   - +1000 if haystack starts with query (case-insensitive)
 *   - +500  if haystack contains query as a contiguous substring
 *   - +(100 to 300) for fuzzy subsequence match, tightness-weighted
 *   - +50   bonus if matched chars are at word boundaries (after
 *           space, dash, dot)
 *
 * Empty query returns a sentinel { score: 1, ranges: [] } so callers
 * that pass "" through the matcher get a stable non-zero score.
 */
export function scoreMatch(query: string, haystack: string): FieldMatch {
  if (!query) return { score: 1, ranges: [] };
  if (!haystack) return { score: 0, ranges: [] };

  const q = query.toLowerCase();
  const h = haystack.toLowerCase();

  // Exact prefix
  if (h.startsWith(q)) {
    return { score: 1000, ranges: [{ start: 0, end: q.length }] };
  }

  // Substring
  const sub = h.indexOf(q);
  if (sub !== -1) {
    return { score: 500, ranges: [{ start: sub, end: sub + q.length }] };
  }

  // Fuzzy subsequence — every char of q must appear in h in order.
  // We greedily anchor each char, prefer matches close together, and
  // give a bonus when a match lands at a word boundary.
  const ranges: MatchRange[] = [];
  let qi = 0;
  let lastMatch = -2;
  let tightness = 0; // sum of gap penalties
  let boundaryBonus = 0;
  let rangeStart = -1;

  for (let hi = 0; hi < h.length && qi < q.length; hi++) {
    if (h.charCodeAt(hi) === q.charCodeAt(qi)) {
      const isContiguous = hi === lastMatch + 1;
      if (isContiguous && rangeStart !== -1) {
        // extend current range — done at flush time
      } else {
        if (rangeStart !== -1) {
          ranges.push({ start: rangeStart, end: lastMatch + 1 });
        }
        rangeStart = hi;
      }
      const prev = hi > 0 ? h.charAt(hi - 1) : " ";
      if (/[\s\-._/]/.test(prev)) boundaryBonus += 25;
      if (lastMatch >= 0) tightness += hi - lastMatch - 1;
      lastMatch = hi;
      qi++;
    }
  }
  if (qi < q.length) return { score: 0, ranges: [] };
  if (rangeStart !== -1) ranges.push({ start: rangeStart, end: lastMatch + 1 });

  // tightness penalty: 0 gap = best, big gaps = worse.
  // Base 300, subtract up to 200 for spread.
  const spreadPenalty = Math.min(200, tightness * 4);
  const score = 100 + (300 - spreadPenalty) + boundaryBonus;
  return { score, ranges };
}

/**
 * Multi-field match against an `IndexEntry`-like shape. Returns the
 * best (highest) score across all searched fields plus a map of which
 * field had matches and their ranges. Callers use the field-level
 * ranges to highlight inside the corresponding piece of UI.
 */
export interface EntryFuzzyResult {
  score: number;
  fieldRanges: {
    name: MatchRange[];
    description: MatchRange[];
    author: MatchRange[];
    id: MatchRange[];
    categories: MatchRange[]; // ranges into the *joined* category string
    tags: MatchRange[]; // ranges into the *joined* tag string
  };
}

/**
 * Each haystack field carries a weight. The name and id matter more
 * than the description. Tags and categories are mid-tier.
 */
const FIELD_WEIGHTS = {
  name: 3.0,
  id: 2.5,
  categories: 1.8,
  tags: 1.6,
  description: 1.0,
  author: 0.8,
} as const;

export interface SearchableEntry {
  id: string;
  name: string;
  description: string;
  author: string;
  categories?: string[];
  tags?: string[];
}

export function fuzzyMatchEntry(query: string, entry: SearchableEntry): EntryFuzzyResult {
  const categoriesStr = (entry.categories ?? []).join(" · ");
  const tagsStr = (entry.tags ?? []).join(" · ");

  const name = scoreMatch(query, entry.name);
  const id = scoreMatch(query, entry.id);
  const description = scoreMatch(query, entry.description);
  const author = scoreMatch(query, entry.author);
  const categories = scoreMatch(query, categoriesStr);
  const tags = scoreMatch(query, tagsStr);

  const total =
    name.score * FIELD_WEIGHTS.name +
    id.score * FIELD_WEIGHTS.id +
    description.score * FIELD_WEIGHTS.description +
    author.score * FIELD_WEIGHTS.author +
    categories.score * FIELD_WEIGHTS.categories +
    tags.score * FIELD_WEIGHTS.tags;

  return {
    score: total,
    fieldRanges: {
      name: name.ranges,
      description: description.ranges,
      author: author.ranges,
      id: id.ranges,
      categories: categories.ranges,
      tags: tags.ranges,
    },
  };
}

/**
 * Render a string with `<mark>` tags around the supplied character
 * ranges. Returns sanitized HTML safe to drop into `{@html ...}` in
 * a Svelte template. Caller-supplied ranges are trusted (they came
 * from `scoreMatch`); we only need to escape the input string.
 *
 * Ranges are assumed non-overlapping and sorted ascending — that's
 * what `scoreMatch` produces.
 */
export function highlightHTML(text: string, ranges: MatchRange[]): string {
  if (ranges.length === 0) return escapeHTML(text);
  const out: string[] = [];
  let cursor = 0;
  for (const r of ranges) {
    if (r.start > cursor) out.push(escapeHTML(text.slice(cursor, r.start)));
    out.push("<mark>");
    out.push(escapeHTML(text.slice(r.start, r.end)));
    out.push("</mark>");
    cursor = r.end;
  }
  if (cursor < text.length) out.push(escapeHTML(text.slice(cursor)));
  return out.join("");
}

function escapeHTML(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
