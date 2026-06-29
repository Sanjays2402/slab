// Pure view-core for the Reader thumbnail rail's hover-zoom preview.
//
// The rail shows tiny page thumbnails; on hover a larger preview pops out
// beside the rail so you can read the page before clicking. The only
// non-trivial logic is keeping that flyout fully on screen: anchored to the
// hovered thumb's centre but clamped so its top/bottom never spill past the
// viewport. That math is a pure function so it's unit-tested without a DOM —
// same discipline as beaconCacheView / ocrQueueView / librarySearchView.

/** Rect the flyout anchors to (the hovered thumbnail), viewport-relative. */
export interface ThumbRect {
  top: number;
  height: number;
}

/**
 * Vertical (top) position for a hover-preview flyout of `flyoutH` pixels,
 * anchored centred on a thumb at `rect`, clamped into [pad, viewportH - pad]
 * so it never spills past the top/bottom edge. A flyout taller than the
 * viewport pins to the top pad. Garbage rect/sizes -> pad. Pure + DOM-free.
 */
export function clampFlyoutTop(
  rect: ThumbRect | null | undefined,
  flyoutH: number,
  viewportH: number,
  pad = 8,
): number {
  const p = Math.max(0, pad);
  if (!rect || !Number.isFinite(rect.top) || !Number.isFinite(rect.height)) return p;
  const h = Number.isFinite(flyoutH) && flyoutH > 0 ? flyoutH : 0;
  const vh = Number.isFinite(viewportH) && viewportH > 0 ? viewportH : 0;
  const centre = rect.top + rect.height / 2;
  const ideal = centre - h / 2;
  const maxTop = Math.max(p, vh - h - p);
  if (ideal < p) return p;
  if (ideal > maxTop) return maxTop;
  return ideal;
}

/**
 * Whether a hover-preview is worth showing for `page`: only when the rail
 * is open, a doc with >1 page is loaded, and the page is in range. A
 * single-page doc has nothing the rail can't already show, so suppress it.
 * Pure + DOM-free.
 */
export function shouldShowPreview(
  page: number,
  pageCount: number,
  railOpen: boolean,
): boolean {
  if (!railOpen) return false;
  if (!Number.isFinite(pageCount) || pageCount <= 1) return false;
  if (!Number.isFinite(page) || page < 1 || page > pageCount) return false;
  return true;
}

/** Caption for a preview: "Page 4 of 96" — clamps the page into range. */
export function previewLabel(page: number, pageCount: number): string {
  const n = Number.isFinite(pageCount) && pageCount > 0 ? Math.trunc(pageCount) : 0;
  if (n <= 0) return "";
  const p = Math.min(n, Math.max(1, Math.trunc(Number.isFinite(page) ? page : 1)));
  return `Page ${p} of ${n}`;
}

// --- Keyboard preview navigation --------------------------------------
//
// The hover-zoom preview is mouse-only: it follows the pointer. But the
// rail is keyboard-reachable, so a focused rail should drive the SAME
// flyout with Up/Down (prev/next page) — the keyboard twin of hover.
// Home/End leap to the first/last page; Esc dismisses. Logic is pure so
// it tests without a DOM, same discipline as the rest of this module.

export type ThumbPreviewAction = "prev" | "next" | "first" | "last" | "dismiss" | null;

/** Minimal keyboard event the rail forwards (DOM-free, easy to test). */
export interface ThumbKeyEvent {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

/**
 * Classify a keypress while the thumbnail rail is focused into a preview
 * move: ArrowUp/-> prev, ArrowDown/-> next, Home/End to ends, Escape
 * dismisses. Any modifier disqualifies so app chords (Cmd+F, etc.) win.
 * Other keys -> null. Pure + DOM-free.
 */
export function classifyThumbPreviewKey(ev: ThumbKeyEvent | null | undefined): ThumbPreviewAction {
  if (!ev || ev.metaKey || ev.ctrlKey || ev.altKey || ev.shiftKey) return null;
  switch (ev.key) {
    case "ArrowUp":
      return "prev";
    case "ArrowDown":
      return "next";
    case "Home":
      return "first";
    case "End":
      return "last";
    case "Escape":
      return "dismiss";
    default:
      return null;
  }
}

/**
 * Page a preview lands on after `action` from `page`, clamped to
 * [1, pageCount] so the ends never wrap (matches the rail's own bounds).
 * "first"->1, "last"->count. dismiss/null/no-doc -> current clamped.
 * Garbage -> 1. Pure.
 */
export function nextPreviewPage(page: number, pageCount: number, action: ThumbPreviewAction): number {
  const n = Number.isFinite(pageCount) && pageCount > 0 ? Math.trunc(pageCount) : 0;
  if (n <= 0) return 1;
  const cur = Math.min(n, Math.max(1, Math.trunc(Number.isFinite(page) ? page : 1)));
  switch (action) {
    case "prev":
      return Math.max(1, cur - 1);
    case "next":
      return Math.min(n, cur + 1);
    case "first":
      return 1;
    case "last":
      return n;
    default:
      return cur;
  }
}
