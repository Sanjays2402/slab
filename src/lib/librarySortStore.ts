// Library Search sort-preference persistence — round 52.
//
// The Library Search panel (Cmd+Shift+F) lets a user re-rank matched docs
// by Relevance / Document / Matches, but the choice lived only in ephemeral
// state: pick "Document" to scan alphabetically, close the panel, run a new
// search — and it sprang back to Relevance. A user who always reads results
// document-first had to re-pick it every session. This is the thin
// localStorage shell mirroring the OCR queue (ocrSortStore.ts) and board
// (recentsView.ts) sort persistence.
//
// SCOPE: the SORT mode only. The transient query + in-results refine are
// deliberately NOT persisted; sort is a durable preference, the query/refine
// are in-the-moment. The mode VALIDATION (isSearchSortMode) stays in the
// tested pure core `$lib/librarySearchView`; this is only the read/write shell.
//
// Storage:
//   slab.library.sort.v1 = "document"
// A single mode string. Unknown / malformed / removed modes decode to the
// default ("relevance") so a corrupt value never wedges the panel.

import { isSearchSortMode, type SearchSortMode } from "./librarySearchView";

const KEY = "slab.library.sort.v1";

/** The default sort the panel falls back to when nothing is persisted. */
export const DEFAULT_LIBRARY_SORT: SearchSortMode = "relevance";

/**
 * Read the persisted library sort mode. Returns the default ("relevance")
 * when unset, in a non-browser context, or when the stored value is missing
 * / malformed / no longer valid. Tolerant of garbage.
 */
export function loadLibrarySort(): SearchSortMode {
  if (typeof localStorage === "undefined") return DEFAULT_LIBRARY_SORT;
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_LIBRARY_SORT;
    return isSearchSortMode(raw) ? raw : DEFAULT_LIBRARY_SORT;
  } catch {
    return DEFAULT_LIBRARY_SORT;
  }
}

/**
 * Persist the library sort mode. Best-effort — a full localStorage (or a
 * non-browser context) silently no-ops. Storing the DEFAULT ("relevance")
 * clears the key so a fresh install and an explicitly-reset panel read alike;
 * a garbage mode is treated as a clear (never written).
 */
export function saveLibrarySort(mode: SearchSortMode): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (!isSearchSortMode(mode) || mode === DEFAULT_LIBRARY_SORT) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, mode);
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
