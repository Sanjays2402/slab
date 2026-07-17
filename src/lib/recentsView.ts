// RecentsHome view-preference persistence — round 51 follow-up.
//
// The first-launch hero (RecentsHome) lets a user re-sort the board
// (Recent / Name / Progress / Pages), but the choice lived only in
// ephemeral component state: pick "Name", close Slab, reopen — and it
// sprang back to "Recent". A power user who always wants their library
// alphabetised had to re-pick it every single launch. This is the thin
// localStorage shell that gives the board the same muscle-memory the
// palette's folded sections (paletteCollapsed.ts) and the saved searches
// (savedSearches.ts) already have.
//
// SCOPE: the SORT mode only. The transient filter QUERY is deliberately
// NOT persisted — restoring a stale search term across an app restart
// would reopen the board with most recents mysteriously hidden behind a
// filter the user forgot they typed, a worse default than a clean board.
// Sort is a durable preference; a filter is an in-the-moment action.
//
// The sort-mode VALIDATION (isRecentSortMode) stays in the tested pure
// core `$lib/recentsHomeView`; this module is only the read/write shell.
//
// Storage:
//   slab.recents.sort.v1 = "name"
// A single mode string. Unknown / malformed / a since-removed mode decodes
// to the default ("recent") so a corrupt value never wedges the board.

import { isRecentSortMode, type RecentSortMode } from "./recentsHomeView";

const KEY = "slab.recents.sort.v1";

/** The default sort the board falls back to when nothing is persisted. */
export const DEFAULT_RECENT_SORT: RecentSortMode = "recent";

/**
 * Read the persisted recents sort mode. Returns the default ("recent")
 * when unset, in a non-browser context, or when the stored value is
 * missing / malformed / no longer a valid mode. Tolerant of garbage.
 */
export function loadRecentSort(): RecentSortMode {
  if (typeof localStorage === "undefined") return DEFAULT_RECENT_SORT;
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_RECENT_SORT;
    return isRecentSortMode(raw) ? raw : DEFAULT_RECENT_SORT;
  } catch {
    return DEFAULT_RECENT_SORT;
  }
}

/**
 * Persist the recents sort mode. A best-effort write — a full localStorage
 * (or a non-browser context) silently no-ops so the sort still works
 * in-session, it just won't survive a restart. Storing the DEFAULT clears
 * the key so a fresh install and an explicitly-reset-to-Recent board read
 * identically; a garbage / unknown mode is treated as a clear (never
 * written) so the store can't hold a value the core would reject.
 */
export function saveRecentSort(mode: RecentSortMode): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (!isRecentSortMode(mode) || mode === DEFAULT_RECENT_SORT) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, mode);
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
