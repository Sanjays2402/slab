// Pure-helper tests for the command-palette search core.
//
// Style matches toastStack.test.ts / hopper.test.ts — no test runner,
// just an inline `expect` so the contract reads at a glance.
//
// Run with:
//   tsx src/lib/paletteSearch.test.ts

import {
  scorePaletteField,
  scorePaletteEntry,
  splitHighlight,
  normalizeRanges,
  type PaletteRange,
} from "./paletteSearch";

let passed = 0;
let failed = 0;

function expect(cond: boolean, label: string): void {
  if (!cond) {
    failed++;
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    passed++;
    // eslint-disable-next-line no-console
    console.log("ok:", label);
  }
}

/** Reconstruct the matched substring(s) from ranges for assertions. */
function pick(text: string, ranges: PaletteRange[]): string {
  return ranges.map((r) => text.slice(r.start, r.end)).join("|");
}

// --- scorePaletteField: empty + no-match sentinels -------------------
{
  expect(scorePaletteField("", "anything").score === 1, "field: empty query -> neutral 1");
  expect(scorePaletteField("", "anything").ranges.length === 0, "field: empty query -> no ranges");
  expect(scorePaletteField("x", "").score === 0, "field: empty haystack -> 0");
  expect(scorePaletteField("zzz", "Reader").score === 0, "field: no match -> 0");
  expect(scorePaletteField("readerx", "Reader").score === 0, "field: query longer than match -> 0");
}

// --- Tier ordering: prefix > substring > subsequence ----------------
{
  const prefix = scorePaletteField("rea", "Reader").score;
  const substring = scorePaletteField("ead", "Reader").score;
  const subseq = scorePaletteField("rdr", "Reader").score;
  expect(prefix > substring, "tier: prefix beats substring");
  expect(substring > subseq, "tier: substring beats subsequence");
  expect(subseq > 0, "tier: subsequence still positive");
}

// --- prefix ranges -------------------------------------------------
{
  const r = scorePaletteField("rea", "Reader");
  expect(r.ranges.length === 1, "prefix: one range");
  expect(pick("Reader", r.ranges) === "Rea", "prefix: range covers the prefix verbatim (orig case)");
  expect(r.ranges[0].start === 0 && r.ranges[0].end === 3, "prefix: [0,3)");
}

// --- substring ranges + boundary bonus ------------------------------
{
  const mid = scorePaletteField("ead", "Reader");
  expect(pick("Reader", mid.ranges) === "ead", "substring: range covers the hit");
  expect(mid.ranges[0].start === 1, "substring: starts at index 1");

  // Boundary substring ("redact" inside "Auto-Redact") should beat a
  // non-boundary substring hit of the same length elsewhere.
  const boundary = scorePaletteField("redact", "Auto-Redact").score;
  const nonBoundary = scorePaletteField("edact", "Auto-Redact").score;
  expect(boundary > nonBoundary, "substring: word-boundary hit scores higher");
}

// --- subsequence: contiguity + ranges -------------------------------
{
  const r = scorePaletteField("cmp", "Compress");
  expect(r.score > 0, "subsequence: c-m-p matches Compress");
  // c(0) m(2) p(3) -> ranges [0,1) and [2,4)
  expect(pick("Compress", r.ranges) === "C|mp", "subsequence: split ranges reconstruct");

  // Tighter subsequence beats a gappier one for the same query.
  const tight = scorePaletteField("com", "Compare").score; // c-o-m contiguous-ish
  const gappy = scorePaletteField("cre", "Compare").score; // c...r...e spread
  expect(tight > gappy, "subsequence: tighter match scores higher");
}

// --- shortness nudge: shorter haystack wins a tie -------------------
{
  const shortHit = scorePaletteField("sign", "Sign").score;
  const longHit = scorePaletteField("sign", "Signet batch").score;
  expect(shortHit > longHit, "nudge: shorter haystack edges out longer on same tier");
  expect(Math.abs(shortHit - longHit) < 1, "nudge: tie-break stays sub-1 (never crosses a tier)");
}

// --- scorePaletteEntry: title vs keyword weighting ------------------
{
  // Title prefix must outrank a keyword-only subsequence.
  const titleHit = scorePaletteEntry("rea", { title: "Reader", keywords: "view pdf" });
  const kwHit = scorePaletteEntry("rea", { title: "Auto-Redact", keywords: "redact area" });
  expect(titleHit.score > kwHit.score, "entry: title prefix beats keyword hit");
  expect(titleHit.titleRanges.length === 1, "entry: title hit yields title ranges");
}

{
  // A keyword-only match still surfaces (score > 0) but highlights nothing
  // in the title (no confusing marks on an unmatched title).
  const kwOnly = scorePaletteEntry("invoice", { title: "Smart Folders Hub", keywords: "invoice tax receipt" });
  expect(kwOnly.score > 0, "entry: keyword-only match still ranks");
  expect(kwOnly.titleRanges.length === 0, "entry: keyword-only match highlights nothing in title");
}

{
  // No match anywhere -> 0, no ranges.
  const miss = scorePaletteEntry("zzzz", { title: "Reader", keywords: "view" });
  expect(miss.score === 0, "entry: total miss -> 0");
  expect(miss.titleRanges.length === 0, "entry: total miss -> no ranges");

  // Missing keywords handled.
  const noKw = scorePaletteEntry("rea", { title: "Reader" });
  expect(noKw.score > 0, "entry: missing keywords field is fine");
}

// --- splitHighlight: segmentation correctness -----------------------
{
  const segs = splitHighlight("Reader", [{ start: 0, end: 3 }]);
  expect(segs.length === 2, "split: prefix -> 2 segments");
  expect(segs[0].text === "Rea" && segs[0].hit === true, "split: first segment is the hit");
  expect(segs[1].text === "der" && segs[1].hit === false, "split: tail is non-hit");
  expect(segs.map((s) => s.text).join("") === "Reader", "split: segments reconstruct input");
}

{
  // Mid-string hit -> 3 segments (before / hit / after).
  const segs = splitHighlight("Auto-Redact", [{ start: 5, end: 11 }]);
  expect(segs.length === 2, "split: hit-at-tail -> 2 segments");
  expect(segs[0].hit === false && segs[1].hit === true, "split: tail hit ordering");
  expect(segs.map((s) => s.text).join("") === "Auto-Redact", "split: tail hit reconstructs");
}

{
  // Multiple ranges -> alternating segments. Ranges mirror the "cmp"
  // subsequence hit on "Compress": C(0) then m(2)p(3) -> [0,1) + [2,4).
  const segs = splitHighlight("Compress", [{ start: 0, end: 1 }, { start: 2, end: 4 }]);
  expect(segs.map((s) => s.text).join("") === "Compress", "split: multi-range reconstructs");
  expect(segs.filter((s) => s.hit).map((s) => s.text).join("|") === "C|mp", "split: two hit segments");
}

{
  // No ranges -> single non-hit segment.
  const segs = splitHighlight("Reader", []);
  expect(segs.length === 1 && segs[0].hit === false, "split: no ranges -> one plain segment");
  expect(segs[0].text === "Reader", "split: no-range segment is whole text");

  // Empty text -> empty list.
  expect(splitHighlight("", [{ start: 0, end: 1 }]).length === 0, "split: empty text -> []");
}

// --- normalizeRanges: clamping, sorting, merging --------------------
{
  // Out-of-bounds clamps.
  const n1 = normalizeRanges([{ start: -5, end: 3 }, { start: 4, end: 99 }], 6);
  expect(n1.length === 2, "normalize: two clamped ranges survive");
  expect(n1[0].start === 0 && n1[0].end === 3, "normalize: negative start clamps to 0");
  expect(n1[1].end === 6, "normalize: oversize end clamps to len");
}

{
  // Overlapping + unsorted merge into one.
  const n = normalizeRanges([{ start: 3, end: 5 }, { start: 0, end: 4 }], 8);
  expect(n.length === 1, "normalize: overlapping ranges merge");
  expect(n[0].start === 0 && n[0].end === 5, "normalize: merged span covers union");
}

{
  // Adjacent ranges (end == next.start) merge into one contiguous span.
  const n = normalizeRanges([{ start: 0, end: 2 }, { start: 2, end: 4 }], 8);
  expect(n.length === 1 && n[0].end === 4, "normalize: adjacent ranges coalesce");
}

{
  // Empty / invalid ranges dropped.
  expect(normalizeRanges([{ start: 3, end: 3 }], 8).length === 0, "normalize: zero-width dropped");
  expect(normalizeRanges([{ start: 5, end: 2 }], 8).length === 0, "normalize: inverted dropped");
  expect(normalizeRanges([], 8).length === 0, "normalize: empty input -> []");
  expect(normalizeRanges([{ start: 0, end: 3 }], 0).length === 0, "normalize: zero length -> []");
  expect(
    normalizeRanges([{ start: NaN, end: 3 }], 8).length === 0,
    "normalize: NaN start dropped",
  );
}

{
  // Fractional indices floored.
  const n = normalizeRanges([{ start: 1.9, end: 4.2 }], 8);
  expect(n.length === 1 && n[0].start === 1 && n[0].end === 4, "normalize: fractional floored");
}

// --- Integration: real palette-like ranking -------------------------
{
  // "comp" against a realistic catalog should rank Compress/Compact/
  // Compare (title prefixes) above an entry that only matches in keywords.
  const catalog = [
    { title: "Compress", keywords: "shrink size reduce" },
    { title: "Reader", keywords: "view pdf open compare" },
    { title: "Compact", keywords: "merge" },
  ];
  const ranked = catalog
    .map((e) => ({ e, s: scorePaletteEntry("comp", e).score }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s);
  expect(ranked.length === 3, "integration: all three match comp (Reader via keyword 'compare')");
  expect(ranked[0].e.title === "Compress" || ranked[0].e.title === "Compact", "integration: a Comp* title ranks first");
  expect(ranked[ranked.length - 1].e.title === "Reader", "integration: keyword-only Reader ranks last");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
