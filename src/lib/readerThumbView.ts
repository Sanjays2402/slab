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
