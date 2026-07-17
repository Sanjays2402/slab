// Pure reorder core for the Convert panel's Images -> PDF list.
//
// The img2pdf list lets a user arrange images into page order. The dropzone
// has always promised "Drag to reorder once added", but the panel only had
// up/down buttons — no actual drag. This module holds the one piece of
// non-trivial logic a drag needs: moving an item from one index to another
// while keeping every other item's relative order intact. Keeping it pure +
// DOM-free means the page-order contract is unit-tested without a browser,
// same discipline as beaconCacheView / readerThumbView / ocrQueueView.

/**
 * Move the element at `from` to position `to`, returning a NEW array; the
 * input is never mutated. Out-of-range / equal indices, or a non-array,
 * return a shallow copy unchanged (so a no-op drag is harmless). The `to`
 * index is interpreted as the destination slot in the ORIGINAL array (a
 * standard splice-remove-then-insert), so dragging row 0 onto row 2 lands
 * it after the item that was at row 2's predecessor — matching how the
 * file rows visually settle. Pure.
 */
export function moveItem<T>(arr: readonly T[], from: number, to: number): T[] {
  if (!Array.isArray(arr)) return [];
  const n = arr.length;
  const f = Math.trunc(Number(from));
  const t = Math.trunc(Number(to));
  const copy = arr.slice();
  if (
    !Number.isFinite(f) ||
    !Number.isFinite(t) ||
    f < 0 ||
    f >= n ||
    t < 0 ||
    t >= n ||
    f === t
  ) {
    return copy;
  }
  const [moved] = copy.splice(f, 1);
  copy.splice(t, 0, moved);
  return copy;
}

/**
 * Whether a drag from `from` to `to` would actually change the order — used
 * to skip persisting / re-rendering on a no-op drop (dropping a row on
 * itself, or an out-of-range index). Pure.
 */
export function isReorder(length: number, from: number, to: number): boolean {
  const n = Math.trunc(Number(length));
  const f = Math.trunc(Number(from));
  const t = Math.trunc(Number(to));
  if (!Number.isFinite(n) || n <= 1) return false;
  if (![f, t].every((x) => Number.isFinite(x) && x >= 0 && x < n)) return false;
  return f !== t;
}
