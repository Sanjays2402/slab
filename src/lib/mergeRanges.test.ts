import assert from "node:assert/strict";
import { parseMergeRanges } from "./mergeRanges";

assert.deepEqual(parseMergeRanges("  "), []);
assert.deepEqual(parseMergeRanges("1-3, 5, 9 - 7"), [
  { start: 1, end: 3 }, { start: 5, end: 5 }, { start: 7, end: 9 },
]);
for (const input of ["1oops-3", "1-3oops", "1.5-3", "0", "0-3", ",,", "1,", "1,,2", "-1", "1-2-3", "NaN", "Infinity", "4294967296", "9007199254740993"]) {
  assert.throws(() => parseMergeRanges(input), Error, input);
}
assert.deepEqual(parseMergeRanges("4294967295"), [{ start: 4294967295, end: 4294967295 }]);
