import assert from "node:assert/strict";
import { splitPlan, oddEvenSplitPlan } from "../src/lib/splitPlan.ts";

// Every division must preserve all pages exactly once, in order, in precisely
// the requested number of files. Uneven divisions differ by at most one page.
for (let total = 1; total <= 100; total++) {
  for (let parts = 1; parts <= total; parts++) {
    const ranges = splitPlan(total, "parts", "", 1, parts);
    assert.equal(ranges.length, parts);
    const pages = ranges.flatMap(r => Array.from({ length: r.end - r.start + 1 }, (_, i) => r.start + i));
    assert.deepEqual(pages, Array.from({ length: total }, (_, i) => i + 1));
    const sizes = ranges.map(r => r.end - r.start + 1);
    assert.ok(Math.max(...sizes) - Math.min(...sizes) <= 1);
  }
}
assert.deepEqual(splitPlan(10, "parts", "", 1, 3), [{ start: 1, end: 4 }, { start: 5, end: 7 }, { start: 8, end: 10 }]);
assert.deepEqual(splitPlan(5, "every", "", 2, 1), [{ start: 1, end: 2 }, { start: 3, end: 4 }, { start: 5, end: 5 }]);
assert.deepEqual(splitPlan(5, "every", "", 9, 1), [{ start: 1, end: 5 }]);
assert.deepEqual(splitPlan(9, "ranges", "7-9, 1-3, 5", 1, 1), [{ start: 7, end: 9 }, { start: 1, end: 3 }, { start: 5, end: 5 }]);
for (const value of [0, -1, 1.5, 11, NaN, Infinity, undefined])
  assert.throws(() => splitPlan(10, "parts", "", 1, value));
for (const value of [0, -1, 1.5, NaN, Infinity, undefined])
  assert.throws(() => splitPlan(10, "every", "", value, 1));
for (const text of ["", "1,", ",1", "1,,2", "0", "2-1", "1-11", "a", "1-3,3-4", "1,1", "999999999999999999999"])
  assert.throws(() => splitPlan(10, "ranges", text, 1, 1));
for (const total of [null, 0, -1, 1.5, NaN, Infinity, 0x100000000])
  assert.throws(() => splitPlan(total, "parts", "", 1, 1));
for (let total = 1; total <= 100; total++) {
  const groups = oddEvenSplitPlan(total);
  assert.equal(groups.length, total === 1 ? 1 : 2);
  assert.equal(groups[0].parity, "odd");
  assert.equal(groups[0].count, Math.ceil(total / 2));
  if (total > 1) {
    assert.equal(groups[1].parity, "even");
    assert.equal(groups[1].count, Math.floor(total / 2));
  }
  assert.equal(groups.reduce((n, g) => n + g.count, 0), total);
}
assert.deepEqual(oddEvenSplitPlan(1), [{ parity: "odd", count: 1, preview: "1" }]);
assert.deepEqual(oddEvenSplitPlan(6), [
  { parity: "odd", count: 3, preview: "1, 3, 5" },
  { parity: "even", count: 3, preview: "2, 4, 6" },
]);
assert.deepEqual(oddEvenSplitPlan(7), [
  { parity: "odd", count: 4, preview: "1, 3, …, 7" },
  { parity: "even", count: 3, preview: "2, 4, 6" },
]);
assert.equal(oddEvenSplitPlan(0xffffffff)[0].preview, "1, 3, …, 4294967295");
for (const total of [null, 0, -1, 1.5, NaN, Infinity, 0x100000000])
  assert.throws(() => oddEvenSplitPlan(total));
console.log("Odd/even previews: counts, short/long page lists, one-page input, and invalid inputs passed.");
console.log("Split plan: 5,050 balanced divisions, chunk/range cases, and invalid inputs passed.");
