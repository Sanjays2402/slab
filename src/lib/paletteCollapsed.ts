// Command Palette collapsed-group persistence — v3.57.0 "Lumen" follow-up.
//
// Round 47 shipped group COLLAPSE in the palette (click a section header to
// fold it), but the fold set lived only in ephemeral component state — fold
// "Appearance" away, restart Slab, and it sprang back open. Raycast/Arc
// remember a folded section across launches; this is the thin localStorage
// shell that gives the palette the same muscle-memory.
//
// The collapse MATH (toggleCollapsedGroup / partitionCollapsedGroups) stays
// in the tested pure core `$lib/paletteSearch`; this module only persists
// WHICH group names are folded, exactly as cmdMru.ts persists frecency.
//
// Storage:
//   slab.palette.collapsed.v1 = ["Appearance", "Library", ...]
// A plain newest-irrelevant set of group names. Unknown/garbage decodes to
// an empty set so a corrupt value never wedges the palette.

const KEY = "slab.palette.collapsed.v1";
/** Defensive cap so a pathological write can't bloat localStorage. */
const LIMIT = 64;

/** Read the persisted set of collapsed group names. Empty when unset or
 *  the stored value is missing / malformed. Tolerant of garbage. */
export function loadCollapsedGroups(): Set<string> {
  if (typeof localStorage === "undefined") return new Set();
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    const names = parsed.filter((x): x is string => typeof x === "string" && x.length > 0);
    return new Set(names.slice(0, LIMIT));
  } catch {
    return new Set();
  }
}

/** Persist the set of collapsed group names. A best-effort write — a full
 *  localStorage (or a non-browser context) silently no-ops so collapse
 *  still works in-session, it just won't survive a restart. */
export function saveCollapsedGroups(groups: ReadonlySet<string>): void {
  if (typeof localStorage === "undefined") return;
  try {
    const names = [...groups].filter((g) => typeof g === "string" && g.length > 0).slice(0, LIMIT);
    if (names.length === 0) {
      // Keep the key tidy: an all-open palette stores nothing rather than
      // an empty array, so a fresh install and a fully-expanded one read
      // identically.
      localStorage.removeItem(KEY);
      return;
    }
    localStorage.setItem(KEY, JSON.stringify(names));
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
