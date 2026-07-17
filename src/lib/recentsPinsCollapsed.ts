// RecentsHome pinned-strip collapsed-state persistence.
//
// The first-launch board pins favourite documents to a horizontal strip
// above the recents grid. For a user with many pins the strip eats the top
// of the board every launch, even when they're really after the recents
// below. This thin localStorage shell remembers a one-bit preference: is the
// pinned strip collapsed? — the same muscle-memory the palette's folded
// sections (paletteCollapsed.ts) and the recents sort (recentsView.ts)
// already have.
//
// SCOPE: collapsed vs expanded only. Default is EXPANDED (false) so a fresh
// install shows pins. Storing the default clears the key, so expanded and
// fresh-install read identically.
//
// Storage:
//   slab.recents.pinsCollapsed.v1 = "1"
// A single flag. Anything other than "1" (incl. absent / garbage) -> expanded.

const KEY = "slab.recents.pinsCollapsed.v1";

/** Read whether the pinned strip should start collapsed. Default false. */
export function loadPinsCollapsed(): boolean {
  if (typeof localStorage === "undefined") return false;
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * Persist the pinned-strip collapsed flag. Best-effort. Expanded (false)
 * clears the key so fresh-install and explicitly-expanded read alike.
 */
export function savePinsCollapsed(collapsed: boolean): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (collapsed) {
      localStorage.setItem(KEY, "1");
    } else {
      localStorage.removeItem(KEY);
    }
  } catch {
    // localStorage full / unavailable — best effort, ignore.
  }
}
