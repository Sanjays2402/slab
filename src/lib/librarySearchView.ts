// Library Search view-core — v3.41.0 "Atlas III".
//
// Pure, DOM-free model for the cross-document Library Search panel
// (`LibrarySearchPanel.svelte`) — the highest-visibility surface in the
// app (Cmd+Shift+F). The panel shipped as a flat, mouse-only results
// list: you type, hits come back grouped by document, and you click one
// to open it. There was no keyboard path through the results, no live
// sense of how your query would be interpreted by the FTS engine, no way
// to narrow a large result set without re-running the search, no sort
// control, and no summary of what you're looking at — and ZERO pure-core
// tests on any of it.
//
// This module owns the navigation / interpretation / refine / sort /
// summarize math as pure functions so every branch is unit-tested
// without a DOM — the same pure-core / thin-shell discipline as
// `paletteSearch.ts`, `beaconCacheView.ts`, `shortcutsOverlay.ts`, and
// `toastStack.ts`. Keyboard nav and the in-results refine both REUSE the
// tested palette core (`classifyPaletteNav` / `nextPaletteIndex` /
// `scorePaletteField`) rather than rolling a second nav/fuzzy engine, so
// behaviour is identical to the command palette, the "?" cheat sheet,
// and the Beacon cache inspector.

import {
  scorePaletteField,
  classifyPaletteNav,
  nextPaletteIndex,
  classifyPaletteGroupNav,
  groupStartIndices,
  nextGroupIndex,
  type PaletteNavIntent,
  type PaletteGroupNavIntent,
} from "./paletteSearch";

// Re-export the palette group-jump primitives so the panel can drive the
// Cmd/Ctrl+Up/Down chord through one import surface (librarySearchView)
// without reaching into paletteSearch directly — keeping the view-core
// the single dependency the component talks to.
export { classifyPaletteGroupNav, nextGroupIndex };
export type { PaletteGroupNavIntent };

/**
 * The fields the view-core reads off a search hit. Mirrors `SearchHit`
 * (library.ts) but kept structural so the pure helpers stay decoupled
 * from the wire type and trivially testable.
 */
export interface SearchHitLike {
  docId: number;
  path: string;
  title: string | null;
  pageIndex: number;
  /** Snippet pre-wrapped with `<mark>…</mark>` around matches. */
  snippet: string;
  /** bm25 rank — lower is better in FTS5. */
  rank: number;
}

/** A document group: its hits plus identity, as the panel renders them. */
export interface SearchGroupLike<T extends SearchHitLike = SearchHitLike> {
  docId: number;
  path: string;
  title: string;
  hits: T[];
}

/**
 * Extract the file basename from a path, tolerating both POSIX `/` and
 * Windows `\` separators. Shared by refine (match on filename) and the
 * component (display) so the two can never disagree on what "the name"
 * is. A trailing separator or empty path degrades gracefully.
 */
export function searchBasename(path: string): string {
  if (!path) return "";
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(i + 1) : path;
}

// --- Slice 1: keyboard navigation through results --------------------
//
// The results list was mouse-only: every hit was a <button> you had to
// click. This adds Raycast-grade keyboard control — arrow to move a flat
// cursor across the grouped hits (wrapping, Home/End, PageUp/Down),
// Enter to open the focused hit, Escape to clear — so the whole flow
// (type -> arrow -> Enter) is keyboard-driven.
//
// The arrow/Home/End/paging math REUSES the tested palette nav core
// (classifyPaletteNav + nextPaletteIndex) rather than rolling a second
// implementation, exactly as the "?" cheat-sheet and beacon inspector
// do. Only the action keys (Enter/Escape) are classified here.

/** What a keypress over the results list should do. */
export type SearchResultAction =
  | { kind: "move"; intent: PaletteNavIntent }
  | { kind: "open" }
  | { kind: "clear" }
  | null;

/** Minimal keyboard-event shape the results classifier reads. */
export interface SearchKeyEvent {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

/**
 * Classify a keypress over the results list into an action, or null if
 * it isn't a results key (so it falls through to the input / browser).
 * Any modifier (Cmd/Ctrl/Alt) disqualifies the key so app/OS chords keep
 * priority — the list owns only bare presses. Navigation keys defer to
 * the tested palette classifier so wrap/paging behaviour is identical.
 * Enter opens the focused hit; Escape clears. A bare Shift is allowed to
 * pass through to the classifier (it never changes a nav/open intent).
 */
export function classifySearchResultKey(ev: SearchKeyEvent): SearchResultAction {
  if (!ev) return null;
  if (ev.ctrlKey || ev.metaKey || ev.altKey) return null;

  const nav = classifyPaletteNav({ key: ev.key });
  if (nav) return { kind: "move", intent: nav };

  switch (ev.key) {
    case "Enter":
      return { kind: "open" };
    case "Escape":
      return { kind: "clear" };
    default:
      return null;
  }
}

/** One flattened hit, tagged with its position in the grouped render. */
export interface FlatSearchHit<T extends SearchHitLike = SearchHitLike> {
  hit: T;
  /** Index of the group this hit belongs to. */
  groupIndex: number;
  /** Index of the hit within its group. */
  hitIndex: number;
  /** Flat cursor index across every group, in render order. */
  flatIndex: number;
}

/**
 * Flatten grouped search results into one cursor index space, in render
 * order (group 0's hits, then group 1's, …). The flat list is what the
 * arrow cursor walks; each entry carries its (group, hit) coordinates so
 * the component can paint the cursor ring on the right row. A null /
 * garbage groups list -> [].
 */
export function flattenSearchHits<T extends SearchHitLike>(
  groups: readonly SearchGroupLike<T>[],
): FlatSearchHit<T>[] {
  if (!Array.isArray(groups)) return [];
  const out: FlatSearchHit<T>[] = [];
  let flat = 0;
  for (let gi = 0; gi < groups.length; gi++) {
    const g = groups[gi];
    if (!g || !Array.isArray(g.hits)) continue;
    for (let hi = 0; hi < g.hits.length; hi++) {
      out.push({ hit: g.hits[hi], groupIndex: gi, hitIndex: hi, flatIndex: flat });
      flat++;
    }
  }
  return out;
}

/** Total flattened hit count across every group. Tolerant of garbage. */
export function flatSearchHitCount(groups: readonly SearchGroupLike[]): number {
  if (!Array.isArray(groups)) return 0;
  let n = 0;
  for (const g of groups) {
    if (g && Array.isArray(g.hits)) n += g.hits.length;
  }
  return n;
}

// --- Slice 1b: group-jump chord (Cmd/Ctrl + Up/Down) -----------------
//
// In a broad search the grouped result list runs to dozens of document
// sections. Walking it one hit at a time with the arrows is slow; this
// reuses the command palette's group-jump core (classifyPaletteGroupNav
// + groupStartIndices + nextGroupIndex) so Cmd/Ctrl+Down leaps the cursor
// to the next document header and Cmd/Ctrl+Up to the previous one, with
// the same "jump to section top, then previous section" semantics. No
// second implementation — the panel imports the classifier/mover from
// here so the chord behaves identically to ⌘K.

/**
 * Flat start index (into the cursor space) of each document group's first
 * hit, in render order. `[{hits:3},{hits:2},{hits:4}]` -> `[0, 3, 5]`.
 * Thin adapter over the tested `groupStartIndices` so the panel doesn't
 * re-derive the section heads. A null/garbage list -> [].
 */
export function searchGroupStarts(groups: readonly SearchGroupLike[]): number[] {
  if (!Array.isArray(groups)) return [];
  const sizes = groups.map((g) => (g && Array.isArray(g.hits) ? g.hits.length : 0));
  return groupStartIndices(sizes);
}

/**
 * Resolve the next cursor index for a move over `count` hits. Thin
 * adapter over the tested `nextPaletteIndex` so the results list and the
 * palette share one wrap/clamp/paging contract. Empty list -> 0.
 */
export function nextSearchCursor(
  intent: PaletteNavIntent,
  current: number,
  count: number,
): number {
  return nextPaletteIndex(intent, current, count);
}

/**
 * Clamp a stored cursor index into a freshly-(re)built list. After a new
 * search, a refine, or a sort the hit count shrinks or the order moves,
 * so a cursor parked at index 40 must snap back into range. Returns 0
 * for an empty list, never a negative or out-of-bounds index.
 */
export function clampSearchCursor(current: number, count: number): number {
  if (!Number.isFinite(count) || count <= 0) return 0;
  if (!Number.isFinite(current) || current < 0) return 0;
  const last = count - 1;
  return Math.min(last, Math.floor(current));
}

// --- Slice 2: live query-interpretation preview ----------------------
//
// FTS5 has its own query grammar (phrases, exclusions, prefix-globs) and
// the backend lexes the raw input into well-formed tokens before MATCH.
// Most of that is invisible: the user can't tell that the last word is
// prefix-matched, that "quotes" mean an adjacent-phrase, that a leading
// `-word` excludes, or — critically — that a query of ONLY exclusions
// returns nothing. This renders a live, truthful preview of how the
// query WILL be interpreted, by mirroring the exact lexer in
// `src-tauri/src/pdf/library/search.rs` (tokenize + build_match_expr).
//
// Keeping this a faithful port (not an approximation) is the whole
// point: the chips must match what the engine actually does, or they'd
// mislead. The Rust tests in search.rs are the ground truth this mirrors.

/** How one parsed query token will be matched. */
export type SearchTokenKind = "term" | "prefix" | "phrase" | "exclude";

/** One interpreted query token, ready to render as a chip. */
export interface SearchToken {
  kind: SearchTokenKind;
  /** Scrubbed display text (no quotes / operators). */
  text: string;
}

/** The full interpretation of a raw query string. */
export interface QueryInterpretation {
  tokens: SearchToken[];
  /**
   * True when the query has positive intent but NO positive anchor —
   * i.e. it parsed to only exclusions (`-draft`), which FTS5 can't run,
   * so the backend deliberately returns an empty result. Worth warning
   * the user about so an empty list doesn't look like a bug.
   */
  noAnchor: boolean;
  /** True when the input is blank / whitespace (nothing to interpret). */
  empty: boolean;
}

// Internal lexer token, mirroring the Rust `Tok` enum.
type RawTok =
  | { t: "bare"; w: string }
  | { t: "phrase"; w: string }
  | { t: "exclude"; w: string };

const FTS_WORD_STRIP = new Set(['"', "^", "*", "-", ":", "(", ")"]);
// Phrases keep `-` (a phrase can contain a hyphen) but drop the rest.
const FTS_PHRASE_STRIP = new Set(['"', "^", "*", ":", "(", ")"]);

/** Mirror of Rust `scrub_word`: strip every FTS5 metacharacter. */
function scrubWord(w: string): string {
  let out = "";
  for (const c of w) if (!FTS_WORD_STRIP.has(c)) out += c;
  return out;
}

/** Mirror of Rust `scrub_phrase`: strip operators, collapse whitespace. */
function scrubPhrase(p: string): string {
  let kept = "";
  for (const c of p) if (!FTS_PHRASE_STRIP.has(c)) kept += c;
  return kept.split(/\s+/).filter(Boolean).join(" ");
}

const QUOTE_CHARS = new Set(['"', "\u201C", "\u201D"]);

/**
 * Lex raw user input into bare / phrase / exclude tokens. Faithful port
 * of `tokenize` in search.rs: curly + straight quotes open phrases, a
 * leading `-` (only at a token start) flips the next bare/phrase into an
 * exclusion, a lone `-` is dropped, an empty `""` cancels a pending
 * exclusion. Internal hyphens (`co-op`) survive into a bare word, where
 * scrubWord strips them to `coop` — matching the backend exactly.
 */
function tokenizeQuery(query: string): RawTok[] {
  const out: RawTok[] = [];
  const chars = Array.from(query ?? "");
  let buf = "";
  let pendingNeg = false;
  let i = 0;

  while (i < chars.length) {
    const c = chars[i];
    if (QUOTE_CHARS.has(c)) {
      // Flush any bare-word accumulator as its own token first.
      if (buf.length > 0) {
        const w = scrubWord(buf);
        if (w.length > 0) {
          if (pendingNeg) {
            out.push({ t: "exclude", w });
            pendingNeg = false;
          } else {
            out.push({ t: "bare", w });
          }
        }
        buf = "";
      }
      // Read until the matching close-quote OR end of input.
      let phrase = "";
      i++;
      while (i < chars.length && !QUOTE_CHARS.has(chars[i])) {
        phrase += chars[i];
        i++;
      }
      if (i < chars.length) i++; // consume the closing quote
      const cleaned = scrubPhrase(phrase);
      if (cleaned.length > 0) {
        if (pendingNeg) {
          out.push({ t: "exclude", w: cleaned });
          pendingNeg = false;
        } else {
          out.push({ t: "phrase", w: cleaned });
        }
      } else {
        // Empty `""` cancels a pending exclusion — the user clearly
        // didn't mean to exclude nothing.
        pendingNeg = false;
      }
      continue;
    }
    if (/\s/.test(c)) {
      if (buf.length > 0) {
        const w = scrubWord(buf);
        if (w.length > 0) {
          out.push(pendingNeg ? { t: "exclude", w } : { t: "bare", w });
        }
        buf = "";
      }
      pendingNeg = false;
      i++;
      continue;
    }
    if (c === "-" && buf.length === 0 && !pendingNeg) {
      pendingNeg = true;
      i++;
      continue;
    }
    buf += c;
    i++;
  }
  if (buf.length > 0) {
    const w = scrubWord(buf);
    if (w.length > 0) {
      out.push(pendingNeg ? { t: "exclude", w } : { t: "bare", w });
    }
  }
  return out;
}

/**
 * Interpret a raw query the way the FTS backend will. Mirrors
 * `build_match_expr`: the LAST bare token becomes a prefix match
 * (`term*`), every other bare token is an exact word, phrases match
 * adjacent words, and `-term` excludes. A query that parses to only
 * exclusions has no positive anchor (`noAnchor`) and returns nothing.
 * A blank query is `empty` with no tokens. Pure + DOM-free.
 */
export function interpretSearchQuery(query: string): QueryInterpretation {
  const raw = tokenizeQuery(query ?? "");
  if (raw.length === 0) {
    return { tokens: [], noAnchor: false, empty: true };
  }
  const hasPositive = raw.some((t) => t.t === "bare" || t.t === "phrase");
  // Index of the LAST bare token — it alone gets the prefix glob.
  let lastBare = -1;
  for (let k = 0; k < raw.length; k++) if (raw[k].t === "bare") lastBare = k;

  const tokens: SearchToken[] = raw.map((t, k) => {
    if (t.t === "phrase") return { kind: "phrase", text: t.w };
    if (t.t === "exclude") return { kind: "exclude", text: t.w };
    // bare
    return { kind: k === lastBare ? "prefix" : "term", text: t.w };
  });

  return { tokens, noAnchor: !hasPositive, empty: false };
}

/**
 * One-line, screen-reader-friendly narration of an interpretation, e.g.
 * `Matching "contract" as a prefix, the phrase "force majeure", excluding
 * "draft"`. Empty input -> "". A no-anchor query explains why it returns
 * nothing. Pure.
 */
export function describeQueryInterpretation(interp: QueryInterpretation): string {
  if (!interp || interp.empty || interp.tokens.length === 0) return "";
  if (interp.noAnchor) {
    return "Only exclusions — add a word to search for, or this returns nothing.";
  }
  const positives: string[] = [];
  const negatives: string[] = [];
  for (const t of interp.tokens) {
    switch (t.kind) {
      case "prefix":
        positives.push(`\u201C${t.text}\u201D as a prefix`);
        break;
      case "term":
        positives.push(`\u201C${t.text}\u201D`);
        break;
      case "phrase":
        positives.push(`the phrase \u201C${t.text}\u201D`);
        break;
      case "exclude":
        negatives.push(`\u201C${t.text}\u201D`);
        break;
    }
  }
  let s = "";
  if (positives.length > 0) s += `Matching ${joinList(positives)}`;
  if (negatives.length > 0) {
    s += `${s ? ", excluding" : "Excluding"} ${joinList(negatives)}`;
  }
  return s;
}

/** Join a list with commas + a trailing "and" (Oxford-free, UI copy). */
function joinList(parts: string[]): string {
  if (parts.length <= 1) return parts.join("");
  if (parts.length === 2) return `${parts[0]} and ${parts[1]}`;
  return `${parts.slice(0, -1).join(", ")}, and ${parts[parts.length - 1]}`;
}

// --- Slice 3: in-results refine filter -------------------------------
//
// A broad query ("contract") can return hundreds of hits across dozens
// of docs. Re-running the FTS search with more words changes the ranking
// and loses your place. Instead, this narrows the ALREADY-RETURNED hits
// client-side, instantly, with no round-trip — type "termination" in the
// refine box and only hits whose snippet, document title, or filename
// contain that survive. Reuses the tested palette scorer so matching +
// ranking feel identical to ⌘K and the other surfaces.

/**
 * Strip `<mark>`/`</mark>` (and any stray tags) out of a server snippet
 * so the refine match runs against the human-readable text, not markup.
 * Also decodes the handful of entities the snippet pipeline emits.
 */
export function stripSnippetMarks(snippet: string): string {
  if (!snippet) return "";
  return snippet
    .replace(/<\/?mark>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

/**
 * Narrow `hits` to those matching `refine`, preserving the input order
 * (relevance is already baked into arrival order; refine only decides
 * membership). A blank refine passes every hit through unchanged. A hit
 * matches if `refine` scores against its snippet text, document title,
 * OR basename — the same "any visible field" contract the palette uses.
 * A null/garbage list -> [].
 */
export function refineSearchHits<T extends SearchHitLike>(
  hits: readonly T[],
  refine: string,
): T[] {
  if (!Array.isArray(hits)) return [];
  const q = (refine ?? "").trim();
  if (!q) return hits.slice();
  return hits.filter((h) => {
    if (!h) return false;
    const snippet = stripSnippetMarks(h.snippet);
    const title = h.title ?? "";
    const name = searchBasename(h.path);
    const best = Math.max(
      scorePaletteField(q, snippet).score,
      scorePaletteField(q, title).score,
      scorePaletteField(q, name).score,
    );
    return best > 0;
  });
}

// --- Slice 3b: snippet refine-highlight -------------------------------
//
// The server snippet already wraps each FTS match in `<mark>…</mark>`
// (the bm25 match), but when an in-results refine is ALSO active the row
// survived because its text contains the refine term too — and nothing
// shows WHERE. This parses a snippet into typed spans: the server-mark
// runs, plus a SECOND highlight painted over every case-insensitive
// occurrence of the refine term in the plain text BETWEEN marks. The
// component renders the two tints distinctly (FTS match vs why-it-
// survived) without using `{@html}`, so there's no XSS surface at all.
//
// Matching the refine as a literal substring (case-insensitive) — not
// the fuzzy palette scorer — is deliberate: a highlight must mark a
// contiguous, visible run, and `refineSearchHits` already decided
// membership; this only paints the obvious literal hits inside the kept
// snippet so the eye lands on them. A refine that only matched the title
// or filename (not the snippet text) simply paints nothing here, which
// is correct.

/** One rendered span of a parsed snippet. */
export interface SnippetSpan {
  text: string;
  /** True for text inside the server `<mark>` (the FTS match). */
  match: boolean;
  /** True for a literal occurrence of the active refine term. */
  refine: boolean;
}

const SNIPPET_ENTITIES: ReadonlyArray<readonly [RegExp, string]> = [
  [/&amp;/g, "&"],
  [/&lt;/g, "<"],
  [/&gt;/g, ">"],
];

/** Decode the handful of entities the snippet pipeline emits. */
function decodeSnippetEntities(s: string): string {
  let out = s;
  for (const [re, ch] of SNIPPET_ENTITIES) out = out.replace(re, ch);
  return out;
}

/**
 * Paint every case-insensitive literal occurrence of `refine` inside a
 * plain (mark-free) text run, returning the run split into refine / non-
 * refine spans that all carry the given `match` flag. A blank refine (or
 * a run with no occurrence) yields a single span. Pure; the empty-string
 * refine is treated as "no refine" so it never matches between every
 * character.
 */
function paintRefine(text: string, refine: string, match: boolean): SnippetSpan[] {
  if (!text) return [];
  const needle = (refine ?? "").trim();
  if (!needle) return [{ text, match, refine: false }];
  const hayLc = text.toLowerCase();
  const needleLc = needle.toLowerCase();
  const out: SnippetSpan[] = [];
  let from = 0;
  let at = hayLc.indexOf(needleLc, from);
  while (at >= 0) {
    if (at > from) out.push({ text: text.slice(from, at), match, refine: false });
    out.push({ text: text.slice(at, at + needle.length), match, refine: true });
    from = at + needle.length;
    at = hayLc.indexOf(needleLc, from);
  }
  if (from < text.length) out.push({ text: text.slice(from), match, refine: false });
  return out;
}

/**
 * Parse a server snippet (the raw `<mark>`-wrapped, entity-encoded string
 * the FTS backend returns) into a flat list of render spans. Text inside
 * `<mark>…</mark>` is flagged `match`; every literal case-insensitive
 * occurrence of `refine` — inside OR outside a mark — is additionally
 * flagged `refine`. Entities (`&amp;`/`&lt;`/`&gt;`) are decoded so the
 * component can render `span.text` as plain text content (no `{@html}`,
 * no XSS surface). Unterminated / stray marks degrade to plain text. A
 * null/empty snippet -> []. Pure + DOM-free.
 */
export function buildSnippetSpans(snippet: string, refine: string): SnippetSpan[] {
  if (!snippet) return [];
  const spans: SnippetSpan[] = [];
  const re = /<mark>([\s\S]*?)<\/mark>/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(snippet)) !== null) {
    if (m.index > last) {
      const plain = decodeSnippetEntities(snippet.slice(last, m.index));
      spans.push(...paintRefine(plain, refine, false));
    }
    const inner = decodeSnippetEntities(m[1]);
    spans.push(...paintRefine(inner, refine, true));
    last = re.lastIndex;
  }
  if (last < snippet.length) {
    const tail = decodeSnippetEntities(snippet.slice(last));
    spans.push(...paintRefine(tail, refine, false));
  }
  return spans;
}

// --- Slice 4: result sort modes --------------------------------------
//
// Results come back bm25-ranked and grouped by document in arrival
// order, which is great for "best match first" but useless when you want
// to scan alphabetically or find the document with the MOST hits. This
// adds a three-mode sort over the document groups — Relevance (the FTS
// order, untouched), Document (title A->Z), Matches (hit count, biggest
// first) — each stable so equal groups never jitter between renders.

/** A sort mode for the grouped results. */
export type SearchSortMode = "relevance" | "document" | "matches";

/** Every sort mode, in display order (drives the segmented control). */
export const SEARCH_SORT_MODES: readonly SearchSortMode[] = [
  "relevance",
  "document",
  "matches",
];

/** Human label for a sort mode (segmented-control button text). */
export function searchSortLabel(mode: SearchSortMode): string {
  switch (mode) {
    case "relevance":
      return "Relevance";
    case "document":
      return "Document";
    case "matches":
      return "Matches";
    default:
      return mode;
  }
}

/** Longer label for the footer / aria ("sorted by …"). */
export function describeSortMode(mode: SearchSortMode): string {
  switch (mode) {
    case "relevance":
      return "best match";
    case "document":
      return "document name";
    case "matches":
      return "match count";
    default:
      return String(mode);
  }
}

/**
 * Advance to the next sort mode, wrapping. Lets a keyboard shortcut cycle
 * the sort without reaching for the segmented control. Pure.
 */
export function cycleSearchSort(current: SearchSortMode): SearchSortMode {
  const i = SEARCH_SORT_MODES.indexOf(current);
  if (i < 0) return SEARCH_SORT_MODES[0];
  return SEARCH_SORT_MODES[(i + 1) % SEARCH_SORT_MODES.length];
}

/**
 * Whether `x` is a valid SearchSortMode. The type guard shared by the panel
 * and the localStorage shell (librarySortStore.ts) so a corrupt or schema-
 * drifted persisted value can never seat itself into the panel's sort state.
 * Pure, garbage-safe.
 */
export function isSearchSortMode(x: unknown): x is SearchSortMode {
  return typeof x === "string" && SEARCH_SORT_MODES.includes(x as SearchSortMode);
}

/**
 * Sort document groups by the given mode, returning a NEW array (input is
 * never mutated). "relevance" preserves arrival order (already bm25); the
 * other modes fall back to arrival index as a stable tie-break so equal
 * groups keep a deterministic order. "document" compares titles
 * case-insensitively + numeric-aware (Doc2 < Doc10). A null list -> [].
 */
export function sortSearchGroups<T extends SearchHitLike>(
  groups: readonly SearchGroupLike<T>[],
  mode: SearchSortMode,
): SearchGroupLike<T>[] {
  if (!Array.isArray(groups)) return [];
  const indexed = groups.map((g, i) => ({ g, i }));
  if (mode === "relevance") {
    // Already in arrival (rank) order; copy without reordering.
    return indexed.map((x) => x.g);
  }
  indexed.sort((a, b) => {
    let primary = 0;
    if (mode === "document") {
      primary = (a.g.title ?? "").localeCompare(b.g.title ?? "", undefined, {
        sensitivity: "base",
        numeric: true,
      });
    } else if (mode === "matches") {
      const an = Array.isArray(a.g.hits) ? a.g.hits.length : 0;
      const bn = Array.isArray(b.g.hits) ? b.g.hits.length : 0;
      primary = bn - an; // biggest first
    }
    if (primary !== 0) return primary;
    return a.i - b.i; // stable arrival-order tie-break
  });
  return indexed.map((x) => x.g);
}

// --- Slice 5: result summary footer + page-spread badges -------------
//
// The panel told you "N matches across M documents" only in a header
// line that vanished under a refine, and a group's hits gave no sense of
// WHERE in the document they landed. This adds a context-aware footer
// that narrates the live view (shown-vs-total, refine, sort) the way the
// command palette and beacon inspector footers do, plus a per-group
// page-spread badge ("pp. 3-47") so you can see a document's match range
// at a glance.

/** The live state the results footer narrates. */
export interface SearchSummaryState {
  /** Hits shown after refine. */
  shown: number;
  /** Distinct documents shown after refine. */
  docs: number;
  /** Total hits before refine (so refine can read "4 of 12"). */
  total: number;
  /** Trimmed refine string ("" = none). */
  refine: string;
  /** Active sort mode. */
  sortMode: SearchSortMode;
}

/**
 * Narrate the current result view for the footer: how many matches
 * across how many documents, whether a refine is narrowing them, and
 * (when not the default) which sort is applied. Mirrors the palette's
 * `describePaletteCount` / beacon's `describeBeaconView` style. Returns
 * "" when there is nothing to summarize (no results). Pure.
 */
export function summarizeSearchResults(state: SearchSummaryState): string {
  const total = Math.max(0, state?.total ?? 0);
  const shown = Math.max(0, Math.min(total, state?.shown ?? 0));
  const docs = Math.max(0, state?.docs ?? 0);
  const refine = (state?.refine ?? "").trim();
  const mode = state?.sortMode ?? "relevance";

  if (total === 0) return "";

  let base: string;
  if (refine && shown !== total) {
    base = `${shown.toLocaleString()} of ${total.toLocaleString()} match${total === 1 ? "" : "es"}`;
  } else {
    base = `${shown.toLocaleString()} match${shown === 1 ? "" : "es"}`;
  }
  base += ` across ${docs.toLocaleString()} document${docs === 1 ? "" : "s"}`;
  if (refine) base += ` \u00b7 refined \u201C${refine}\u201D`;
  if (mode !== "relevance") base += ` \u00b7 by ${describeSortMode(mode)}`;
  return base;
}

/**
 * The page range a document's hits span, 1-based, for a compact badge.
 * One distinct page -> "p. 4"; a spread -> "pp. 3\u201347" (min\u2013max).
 * Tolerates 0-based `pageIndex` (the wire shape), unsorted hits, and a
 * null/empty list (-> ""). Pure.
 */
export function pageSpread(hits: readonly SearchHitLike[]): string {
  if (!Array.isArray(hits) || hits.length === 0) return "";
  let min = Infinity;
  let max = -Infinity;
  for (const h of hits) {
    if (!h || !Number.isFinite(h.pageIndex)) continue;
    const p = h.pageIndex + 1; // 0-based -> 1-based
    if (p < min) min = p;
    if (p > max) max = p;
  }
  if (!Number.isFinite(min)) return "";
  if (min === max) return `p. ${min}`;
  return `pp. ${min}\u2013${max}`;
}

// --- Slice 6: recent-search chips keyboard navigation ----------------
//
// The empty-query state shows a strip of "recent searches" chips, but it
// was mouse-only — a keyboard user landing on the panel with no query
// had to reach for the pointer to re-run a prior search. This adds a flat
// HORIZONTAL cursor over the chip row: Left/Right walk it (wrapping),
// Home/End leap to either end, Enter runs the focused chip, Escape parks
// the cursor. The chips are a single row so the natural axis is
// left/right (unlike the vertical results list, which owns up/down) —
// keeping the two cursors on different axes means neither steals the
// other's keys. The index math REUSES the tested palette nav core
// (nextPaletteIndex) so wrap/clamp behaviour matches every other surface.

/** What a keypress over the recent-search chip strip should do. */
export type RecentChipAction =
  | { kind: "move"; intent: PaletteNavIntent }
  | { kind: "run" }
  | { kind: "delete" }
  | { kind: "clear" }
  | null;

/**
 * Classify a keypress over the recent-search chip strip into an action,
 * or null if it isn't a chip key (so it falls through to the search
 * input / browser). Any modifier (Cmd/Ctrl/Alt) disqualifies so app
 * chords keep priority. ArrowLeft/Right map to prev/next (the strip is a
 * horizontal row); Home/End leap; Enter runs the focused chip; Backspace
 * or Delete drops the focused chip (one stray query, complementing the
 * all-or-nothing Clear history); Escape parks the cursor. ArrowUp/Down
 * are deliberately NOT claimed — they belong to the results cursor — so
 * the two never collide.
 */
export function classifyRecentChipKey(ev: SearchKeyEvent): RecentChipAction {
  if (!ev) return null;
  if (ev.ctrlKey || ev.metaKey || ev.altKey) return null;
  switch (ev.key) {
    case "ArrowLeft":
      return { kind: "move", intent: "prev" };
    case "ArrowRight":
      return { kind: "move", intent: "next" };
    case "Home":
      return { kind: "move", intent: "first" };
    case "End":
      return { kind: "move", intent: "last" };
    case "Enter":
      return { kind: "run" };
    case "Backspace":
    case "Delete":
      return { kind: "delete" };
    case "Escape":
      return { kind: "clear" };
    default:
      return null;
  }
}

/**
 * Resolve the next chip-cursor index for a move over `count` chips. Thin
 * adapter over the tested `nextPaletteIndex` so the chip strip and the
 * palette share one wrap/clamp contract (Left/Right wrap; Home/End jump).
 * Empty strip -> 0.
 */
export function nextChipCursor(
  intent: PaletteNavIntent,
  current: number,
  count: number,
): number {
  return nextPaletteIndex(intent, current, count);
}

/**
 * Clamp a stored chip cursor into a freshly-(re)built strip. The recent
 * list refreshes after every search (a run bubbles a query to the head,
 * or clearing empties it), so a cursor parked at index 7 must snap back.
 * Returns -1 (no chip focused) for an empty strip or a negative cursor,
 * else clamps into [0, count-1].
 */
export function clampChipCursor(current: number, count: number): number {
  if (!Number.isFinite(count) || count <= 0) return -1;
  if (!Number.isFinite(current) || current < 0) return -1;
  return Math.min(count - 1, Math.floor(current));
}

// --- Recent-chip relative age -----------------------------------------
//
// Each recent-search chip showed only its last-run match count — but not
// WHEN it was last run, so a query from five minutes ago and one from
// last month looked identical. This pure helper turns a chip's unix
// timestamp into a compact, human relative age ("just now", "2h",
// "3d", "5w") for a muted suffix on the chip, mirroring the coarse
// granularity Linear/GitHub use on activity rows.

/**
 * Format a unix-seconds timestamp as a compact relative age against `now`
 * (also unix seconds). Buckets: <45s "just now"; minutes "Nm"; hours
 * "Nh"; days "Nd"; weeks "Nw"; beyond ~1y "1y+". A future or garbage
 * timestamp degrades to "just now" rather than a negative age. Pure; the
 * units are deliberately coarse (one significant figure) so the chip stays
 * a glanceable suffix, not a precise clock.
 */
export function formatRelativeAge(ts: number, now: number): string {
  if (!Number.isFinite(ts) || !Number.isFinite(now)) return "just now";
  const secs = Math.floor(now - ts);
  if (secs < 45) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${Math.max(1, mins)}m`;
  const hours = Math.floor(secs / 3600);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(secs / 86400);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  if (weeks < 52) return `${weeks}w`;
  return "1y+";
}

// --- Recent-chip sort toggle ------------------------------------------
//
// The recent-search strip carries two signals per chip — WHEN it was last
// run (ts, now shown as a relative age) and HOW MANY hits it produced
// (resultCount). The strip only ever ordered newest-first, so the query
// that historically found the most documents (your richest saved search)
// was buried wherever it happened to fall chronologically. This adds a
// two-mode sort toggle over the strip — Recent (newest ts first, the
// shipped default) vs Results (biggest resultCount first) — so a user can
// flip the strip to surface their highest-yield prior searches. Pure +
// non-mutating, with a stable arrival tie-break so equal chips never
// jitter between renders, mirroring the result-group sort discipline.

/** A sort mode for the recent-search chip strip. */
export type RecentChipSortMode = "recent" | "results";

/** Every chip sort mode, in display order (drives the segmented control). */
export const RECENT_CHIP_SORT_MODES: readonly RecentChipSortMode[] = [
  "recent",
  "results",
];

/** Human label for a chip sort mode (segmented-control button text). */
export function recentChipSortLabel(mode: RecentChipSortMode): string {
  switch (mode) {
    case "recent":
      return "Recent";
    case "results":
      return "Results";
    default:
      return mode;
  }
}

/** The minimum shape a recent-search chip needs to be sorted. */
export interface RecentChipLike {
  /** Unix-seconds timestamp of the last run. */
  ts: number;
  /** How many hits the query produced last time it ran. */
  resultCount: number;
}

/** Coerce a possibly-garbage numeric field to a finite number (0 fallback). */
function chipNum(n: unknown): number {
  return typeof n === "number" && Number.isFinite(n) ? n : 0;
}

/**
 * Sort the recent-search chips by the active mode, returning a NEW array
 * (never mutates the input). `recent` orders newest-`ts` first — the
 * shipped default, matching the backend's newest-first log order;
 * `results` orders biggest-`resultCount` first so the highest-yield prior
 * searches lead. Every comparison falls back to the original arrival
 * index as a stable tie-break, so two chips with the same age (or the
 * same hit count) keep their incoming order rather than jittering. A
 * null/garbage list -> []. Pure + DOM-free.
 */
export function sortRecentChips<T extends RecentChipLike>(
  chips: readonly T[],
  mode: RecentChipSortMode,
): T[] {
  if (!Array.isArray(chips)) return [];
  const indexed = chips.map((chip, index) => ({ chip, index }));
  indexed.sort((a, b) => {
    let cmp = 0;
    if (mode === "results") {
      cmp = chipNum(b.chip.resultCount) - chipNum(a.chip.resultCount);
    } else {
      cmp = chipNum(b.chip.ts) - chipNum(a.chip.ts);
    }
    if (cmp !== 0) return cmp;
    return a.index - b.index; // stable arrival tie-break
  });
  return indexed.map((x) => x.chip);
}

// --- Empty-state suggested queries ------------------------------------
//
// When a search returns zero hits the panel showed only a "try shorter
// words" paragraph — a dead end. But the user's own recent-search log is
// a rich source of queries that DID find something: surfacing the
// highest-yield prior searches (that aren't the one that just failed)
// turns the empty state into a one-click recovery, mirroring the
// palette's suggestPaletteFallback. This picks the best recent searches
// to offer, ranked by last-run hit count so the most productive queries
// lead.

/** A suggested recovery query for the no-matches empty state. */
export interface EmptyQuerySuggestion {
  id: number;
  query: string;
  resultCount: number;
}

/**
 * Choose recovery suggestions for a no-matches empty state from the
 * recent-search log. Excludes the query that just failed (case-insensitive,
 * trimmed) and any chip that itself last found nothing (resultCount <= 0 —
 * suggesting another dead end helps no one). Ranks by last-run hit count
 * (biggest first) with a stable arrival tie-break, capped at `limit`
 * (default 4). A null/garbage list -> []. Pure + DOM-free.
 */
export function suggestEmptyQueries<T extends RecentChipLike & { id: number; query: string }>(
  recents: readonly T[],
  failedQuery: string,
  limit: number = 4,
): EmptyQuerySuggestion[] {
  if (!Array.isArray(recents)) return [];
  const cap = Number.isFinite(limit) && limit > 0 ? Math.floor(limit) : 4;
  const failed = (failedQuery ?? "").trim().toLowerCase();
  const eligible = recents
    .map((chip, index) => ({ chip, index }))
    .filter(({ chip }) => {
      if (!chip || typeof chip.query !== "string") return false;
      if (chipNum(chip.resultCount) <= 0) return false; // never suggest a dead end
      return chip.query.trim().toLowerCase() !== failed; // not the one that just failed
    });
  eligible.sort((a, b) => {
    const cmp = chipNum(b.chip.resultCount) - chipNum(a.chip.resultCount);
    if (cmp !== 0) return cmp;
    return a.index - b.index; // stable arrival tie-break
  });
  return eligible.slice(0, cap).map(({ chip }) => ({
    id: chip.id,
    query: chip.query,
    resultCount: chip.resultCount,
  }));
}

// --- Pinned (saved) searches ------------------------------------------
//
// The recent-search strip is a rolling log — the backend evicts the oldest
// queries, so a search you run often but not recently disappears. Pinning
// promotes a query to a sticky saved-search chip persisted in localStorage
// (see savedSearches.ts), surviving that eviction. This pure core owns the
// normalize / membership / toggle / label math; the storage shell only
// reads + writes the resulting string list, and the panel renders it.

/**
 * Normalize a query for pinning: collapse internal runs of whitespace to a
 * single space and trim the ends, so "  tax   2024 " and "tax 2024" pin to
 * the same chip. A null/garbage/blank value -> "". Pure.
 */
export function normalizePinnedQuery(query: string | null | undefined): string {
  if (typeof query !== "string") return "";
  return query.replace(/\s+/g, " ").trim();
}

/**
 * Whether a query is already pinned, matched case-insensitively against the
 * normalized form (so casing/spacing variants don't double-pin). A
 * blank/garbage query or list -> false. Pure.
 */
export function isPinnedSearch(pinned: readonly string[], query: string): boolean {
  const q = normalizePinnedQuery(query).toLowerCase();
  if (!q || !Array.isArray(pinned)) return false;
  return pinned.some((p) => normalizePinnedQuery(p).toLowerCase() === q);
}

/**
 * Toggle a query's pinned state, returning a NEW newest-first list (never
 * mutates the input). Pinning prepends the normalized query (newest first)
 * and drops any prior case-insensitive duplicate; unpinning removes every
 * case-insensitive match. The result is capped at `limit` (default 32) so
 * the strip can't grow without bound — the oldest pins fall off the end. A
 * blank/garbage query returns the list unchanged (normalized). Pure.
 */
export function togglePinnedSearch(
  pinned: readonly string[],
  query: string,
  limit: number = 32,
): string[] {
  const cap = Number.isFinite(limit) && limit > 0 ? Math.floor(limit) : 32;
  const list = Array.isArray(pinned) ? pinned : [];
  const q = normalizePinnedQuery(query);
  // Rebuild the existing list normalized + de-duped (preserves order).
  const seen = new Set<string>();
  const base: string[] = [];
  for (const p of list) {
    const np = normalizePinnedQuery(p);
    if (!np) continue;
    const key = np.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    base.push(np);
  }
  if (!q) return base.slice(0, cap);
  const qKey = q.toLowerCase();
  if (seen.has(qKey)) {
    // Already pinned -> unpin (remove every match).
    return base.filter((p) => p.toLowerCase() !== qKey);
  }
  // Not pinned -> prepend (newest first), capped.
  return [q, ...base].slice(0, cap);
}

/**
 * Narrate the pinned strip for an aria-live label, e.g. "3 saved searches"
 * / "1 saved search". Returns "" when nothing is pinned (the strip hides).
 * Pure (locale grouping via toLocaleString).
 */
export function describePinnedSearches(pinned: readonly string[]): string {
  const n = Array.isArray(pinned) ? pinned.filter((p) => normalizePinnedQuery(p)).length : 0;
  if (n <= 0) return "";
  return `${n.toLocaleString()} saved ${n === 1 ? "search" : "searches"}`;
}

// --- Reorder the Saved-searches strip (round 51 slice 4) -------------
//
// The saved-search strip renders newest-pinned-first with no way to
// arrange it — a query you saved long ago but reach for daily sat at the
// far end. This adds a user-defined order: moveSavedSearch computes the
// new newest-first list after a drag or an Alt+Arrow keyboard move, the
// exact RecentsHome.movePinned pattern (splice from -> to) but on the
// flat string[] the saved strip persists, so there's no second reorder
// engine. The component persists the result via savePinnedSearches.

/**
 * Compute the new saved-search order after moving the chip at `from` to
 * index `to` within the current list. Returns a NEW normalized + de-duped
 * list (input never mutated), ready for savePinnedSearches. Indices are
 * clamped into range; an out-of-range or no-op move returns the list
 * unchanged (normalized). A null/garbage list -> []. Pure + DOM-free.
 */
export function moveSavedSearch(
  pinned: readonly string[],
  from: number,
  to: number,
): string[] {
  // Normalize + de-dupe first so the reorder operates on exactly what the
  // strip renders (and can never reintroduce a casing/spacing duplicate).
  const seen = new Set<string>();
  const list: string[] = [];
  for (const p of Array.isArray(pinned) ? pinned : []) {
    const np = normalizePinnedQuery(p);
    if (!np) continue;
    const key = np.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    list.push(np);
  }
  const n = list.length;
  if (n === 0) return [];
  const f = Math.floor(Number.isFinite(from) ? from : -1);
  const t = Math.max(0, Math.min(n - 1, Math.floor(Number.isFinite(to) ? to : 0)));
  if (f < 0 || f >= n || f === t) return list;
  const next = list.slice();
  const [moved] = next.splice(f, 1);
  next.splice(t, 0, moved);
  return next;
}

// --- Saved-searches strip keyboard cursor (round 51 slice 5) ---------
//
// The recent-search strip has a horizontal keyboard cursor
// (classifyRecentChipKey); the saved strip shipped click-only. This gives
// it the SAME Left/Right/Home/End/Enter/unpin cursor, PLUS an Alt+Arrow
// reorder path that drives the slice-4 moveSavedSearch from the keyboard
// (the saved strip is reorderable; the recent strip is not, so this is a
// distinct classifier rather than a shared one). The cursor index math
// reuses the tested nextChipCursor/clampChipCursor so wrap/clamp matches
// the recent strip and every other surface.

/** What a keypress over the SAVED-search chip strip should do. */
export type SavedSearchAction =
  | { kind: "move"; intent: PaletteNavIntent }
  | { kind: "reorder"; dir: -1 | 1 }
  | { kind: "run" }
  | { kind: "unpin" }
  | { kind: "clear" }
  | null;

/**
 * Classify a keypress over the saved-search chip strip into an action, or
 * null if it isn't a strip key (so it falls through to the search input /
 * browser). Alt+ArrowLeft/Right REORDER the focused chip one slot (the
 * keyboard twin of drag); plain ArrowLeft/Right MOVE the cursor; Home/End
 * leap; Enter runs the focused chip; Backspace/Delete unpins it; Escape
 * parks the cursor. Cmd/Ctrl disqualify everything so app chords keep
 * priority. ArrowUp/Down are deliberately NOT claimed (they belong to the
 * results cursor). The Alt branch is checked BEFORE the plain-arrow branch
 * so a held Alt always reorders rather than moving.
 */
export function classifySavedSearchKey(ev: SearchKeyEvent): SavedSearchAction {
  if (!ev) return null;
  if (ev.ctrlKey || ev.metaKey) return null;
  // Alt+Arrow reorders (the keyboard twin of drag); plain Arrow moves.
  if (ev.altKey) {
    if (ev.key === "ArrowLeft") return { kind: "reorder", dir: -1 };
    if (ev.key === "ArrowRight") return { kind: "reorder", dir: 1 };
    return null; // any other Alt-combo isn't ours
  }
  switch (ev.key) {
    case "ArrowLeft":
      return { kind: "move", intent: "prev" };
    case "ArrowRight":
      return { kind: "move", intent: "next" };
    case "Home":
      return { kind: "move", intent: "first" };
    case "End":
      return { kind: "move", intent: "last" };
    case "Enter":
      return { kind: "run" };
    case "Backspace":
    case "Delete":
      return { kind: "unpin" };
    case "Escape":
      return { kind: "clear" };
    default:
      return null;
  }
}

// --- Jump-to-next-saved chord ----------------------------------------
//
// The saved-search strip is reachable by Tab + arrows, but a power user
// living in two or three saved queries wants to CYCLE them without
// reaching for the strip: Cmd/Ctrl+] runs the next saved search, +[
// the previous. nextSavedIndex computes the landing index given how many
// are pinned and which (if any) just ran, wrapping at both ends so the
// cycle never dead-ends. The chord classifier keeps it distinct from the
// per-chip arrow strip so the two can't collide.

/** Direction a jump-saved chord steps through the pinned list. */
export type SavedJump = { dir: -1 | 1 } | null;

/**
 * Classify a global keypress into a saved-search jump, or null. Cmd/Ctrl+]
 * steps to the next saved search, Cmd/Ctrl+[ to the previous (Finder/IDE
 * tab-cycle convention). Requires exactly the platform meta/ctrl modifier;
 * Alt/Shift disqualify so it never collides with the per-chip Alt+Arrow
 * reorder or app chords. Pure + DOM-free.
 */
export function classifyJumpSavedKey(ev: SearchKeyEvent): SavedJump {
  if (!ev) return null;
  if (!(ev.metaKey || ev.ctrlKey)) return null;
  if (ev.altKey || ev.shiftKey) return null;
  if (ev.key === "]") return { dir: 1 };
  if (ev.key === "[") return { dir: -1 };
  return null;
}

/**
 * Next pinned index after a jump, wrapping at both ends. `current` is the
 * index of the saved search that ran last (-1 = none yet: a forward jump
 * starts at 0, a backward jump at the last). An empty list -> -1 (nothing
 * to run). Always returns an in-range index for a non-empty list. Pure.
 */
export function nextSavedIndex(current: number, count: number, dir: -1 | 1): number {
  if (!Number.isFinite(count) || count <= 0) return -1;
  const n = Math.floor(count);
  if (!Number.isFinite(current) || current < 0) return dir > 0 ? 0 : n - 1;
  const cur = Math.min(n - 1, Math.floor(current));
  return ((cur + dir) % n + n) % n;
}

// --- Saved-chip last-run hit count -----------------------------------
//
// The recent-search chips show a per-chip count (how many hits the query
// found last time it ran); the SAVED chips show nothing. But a pinned
// query is exactly the kind a user runs often, so the same yield badge
// matters MORE there — a saved chip stuck at "0" tells you that query
// stopped finding anything. The hit count isn't stored with the pin (the
// pinned list is just query strings); we recover it by matching the pin
// against the live recent log, which carries resultCount. A pin that's
// fallen out of the rolling log has no known count -> null (no badge),
// rather than a misleading "0".

/** Minimum shape needed to look up a query's last-run hit count. */
export interface RecentCountLike {
  query: string;
  resultCount: number;
}

/**
 * Last-known hit count for a saved query, recovered from the recent log
 * by case-insensitive normalized-query match. Returns null when the query
 * isn't in the log (count genuinely unknown — show no badge) so a pin that
 * predates the rolling window never displays a misleading 0. Garbage/empty
 * query or list -> null. Pure + DOM-free.
 */
export function savedSearchHitCount(
  query: string,
  recents: readonly RecentCountLike[],
): number | null {
  const q = normalizePinnedQuery(query).toLowerCase();
  if (!q || !Array.isArray(recents)) return null;
  for (const r of recents) {
    if (r && normalizePinnedQuery(r.query).toLowerCase() === q) {
      return chipNum(r.resultCount);
    }
  }
  return null;
}

// --- Run-all-saved sweep summary -------------------------------------
//
// A user with several saved searches wants a one-shot health check: run
// every pin, see which still find hits and which have gone dry, without
// clicking each chip. The panel does the actual sequential runs; this is
// the pure summary core that turns the per-pin yields into a sorted,
// labeled report (biggest yield first, dry pins last) plus a one-line
// digest, so a dead pin ("0 hits") jumps out.

/** One pin's sweep result: its query + how many hits the run returned. */
export interface SweepResult {
  query: string;
  count: number;
}

/**
 * Sort sweep results biggest-yield first, dry pins (0) last, stable
 * alphabetical tie-break — so the most productive saved searches lead and
 * any dead query sinks to the bottom where it's obvious. Returns a NEW
 * array; never mutates. Negative/garbage counts floor to 0; blank queries
 * dropped. Null/empty -> []. Pure + DOM-free.
 */
export function rankSweepResults(results: readonly SweepResult[]): SweepResult[] {
  if (!Array.isArray(results)) return [];
  const clean = results
    .filter((r) => r && typeof r.query === "string" && r.query.trim().length > 0)
    .map((r) => ({ query: r.query, count: Math.max(0, Math.trunc(Number(r.count) || 0)) }));
  return clean.sort((a, b) => b.count - a.count || a.query.localeCompare(b.query));
}

/**
 * One-line digest of a finished sweep: total hits across pins, count of
 * dry pins, e.g. "5 searches, 84 hits, 1 came up empty" / "all 3 came up
 * empty". Empty input -> "No saved searches to run". Pure.
 */
export function describeSweep(results: readonly SweepResult[]): string {
  const ranked = rankSweepResults(results);
  const n = ranked.length;
  if (n === 0) return "No saved searches to run";
  const hits = ranked.reduce((s, r) => s + r.count, 0);
  const dry = ranked.filter((r) => r.count === 0).length;
  const sLbl = n === 1 ? "1 search" : `${n} searches`;
  const hLbl = hits === 1 ? "1 hit" : `${hits} hits`;
  if (dry === 0) return `${sLbl}, ${hLbl}`;
  if (dry === n) return n === 1 ? "1 search came up empty" : `all ${n} came up empty`;
  return `${sLbl}, ${hLbl}, ${dry === 1 ? "1 came up empty" : `${dry} came up empty`}`;
}
