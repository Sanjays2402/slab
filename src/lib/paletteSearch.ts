// Command Palette search core — v3.40.0 "Lumen" Slice 1.
//
// Pure, DOM-free scoring + highlight model for the ⌘K command palette.
// The palette (`CommandPalette.svelte`) used to carry a hand-rolled
// `fuzzyScore(q, hay)` inline with no tests, no match positions, and a
// crude tie-break. This module replaces it with a tested contract that
// also emits the *character ranges* that matched — the missing piece
// the UI needs to render Raycast/Linear-grade live highlighting.
//
// ## Why a palette-specific matcher (not the marketplace one)?
//
// `src/lib/marketplace/fuzzy.ts` already ships a `scoreMatch` for the
// Workshop search box, but it's tuned for a 6-field plugin index and
// emits HTML via `{@html}`. The palette has different needs:
//   - Two fields only (visible `title` + hidden `keywords`), and we
//     only ever highlight the *title* (keyword hits stay invisible —
//     highlighting a substring the user can't see is confusing).
//   - A title prefix must win decisively over a keyword subsequence so
//     "rea" surfaces "Reader" above "Auto-Redact" (keyword: redact).
//   - The UI renders highlights as discrete segments (`splitHighlight`)
//     so the Svelte template can use real <mark> elements instead of
//     `{@html}` — safer for plugin-contributed titles.
//
// Same pure-core / thin-shell discipline as `toastStack.ts` and the
// Hopper helpers: every scoring + range decision is a pure function
// unit-tested without a DOM; the Svelte component only renders.

/** One contiguous matched character span inside a haystack, [start, end). */
export interface PaletteRange {
  /** Inclusive start char index into the original (un-lowercased) string. */
  start: number;
  /** Exclusive end char index. */
  end: number;
}

/** Result of scoring a query against one haystack field. */
export interface PaletteFieldScore {
  /** Raw match score; 0 means no match. Higher is better. */
  score: number;
  /** Character ranges that matched, ascending + non-overlapping. */
  ranges: PaletteRange[];
}

// --- Scoring constants (additive, so tiers never cross) --------------
//   prefix   >> substring-at-boundary > substring > subsequence
const SCORE_PREFIX = 1000;
const SCORE_SUBSTRING = 600;
const SCORE_SUBSEQUENCE_BASE = 200;
/** Bonus when a substring/subsequence char lands on a word boundary. */
const BONUS_BOUNDARY = 80;
/** Per-extra-contiguous-char bonus inside a subsequence run. */
const BONUS_CONTIGUOUS = 18;
/** Spread penalty cap for a loose subsequence (gappy matches rank low). */
const PENALTY_SPREAD_CAP = 150;

/** A char counts as a word boundary if the previous char is a separator. */
const BOUNDARY_RE = /[\s\-._/:·|()]/;

/**
 * Tiny tie-break nudge favouring shorter haystacks ("Sign" over
 * "Signet batch" for query "sign"). Always < 1 so it never crosses a
 * scoring tier. NaN/empty -> 0.
 */
function shortnessNudge(len: number): number {
  if (!Number.isFinite(len) || len <= 0) return 0;
  return 1 / (len + 10);
}

/**
 * Score `query` against a single `haystack` string, returning the score
 * and the matched character ranges (for highlighting).
 *
 * Tiers (case-insensitive):
 *   - prefix     : haystack starts with query
 *   - substring  : query appears contiguously somewhere inside
 *   - subsequence: every query char appears in order (gappy ok)
 *
 * Empty query returns a neutral `{ score: 1, ranges: [] }` so callers
 * that pass "" through still get a stable positive score. A query that
 * doesn't match returns `{ score: 0, ranges: [] }`.
 */
export function scorePaletteField(query: string, haystack: string): PaletteFieldScore {
  if (!query) return { score: 1, ranges: [] };
  if (!haystack) return { score: 0, ranges: [] };

  const q = query.toLowerCase();
  const h = haystack.toLowerCase();

  // Tier 1 — prefix.
  if (h.startsWith(q)) {
    return {
      score: SCORE_PREFIX + shortnessNudge(h.length),
      ranges: [{ start: 0, end: q.length }],
    };
  }

  // Tier 2 — contiguous substring.
  const sub = h.indexOf(q);
  if (sub !== -1) {
    const onBoundary = sub === 0 || BOUNDARY_RE.test(h.charAt(sub - 1));
    return {
      score: SCORE_SUBSTRING + (onBoundary ? BONUS_BOUNDARY : 0) + shortnessNudge(h.length),
      ranges: [{ start: sub, end: sub + q.length }],
    };
  }

  // Tier 3 — fuzzy subsequence. Greedily anchor each query char, merging
  // adjacent matched indices into contiguous ranges and rewarding tight,
  // boundary-aligned runs.
  const ranges: PaletteRange[] = [];
  let qi = 0;
  let lastIdx = -2;
  let runStart = -1;
  let contiguousBonus = 0;
  let boundaryBonus = 0;
  let spread = 0;

  for (let hi = 0; hi < h.length && qi < q.length; hi++) {
    if (h.charCodeAt(hi) !== q.charCodeAt(qi)) continue;
    const isContiguous = hi === lastIdx + 1;
    if (isContiguous && runStart !== -1) {
      contiguousBonus += BONUS_CONTIGUOUS;
    } else {
      if (runStart !== -1) ranges.push({ start: runStart, end: lastIdx + 1 });
      runStart = hi;
      if (hi === 0 || BOUNDARY_RE.test(h.charAt(hi - 1))) boundaryBonus += BONUS_BOUNDARY * 0.5;
    }
    if (lastIdx >= 0 && !isContiguous) spread += hi - lastIdx - 1;
    lastIdx = hi;
    qi++;
  }

  if (qi < q.length) return { score: 0, ranges: [] };
  if (runStart !== -1) ranges.push({ start: runStart, end: lastIdx + 1 });

  const spreadPenalty = Math.min(PENALTY_SPREAD_CAP, spread * 3);
  const score =
    SCORE_SUBSEQUENCE_BASE +
    contiguousBonus +
    boundaryBonus -
    spreadPenalty +
    shortnessNudge(h.length);
  return { score: Math.max(1, score), ranges };
}

/** A palette entry as far as search cares: a visible title + hidden keywords. */
export interface PaletteSearchable {
  title: string;
  keywords?: string;
}

/** Combined score for an entry plus the ranges to highlight in its title. */
export interface PaletteEntryScore {
  /** Best weighted score across title + keywords; 0 = filtered out. */
  score: number;
  /** Ranges into `title` only (keyword matches are never highlighted). */
  titleRanges: PaletteRange[];
}

// The visible title outweighs the hidden keyword bag so a title hit
// always beats a keyword-only hit of the same tier.
const WEIGHT_TITLE = 1;
const WEIGHT_KEYWORDS = 0.55;

/**
 * Score a palette entry. Ranks on the higher of the weighted title score
 * and the weighted keyword score, but only ever returns *title* ranges —
 * so a row that matched purely on a hidden keyword still shows in the
 * list (correct rank) without confusing highlight marks on its title.
 */
export function scorePaletteEntry(query: string, entry: PaletteSearchable): PaletteEntryScore {
  const title = scorePaletteField(query, entry.title ?? "");
  const kw = entry.keywords
    ? scorePaletteField(query, entry.keywords)
    : { score: 0, ranges: [] as PaletteRange[] };
  const score = Math.max(title.score * WEIGHT_TITLE, kw.score * WEIGHT_KEYWORDS);
  return { score, titleRanges: score > 0 ? title.ranges : [] };
}

/** One slice of a title split for rendering: matched (`hit`) or not. */
export interface HighlightSegment {
  text: string;
  hit: boolean;
}

/**
 * Split `text` into alternating hit / non-hit segments for the supplied
 * ranges so a Svelte template can render real <mark> elements (no
 * `{@html}`). Ranges are clamped + sorted + merged defensively so a
 * malformed range list can never drop or duplicate characters — the
 * concatenation of all segment texts always equals the input verbatim.
 *
 * Empty ranges (or empty text) yield a single non-hit segment so the
 * caller can always `{#each}` over the result uniformly.
 */
export function splitHighlight(text: string, ranges: PaletteRange[]): HighlightSegment[] {
  if (!text) return [];
  const clean = normalizeRanges(ranges, text.length);
  if (clean.length === 0) return [{ text, hit: false }];

  const out: HighlightSegment[] = [];
  let cursor = 0;
  for (const r of clean) {
    if (r.start > cursor) out.push({ text: text.slice(cursor, r.start), hit: false });
    out.push({ text: text.slice(r.start, r.end), hit: true });
    cursor = r.end;
  }
  if (cursor < text.length) out.push({ text: text.slice(cursor), hit: false });
  return out;
}

/**
 * Clamp ranges into [0, len), drop empty/invalid ones, sort ascending,
 * and merge overlapping/adjacent spans. Defensive so `splitHighlight`
 * never produces overlapping <mark>s or out-of-bounds slices.
 */
export function normalizeRanges(ranges: PaletteRange[], len: number): PaletteRange[] {
  if (!Array.isArray(ranges) || ranges.length === 0 || len <= 0) return [];
  const clamped: PaletteRange[] = [];
  for (const r of ranges) {
    if (!r) continue;
    const start = Math.max(0, Math.min(len, Math.floor(r.start)));
    const end = Math.max(0, Math.min(len, Math.floor(r.end)));
    if (Number.isFinite(start) && Number.isFinite(end) && end > start) {
      clamped.push({ start, end });
    }
  }
  if (clamped.length === 0) return [];
  clamped.sort((a, b) => a.start - b.start || a.end - b.end);
  const merged: PaletteRange[] = [clamped[0]];
  for (let i = 1; i < clamped.length; i++) {
    const prev = merged[merged.length - 1];
    const cur = clamped[i];
    if (cur.start <= prev.end) {
      prev.end = Math.max(prev.end, cur.end);
    } else {
      merged.push({ ...cur });
    }
  }
  return merged;
}

// --- Keyboard navigation (Lumen Slice 3) -----------------------------
//
// The palette's list cursor needs Raycast-grade movement: arrows that
// WRAP at the ends (so ↓ on the last row jumps to the first), Home/End
// to leap to either extreme, and PageUp/PageDown to page through a long
// list. The classifier + index math live here as pure functions so
// every wrap/clamp branch is testable without a DOM KeyboardEvent; the
// Svelte handler only translates the result into `selected` + scroll.

/** Minimal shape the nav classifier reads off a KeyboardEvent. */
export interface PaletteNavEvent {
  key: string;
}

export type PaletteNavIntent = "next" | "prev" | "first" | "last" | "page-up" | "page-down";

/** Rows a PageUp / PageDown press moves the cursor. */
export const PALETTE_PAGE_JUMP = 8;

/**
 * Classify a keypress into a navigation intent, or null if it isn't a
 * nav key (so the caller leaves it for Enter/Escape/typing). Modifiers
 * are intentionally ignored here — the palette input owns no Cmd/Ctrl
 * nav chords, so a bare Arrow/Home/End/PageUp/PageDown is unambiguous.
 */
export function classifyPaletteNav(ev: PaletteNavEvent): PaletteNavIntent | null {
  switch (ev.key) {
    case "ArrowDown":
      return "next";
    case "ArrowUp":
      return "prev";
    case "Home":
      return "first";
    case "End":
      return "last";
    case "PageUp":
      return "page-up";
    case "PageDown":
      return "page-down";
    default:
      return null;
  }
}

/**
 * Resolve the next cursor index for a nav intent over a list of `count`
 * items, given the `current` index. Arrows WRAP (next past the end ->
 * 0, prev before the start -> last); Home/End jump to the extremes;
 * Page up/down clamp (never wrap) by `page` rows. `current` is clamped
 * into range first so a stale index can't escape. Empty list -> 0.
 */
export function nextPaletteIndex(
  intent: PaletteNavIntent,
  current: number,
  count: number,
  page: number = PALETTE_PAGE_JUMP,
): number {
  if (!Number.isFinite(count) || count <= 0) return 0;
  const last = count - 1;
  const cur = Number.isFinite(current) ? Math.max(0, Math.min(last, Math.floor(current))) : 0;
  const step = Number.isFinite(page) && page > 0 ? Math.floor(page) : PALETTE_PAGE_JUMP;
  switch (intent) {
    case "next":
      return cur >= last ? 0 : cur + 1;
    case "prev":
      return cur <= 0 ? last : cur - 1;
    case "first":
      return 0;
    case "last":
      return last;
    case "page-up":
      return Math.max(0, cur - step);
    case "page-down":
      return Math.min(last, cur + step);
    default:
      return cur;
  }
}

