// Pure-helper tests for the RecentsHome view-core (Atlas V).
//
// Style matches paletteSearch.test.ts / librarySearchView.test.ts /
// readerFindView.test.ts — no test runner, just an inline `expect` so the
// contract reads at a glance.
//
// Run with:
//   tsx src/lib/recentsHomeView.test.ts

import {
  chooseContinueCandidate,
  heroKind,
  partitionRecents,
  recentMatches,
  filterRecents,
  highlightRecentName,
  RECENT_SORT_MODES,
  isRecentSortMode,
  recentSortLabel,
  describeRecentSort,
  cycleRecentSort,
  sortRecentView,
  flattenRecentCards,
  classifyRecentKey,
  recentCardScrollOptions,
  moveRecentCursor,
  clampRecentCursor,
  countInProgress,
  summarizeRecents,
  countUnpinned,
  describeClearUnpinned,
  recentProgressBar,
  pinnedStripEdges,
  orderPinnedStrip,
  movePinned,
  anyPinOrder,
  describeResetPinOrder,
  type RecentLike,
} from "./recentsHomeView";

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

const mk = (over: Partial<RecentLike> = {}): RecentLike => ({
  path: over.path ?? "/docs/file.pdf",
  name: over.name ?? "file.pdf",
  openedAt: over.openedAt ?? 1000,
  pageCount: over.pageCount,
  pinned: over.pinned,
  pinOrder: over.pinOrder,
  lastPage: over.lastPage,
  totalPages: over.totalPages,
  lastReadAt: over.lastReadAt,
});

// =====================================================================
// Slice 1: chooseContinueCandidate / heroKind / partitionRecents
// =====================================================================
{
  // Empty / garbage.
  expect(chooseContinueCandidate([]) === null, "candidate: empty -> null");
  expect(chooseContinueCandidate(null as never) === null, "candidate: null -> null");
  expect(chooseContinueCandidate([null as never, undefined as never]) === null, "candidate: all-nullish -> null");
  expect(chooseContinueCandidate([null as never]) === null, "candidate: only-nullish -> null");

  // Prefers an in-progress file over a more-recently-opened complete one.
  const a = mk({ path: "/a", name: "a", openedAt: 100, lastPage: 5, totalPages: 20, lastReadAt: 90 });
  const b = mk({ path: "/b", name: "b", openedAt: 200 }); // newer open, no progress
  expect(chooseContinueCandidate([b, a])?.path === "/a", "candidate: in-progress beats fresher-open-without-progress");

  // Among in-progress files, newest-read wins (lastReadAt).
  const p1 = mk({ path: "/p1", name: "p1", openedAt: 100, lastPage: 2, totalPages: 10, lastReadAt: 500 });
  const p2 = mk({ path: "/p2", name: "p2", openedAt: 100, lastPage: 3, totalPages: 10, lastReadAt: 900 });
  expect(chooseContinueCandidate([p1, p2])?.path === "/p2", "candidate: newest lastReadAt wins");

  // A finished file (lastPage == total) is NOT in-progress; falls back to first.
  const done = mk({ path: "/done", name: "done", openedAt: 300, lastPage: 10, totalPages: 10 });
  const plain = mk({ path: "/plain", name: "plain", openedAt: 200 });
  expect(chooseContinueCandidate([done, plain])?.path === "/done", "candidate: finished file not in-progress -> first entry");

  // lastReadAt absent -> falls back to openedAt for ordering.
  const q1 = mk({ path: "/q1", name: "q1", openedAt: 800, lastPage: 1, totalPages: 9 });
  const q2 = mk({ path: "/q2", name: "q2", openedAt: 400, lastPage: 1, totalPages: 9 });
  expect(chooseContinueCandidate([q2, q1])?.path === "/q1", "candidate: no lastReadAt falls back to openedAt");

  // heroKind.
  expect(heroKind(null) === "empty", "heroKind: null -> empty");
  expect(heroKind(undefined) === "empty", "heroKind: undefined -> empty");
  expect(heroKind(mk({ lastPage: 3, totalPages: 10 })) === "resume", "heroKind: with progress -> resume");
  expect(heroKind(mk({})) === "cold", "heroKind: no progress -> cold");
  expect(heroKind(mk({ lastPage: 10, totalPages: 10 })) === "resume", "heroKind: finished still has progress -> resume");

  // partitionRecents: hero excluded from others; pinned its own list (may include hero).
  const h = mk({ path: "/h", name: "h", openedAt: 500, lastPage: 2, totalPages: 9, lastReadAt: 999 });
  const pin = mk({ path: "/pin", name: "pin", openedAt: 400, pinned: true });
  const o1 = mk({ path: "/o1", name: "o1", openedAt: 300 });
  const part = partitionRecents([h, pin, o1]);
  expect(part.hero?.path === "/h", "partition: hero is the in-progress file");
  expect(part.pinned.length === 1 && part.pinned[0].path === "/pin", "partition: pinned list");
  expect(part.others.length === 1 && part.others[0].path === "/o1", "partition: others excludes hero + pinned");

  // A pinned hero appears in pinned but never in others.
  const ph = mk({ path: "/ph", name: "ph", openedAt: 600, pinned: true, lastPage: 1, totalPages: 8, lastReadAt: 999 });
  const part2 = partitionRecents([ph, o1]);
  expect(part2.hero?.path === "/ph", "partition: pinned in-progress can be hero");
  expect(part2.pinned.some((r) => r.path === "/ph"), "partition: pinned hero still in pinned strip");
  expect(!part2.others.some((r) => r.path === "/ph"), "partition: pinned hero not duplicated in others");

  // Null list -> all empty, no throw.
  const empty = partitionRecents(null as never);
  expect(empty.hero === null && empty.pinned.length === 0 && empty.others.length === 0, "partition: null -> all empty");
}

// =====================================================================
// Slice 2: recentMatches / filterRecents / highlightRecentName
// =====================================================================
{
  const f = mk({ path: "/Users/me/Downloads/report-2024.pdf", name: "report-2024.pdf" });

  // Empty query matches everything.
  expect(recentMatches(f, "") === true, "match: empty query -> true");
  expect(recentMatches(f, "   ") === true, "match: whitespace query -> true");

  // Basename fuzzy match.
  expect(recentMatches(f, "report") === true, "match: basename prefix");
  expect(recentMatches(f, "rpt") === true, "match: basename subsequence");
  expect(recentMatches(f, "2024") === true, "match: basename substring");

  // Path-only (folder) match — name doesn't contain "downloads".
  expect(recentMatches(f, "downloads") === true, "match: folder path match");
  expect(recentMatches(f, "zzzzz") === false, "match: no match -> false");
  expect(recentMatches(null as never, "x") === false, "match: null file -> false");

  // filterRecents preserves order, drops non-matches.
  const a = mk({ path: "/a/alpha.pdf", name: "alpha.pdf", openedAt: 3 });
  const b = mk({ path: "/b/beta.pdf", name: "beta.pdf", openedAt: 2 });
  const c = mk({ path: "/c/alphabet.pdf", name: "alphabet.pdf", openedAt: 1 });
  const filtered = filterRecents([a, b, c], "alph");
  expect(filtered.length === 2, "filter: 2 of 3 match 'alph'");
  expect(filtered[0].path === "/a/alpha.pdf" && filtered[1].path === "/c/alphabet.pdf", "filter: preserves input order");
  expect(filterRecents([a, b, c], "").length === 3, "filter: empty query -> all");
  expect(filterRecents(null as never, "x").length === 0, "filter: null -> []");
  expect(filterRecents([a, null as never, b], "beta").length === 1, "filter: drops nullish");

  // highlightRecentName: only name-matches highlight; path-only match leaves it plain.
  const segs = highlightRecentName("report-2024.pdf", "report");
  expect(segs.length >= 1 && segs.some((s) => s.hit && s.text.toLowerCase() === "report"), "highlight: marks the name hit");
  expect(segs.map((s) => s.text).join("") === "report-2024.pdf", "highlight: concatenation is lossless");
  const plainSegs = highlightRecentName("report-2024.pdf", "downloads");
  expect(plainSegs.length === 1 && plainSegs[0].hit === false, "highlight: path-only match leaves name plain");
  expect(highlightRecentName("", "x").length === 1, "highlight: empty name -> single seg");
  expect(highlightRecentName("file.pdf", "")[0].hit === false, "highlight: empty query -> plain");
}

// =====================================================================
// Slice 3: sort modes
// =====================================================================
{
  expect(RECENT_SORT_MODES.length === 4, "sort: 4 modes");
  expect(recentSortLabel("recent") === "Recent" && recentSortLabel("pages") === "Pages", "sort: labels");
  expect(describeRecentSort("name") === "file name", "sort: describe");
  expect(cycleRecentSort("recent") === "name", "sort: cycle recent -> name");
  expect(cycleRecentSort("pages") === "recent", "sort: cycle wraps pages -> recent");
  expect(cycleRecentSort("bogus" as never) === "recent", "sort: cycle garbage -> first");

  const a = mk({ path: "/a", name: "file10.pdf", openedAt: 100, lastPage: 1, totalPages: 10, pageCount: 10 });
  const b = mk({ path: "/b", name: "file2.pdf", openedAt: 300, lastPage: 9, totalPages: 10, pageCount: 10 });
  const c = mk({ path: "/c", name: "Apple.pdf", openedAt: 200, pageCount: 50 });

  // recent: openedAt desc.
  const byRecent = sortRecentView([a, b, c], "recent");
  expect(byRecent.map((x) => x.path).join() === "/b,/c,/a", "sort: recent = openedAt desc");

  // name: case-insensitive + numeric-aware (Apple < file2 < file10).
  const byName = sortRecentView([a, b, c], "name");
  expect(byName.map((x) => x.name).join() === "Apple.pdf,file2.pdf,file10.pdf", "sort: name numeric-aware + case-insensitive");

  // progress: furthest-read first (b 90% > a 10% > c 0%).
  const byProg = sortRecentView([a, b, c], "progress");
  expect(byProg[0].path === "/b" && byProg[1].path === "/a" && byProg[2].path === "/c", "sort: progress furthest-first");

  // pages: biggest first (c 50 > a/b 10), arrival tie-break for a vs b.
  const byPages = sortRecentView([a, b, c], "pages");
  expect(byPages[0].path === "/c", "sort: pages biggest first");
  expect(byPages[1].path === "/a" && byPages[2].path === "/b", "sort: pages stable arrival tie-break");

  // Does not mutate input.
  const orig = [a, b, c];
  sortRecentView(orig, "name");
  expect(orig[0].path === "/a", "sort: input not mutated");
  expect(sortRecentView(null as never, "name").length === 0, "sort: null -> []");
}

// =====================================================================
// Slice 4: keyboard nav
// =====================================================================
{
  const p1 = mk({ path: "/p1", name: "p1", pinned: true });
  const p2 = mk({ path: "/p2", name: "p2", pinned: true });
  const o1 = mk({ path: "/o1", name: "o1" });

  const flat = flattenRecentCards([p1, p2], [o1]);
  expect(flat.length === 3, "flat: 3 cards");
  expect(flat[0].section === "pinned" && flat[2].section === "others", "flat: pinned first then others");
  expect(flat[0].index === 0 && flat[2].index === 2, "flat: flat indices");
  expect(flattenRecentCards(null as never, null as never).length === 0, "flat: null -> []");
  expect(flattenRecentCards([p1, null as never], [o1]).length === 2, "flat: drops nullish");

  // classifyRecentKey.
  expect(classifyRecentKey({ key: "ArrowDown" })?.kind === "move", "key: ArrowDown -> move");
  const mv = classifyRecentKey({ key: "ArrowUp" });
  expect(mv?.kind === "move" && mv.intent === "prev", "key: ArrowUp -> move prev");
  expect(classifyRecentKey({ key: "Enter" })?.kind === "open", "key: Enter -> open");
  expect(classifyRecentKey({ key: "p" })?.kind === "pin", "key: p -> pin");
  expect(classifyRecentKey({ key: "P" })?.kind === "pin", "key: P -> pin");
  expect(classifyRecentKey({ key: "Backspace" })?.kind === "remove", "key: Backspace -> remove");
  expect(classifyRecentKey({ key: "Delete" })?.kind === "remove", "key: Delete -> remove");
  expect(classifyRecentKey({ key: "Escape" })?.kind === "clear", "key: Escape -> clear");
  expect(classifyRecentKey({ key: "x" }) === null, "key: typing -> null");
  // Modifiers fall through so app chords (Cmd+O, Cmd+0, Cmd+K) win.
  expect(classifyRecentKey({ key: "ArrowDown", metaKey: true }) === null, "key: Cmd+Arrow falls through");
  expect(classifyRecentKey({ key: "p", ctrlKey: true }) === null, "key: Ctrl+p falls through");
  expect(classifyRecentKey({ key: "Enter", altKey: true }) === null, "key: Alt+Enter falls through");
  expect(classifyRecentKey(null as never) === null, "key: null -> null");
  expect(classifyRecentKey({ key: 5 as never }) === null, "key: non-string -> null");

  // Slice 8: horizontal axis. Left/Right walk the flat cursor too, so the
  // pinned strip is keyboard-reachable along its natural axis.
  const right = classifyRecentKey({ key: "ArrowRight" });
  expect(right?.kind === "move" && right.intent === "next", "key: ArrowRight -> move next");
  const left = classifyRecentKey({ key: "ArrowLeft" });
  expect(left?.kind === "move" && left.intent === "prev", "key: ArrowLeft -> move prev");
  // Still modifier-gated so app chords win.
  expect(classifyRecentKey({ key: "ArrowRight", metaKey: true }) === null, "key: Cmd+Right falls through");
  expect(classifyRecentKey({ key: "ArrowLeft", altKey: true }) === null, "key: Alt+Left falls through");

  // Slice 8: per-section scroll alignment. Pinned scrolls horizontally
  // (inline center) so the next card peeks in; grid scrolls vertically.
  const pinScroll = recentCardScrollOptions("pinned");
  expect(pinScroll.inline === "center" && pinScroll.block === "nearest", "scroll: pinned -> inline center");
  const gridScroll = recentCardScrollOptions("others");
  expect(gridScroll.inline === "nearest" && gridScroll.block === "nearest", "scroll: others -> nearest");
  // Unknown section is safe.
  expect(recentCardScrollOptions("mystery").inline === "nearest", "scroll: unknown -> nearest fallback");

  // moveRecentCursor: unselected (-1) seeds first/last by direction.
  expect(moveRecentCursor("next", -1, 3) === 0, "cursor: next from -1 -> 0");
  expect(moveRecentCursor("prev", -1, 3) === 2, "cursor: prev from -1 -> last");
  expect(moveRecentCursor("last", -1, 3) === 2, "cursor: last from -1 -> last");
  expect(moveRecentCursor("next", 0, 3) === 1, "cursor: next 0 -> 1");
  expect(moveRecentCursor("next", 2, 3) === 0, "cursor: next wraps last -> 0");
  expect(moveRecentCursor("prev", 0, 3) === 2, "cursor: prev wraps 0 -> last");
  expect(moveRecentCursor("next", -1, 0) === -1, "cursor: empty -> -1");

  // clampRecentCursor.
  expect(clampRecentCursor(5, 3) === 2, "clamp: over -> last");
  expect(clampRecentCursor(1, 3) === 1, "clamp: in-range unchanged");
  expect(clampRecentCursor(-1, 3) === -1, "clamp: -1 stays unselected");
  expect(clampRecentCursor(2, 0) === -1, "clamp: empty -> -1");
}

// =====================================================================
// Slice 5: summarize footer
// =====================================================================
{
  expect(countInProgress([]) === 0, "count: empty -> 0");
  const mid = mk({ lastPage: 3, totalPages: 10 });
  const done = mk({ lastPage: 10, totalPages: 10 });
  const none = mk({});
  expect(countInProgress([mid, done, none]) === 1, "count: only mid-read counts");
  expect(countInProgress(null as never) === 0, "count: null -> 0");

  // Empty.
  expect(summarizeRecents({ total: 0, shown: 0, query: "", inProgress: 0, sort: "recent" }) === "No recent documents yet", "summary: empty");

  // Plain total, singular/plural.
  expect(summarizeRecents({ total: 1, shown: 1, query: "", inProgress: 0, sort: "recent" }) === "1 document", "summary: singular");
  expect(summarizeRecents({ total: 5, shown: 5, query: "", inProgress: 0, sort: "recent" }) === "5 documents", "summary: plural");

  // With filter.
  expect(
    summarizeRecents({ total: 12, shown: 3, query: "report", inProgress: 0, sort: "recent" }) === "3 of 12 documents matching \u201creport\u201d",
    "summary: filtered",
  );

  // In-progress + non-default sort appended.
  expect(
    summarizeRecents({ total: 5, shown: 5, query: "", inProgress: 2, sort: "name" }) === "5 documents \u00b7 2 in progress \u00b7 by file name",
    "summary: in-progress + sort suffixes",
  );

  // Default sort omitted.
  expect(
    summarizeRecents({ total: 5, shown: 5, query: "", inProgress: 0, sort: "recent" }) === "5 documents",
    "summary: default sort not shown",
  );

  // Garbage numbers tolerated.
  expect(summarizeRecents({ total: NaN, shown: NaN, query: "", inProgress: NaN, sort: "recent" } as never) === "No recent documents yet", "summary: NaN total -> empty");
}

// --- countUnpinned + describeClearUnpinned (Slice 6) ---
{
  const files = [
    mk({ path: "/a.pdf", pinned: true }),
    mk({ path: "/b.pdf", pinned: false }),
    mk({ path: "/c.pdf" }), // missing flag -> unpinned
    mk({ path: "/d.pdf", pinned: true }),
  ];
  expect(countUnpinned(files) === 2, "unpinned: counts non-pinned incl. missing flag");
  expect(countUnpinned([]) === 0, "unpinned: empty -> 0");
  // @ts-expect-error — garbage
  expect(countUnpinned(null) === 0, "unpinned: null -> 0");
  expect(
    countUnpinned([mk({ pinned: true }), mk({ pinned: true })]) === 0,
    "unpinned: all pinned -> 0",
  );
  // null entries are skipped, not counted.
  // @ts-expect-error — garbage entry
  expect(countUnpinned([null, mk({ pinned: false })]) === 1, "unpinned: skips null entries");

  expect(describeClearUnpinned(9) === "Clear 9 unpinned", "clearlabel: composes count");
  expect(describeClearUnpinned(1) === "Clear 1 unpinned", "clearlabel: count of one");
  expect(describeClearUnpinned(0) === "", "clearlabel: zero -> '' (hidden)");
  expect(describeClearUnpinned(-3) === "", "clearlabel: negative -> '' (hidden)");
  expect(describeClearUnpinned(NaN) === "", "clearlabel: NaN -> '' (hidden)");
}

// --- recentProgressBar (Slice 7) ---
{
  // Mid-read file -> visible bar with percent + label, not finished.
  const mid = recentProgressBar(mk({ lastPage: 12, totalPages: 80 }));
  expect(mid.show && mid.percent === 15 && !mid.finished, "bar: mid-read shows 15%");
  expect(mid.label === "p.12/80 · 15%", "bar: mid-read label matches chip");

  // Finished file -> full bar, finished flag, "Finished" label.
  const done = recentProgressBar(mk({ lastPage: 80, totalPages: 80 }));
  expect(done.show && done.finished && done.percent === 100, "bar: finished is full");
  expect(done.label === "Finished", "bar: finished label");

  // No usable progress -> hidden bar.
  const none = recentProgressBar(mk({ path: "/x.pdf" }));
  expect(!none.show && none.label === "", "bar: no progress -> hidden");
  // Only a page count, no last page -> still hidden.
  expect(!recentProgressBar(mk({ totalPages: 50 })).show, "bar: total without lastPage hidden");
  // Null / garbage -> hidden.
  expect(!recentProgressBar(null).show, "bar: null -> hidden");
  expect(!recentProgressBar(undefined).show, "bar: undefined -> hidden");
  expect(!recentProgressBar({} as RecentLike).show, "bar: empty object -> hidden");
}

// --- pinnedStripEdges -------------------------------------------------
{
  // Fits entirely (content == viewport) -> not overflowing, no edges.
  const fits = pinnedStripEdges({ scrollLeft: 0, scrollWidth: 400, clientWidth: 400 });
  expect(!fits.overflowing && !fits.atStart && !fits.atEnd, "strip: fits -> no affordance");

  // Overflowing, scrolled to the very start -> trailing edge only.
  const start = pinnedStripEdges({ scrollLeft: 0, scrollWidth: 1000, clientWidth: 400 });
  expect(start.overflowing && !start.atStart && start.atEnd, "strip: at start -> only trailing fade");

  // Scrolled into the middle -> both edges hide content.
  const mid = pinnedStripEdges({ scrollLeft: 300, scrollWidth: 1000, clientWidth: 400 });
  expect(mid.atStart && mid.atEnd, "strip: middle -> both fades");

  // Scrolled to the very end -> leading edge only (trailing clears).
  const end = pinnedStripEdges({ scrollLeft: 600, scrollWidth: 1000, clientWidth: 400 });
  expect(end.atStart && !end.atEnd, "strip: at end -> only leading fade");

  // Sub-pixel short of the end (rounding) still reads as "at end".
  const nearEnd = pinnedStripEdges({ scrollLeft: 599.4, scrollWidth: 1000, clientWidth: 400 });
  expect(!nearEnd.atEnd, "strip: within 1px of end -> trailing fade cleared");

  // Garbage / null is safe.
  expect(!pinnedStripEdges(null).overflowing, "strip: null -> no affordance");
  const junk = pinnedStripEdges({ scrollLeft: NaN, scrollWidth: NaN, clientWidth: NaN });
  expect(!junk.overflowing && !junk.atStart && !junk.atEnd, "strip: NaN -> no affordance");
  // Over-scrolled past the end is clamped (no phantom trailing fade).
  const over = pinnedStripEdges({ scrollLeft: 9999, scrollWidth: 1000, clientWidth: 400 });
  expect(over.atStart && !over.atEnd, "strip: over-scroll clamps to end");
}

// --- orderPinnedStrip + movePinned ------------------------------------
{
  // No pinOrder anywhere: incoming (store) order is preserved.
  {
    const a = mk({ path: "/a", pinned: true });
    const b = mk({ path: "/b", pinned: true });
    const c = mk({ path: "/c", pinned: true });
    const out = orderPinnedStrip([a, b, c]);
    expect(out.map((f) => f.path).join() === "/a,/b,/c", "order: no pinOrder -> store order");
  }

  // Fully stamped: ascending by pinOrder regardless of arrival order.
  {
    const a = mk({ path: "/a", pinned: true, pinOrder: 2 });
    const b = mk({ path: "/b", pinned: true, pinOrder: 0 });
    const c = mk({ path: "/c", pinned: true, pinOrder: 1 });
    const out = orderPinnedStrip([a, b, c]);
    expect(out.map((f) => f.path).join() === "/b,/c,/a", "order: ascending by pinOrder");
  }

  // pinOrder 0 is honoured (not treated as missing).
  {
    const a = mk({ path: "/a", pinned: true, pinOrder: 1 });
    const b = mk({ path: "/b", pinned: true, pinOrder: 0 });
    const out = orderPinnedStrip([a, b]);
    expect(out[0].path === "/b", "order: pinOrder 0 leads");
  }

  // Mixed: stamped cards lead unstamped, unstamped keep arrival order.
  {
    const a = mk({ path: "/a", pinned: true });
    const b = mk({ path: "/b", pinned: true, pinOrder: 5 });
    const c = mk({ path: "/c", pinned: true });
    const out = orderPinnedStrip([a, b, c]);
    expect(out.map((f) => f.path).join() === "/b,/a,/c", "order: stamped leads, unstamped stable");
  }

  // Stable tie-break on equal pinOrder.
  {
    const a = mk({ path: "/a", pinned: true, pinOrder: 3 });
    const b = mk({ path: "/b", pinned: true, pinOrder: 3 });
    const out = orderPinnedStrip([a, b]);
    expect(out[0].path === "/a" && out[1].path === "/b", "order: stable on equal pinOrder");
  }

  // Non-mutating + garbage tolerant.
  {
    const src = [mk({ path: "/a", pinned: true, pinOrder: 1 }), mk({ path: "/b", pinned: true, pinOrder: 0 })];
    orderPinnedStrip(src);
    expect(src[0].path === "/a", "order: input not mutated");
    expect(orderPinnedStrip([]).length === 0, "order: empty -> []");
    expect(orderPinnedStrip(null as unknown as RecentLike[]).length === 0, "order: null -> []");
  }

  // movePinned: drag a card to a new index -> new path order.
  {
    const list = [mk({ path: "/a" }), mk({ path: "/b" }), mk({ path: "/c" }), mk({ path: "/d" })];
    expect(movePinned(list, 0, 2).join() === "/b,/c,/a,/d", "move: first to index 2");
    expect(movePinned(list, 3, 0).join() === "/d,/a,/b,/c", "move: last to front");
    expect(movePinned(list, 1, 2).join() === "/a,/c,/b,/d", "move: adjacent swap");
  }

  // movePinned: no-op + out-of-range returns paths unchanged.
  {
    const list = [mk({ path: "/a" }), mk({ path: "/b" }), mk({ path: "/c" })];
    expect(movePinned(list, 1, 1).join() === "/a,/b,/c", "move: same index -> unchanged");
    expect(movePinned(list, -1, 0).join() === "/a,/b,/c", "move: negative from -> unchanged");
    expect(movePinned(list, 5, 0).join() === "/a,/b,/c", "move: from out of range -> unchanged");
    // `to` clamps into range rather than dropping the card.
    expect(movePinned(list, 0, 99).join() === "/b,/c,/a", "move: to over-range clamps to end");
  }

  // movePinned: garbage tolerant.
  expect(movePinned([], 0, 1).length === 0, "move: empty -> []");
  expect(movePinned(null as unknown as RecentLike[], 0, 1).length === 0, "move: null -> []");
}

// --- Slice 9: anyPinOrder + describeResetPinOrder ---------------------
{
  // anyPinOrder: true once any card carries a finite pinOrder stamp.
  const unstamped = [mk({ path: "/a" }), mk({ path: "/b" }), mk({ path: "/c" })];
  expect(anyPinOrder(unstamped) === false, "any-order: no stamps -> false");

  const stamped = [mk({ path: "/a", pinOrder: 1 }), mk({ path: "/b" })];
  expect(anyPinOrder(stamped) === true, "any-order: one stamp -> true");

  // A zero stamp is a valid order (index 0), not "no order".
  expect(anyPinOrder([mk({ path: "/a", pinOrder: 0 })]) === true, "any-order: pinOrder 0 counts");

  // Non-finite / garbage stamps don't count.
  expect(
    anyPinOrder([mk({ path: "/a", pinOrder: NaN as unknown as number })]) === false,
    "any-order: NaN stamp -> false",
  );
  expect(anyPinOrder([]) === false, "any-order: empty -> false");
  expect(anyPinOrder(null as unknown as RecentLike[]) === false, "any-order: null -> false");

  // describeResetPinOrder: hidden under 2 cards (nothing to reset), else
  // "Reset order (N)" with thousands grouping.
  expect(describeResetPinOrder(0) === "", "reset-label: zero -> '' (hidden)");
  expect(describeResetPinOrder(1) === "", "reset-label: one card -> '' (no order to undo)");
  expect(describeResetPinOrder(2) === "Reset order (2)", "reset-label: two cards");
  expect(describeResetPinOrder(12) === "Reset order (12)", "reset-label: many cards");
  expect(describeResetPinOrder(1500) === "Reset order (1,500)", "reset-label: thousands-grouped");
  expect(describeResetPinOrder(NaN) === "", "reset-label: NaN -> '' safe");
  expect(describeResetPinOrder(-3) === "", "reset-label: negative -> '' safe");
}

// --- isRecentSortMode (round 51 slice 3) -----------------------------
{
  // Every real mode validates.
  for (const m of RECENT_SORT_MODES) {
    expect(isRecentSortMode(m), `sort-guard: '${m}' is valid`);
  }
  // Unknown strings + garbage reject (so a stale/corrupt store decodes to default).
  expect(!isRecentSortMode("frecency"), "sort-guard: unknown mode -> false");
  expect(!isRecentSortMode("Recent"), "sort-guard: wrong case -> false");
  expect(!isRecentSortMode(""), "sort-guard: empty -> false");
  expect(!isRecentSortMode(null), "sort-guard: null -> false");
  expect(!isRecentSortMode(undefined), "sort-guard: undefined -> false");
  expect(!isRecentSortMode(0), "sort-guard: number -> false");
  expect(!isRecentSortMode({}), "sort-guard: object -> false");
  expect(!isRecentSortMode(["name"]), "sort-guard: array -> false");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
