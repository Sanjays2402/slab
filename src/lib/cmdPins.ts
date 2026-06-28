// Command Palette pinned-command persistence — round 52.
//
// cmdMru floats frequently-used actions in the browse "Recently used" row,
// but it's a moving target — a daily-driver command drops off after a burst
// of other work. Pinning promotes a command to a sticky "Pinned" section at
// the top of browse that survives frecency churn, exactly as savedSearches.ts
// pins a search and paletteCollapsed.ts persists folded sections.
//
// The pin MATH (toggle / membership / count) lives in the tested pure core
// `$lib/paletteSearch`; this module is only the thin read/write shell that
// persists WHICH command ids are pinned, oldest-pin-first.
//
// Storage:
//   slab.cmd.pinned.v1 = ["panel:reader", "accent:blue", ...]
// A plain ordered list of command ids. Unknown / malformed values decode to
// [] so a corrupt entry never wedges the palette.

const KEY = "slab.cmd.pinned.v1";
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
 * Read the persisted pinned command ids, oldest-pin-first. Empty when unset
 * or the stored value is missing / malformed. De-duplicated, garbage-safe.
 */
export function loadPinnedCommands(): string[] {
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
 * Persist the pinned command ids. Best-effort — a full localStorage (or a
 * non-browser context) silently no-ops. An empty list clears the key so a
 * fresh install and an unpinned-everything state read alike.
 */
export function savePinnedCommands(ids: readonly string[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    const out = clean(ids);
    if (out.length === 0) {
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify(out));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
