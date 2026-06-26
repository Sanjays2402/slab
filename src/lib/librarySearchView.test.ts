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
  nextSearchCursor,
  clampSearchCursor,
  interpretSearchQuery,
  describeQueryInterpretation,
  stripSnippetMarks,
  refineSearchHits,
  type SearchHitLike,
  type SearchGroupLike,
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

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
