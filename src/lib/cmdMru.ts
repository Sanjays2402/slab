// Command Palette frecency store — v3.40.0 "Lumen" Slice 4.
//
// Tracks, per command id, how OFTEN and how RECENTLY the user invoked it
// from the ⌘K palette, so the empty-query "Recently used" group ranks by
// *frecency* (frequency blended with recency) the way Raycast/Arc do —
// instead of pure recency, where a command tapped once 5s ago buried a
// daily-driver used 50 times this week.
//
// The frecency math lives in the tested pure core `$lib/paletteSearch`
// (recencyWeight / frecencyScore / rankFrecency / recordFrecency); this
// module is the thin localStorage shell.
//
// Storage (v2):
//   slab.cmd.frecency.v2 = [{ id, count, lastUsedAt }, ...]
//
// Backward compatible: if only the legacy v1 list
//   slab.cmd.mru.v1 = ["accent:blue", "panel:reader", ...]
// is present (newest-first string ids), it is migrated on first read into
// synthetic records (count 1, descending timestamps so order is kept) and
// written forward to v2. The legacy key is then left untouched/ignored.

import {
  rankFrecency,
  recordFrecency,
  type FrecencyRecord,
} from "$lib/paletteSearch";

const KEY = "slab.cmd.frecency.v2";
const LEGACY_KEY = "slab.cmd.mru.v1";
const LIMIT = 64;

function now(): number {
  return Date.now();
}

/** Validate one decoded record shape defensively. */
function isRecord(x: unknown): x is FrecencyRecord {
  return (
    !!x &&
    typeof x === "object" &&
    typeof (x as FrecencyRecord).id === "string" &&
    typeof (x as FrecencyRecord).count === "number" &&
    typeof (x as FrecencyRecord).lastUsedAt === "number"
  );
}

/** Migrate the legacy newest-first string[] MRU into frecency records. */
function migrateLegacy(): FrecencyRecord[] | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(LEGACY_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;
    const ids = parsed.filter((x): x is string => typeof x === "string");
    if (ids.length === 0) return null;
    // Newest first -> descending synthetic timestamps so the existing
    // order is preserved on the very first frecency render.
    const base = now();
    const recs: FrecencyRecord[] = ids.map((id, i) => ({
      id,
      count: 1,
      lastUsedAt: base - i * 1000,
    }));
    return recs;
  } catch {
    return null;
  }
}

function read(): FrecencyRecord[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) {
      const migrated = migrateLegacy();
      if (migrated) {
        write(migrated);
        return migrated;
      }
      return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isRecord);
  } catch {
    return [];
  }
}

function write(records: FrecencyRecord[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(KEY, JSON.stringify(records.slice(0, LIMIT)));
  } catch {
    // localStorage full — best effort, ignore
  }
}

/** Record an invocation of `id` (bumps count + recency). */
export function recordMru(id: string): void {
  if (!id) return;
  write(recordFrecency(read(), id, now(), LIMIT));
}

/** Clear the entire frecency history (and the legacy key). */
export function clearMru(): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.removeItem(KEY);
    localStorage.removeItem(LEGACY_KEY);
  } catch {
    // ignore
  }
}

/**
 * Map of id -> rank, where rank 0 is the strongest frecency. The palette
 * uses this to float frequently+recently used actions to the top when the
 * query is empty. Empty object when there is no history.
 */
export function mruRanks(): Record<string, number> {
  return rankFrecency(read(), now());
}

/** Raw record count — used for the "Clear command history" subtitle. */
export function mruCount(): number {
  return read().length;
}
