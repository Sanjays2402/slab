// Pure core for the Convert panel's PDF -> Images preview grid (round 59).
//
// The pdf2img tab told you only a page COUNT ("12 pages") before you
// exported — you couldn't see WHAT you were about to turn into images, so a
// wrong file or wrong range wasn't caught until after the ZIP landed. This
// backs a thumbnail preview grid. The one piece of non-trivial logic is
// deciding WHICH pages to render: rendering every page of a 500-page book
// would be slow and pointless, so for large selections we render a capped,
// evenly-spread sample and label the gap. Keeping that pure + DOM-free means
// the selection contract is unit-tested without a browser, same discipline
// as convertReorder / readerThumbView / ocrQueueView.

/** Default ceiling on how many page thumbnails to render at once. */
export const PREVIEW_CAP = 24;

/**
 * Given the full list of selected 1-based page numbers, return the subset to
 * actually render thumbnails for. At or under the cap, returns them all (a
 * copy). Over the cap, returns an evenly-spread sample of `cap` pages that
 * always includes the first and last selected page, so the preview spans the
 * whole selection rather than just the front. Input is never mutated; a
 * non-array or empty list -> []. Pure.
 */
export function selectPreviewPages(
  pages: readonly number[],
  cap: number = PREVIEW_CAP,
): number[] {
  if (!Array.isArray(pages) || pages.length === 0) return [];
  const clean = pages.filter((p) => Number.isFinite(p) && p >= 1).map((p) => Math.trunc(p));
  const n = clean.length;
  const c = Number.isFinite(cap) && cap >= 1 ? Math.trunc(cap) : PREVIEW_CAP;
  if (n <= c) return clean.slice();
  if (c === 1) return [clean[0]];
  // Evenly spread `c` indices across [0, n-1], endpoints inclusive.
  const out: number[] = [];
  const seen = new Set<number>();
  for (let i = 0; i < c; i++) {
    const idx = Math.round((i * (n - 1)) / (c - 1));
    if (!seen.has(idx)) {
      seen.add(idx);
      out.push(clean[idx]);
    }
  }
  return out;
}

/**
 * Whether the preview is a sampled subset (more pages selected than rendered)
 * — drives the "showing N of M" note. Pure.
 */
export function isPreviewSampled(totalSelected: number, shown: number): boolean {
  const t = Number.isFinite(totalSelected) ? totalSelected : 0;
  const s = Number.isFinite(shown) ? shown : 0;
  return t > s && s > 0;
}

/**
 * Caption for the preview grid header. Empty selection -> "". A full preview
 * -> "N pages". A sampled preview -> "Showing S of T pages". Singular-aware.
 * Pure.
 */
export function describePreview(totalSelected: number, shown: number): string {
  const t = Number.isFinite(totalSelected) ? Math.max(0, Math.trunc(totalSelected)) : 0;
  const s = Number.isFinite(shown) ? Math.max(0, Math.trunc(shown)) : 0;
  if (t === 0) return "";
  if (isPreviewSampled(t, s)) {
    return `Showing ${s} of ${t} page${t === 1 ? "" : "s"}`;
  }
  return `${t} page${t === 1 ? "" : "s"}`;
}
