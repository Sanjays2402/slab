// Tests for the Convert pdf2img preview-grid core.
//
// Style matches convertReorder.test.ts — inline expect, no test runner.
//
// Run with:
//   tsx src/lib/convertPreview.test.ts

import {
  selectPreviewPages,
  isPreviewSampled,
  describePreview,
  PREVIEW_CAP,
} from "./convertPreview";

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

const seq = (n: number, start = 1): number[] => Array.from({ length: n }, (_, i) => i + start);

// --- selectPreviewPages ----------------------------------------------
{
  // empty / garbage
  expect(selectPreviewPages([]).length === 0, "select: empty -> []");
  expect(selectPreviewPages(null as unknown as number[]).length === 0, "select: null -> []");
  // under the cap returns everything, in order, as a copy
  const five = seq(5);
  const out5 = selectPreviewPages(five);
  expect(JSON.stringify(out5) === JSON.stringify([1, 2, 3, 4, 5]), "select: under cap -> all");
  expect(out5 !== five, "select: returns a new array (copy)");
  // exactly the cap -> all
  const atCap = seq(PREVIEW_CAP);
  expect(selectPreviewPages(atCap).length === PREVIEW_CAP, "select: exactly cap -> all");
  // over the cap -> sampled down to cap, endpoints included
  const big = seq(500);
  const sampled = selectPreviewPages(big);
  expect(sampled.length <= PREVIEW_CAP, "select: over cap -> <= cap");
  expect(sampled.length >= PREVIEW_CAP - 1, "select: over cap -> near cap (allowing dedupe)");
  expect(sampled[0] === 1, "select: sample includes first page");
  expect(sampled[sampled.length - 1] === 500, "select: sample includes last page");
  // sample is strictly ascending (spread preserves order)
  let asc = true;
  for (let i = 1; i < sampled.length; i++) if (sampled[i] <= sampled[i - 1]) asc = false;
  expect(asc, "select: sample is strictly ascending");
  // custom small cap
  const cap3 = selectPreviewPages(seq(100), 3);
  expect(cap3.length === 3 && cap3[0] === 1 && cap3[2] === 100, "select: cap 3 -> [1, mid, 100]");
  // cap 1 -> just the first
  const cap1 = selectPreviewPages(seq(100), 1);
  expect(cap1.length === 1 && cap1[0] === 1, "select: cap 1 -> first only");
  // honours a non-contiguous range (e.g. pages 3,7,9,12)
  const ranged = selectPreviewPages([3, 7, 9, 12]);
  expect(JSON.stringify(ranged) === JSON.stringify([3, 7, 9, 12]), "select: non-contiguous range preserved under cap");
  // filters out garbage page numbers
  const dirty = selectPreviewPages([1, 0, -3, 2, NaN as number, 3]);
  expect(JSON.stringify(dirty) === JSON.stringify([1, 2, 3]), "select: drops <1 / NaN");
}

// --- isPreviewSampled ------------------------------------------------
{
  expect(isPreviewSampled(500, 24) === true, "sampled: 500>24 -> true");
  expect(isPreviewSampled(24, 24) === false, "sampled: equal -> false");
  expect(isPreviewSampled(10, 10) === false, "sampled: full -> false");
  expect(isPreviewSampled(5, 0) === false, "sampled: nothing shown -> false");
}

// --- describePreview -------------------------------------------------
{
  expect(describePreview(0, 0) === "", "describe: nothing -> empty");
  expect(describePreview(1, 1) === "1 page", "describe: singular");
  expect(describePreview(12, 12) === "12 pages", "describe: full plural");
  expect(describePreview(500, 24) === "Showing 24 of 500 pages", "describe: sampled");
  expect(describePreview(1, 1) === "1 page", "describe: singular full");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
