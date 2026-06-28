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
  classifyPaletteNav,
  nextPaletteIndex,
  PALETTE_PAGE_JUMP,
  recencyWeight,
  frecencyScore,
  rankFrecency,
  recordFrecency,
  paletteKeymapId,
  suggestPaletteFallback,
  parsePaletteScope,
  entryMatchesScope,
  describePaletteScope,
  classifyPaletteGroupNav,
  groupStartIndices,
  nextGroupIndex,
  recentReadingProgress,
  describePaletteCount,
  paletteActionVerb,
  toggleCollapsedGroup,
  partitionCollapsedGroups,
  collapseAllGroups,
  isEveryGroupCollapsed,
  describeCollapseState,
  soloExpandGroup,
  isCommandPinned,
  toggleCommandPin,
  countPinnedCommands,
  type PaletteRange,
  type FrecencyRecord,
  type PaletteFallbackEntry,
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

// --- classifyPaletteNav: key -> intent ------------------------------
{
  expect(classifyPaletteNav({ key: "ArrowDown" }) === "next", "nav: ArrowDown -> next");
  expect(classifyPaletteNav({ key: "ArrowUp" }) === "prev", "nav: ArrowUp -> prev");
  expect(classifyPaletteNav({ key: "Home" }) === "first", "nav: Home -> first");
  expect(classifyPaletteNav({ key: "End" }) === "last", "nav: End -> last");
  expect(classifyPaletteNav({ key: "PageUp" }) === "page-up", "nav: PageUp -> page-up");
  expect(classifyPaletteNav({ key: "PageDown" }) === "page-down", "nav: PageDown -> page-down");
  expect(classifyPaletteNav({ key: "Enter" }) === null, "nav: Enter -> null (not nav)");
  expect(classifyPaletteNav({ key: "a" }) === null, "nav: typing -> null");
}

// --- nextPaletteIndex: arrows wrap ----------------------------------
{
  expect(nextPaletteIndex("next", 0, 5) === 1, "nav: next steps forward");
  expect(nextPaletteIndex("next", 4, 5) === 0, "nav: next wraps past end to 0");
  expect(nextPaletteIndex("prev", 3, 5) === 2, "nav: prev steps back");
  expect(nextPaletteIndex("prev", 0, 5) === 4, "nav: prev wraps before start to last");
}

// --- nextPaletteIndex: Home / End jump extremes ---------------------
{
  expect(nextPaletteIndex("first", 3, 5) === 0, "nav: first -> 0");
  expect(nextPaletteIndex("last", 1, 5) === 4, "nav: last -> count-1");
}

// --- nextPaletteIndex: paging clamps (never wraps) ------------------
{
  expect(nextPaletteIndex("page-down", 0, 20) === PALETTE_PAGE_JUMP, "nav: page-down jumps default page");
  expect(nextPaletteIndex("page-down", 18, 20) === 19, "nav: page-down clamps at last (no wrap)");
  expect(nextPaletteIndex("page-up", 19, 20) === 19 - PALETTE_PAGE_JUMP, "nav: page-up jumps back a page");
  expect(nextPaletteIndex("page-up", 2, 20) === 0, "nav: page-up clamps at 0 (no wrap)");
  expect(nextPaletteIndex("page-down", 5, 20, 3) === 8, "nav: custom page size honoured");
}

// --- nextPaletteIndex: defensive guards -----------------------------
{
  expect(nextPaletteIndex("next", 0, 0) === 0, "nav: empty list -> 0");
  expect(nextPaletteIndex("next", 0, 1) === 0, "nav: single item next wraps to itself");
  expect(nextPaletteIndex("prev", 0, 1) === 0, "nav: single item prev wraps to itself");
  // Stale/out-of-range current is clamped before stepping.
  expect(nextPaletteIndex("next", 99, 5) === 0, "nav: stale current past end clamps then wraps");
  expect(nextPaletteIndex("prev", -5, 5) === 4, "nav: negative current clamps to 0 then wraps to last");
  expect(nextPaletteIndex("next", NaN, 5) === 1, "nav: NaN current treated as 0");
  expect(nextPaletteIndex("page-down", 0, NaN) === 0, "nav: NaN count -> 0");
}

// --- Frecency: recencyWeight buckets --------------------------------
{
  const MIN = 60_000;
  const HR = 60 * MIN;
  const DY = 24 * HR;
  expect(recencyWeight(0) === 1000, "recency: just-now -> 1000");
  expect(recencyWeight(2 * MIN) === 1000, "recency: < 5min -> 1000");
  expect(recencyWeight(30 * MIN) === 350, "recency: < 1h -> 350");
  expect(recencyWeight(5 * HR) === 120, "recency: < 1d -> 120");
  expect(recencyWeight(3 * DY) === 50, "recency: < 1w -> 50");
  expect(recencyWeight(10 * DY) === 20, "recency: < 1mo -> 20");
  expect(recencyWeight(200 * DY) === 8, "recency: ancient -> 8 floor");
  expect(recencyWeight(-5) === 1000, "recency: negative age (clock skew) -> 1000");
  expect(recencyWeight(NaN) === 1000, "recency: NaN -> 1000");
  // Monotonic non-increasing with age.
  expect(recencyWeight(MIN) >= recencyWeight(HR), "recency: weight non-increasing with age");
}

// --- Frecency: score blends frequency + recency ---------------------
{
  const now = 1_000_000_000_000;
  const heavyStale: FrecencyRecord = { id: "a", count: 50, lastUsedAt: now - 10 * 24 * 60 * 60_000 }; // 10d
  const lightFresh: FrecencyRecord = { id: "b", count: 1, lastUsedAt: now - 60_000 }; // 1m
  // Recency dominates: a fresh single use beats a stale heavy one.
  expect(frecencyScore(lightFresh, now) > frecencyScore(heavyStale, now), "frecency: recency dominates frequency");

  // Within the same recency bucket, more frequent wins.
  const a: FrecencyRecord = { id: "a", count: 10, lastUsedAt: now - 60_000 };
  const b: FrecencyRecord = { id: "b", count: 2, lastUsedAt: now - 60_000 };
  expect(frecencyScore(a, now) > frecencyScore(b, now), "frecency: frequency breaks recency-tie");

  // Zero / missing count -> 0.
  expect(frecencyScore({ id: "x", count: 0, lastUsedAt: now }, now) === 0, "frecency: count 0 -> 0");
  expect(frecencyScore({ id: "x", count: NaN, lastUsedAt: now }, now) === 0, "frecency: NaN count -> 0");
}

// --- Frecency: log temper means freq never swamps recency -----------
{
  const now = 2_000_000_000_000;
  // A command used 1000x a week ago should still lose to one used once a
  // minute ago, because recency is a bucket multiplier.
  const huge: FrecencyRecord = { id: "huge", count: 1000, lastUsedAt: now - 8 * 24 * 60 * 60_000 };
  const fresh: FrecencyRecord = { id: "fresh", count: 1, lastUsedAt: now - 60_000 };
  expect(frecencyScore(fresh, now) > frecencyScore(huge, now), "frecency: log-tempered freq can't swamp recency");
}

// --- rankFrecency: ordering + tie-breaks ----------------------------
{
  const now = 3_000_000_000_000;
  const recs: FrecencyRecord[] = [
    { id: "stale-heavy", count: 40, lastUsedAt: now - 20 * 24 * 60 * 60_000 },
    { id: "fresh-light", count: 1, lastUsedAt: now - 30_000 },
    { id: "mid", count: 5, lastUsedAt: now - 2 * 60 * 60_000 },
  ];
  const ranks = rankFrecency(recs, now);
  expect(ranks["fresh-light"] === 0, "rank: freshest at rank 0");
  expect(ranks["mid"] === 1, "rank: mid second");
  expect(ranks["stale-heavy"] === 2, "rank: stale-heavy last despite high count");

  // Dropped non-positive counts.
  const filtered = rankFrecency([{ id: "z", count: 0, lastUsedAt: now }], now);
  expect(Object.keys(filtered).length === 0, "rank: zero-count dropped");
  expect(Object.keys(rankFrecency([], now)).length === 0, "rank: empty -> {}");

  // Exact-tie breaks deterministically (same score+time -> id order).
  const tie = rankFrecency(
    [
      { id: "b", count: 3, lastUsedAt: now - 60_000 },
      { id: "a", count: 3, lastUsedAt: now - 60_000 },
    ],
    now,
  );
  expect(tie["a"] === 0 && tie["b"] === 1, "rank: exact tie breaks by id");
}

// --- recordFrecency: bump / insert / cap ----------------------------
{
  const now = 4_000_000_000_000;
  // Insert new.
  const r1 = recordFrecency([], "new", now, 16);
  expect(r1.length === 1 && r1[0].count === 1 && r1[0].lastUsedAt === now, "record: insert new count-1");

  // Bump existing.
  const r2 = recordFrecency([{ id: "x", count: 3, lastUsedAt: now - 10_000 }], "x", now, 16);
  expect(r2.length === 1 && r2[0].count === 4 && r2[0].lastUsedAt === now, "record: bump existing count + ts");

  // Never mutates input.
  const input: FrecencyRecord[] = [{ id: "x", count: 1, lastUsedAt: now - 1 }];
  const before = input[0].count;
  recordFrecency(input, "x", now, 16);
  expect(input[0].count === before, "record: input not mutated");

  // Empty id is a no-op (just capped passthrough).
  const r3 = recordFrecency([{ id: "x", count: 1, lastUsedAt: now }], "", now, 16);
  expect(r3.length === 1 && r3[0].id === "x", "record: empty id no-op");
}

{
  // Capacity eviction keeps the highest-frecency records + always the
  // just-touched one.
  const now = 5_000_000_000_000;
  const DY = 24 * 60 * 60_000;
  const records: FrecencyRecord[] = [
    { id: "keep1", count: 20, lastUsedAt: now - 60_000 },
    { id: "keep2", count: 10, lastUsedAt: now - 2 * 60_000 },
    { id: "evict", count: 1, lastUsedAt: now - 60 * DY },
  ];
  const out = recordFrecency(records, "brand-new", now, 3);
  expect(out.length === 3, "record: capped to limit");
  expect(out.some((r) => r.id === "brand-new"), "record: just-touched record retained");
  expect(!out.some((r) => r.id === "evict"), "record: lowest-frecency evicted");
}

// --- paletteKeymapId: palette row -> keymap action id ---------------
{
  // Known mappings resolve.
  expect(paletteKeymapId("panel:hopper") === "hopper.open", "keymap: panel:hopper -> hopper.open");
  expect(paletteKeymapId("home:open") === "home.open", "keymap: home:open -> home.open");
  expect(paletteKeymapId("home:continue") === "home.continue", "keymap: home:continue -> home.continue");
  expect(paletteKeymapId("library:search") === "library.search", "keymap: library:search -> library.search");
  expect(paletteKeymapId("theater:open") === "theater.start", "keymap: theater:open -> theater.start");
  expect(paletteKeymapId("panel:theater") === "theater.start", "keymap: panel:theater -> theater.start");
  expect(paletteKeymapId("panel:forms:batch") === "quill.batch", "keymap: forms batch sub-tab -> quill.batch");
  expect(paletteKeymapId("forms:tour") === "quill.tour", "keymap: forms:tour -> quill.tour");
  expect(paletteKeymapId("help:shortcuts") === "shortcuts.show", "keymap: help:shortcuts -> shortcuts.show");

  // Unmapped rows (themes, accents, recents, plugin cmds) -> null.
  expect(paletteKeymapId("theme:dark") === null, "keymap: theme row -> null");
  expect(paletteKeymapId("accent:blue") === null, "keymap: accent row -> null");
  expect(paletteKeymapId("recent:/foo.pdf") === null, "keymap: recent file -> null");
  expect(paletteKeymapId("plugin-cmd:x:y") === null, "keymap: plugin cmd -> null");
  expect(paletteKeymapId("") === null, "keymap: empty id -> null");
  expect(paletteKeymapId("panel:reader") === null, "keymap: reader has no global chord -> null");
}

{
  // Every mapped keymap id must be one of the real keymap.ts ActionIds —
  // a typo here would silently render a blank chord. This list mirrors
  // the ActionId union in src/lib/keymap.ts.
  const VALID_KEYMAP_IDS = new Set([
    "palette.open", "shortcuts.show", "tabs.new", "tabs.close", "tabs.next",
    "tabs.prev", "tabs.goto1", "tabs.goto2", "tabs.goto3", "tabs.goto4",
    "tabs.goto5", "tabs.goto6", "tabs.goto7", "tabs.goto8", "tabs.goto9",
    "find.open", "zoom.in", "zoom.out", "beacon.send", "library.search",
    "theater.start", "theater.next", "theater.prev", "theater.blackout",
    "theater.ink", "theater.exit", "bedrock.open", "press.open", "forms.open",
    "quill.batch", "quill.designer", "quill.autodetect", "quill.smartfill",
    "quill.tour", "atelier.open", "hopper.open", "home.open", "home.continue",
  ]);
  const sampleRows = [
    "panel:bedrock", "panel:press", "panel:forms", "panel:atelier",
    "panel:hopper", "panel:theater", "theater:open", "home:open",
    "home:continue", "library:search", "help:shortcuts", "panel:forms:batch",
    "panel:forms:design", "panel:forms:detect", "panel:forms:smartfill",
    "forms:tour",
  ];
  let allValid = true;
  for (const row of sampleRows) {
    const id = paletteKeymapId(row);
    if (id !== null && !VALID_KEYMAP_IDS.has(id)) allValid = false;
  }
  expect(allValid, "keymap: every mapped target is a real keymap ActionId (no typos)");
}

// --- Lumen II Slice 1: empty-state fallback -------------------------
{
  const entries = [
    { id: "panel:reader", title: "Reader", keywords: "read pdf view" },
    { id: "panel:redact", title: "Redact", keywords: "redact remove black" },
    { id: "home:open", title: "Go to Recents Home", keywords: "home" },
    { id: "library:search", title: "Search library", keywords: "find" },
    { id: "settings:keymap", title: "Customize keyboard shortcuts", keywords: "keys" },
    { id: "help:shortcuts", title: "Keyboard shortcuts", keywords: "help" },
  ];

  // Fat-fingered an extra char: "readerr" relaxes to "reader" then matches.
  const typo = suggestPaletteFallback("readerr", entries);
  expect(typo.kind === "typo", "fallback: extra-char typo -> typo kind");
  expect(typo.relaxed.length < "readerr".length, "fallback: relaxed query is strictly shorter");
  expect(typo.ids.includes("panel:reader"), "fallback: typo surfaces the intended row");

  // Trailing-noise multi-word relaxes through the first word.
  const multi = suggestPaletteFallback("redact pls", entries);
  expect(multi.kind === "typo", "fallback: trailing noise -> typo kind");
  expect(multi.ids.includes("panel:redact"), "fallback: 'redact pls' suggests Redact");

  // Total miss -> curated starters that are present, in priority order.
  const starter = suggestPaletteFallback("zzzzzz", entries);
  expect(starter.kind === "starter", "fallback: total miss -> starter kind");
  expect(starter.relaxed === "", "fallback: starter has no relaxed query");
  expect(starter.ids[0] === "home:open", "fallback: starters in priority order (home first)");
  expect(
    starter.ids.every((id) => entries.some((e) => e.id === id)),
    "fallback: every starter id is present in the entry list",
  );

  // Absent starters are skipped rather than rendered dead.
  const sparse = suggestPaletteFallback("zzzzzz", [
    { id: "library:search", title: "Search library" },
  ]);
  expect(sparse.kind === "starter", "fallback: sparse list still offers present starters");
  expect(
    sparse.ids.length === 1 && sparse.ids[0] === "library:search",
    "fallback: only the present starter is offered",
  );

  // No starters present at all -> none.
  const noneKind = suggestPaletteFallback("zzzzzz", [
    { id: "theme:dark", title: "Theme: Dark" },
  ]);
  expect(noneKind.kind === "none", "fallback: no present starters -> none");

  // Degenerate inputs -> none, never throws.
  expect(suggestPaletteFallback("", entries).kind === "none", "fallback: empty query -> none");
  expect(suggestPaletteFallback("   ", entries).kind === "none", "fallback: blank query -> none");
  expect(suggestPaletteFallback("xx", []).kind === "none", "fallback: empty entry list -> none");

  // 1-char queries don't relax below the floor (would match too much).
  const single = suggestPaletteFallback("r", entries);
  expect(single.kind !== "typo" || single.relaxed.length >= 2, "fallback: no sub-2-char relax");

  // Cap is honoured + ids de-duplicated.
  const dupEntries = [
    { id: "a", title: "Reader one" },
    { id: "a", title: "Reader one dup" },
    { id: "b", title: "Reader two" },
    { id: "c", title: "Reader three" },
  ];
  const capped = suggestPaletteFallback("readerz", dupEntries, 2);
  expect(capped.ids.length <= 2, "fallback: respects the limit cap");
  expect(new Set(capped.ids).size === capped.ids.length, "fallback: ids de-duplicated");

  // Pure: inputs not mutated.
  const frozen = Object.freeze([
    Object.freeze({ id: "panel:reader", title: "Reader" }),
  ]) as PaletteFallbackEntry[];
  suggestPaletteFallback("readerr", frozen);
  expect(frozen.length === 1, "fallback: input array not mutated");
}

// --- Lumen II Slice 2: typed scope sigils ---------------------------
{
  // Parse: each sigil selects its scope and strips the leading char.
  const cmd = parsePaletteScope(">redact");
  expect(cmd.scope === "commands", "scope: '>' -> commands");
  expect(cmd.term === "redact", "scope: '>' strips the sigil from the term");
  expect(cmd.sigil === ">", "scope: '>' sigil recorded");

  const file = parsePaletteScope("@invoice");
  expect(file.scope === "files", "scope: '@' -> files");
  expect(file.term === "invoice", "scope: '@' term stripped");

  const appr = parsePaletteScope("#dark");
  expect(appr.scope === "appearance", "scope: '#' -> appearance");
  expect(appr.term === "dark", "scope: '#' term stripped");

  // Leading whitespace before the sigil is tolerated.
  const ws = parsePaletteScope("  >compress");
  expect(ws.scope === "commands", "scope: leading whitespace before sigil ok");
  expect(ws.term === "compress", "scope: whitespace + sigil strips to term");

  // Space after the sigil is trimmed.
  const spaced = parsePaletteScope("> redact");
  expect(spaced.term === "redact", "scope: space after sigil trimmed");

  // Bare sigil -> empty term (shows the whole scoped class).
  const bare = parsePaletteScope(">");
  expect(bare.scope === "commands" && bare.term === "", "scope: bare sigil -> empty term");

  // No sigil -> all, term is the trimmed raw query.
  const plain = parsePaletteScope("  reader ");
  expect(plain.scope === "all", "scope: no sigil -> all");
  expect(plain.term === "reader", "scope: unscoped term trimmed");
  expect(plain.sigil === "", "scope: unscoped sigil empty");

  // Sigil only counts when LEADING — mid-query '#' is literal text.
  const mid = parsePaletteScope("page #3");
  expect(mid.scope === "all", "scope: non-leading sigil is literal");
  expect(mid.term === "page #3", "scope: literal sigil kept in term");

  // Degenerate.
  expect(parsePaletteScope("").scope === "all", "scope: empty -> all");
  expect(parsePaletteScope("").term === "", "scope: empty -> empty term");

  // Membership: files scope.
  expect(entryMatchesScope("Recent files", "files"), "scope: Recent files in files scope");
  expect(entryMatchesScope("Pinned", "files"), "scope: Pinned in files scope");
  expect(!entryMatchesScope("Appearance", "files"), "scope: Appearance not in files scope");

  // Membership: appearance scope.
  expect(entryMatchesScope("Appearance", "appearance"), "scope: Appearance in appearance scope");
  expect(!entryMatchesScope("Panels", "appearance"), "scope: Panels not in appearance scope");

  // Membership: commands scope excludes files (VSCode '>' semantics) but
  // includes appearance + panels + everything non-file.
  expect(entryMatchesScope("Panels", "commands"), "scope: Panels in commands scope");
  expect(entryMatchesScope("Appearance", "commands"), "scope: Appearance counts as a command");
  expect(!entryMatchesScope("Recent files", "commands"), "scope: files excluded from commands");
  expect(!entryMatchesScope("Pinned", "commands"), "scope: pinned files excluded from commands");

  // "all" matches everything.
  expect(entryMatchesScope("Recent files", "all"), "scope: all includes files");
  expect(entryMatchesScope("Appearance", "all"), "scope: all includes appearance");

  // Labels.
  expect(describePaletteScope("commands") === "Commands", "scope: commands label");
  expect(describePaletteScope("files") === "Files", "scope: files label");
  expect(describePaletteScope("appearance") === "Appearance", "scope: appearance label");
  expect(describePaletteScope("all") === "", "scope: all has no pill label");
}

// --- Lumen II Slice 3: group-jump navigation ------------------------
{
  // Classifier: Cmd/Ctrl + Arrow only.
  expect(
    classifyPaletteGroupNav({ key: "ArrowDown", metaKey: true }) === "group-next",
    "groupnav: Cmd+Down -> group-next",
  );
  expect(
    classifyPaletteGroupNav({ key: "ArrowUp", ctrlKey: true }) === "group-prev",
    "groupnav: Ctrl+Up -> group-prev",
  );
  expect(
    classifyPaletteGroupNav({ key: "ArrowDown" }) === null,
    "groupnav: bare Arrow is NOT a group jump (per-row nav owns it)",
  );
  expect(
    classifyPaletteGroupNav({ key: "ArrowDown", metaKey: true, altKey: true }) === null,
    "groupnav: Alt disqualifies",
  );
  expect(
    classifyPaletteGroupNav({ key: "ArrowDown", metaKey: true, shiftKey: true }) === null,
    "groupnav: Shift disqualifies",
  );
  expect(
    classifyPaletteGroupNav({ key: "Home", metaKey: true }) === null,
    "groupnav: non-arrow key -> null",
  );

  // groupStartIndices: running offsets.
  expect(
    JSON.stringify(groupStartIndices([3, 2, 4])) === JSON.stringify([0, 3, 5]),
    "groupnav: start indices are running offsets",
  );
  expect(
    JSON.stringify(groupStartIndices([])) === JSON.stringify([]),
    "groupnav: empty sizes -> empty starts",
  );
  expect(
    JSON.stringify(groupStartIndices([1, -2, 3])) === JSON.stringify([0, 1, 1]),
    "groupnav: negative size counts as 0",
  );

  // 3 groups of sizes [3,2,4] -> heads [0,3,5], 9 rows (last index 8).
  const starts = groupStartIndices([3, 2, 4]);
  const N = 9;

  // group-next from inside group 0 -> head of group 1.
  expect(nextGroupIndex(starts, 0, "group-next", N) === 3, "groupnav: next from g0 head -> 3");
  expect(nextGroupIndex(starts, 1, "group-next", N) === 3, "groupnav: next from mid-g0 -> 3");
  expect(nextGroupIndex(starts, 3, "group-next", N) === 5, "groupnav: next from g1 head -> 5");
  // group-next from the LAST group drops to the final row.
  expect(nextGroupIndex(starts, 5, "group-next", N) === 8, "groupnav: next in last group -> last row");
  expect(nextGroupIndex(starts, 7, "group-next", N) === 8, "groupnav: next from mid-last -> last row");
  expect(nextGroupIndex(starts, 8, "group-next", N) === 8, "groupnav: next at last row stays");

  // group-prev two-stage: below head -> head; at head -> previous head.
  expect(nextGroupIndex(starts, 7, "group-prev", N) === 5, "groupnav: prev from mid-g2 -> g2 head");
  expect(nextGroupIndex(starts, 5, "group-prev", N) === 3, "groupnav: prev at g2 head -> g1 head");
  expect(nextGroupIndex(starts, 3, "group-prev", N) === 0, "groupnav: prev at g1 head -> g0 head");
  expect(nextGroupIndex(starts, 2, "group-prev", N) === 0, "groupnav: prev from mid-g0 -> 0");
  expect(nextGroupIndex(starts, 0, "group-prev", N) === 0, "groupnav: prev at row 0 stays");

  // Defensive: empty list, stale/NaN current, unsorted/out-of-range heads.
  expect(nextGroupIndex(starts, 0, "group-next", 0) === 0, "groupnav: empty list -> 0");
  expect(nextGroupIndex(starts, 99, "group-next", N) === 8, "groupnav: stale current clamps");
  expect(nextGroupIndex(starts, NaN, "group-prev", N) === 0, "groupnav: NaN current -> 0");
  expect(
    nextGroupIndex([5, 0, 3, 999], 4, "group-prev", N) === 3,
    "groupnav: out-of-range + unsorted heads normalized (4 -> 3)",
  );
  // A heads array missing the 0 anchor still works (0 always added).
  expect(nextGroupIndex([3, 5], 1, "group-prev", N) === 0, "groupnav: 0 head always present");
}

// --- Lumen II Slice 4: recent-file reading progress -----------------
{
  // Mid-document progress: page/total + percent.
  const mid = recentReadingProgress({ lastPage: 12, totalPages: 80 });
  expect(mid.hasProgress, "progress: mid-doc has progress");
  expect(mid.page === 12 && mid.total === 80, "progress: page + total carried");
  expect(mid.percent === 15, "progress: 12/80 -> 15%");
  expect(Math.abs(mid.fraction - 0.15) < 1e-9, "progress: fraction 0.15");
  expect(!mid.finished, "progress: mid-doc not finished");
  expect(mid.label === "p.12/80 · 15%", "progress: mid-doc chip label");

  // Finished (last page reached) reads as "Finished", not "100%".
  const done = recentReadingProgress({ lastPage: 80, totalPages: 80 });
  expect(done.finished, "progress: last page -> finished");
  expect(done.label === "Finished", "progress: finished chip label");
  expect(done.percent === 100, "progress: finished still reports 100 percent");

  // lastPage past the end is clamped to total (and counts as finished).
  const over = recentReadingProgress({ lastPage: 200, totalPages: 80 });
  expect(over.page === 80, "progress: over-end lastPage clamped to total");
  expect(over.finished, "progress: clamped-to-end is finished");

  // pageCount is a fallback total when totalPages is absent.
  const viaCount = recentReadingProgress({ lastPage: 5, pageCount: 10 });
  expect(viaCount.hasProgress && viaCount.total === 10, "progress: pageCount used as total fallback");
  expect(viaCount.percent === 50, "progress: 5/10 via pageCount -> 50%");

  // totalPages takes precedence over pageCount when both present.
  const both = recentReadingProgress({ lastPage: 3, totalPages: 6, pageCount: 99 });
  expect(both.total === 6, "progress: totalPages wins over pageCount");

  // Page 1 of N is real progress (not nothing).
  const start = recentReadingProgress({ lastPage: 1, totalPages: 50 });
  expect(start.hasProgress && start.page === 1, "progress: page 1 is real progress");
  expect(start.percent === 2, "progress: 1/50 -> 2%");

  // No usable data -> empty summary, falls back to plain subtitle.
  expect(!recentReadingProgress({}).hasProgress, "progress: no fields -> no progress");
  expect(recentReadingProgress({}).label === "", "progress: no fields -> empty label");
  expect(
    !recentReadingProgress({ totalPages: 80 }).hasProgress,
    "progress: total without lastPage -> no progress",
  );
  expect(
    !recentReadingProgress({ lastPage: 12 }).hasProgress,
    "progress: lastPage without total -> no progress",
  );

  // Garbage / degenerate inputs never throw, never show progress.
  expect(
    !recentReadingProgress({ lastPage: NaN, totalPages: 80 }).hasProgress,
    "progress: NaN lastPage -> no progress",
  );
  expect(
    !recentReadingProgress({ lastPage: 0, totalPages: 80 }).hasProgress,
    "progress: page 0 -> no progress",
  );
  expect(
    !recentReadingProgress({ lastPage: 5, totalPages: 0 }).hasProgress,
    "progress: zero total -> no progress",
  );
  expect(
    !recentReadingProgress({ lastPage: 5, totalPages: -3 }).hasProgress,
    "progress: negative total -> no progress",
  );
  // @ts-expect-error null tolerance
  expect(!recentReadingProgress(null).hasProgress, "progress: null file -> no progress");

  // Fraction always clamps to [0,1].
  const f = recentReadingProgress({ lastPage: 40, totalPages: 80 });
  expect(f.fraction >= 0 && f.fraction <= 1, "progress: fraction in [0,1]");
  expect(f.percent === 50, "progress: 40/80 -> 50%");
}

// --- Lumen II Slice 5: context-aware footer -------------------------
{
  // Count pluralisation.
  expect(describePaletteCount(0) === "No results", "footer: 0 -> No results");
  expect(describePaletteCount(1) === "1 result", "footer: 1 -> 1 result (singular)");
  expect(describePaletteCount(2) === "2 results", "footer: 2 -> 2 results");
  expect(describePaletteCount(42) === "42 results", "footer: 42 results");
  expect(describePaletteCount(-5) === "No results", "footer: negative -> No results");
  expect(describePaletteCount(NaN) === "No results", "footer: NaN -> No results");
  expect(describePaletteCount(3.9) === "3 results", "footer: fractional floored");

  // Verb by id prefix / group.
  expect(paletteActionVerb({ id: "recent:/a.pdf", group: "Recent files" }) === "Open", "footer: recent -> Open");
  expect(paletteActionVerb({ id: "recent:/b.pdf", group: "Pinned" }) === "Open", "footer: pinned -> Open");
  expect(paletteActionVerb({ id: "panel-window:reader", group: "Windows" }) === "Open", "footer: new window -> Open");

  expect(paletteActionVerb({ id: "panel:reader", group: "Panels" }) === "Switch to", "footer: panel -> Switch to");
  expect(paletteActionVerb({ id: "home:open", group: "Home" }) === "Switch to", "footer: home -> Switch to");
  expect(paletteActionVerb({ id: "library:search", group: "Library" }) === "Switch to", "footer: library search -> Switch to");
  expect(paletteActionVerb({ id: "panel:forms:batch", group: "Forms" }) === "Switch to", "footer: forms subtab -> Switch to");

  expect(paletteActionVerb({ id: "theme:dark", group: "Appearance" }) === "Apply", "footer: theme -> Apply");
  expect(paletteActionVerb({ id: "accent:blue", group: "Appearance" }) === "Apply", "footer: accent -> Apply");
  expect(paletteActionVerb({ id: "density:compact", group: "Appearance" }) === "Apply", "footer: density -> Apply");
  expect(paletteActionVerb({ id: "plugin-theme:x:y", group: "Appearance" }) === "Apply", "footer: plugin theme -> Apply");

  expect(paletteActionVerb({ id: "stack:compare", group: "Stack" }) === "Run", "footer: command -> Run");
  expect(paletteActionVerb({ id: "plugin-cmd:x:y", group: "Plugin commands" }) === "Run", "footer: plugin cmd -> Run");
  expect(paletteActionVerb({ id: "help:shortcuts", group: "Help" }) === "Run", "footer: help -> Run");

  // panel-window beats the panel: prefix (more specific, an Open not a switch).
  expect(
    paletteActionVerb({ id: "panel-window:reader" }) === "Open",
    "footer: panel-window not misread as panel switch",
  );

  // Degenerate.
  expect(paletteActionVerb(null) === "", "footer: null row -> empty verb");
  expect(paletteActionVerb(undefined) === "", "footer: undefined row -> empty verb");
  expect(paletteActionVerb({ id: "" }) === "", "footer: empty id -> empty verb");
  // Unknown id with no group falls back to Run.
  expect(paletteActionVerb({ id: "mystery:thing" }) === "Run", "footer: unknown -> Run fallback");
}

// --- Group collapse (Lumen III Slice 1) ------------------------------
{
  // Toggle: adds when absent, removes when present, never mutates input.
  const base = new Set<string>();
  const a = toggleCollapsedGroup(base, "Appearance");
  expect(a.has("Appearance") && base.size === 0, "collapse: toggle adds (input untouched)");
  const b = toggleCollapsedGroup(a, "Appearance");
  expect(!b.has("Appearance"), "collapse: toggle removes when present");
  expect(a !== b && a !== base, "collapse: each toggle returns a new set");

  const grouped: [string, { id: string }[]][] = [
    ["Panels", [{ id: "p1" }, { id: "p2" }]],
    ["Appearance", [{ id: "t1" }, { id: "t2" }, { id: "t3" }]],
    ["Library", [{ id: "l1" }]],
  ];

  // No collapse: every item visible, heads at 0 / 2 / 5.
  const open = partitionCollapsedGroups(grouped, new Set());
  expect(open.visible.length === 6, "collapse: nothing folded -> all 6 visible");
  expect(open.starts.join() === "0,2,5", "collapse: heads at 0,2,5 when open");
  expect(open.display.every((d) => !d.collapsed), "collapse: no group flagged collapsed");
  expect(open.display[1].items.length === 3, "collapse: open group keeps its items");
  expect(open.display[1].count === 3, "collapse: header count is the true size");

  // Fold the middle group: its items vanish from `visible`, header stays,
  // and the following group's start index shifts down accordingly.
  const folded = partitionCollapsedGroups(grouped, new Set(["Appearance"]));
  expect(folded.visible.map((x) => x.id).join() === "p1,p2,l1", "collapse: folded items absent from cursor space");
  expect(folded.display.length === 3, "collapse: all three headers still render");
  expect(folded.display[1].collapsed && folded.display[1].items.length === 0, "collapse: folded group shows header, no items");
  expect(folded.display[1].count === 3, "collapse: folded header still shows true count");
  expect(folded.starts.join() === "0,2", "collapse: only open groups contribute heads");

  // Fold everything: zero visible rows, three headers, no heads.
  const allFolded = partitionCollapsedGroups(grouped, new Set(["Panels", "Appearance", "Library"]));
  expect(allFolded.visible.length === 0, "collapse: all folded -> empty cursor space");
  expect(allFolded.display.length === 3, "collapse: all folded -> headers remain");
  expect(allFolded.starts.length === 0, "collapse: all folded -> no jump heads");

  // Garbage tolerant.
  // @ts-expect-error — garbage grouped list
  expect(partitionCollapsedGroups(null, new Set()).visible.length === 0, "collapse: null grouped -> empty");
  // @ts-expect-error — garbage collapsed arg
  expect(partitionCollapsedGroups(grouped, null).visible.length === 6, "collapse: null set treated as nothing folded");
}

// --- collapseAllGroups + isEveryGroupCollapsed ------------------------
{
  const grouped: [string, { id: string }[]][] = [
    ["Panels", [{ id: "p1" }, { id: "p2" }]],
    ["Appearance", [{ id: "a1" }]],
    ["Library", [{ id: "l1" }]],
  ];

  // collapseAllGroups returns every group name.
  const all = collapseAllGroups(grouped);
  expect(all.size === 3, "collapse-all: every group name captured");
  expect(all.has("Panels") && all.has("Appearance") && all.has("Library"), "collapse-all: names present");

  // isEveryGroupCollapsed: true only when the set covers every group.
  expect(isEveryGroupCollapsed(grouped, all) === true, "all-collapsed: full set -> true");
  expect(
    isEveryGroupCollapsed(grouped, new Set(["Panels", "Appearance"])) === false,
    "all-collapsed: missing one group -> false",
  );
  expect(isEveryGroupCollapsed(grouped, new Set()) === false, "all-collapsed: empty set -> false");

  // A superset (a stale collapsed name no longer present) still counts as
  // all-collapsed, since every CURRENT group is folded.
  expect(
    isEveryGroupCollapsed(grouped, new Set(["Panels", "Appearance", "Library", "Gone"])) === true,
    "all-collapsed: superset still all-collapsed",
  );

  // Empty grouped list -> nothing to collapse / expand.
  expect(collapseAllGroups([]).size === 0, "collapse-all: empty -> empty set");
  expect(isEveryGroupCollapsed([], new Set()) === false, "all-collapsed: empty grouped -> false");

  // Round-trips with the real collapse partition: collapse-all then
  // partition yields zero visible rows but every header.
  const folded = partitionCollapsedGroups(grouped, collapseAllGroups(grouped));
  expect(folded.visible.length === 0, "collapse-all: partitions to empty cursor space");
  expect(folded.display.length === 3, "collapse-all: every header still renders");

  // Garbage tolerant.
  // @ts-expect-error — garbage grouped list
  expect(collapseAllGroups(null).size === 0, "collapse-all: null grouped -> empty");
  // @ts-expect-error — garbage grouped list
  expect(isEveryGroupCollapsed(null, new Set(["x"])) === false, "all-collapsed: null grouped -> false");
  // @ts-expect-error — garbage collapsed arg
  expect(isEveryGroupCollapsed(grouped, null) === false, "all-collapsed: null set -> false");
}

// --- describeCollapseState (footer collapse legibility) ---------------
{
  const grouped: [string, { id: string }[]][] = [
    ["Panels", [{ id: "p1" }, { id: "p2" }]],
    ["Appearance", [{ id: "a1" }]],
    ["Library", [{ id: "l1" }]],
    ["Tools", [{ id: "t1" }]],
  ];

  // Nothing folded -> empty label (footer falls back to its count), but
  // the counts are still exposed for any caller that wants them.
  const none = describeCollapseState(grouped, new Set());
  expect(none.total === 4 && none.open === 4 && none.collapsed === 0, "collapse-state: none folded counts");
  expect(none.noneCollapsed && !none.allCollapsed, "collapse-state: none folded flags");
  expect(none.label === "", "collapse-state: none folded -> empty label");

  // Some folded -> "N of M sections open".
  const some = describeCollapseState(grouped, new Set(["Appearance", "Tools"]));
  expect(some.open === 2 && some.collapsed === 2, "collapse-state: some folded counts");
  expect(!some.allCollapsed && !some.noneCollapsed, "collapse-state: some folded flags");
  expect(some.label === "2 of 4 sections open", "collapse-state: some folded label");

  // All folded -> "All M collapsed".
  const all = describeCollapseState(grouped, collapseAllGroups(grouped));
  expect(all.open === 0 && all.allCollapsed, "collapse-state: all folded flags");
  expect(all.label === "All 4 collapsed", "collapse-state: all folded label");

  // A stale folded name (group no longer present) is ignored.
  const stale = describeCollapseState(grouped, new Set(["Appearance", "Ghost"]));
  expect(stale.total === 4 && stale.collapsed === 1, "collapse-state: stale fold ignored");
  expect(stale.label === "3 of 4 sections open", "collapse-state: stale fold label");

  // Thousands grouping holds for a large surface.
  const many: [string, { id: string }[]][] = Array.from({ length: 1200 }, (_, i) => [
    `G${i}`,
    [{ id: `x${i}` }],
  ]);
  const big = describeCollapseState(many, collapseAllGroups(many));
  expect(big.label === "All 1,200 collapsed", "collapse-state: thousands-grouped all");

  // Empty / garbage grouped list -> all-zero, empty label.
  const empty = describeCollapseState([], new Set(["x"]));
  expect(empty.total === 0 && empty.label === "" && empty.noneCollapsed, "collapse-state: empty grouped safe");
  // @ts-expect-error — garbage grouped list
  expect(describeCollapseState(null, new Set()).label === "", "collapse-state: null grouped -> empty label");
  // @ts-expect-error — garbage collapsed arg
  expect(describeCollapseState(grouped, null).label === "", "collapse-state: null set -> none folded -> empty label");
}

// --- soloExpandGroup (round 51 slice 2) ------------------------------
{
  const grouped: [string, { id: string }[]][] = [
    ["Appearance", [{ id: "a" }]],
    ["Library", [{ id: "b" }]],
    ["Reader", [{ id: "c" }]],
  ];

  // From all-open, solo "Library" -> fold every OTHER group.
  const solo = soloExpandGroup(grouped, new Set<string>(), "Library");
  expect(
    solo.has("Appearance") && solo.has("Reader") && !solo.has("Library"),
    "solo: folds every sibling, keeps target open",
  );
  expect(solo.size === 2, "solo: exactly the two siblings folded");

  // Already solo (target open, siblings folded) -> Alt-click again expands all.
  const back = soloExpandGroup(grouped, new Set(["Appearance", "Reader"]), "Library");
  expect(back.size === 0, "solo: re-soloing an already-solo group expands everything");

  // Solo a DIFFERENT group while another is solo -> re-folds around the new target.
  const reSolo = soloExpandGroup(grouped, new Set(["Appearance", "Reader"]), "Appearance");
  expect(
    reSolo.has("Library") && reSolo.has("Reader") && !reSolo.has("Appearance"),
    "solo: switching solo target re-folds around the new group",
  );

  // Target currently folded -> solo opens it, folds the rest.
  const fromFolded = soloExpandGroup(grouped, new Set(["Library"]), "Library");
  expect(
    fromFolded.has("Appearance") && fromFolded.has("Reader") && !fromFolded.has("Library"),
    "solo: a folded target opens and its siblings fold",
  );

  // A group not present -> empty set (nothing to solo, all open).
  expect(soloExpandGroup(grouped, new Set<string>(), "Nope").size === 0, "solo: absent group -> all open");

  // Single-group list: nothing to solo against -> folds nothing (all open).
  const one: [string, { id: string }[]][] = [["Only", [{ id: "x" }]]];
  expect(soloExpandGroup(one, new Set<string>(), "Only").size === 0, "solo: single group -> nothing to fold");

  // Garbage safety.
  // @ts-expect-error — null grouped
  expect(soloExpandGroup(null, new Set(), "x").size === 0, "solo: null grouped -> empty");
  expect(soloExpandGroup(grouped, new Set<string>(), "").size === 0, "solo: blank group name -> empty");
  // @ts-expect-error — garbage collapsed arg
  expect(soloExpandGroup(grouped, null, "Library").has("Appearance"), "solo: null set treated as all-open");
}

// --- pinned commands (round 52) --------------------------------------
{
  expect(isCommandPinned([], "a") === false, "pin: empty list -> not pinned");
  expect(isCommandPinned(["a", "b"], "b") === true, "pin: member -> pinned");
  expect(isCommandPinned(["a"], "") === false, "pin: blank id -> never pinned");

  // toggle on appends to the end (oldest-first), off removes.
  const p1 = toggleCommandPin([], "panel:reader");
  expect(p1.length === 1 && p1[0] === "panel:reader", "pin: toggle on appends");
  const p2 = toggleCommandPin(p1, "accent:blue");
  expect(p2.join(",") === "panel:reader,accent:blue", "pin: second pin keeps order");
  const p3 = toggleCommandPin(p2, "panel:reader");
  expect(p3.join(",") === "accent:blue", "pin: toggle off removes");
  expect(toggleCommandPin(["a"], "").join(",") === "a", "pin: blank id no-op");

  // dedupe defensive: a duplicate-laden input collapses.
  expect(toggleCommandPin(["a", "a", "b"], "c").join(",") === "a,b,c", "pin: dedupe input");

  // count distinct.
  expect(countPinnedCommands(["a", "b", "a"]) === 2, "pin: count distinct");
  expect(countPinnedCommands([]) === 0, "pin: count empty");
  // @ts-expect-error — garbage input
  expect(countPinnedCommands(null) === 0, "pin: count null-safe");
  // @ts-expect-error — garbage input
  expect(toggleCommandPin(null, "a").join(",") === "a", "pin: null list-safe");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
