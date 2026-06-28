// RecentsHome view-core — v3.43.0 "Atlas V".
//
// Pure, DOM-free model for Slab's first-launch hero (`RecentsHome.svelte`)
// — the single highest-visibility surface in the app, the screen every
// user sees the moment they open Slab with no document loaded. It shipped
// as a flat, mouse-only board: a "Continue reading" hero card, a pinned
// strip, and a recents grid. There was no way to FILTER it (a user near
// the 12-file cap had to eyeball the grid), no keyboard path through the
// cards (every open/pin/remove was a click), no sort control (always
// openedAt-desc), no summary of what you were looking at — and ZERO
// pure-core tests on the hero-selection / partition logic that decides
// what the headline card even is.
//
// This module owns the candidate-selection / partition / filter / sort /
// keyboard / summarize math as pure functions so every branch is
// unit-tested without a DOM — the same pure-core / thin-shell discipline
// as `librarySearchView.ts`, `readerFindView.ts`, `beaconCacheView.ts`,
// `paletteSearch.ts`, and `toastStack.ts`. Filtering, highlighting,
// keyboard nav, and the reading-progress chip all REUSE the tested
// palette core (`scorePaletteField` / `splitHighlight` /
// `classifyPaletteNav` / `nextPaletteIndex` / `recentReadingProgress`)
// rather than rolling a second fuzzy/nav/progress engine, so behaviour is
// identical to the command palette, the library search panel, and the
// beacon cache inspector.

import {
  scorePaletteField,
  splitHighlight,
  classifyPaletteNav,
  nextPaletteIndex,
  recentReadingProgress,
  type HighlightSegment,
  type PaletteNavIntent,
} from "./paletteSearch";

/**
 * The fields the view-core reads off a recent file. Mirrors `RecentFile`
 * (recent.ts) but kept structural so the pure helpers stay decoupled from
 * the store type and trivially testable.
 */
export interface RecentLike {
  path: string;
  name: string;
  openedAt: number;
  pageCount?: number;
  pinned?: boolean;
  pinOrder?: number;
  lastPage?: number;
  totalPages?: number;
  lastReadAt?: number;
}

/** Whether a value is a usable positive integer count/page. */
function posInt(n: unknown): n is number {
  return typeof n === "number" && Number.isFinite(n) && n >= 1;
}

/** Pluralize a word against a count ("1 document" / "3 documents"). */
function plural(n: number, word: string): string {
  return `${word}${n === 1 ? "" : "s"}`;
}

/** The page count we sort/scan by: explicit total beats cached pageCount. */
function pageCountOf(f: RecentLike): number {
  if (posInt(f?.totalPages)) return Math.floor(f.totalPages as number);
  if (posInt(f?.pageCount)) return Math.floor(f.pageCount as number);
  return 0;
}

// =====================================================================
// Slice 1: continue-candidate + partition model
// =====================================================================
//
// The hero card surfaces the single most useful next action: the file
// with the freshest reading momentum. The selection rule (prefer an
// in-progress file by most-recent read, else the top recent) and the
// partition into hero / pinned / others lived as inline `$derived`
// blocks in the component with no tests — yet they decide what the
// app's headline even is. Extracted here so the contract is pinned.

/** Coarse kind of hero the board should render. */
export type HeroKind = "resume" | "cold" | "empty";

/**
 * Pick the "Continue reading" candidate. Prefers files that are
 * partway through (have a lastPage strictly before totalPages),
 * newest-read first (lastReadAt, falling back to openedAt). With no
 * in-progress file it falls back to the FIRST entry of the supplied
 * list (the store already sorts pinned-first then openedAt-desc, so
 * this preserves the shipped behaviour exactly). Empty/garbage -> null.
 */
export function chooseContinueCandidate<T extends RecentLike>(files: readonly T[]): T | null {
  if (!Array.isArray(files)) return null;
  const list = files.filter(Boolean);
  if (list.length === 0) return null;
  const inProgress = list
    .filter((r) => posInt(r.lastPage) && posInt(r.totalPages) && (r.lastPage as number) < (r.totalPages as number))
    .sort((a, b) => (b.lastReadAt ?? b.openedAt) - (a.lastReadAt ?? a.openedAt));
  if (inProgress.length > 0) return inProgress[0];
  return list[0] ?? null;
}

/** Classify the hero into resume (has progress) / cold / empty. */
export function heroKind(hero: RecentLike | null | undefined): HeroKind {
  if (!hero) return "empty";
  return recentReadingProgress(hero).hasProgress ? "resume" : "cold";
}

/** The three rendered regions of the board. */
export interface RecentPartition<T extends RecentLike> {
  hero: T | null;
  pinned: T[];
  others: T[];
}

/**
 * Split a recents list into hero / pinned / others, mirroring the
 * component's render regions: `pinned` is every pinned file (the hero
 * may also appear here, by design — it gets its own strip card), and
 * `others` is everything that is neither the hero nor pinned. Input is
 * never mutated; nullish entries are dropped. A null list -> all empty.
 */
export function partitionRecents<T extends RecentLike>(files: readonly T[]): RecentPartition<T> {
  const hero = chooseContinueCandidate(files);
  const list = Array.isArray(files) ? files.filter(Boolean) : [];
  const heroPath = hero?.path;
  const pinned = list.filter((r) => r.pinned);
  const pinnedPaths = new Set(pinned.map((p) => p.path));
  const others = list.filter((r) => r.path !== heroPath && !pinnedPaths.has(r.path));
  return { hero, pinned, others };
}

// =====================================================================
// Slice 2: filter-as-you-type
// =====================================================================
//
// A user near the 12-file recents cap had no way to jump to one file —
// the grid was eyeball-only. This adds palette-grade filtering: a row
// survives on a fuzzy basename hit (the common case, with a live <mark>
// highlight) OR silently on its full path (so "downloads" surfaces
// everything from that folder). Reuses `scorePaletteField` so ranking
// and highlight behave EXACTLY like Cmd+K and the library search panel.

/**
 * Whether a recent file matches a filter query. Empty query matches
 * everything. Matches on the basename or the full path (folder search).
 * Pure; tolerant of missing fields.
 */
export function recentMatches(file: RecentLike, query: string): boolean {
  const q = (query ?? "").trim();
  if (!q) return true;
  if (!file) return false;
  if (scorePaletteField(q, file.name ?? "").score > 0) return true;
  if (scorePaletteField(q, file.path ?? "").score > 0) return true;
  return false;
}

/**
 * Filter a recents list to the rows matching `query`, preserving the
 * input order (the SORT step stays authoritative — filtering only
 * drops non-matches, it never re-ranks). Empty query returns the list
 * unchanged (sans nullish entries). A null list -> [].
 */
export function filterRecents<T extends RecentLike>(files: readonly T[], query: string): T[] {
  const list = Array.isArray(files) ? files.filter(Boolean) : [];
  const q = (query ?? "").trim();
  if (!q) return list;
  return list.filter((f) => recentMatches(f, q));
}

/**
 * Split a file's display name into hit / non-hit segments for a real
 * <mark> render (no `{@html}`), highlighting only where the NAME itself
 * matched — a path-only (folder) match leaves the name unhighlighted.
 * Empty query / name -> a single non-hit segment so the caller can
 * always `{#each}` uniformly.
 */
export function highlightRecentName(name: string, query: string): HighlightSegment[] {
  const text = name ?? "";
  const q = (query ?? "").trim();
  if (!q || !text) return [{ text, hit: false }];
  const { ranges } = scorePaletteField(q, text);
  if (ranges.length === 0) return [{ text, hit: false }];
  return splitHighlight(text, ranges);
}

// =====================================================================
// Slice 3: sort modes
// =====================================================================
//
// Recents only ever sorted newest-first. This adds a four-mode sort
// over the rendered grid — Recent (openedAt desc, the default), Name
// (A->Z, numeric-aware), Progress (furthest-read first), Pages (biggest
// first) — each returning a NEW array with a stable arrival tie-break so
// equal rows never jitter between renders.

/** A sort mode for the recents grid. */
export type RecentSortMode = "recent" | "name" | "progress" | "pages";

/** Every sort mode, in display order (drives the segmented control). */
export const RECENT_SORT_MODES: readonly RecentSortMode[] = ["recent", "name", "progress", "pages"];

/** Short label for a sort mode (segmented-control button text). */
export function recentSortLabel(mode: RecentSortMode): string {
  switch (mode) {
    case "recent":
      return "Recent";
    case "name":
      return "Name";
    case "progress":
      return "Progress";
    case "pages":
      return "Pages";
    default:
      return String(mode);
  }
}

/** Longer label for the footer / aria ("sorted by …"). */
export function describeRecentSort(mode: RecentSortMode): string {
  switch (mode) {
    case "recent":
      return "last opened";
    case "name":
      return "file name";
    case "progress":
      return "reading progress";
    case "pages":
      return "page count";
    default:
      return String(mode);
  }
}

/** Advance to the next sort mode, wrapping. Lets a chord cycle the sort. */
export function cycleRecentSort(current: RecentSortMode): RecentSortMode {
  const i = RECENT_SORT_MODES.indexOf(current);
  if (i < 0) return RECENT_SORT_MODES[0];
  return RECENT_SORT_MODES[(i + 1) % RECENT_SORT_MODES.length];
}

/**
 * Sort a recents list by the given mode, returning a NEW array (input
 * never mutated). Every non-default comparison falls back to arrival
 * index as a stable tie-break. "name" compares case-insensitively +
 * numeric-aware (file2 < file10); "progress" puts the furthest-read
 * file first (files with no progress sink to arrival order at the end);
 * "pages" is biggest-first. A null list -> [].
 */
export function sortRecentView<T extends RecentLike>(files: readonly T[], mode: RecentSortMode): T[] {
  if (!Array.isArray(files)) return [];
  const indexed = files.filter(Boolean).map((f, i) => ({ f, i }));
  indexed.sort((a, b) => {
    let primary = 0;
    switch (mode) {
      case "name":
        primary = (a.f.name ?? "").localeCompare(b.f.name ?? "", undefined, {
          sensitivity: "base",
          numeric: true,
        });
        break;
      case "progress":
        primary = recentReadingProgress(b.f).fraction - recentReadingProgress(a.f).fraction;
        break;
      case "pages":
        primary = pageCountOf(b.f) - pageCountOf(a.f);
        break;
      case "recent":
      default:
        primary = b.f.openedAt - a.f.openedAt;
        break;
    }
    if (primary !== 0) return primary;
    return a.i - b.i;
  });
  return indexed.map((x) => x.f);
}

// =====================================================================
// Slice 4: keyboard navigation
// =====================================================================
//
// The board was mouse-only. This adds Raycast-grade keyboard control: a
// flat cursor across the pinned + others cards (arrows wrap, Home/End
// leap, PageUp/Down page — the tested palette nav core), Enter to open
// the focused card, P to pin/unpin it, Backspace/Delete to remove it,
// Escape to clear. The hero keeps its own ⌘0 affordance, so the cursor
// space is exactly the rendered card grid.

/** One flat cursor slot over the rendered cards. */
export interface RecentFlatRow<T extends RecentLike> {
  file: T;
  section: "pinned" | "others";
  /** Flat cursor index across both sections. */
  index: number;
}

/**
 * Flatten the pinned strip and others grid into one cursor index space,
 * pinned first (matching render order), each row tagged with its
 * section so the component can resolve the cursor back to the right DOM
 * node. Nullish entries dropped. Pure.
 */
export function flattenRecentCards<T extends RecentLike>(
  pinned: readonly T[],
  others: readonly T[],
): RecentFlatRow<T>[] {
  const out: RecentFlatRow<T>[] = [];
  for (const f of (Array.isArray(pinned) ? pinned : []).filter(Boolean)) {
    out.push({ file: f, section: "pinned", index: out.length });
  }
  for (const f of (Array.isArray(others) ? others : []).filter(Boolean)) {
    out.push({ file: f, section: "others", index: out.length });
  }
  return out;
}

/** What a keypress over the card grid should do. */
export type RecentCardAction =
  | { kind: "move"; intent: PaletteNavIntent }
  | { kind: "open" }
  | { kind: "pin" }
  | { kind: "remove" }
  | { kind: "clear" }
  | null;

/** Minimal keyboard-event shape the classifier reads. */
export interface RecentKeyEvent {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

/**
 * Classify a bare keypress over the card grid into an action, or null
 * if it isn't ours (so app chords win). Any Cmd/Ctrl/Alt disqualifies —
 * the board owns no modifier chords, so ⌘0 / ⌘O / ⌘K all fall through.
 * Arrow/Home/End/PageUp/PageDown -> move (palette nav core); Enter ->
 * open; P -> pin; Backspace/Delete -> remove; Escape -> clear.
 *
 * Left/Right map to prev/next just like Up/Down: the rendered board is a
 * horizontal pinned STRIP followed by a grid, so a single flat cursor
 * reads most naturally when EITHER axis advances it — a user walking the
 * pinned strip presses Right, a user in the grid presses Down, and both
 * land on the next card. (ArrowLeft/Right aren't part of the shared
 * `classifyPaletteNav` vocabulary — the palette list is strictly
 * vertical — so they're mapped here, local to the board.)
 */
export function classifyRecentKey(ev: RecentKeyEvent): RecentCardAction {
  if (!ev || typeof ev.key !== "string") return null;
  if (ev.metaKey || ev.ctrlKey || ev.altKey) return null;
  const nav = classifyPaletteNav(ev);
  if (nav) return { kind: "move", intent: nav };
  switch (ev.key) {
    case "ArrowRight":
      return { kind: "move", intent: "next" };
    case "ArrowLeft":
      return { kind: "move", intent: "prev" };
    case "Enter":
      return { kind: "open" };
    case "p":
    case "P":
      return { kind: "pin" };
    case "Backspace":
    case "Delete":
      return { kind: "remove" };
    case "Escape":
      return { kind: "clear" };
    default:
      return null;
  }
}

/** Where a focused card should sit after a scroll-into-view, per axis. */
export interface RecentScrollOptions {
  block: "nearest" | "center";
  inline: "nearest" | "center";
}

/**
 * The `scrollIntoView` alignment for a focused card, tuned to its
 * section. A pinned card lives in a horizontally-OVERFLOWING strip, so
 * the focus walk needs to scroll it along the INLINE (horizontal) axis —
 * centered, so the next/previous card peeks into view as you arrow
 * across a long strip. A grid card scrolls along the BLOCK (vertical)
 * axis, nearest, like any list. Returning a structural object (not a
 * DOM call) keeps this pure + unit-testable. An unknown section falls
 * back to the safe nearest/nearest. Pure.
 */
export function recentCardScrollOptions(section: "pinned" | "others" | string): RecentScrollOptions {
  if (section === "pinned") return { block: "nearest", inline: "center" };
  return { block: "nearest", inline: "nearest" };
}

/**
 * Resolve the next cursor index for a nav intent. Like `nextPaletteIndex`
 * but seeds an UNSELECTED cursor (-1): the first forward press lands on
 * the first card, the first backward press on the last — so a user who
 * just typed into the filter and presses Down gets card 0, not card 1.
 * Empty list -> -1 (nothing to focus).
 */
export function moveRecentCursor(intent: PaletteNavIntent, current: number, count: number): number {
  if (!Number.isFinite(count) || count <= 0) return -1;
  if (current < 0 || !Number.isFinite(current)) {
    if (intent === "prev" || intent === "last" || intent === "page-up") return count - 1;
    return 0;
  }
  return nextPaletteIndex(intent, current, count);
}

/**
 * Snap a possibly-stale cursor back into range after the list shrinks
 * (a filter narrowed it, a file was removed). Returns -1 (unselected)
 * for an empty list or a negative cursor, else clamps into [0, count-1].
 */
export function clampRecentCursor(cursor: number, count: number): number {
  if (!Number.isFinite(count) || count <= 0) return -1;
  if (!Number.isFinite(cursor) || cursor < 0) return -1;
  return Math.min(Math.floor(cursor), count - 1);
}

// =====================================================================
// Slice 5: summary footer
// =====================================================================
//
// The board gave no running sense of what you were looking at. This
// adds a context-aware footer that narrates the live view (shown-vs-
// total, the filter term, the in-progress count, the active sort) in an
// aria-live region, mirroring the command-palette / library-search /
// beacon-cache footers.

/** The live state the recents footer narrates. */
export interface RecentSummaryState {
  /** Total recents before filtering. */
  total: number;
  /** Rows shown after filtering. */
  shown: number;
  /** Active filter query (trimmed by the helper). */
  query: string;
  /** How many shown rows are mid-read (0 < fraction < 1). */
  inProgress: number;
  /** Active sort mode. */
  sort: RecentSortMode;
}

/**
 * Count how many of the given files are mid-read (started but not
 * finished). Pure; reuses the palette progress core so the threshold
 * matches the chip exactly.
 */
export function countInProgress(files: readonly RecentLike[]): number {
  if (!Array.isArray(files)) return 0;
  let n = 0;
  for (const f of files) {
    if (!f) continue;
    const p = recentReadingProgress(f);
    if (p.hasProgress && !p.finished && p.fraction > 0) n++;
  }
  return n;
}

/**
 * Narrate the live recents view. With no recents -> a friendly empty
 * line; with a filter -> "N of M documents matching "q""; else the
 * total. Appends "· K in progress" and "· by <sort>" (when not the
 * default Recent) so the footer always reflects the visible state.
 * Pure; tolerant of garbage numbers.
 */
export function summarizeRecents(state: RecentSummaryState): string {
  const total = Math.max(0, Math.floor(Number.isFinite(state?.total) ? state.total : 0));
  if (total === 0) return "No recent documents yet";
  const shown = Math.max(0, Math.floor(Number.isFinite(state?.shown) ? state.shown : 0));
  const q = (state?.query ?? "").trim();
  const parts: string[] = [];
  if (q) {
    parts.push(`${shown} of ${total} ${plural(total, "document")} matching \u201c${q}\u201d`);
  } else {
    parts.push(`${total} ${plural(total, "document")}`);
  }
  const ip = Math.max(0, Math.floor(Number.isFinite(state?.inProgress) ? state.inProgress : 0));
  if (ip > 0) parts.push(`${ip} in progress`);
  if (state?.sort && state.sort !== "recent") parts.push(`by ${describeRecentSort(state.sort)}`);
  return parts.join(" \u00b7 ");
}

// --- Slice 6: clear-unpinned affordance ------------------------------
//
// The board accumulates recents up to the store cap; the only way to
// tidy it was to remove rows one at a time. The store already has a
// "clear unpinned" primitive (recent.clearRecent preserves pinned), but
// the board never surfaced it. This counts the unpinned rows so the
// footer can offer a single "Clear N unpinned" action with an honest
// count — and hide it entirely when there's nothing to clear.

/**
 * Count how many of the given recents are NOT pinned — the rows a
 * "clear unpinned" action would remove. Pinned rows (and a missing
 * `pinned` flag, treated as unpinned) are classified the same way the
 * store's clearRecent does. A null/garbage list -> 0.
 */
export function countUnpinned(files: readonly RecentLike[]): number {
  if (!Array.isArray(files)) return 0;
  let n = 0;
  for (const f of files) {
    if (!f) continue;
    if (!f.pinned) n++;
  }
  return n;
}

/**
 * Compose the "clear unpinned" affordance label for the footer, e.g.
 * "Clear 9 unpinned". Returns "" when there is nothing to clear (every
 * row is pinned, or the list is empty) so the component can hide the
 * button. Pure.
 */
export function describeClearUnpinned(count: number): string {
  const n = Math.max(0, Math.floor(Number.isFinite(count) ? count : 0));
  if (n <= 0) return "";
  return `Clear ${n} unpinned`;
}

// --- Slice 7: thumbnail reading-progress overlay ---------------------
//
// The recents cards showed reading position only as a row of dots below
// the title — easy to miss, and the hero/pinned thumbnails showed
// nothing at all. This derives a compact progress-bar model from the
// SAME tested `recentReadingProgress` core the chip uses, so a thin
// accent bar can sit along the bottom edge of any card's thumbnail
// (finished cards read as a full bar with a distinct tint). One source
// of truth: the bar, the dots, and the chip can never disagree.

/** The thumbnail progress-overlay model for one recent card. */
export interface RecentProgressBar {
  /** Whether to render the bar at all (there's a usable position). */
  show: boolean;
  /** Fill percent in [0, 100]. */
  percent: number;
  /** True once the final page was reached (drives the "done" tint). */
  finished: boolean;
  /** aria-label / tooltip text ("p.12/80 · 15%" or "Finished"), "" when hidden. */
  label: string;
}

const HIDDEN_PROGRESS_BAR: RecentProgressBar = {
  show: false,
  percent: 0,
  finished: false,
  label: "",
};

/**
 * Derive the thumbnail progress-bar model for a recent file. Reuses the
 * tested `recentReadingProgress` so the bar's fill + finished state match
 * the dots and the palette chip exactly. A file with no usable last-page
 * + total yields a hidden bar (show:false). Pure; tolerant of garbage.
 */
export function recentProgressBar(file: RecentLike | null | undefined): RecentProgressBar {
  if (!file) return HIDDEN_PROGRESS_BAR;
  const p = recentReadingProgress(file);
  if (!p.hasProgress) return HIDDEN_PROGRESS_BAR;
  return { show: true, percent: p.percent, finished: p.finished, label: p.label };
}

// --- Pinned-strip overflow affordance ---------------------------------
//
// Round 47 made the pinned strip horizontally scrollable (ArrowLeft/Right
// + recentCardScrollOptions). But a strip that overflows off the right
// edge gave NO hint there was more — no fade, no chevron — so a user with
// 12 pinned docs never knew six were hidden. This pure helper turns the
// strip's live scroll geometry into a render model: whether it overflows
// at all, and which edges still have content beyond them, so the
// component can paint gradient masks + enable/disable the scroll chevrons.

/** Scroll geometry of a horizontally-scrolling strip. */
export interface StripScroll {
  /** Current horizontal scroll offset (px from the left). */
  scrollLeft: number;
  /** Total scrollable content width (px). */
  scrollWidth: number;
  /** Visible width of the strip viewport (px). */
  clientWidth: number;
}

/** Which edges of an overflowing strip still have hidden content. */
export interface StripEdges {
  /** True when the content is wider than the viewport (scroll is possible). */
  overflowing: boolean;
  /** True when there is hidden content to the LEFT (scrolled away from start). */
  atStart: boolean;
  /** True when there is hidden content to the RIGHT (more to scroll into). */
  atEnd: boolean;
}

/**
 * Derive the overflow-affordance model for a horizontally-scrolling strip
 * from its live scroll geometry. `atStart` / `atEnd` report whether the
 * LEADING / TRAILING edge still hides content (so the component shows a
 * fade mask + enables a chevron on that side). A 1px tolerance absorbs
 * sub-pixel rounding (a strip scrolled to the very end often reports
 * scrollLeft a fraction short of scrollWidth-clientWidth). A strip that
 * fits entirely is `overflowing:false` with both edges false. Pure;
 * tolerant of garbage / negative numbers.
 */
export function pinnedStripEdges(scroll: StripScroll | null | undefined): StripEdges {
  const none: StripEdges = { overflowing: false, atStart: false, atEnd: false };
  if (!scroll) return none;
  const sw = Number.isFinite(scroll.scrollWidth) ? Math.max(0, scroll.scrollWidth) : 0;
  const cw = Number.isFinite(scroll.clientWidth) ? Math.max(0, scroll.clientWidth) : 0;
  const maxScroll = sw - cw;
  // Tolerance for sub-pixel rounding so the trailing fade vanishes cleanly
  // at the true end instead of lingering a fraction of a pixel short.
  const EPS = 1;
  if (maxScroll <= EPS) return none;
  const sl = Number.isFinite(scroll.scrollLeft)
    ? Math.min(maxScroll, Math.max(0, scroll.scrollLeft))
    : 0;
  return {
    overflowing: true,
    atStart: sl > EPS,
    atEnd: sl < maxScroll - EPS,
  };
}

// --- Slice 8: drag-to-reorder the pinned strip ------------------------
//
// The pinned strip rendered in the store's pinned-first/openedAt-desc
// order with no way to arrange it — your most-used pin could sit third
// because you opened it least recently. This adds a user-defined order:
// orderPinnedStrip honours a per-card `pinOrder` stamp (set by dragging),
// and movePinned computes the new path order after a drag or an Alt+Arrow
// keyboard move. Both are pure; the store's reorderPinned persists the
// result, and crucially the GLOBAL store sort still ignores pinOrder so
// reordering the strip can't perturb the hero / palette / eviction.

/**
 * Order the pinned cards for the strip. Cards carrying a finite `pinOrder`
 * lead, ascending by that stamp; cards without one keep their incoming
 * (store-sorted) order and follow. Every comparison falls back to arrival
 * index so the sort is stable. Returns a NEW array (input never mutated).
 * A null/garbage list -> []. Pure + DOM-free.
 */
export function orderPinnedStrip<T extends RecentLike>(pinned: readonly T[]): T[] {
  if (!Array.isArray(pinned)) return [];
  const hasOrder = (f: T): boolean =>
    typeof f.pinOrder === "number" && Number.isFinite(f.pinOrder);
  const indexed = pinned.filter(Boolean).map((f, i) => ({ f, i }));
  indexed.sort((a, b) => {
    const aHas = hasOrder(a.f);
    const bHas = hasOrder(b.f);
    if (aHas && bHas) {
      const d = (a.f.pinOrder as number) - (b.f.pinOrder as number);
      if (d !== 0) return d;
    } else if (aHas !== bHas) {
      return aHas ? -1 : 1; // stamped cards lead unstamped ones
    }
    return a.i - b.i; // stable arrival tie-break
  });
  return indexed.map((x) => x.f);
}

/**
 * Compute the new left-to-right path order after moving the card at
 * `from` to index `to` within the CURRENT strip order. Returns the
 * reordered list of paths (ready for the store's reorderPinned). Indices
 * are clamped into range; an out-of-range / no-op move returns the paths
 * unchanged. A null/garbage list -> []. Pure.
 */
export function movePinned<T extends RecentLike>(
  ordered: readonly T[],
  from: number,
  to: number,
): string[] {
  if (!Array.isArray(ordered)) return [];
  const paths = ordered.filter(Boolean).map((f) => f.path);
  const n = paths.length;
  if (n === 0) return [];
  const f = Math.floor(Number.isFinite(from) ? from : -1);
  const t = Math.max(0, Math.min(n - 1, Math.floor(Number.isFinite(to) ? to : 0)));
  if (f < 0 || f >= n || f === t) return paths;
  const next = paths.slice();
  const [moved] = next.splice(f, 1);
  next.splice(t, 0, moved);
  return next;
}

// --- Slice 9: reset the pinned-strip order ----------------------------
//
// Once a user has dragged the strip into a custom arrangement, the only
// way back to the store's natural pinned-first/openedAt-desc order was to
// re-drag every card. `anyPinOrder` detects whether ANY card carries a
// drag stamp (so the reset affordance only appears once an order exists),
// and the store's clearPinOrder strips every stamp so orderPinnedStrip
// falls straight back to arrival order. Pure complement to the drag core.

/**
 * Whether any pinned card carries a finite `pinOrder` stamp — i.e. the
 * strip has been manually reordered, so a "reset order" affordance is
 * meaningful. False for an empty/garbage list or a strip with no stamps.
 * Pure + DOM-free.
 */
export function anyPinOrder(pinned: readonly RecentLike[]): boolean {
  if (!Array.isArray(pinned)) return false;
  for (const f of pinned) {
    if (f && typeof f.pinOrder === "number" && Number.isFinite(f.pinOrder)) return true;
  }
  return false;
}

/**
 * Compose the "reset pin order" affordance label from the pinned count,
 * e.g. "Reset order (4)". Returns "" when there is nothing to reset
 * (fewer than 2 cards — a single card has no order to undo), so the
 * component hides the control. Pure (locale grouping via toLocaleString).
 */
export function describeResetPinOrder(pinnedCount: number): string {
  const n = Math.max(0, Math.floor(Number.isFinite(pinnedCount) ? pinnedCount : 0));
  if (n < 2) return "";
  return `Reset order (${n.toLocaleString()})`;
}
