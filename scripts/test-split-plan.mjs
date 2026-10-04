import assert from "node:assert/strict";
import { splitPlan } from "../src/lib/splitPlan.ts";

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
console.log("Split plan: 5,050 balanced divisions, chunk/range cases, and invalid inputs passed.");
