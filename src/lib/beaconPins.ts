// Beacon cache-inspector pinned-model persistence.
//
// The inspector's model tiles are a click-to-filter facet, but the choice
// is ephemeral — refresh or reopen and the filter resets. A user who lives
// in one embed model wants it pinned to a sticky strip so refiltering is one
// tap, exactly as savedSearches.ts pins a query and cmdPins.ts pins a palette
// command. This is the thin read/write shell that persists WHICH model names
// are pinned, oldest-pin-first.
//
// The pin MATH (toggle / membership / live-reconcile) lives in the tested
// pure core `$lib/beaconCacheView`; this module is only storage.
//
// Storage:
//   slab.beacon.pinnedModels.v1 = ["nomic-embed-text", "mxbai-large", ...]
// A plain ordered list of model names. Unknown / malformed values decode to
// [] so a corrupt entry never wedges the inspector.

const KEY = "slab.beacon.pinnedModels.v1";
/** Defensive cap so a pathological write can't bloat localStorage. */
const LIMIT = 32;

function clean(list: unknown): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const x of Array.isArray(list) ? list : []) {
    if (typeof x !== "string" || !x) continue;
    if (seen.has(x)) continue;
    seen.add(x);
    out.push(x);
    if (out.length >= LIMIT) break;
  }
  return out;
}

/**
 * Read the persisted pinned model names, oldest-pin-first. Empty when unset
 * or the stored value is missing / malformed. De-duplicated, garbage-safe.
 */
export function loadPinnedModels(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    return clean(JSON.parse(raw));
  } catch {
    return [];
  }
}

/**
 * Persist the pinned model names. Best-effort — a full localStorage (or a
 * non-browser context) silently no-ops. An empty list clears the key so a
 * fresh install and an unpinned-everything state read alike.
 */
export function savePinnedModels(models: readonly string[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    const out = clean(models);
    if (out.length === 0) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify(out));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
