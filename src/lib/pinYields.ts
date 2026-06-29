// Library Search saved-pin yield persistence — round-55 follow-up.
//
// The "run all saved searches" sweep (round 55) reports a live digest, but
// it evaporates on close: re-open the panel and a dry pin looks identical
// to a productive one until you sweep again. This module persists the LAST
// known hit count per pinned query so each saved chip can wear a yield
// badge — including a distinct "0" for a pin that came up empty — the
// moment the panel opens, before any re-run.
//
// The badge MATH (lookup / merge) stays in the tested pure core
// `$lib/librarySearchView`; this is only the thin localStorage shell that
// reads + writes the {query: count} map, exactly as savedSearches.ts /
// cmdPins.ts persist their lists.
//
// Storage:
//   slab.library.pinYields.v1 = { "invoices 2024": 84, "old tax": 0 }
// Keys are normalized (trimmed) queries; values are clamped non-negative
// integers. Malformed values decode to {} so a corrupt entry never wedges.

import { normalizePinnedQuery } from "./librarySearchView";

const KEY = "slab.library.pinYields.v1";
/** Defensive cap so a pathological write can't bloat localStorage. */
const LIMIT = 64;

function clean(raw: unknown): Record<string, number> {
  const out: Record<string, number> = {};
  if (!raw || typeof raw !== "object") return out;
  let n = 0;
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    const q = normalizePinnedQuery(k);
    if (!q) continue;
    const c = Math.max(0, Math.trunc(Number(v)));
    if (!Number.isFinite(c)) continue;
    out[q] = c;
    if (++n >= LIMIT) break;
  }
  return out;
}

/**
 * Read the persisted per-pin yields. Empty when unset or malformed.
 * Garbage-safe: a non-object, non-numeric counts, or a corrupt JSON blob
 * all decode to {}.
 */
export function loadPinYields(): Record<string, number> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    return clean(JSON.parse(raw));
  } catch {
    return {};
  }
}

/**
 * Persist the per-pin yields. Best-effort — a full localStorage (or a
 * non-browser context) silently no-ops. An empty map clears the key so a
 * fresh install and a swept-then-cleared state read alike.
 */
export function savePinYields(yields: Record<string, number>): void {
  if (typeof localStorage === "undefined") return;
  try {
    const out = clean(yields);
    if (Object.keys(out).length === 0) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify(out));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
