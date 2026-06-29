// Pure-helper tests for the Library Search view-core (Atlas III).
//
// Style matches paletteSearch.test.ts / beaconCacheView.test.ts —
// no test runner, just an inline `expect` so the contract reads at a
// glance.
//
// Run with:
//   tsx src/lib/librarySearchView.test.ts

import {
  searchBasename,
  classifySearchResultKey,
  flattenSearchHits,
  flatSearchHitCount,
  searchGroupStarts,
  classifyPaletteGroupNav,
  nextGroupIndex,
  nextSearchCursor,
  clampSearchCursor,
  interpretSearchQuery,
  describeQueryInterpretation,
  stripSnippetMarks,
  refineSearchHits,
  buildSnippetSpans,
  cycleSearchSort,
  sortSearchGroups,
  searchSortLabel,
  describeSortMode,
  summarizeSearchResults,
  pageSpread,
  classifyRecentChipKey,
  nextChipCursor,
  clampChipCursor,
  formatRelativeAge,
  sortRecentChips,
  recentChipSortLabel,
  RECENT_CHIP_SORT_MODES,
  suggestEmptyQueries,
  normalizePinnedQuery,
  isPinnedSearch,
  togglePinnedSearch,
  describePinnedSearches,
  moveSavedSearch,
  classifySavedSearchKey,
  classifyJumpSavedKey,
  nextSavedIndex,
  savedSearchHitCount,
  pinYieldBadge,
  mergeSweepYields,
  dryPinQueries,
  clearDryPins,
  describeClearDryPins,
  rankSweepResults,
  describeSweep,
  SEARCH_SORT_MODES,
  type SearchHitLike,
  type SearchGroupLike,
  type RecentChipLike,
  type RecentChipSortMode,
} from "./librarySearchView";

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

let seq = 0;
const hit = (over: Partial<SearchHitLike> = {}): SearchHitLike => ({
  docId: over.docId ?? 1,
  path: over.path ?? `/docs/file${seq++}.pdf`,
  title: over.title ?? null,
  pageIndex: over.pageIndex ?? 0,
  snippet: over.snippet ?? "a <mark>match</mark> here",
  rank: over.rank ?? -1,
});

const group = (
  docId: number,
  title: string,
  hits: SearchHitLike[],
  path = `/docs/${title}.pdf`,
): SearchGroupLike => ({ docId, path, title, hits });

// =====================================================================
// searchBasename
// =====================================================================
{
  expect(searchBasename("/a/b/c.pdf") === "c.pdf", "basename: posix path");
  expect(
    searchBasename("C:\\Users\\x\\report.pdf") === "report.pdf",
    "basename: windows path",
  );
  expect(searchBasename("bare.pdf") === "bare.pdf", "basename: no separator");
  expect(searchBasename("") === "", "basename: empty");
  expect(searchBasename("/trailing/") === "", "basename: trailing slash -> empty");
}

// =====================================================================
// Slice 1 — keyboard navigation
// =====================================================================

// --- classifySearchResultKey ---
{
  expect(
    classifySearchResultKey({ key: "ArrowDown" })?.kind === "move",
    "key: ArrowDown -> move",
  );
  const down = classifySearchResultKey({ key: "ArrowDown" });
  expect(
    down?.kind === "move" && down.intent === "next",
    "key: ArrowDown intent is next",
  );
  const up = classifySearchResultKey({ key: "ArrowUp" });
  expect(up?.kind === "move" && up.intent === "prev", "key: ArrowUp intent is prev");
  const home = classifySearchResultKey({ key: "Home" });
  expect(home?.kind === "move" && home.intent === "first", "key: Home -> first");
  const end = classifySearchResultKey({ key: "End" });
  expect(end?.kind === "move" && end.intent === "last", "key: End -> last");
  const pgdn = classifySearchResultKey({ key: "PageDown" });
  expect(pgdn?.kind === "move" && pgdn.intent === "page-down", "key: PageDown -> page-down");
  expect(classifySearchResultKey({ key: "Enter" })?.kind === "open", "key: Enter -> open");
  expect(classifySearchResultKey({ key: "Escape" })?.kind === "clear", "key: Escape -> clear");
  expect(classifySearchResultKey({ key: "x" }) === null, "key: plain char -> null");
  // Modifiers disqualify so OS/app chords keep priority.
  expect(
    classifySearchResultKey({ key: "ArrowDown", metaKey: true }) === null,
    "key: Cmd+ArrowDown -> null (app chord)",
  );
  expect(
    classifySearchResultKey({ key: "Enter", ctrlKey: true }) === null,
    "key: Ctrl+Enter -> null",
  );
  expect(
    classifySearchResultKey({ key: "ArrowDown", altKey: true }) === null,
    "key: Alt+ArrowDown -> null",
  );
  // Bare Shift still passes nav through (never changes the intent).
  expect(
    classifySearchResultKey({ key: "ArrowDown", shiftKey: true })?.kind === "move",
    "key: Shift+ArrowDown still navigates",
  );
  // @ts-expect-error — defensive null
  expect(classifySearchResultKey(null) === null, "key: null event -> null");
}

// --- flattenSearchHits / flatSearchHitCount ---
{
  const groups = [
    group(1, "Alpha", [hit({ docId: 1 }), hit({ docId: 1 })]),
    group(2, "Bravo", [hit({ docId: 2 })]),
    group(3, "Charlie", [hit({ docId: 3 }), hit({ docId: 3 }), hit({ docId: 3 })]),
  ];
  const flat = flattenSearchHits(groups);
  expect(flat.length === 6, "flatten: total across groups");
  expect(flat[0].groupIndex === 0 && flat[0].hitIndex === 0, "flatten: first coord");
  expect(flat[0].flatIndex === 0, "flatten: first flatIndex");
  expect(flat[2].groupIndex === 1 && flat[2].hitIndex === 0, "flatten: group boundary coord");
  expect(flat[3].groupIndex === 2 && flat[3].hitIndex === 0, "flatten: into third group");
  expect(flat[5].flatIndex === 5, "flatten: last flatIndex");
  // flatIndex is monotonic + contiguous.
  expect(
    flat.every((f, i) => f.flatIndex === i),
    "flatten: flatIndex is contiguous render order",
  );
  expect(flatSearchHitCount(groups) === 6, "count: matches flatten length");
  expect(flatSearchHitCount([]) === 0, "count: empty groups -> 0");
  // @ts-expect-error — garbage
  expect(flattenSearchHits(null).length === 0, "flatten: null -> []");
  // @ts-expect-error — garbage
  expect(flatSearchHitCount(null) === 0, "count: null -> 0");
  // A group with a non-array hits is skipped, not thrown.
  const dirty = [group(1, "A", [hit()]), { docId: 2, path: "", title: "B", hits: null as never }];
  expect(flattenSearchHits(dirty).length === 1, "flatten: skips garbage group");
}

// --- nextSearchCursor (adapter over palette nav) ---
{
  expect(nextSearchCursor("next", 5, 6) === 0, "cursor: next wraps past end to 0");
  expect(nextSearchCursor("prev", 0, 6) === 5, "cursor: prev wraps before start to last");
  expect(nextSearchCursor("next", 1, 6) === 2, "cursor: next steps forward");
  expect(nextSearchCursor("first", 3, 6) === 0, "cursor: first -> 0");
  expect(nextSearchCursor("last", 1, 6) === 5, "cursor: last -> last index");
  expect(nextSearchCursor("page-down", 0, 6) === 5, "cursor: page-down clamps to last");
  expect(nextSearchCursor("page-up", 5, 6) === 0, "cursor: page-up clamps to 0");
  expect(nextSearchCursor("next", 0, 0) === 0, "cursor: empty list -> 0");
}

// --- clampSearchCursor ---
{
  expect(clampSearchCursor(40, 10) === 9, "clamp: out-of-range -> last");
  expect(clampSearchCursor(3, 10) === 3, "clamp: in-range preserved");
  expect(clampSearchCursor(5, 0) === 0, "clamp: empty -> 0");
  expect(clampSearchCursor(-2, 10) === 0, "clamp: negative -> 0");
  expect(clampSearchCursor(NaN, 10) === 0, "clamp: NaN -> 0");
  expect(clampSearchCursor(2.9, 10) === 2, "clamp: fractional floored");
}

// =====================================================================
// Slice 2 — query interpretation (mirrors search.rs tokenize/build_match_expr)
// =====================================================================
{
  // Empty / whitespace.
  const e = interpretSearchQuery("");
  expect(e.empty && e.tokens.length === 0 && !e.noAnchor, "interp: empty string");
  expect(interpretSearchQuery("   ").empty, "interp: whitespace -> empty");

  // Single word -> the last (only) bare token becomes a prefix.
  const one = interpretSearchQuery("indemn");
  expect(
    one.tokens.length === 1 && one.tokens[0].kind === "prefix" && one.tokens[0].text === "indemn",
    "interp: single word is a prefix match",
  );
  expect(!one.noAnchor && !one.empty, "interp: single word has anchor");

  // Two words -> first is exact term, last is prefix.
  const two = interpretSearchQuery("indemnification clause");
  expect(two.tokens.length === 2, "interp: two tokens");
  expect(
    two.tokens[0].kind === "term" && two.tokens[0].text === "indemnification",
    "interp: leading word is exact term",
  );
  expect(
    two.tokens[1].kind === "prefix" && two.tokens[1].text === "clause",
    "interp: trailing word is prefix",
  );

  // Quoted phrase + trailing word -> phrase then prefix.
  // Mirrors Rust: `"force majeure" clause` -> `"force majeure" "clause"*`.
  const ph = interpretSearchQuery('"force majeure" clause');
  expect(ph.tokens.length === 2, "interp: phrase + word = 2 tokens");
  expect(
    ph.tokens[0].kind === "phrase" && ph.tokens[0].text === "force majeure",
    "interp: phrase token preserved",
  );
  expect(ph.tokens[1].kind === "prefix", "interp: word after phrase is prefix (last bare)");

  // Phrase ALONE never gets a prefix; it stays a phrase.
  const phAlone = interpretSearchQuery('"force majeure"');
  expect(
    phAlone.tokens.length === 1 && phAlone.tokens[0].kind === "phrase",
    "interp: lone phrase stays a phrase (no prefix)",
  );
  expect(!phAlone.noAnchor, "interp: lone phrase has anchor");

  // Exclusion: `contract -draft` -> term-prefix + exclude.
  // Mirrors Rust: `"contract"* NOT "draft"`.
  const exc = interpretSearchQuery("contract -draft");
  expect(exc.tokens.length === 2, "interp: exclusion = 2 tokens");
  expect(exc.tokens[0].kind === "prefix" && exc.tokens[0].text === "contract", "interp: anchor is prefix");
  expect(exc.tokens[1].kind === "exclude" && exc.tokens[1].text === "draft", "interp: -draft excludes");
  expect(!exc.noAnchor, "interp: has positive anchor");

  // Only-exclusion: `-draft` -> noAnchor, empty result on the backend.
  const onlyExc = interpretSearchQuery("-draft");
  expect(
    onlyExc.tokens.length === 1 && onlyExc.tokens[0].kind === "exclude",
    "interp: lone -draft is a single exclude token",
  );
  expect(onlyExc.noAnchor, "interp: only-exclusion -> noAnchor true");
  expect(!onlyExc.empty, "interp: only-exclusion is not empty");

  // The LAST bare token is the prefix even with an exclusion between words.
  const mid = interpretSearchQuery("alpha -skip beta");
  expect(mid.tokens.length === 3, "interp: alpha -skip beta = 3 tokens");
  expect(mid.tokens[0].kind === "term", "interp: alpha is exact term");
  expect(mid.tokens[1].kind === "exclude", "interp: -skip excludes");
  expect(mid.tokens[2].kind === "prefix", "interp: beta is the last bare -> prefix");

  // Excluded phrase: `-"first draft"`.
  const excPh = interpretSearchQuery('report -"first draft"');
  expect(excPh.tokens.length === 2, "interp: excluded phrase = 2 tokens");
  expect(excPh.tokens[1].kind === "exclude" && excPh.tokens[1].text === "first draft", "interp: -\"first draft\" excludes phrase");

  // Curly quotes (macOS autocorrect) open phrases too.
  const curly = interpretSearchQuery("\u201Cforce majeure\u201D clause");
  expect(curly.tokens[0].kind === "phrase" && curly.tokens[0].text === "force majeure", "interp: curly quotes form a phrase");

  // Metacharacters scrubbed out of a bare word (mirrors scrub_word).
  const scrub = interpretSearchQuery("foo:bar");
  expect(
    scrub.tokens.length === 1 && scrub.tokens[0].text === "foobar",
    "interp: scrubs FTS metachars from bare word",
  );

  // Mid-word hyphen survives the dash-strip into one word (co-op -> coop).
  const coop = interpretSearchQuery("co-op");
  expect(
    coop.tokens.length === 1 && coop.tokens[0].text === "coop" && coop.tokens[0].kind === "prefix",
    "interp: mid-word hyphen scrubbed to coop",
  );

  // A lone `-` before whitespace is just a dropped dash, not an exclusion.
  const lone = interpretSearchQuery("- word");
  expect(
    lone.tokens.length === 1 && lone.tokens[0].kind === "prefix" && lone.tokens[0].text === "word",
    "interp: lone dash dropped, word is a normal prefix",
  );

  // Empty quotes cancel a pending exclusion.
  const emptyQ = interpretSearchQuery('alpha -""');
  expect(
    emptyQ.tokens.length === 1 && emptyQ.tokens[0].kind === "prefix",
    "interp: empty quotes cancel pending exclusion",
  );

  // Unterminated quote runs to end-of-input as a phrase.
  const unterm = interpretSearchQuery('"force majeure');
  expect(
    unterm.tokens.length === 1 && unterm.tokens[0].kind === "phrase" && unterm.tokens[0].text === "force majeure",
    "interp: unterminated quote -> phrase to EOI",
  );
}

// --- describeQueryInterpretation ---
{
  expect(describeQueryInterpretation(interpretSearchQuery("")) === "", "describe: empty -> ''");
  const d1 = describeQueryInterpretation(interpretSearchQuery("contract"));
  expect(d1.includes("prefix") && d1.includes("contract"), "describe: single prefix mentions prefix");
  const d2 = describeQueryInterpretation(interpretSearchQuery("contract -draft"));
  expect(d2.includes("excluding") && d2.includes("draft"), "describe: mentions exclusion");
  const d3 = describeQueryInterpretation(interpretSearchQuery("-draft"));
  expect(d3.includes("Only exclusions"), "describe: no-anchor explains why empty");
  const d4 = describeQueryInterpretation(interpretSearchQuery('"force majeure" clause'));
  expect(d4.includes("phrase") && d4.includes("force majeure"), "describe: mentions the phrase");
  // Oxford-style list join with 3 positives.
  const d5 = describeQueryInterpretation(interpretSearchQuery("alpha beta gamma"));
  expect(d5.includes(", and "), "describe: 3-item list uses ', and'");
}

// =====================================================================
// Slice 3 — in-results refine
// =====================================================================

// --- stripSnippetMarks ---
{
  expect(stripSnippetMarks("a <mark>b</mark> c") === "a b c", "strip: removes mark tags");
  expect(stripSnippetMarks("x &amp; y") === "x & y", "strip: decodes &amp;");
  expect(stripSnippetMarks("a &lt;b&gt; c") === "a <b> c", "strip: decodes lt/gt");
  expect(stripSnippetMarks("") === "", "strip: empty -> ''");
  expect(stripSnippetMarks("plain text") === "plain text", "strip: passthrough");
}

// --- refineSearchHits ---
{
  const hits = [
    hit({ docId: 1, title: "Lease Agreement", path: "/d/lease.pdf", snippet: "the <mark>termination</mark> clause" }),
    hit({ docId: 2, title: "Invoice", path: "/d/invoice.pdf", snippet: "amount <mark>due</mark> on receipt" }),
    hit({ docId: 3, title: "NDA", path: "/d/secret-nda.pdf", snippet: "<mark>confidential</mark> information" }),
  ];
  // Blank refine passes all through.
  expect(refineSearchHits(hits, "").length === 3, "refine: blank passes all");
  expect(refineSearchHits(hits, "   ").length === 3, "refine: whitespace passes all");
  // Match on snippet text.
  const r1 = refineSearchHits(hits, "termination");
  expect(r1.length === 1 && r1[0].docId === 1, "refine: matches snippet text");
  // Match on title.
  const r2 = refineSearchHits(hits, "invoice");
  expect(r2.length === 1 && r2[0].docId === 2, "refine: matches document title");
  // Match on basename (not the folder).
  const r3 = refineSearchHits(hits, "nda");
  expect(r3.length === 1 && r3[0].docId === 3, "refine: matches basename");
  // No match -> empty.
  expect(refineSearchHits(hits, "zzzznotpresent").length === 0, "refine: no match -> []");
  // Order preserved (membership only).
  const r4 = refineSearchHits(hits, "e");
  expect(
    r4.length >= 2 && r4[0].docId <= r4[r4.length - 1].docId,
    "refine: preserves input order",
  );
  // The `<mark>` markup itself is never matchable (stripped first).
  expect(refineSearchHits(hits, "mark").length === 0, "refine: snippet markup not matchable");
  // @ts-expect-error — garbage
  expect(refineSearchHits(null, "x").length === 0, "refine: null list -> []");
}

// =====================================================================
// Slice 4 — sort modes
// =====================================================================
{
  expect(SEARCH_SORT_MODES.length === 3, "sort: three modes");
  expect(searchSortLabel("relevance") === "Relevance", "sort: relevance label");
  expect(searchSortLabel("document") === "Document", "sort: document label");
  expect(searchSortLabel("matches") === "Matches", "sort: matches label");
  expect(describeSortMode("matches") === "match count", "sort: describe matches");

  // cycle wraps through all three.
  expect(cycleSearchSort("relevance") === "document", "sort: cycle rel -> doc");
  expect(cycleSearchSort("document") === "matches", "sort: cycle doc -> matches");
  expect(cycleSearchSort("matches") === "relevance", "sort: cycle matches -> rel (wraps)");
  // @ts-expect-error — garbage current
  expect(cycleSearchSort("nonsense") === "relevance", "sort: garbage -> first mode");

  const groups = [
    group(1, "Charlie", [hit({ docId: 1 }), hit({ docId: 1 })]), // 2 hits, arrival 0
    group(2, "Alpha", [hit({ docId: 2 }), hit({ docId: 2 }), hit({ docId: 2 })]), // 3 hits, arrival 1
    group(3, "Bravo", [hit({ docId: 3 })]), // 1 hit, arrival 2
  ];

  // Relevance preserves arrival order.
  const rel = sortSearchGroups(groups, "relevance");
  expect(
    rel.map((g) => g.docId).join(",") === "1,2,3",
    "sort: relevance preserves arrival order",
  );
  expect(rel !== groups, "sort: returns a new array (relevance)");

  // Document sorts title A->Z.
  const doc = sortSearchGroups(groups, "document");
  expect(
    doc.map((g) => g.title).join(",") === "Alpha,Bravo,Charlie",
    "sort: document is A->Z by title",
  );

  // Matches sorts by hit count, biggest first.
  const matches = sortSearchGroups(groups, "matches");
  expect(
    matches.map((g) => g.docId).join(",") === "2,1,3",
    "sort: matches is biggest hit-count first",
  );

  // Numeric-aware document sort (Doc2 < Doc10).
  const numeric = sortSearchGroups(
    [group(1, "Doc10", [hit()]), group(2, "Doc2", [hit()])],
    "document",
  );
  expect(numeric.map((g) => g.title).join(",") === "Doc2,Doc10", "sort: document numeric-aware");

  // Stable tie-break: equal match counts keep arrival order.
  const ties = [
    group(10, "Z", [hit()]),
    group(20, "Y", [hit()]),
    group(30, "X", [hit()]),
  ];
  const tied = sortSearchGroups(ties, "matches");
  expect(
    tied.map((g) => g.docId).join(",") === "10,20,30",
    "sort: equal match counts keep arrival order (stable)",
  );

  // Input never mutated.
  const before = groups.map((g) => g.docId).join(",");
  sortSearchGroups(groups, "matches");
  expect(groups.map((g) => g.docId).join(",") === before, "sort: input array not mutated");

  // @ts-expect-error — garbage
  expect(sortSearchGroups(null, "relevance").length === 0, "sort: null -> []");
}

// =====================================================================
// Slice 5 — summary footer + page spread
// =====================================================================

// --- summarizeSearchResults ---
{
  expect(
    summarizeSearchResults({ shown: 0, docs: 0, total: 0, refine: "", sortMode: "relevance" }) === "",
    "summary: no results -> ''",
  );
  const s1 = summarizeSearchResults({ shown: 12, docs: 3, total: 12, refine: "", sortMode: "relevance" });
  expect(s1 === "12 matches across 3 documents", "summary: plain count");
  const s2 = summarizeSearchResults({ shown: 1, docs: 1, total: 1, refine: "", sortMode: "relevance" });
  expect(s2 === "1 match across 1 document", "summary: singular pluralization");
  const s3 = summarizeSearchResults({ shown: 4, docs: 2, total: 12, refine: "term", sortMode: "relevance" });
  expect(
    s3.includes("4 of 12 matches") && s3.includes("refined") && s3.includes("term"),
    "summary: refine shows 'N of M' + refine term",
  );
  const s4 = summarizeSearchResults({ shown: 8, docs: 3, total: 8, refine: "", sortMode: "matches" });
  expect(s4.includes("by match count"), "summary: non-default sort named");
  const s5 = summarizeSearchResults({ shown: 8, docs: 3, total: 8, refine: "", sortMode: "relevance" });
  expect(!s5.includes("by "), "summary: default sort not named");
  // shown clamps to total defensively.
  const s6 = summarizeSearchResults({ shown: 99, docs: 1, total: 5, refine: "", sortMode: "relevance" });
  expect(s6.startsWith("5 matches"), "summary: shown clamps to total");
}

// --- pageSpread ---
{
  expect(pageSpread([hit({ pageIndex: 3 })]) === "p. 4", "spread: single page (0-based -> 1-based)");
  expect(
    pageSpread([hit({ pageIndex: 2 }), hit({ pageIndex: 46 })]) === "pp. 3\u201347",
    "spread: min-max range",
  );
  // Unsorted input still yields min..max.
  expect(
    pageSpread([hit({ pageIndex: 46 }), hit({ pageIndex: 2 }), hit({ pageIndex: 10 })]) === "pp. 3\u201347",
    "spread: unsorted hits -> correct min-max",
  );
  // All on the same page collapse to "p. N".
  expect(
    pageSpread([hit({ pageIndex: 5 }), hit({ pageIndex: 5 })]) === "p. 6",
    "spread: same page collapses",
  );
  expect(pageSpread([]) === "", "spread: empty -> ''");
  // @ts-expect-error — garbage
  expect(pageSpread(null) === "", "spread: null -> ''");
  // Non-finite pageIndex is skipped.
  expect(
    pageSpread([hit({ pageIndex: NaN as never }), hit({ pageIndex: 7 })]) === "p. 8",
    "spread: skips non-finite pageIndex",
  );
}

// --- buildSnippetSpans (Slice 3b refine-highlight) ---
{
  // Plain text, no marks, no refine -> a single non-match non-refine span.
  const a = buildSnippetSpans("hello world", "");
  expect(a.length === 1 && a[0].text === "hello world", "spans: plain single span");
  expect(!a[0].match && !a[0].refine, "spans: plain not match/refine");

  // Server mark is flagged match.
  const b = buildSnippetSpans("a <mark>cat</mark> sat", "");
  expect(b.length === 3, "spans: split around one mark");
  expect(b[0].text === "a " && !b[0].match, "spans: pre-mark plain");
  expect(b[1].text === "cat" && b[1].match && !b[1].refine, "spans: mark flagged match");
  expect(b[2].text === " sat" && !b[2].match, "spans: post-mark plain");

  // Refine paints a literal occurrence OUTSIDE a mark, case-insensitive.
  const c = buildSnippetSpans("the Termination clause", "termination");
  const cr = c.find((s) => s.refine);
  expect(!!cr && cr.text === "Termination", "spans: refine keeps original casing");
  expect(c.map((s) => s.text).join("") === "the Termination clause", "spans: refine lossless reassembly");

  // Refine INSIDE a mark carries BOTH flags.
  const d = buildSnippetSpans("x <mark>force majeure</mark> y", "majeure");
  const both = d.find((s) => s.match && s.refine);
  expect(!!both && both.text === "majeure", "spans: refine inside mark is match+refine");

  // Multiple refine occurrences across mark boundary.
  const e = buildSnippetSpans("pay <mark>pay</mark> pay", "pay");
  expect(e.filter((s) => s.refine).length === 3, "spans: all three 'pay' painted");
  expect(e.filter((s) => s.match && s.refine).length === 1, "spans: middle 'pay' is match+refine");

  // Entities decode so render is plain text (no {@html}).
  const f = buildSnippetSpans("a &amp; <mark>b &lt;c&gt;</mark>", "");
  expect(f.map((s) => s.text).join("") === "a & b <c>", "spans: entities decoded");

  // Blank refine never paints (no zero-width matches between chars).
  const g = buildSnippetSpans("abc", "   ");
  expect(g.length === 1 && !g[0].refine, "spans: whitespace refine paints nothing");

  // Empty / null snippet -> [].
  expect(buildSnippetSpans("", "x").length === 0, "spans: empty snippet -> []");
  // @ts-expect-error — garbage
  expect(buildSnippetSpans(null, "x").length === 0, "spans: null snippet -> []");

  // Unterminated mark degrades to plain text rather than throwing.
  const h = buildSnippetSpans("a <mark>b", "");
  expect(h.map((s) => s.text).join("") === "a <mark>b", "spans: unterminated mark kept literal");

  // Refine that doesn't occur in the snippet text -> no refine spans
  // (it matched the title/filename instead, which is correct).
  const k = buildSnippetSpans("nothing here", "absent");
  expect(k.every((s) => !s.refine), "spans: non-occurring refine paints nothing");
}

// --- searchGroupStarts + group-jump chord (Slice 1b) ---
{
  const g3 = [
    group(1, "A", [hit(), hit(), hit()]),
    group(2, "B", [hit(), hit()]),
    group(3, "C", [hit(), hit(), hit(), hit()]),
  ];
  expect(
    JSON.stringify(searchGroupStarts(g3)) === JSON.stringify([0, 3, 5]),
    "groupstarts: flat heads from group sizes",
  );
  expect(JSON.stringify(searchGroupStarts([])) === "[]", "groupstarts: empty -> []");
  // @ts-expect-error — garbage
  expect(JSON.stringify(searchGroupStarts(null)) === "[]", "groupstarts: null -> []");
  // A group with no hits contributes a head but zero width.
  const gEmpty = [group(1, "A", [hit()]), group(2, "B", []), group(3, "C", [hit(), hit()])];
  expect(
    JSON.stringify(searchGroupStarts(gEmpty)) === JSON.stringify([0, 1, 1]),
    "groupstarts: empty group keeps a head at zero width",
  );

  // The chord classifier is the palette's (re-exported) — sanity-check
  // the re-export wiring resolves and behaves.
  expect(classifyPaletteGroupNav({ key: "ArrowDown", metaKey: true }) === "group-next", "chord: Cmd+Down -> next");
  expect(classifyPaletteGroupNav({ key: "ArrowUp", ctrlKey: true }) === "group-prev", "chord: Ctrl+Up -> prev");
  expect(classifyPaletteGroupNav({ key: "ArrowDown" }) === null, "chord: bare arrow -> null");
  expect(classifyPaletteGroupNav({ key: "ArrowDown", metaKey: true, shiftKey: true }) === null, "chord: Shift disqualifies");

  // The mover leaps to group heads using those starts.
  const starts = searchGroupStarts(g3); // [0,3,5], count 9
  expect(nextGroupIndex(starts, 0, "group-next", 9) === 3, "jump: from row 0 -> head of group 2");
  expect(nextGroupIndex(starts, 3, "group-next", 9) === 5, "jump: from head 3 -> head of group 3");
  expect(nextGroupIndex(starts, 6, "group-next", 9) === 8, "jump: in last group -> last row");
  expect(nextGroupIndex(starts, 6, "group-prev", 9) === 5, "jump: below head -> section top first");
  expect(nextGroupIndex(starts, 5, "group-prev", 9) === 3, "jump: at head -> previous head");
  expect(nextGroupIndex(starts, 0, "group-prev", 9) === 0, "jump: top stays at 0");
}

// --- Slice 6: recent-search chips keyboard navigation ----------------
{
  // Horizontal axis: Left/Right map to prev/next, Home/End leap.
  expect(
    JSON.stringify(classifyRecentChipKey({ key: "ArrowRight" })) ===
      JSON.stringify({ kind: "move", intent: "next" }),
    "chip: Right -> next",
  );
  expect(
    JSON.stringify(classifyRecentChipKey({ key: "ArrowLeft" })) ===
      JSON.stringify({ kind: "move", intent: "prev" }),
    "chip: Left -> prev",
  );
  expect(
    JSON.stringify(classifyRecentChipKey({ key: "Home" })) ===
      JSON.stringify({ kind: "move", intent: "first" }),
    "chip: Home -> first",
  );
  expect(
    JSON.stringify(classifyRecentChipKey({ key: "End" })) ===
      JSON.stringify({ kind: "move", intent: "last" }),
    "chip: End -> last",
  );
  // Enter runs, Escape parks.
  expect(classifyRecentChipKey({ key: "Enter" })?.kind === "run", "chip: Enter -> run");
  expect(classifyRecentChipKey({ key: "Escape" })?.kind === "clear", "chip: Escape -> clear");
  // Backspace / Delete drop the focused chip (one stray query).
  expect(classifyRecentChipKey({ key: "Backspace" })?.kind === "delete", "chip: Backspace -> delete");
  expect(classifyRecentChipKey({ key: "Delete" })?.kind === "delete", "chip: Delete -> delete");
  expect(
    classifyRecentChipKey({ key: "Backspace", metaKey: true }) === null,
    "chip: Cmd+Backspace falls through (app chord)",
  );
  // Up/Down are NOT claimed — they belong to the results cursor.
  expect(classifyRecentChipKey({ key: "ArrowUp" }) === null, "chip: Up not claimed (results owns it)");
  expect(classifyRecentChipKey({ key: "ArrowDown" }) === null, "chip: Down not claimed");
  // Modifiers disqualify so app chords win.
  expect(classifyRecentChipKey({ key: "ArrowRight", metaKey: true }) === null, "chip: Cmd+Right falls through");
  expect(classifyRecentChipKey({ key: "ArrowLeft", ctrlKey: true }) === null, "chip: Ctrl+Left falls through");
  expect(classifyRecentChipKey({ key: "Enter", altKey: true }) === null, "chip: Alt+Enter falls through");
  expect(classifyRecentChipKey({ key: "x" }) === null, "chip: a letter is not a chip key");
  // @ts-expect-error — garbage event
  expect(classifyRecentChipKey(null) === null, "chip: null event -> null");

  // Cursor wraps over a strip of 5 chips (palette nav contract).
  expect(nextChipCursor("next", 4, 5) === 0, "chip: next past end wraps to 0");
  expect(nextChipCursor("prev", 0, 5) === 4, "chip: prev before start wraps to last");
  expect(nextChipCursor("first", 3, 5) === 0, "chip: first -> 0");
  expect(nextChipCursor("last", 1, 5) === 4, "chip: last -> end");

  // Clamp snaps a stale cursor into a refreshed strip; -1 = nothing focused.
  expect(clampChipCursor(7, 5) === 4, "chip: clamp 7 into 5 -> 4");
  expect(clampChipCursor(2, 5) === 2, "chip: clamp in-range unchanged");
  expect(clampChipCursor(0, 0) === -1, "chip: empty strip -> -1");
  expect(clampChipCursor(-1, 5) === -1, "chip: negative cursor -> -1 (unfocused)");
  expect(clampChipCursor(3, NaN) === -1, "chip: NaN count -> -1");
}

// --- formatRelativeAge ------------------------------------------------
{
  const NOW = 1_700_000_000; // fixed reference
  expect(formatRelativeAge(NOW, NOW) === "just now", "age: same instant -> just now");
  expect(formatRelativeAge(NOW - 10, NOW) === "just now", "age: 10s -> just now");
  expect(formatRelativeAge(NOW - 44, NOW) === "just now", "age: 44s -> just now (under threshold)");
  expect(formatRelativeAge(NOW - 60, NOW) === "1m", "age: 60s -> 1m");
  expect(formatRelativeAge(NOW - 90, NOW) === "1m", "age: 90s -> 1m (floored)");
  expect(formatRelativeAge(NOW - 59 * 60, NOW) === "59m", "age: 59m -> 59m");
  expect(formatRelativeAge(NOW - 60 * 60, NOW) === "1h", "age: 1h -> 1h");
  expect(formatRelativeAge(NOW - 23 * 3600, NOW) === "23h", "age: 23h -> 23h");
  expect(formatRelativeAge(NOW - 24 * 3600, NOW) === "1d", "age: 24h -> 1d");
  expect(formatRelativeAge(NOW - 6 * 86400, NOW) === "6d", "age: 6d -> 6d");
  expect(formatRelativeAge(NOW - 7 * 86400, NOW) === "1w", "age: 7d -> 1w");
  expect(formatRelativeAge(NOW - 51 * 7 * 86400, NOW) === "51w", "age: 51w -> 51w");
  expect(formatRelativeAge(NOW - 52 * 7 * 86400, NOW) === "1y+", "age: >=52w -> 1y+");
  // A future timestamp (clock skew) degrades to "just now", never negative.
  expect(formatRelativeAge(NOW + 5000, NOW) === "just now", "age: future ts -> just now");
  // Garbage is safe.
  expect(formatRelativeAge(NaN, NOW) === "just now", "age: NaN ts -> just now");
  expect(formatRelativeAge(NOW, NaN) === "just now", "age: NaN now -> just now");
}

// --- sortRecentChips + labels -----------------------------------------
{
  const chip = (ts: number, resultCount: number): RecentChipLike => ({ ts, resultCount });

  // Mode metadata: two modes in display order, with sensible labels.
  expect(RECENT_CHIP_SORT_MODES.length === 2, "chip-sort: two modes");
  expect(RECENT_CHIP_SORT_MODES[0] === "recent", "chip-sort: recent leads");
  expect(RECENT_CHIP_SORT_MODES[1] === "results", "chip-sort: results second");
  expect(recentChipSortLabel("recent") === "Recent", "chip-sort: recent label");
  expect(recentChipSortLabel("results") === "Results", "chip-sort: results label");
  // Unknown mode degrades to the raw string (never throws / blanks).
  expect(
    recentChipSortLabel("bogus" as RecentChipSortMode) === "bogus",
    "chip-sort: unknown label passthrough",
  );

  // Recent mode: newest ts first.
  {
    const chips = [chip(100, 5), chip(300, 1), chip(200, 9)];
    const out = sortRecentChips(chips, "recent");
    expect(
      out.map((c) => c.ts).join(",") === "300,200,100",
      "chip-sort: recent orders newest ts first",
    );
    // Non-mutating: the input keeps its original order.
    expect(chips[0].ts === 100, "chip-sort: input not mutated (recent)");
  }

  // Results mode: biggest resultCount first.
  {
    const chips = [chip(100, 5), chip(300, 1), chip(200, 9)];
    const out = sortRecentChips(chips, "results");
    expect(
      out.map((c) => c.resultCount).join(",") === "9,5,1",
      "chip-sort: results orders biggest count first",
    );
  }

  // Stable arrival tie-break: equal ts keeps incoming order under recent.
  {
    const a = chip(500, 1);
    const b = chip(500, 2);
    const c = chip(500, 3);
    const out = sortRecentChips([a, b, c], "recent");
    expect(out[0] === a && out[1] === b && out[2] === c, "chip-sort: stable on equal ts");
  }

  // Stable arrival tie-break: equal resultCount keeps incoming order.
  {
    const a = chip(100, 7);
    const b = chip(200, 7);
    const c = chip(300, 7);
    const out = sortRecentChips([a, b, c], "results");
    expect(out[0] === a && out[1] === b && out[2] === c, "chip-sort: stable on equal count");
  }

  // Garbage numbers coerce to 0 (no NaN-driven jitter), input order kept.
  {
    const chips = [
      { ts: NaN, resultCount: 3 } as RecentChipLike,
      { ts: 100, resultCount: NaN } as RecentChipLike,
    ];
    const out = sortRecentChips(chips, "recent");
    expect(out.length === 2, "chip-sort: garbage ts handled without dropping rows");
    // ts=100 outranks ts=NaN(->0) under recent.
    expect(out[0].ts === 100, "chip-sort: finite ts outranks NaN ts");
  }

  // Empty + garbage inputs -> [].
  expect(sortRecentChips([], "recent").length === 0, "chip-sort: empty -> []");
  expect(
    sortRecentChips(null as unknown as RecentChipLike[], "results").length === 0,
    "chip-sort: null -> []",
  );
}

// --- suggestEmptyQueries ----------------------------------------------
{
  type Rec = RecentChipLike & { id: number; query: string };
  const rec = (id: number, query: string, resultCount: number, ts = id): Rec => ({
    id,
    query,
    resultCount,
    ts,
  });

  // Ranks by hit count, biggest first; excludes the failed query.
  {
    const recents = [
      rec(1, "invoices", 3),
      rec(2, "contracts", 42),
      rec(3, "receipts", 12),
    ];
    const out = suggestEmptyQueries(recents, "zzznope");
    expect(out.map((s) => s.query).join(",") === "contracts,receipts,invoices", "suggest: ranked by hit count");
    expect(out[0].resultCount === 42, "suggest: carries the result count");
  }

  // Excludes the exact failed query (case + whitespace insensitive).
  {
    const recents = [rec(1, "Contracts", 42), rec(2, "receipts", 12)];
    const out = suggestEmptyQueries(recents, "  contracts ");
    expect(out.length === 1 && out[0].query === "receipts", "suggest: drops the failed query (case/space-insensitive)");
  }

  // Skips dead-end chips (a recent that itself found nothing).
  {
    const recents = [rec(1, "ghost", 0), rec(2, "real", 5)];
    const out = suggestEmptyQueries(recents, "x");
    expect(out.length === 1 && out[0].query === "real", "suggest: skips zero-result chips");
  }

  // Stable arrival tie-break on equal counts.
  {
    const a = rec(1, "aa", 7, 100);
    const b = rec(2, "bb", 7, 200);
    const out = suggestEmptyQueries([a, b], "x");
    expect(out[0].query === "aa" && out[1].query === "bb", "suggest: stable on equal counts");
  }

  // Cap honoured; default is 4.
  {
    const recents = [
      rec(1, "a", 10),
      rec(2, "b", 9),
      rec(3, "c", 8),
      rec(4, "d", 7),
      rec(5, "e", 6),
    ];
    expect(suggestEmptyQueries(recents, "x").length === 4, "suggest: default cap 4");
    expect(suggestEmptyQueries(recents, "x", 2).length === 2, "suggest: explicit cap 2");
    expect(suggestEmptyQueries(recents, "x", 0).length === 4, "suggest: garbage cap -> default 4");
  }

  // Empty / garbage inputs -> [].
  expect(suggestEmptyQueries([], "x").length === 0, "suggest: empty -> []");
  expect(suggestEmptyQueries(null as unknown as Rec[], "x").length === 0, "suggest: null -> []");
  // A list where every chip is dead-end or the failed query -> [].
  expect(
    suggestEmptyQueries([rec(1, "fail", 5), rec(2, "dead", 0)], "fail").length === 0,
    "suggest: nothing eligible -> []",
  );
}

// --- Pinned (saved) searches ------------------------------------------
{
  // normalizePinnedQuery: collapse internal whitespace + trim.
  expect(normalizePinnedQuery("  tax   2024 ") === "tax 2024", "pin-norm: collapse + trim");
  expect(normalizePinnedQuery("plain") === "plain", "pin-norm: already clean");
  expect(normalizePinnedQuery("") === "", "pin-norm: empty -> ''");
  expect(normalizePinnedQuery("   ") === "", "pin-norm: blank -> ''");
  expect(normalizePinnedQuery(null) === "", "pin-norm: null -> ''");
  expect(normalizePinnedQuery(undefined) === "", "pin-norm: undefined -> ''");

  // isPinnedSearch: case/space-insensitive membership.
  const list = ["invoices 2024", "tax final"];
  expect(isPinnedSearch(list, "invoices 2024") === true, "pin-is: exact match");
  expect(isPinnedSearch(list, "INVOICES 2024") === true, "pin-is: case-insensitive");
  expect(isPinnedSearch(list, "  tax   final ") === true, "pin-is: spacing-insensitive");
  expect(isPinnedSearch(list, "missing") === false, "pin-is: absent -> false");
  expect(isPinnedSearch(list, "") === false, "pin-is: blank -> false");
  expect(isPinnedSearch(null as unknown as string[], "x") === false, "pin-is: null list -> false");

  // togglePinnedSearch: pin prepends (newest first), drops dup, caps.
  {
    const after = togglePinnedSearch(["a", "b"], "c");
    expect(after.join() === "c,a,b", "pin-toggle: new pin prepends");
    // Pinning an existing (case/space variant) unpins it.
    expect(togglePinnedSearch(["a", "b"], "A").join() === "b", "pin-toggle: re-pin variant unpins");
    expect(togglePinnedSearch(["a", "b"], "  b ").join() === "a", "pin-toggle: unpin by spacing variant");
    // Normalizes on pin.
    expect(togglePinnedSearch(["a"], "  hello   world ").join() === "hello world,a", "pin-toggle: normalizes new pin");
    // Existing dups in the base list collapse (keeping first).
    expect(togglePinnedSearch(["a", "A", "b"], "z").join() === "z,a,b", "pin-toggle: base dups collapse");
    // Blank query -> list returned normalized, unchanged otherwise.
    expect(togglePinnedSearch(["a", "b"], "   ").join() === "a,b", "pin-toggle: blank -> unchanged");
    // Garbage list -> safe.
    expect(togglePinnedSearch(null as unknown as string[], "x").join() === "x", "pin-toggle: null list -> [q]");
  }

  // togglePinnedSearch: cap drops the oldest when over the limit.
  {
    const full = ["p1", "p2", "p3"];
    const capped = togglePinnedSearch(full, "new", 3);
    expect(capped.join() === "new,p1,p2", "pin-toggle: cap drops oldest");
    expect(togglePinnedSearch(full, "new", 0).length === 4, "pin-toggle: garbage cap -> default 32 keeps all");
  }

  // describePinnedSearches: pluralized count, "" when empty.
  expect(describePinnedSearches(["a", "b", "c"]) === "3 saved searches", "pin-desc: plural");
  expect(describePinnedSearches(["only"]) === "1 saved search", "pin-desc: singular");
  expect(describePinnedSearches([]) === "", "pin-desc: empty -> ''");
  expect(describePinnedSearches(["", "  "]) === "", "pin-desc: only-blanks -> ''");
  expect(describePinnedSearches(null as unknown as string[]) === "", "pin-desc: null -> ''");
}

// --- moveSavedSearch (round 51 slice 4) ------------------------------
{
  const base = ["alpha", "beta", "gamma", "delta"];

  // Move the first chip to the end.
  expect(
    JSON.stringify(moveSavedSearch(base, 0, 3)) === JSON.stringify(["beta", "gamma", "delta", "alpha"]),
    "move-saved: first -> last",
  );
  // Move the last chip to the front.
  expect(
    JSON.stringify(moveSavedSearch(base, 3, 0)) === JSON.stringify(["delta", "alpha", "beta", "gamma"]),
    "move-saved: last -> first",
  );
  // Move a middle chip one left.
  expect(
    JSON.stringify(moveSavedSearch(base, 2, 1)) === JSON.stringify(["alpha", "gamma", "beta", "delta"]),
    "move-saved: middle one left",
  );
  // No-op move (from === to) returns the normalized list unchanged.
  expect(
    JSON.stringify(moveSavedSearch(base, 1, 1)) === JSON.stringify(base),
    "move-saved: no-op move unchanged",
  );
  // `to` clamps into range (moving past the end lands at the end).
  expect(
    JSON.stringify(moveSavedSearch(base, 0, 99)) === JSON.stringify(["beta", "gamma", "delta", "alpha"]),
    "move-saved: to clamps to last",
  );
  // Out-of-range `from` -> unchanged (normalized).
  expect(
    JSON.stringify(moveSavedSearch(base, 9, 0)) === JSON.stringify(base),
    "move-saved: from out of range -> unchanged",
  );
  // Normalizes + de-dupes before moving (so the strip stays clean).
  expect(
    JSON.stringify(moveSavedSearch(["  a  ", "b", "A", ""], 0, 1)) === JSON.stringify(["b", "a"]),
    "move-saved: normalizes + de-dupes, then moves",
  );
  // Garbage safety.
  expect(JSON.stringify(moveSavedSearch([], 0, 1)) === "[]", "move-saved: empty -> []");
  expect(JSON.stringify(moveSavedSearch(null as unknown as string[], 0, 1)) === "[]", "move-saved: null -> []");
  expect(
    JSON.stringify(moveSavedSearch(base, NaN, NaN)) === JSON.stringify(base),
    "move-saved: NaN indices -> unchanged",
  );
}

// --- classifySavedSearchKey (round 51 slice 5) -----------------------
{
  // Plain Left/Right MOVE the cursor; Home/End leap.
  expect(
    JSON.stringify(classifySavedSearchKey({ key: "ArrowRight" })) ===
      JSON.stringify({ kind: "move", intent: "next" }),
    "saved-key: Right -> move next",
  );
  expect(
    JSON.stringify(classifySavedSearchKey({ key: "ArrowLeft" })) ===
      JSON.stringify({ kind: "move", intent: "prev" }),
    "saved-key: Left -> move prev",
  );
  expect(
    JSON.stringify(classifySavedSearchKey({ key: "Home" })) ===
      JSON.stringify({ kind: "move", intent: "first" }),
    "saved-key: Home -> first",
  );
  expect(
    JSON.stringify(classifySavedSearchKey({ key: "End" })) ===
      JSON.stringify({ kind: "move", intent: "last" }),
    "saved-key: End -> last",
  );
  // Alt+Left/Right REORDER (keyboard twin of drag), checked before move.
  expect(
    JSON.stringify(classifySavedSearchKey({ key: "ArrowRight", altKey: true })) ===
      JSON.stringify({ kind: "reorder", dir: 1 }),
    "saved-key: Alt+Right -> reorder +1",
  );
  expect(
    JSON.stringify(classifySavedSearchKey({ key: "ArrowLeft", altKey: true })) ===
      JSON.stringify({ kind: "reorder", dir: -1 }),
    "saved-key: Alt+Left -> reorder -1",
  );
  // Enter runs, Backspace/Delete unpin, Escape parks.
  expect(classifySavedSearchKey({ key: "Enter" })?.kind === "run", "saved-key: Enter -> run");
  expect(classifySavedSearchKey({ key: "Backspace" })?.kind === "unpin", "saved-key: Backspace -> unpin");
  expect(classifySavedSearchKey({ key: "Delete" })?.kind === "unpin", "saved-key: Delete -> unpin");
  expect(classifySavedSearchKey({ key: "Escape" })?.kind === "clear", "saved-key: Escape -> clear");
  // Up/Down NOT claimed (results cursor owns the vertical axis).
  expect(classifySavedSearchKey({ key: "ArrowUp" }) === null, "saved-key: Up not claimed");
  expect(classifySavedSearchKey({ key: "ArrowDown" }) === null, "saved-key: Down not claimed");
  // Cmd/Ctrl disqualify everything so app chords win.
  expect(classifySavedSearchKey({ key: "ArrowRight", metaKey: true }) === null, "saved-key: Cmd+Right falls through");
  expect(classifySavedSearchKey({ key: "ArrowLeft", ctrlKey: true }) === null, "saved-key: Ctrl+Left falls through");
  expect(classifySavedSearchKey({ key: "Enter", metaKey: true }) === null, "saved-key: Cmd+Enter falls through");
  // Alt on a non-arrow key isn't ours (e.g. Alt+Enter falls through).
  expect(classifySavedSearchKey({ key: "Enter", altKey: true }) === null, "saved-key: Alt+Enter falls through");
  expect(classifySavedSearchKey({ key: "Home", altKey: true }) === null, "saved-key: Alt+Home falls through");
  // A letter is not a strip key; null event safe.
  expect(classifySavedSearchKey({ key: "p" }) === null, "saved-key: a letter is not a strip key");
  expect(classifySavedSearchKey(null as never) === null, "saved-key: null event -> null");
}

// --- savedSearchHitCount: recover a pin's last-run yield -------------
{
  const recents = [
    { query: "invoices 2024", resultCount: 42 },
    { query: "Tax Final", resultCount: 0 },
    { query: "contracts", resultCount: 7 },
  ];
  expect(savedSearchHitCount("invoices 2024", recents) === 42, "saved-count: exact match");
  expect(savedSearchHitCount("  INVOICES 2024 ", recents) === 42, "saved-count: case+trim insensitive");
  expect(savedSearchHitCount("tax final", recents) === 0, "saved-count: known zero is a real 0, not null");
  expect(savedSearchHitCount("never run", recents) === null, "saved-count: pin not in log -> null");
  expect(savedSearchHitCount("", recents) === null, "saved-count: blank query -> null");
  expect(savedSearchHitCount("x", null as never) === null, "saved-count: null list -> null");
}

// --- pinYieldBadge + mergeSweepYields --------------------------------
{
  const y = { "invoices 2024": 84, "old tax": 0 };
  expect(pinYieldBadge("invoices 2024", y) === 84, "yield: exact match");
  expect(pinYieldBadge("  INVOICES 2024 ", y) === 84, "yield: case+trim");
  expect(pinYieldBadge("old tax", y) === 0, "yield: dry pin is a real 0");
  expect(pinYieldBadge("never swept", y) === null, "yield: unswept -> null");
  expect(pinYieldBadge("", y) === null, "yield: blank -> null");
  expect(pinYieldBadge("x", null) === null, "yield: null map -> null");
  const merged = mergeSweepYields({ keep: 5, "old tax": 9 }, [{ query: "old tax", count: 0 }, { query: "new", count: 3 }]);
  expect(merged["old tax"] === 0, "merge: overwrites prior with new sweep");
  expect(merged.keep === 5, "merge: untouched prior preserved");
  expect(merged.new === 3, "merge: adds new pin");
  expect(mergeSweepYields(null, []).keep === undefined, "merge: null prior -> {}");
}

// --- dryPinQueries / clearDryPins / describeClearDryPins -------------
{
  const pins = ["invoices 2024", "old tax", "vendors", "stale"];
  const y = { "invoices 2024": 84, "old tax": 0, vendors: 3, stale: 0 };
  expect(JSON.stringify(dryPinQueries(pins, y)) === JSON.stringify(["old tax", "stale"]), "dry: picks measured-0 pins in order");
  expect(dryPinQueries(pins, { "old tax": 0 }).length === 1, "dry: only the swept-0 is dry, unswept stay");
  expect(JSON.stringify(clearDryPins(pins, y)) === JSON.stringify(["invoices 2024", "vendors"]), "clear: drops dry, keeps order");
  expect(clearDryPins(pins, y) !== pins, "clear: new array");
  expect(JSON.stringify(clearDryPins(pins, {})) === JSON.stringify(pins), "clear: none dry -> unchanged");
  expect(clearDryPins(null as never, y).length === 0, "clear: garbage -> []");
  expect(describeClearDryPins(2) === "Clear 2 dry pins", "label: plural");
  expect(describeClearDryPins(1) === "Clear 1 dry pin", "label: singular");
  expect(describeClearDryPins(0) === "", "label: zero hides");
}

// --- jump-to-next-saved chord ----------------------------------------
{
  expect(JSON.stringify(classifyJumpSavedKey({ key: "]", metaKey: true })) === JSON.stringify({ dir: 1 }), "jump: cmd+] -> next");
  expect(JSON.stringify(classifyJumpSavedKey({ key: "[", ctrlKey: true })) === JSON.stringify({ dir: -1 }), "jump: ctrl+[ -> prev");
  expect(classifyJumpSavedKey({ key: "]" }) === null, "jump: bare ] not claimed");
  expect(classifyJumpSavedKey({ key: "]", metaKey: true, altKey: true }) === null, "jump: alt disqualifies");
  expect(classifyJumpSavedKey({ key: "]", metaKey: true, shiftKey: true }) === null, "jump: shift disqualifies");
  expect(classifyJumpSavedKey({ key: "a", metaKey: true }) === null, "jump: other key -> null");
  // nextSavedIndex wrapping
  expect(nextSavedIndex(-1, 3, 1) === 0, "next: none + forward -> 0");
  expect(nextSavedIndex(-1, 3, -1) === 2, "next: none + back -> last");
  expect(nextSavedIndex(2, 3, 1) === 0, "next: last forward wraps to 0");
  expect(nextSavedIndex(0, 3, -1) === 2, "next: first back wraps to last");
  expect(nextSavedIndex(1, 3, 1) === 2, "next: middle forward");
  expect(nextSavedIndex(0, 0, 1) === -1, "next: empty -> -1");
  expect(nextSavedIndex(9, 3, 1) === 0, "next: out-of-range clamps then wraps");
}

// --- run-all-saved sweep summary -------------------------------------
{
  const ranked = rankSweepResults([
    { query: "alpha", count: 0 },
    { query: "beta", count: 40 },
    { query: "gamma", count: 3 },
  ]);
  expect(ranked[0].query === "beta" && ranked[2].query === "alpha", "sweep: biggest first, dry last");
  expect(rankSweepResults([{ query: "z", count: 2 }, { query: "a", count: 2 }])[0].query === "a", "sweep: tie alpha");
  expect(rankSweepResults([{ query: " ", count: 5 }]).length === 0, "sweep: blank query dropped");
  expect(rankSweepResults([{ query: "x", count: -3 }])[0].count === 0, "sweep: negative floors to 0");
  expect(rankSweepResults(null as never).length === 0, "sweep: garbage -> []");
  expect(describeSweep([{ query: "a", count: 5 }, { query: "b", count: 3 }]) === "2 searches, 8 hits", "sweep: clean digest");
  expect(describeSweep([{ query: "a", count: 5 }, { query: "b", count: 0 }]) === "2 searches, 5 hits, 1 came up empty", "sweep: one dry");
  expect(describeSweep([{ query: "a", count: 0 }, { query: "b", count: 0 }]) === "all 2 came up empty", "sweep: all dry");
  expect(describeSweep([{ query: "a", count: 1 }]) === "1 search, 1 hit", "sweep: singular");
  expect(describeSweep([]) === "No saved searches to run", "sweep: empty");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
