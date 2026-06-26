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
  type PaletteNavIntent,
} from "./paletteSearch";

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
