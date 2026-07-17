// Beacon Cache Inspector sort-preference persistence — round 52.
//
// The inspector lets a user re-sort the indexed-PDF table by Name / Model /
// Chunks / Pages / Indexed with a direction caret, but the choice lived only
// in ephemeral component state: pick "Chunks desc" to find the heaviest docs,
// close the inspector, reopen — and it sprang back to Indexed-desc. A user
// pruning a bloated cache who always wants biggest-chunk-first had to re-pick
// it on every open. This is the thin localStorage shell mirroring the OCR
// queue sort (ocrSortStore.ts) and the board sort (recentsView.ts).
//
// SCOPE: the SORT only (column + direction). The transient search query and
// model facet are deliberately NOT persisted — restoring a stale term/facet
// would reopen the table showing a mysterious subset, a worse default than
// the full cache. Sort is durable; search/facet are in-the-moment actions.
//
// The sort VALIDATION (isBeaconSort) stays in the tested pure core
// `$lib/beaconCacheView`; this module is only the read/write shell.
//
// Storage:
//   slab.beacon.sort.v1 = {"field":"chunks","dir":"desc"}
// A single BeaconSort object. Unknown / malformed / schema-drifted values
// decode to the default (indexed-desc, newest-first) so a corrupt value
// never wedges the inspector.

import { isBeaconSort, type BeaconSort } from "./beaconCacheView";

const KEY = "slab.beacon.sort.v1";

/** The default sort the inspector falls back to (newest-indexed first). */
export const DEFAULT_BEACON_SORT: BeaconSort = { field: "indexed", dir: "desc" };

/** Whether a sort is the default (indexed-desc) — stored as a cleared key. */
function isDefaultBeaconSort(sort: BeaconSort): boolean {
  return sort.field === "indexed" && sort.dir === "desc";
}

/**
 * Read the persisted beacon sort. Returns the default (indexed-desc) when
 * unset, in a non-browser context, or when the stored value is missing /
 * malformed / no longer a valid sort. Tolerant of garbage — a fresh copy
 * each call so callers can mutate freely.
 */
export function loadBeaconSort(): BeaconSort {
  if (typeof localStorage === "undefined") return { ...DEFAULT_BEACON_SORT };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_BEACON_SORT };
    const parsed = JSON.parse(raw);
    return isBeaconSort(parsed) ? { field: parsed.field, dir: parsed.dir } : { ...DEFAULT_BEACON_SORT };
  } catch {
    return { ...DEFAULT_BEACON_SORT };
  }
}

/**
 * Persist the beacon sort. A best-effort write — a full localStorage (or a
 * non-browser context) silently no-ops so the sort still works in-session,
 * it just won't survive a restart. Storing the DEFAULT (indexed-desc) clears
 * the key so a fresh install and an explicitly-reset board read alike; a
 * garbage value is treated as a clear (never written).
 */
export function saveBeaconSort(sort: BeaconSort): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (!isBeaconSort(sort) || isDefaultBeaconSort(sort)) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify({ field: sort.field, dir: sort.dir }));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
