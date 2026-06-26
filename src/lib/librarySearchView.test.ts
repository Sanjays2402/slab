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

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
