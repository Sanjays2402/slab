// Library Search pinned-query persistence — "Atlas Recent-Searches" follow-up.
//
// The recent-search chip strip (LibrarySearchPanel) is a ROLLING log: the
// backend keeps only the most-recent N queries, so a search you run often
// but not recently gets evicted. Pinning promotes a query to a sticky
// saved-search chip that survives that eviction, living in localStorage
// independent of the backend history — exactly as paletteCollapsed.ts
// persists the palette's folded sections.
//
// The pin MATH (normalize / membership / labels) stays in the tested pure
// core `$lib/librarySearchView`; this module is only the thin storage shell
// that reads + writes WHICH queries are pinned, newest-pinned first.
//
// Storage:
//   slab.library.pinnedSearches.v1 = ["invoices 2024", "tax \"final\"", ...]
// A plain ordered list of trimmed query strings (newest pin first). Unknown
// or malformed values decode to [] so a corrupt entry never wedges the panel.

import { normalizePinnedQuery } from "./librarySearchView";

const KEY = "slab.library.pinnedSearches.v1";
/** Defensive cap so a pathological write can't bloat localStorage. */
const LIMIT = 32;

/**
 * Read the persisted pinned queries, newest-pinned first. Empty when unset
 * or the stored value is missing / malformed. Each entry is normalized
 * (trimmed, non-empty) and de-duplicated case-insensitively, keeping the
 * first (newest) occurrence. Tolerant of garbage.
 */
export function loadPinnedSearches(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const seen = new Set<string>();
    const out: string[] = [];
    for (const x of parsed) {
      const q = normalizePinnedQuery(typeof x === "string" ? x : "");
      if (!q) continue;
      const key = q.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(q);
      if (out.length >= LIMIT) break;
    }
    return out;
  } catch {
    return [];
  }
}

/**
 * Persist the pinned queries (newest first). A best-effort write — a full
 * localStorage (or a non-browser context) silently no-ops so pinning still
 * works in-session, it just won't survive a restart. An empty list clears
 * the key so a fresh install and an unpinned-everything state read alike.
 */
export function savePinnedSearches(queries: readonly string[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const x of Array.isArray(queries) ? queries : []) {
      const q = normalizePinnedQuery(typeof x === "string" ? x : "");
      if (!q) continue;
      const key = q.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(q);
      if (out.length >= LIMIT) break;
    }
    if (out.length === 0) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify(out));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
