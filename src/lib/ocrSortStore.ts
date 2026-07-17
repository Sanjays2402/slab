// OCR Queue sort-preference persistence — round 52.
//
// The OCR Queue Panel lets a user re-sort BOTH lists by Name / Folder /
// Pages / State with a direction caret, but the choice lived only in
// ephemeral component state: pick "Pages desc", close the panel, reopen
// it — and it sprang back to name-asc. An operator triaging a recurring
// failure pile who always wants biggest-document-first had to re-pick it
// on every open. This is the thin localStorage shell that gives the queue
// the same muscle-memory the board sort (recentsView.ts), palette folds
// (paletteCollapsed.ts) and saved searches (savedSearches.ts) already have.
//
// SCOPE: the SORT only (column + direction). The transient search query
// and reason facet are deliberately NOT persisted — restoring a stale
// term/facet across a restart would reopen the queue showing a mysterious
// subset, a worse default than the full pile. Sort is a durable preference;
// search/facet are in-the-moment triage actions.
//
// The sort VALIDATION (isOcrSort) stays in the tested pure core
// `$lib/ocrQueueView`; this module is only the read/write shell.
//
// Storage:
//   slab.ocr.sort.v1 = {"field":"pages","dir":"desc"}
// A single OcrSort object. Unknown / malformed / schema-drifted values
// decode to the default (name-asc) so a corrupt value never wedges the panel.

import { isOcrSort, type OcrSort } from "./ocrQueueView";

const KEY = "slab.ocr.sort.v1";

/** The default sort the queue falls back to when nothing is persisted. */
export const DEFAULT_OCR_SORT: OcrSort = { field: "name", dir: "asc" };

/** Whether a sort is the default (name-asc) — stored as a cleared key. */
function isDefaultOcrSort(sort: OcrSort): boolean {
  return sort.field === "name" && sort.dir === "asc";
}

/**
 * Read the persisted OCR sort. Returns the default (name-asc) when unset,
 * in a non-browser context, or when the stored value is missing / malformed
 * / no longer a valid sort. Tolerant of garbage — a fresh copy each call
 * so callers can mutate freely.
 */
export function loadOcrSort(): OcrSort {
  if (typeof localStorage === "undefined") return { ...DEFAULT_OCR_SORT };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_OCR_SORT };
    const parsed = JSON.parse(raw);
    return isOcrSort(parsed) ? { field: parsed.field, dir: parsed.dir } : { ...DEFAULT_OCR_SORT };
  } catch {
    return { ...DEFAULT_OCR_SORT };
  }
}

/**
 * Persist the OCR sort. A best-effort write — a full localStorage (or a
 * non-browser context) silently no-ops so the sort still works in-session,
 * it just won't survive a restart. Storing the DEFAULT (name-asc) clears the
 * key so a fresh install and an explicitly-reset-to-name board read alike;
 * a garbage value is treated as a clear (never written).
 */
export function saveOcrSort(sort: OcrSort): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (!isOcrSort(sort) || isDefaultOcrSort(sort)) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify({ field: sort.field, dir: sort.dir }));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
