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

import { scorePaletteField, type PaletteRange } from "./paletteSearch";

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
export function countShortcutRows(
  groups: ReadonlyArray<{ rows: ReadonlyArray<unknown> }>,
): number {
  if (!Array.isArray(groups)) return 0;
  return groups.reduce((n, g) => n + (Array.isArray(g.rows) ? g.rows.length : 0), 0);
}

// --- Filter-as-you-type (Lexicon Slice 2) ----------------------------
//
// The overlay shows 50+ rows; scanning for one shortcut means eyeballing
// every section. A search box filters live. We reuse the tested palette
// scorer (`scorePaletteField`) so highlight ranges and tie-breaking are
// the SAME engine the ⌘K palette uses — no second fuzzy implementation
// to drift.
//
// A row matches the query if any of these score > 0:
//   - its label (the visible text) — ranges from here drive the <mark>s
//   - its CANONICAL key text ("Mod+Shift+K", "Esc") so "shift" or "esc"
//     finds rows by their keys, not just their names
//   - its group title, so "theater" surfaces the whole Theater section
//
// We only ever highlight the LABEL (the palette's discipline: never mark
// a substring the user can't see). Stable order is preserved within each
// group — this is a reference sheet, so "learn where things live" beats
// re-ranking on every keystroke.

/** A row plus the label ranges that matched the active filter query. */
export interface FilteredShortcutRow extends ShortcutRow {
  /** Matched character ranges in `label` (empty when query is empty). */
  titleRanges: PaletteRange[];
}

/** A filtered, titled section. */
export interface FilteredShortcutGroup {
  title: string;
  rows: FilteredShortcutRow[];
}

/** Searchable key text for a row: canonical binding + any static keys. */
function rowKeyText(row: ShortcutRow): string {
  const parts: string[] = [];
  if (row.binding) parts.push(row.binding);
  if (Array.isArray(row.staticKeys) && row.staticKeys.length) {
    parts.push(row.staticKeys.join(" "));
  }
  return parts.join(" ");
}

/**
 * Filter `groups` by `query`, returning only matching rows (with label
 * highlight ranges) and dropping any group left empty. An empty/blank
 * query returns every row with empty ranges (the unfiltered sheet).
 *
 * Pure + defensive: a null groups array yields []; a row that matches
 * only on its keys or its group title is kept with empty title ranges
 * (nothing to highlight in the visible label).
 */
export function filterShortcutGroups(
  groups: ShortcutGroup[],
  query: string,
): FilteredShortcutGroup[] {
  if (!Array.isArray(groups)) return [];
  const q = (query ?? "").trim();

  const passAll = (g: ShortcutGroup): FilteredShortcutGroup => ({
    title: g.title,
    rows: g.rows.map((r) => ({ ...r, titleRanges: [] })),
  });

  if (!q) return groups.map(passAll);

  const out: FilteredShortcutGroup[] = [];
  for (const g of groups) {
    // A group-title hit surfaces the entire section (with no per-row
    // label marks — the section heading is what matched).
    const titleHit = scorePaletteField(q, g.title).score > 0;
    if (titleHit) {
      out.push(passAll(g));
      continue;
    }
    const rows: FilteredShortcutRow[] = [];
    for (const r of g.rows) {
      const labelScore = scorePaletteField(q, r.label);
      if (labelScore.score > 0) {
        rows.push({ ...r, titleRanges: labelScore.ranges });
        continue;
      }
      // Fall back to a key-text match (no visible label highlight).
      if (scorePaletteField(q, rowKeyText(r)).score > 0) {
        rows.push({ ...r, titleRanges: [] });
      }
    }
    if (rows.length > 0) out.push({ title: g.title, rows });
  }
  return out;
}

// --- Flat navigation model (Lexicon Slice 3) -------------------------
//
// The overlay renders as titled sections, but a keyboard user wants to
// walk it as ONE list (↓ from the last row of a section lands on the
// first row of the next). We flatten the grouped model into a single
// indexable array — each entry carries its group title + whether it's
// the first row of its group, so the renderer can emit a section heading
// inline while the cursor math stays a simple flat index. The actual
// arrow/Home/End math is reused from the tested palette nav core
// (`classifyPaletteNav` / `nextPaletteIndex`) — no second implementation.

/** One row flattened out of its group, with render hints. */
export interface FlatShortcutEntry<R extends ShortcutRow = ShortcutRow> {
  /** The section this row belongs to. */
  groupTitle: string;
  /** True when this is the first row of its group (renderer emits an h3). */
  isGroupStart: boolean;
  /** Flat index across every group (0-based). */
  flatIndex: number;
  /** The underlying row (FilteredShortcutRow when filtering). */
  row: R;
}

/**
 * Flatten grouped rows into a single navigable list, tagging the first
 * row of each group so the renderer can place section headers without a
 * second pass. Pure; tolerates a null/garbage groups array (-> []).
 */
export function flattenShortcutRows<R extends ShortcutRow>(
  groups: ReadonlyArray<{ title: string; rows: R[] }>,
): FlatShortcutEntry<R>[] {
  if (!Array.isArray(groups)) return [];
  const out: FlatShortcutEntry<R>[] = [];
  let flat = 0;
  for (const g of groups) {
    if (!g) continue;
    const rows: R[] = Array.isArray(g.rows) ? g.rows : [];
    for (let i = 0; i < rows.length; i++) {
      out.push({
        groupTitle: g.title,
        isGroupStart: i === 0,
        flatIndex: flat,
        row: rows[i],
      });
      flat++;
    }
  }
  return out;
}

// --- Conflict detection (Lexicon Slice 4) ----------------------------
//
// Two actions bound to the SAME chord is a real, shipping bug — e.g.
// `library.search` and `forms.open` both default to Mod+Shift+F, so the
// second handler can shadow the first. A pro cheat sheet (VSCode,
// Raycast) flags these so the user knows WHY a key "doesn't work" and
// can rebind one. We compute conflicts over the live bindable rows by
// canonicalizing each binding (case-insensitive, modifier-order-
// independent) and grouping action ids that collide.
//
// Only keymap-bindable rows participate — the static info hints
// (Esc, scroll keys) are intentionally non-unique panel-local chords and
// would produce noise. Empty/blank bindings are ignored.

/** A set of 2+ actions that share one chord. */
export interface ShortcutConflict {
  /** Canonical chord they collide on (e.g. "mod+shift+f"). */
  canonical: string;
  /** Action ids sharing the chord, in first-seen order. */
  actionIds: string[];
}

/**
 * Canonicalize a binding string so "Mod+Shift+F", "shift+mod+f", and
 * "MOD+SHIFT+F" all compare equal: lowercase, split on "+", sort the
 * modifier tokens, keep the final key last. Tolerates the literal "+"
 * key ("Mod++") and a bare key. Blank -> "".
 */
export function canonicalizeBinding(binding: string): string {
  const s = (binding ?? "").trim().toLowerCase();
  if (!s) return "";
  // Split into [...modifiers, key], preserving a trailing literal "+".
  let tokens: string[];
  if (s.endsWith("++")) {
    tokens = s.slice(0, -2).split("+");
    tokens.push("+");
  } else if (s === "+") {
    tokens = ["+"];
  } else {
    tokens = s.split("+");
  }
  tokens = tokens.map((t) => t.trim()).filter((t) => t.length > 0);
  if (tokens.length === 0) return "";
  const key = tokens[tokens.length - 1];
  const mods = tokens.slice(0, -1);
  // Normalize modifier aliases so option==alt, control==ctrl, command==mod.
  const normMod = (m: string): string => {
    switch (m) {
      case "option":
      case "opt":
        return "alt";
      case "control":
        return "ctrl";
      case "command":
      case "cmd":
      case "meta":
        return "mod";
      default:
        return m;
    }
  };
  const sortedMods = [...new Set(mods.map(normMod))].sort();
  return [...sortedMods, key].join("+");
}

/**
 * Find every chord shared by 2+ bindable actions across the grouped
 * model. Returns one entry per colliding chord (with the action ids),
 * ordered by first appearance. Static info rows and blank bindings are
 * ignored. Pure; null/garbage groups -> [].
 */
export function detectShortcutConflicts(groups: ShortcutGroup[]): ShortcutConflict[] {
  if (!Array.isArray(groups)) return [];
  const byChord = new Map<string, string[]>();
  const order: string[] = [];
  for (const g of groups) {
    if (!g || !Array.isArray(g.rows)) continue;
    for (const r of g.rows) {
      if (!r || !r.actionId) continue; // info rows excluded
      const canonical = canonicalizeBinding(r.binding);
      if (!canonical) continue;
      let ids = byChord.get(canonical);
      if (!ids) {
        ids = [];
        byChord.set(canonical, ids);
        order.push(canonical);
      }
      if (!ids.includes(r.actionId)) ids.push(r.actionId);
    }
  }
  const out: ShortcutConflict[] = [];
  for (const canonical of order) {
    const actionIds = byChord.get(canonical)!;
    if (actionIds.length >= 2) out.push({ canonical, actionIds });
  }
  return out;
}

/**
 * Build a fast lookup: action id -> true when that action is part of a
 * conflict. The overlay uses this to flag each colliding row inline.
 */
export function conflictingActionIds(groups: ShortcutGroup[]): Set<string> {
  const out = new Set<string>();
  for (const c of detectShortcutConflicts(groups)) {
    for (const id of c.actionIds) out.add(id);
  }
  return out;
}
