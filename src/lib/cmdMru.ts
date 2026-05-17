// Command Palette MRU (most recently used) — v1.0.0 "Glass" Slice 5.
//
// Tiny localStorage-backed list of action IDs (strings) the user has most
// recently invoked from the Command Palette. The palette uses this to float
// frequently-used actions to the top when the query is empty, so opening
// ⌘K and just pressing Enter does the right thing 80% of the time.
//
// Layout:
//   slab.cmd.mru.v1 = ["accent:blue", "panel:reader", "theme:dark", ...]
//
// At most LIMIT entries. New entries go to position 0; duplicates are pulled
// to the front. The list is independent of the action catalog — IDs that no
// longer resolve are silently ignored at render time.

const KEY = "slab.cmd.mru.v1";
const LIMIT = 16;

function read(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === "string");
  } catch {
    return [];
  }
}

function write(ids: string[]) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(KEY, JSON.stringify(ids.slice(0, LIMIT)));
  } catch {
    // localStorage full — best effort, ignore
  }
}

/** Get the current MRU list (newest first). */
export function listMru(): string[] {
  return read();
}

/** Bump an action ID to position 0. */
export function recordMru(id: string) {
  if (!id) return;
  const cur = read().filter((x) => x !== id);
  cur.unshift(id);
  write(cur);
}

/** Clear the entire MRU list. */
export function clearMru() {
  write([]);
}

/**
 * Map of id → rank, where rank 0 is most recent. Useful for sorting in O(1).
 * Returns an empty object when there is no MRU history.
 */
export function mruRanks(): Record<string, number> {
  const cur = read();
  const out: Record<string, number> = {};
  for (let i = 0; i < cur.length; i++) out[cur[i]] = i;
  return out;
}
