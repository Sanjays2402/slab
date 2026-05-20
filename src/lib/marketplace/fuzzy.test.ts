// Smoke tests for the homemade fuzzy matcher used by the Plugin Store
// Browse tab. Not run by `pnpm check` — they're documented as inline
// expectations so future hands can grok the algorithm at a glance.
//
// Run with `node --experimental-vm-modules src/lib/marketplace/fuzzy.test.ts`
// after compiling, or copy into any node REPL. Kept lightweight on
// purpose: the matcher is small enough that adding vitest as a dep
// would cost more than it earns. If the matcher ever grows complex
// enough to need a real test runner, promote this file then.

import { scoreMatch, fuzzyMatchEntry, highlightHTML } from "./fuzzy";

function expect(cond: boolean, label: string): void {
  if (!cond) {
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    process.exitCode = 1;
  } else {
    // eslint-disable-next-line no-console
    console.log("ok  ", label);
  }
}

// --- scoreMatch ---
expect(scoreMatch("", "anything").score === 1, "empty query → sentinel 1");
expect(scoreMatch("foo", "").score === 0, "empty haystack → 0");
expect(scoreMatch("redact", "Redactor").score === 1000, "prefix match scores 1000");
expect(scoreMatch("act", "Redactor").score === 500, "substring match scores 500");
expect(scoreMatch("rdr", "Redactor").score > 0, "fuzzy subsequence matches");
expect(scoreMatch("xyz", "Redactor").score === 0, "no match → 0");

// --- highlightHTML ---
expect(
  highlightHTML("Redactor", [{ start: 0, end: 6 }]) === "<mark>Redact</mark>or",
  "highlight wraps mark tag at correct range"
);
expect(
  highlightHTML("<script>", []) === "&lt;script&gt;",
  "highlight escapes HTML when no ranges"
);

// --- fuzzyMatchEntry ---
const entry = {
  id: "com.example.redactor",
  name: "Redactor",
  description: "Find and redact PII in your PDFs.",
  author: "Slab Maintainers",
  categories: ["Privacy"],
  tags: ["redact", "pii"],
};
const result = fuzzyMatchEntry("redact", entry);
expect(result.score > 0, "fuzzyMatchEntry finds matching entry");
expect(result.fieldRanges.name.length > 0, "name field has match range");
expect(result.fieldRanges.tags.length > 0, "tags field has match range");

const noMatch = fuzzyMatchEntry("xyz123nopechain", entry);
expect(noMatch.score === 0, "non-matching query → 0 total score");
