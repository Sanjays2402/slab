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

// --- Frecency ranking (Lumen Slice 4) --------------------------------
//
// The empty-query "Recently used" group floated commands by pure
// recency, so a command invoked once 5 seconds ago outranked one used
// 50 times a day. Raycast/Arc rank by *frecency* — frequency blended
// with recency — so your daily-driver commands stay on top even if the
// very last thing you touched was a one-off.
//
// We only persist an aggregate (count + lastUsedAt) per command, not a
// full visit log, so this approximates Mozilla's frecency: a recency
// BUCKET multiplier (recency dominates) times a logarithmically-tempered
// frequency term (heavy use still wins ties without swamping recency).

/** Persisted usage record for one command id. */
export interface FrecencyRecord {
  id: string;
  /** Total times invoked. */
  count: number;
  /** Epoch ms of the most recent invocation. */
  lastUsedAt: number;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const WEEK = 7 * DAY;
const MONTH = 30 * DAY;

/**
 * Recency multiplier for an age in ms. Bucketed (not continuous) so the
 * ordering is stable and predictable, with STEEP ratios between buckets
 * so recency dominates: frequency (log-tempered below) can only overtake
 * within the same bucket or across a near-adjacent one, never leapfrog a
 * "used in the last 5 minutes" command with a stale heavy-use one.
 */
export function recencyWeight(ageMs: number): number {
  if (!Number.isFinite(ageMs) || ageMs < 0) return 1000; // treat future/NaN as "just now"
  if (ageMs < 5 * MINUTE) return 1000;
  if (ageMs < HOUR) return 350;
  if (ageMs < DAY) return 120;
  if (ageMs < WEEK) return 50;
  if (ageMs < MONTH) return 20;
  return 8;
}

/**
 * Frecency score for a record at time `now`. Higher = should rank
 * sooner. `(1 + ln(count)) * recencyWeight(age)` — the log temper means
 * frequency rewards heavy use and breaks ties without ever swamping the
 * recency bucket. A count <= 0 or missing record scores 0.
 */
export function frecencyScore(record: FrecencyRecord, now: number): number {
  if (!record || !Number.isFinite(record.count) || record.count <= 0) return 0;
  const last = Number.isFinite(record.lastUsedAt) ? record.lastUsedAt : 0;
  const age = now - last;
  const freqTerm = 1 + Math.log(record.count);
  return freqTerm * recencyWeight(age);
}

/**
 * Rank records by frecency, returning a map of id -> rank where rank 0
 * is the strongest. Ties break by most-recent-first, then id for a
 * stable order. Records with a non-positive count are dropped.
 */
export function rankFrecency(records: FrecencyRecord[], now: number): Record<string, number> {
  if (!Array.isArray(records)) return {};
  const scored = records
    .filter((r) => r && Number.isFinite(r.count) && r.count > 0)
    .map((r) => ({ id: r.id, score: frecencyScore(r, now), lastUsedAt: r.lastUsedAt }))
    .sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score;
      const al = Number.isFinite(a.lastUsedAt) ? a.lastUsedAt : 0;
      const bl = Number.isFinite(b.lastUsedAt) ? b.lastUsedAt : 0;
      if (bl !== al) return bl - al;
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
    });
  const out: Record<string, number> = {};
  for (let i = 0; i < scored.length; i++) out[scored[i].id] = i;
  return out;
}

/**
 * Fold a fresh invocation of `id` into an existing record list: bump the
 * matching record's count + timestamp, or insert a new count-1 record.
 * Returns a NEW array (never mutates input) capped to `limit` records,
 * evicting the lowest-frecency entries when over capacity so the store
 * can't grow without bound. The bumped/new record is always retained.
 */
export function recordFrecency(
  records: FrecencyRecord[],
  id: string,
  now: number,
  limit: number,
): FrecencyRecord[] {
  const base = Array.isArray(records) ? records : [];
  if (!id) return base.slice(0, Math.max(0, limit));
  let found = false;
  const next: FrecencyRecord[] = base.map((r) => {
    if (r.id === id) {
      found = true;
      return { id, count: (Number.isFinite(r.count) ? r.count : 0) + 1, lastUsedAt: now };
    }
    return r;
  });
  if (!found) next.push({ id, count: 1, lastUsedAt: now });
  if (next.length <= limit) return next;
  // Over capacity — keep the top `limit` by frecency, but never evict the
  // record we just touched.
  const ranks = rankFrecency(next, now);
  const keep = next
    .slice()
    .sort((a, b) => (ranks[a.id] ?? Infinity) - (ranks[b.id] ?? Infinity))
    .slice(0, limit);
  if (!keep.some((r) => r.id === id)) {
    keep[keep.length - 1] = next.find((r) => r.id === id)!;
  }
  return keep;
}

// --- Shortcut chord hints (Lumen Slice 5) ----------------------------
//
// Raycast prints the bound keyboard shortcut on each palette row that
// has one, so users learn the chord as they mouse. Slab already has a
// full keymap (keymap.ts ActionId -> binding), but the palette never
// surfaced it on rows. This pure table maps a palette ACTION id (e.g.
// "panel:hopper", "home:open", "panel:forms:batch") to the keymap action
// id whose binding it triggers — or null when the row has no global
// chord. Kept dependency-free (returns a plain string) so the pure core
// never imports the keymap store; the Svelte shell casts the result to
// ActionId and calls prettyBindingFor().

const PALETTE_KEYMAP_IDS: Record<string, string> = {
  // Panel switches that have a dedicated global chord.
  "panel:bedrock": "bedrock.open",
  "panel:press": "press.open",
  "panel:forms": "forms.open",
  "panel:atelier": "atelier.open",
  "panel:hopper": "hopper.open",
  "panel:theater": "theater.start",
  // Standalone palette commands.
  "theater:open": "theater.start",
  "home:open": "home.open",
  "home:continue": "home.continue",
  "library:search": "library.search",
  "help:shortcuts": "shortcuts.show",
  // Forms sub-tabs (Quill hub) — each its own chord.
  "panel:forms:batch": "quill.batch",
  "panel:forms:design": "quill.designer",
  "panel:forms:detect": "quill.autodetect",
  "panel:forms:smartfill": "quill.smartfill",
  "forms:tour": "quill.tour",
};

/**
 * The keymap action id whose global shortcut this palette row triggers,
 * or null if the row has no bound chord. Pure lookup — the caller resolves
 * the id to a printable chord via the keymap store.
 */
export function paletteKeymapId(paletteActionId: string): string | null {
  if (!paletteActionId) return null;
  return PALETTE_KEYMAP_IDS[paletteActionId] ?? null;
}

// --- Empty-state fallback (Lumen II Slice 1) -------------------------
//
// When a query matches nothing, a bare "No matches" is a dead end. Raycast
// instead either offers a typo-corrected "did you mean" (the closest
// SHORTER query that does match — catches the very common "typed one char
// too many / fat-fingered the end" case) or, failing that, a curated set
// of high-value STARTER commands so the user always has a next move.
//
// This is a pure relax-and-rescore over the same `scorePaletteEntry`
// contract: no DOM, no store. The caller invokes it only when the real
// filter came back empty, then renders the returned ids as live rows.

/** A searchable entry carrying its stable id (so the fallback can name it). */
export interface PaletteFallbackEntry extends PaletteSearchable {
  id: string;
  group?: string;
}

/** Outcome of an empty-result fallback. */
export interface PaletteFallback {
  /**
   * "typo"    — a strictly-shorter relaxed query matched; `relaxed` holds it.
   * "starter" — nothing relaxed matched; `ids` are curated starter commands.
   * "none"    — nothing to offer (empty query, or no starters present).
   */
  kind: "typo" | "starter" | "none";
  /** The relaxed query that produced the typo matches; "" otherwise. */
  relaxed: string;
  /** Suggested entry ids, best first, de-duplicated, capped to the limit. */
  ids: string[];
}

/**
 * Curated high-value commands offered when even relaxation finds nothing.
 * Filtered against the entries actually present, so a missing one is just
 * skipped (never renders a dead row). Order = offer priority.
 */
export const STARTER_SUGGESTION_IDS: readonly string[] = [
  "home:open",
  "library:search",
  "settings:keymap",
  "help:shortcuts",
];

/** Shortest prefix we'll relax to — a 1-char query matches too much to help. */
const FALLBACK_MIN_PREFIX = 2;

/**
 * Suggest a fallback for a query that matched nothing. Tries progressively
 * shorter PREFIXES of the (trimmed) query down to FALLBACK_MIN_PREFIX chars;
 * the first prefix that scores any entries yields a "typo" suggestion with
 * the top `limit` ids. (A multi-word query naturally relaxes through its
 * first word, so "redact pls" -> "redact".) If no prefix matches, returns
 * the present STARTER ids. Pure; never mutates inputs.
 */
export function suggestPaletteFallback(
  query: string,
  entries: PaletteFallbackEntry[],
  limit: number = 6,
): PaletteFallback {
  const q = (query ?? "").trim();
  const cap = Number.isFinite(limit) && limit > 0 ? Math.floor(limit) : 6;
  const list = Array.isArray(entries) ? entries : [];
  if (!q || list.length === 0) {
    return { kind: "none", relaxed: "", ids: [] };
  }

  // Relax by trimming trailing chars: the input already failed to match,
  // so start strictly shorter and walk down to the minimum useful prefix.
  for (let k = q.length - 1; k >= FALLBACK_MIN_PREFIX; k--) {
    const prefix = q.slice(0, k);
    const scored = list
      .map((e) => ({ id: e.id, score: scorePaletteEntry(prefix, e).score }))
      .filter((x) => x.score > 0)
      .sort((a, b) => b.score - a.score);
    if (scored.length > 0) {
      return { kind: "typo", relaxed: prefix, ids: dedupeIds(scored.map((x) => x.id), cap) };
    }
  }

  // Nothing relaxed — offer the curated starters that are actually present.
  const present = new Set(list.map((e) => e.id));
  const starters = STARTER_SUGGESTION_IDS.filter((id) => present.has(id));
  if (starters.length === 0) return { kind: "none", relaxed: "", ids: [] };
  return { kind: "starter", relaxed: "", ids: dedupeIds(starters, cap) };
}

/** First-seen de-dupe of an id list, capped to `limit`. */
function dedupeIds(ids: string[], limit: number): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const id of ids) {
    if (seen.has(id)) continue;
    seen.add(id);
    out.push(id);
    if (out.length >= limit) break;
  }
  return out;
}

// --- Typed scope sigils (Lumen II Slice 2) ---------------------------
//
// Power users live in VSCode's ⌘P, where a leading sigil scopes the
// search: ">" runs commands, "@" jumps to symbols, "#" searches workspace
// symbols. The Slab palette mixes files, panel switches, appearance, and
// commands into one flat list — so a leading sigil that narrows the list
// to one CLASS of result lets a user who knows what they want skip the
// noise: "@invoice" only files, "#dark" only themes, ">redact" only
// commands (excluding file navigation, like VSCode).
//
// Pure parse + membership predicate; the Svelte shell renders a scope
// pill and feeds the stripped `term` to the existing scorer.

export type PaletteScope = "all" | "commands" | "files" | "appearance";

/** Decomposition of a raw query into an optional scope + the search term. */
export interface PaletteScopeParse {
  scope: PaletteScope;
  /** Query with the leading sigil stripped + trimmed. */
  term: string;
  /** The sigil that triggered the scope, or "" when unscoped. */
  sigil: string;
}

/** Leading-character -> scope. Mirrors VSCode's ⌘P vocabulary. */
export const PALETTE_SCOPE_SIGILS: Readonly<Record<string, PaletteScope>> = {
  ">": "commands",
  "@": "files",
  "#": "appearance",
};

/** Groups (from CommandPalette's `group` field) that represent files. */
const FILE_GROUPS: ReadonlySet<string> = new Set(["Pinned", "Recent", "Recent files"]);

/**
 * Parse a raw palette query. A leading sigil (after optional whitespace)
 * selects a scope and is stripped from the returned `term`; anything else
 * is the unscoped "all" search. Pure; tolerant of null/empty.
 */
export function parsePaletteScope(query: string): PaletteScopeParse {
  const raw = query ?? "";
  const lead = raw.replace(/^\s+/, "");
  const first = lead.charAt(0);
  const scope = PALETTE_SCOPE_SIGILS[first];
  if (scope) {
    return { scope, term: lead.slice(1).trim(), sigil: first };
  }
  return { scope: "all", term: raw.trim(), sigil: "" };
}

/**
 * Whether an entry in `group` belongs to `scope`. "all" matches
 * everything; "files" matches the file groups; "appearance" matches the
 * Appearance group; "commands" matches everything that ISN'T a file
 * (VSCode's ">" excludes file navigation — appearance toggles are still
 * commands).
 */
export function entryMatchesScope(group: string, scope: PaletteScope): boolean {
  switch (scope) {
    case "files":
      return FILE_GROUPS.has(group);
    case "appearance":
      return group === "Appearance";
    case "commands":
      return !FILE_GROUPS.has(group);
    case "all":
    default:
      return true;
  }
}

/** Human label for a scope (for the input pill); "" for the unscoped case. */
export function describePaletteScope(scope: PaletteScope): string {
  switch (scope) {
    case "commands":
      return "Commands";
    case "files":
      return "Files";
    case "appearance":
      return "Appearance";
    case "all":
    default:
      return "";
  }
}

// --- Group-jump navigation (Lumen II Slice 3) ------------------------
//
// The action catalog is 100+ entries across a dozen labelled groups
// (Panels, Forms, Appearance, Library, Stack, Theater…). Arrowing one
// row at a time across that is slow; macOS Finder / Linear / Raycast all
// let you leap by SECTION. Slab's palette already owns bare arrows for
// per-row movement (Lumen Slice 3), so the natural chord for section
// jumps is Cmd/Ctrl+Arrow — currently unclaimed (the onKey handler bails
// on any modifier). This pure core resolves the chord + the target index.

/** Minimal shape the group-nav classifier reads off a KeyboardEvent. */
export interface PaletteGroupNavEvent {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

export type PaletteGroupNavIntent = "group-next" | "group-prev";

/**
 * Classify a keypress into a group-jump intent, or null. Cmd (mac) or
 * Ctrl (win/linux) + ArrowUp/Down — matching the palette's existing
 * meta-or-ctrl equivalence. Alt or Shift disqualify so other chords stay
 * free.
 */
export function classifyPaletteGroupNav(ev: PaletteGroupNavEvent): PaletteGroupNavIntent | null {
  if (ev.altKey || ev.shiftKey) return null;
  if (!(ev.metaKey || ev.ctrlKey)) return null;
  if (ev.key === "ArrowDown") return "group-next";
  if (ev.key === "ArrowUp") return "group-prev";
  return null;
}

/**
 * Flat start index of each group, given the groups' item counts in order.
 * `[3, 2, 4]` -> `[0, 3, 5]`. Non-finite/negative sizes count as 0.
 */
export function groupStartIndices(sizes: number[]): number[] {
  const out: number[] = [];
  let acc = 0;
  if (!Array.isArray(sizes)) return out;
  for (const s of sizes) {
    out.push(acc);
    acc += Number.isFinite(s) && s > 0 ? Math.floor(s) : 0;
  }
  return out;
}

/** Sorted, de-duplicated, in-range group heads (always anchored at 0). */
function normalizeHeads(starts: number[], count: number): number[] {
  const set = new Set<number>([0]);
  if (Array.isArray(starts)) {
    for (const s of starts) {
      if (Number.isFinite(s) && s >= 0 && s < count) set.add(Math.floor(s));
    }
  }
  return Array.from(set).sort((a, b) => a - b);
}

/**
 * Resolve the cursor index for a group-jump over `count` rows whose group
 * heads are at `starts`. `current` is clamped into range first.
 *
 *   group-next: head of the next group; if already in the LAST group,
 *               drop to the very last row (so Cmd+Down always advances).
 *   group-prev: if below the current group's head, leap UP to that head
 *               (first press = top of section); if already at the head,
 *               leap to the previous group's head; clamps at row 0.
 *
 * This two-stage prev mirrors editors' "jump to section top, then to the
 * previous section" — discoverable and never a dead press. Pure.
 */
export function nextGroupIndex(
  starts: number[],
  current: number,
  intent: PaletteGroupNavIntent,
  count: number,
): number {
  if (!Number.isFinite(count) || count <= 0) return 0;
  const last = count - 1;
  const cur = Number.isFinite(current) ? Math.max(0, Math.min(last, Math.floor(current))) : 0;
  const heads = normalizeHeads(starts, count);

  // Index of the group containing `cur` = last head <= cur.
  let gi = 0;
  for (let i = 0; i < heads.length; i++) {
    if (heads[i] <= cur) gi = i;
    else break;
  }

  if (intent === "group-next") {
    return gi < heads.length - 1 ? heads[gi + 1] : last;
  }
  // group-prev
  const head = heads[gi];
  if (cur > head) return head;
  return gi > 0 ? heads[gi - 1] : 0;
}

// --- Recent-file reading progress (Lumen II Slice 4) -----------------
//
// The empty-query palette lists recent + pinned PDFs, but the rows only
// said "Open · 12m ago". The recent store already tracks per-document
// reading position (lastPage / totalPages); surfacing it as a Raycast-
// grade progress chip ("p.12/80 · 15%", or "Finished") turns the list
// into a genuine "continue where I left off" launcher.
//
// Pure: takes a minimal structural shape (not the RecentFile import) so
// the palette core stays dependency-free, like the rest of this module.

/** The reading-position fields this helper reads off a recent file. */
export interface RecentProgressInput {
  lastPage?: number;
  totalPages?: number;
  pageCount?: number;
}

/** Derived reading-progress summary for a recent-file row. */
export interface RecentProgress {
  /** True when there's a usable last-page + total to show progress for. */
  hasProgress: boolean;
  /** Fraction read in [0, 1]. 0 when unknown. */
  fraction: number;
  /** Rounded percent in [0, 100]. */
  percent: number;
  /** Last viewed page (clamped to total), or 0 when unknown. */
  page: number;
  /** Total page count, or 0 when unknown. */
  total: number;
  /** True once the reader reached the final page. */
  finished: boolean;
  /** Compact chip label ("p.12/80 · 15%", "Finished", or "" when none). */
  label: string;
}

const EMPTY_PROGRESS: RecentProgress = {
  hasProgress: false,
  fraction: 0,
  percent: 0,
  page: 0,
  total: 0,
  finished: false,
  label: "",
};

/** Whether a value is a usable positive integer count/page. */
function posInt(n: unknown): n is number {
  return typeof n === "number" && Number.isFinite(n) && n >= 1;
}

/**
 * Compute the reading-progress summary for a recent file. Returns an
 * all-zero EMPTY summary (hasProgress false, label "") when there's no
 * usable last-page + total — so the row falls back to its plain subtitle.
 * `lastPage` is clamped into [1, total]; reaching the last page reads as
 * "Finished" rather than "100%". Pure; tolerant of missing/garbage fields.
 */
export function recentReadingProgress(file: RecentProgressInput): RecentProgress {
  if (!file) return EMPTY_PROGRESS;
  const total = posInt(file.totalPages) ? Math.floor(file.totalPages) : posInt(file.pageCount) ? Math.floor(file.pageCount) : 0;
  if (total <= 0 || !posInt(file.lastPage)) return EMPTY_PROGRESS;
  const page = Math.min(Math.floor(file.lastPage), total);
  const fraction = Math.max(0, Math.min(1, page / total));
  const percent = Math.round(fraction * 100);
  const finished = page >= total;
  const label = finished ? "Finished" : `p.${page}/${total} · ${percent}%`;
  return { hasProgress: true, fraction, percent, page, total, finished, label };
}

// --- Context-aware footer (Lumen II Slice 5) -------------------------
//
// Raycast/Linear give the palette footer a live pulse: the result count
// ("12 results") and an Enter hint whose VERB matches the selected row —
// "Open" for a file, "Switch to" for a panel, "Apply" for a theme, "Run"
// for everything else. That micro-feedback tells the user what Return is
// about to do before they commit. Pure: derives both strings from the
// selected row's id + group and the result count.

/** The few fields the footer hint reads off the selected row. */
export interface PaletteFooterRow {
  id: string;
  group?: string;
}

/** Pluralised result-count label ("No results" / "1 result" / "N results"). */
export function describePaletteCount(count: number): string {
  const n = Number.isFinite(count) && count > 0 ? Math.floor(count) : 0;
  if (n === 0) return "No results";
  if (n === 1) return "1 result";
  return `${n} results`;
}

/**
 * Verb describing what Enter does on the selected row, by id prefix /
 * group. "Open" (files + windows), "Switch to" (panel/home navigation),
 * "Apply" (appearance), "Run" (everything else, incl. plugin commands).
 * Returns "" when there's no selected row. Pure.
 */
export function paletteActionVerb(row: PaletteFooterRow | null | undefined): string {
  if (!row || !row.id) return "";
  const id = row.id;
  const group = row.group ?? "";
  if (id.startsWith("recent:") || group === "Recent files" || group === "Pinned" || group === "Recent") {
    return "Open";
  }
  if (id.startsWith("panel-window:")) return "Open";
  if (
    id.startsWith("panel:") ||
    id.startsWith("home:") ||
    id === "library:search" ||
    group === "Panels" ||
    group === "Home"
  ) {
    return "Switch to";
  }
  if (id.startsWith("theme:") || id.startsWith("plugin-theme:") || id.startsWith("accent:") || id.startsWith("density:") || group === "Appearance") {
    return "Apply";
  }
  return "Run";
}

// --- Group collapse (Lumen III Slice 1) ------------------------------
//
// The empty-query palette is a browse surface: 100+ commands across a
// dozen labelled sections (Panels, Forms, Appearance, Library, …).
// Linear / Raycast / Finder all let you FOLD a section you don't care
// about so the ones you do are closer. The palette already owns the
// per-section group-jump chord (Lumen II Slice 3); collapse is the
// remaining half. This pure core decides, given the rendered groups and
// the set of collapsed group names, what to DISPLAY (every header stays,
// collapsed groups drop their items) and the flat VISIBLE list the
// keyboard cursor walks (collapsed groups' items are skipped, so arrows
// never land on a hidden row), plus the visible group heads for the
// jump chord. The Svelte shell only renders + toggles.

/** Toggle a group's collapsed state, returning a NEW set (never mutates). */
export function toggleCollapsedGroup(
  collapsed: ReadonlySet<string>,
  group: string,
): Set<string> {
  const next = new Set(collapsed);
  if (next.has(group)) next.delete(group);
  else next.add(group);
  return next;
}

/** One rendered group row: header always shown, items empty when folded. */
export interface CollapsedGroupRow<T> {
  group: string;
  /** Items to render — [] when the group is collapsed. */
  items: T[];
  /** Whether this group is folded. */
  collapsed: boolean;
  /** The group's TRUE item count (for the header badge), folded or not. */
  count: number;
}

/** The display structure + flat cursor space after applying collapse. */
export interface CollapsedView<T> {
  /** Per-group rows to render (headers + visible items). */
  display: CollapsedGroupRow<T>[];
  /** Flat list of items from NON-collapsed groups, in render order. This
   *  is the index space the keyboard cursor lives in — a collapsed
   *  group's items are absent, so the cursor can't land on a hidden row. */
  visible: T[];
  /** Flat start index (into `visible`) of each non-collapsed group, for
   *  the Cmd/Ctrl+Arrow group-jump. */
  starts: number[];
}

/**
 * Partition grouped entries by a collapsed-group set. Every group keeps
 * its header (so a folded section is still visible + re-openable) but a
 * collapsed group renders zero items and contributes nothing to the
 * `visible` cursor space — so arrowing never highlights a hidden row, and
 * the group-jump heads line up with what's on screen. Input is never
 * mutated; a null/garbage grouped list -> all-empty view. Pure.
 */
export function partitionCollapsedGroups<T>(
  grouped: readonly (readonly [string, T[]])[],
  collapsed: ReadonlySet<string>,
): CollapsedView<T> {
  const display: CollapsedGroupRow<T>[] = [];
  const visible: T[] = [];
  const starts: number[] = [];
  if (!Array.isArray(grouped)) return { display, visible, starts };
  const folded = collapsed instanceof Set ? collapsed : new Set<string>();
  for (const entry of grouped) {
    if (!Array.isArray(entry)) continue;
    const group = entry[0];
    const items = Array.isArray(entry[1]) ? entry[1] : [];
    const isCollapsed = folded.has(group);
    display.push({
      group,
      items: isCollapsed ? [] : items,
      collapsed: isCollapsed,
      count: items.length,
    });
    if (!isCollapsed) {
      starts.push(visible.length);
      for (const it of items) visible.push(it);
    }
  }
  return { display, visible, starts };
}

/**
 * Collapse-all: return the set of EVERY group name present in `grouped`,
 * so the palette can fold every section at once. Pure; never mutates the
 * input. A null/garbage grouped list -> empty set. Used by the
 * collapse-all/expand-all toggle so a power user can clear the whole
 * browse surface to its headers in one keystroke, then drill back in.
 */
export function collapseAllGroups<T>(
  grouped: readonly (readonly [string, T[]])[],
): Set<string> {
  const out = new Set<string>();
  if (!Array.isArray(grouped)) return out;
  for (const entry of grouped) {
    if (!Array.isArray(entry)) continue;
    const group = entry[0];
    if (typeof group === "string" && group.length > 0) out.add(group);
  }
  return out;
}

/**
 * Whether EVERY group in `grouped` is currently collapsed — drives the
 * toggle's two-way state (an all-collapsed palette offers "Expand all",
 * otherwise "Collapse all"). True only when there is at least one group
 * and every one of them is in `collapsed`. An empty/garbage grouped list
 * -> false (nothing to expand). Pure.
 */
export function isEveryGroupCollapsed<T>(
  grouped: readonly (readonly [string, T[]])[],
  collapsed: ReadonlySet<string>,
): boolean {
  if (!Array.isArray(grouped) || grouped.length === 0) return false;
  const folded = collapsed instanceof Set ? collapsed : new Set<string>();
  let groupCount = 0;
  for (const entry of grouped) {
    if (!Array.isArray(entry)) continue;
    const group = entry[0];
    if (typeof group !== "string" || group.length === 0) continue;
    groupCount++;
    if (!folded.has(group)) return false;
  }
  return groupCount > 0;
}

/** The legible collapse state of the browse surface (drives the footer). */
export interface CollapseState {
  /** Total named groups present. */
  total: number;
  /** Groups currently OPEN (total minus folded). */
  open: number;
  /** Groups currently folded. */
  collapsed: number;
  /** True when every group is folded (`total > 0` && open === 0). */
  allCollapsed: boolean;
  /** True when no group is folded. */
  noneCollapsed: boolean;
  /** Footer phrase, e.g. "3 of 8 sections open" / "All 8 collapsed". */
  label: string;
}

/**
 * Summarize how much of the grouped browse surface is folded, for the
 * palette footer — so the bulk collapse/expand state is legible at a
 * glance now that collapse-all exists. Counts only real named groups
 * (mirrors `collapseAllGroups`/`isEveryGroupCollapsed`); a folded set
 * entry for a group no longer present is ignored. The label reads
 * "N of M sections open" while some are folded, "All M collapsed" when
 * every one is folded, and "" when nothing is folded (the footer falls
 * back to its result count then). A null/empty grouped list -> all-zero,
 * "" label. Pure + DOM-free.
 */
export function describeCollapseState<T>(
  grouped: readonly (readonly [string, T[]])[],
  collapsed: ReadonlySet<string>,
): CollapseState {
  const empty: CollapseState = {
    total: 0,
    open: 0,
    collapsed: 0,
    allCollapsed: false,
    noneCollapsed: true,
    label: "",
  };
  if (!Array.isArray(grouped) || grouped.length === 0) return empty;
  const folded = collapsed instanceof Set ? collapsed : new Set<string>();
  let total = 0;
  let foldedCount = 0;
  for (const entry of grouped) {
    if (!Array.isArray(entry)) continue;
    const group = entry[0];
    if (typeof group !== "string" || group.length === 0) continue;
    total++;
    if (folded.has(group)) foldedCount++;
  }
  if (total === 0) return empty;
  const open = total - foldedCount;
  const allCollapsed = foldedCount === total;
  const noneCollapsed = foldedCount === 0;
  let label: string;
  if (noneCollapsed) {
    label = "";
  } else if (allCollapsed) {
    label = `All ${total.toLocaleString()} collapsed`;
  } else {
    label = `${open.toLocaleString()} of ${total.toLocaleString()} sections open`;
  }
  return { total, open, collapsed: foldedCount, allCollapsed, noneCollapsed, label };
}

// --- Solo-expand a group (round 51) ----------------------------------
//
// Collapse-all folds every section; the inverse a power user reaches for
// is "show me ONLY this one" — fold every OTHER section so a single group
// fills the surface. Alt-clicking a header drives this (a plain click
// still toggles just that one). The toggle is symmetric: if a group is
// ALREADY solo (it's open and every other group is folded), Alt-clicking
// it again expands everything back open — so the same gesture drills in
// and pops back out. Mirrors the group-iteration discipline of
// `collapseAllGroups` / `isEveryGroupCollapsed`; never mutates its input.

/**
 * Compute the collapsed-set that leaves `group` the only OPEN section —
 * folding every other named group. If `group` is already solo (open while
 * all its siblings are folded), returns an EMPTY set instead, so the same
 * Alt-click that drilled into a group pops the whole surface back open.
 * A group not present in `grouped`, or a null/garbage grouped list, yields
 * the empty set (nothing to solo -> all open). Pure; never mutates.
 */
export function soloExpandGroup<T>(
  grouped: readonly (readonly [string, T[]])[],
  collapsed: ReadonlySet<string>,
  group: string,
): Set<string> {
  const out = new Set<string>();
  if (!Array.isArray(grouped) || typeof group !== "string" || group.length === 0) {
    return out;
  }
  const folded = collapsed instanceof Set ? collapsed : new Set<string>();
  // Gather the real named groups + whether the target is present.
  const names: string[] = [];
  let hasTarget = false;
  for (const entry of grouped) {
    if (!Array.isArray(entry)) continue;
    const name = entry[0];
    if (typeof name !== "string" || name.length === 0) continue;
    names.push(name);
    if (name === group) hasTarget = true;
  }
  if (!hasTarget) return out; // target absent -> nothing to solo, all open
  // Already solo? (target open, every sibling folded.) Toggle back to all-open.
  const targetOpen = !folded.has(group);
  const siblingsAllFolded = names.every((n) => n === group || folded.has(n));
  if (targetOpen && siblingsAllFolded && names.length > 1) return out;
  // Otherwise fold every sibling, leaving only the target open.
  for (const n of names) {
    if (n !== group) out.add(n);
  }
  return out;
}

