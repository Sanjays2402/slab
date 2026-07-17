// Tests for the Convert img2pdf reorder core.
//
// Style matches librarySortStore.test.ts / readerThumbView.test.ts — inline
// expect, no test runner.
//
// Run with:
//   tsx src/lib/convertReorder.test.ts

import { moveItem, isReorder } from "./convertReorder";

let passed = 0;
let failed = 0;
function expect(cond: boolean, label: string): void {
  if (cond) {
    passed++;
    // eslint-disable-next-line no-console
    console.log(`ok: ${label}`);
  } else {
    failed++;
    // eslint-disable-next-line no-console
    console.error(`FAIL: ${label}`);
  }
}

const eq = (a: unknown[], b: unknown[]) => JSON.stringify(a) === JSON.stringify(b);

// --- moveItem: basic moves --------------------------------------------
expect(eq(moveItem(["a", "b", "c", "d"], 0, 2), ["b", "c", "a", "d"]), "move: 0 -> 2 (down)");
expect(eq(moveItem(["a", "b", "c", "d"], 3, 1), ["a", "d", "b", "c"]), "move: 3 -> 1 (up)");
expect(eq(moveItem(["a", "b", "c"], 1, 0), ["b", "a", "c"]), "move: middle to front");
expect(eq(moveItem(["a", "b", "c"], 0, 2), ["b", "c", "a"]), "move: front to back");

// --- moveItem: no-ops -------------------------------------------------
expect(eq(moveItem(["a", "b", "c"], 1, 1), ["a", "b", "c"]), "move: same index -> unchanged");
expect(eq(moveItem(["a", "b", "c"], -1, 1), ["a", "b", "c"]), "move: negative from -> unchanged");
expect(eq(moveItem(["a", "b", "c"], 1, 9), ["a", "b", "c"]), "move: out-of-range to -> unchanged");
expect(eq(moveItem(["a", "b", "c"], 9, 0), ["a", "b", "c"]), "move: out-of-range from -> unchanged");
expect(eq(moveItem([], 0, 0), []), "move: empty -> empty");
expect(eq(moveItem(["only"], 0, 0), ["only"]), "move: single element -> unchanged");

// --- moveItem: immutability -------------------------------------------
{
  const src = ["a", "b", "c"];
  const out = moveItem(src, 0, 2);
  expect(eq(src, ["a", "b", "c"]), "move: input not mutated");
  expect(out !== src, "move: returns a new array");
}

// --- moveItem: garbage ------------------------------------------------
expect(eq(moveItem(null as never, 0, 1), []), "move: null -> []");
expect(eq(moveItem(["a", "b"], NaN as number, 1), ["a", "b"]), "move: NaN from -> copy");
expect(eq(moveItem(["a", "b"], 0.7, 1.2), ["b", "a"]), "move: fractional indices truncate");

// --- isReorder --------------------------------------------------------
expect(isReorder(4, 0, 2) === true, "isReorder: real move -> true");
expect(isReorder(4, 2, 2) === false, "isReorder: same index -> false");
expect(isReorder(1, 0, 0) === false, "isReorder: single element -> false");
expect(isReorder(0, 0, 0) === false, "isReorder: empty -> false");
expect(isReorder(3, -1, 1) === false, "isReorder: negative -> false");
expect(isReorder(3, 1, 9) === false, "isReorder: out-of-range -> false");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
