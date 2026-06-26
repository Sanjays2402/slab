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
