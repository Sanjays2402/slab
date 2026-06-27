// Pure-helper tests for the Reader Find-in-Page view-core (Atlas IV).
//
// Style matches paletteSearch.test.ts / librarySearchView.test.ts —
// no test runner, just an inline `expect` so the contract reads at a
// glance.
//
// Run with:
//   tsx src/lib/readerFindView.test.ts

import {
  FIND_STATE,
  interpretFindState,
  idleFindStatus,
  describeFindStatus,
  findStatusTone,
  defaultFindOptions,
  buildFindDispatch,
  toggleFindOption,
  describeFindOptions,
  FIND_OPTION_TOGGLES,
  FIND_HISTORY_LIMIT,
  pushFindHistory,
  suggestFindHistory,
  suggestionSegments,
  classifyFindGlobalKey,
  classifyFindDropdownKey,
  announceFindStatus,
  type FindControlEvent,
  type FindOptions,
} from "./readerFindView";

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

const ev = (over: Partial<FindControlEvent> = {}): FindControlEvent => ({
  state: over.state ?? null,
  matchesCount: over.matchesCount ?? { current: 0, total: 0 },
  rawQuery: over.rawQuery ?? null,
  previous: over.previous ?? null,
});

// =====================================================================
// Slice 1: interpretFindState / describeFindStatus / tone
// =====================================================================
{
  // Blank query is always idle regardless of payload.
  expect(interpretFindState(ev({ state: FIND_STATE.FOUND }), "").phase === "idle", "interpret: blank query -> idle");
  expect(interpretFindState(ev(), "   ").phase === "idle", "interpret: whitespace query -> idle");
  expect(interpretFindState(null, "x").phase === "not-found" || interpretFindState(null, "x").phase === "pending", "interpret: null ev with query -> pending/not-found (not crash)");

  // PENDING.
  const pend = interpretFindState(ev({ state: FIND_STATE.PENDING }), "foo");
  expect(pend.phase === "pending", "interpret: PENDING -> pending");

  // FOUND with counts.
  const found = interpretFindState(
    ev({ state: FIND_STATE.FOUND, matchesCount: { current: 3, total: 17 } }),
    "foo",
  );
  expect(found.phase === "found", "interpret: FOUND -> found");
  expect(found.current === 3 && found.total === 17, "interpret: FOUND keeps counts");
  expect(found.wrapped === false, "interpret: FOUND not wrapped");
  expect(found.query === "foo", "interpret: status carries trimmed query");

  // WRAPPED is a found state with wrapped=true.
  const wrap = interpretFindState(
    ev({ state: FIND_STATE.WRAPPED, matchesCount: { current: 1, total: 4 } }),
    "foo",
  );
  expect(wrap.phase === "found" && wrap.wrapped === true, "interpret: WRAPPED -> found + wrapped");

  // NOT_FOUND with zero matches.
  const nf = interpretFindState(ev({ state: FIND_STATE.NOT_FOUND, matchesCount: { current: 0, total: 0 } }), "zzz");
  expect(nf.phase === "not-found", "interpret: NOT_FOUND -> not-found");

  // Count-only progress event (no state) with zero so far -> pending, NOT not-found.
  const prog = interpretFindState(ev({ state: null, matchesCount: { current: 0, total: 0 } }), "foo");
  expect(prog.phase === "pending", "interpret: count-only zero mid-scan -> pending (no false 'No matches')");

  // Count-only progress with some matches -> found.
  const prog2 = interpretFindState(ev({ state: null, matchesCount: { current: 1, total: 5 } }), "foo");
  expect(prog2.phase === "found" && prog2.total === 5, "interpret: count-only with hits -> found");

  // Negative / garbage counts clamp to 0.
  const garbage = interpretFindState(
    ev({ state: FIND_STATE.FOUND, matchesCount: { current: -3 as number, total: NaN as never } }),
    "foo",
  );
  expect(garbage.current === 0 && garbage.total === 0, "interpret: garbage counts clamp to 0");
  // total 0 + FOUND state but no NOT_FOUND -> falls to pending (scan continues).
  expect(garbage.phase === "pending", "interpret: FOUND state with 0 total -> pending");

  // describeFindStatus.
  expect(describeFindStatus(idleFindStatus()) === "", "describe: idle -> ''");
  expect(describeFindStatus(found) === "3 of 17", "describe: found -> 'N of M'");
  expect(describeFindStatus(nf) === "No matches", "describe: not-found copy");
  expect(describeFindStatus(pend) === "Searching\u2026", "describe: pending with no counts -> 'Searching…'");
  const pendPartial = interpretFindState(ev({ state: FIND_STATE.PENDING, matchesCount: { current: 2, total: 9 } }), "foo");
  expect(describeFindStatus(pendPartial) === "2 of 9\u2026", "describe: pending with partial counts shows running tally");

  // tone.
  expect(findStatusTone(idleFindStatus()) === "muted", "tone: idle -> muted");
  expect(findStatusTone(pend) === "muted", "tone: pending -> muted");
  expect(findStatusTone(nf) === "warn", "tone: not-found -> warn");
  expect(findStatusTone(found) === "normal", "tone: found -> normal");
}

// =====================================================================
// Slice 2: buildFindDispatch / options
// =====================================================================
{
  const opts = defaultFindOptions();
  expect(
    opts.caseSensitive === false && opts.wholeWord === false && opts.matchDiacritics === false && opts.highlightAll === true,
    "options: defaults",
  );

  const find = buildFindDispatch("find", "foo", opts);
  expect(find.type === "find" && find.query === "foo" && find.findPrevious === false, "dispatch: find type/query/dir");
  expect(find.source === null, "dispatch: source null (pdf.js convention)");
  expect(find.entireWord === false, "dispatch: wholeWord maps to entireWord");
  expect(find.matchDiacritics === false, "dispatch: matchDiacritics present");
  expect(find.highlightAll === true, "dispatch: highlightAll on by default");

  const next = buildFindDispatch("again-next", "foo", opts);
  expect(next.type === "again" && next.findPrevious === false, "dispatch: again-next");
  const prev = buildFindDispatch("again-prev", "foo", opts);
  expect(prev.type === "again" && prev.findPrevious === true, "dispatch: again-prev -> findPrevious true");

  const onToggle = buildFindDispatch("options", "foo", opts);
  expect(onToggle.type === "highlightallchange", "dispatch: options -> highlightallchange");

  const clear = buildFindDispatch("clear", "foo", opts);
  expect(clear.query === "" && clear.type === "find", "dispatch: clear empties the query");

  // Options flow through identically across actions (the whole point: no drift).
  const custom: FindOptions = { caseSensitive: true, wholeWord: true, matchDiacritics: true, highlightAll: false };
  const a = buildFindDispatch("find", "x", custom);
  const b = buildFindDispatch("again-next", "x", custom);
  expect(
    a.caseSensitive === b.caseSensitive &&
      a.entireWord === b.entireWord &&
      a.matchDiacritics === b.matchDiacritics &&
      a.highlightAll === b.highlightAll,
    "dispatch: options identical across find & again (no drift)",
  );
  expect(a.caseSensitive === true && a.entireWord === true && a.matchDiacritics === true && a.highlightAll === false, "dispatch: custom options carried");

  // toggleFindOption is immutable.
  const t = toggleFindOption(opts, "caseSensitive");
  expect(t.caseSensitive === true && opts.caseSensitive === false, "toggle: immutable flip");
  expect(toggleFindOption(opts, "wholeWord").wholeWord === true, "toggle: wholeWord");

  // describeFindOptions.
  expect(describeFindOptions(opts) === "", "describeOptions: defaults -> ''");
  expect(describeFindOptions(custom) === "match case, whole words, match diacritics", "describeOptions: all on");
  expect(describeFindOptions({ ...opts, wholeWord: true }) === "whole words", "describeOptions: single");

  // Toggle metadata covers the 3 real options.
  expect(FIND_OPTION_TOGGLES.length === 3, "toggles: three options");
  expect(FIND_OPTION_TOGGLES.every((t) => typeof t.label === "string" && typeof t.title === "string"), "toggles: each has label + title");
  expect(FIND_OPTION_TOGGLES.map((t) => t.key).join(",") === "caseSensitive,wholeWord,matchDiacritics", "toggles: stable order");
}

// =====================================================================
// Slice 3: pushFindHistory / suggestFindHistory
// =====================================================================
{
  // Push prepends, dedupes case-insensitively keeping new casing.
  let h: string[] = [];
  h = pushFindHistory(h, "Foo");
  expect(h.length === 1 && h[0] === "Foo", "history: first push");
  h = pushFindHistory(h, "bar");
  expect(h.join(",") === "bar,Foo", "history: newest first");
  h = pushFindHistory(h, "FOO");
  expect(h.join(",") === "FOO,bar", "history: dedupe case-insensitive, replace casing, move to front");

  // Blank ignored.
  expect(pushFindHistory(h, "   ").join(",") === "FOO,bar", "history: blank ignored");
  expect(pushFindHistory(h, "").length === 2, "history: empty ignored");

  // Trims to limit.
  let big: string[] = [];
  for (let i = 0; i < FIND_HISTORY_LIMIT + 5; i++) big = pushFindHistory(big, `q${i}`);
  expect(big.length === FIND_HISTORY_LIMIT, "history: trims to limit");
  expect(big[0] === `q${FIND_HISTORY_LIMIT + 4}`, "history: newest retained");

  // Non-array input tolerated.
  expect(pushFindHistory(null as never, "x").join(",") === "x", "history: null input tolerated");

  // suggest: empty query -> most-recent N, no scoring.
  const ring = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"];
  const recent = suggestFindHistory(ring, "", 3);
  expect(recent.length === 3 && recent[0].query === "alpha" && recent[0].score === 0, "suggest: empty query -> top-N MRU");
  expect(recent.every((s) => s.ranges.length === 0), "suggest: empty query -> no highlight ranges");

  // suggest: filters + scores + highlights.
  const scored = suggestFindHistory(["report-final", "report-draft", "invoice"], "rep");
  expect(scored.length === 2, "suggest: only matching entries");
  expect(scored.every((s) => s.score > 0 && s.ranges.length > 0), "suggest: scored entries carry ranges");
  expect(scored[0].query.startsWith("report"), "suggest: report entries surface");

  // suggest: excludes the exact already-typed term.
  const exact = suggestFindHistory(["foo", "foobar"], "foo");
  expect(exact.every((s) => s.query.toLowerCase() !== "foo"), "suggest: excludes exact-typed term");
  expect(exact.some((s) => s.query === "foobar"), "suggest: keeps prefix-extension");

  // suggest: no matches -> empty.
  expect(suggestFindHistory(["alpha", "beta"], "zzz").length === 0, "suggest: no matches -> []");

  // suggestionSegments reconstructs the query verbatim.
  const seg = suggestionSegments(scored[0]);
  expect(seg.map((s) => s.text).join("") === scored[0].query, "suggest: segments reconstruct query");
  expect(seg.some((s) => s.hit), "suggest: at least one highlighted segment");

  // visible cap respected on scored path too.
  const capped = suggestFindHistory(["ab1", "ab2", "ab3", "ab4"], "ab", 2);
  expect(capped.length === 2, "suggest: visible cap on scored path");
}

// =====================================================================
// Slice 4: classifyFindGlobalKey / classifyFindDropdownKey
// =====================================================================
{
  // Cmd+F / Ctrl+F open.
  expect(classifyFindGlobalKey({ key: "f", metaKey: true }) === "open", "global: Cmd+F -> open");
  expect(classifyFindGlobalKey({ key: "F", ctrlKey: true }) === "open", "global: Ctrl+F (caps) -> open");
  // Shift+Cmd+F is the LIBRARY search — must NOT be claimed here.
  expect(classifyFindGlobalKey({ key: "f", metaKey: true, shiftKey: true }) === null, "global: Shift+Cmd+F not claimed (library search)");
  // Bare f is typing.
  expect(classifyFindGlobalKey({ key: "f" }) === null, "global: bare f -> null");

  // F3 cycles.
  expect(classifyFindGlobalKey({ key: "F3" }) === "again-next", "global: F3 -> next");
  expect(classifyFindGlobalKey({ key: "F3", shiftKey: true }) === "again-prev", "global: Shift+F3 -> prev");
  expect(classifyFindGlobalKey({ key: "F3", altKey: true }) === null, "global: Alt+F3 disqualified");

  // Cmd+G cycles.
  expect(classifyFindGlobalKey({ key: "g", metaKey: true }) === "again-next", "global: Cmd+G -> next");
  expect(classifyFindGlobalKey({ key: "g", ctrlKey: true, shiftKey: true }) === "again-prev", "global: Ctrl+Shift+G -> prev");
  expect(classifyFindGlobalKey({ key: "g" }) === null, "global: bare g -> null");

  // Garbage tolerated.
  expect(classifyFindGlobalKey(null as never) === null, "global: null -> null");
  expect(classifyFindGlobalKey({ key: 5 as never }) === null, "global: non-string key -> null");

  // Dropdown nav.
  expect(classifyFindDropdownKey({ key: "ArrowDown" }, false) === "next", "dropdown: ArrowDown -> next");
  expect(classifyFindDropdownKey({ key: "ArrowUp" }, false) === "prev", "dropdown: ArrowUp -> prev");
  expect(classifyFindDropdownKey({ key: "Escape" }, false) === "close", "dropdown: Escape -> close");
  // Enter only commits when a suggestion is highlighted.
  expect(classifyFindDropdownKey({ key: "Enter" }, true) === "commit", "dropdown: Enter+highlight -> commit");
  expect(classifyFindDropdownKey({ key: "Enter" }, false) === null, "dropdown: Enter without highlight -> null (find runs)");
  // Modifiers fall through.
  expect(classifyFindDropdownKey({ key: "ArrowDown", metaKey: true }, false) === null, "dropdown: Cmd+Arrow falls through");
  expect(classifyFindDropdownKey({ key: "x" }, true) === null, "dropdown: typing -> null");
}

// =====================================================================
// Slice 5: announceFindStatus
// =====================================================================
{
  expect(announceFindStatus(idleFindStatus()) === "", "announce: idle -> ''");

  const found = interpretFindState(ev({ state: FIND_STATE.FOUND, matchesCount: { current: 3, total: 17 } }), "foo");
  expect(announceFindStatus(found) === "Match 3 of 17 for \u201cfoo\u201d", "announce: found phrasing");

  const wrapped = interpretFindState(ev({ state: FIND_STATE.WRAPPED, matchesCount: { current: 1, total: 4 } }), "foo");
  expect(announceFindStatus(wrapped) === "Match 1 of 4 for \u201cfoo\u201d, wrapped", "announce: wrapped suffix");

  const nf = interpretFindState(ev({ state: FIND_STATE.NOT_FOUND, matchesCount: { current: 0, total: 0 } }), "zzz");
  expect(announceFindStatus(nf) === "No matches for \u201czzz\u201d", "announce: not-found phrasing");

  const pend = interpretFindState(ev({ state: FIND_STATE.PENDING }), "foo");
  expect(announceFindStatus(pend) === "Searching for \u201cfoo\u201d\u2026", "announce: pending phrasing");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
