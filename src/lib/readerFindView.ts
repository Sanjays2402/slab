// Reader Find-in-Page view-core — v3.42.0 "Atlas IV".
//
// Pure, DOM-free model for the in-document Find bar (Cmd+F) inside
// `ReaderPanel.svelte`. Round 41 took the cross-document Library Search
// panel (Cmd+Shift+F) to Raycast/Linear grade on a tested pure core
// (`librarySearchView.ts`); this is the parallel treatment for the
// IN-document find, which shipped thin and untested:
//
//   - the match status was a single inline expression that could only
//     say "N / total" or a bare "no matches" — no sense of "searching…",
//     no "wrapped to top", no idle-vs-not-found distinction;
//   - the pdf.js find-event payload was assembled by hand in THREE
//     separate places (runFind / findNext / findPrev), so a flag added
//     to one was silently missing from the others;
//   - there was no match-diacritics option, no recent-search history,
//     no global find-again chord (F3 / Cmd+G), and nothing announced to
//     a screen reader.
//
// This module owns the find-state interpretation / param building /
// history / cycle-chord / announcement math as pure functions so every
// branch is unit-tested without a DOM — the same pure-core / thin-shell
// discipline as `librarySearchView.ts`, `paletteSearch.ts`,
// `beaconCacheView.ts`, and `toastStack.ts`. Recent-search filtering and
// nav REUSE the tested palette core (`scorePaletteField` /
// `classifyPaletteNav` / `nextPaletteIndex`) rather than rolling a
// second fuzzy/nav engine.

import {
  scorePaletteField,
  splitHighlight,
  type PaletteRange,
  type HighlightSegment,
} from "./paletteSearch";

// =====================================================================
// Slice 1: tested find-state model
// =====================================================================
//
// pdf.js drives the find UI through two events on its EventBus:
//   - `updatefindcontrolstate` { state, previous, matchesCount, rawQuery }
//     where `state` is one of FindState below;
//   - `updatefindmatchescount`  { matchesCount } — fired repeatedly as
//     the async page scan turns up more hits, with NO `state` field.
// The old component collapsed both into `{current, total}` and rendered
// a bare ternary. We instead interpret the raw payload into a clean
// discriminated status so the bar can show "Searching…", "No matches",
// "3 of 17", and a subtle "wrapped" hint distinctly.

/** Mirrors pdf.js `FindState` (web/pdf_viewer.mjs). */
export const FIND_STATE = {
  FOUND: 0,
  NOT_FOUND: 1,
  WRAPPED: 2,
  PENDING: 3,
} as const;

export type FindStateCode = (typeof FIND_STATE)[keyof typeof FIND_STATE];

/** The shape of either find event pdf.js dispatches (both optional-ish). */
export interface FindControlEvent {
  /** A `FIND_STATE` code, or absent on a count-only progress event. */
  state?: number | null;
  matchesCount?: { current?: number | null; total?: number | null } | null;
  rawQuery?: string | null;
  previous?: boolean | null;
}

/** Coarse phase the bar renders against. */
export type FindPhase = "idle" | "pending" | "found" | "not-found";

/** The interpreted, render-ready find status. */
export interface FindStatus {
  phase: FindPhase;
  /** 1-based index of the active match (0 when none/idle). */
  current: number;
  /** Total matches found so far (grows during an async scan). */
  total: number;
  /** True when the last step wrapped past an end of the document. */
  wrapped: boolean;
  /** The query the status reflects (the live input value). */
  query: string;
}

function toCount(n: unknown): number {
  if (typeof n !== "number" || !Number.isFinite(n) || n < 0) return 0;
  return Math.floor(n);
}

/**
 * Interpret a pdf.js find event into a clean status. `query` is the live
 * find-input value — count-only progress events don't carry it, and a
 * stale `rawQuery` could otherwise lag the box by a keystroke.
 *
 * Rules:
 *   - blank query        -> idle (nothing to show)
 *   - explicit PENDING   -> pending ("Searching…")
 *   - any matches (>0)   -> found; `wrapped` iff state === WRAPPED
 *   - explicit NOT_FOUND -> not-found ("No matches")
 *   - zero matches on a count-only event (state absent) -> pending,
 *     because the async scan simply hasn't reached a hit yet and we
 *     must not flash "No matches" mid-scan.
 */
export function interpretFindState(
  ev: FindControlEvent | null | undefined,
  query: string,
): FindStatus {
  const q = (query ?? "").trim();
  const current = toCount(ev?.matchesCount?.current);
  const total = toCount(ev?.matchesCount?.total);

  if (!q) {
    return { phase: "idle", current: 0, total: 0, wrapped: false, query: "" };
  }

  const state = typeof ev?.state === "number" ? ev.state : null;

  if (state === FIND_STATE.PENDING) {
    return { phase: "pending", current, total, wrapped: false, query: q };
  }

  if (total > 0) {
    return {
      phase: "found",
      current,
      total,
      wrapped: state === FIND_STATE.WRAPPED,
      query: q,
    };
  }

  if (state === FIND_STATE.NOT_FOUND) {
    return { phase: "not-found", current: 0, total: 0, wrapped: false, query: q };
  }

  // total === 0, no decisive state -> still scanning.
  return { phase: "pending", current, total: 0, wrapped: false, query: q };
}

/** A render-friendly idle status (no active query). */
export function idleFindStatus(): FindStatus {
  return { phase: "idle", current: 0, total: 0, wrapped: false, query: "" };
}

/**
 * The compact label shown in the find bar's count slot. Kept tight so it
 * fits the narrow column: "", "Searching…", "No matches", "3 of 17".
 * The wrapped hint is surfaced separately (a subtle pill) so the count
 * itself never jitters in width.
 */
export function describeFindStatus(status: FindStatus): string {
  switch (status.phase) {
    case "idle":
      return "";
    case "pending":
      return status.total > 0 ? `${status.current} of ${status.total}\u2026` : "Searching\u2026";
    case "not-found":
      return "No matches";
    case "found":
      return `${status.current} of ${status.total}`;
  }
}

/** Tone token for styling the count slot (muted / warn / normal). */
export function findStatusTone(status: FindStatus): "muted" | "warn" | "normal" {
  if (status.phase === "not-found") return "warn";
  if (status.phase === "idle" || status.phase === "pending") return "muted";
  return "normal";
}

// =====================================================================
// Slice 2: single source of truth for the pdf.js find payload
// =====================================================================
//
// The component dispatched a hand-written object literal to the find
// EventBus in THREE places (initial find, find-next, find-prev). They
// drifted: a flag added to one was missing from the others, and there
// was no match-diacritics control at all. This collapses every dispatch
// into one builder so the options (case / whole-word / diacritics /
// highlight-all) are guaranteed identical across find, again, and the
// option-toggle re-runs.

/** The user-toggleable find options. */
export interface FindOptions {
  caseSensitive: boolean;
  wholeWord: boolean;
  /** When true, accented chars must match exactly (pdf.js matchDiacritics). */
  matchDiacritics: boolean;
  /** Paint every match, not just the active one. */
  highlightAll: boolean;
}

/** The full payload pdf.js `find`/`again`/`highlightallchange` expects. */
export interface FindDispatch {
  source: null;
  type: string;
  query: string;
  caseSensitive: boolean;
  entireWord: boolean;
  matchDiacritics: boolean;
  highlightAll: boolean;
  findPrevious: boolean;
}

/** Default options for a freshly-opened find bar. */
export function defaultFindOptions(): FindOptions {
  return {
    caseSensitive: false,
    wholeWord: false,
    matchDiacritics: false,
    highlightAll: true,
  };
}

/**
 * The action the bar is performing, mapped to the pdf.js event `type`
 * and direction:
 *   - "find"      a fresh query (the user typed)        -> type "find"
 *   - "again-next"/"again-prev" step through matches    -> type "again"
 *   - "options"   re-run after toggling a checkbox      -> "highlightallchange"
 *   - "clear"     empty the query (bar closed/cleared)  -> type "find", ""
 */
export type FindAction = "find" | "again-next" | "again-prev" | "options" | "clear";

/** Build the exact pdf.js dispatch payload for an action + query + options. */
export function buildFindDispatch(
  action: FindAction,
  query: string,
  opts: FindOptions,
): FindDispatch {
  const base: Omit<FindDispatch, "type" | "findPrevious" | "query"> = {
    source: null,
    caseSensitive: !!opts.caseSensitive,
    entireWord: !!opts.wholeWord,
    matchDiacritics: !!opts.matchDiacritics,
    highlightAll: !!opts.highlightAll,
  };
  switch (action) {
    case "find":
      return { ...base, type: "find", query, findPrevious: false };
    case "again-next":
      return { ...base, type: "again", query, findPrevious: false };
    case "again-prev":
      return { ...base, type: "again", query, findPrevious: true };
    case "options":
      return { ...base, type: "highlightallchange", query, findPrevious: false };
    case "clear":
      return { ...base, type: "find", query: "", highlightAll: true, findPrevious: false };
  }
}

/** Order + display metadata for the option toggles, so the bar renders
 *  them from one tested list rather than three ad-hoc <label>s. */
export interface FindOptionToggle {
  key: keyof FindOptions;
  /** Compact glyph/label shown on the chip. */
  label: string;
  /** Full description for the tooltip + aria-label. */
  title: string;
}

export const FIND_OPTION_TOGGLES: readonly FindOptionToggle[] = [
  { key: "caseSensitive", label: "Aa", title: "Match case" },
  { key: "wholeWord", label: "Word", title: "Whole words only" },
  { key: "matchDiacritics", label: "Diac", title: "Match diacritics (accents)" },
] as const;

/** Toggle one option immutably (handy for the component's onchange). */
export function toggleFindOption(opts: FindOptions, key: keyof FindOptions): FindOptions {
  return { ...opts, [key]: !opts[key] };
}

/** A short summary of which non-default options are active, for the SR
 *  announcer and an optional tooltip. "" when all are default. */
export function describeFindOptions(opts: FindOptions): string {
  const on: string[] = [];
  if (opts.caseSensitive) on.push("match case");
  if (opts.wholeWord) on.push("whole words");
  if (opts.matchDiacritics) on.push("match diacritics");
  return on.join(", ");
}

// =====================================================================
// Slice 3: recent-search history (filter-as-you-type, palette-scored)
// =====================================================================
//
// A fresh find bar was a blank box every time — re-finding the same term
// across sessions meant retyping it. This adds a small MRU ring of recent
// queries, surfaced as a dropdown under the input. Typing filters the
// ring with the SAME scorer as the command palette / library search, so
// the highlight + ranking behave identically; an empty box shows the
// most-recent few.

export const FIND_HISTORY_LIMIT = 12;
/** How many suggestions to surface in the dropdown at once. */
export const FIND_HISTORY_VISIBLE = 6;

/**
 * Push a query to the front of the MRU ring (most-recent first),
 * de-duplicating case-insensitively (the existing casing is replaced by
 * the new one so the ring reflects what the user last typed), trimming
 * to the limit. Blank/whitespace queries are ignored. Never mutates the
 * input array.
 */
export function pushFindHistory(history: string[], query: string): string[] {
  const q = (query ?? "").trim();
  if (!q) return Array.isArray(history) ? history.slice(0, FIND_HISTORY_LIMIT) : [];
  const prior = Array.isArray(history) ? history : [];
  const lower = q.toLowerCase();
  const deduped = prior.filter(
    (h) => typeof h === "string" && h.trim() && h.trim().toLowerCase() !== lower,
  );
  return [q, ...deduped].slice(0, FIND_HISTORY_LIMIT);
}

/** One scored recent-search suggestion (with highlight ranges). */
export interface FindSuggestion {
  query: string;
  score: number;
  ranges: PaletteRange[];
}

/**
 * Rank the recent-search ring against the live query using the palette
 * scorer. An empty query returns the most-recent `FIND_HISTORY_VISIBLE`
 * in MRU order (no scoring, no highlight). A non-empty query keeps only
 * positive-scoring entries, sorts by score desc with the MRU index as a
 * stable tie-break, and never suggests the exact term already typed
 * (case-insensitive) — re-finding it is just Enter.
 */
export function suggestFindHistory(
  history: string[],
  query: string,
  visible: number = FIND_HISTORY_VISIBLE,
): FindSuggestion[] {
  const ring = (Array.isArray(history) ? history : []).filter(
    (h) => typeof h === "string" && h.trim().length > 0,
  );
  const cap = Number.isFinite(visible) && visible > 0 ? Math.floor(visible) : FIND_HISTORY_VISIBLE;
  const q = (query ?? "").trim();
  if (!q) {
    return ring.slice(0, cap).map((entry) => ({ query: entry, score: 0, ranges: [] }));
  }
  const lower = q.toLowerCase();
  const scored: Array<FindSuggestion & { mru: number }> = [];
  ring.forEach((entry, mru) => {
    if (entry.trim().toLowerCase() === lower) return; // already typed
    const { score, ranges } = scorePaletteField(q, entry);
    if (score > 0) scored.push({ query: entry, score, ranges, mru });
  });
  scored.sort((a, b) => b.score - a.score || a.mru - b.mru);
  return scored.slice(0, cap).map(({ query: queryText, score, ranges }) => ({
    query: queryText,
    score,
    ranges,
  }));
}

/** Highlight segments for a suggestion's matched chars (palette-style). */
export function suggestionSegments(suggestion: FindSuggestion): HighlightSegment[] {
  return splitHighlight(suggestion.query, suggestion.ranges);
}

// =====================================================================
// Slice 4: global find-again chord (F3 / Cmd+G) + dropdown nav
// =====================================================================
//
// Stepping through matches required the find bar to be focused and you
// to click the up/down arrows or Enter in the box. Power users expect
// the universal find-again chords to cycle matches from ANYWHERE in the
// reader: F3 / Cmd+G next, Shift+F3 / Cmd+Shift+G previous. This
// classifies a keydown into a find intent so the component's one window
// handler can route it without a tangle of inline conditions.

export interface FindKeyEvent {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

/**
 * Find-bar / global intents:
 *   - "open"       Cmd/Ctrl+F                -> open (or focus) the bar
 *   - "again-next" F3 / Cmd-or-Ctrl+G       -> next match
 *   - "again-prev" Shift+F3 / Cmd-or-Ctrl+Shift+G -> previous match
 * Returns null for anything else so the caller's other handlers run.
 * `mod` lets the platform decide Cmd (mac) vs Ctrl; both are accepted
 * here so a test or a cross-platform build doesn't have to special-case.
 */
export type FindGlobalIntent = "open" | "again-next" | "again-prev";

export function classifyFindGlobalKey(ev: FindKeyEvent): FindGlobalIntent | null {
  if (!ev || typeof ev.key !== "string") return null;
  const mod = !!(ev.metaKey || ev.ctrlKey);
  const lower = ev.key.toLowerCase();

  // F3 / Shift+F3 — no modifier required (Alt disqualifies).
  if (lower === "f3" && !ev.altKey && !ev.metaKey && !ev.ctrlKey) {
    return ev.shiftKey ? "again-prev" : "again-next";
  }
  // Cmd/Ctrl+G — find again; Shift reverses.
  if (mod && lower === "g" && !ev.altKey) {
    return ev.shiftKey ? "again-prev" : "again-next";
  }
  // Cmd/Ctrl+F — open the bar (Shift+Cmd+F is the LIBRARY search, leave it).
  if (mod && lower === "f" && !ev.altKey && !ev.shiftKey) {
    return "open";
  }
  return null;
}

/**
 * Classify a keypress while the suggestion dropdown is open. ArrowUp/Down
 * move the highlighted suggestion (handled by the palette nav core in the
 * component); Enter commits the highlighted suggestion; Escape closes the
 * dropdown without closing the bar. Returns null otherwise so typing and
 * the find-input's own Enter/Escape still work when no suggestion is
 * highlighted.
 */
export type FindDropdownIntent = "next" | "prev" | "commit" | "close";

export function classifyFindDropdownKey(
  ev: FindKeyEvent,
  hasHighlight: boolean,
): FindDropdownIntent | null {
  if (!ev || typeof ev.key !== "string") return null;
  if (ev.metaKey || ev.ctrlKey || ev.altKey) return null;
  switch (ev.key) {
    case "ArrowDown":
      return "next";
    case "ArrowUp":
      return "prev";
    case "Enter":
      return hasHighlight ? "commit" : null;
    case "Escape":
      return "close";
    default:
      return null;
  }
}

// =====================================================================
// Slice 5: screen-reader announcer
// =====================================================================
//
// The find bar's status was purely visual — a sighted user saw "3 of 17"
// or "No matches" but a screen-reader user got nothing as they stepped
// through matches. This composes a single polite-live-region string from
// the interpreted status, debounced by equality in the component so the
// same phrase isn't re-announced. Mirrors the library-search /
// palette / beacon footer narration style.

/**
 * Compose the aria-live announcement for a find status + the query.
 * Empty string when idle (nothing to announce). Distinct, full-word
 * phrasing (not the compact visual label) so it reads naturally:
 *   - pending   -> "Searching for "foo"…"
 *   - not-found -> "No matches for "foo""
 *   - found     -> "Match 3 of 17 for "foo"" (+ ", wrapped to top/bottom"
 *                  when the step wrapped)
 */
export function announceFindStatus(status: FindStatus): string {
  if (!status || status.phase === "idle" || !status.query) return "";
  const q = `\u201c${status.query}\u201d`;
  switch (status.phase) {
    case "pending":
      return `Searching for ${q}\u2026`;
    case "not-found":
      return `No matches for ${q}`;
    case "found": {
      const base = `Match ${status.current} of ${status.total} for ${q}`;
      return status.wrapped ? `${base}, wrapped` : base;
    }
  }
}

