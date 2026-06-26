// Shortcuts-overlay core — v3.40.0 "Lexicon" Slice 1.
//
// Pure, DOM-free model for the "?" keyboard cheat-sheet
// (`ShortcutsOverlay.svelte`). The overlay used to hand-maintain a giant
// literal array of ~120 rows that silently DRIFTED from the real keymap:
// a Bates row bound to a key the backend never registered, a Theater
// section missing actions that shipped later, every rebind invisible
// until someone edited the array by hand. The fix is to drive the
// bindable rows straight off the live `keymapView` store and reduce the
// hand-maintained part to a small set of genuinely-unbindable "info"
// hints (Esc closes, ↑/↓ scroll, the Theater pen tools).
//
// This module owns the grouping/merge/order math as pure functions so
// every branch is unit-tested without a DOM — same pure-core/thin-shell
// discipline as `paletteSearch.ts` and `toastStack.ts`. The Svelte
// component only renders what these functions return.

/** Minimal shape we read off a live keymap action (matches KeymapAction). */
export interface ShortcutActionLike {
  id: string;
  label: string;
  group: string;
  /** Canonical binding string, e.g. "Mod+Shift+K". */
  binding: string;
  /** Canonical default; used to derive the customized flag if absent. */
  default_binding?: string;
  /** True when the user rebound this from its default. */
  is_override?: boolean;
}

/** A hand-curated non-bindable hint (Esc, scroll keys, Theater pen). */
export interface ShortcutInfoSpec {
  /** Section heading this hint belongs under. */
  group: string;
  /** Human label. */
  label: string;
  /** Pre-split display keys, e.g. ["⇧", "↵"] or ["W"]. */
  keys: string[];
}

/** One rendered row — either a live keymap binding or a static hint. */
export interface ShortcutRow {
  /** Stable key for `{#each}`. */
  key: string;
  /** Keymap action id, or null for a static info row. */
  actionId: string | null;
  /** Human label. */
  label: string;
  /** Canonical binding string for bindable rows; "" for info rows. */
  binding: string;
  /** Display keys for info rows; [] for bindable rows (component derives). */
  staticKeys: string[];
  /** True when the user customized this binding. */
  isOverride: boolean;
}

/** A titled section of rows. */
export interface ShortcutGroup {
  title: string;
  rows: ShortcutRow[];
}

/**
 * Curated section order. Groups not listed here sort alphabetically
 * AFTER every known group, so a future keymap group still appears (at
 * the end) without a code change — the overlay can never silently drop
 * a section again.
 */
export const SHORTCUT_GROUP_ORDER: readonly string[] = [
  "Global",
  "Tabs",
  "Reading",
  "Beacon",
  "Library",
  "Theater",
  "Forms",
  "Atelier",
  "Hopper",
  "Archive",
  "Press",
  "Home",
];

/**
 * Rank a group title for ordering. Known groups keep their curated
 * index; unknown groups get a large rank so they sort after known ones
 * (ties broken alphabetically by the caller).
 */
export function shortcutGroupRank(title: string): number {
  const i = SHORTCUT_GROUP_ORDER.indexOf(title);
  return i === -1 ? SHORTCUT_GROUP_ORDER.length : i;
}

/** Derive the customized flag, tolerating a missing is_override field. */
function isCustomized(a: ShortcutActionLike): boolean {
  if (typeof a.is_override === "boolean") return a.is_override;
  if (typeof a.default_binding === "string") {
    return a.binding.trim() !== a.default_binding.trim();
  }
  return false;
}

/**
 * Build the ordered, grouped overlay model from the live keymap actions
 * plus the curated info hints. Bindable rows come first within a group
 * (in keymap order), info hints follow. Groups are ordered by
 * `shortcutGroupRank` then alphabetically; empty groups are dropped.
 *
 * Pure: never mutates its inputs. Defensive against a null/garbage
 * actions array (treated as empty) so a failed `bootKeymap()` degrades
 * to just the info hints rather than throwing.
 */
export function buildShortcutGroups(
  actions: ShortcutActionLike[],
  infos: ShortcutInfoSpec[] = [],
): ShortcutGroup[] {
  const byTitle = new Map<string, ShortcutRow[]>();
  const ensure = (title: string): ShortcutRow[] => {
    let rows = byTitle.get(title);
    if (!rows) {
      rows = [];
      byTitle.set(title, rows);
    }
    return rows;
  };

  if (Array.isArray(actions)) {
    for (const a of actions) {
      if (!a || typeof a.id !== "string" || typeof a.group !== "string") continue;
      ensure(a.group).push({
        key: `act:${a.id}`,
        actionId: a.id,
        label: typeof a.label === "string" ? a.label : a.id,
        binding: typeof a.binding === "string" ? a.binding : "",
        staticKeys: [],
        isOverride: isCustomized(a),
      });
    }
  }

  if (Array.isArray(infos)) {
    infos.forEach((info, i) => {
      if (!info || typeof info.group !== "string") return;
      ensure(info.group).push({
        key: `info:${info.group}:${i}`,
        actionId: null,
        label: typeof info.label === "string" ? info.label : "",
        binding: "",
        staticKeys: Array.isArray(info.keys) ? info.keys.slice() : [],
        isOverride: false,
      });
    });
  }

  const titles = [...byTitle.keys()].sort((a, b) => {
    const ra = shortcutGroupRank(a);
    const rb = shortcutGroupRank(b);
    return ra !== rb ? ra - rb : a.localeCompare(b);
  });

  const out: ShortcutGroup[] = [];
  for (const title of titles) {
    const rows = byTitle.get(title)!;
    if (rows.length > 0) out.push({ title, rows });
  }
  return out;
}

/** Total bindable + info row count across every group (for the header). */
export function countShortcutRows(groups: ShortcutGroup[]): number {
  if (!Array.isArray(groups)) return 0;
  return groups.reduce((n, g) => n + (Array.isArray(g.rows) ? g.rows.length : 0), 0);
}
