// Pure-helper tests for the OCR Queue Panel view-core.
//
// Style matches beaconCacheView.test.ts / paletteSearch.test.ts — no
// test runner, just an inline `expect` so the contract reads at a glance.
//
// Run with:
//   tsx src/lib/ocrQueueView.test.ts

import {
  ocrBasename,
  ocrFolder,
  searchOcrDocs,
  sortOcrDocs,
  cycleOcrSort,
  ocrDefaultDir,
  ocrSortLabel,
  OCR_SORT_FIELDS,
  canonicalizeOcrError,
  groupFailureReasons,
  filterByReason,
  reconcileReasonFacet,
  describeDominantReason,
  collectReasonRetryIds,
  describeReasonRetry,
  groupPendingStates,
  filterByPendingState,
  reconcilePendingStateFacet,
  pendingStateLabel,
  flattenOcrRows,
  classifyOcrTableKey,
  nextOcrCursor,
  clampOcrCursor,
  summarizePending,
  describeOcrImpact,
  describeOcrView,
  OCR_REASON_UNKNOWN,
  type OcrDocLike,
  type OcrSort,
} from "./ocrQueueView";

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

/** Compact builder for a queue doc row. */
function doc(over: Partial<OcrDocLike> & { id: number; path: string }): OcrDocLike {
  return {
    title: null,
    pages: 1,
    ocr_state: "scanned",
    ocr_error: null,
    ...over,
  };
}

// --- ocrBasename / ocrFolder -----------------------------------------
{
  expect(ocrBasename("/a/b/c.pdf") === "c.pdf", "basename: posix path");
  expect(ocrBasename("C:\\docs\\scan.pdf") === "scan.pdf", "basename: windows path");
  expect(ocrBasename("loose.pdf") === "loose.pdf", "basename: bare filename");
  expect(ocrBasename("") === "", "basename: empty path -> empty");
  expect(ocrBasename("/trailing/") === "", "basename: trailing sep -> empty");

  expect(ocrFolder("/a/b/c.pdf") === "/a/b", "folder: posix dirname");
  expect(ocrFolder("C:\\docs\\scan.pdf") === "C:\\docs", "folder: windows dirname");
  expect(ocrFolder("loose.pdf") === "", "folder: bare filename -> empty");
  expect(ocrFolder("") === "", "folder: empty path -> empty");
}

// --- Slice 1: searchOcrDocs ------------------------------------------
{
  const docs = [
    doc({ id: 1, path: "/inbox/invoice-2024.pdf", ocr_state: "scanned" }),
    doc({ id: 2, path: "/archive/receipt.pdf", ocr_state: "mixed" }),
    doc({
      id: 3,
      path: "/inbox/scan_004.pdf",
      ocr_state: "ocr_failed",
      ocr_error: "tesseract: command not found",
    }),
  ];

  // Empty query passes all through unmarked, in input order.
  const all = searchOcrDocs(docs, "");
  expect(all.length === 3, "search: empty query keeps all rows");
  expect(all.every((h) => h.nameRanges.length === 0), "search: empty query no highlight");
  expect(all[0].record.id === 1 && all[2].record.id === 3, "search: empty query preserves order");

  // Basename match highlights.
  const inv = searchOcrDocs(docs, "invoice");
  expect(inv.length === 1 && inv[0].record.id === 1, "search: basename match filters");
  expect(inv[0].nameRanges.length > 0, "search: basename match has highlight ranges");

  // Folder match — silent (no name highlight), still surfaces the row.
  const arch = searchOcrDocs(docs, "archive");
  expect(arch.length === 1 && arch[0].record.id === 2, "search: folder match surfaces row");
  expect(arch[0].nameRanges.length === 0, "search: folder match no name highlight");

  // Error-reason match — type the root cause, surface the failed doc.
  const tess = searchOcrDocs(docs, "tesseract");
  expect(tess.length === 1 && tess[0].record.id === 3, "search: error-reason match surfaces failure");
  expect(tess[0].nameRanges.length === 0, "search: error match no name highlight");

  // State match.
  const mixed = searchOcrDocs(docs, "mixed");
  expect(mixed.length === 1 && mixed[0].record.id === 2, "search: ocr_state match surfaces row");

  // No match -> empty.
  expect(searchOcrDocs(docs, "zzzznope").length === 0, "search: no match -> empty");

  // Basename hit outranks a folder-only hit (weighting).
  const mixDocs = [
    doc({ id: 10, path: "/report/summary.pdf" }), // folder match on "report"
    doc({ id: 11, path: "/x/report.pdf" }), // basename match on "report"
  ];
  const reportHits = searchOcrDocs(mixDocs, "report");
  const byName = reportHits.find((h) => h.record.id === 11)!;
  const byFolder = reportHits.find((h) => h.record.id === 10)!;
  expect(byName.score > byFolder.score, "search: basename hit outranks folder hit");

  // Defensive: non-array / null rows.
  expect(searchOcrDocs(null as unknown as OcrDocLike[], "x").length === 0, "search: null list -> []");
}

// --- Slice 2: sort ----------------------------------------------------
{
  const docs = [
    doc({ id: 1, path: "/a/file10.pdf", pages: 5, ocr_state: "scanned" }),
    doc({ id: 2, path: "/a/file2.pdf", pages: 30, ocr_state: "mixed" }),
    doc({ id: 3, path: "/b/file2.pdf", pages: 1, ocr_state: "ocr_failed" }),
  ];

  // Name asc — numeric-aware: file2 < file10.
  const nameAsc = sortOcrDocs(docs, { field: "name", dir: "asc" });
  expect(
    nameAsc[0].id !== 1,
    "sort: name asc is numeric-aware (file2 before file10)",
  );
  const names = nameAsc.map((d) => ocrBasename(d.path));
  expect(names[2] === "file10.pdf", "sort: name asc puts file10 last");

  // Does not mutate input.
  expect(docs[0].id === 1, "sort: input array not mutated");

  // Pages desc — biggest first.
  const pagesDesc = sortOcrDocs(docs, { field: "pages", dir: "desc" });
  expect(pagesDesc[0].id === 2, "sort: pages desc biggest first");
  expect(pagesDesc[2].id === 3, "sort: pages desc smallest last");

  // Folder asc.
  const folderAsc = sortOcrDocs(docs, { field: "folder", dir: "asc" });
  expect(folderAsc[2].id === 3, "sort: folder asc puts /b after /a");

  // State asc — alphabetical (mixed < ocr_failed < scanned).
  const stateAsc = sortOcrDocs(docs, { field: "state", dir: "asc" });
  expect(stateAsc[0].ocr_state === "mixed", "sort: state asc alphabetical");

  // Stable id tie-break on equal primary (two file2 by name).
  const tie = sortOcrDocs(
    [doc({ id: 9, path: "/z/file2.pdf" }), doc({ id: 4, path: "/y/file2.pdf" })],
    { field: "name", dir: "asc" },
  );
  expect(tie[0].id === 4 && tie[1].id === 9, "sort: equal name falls back to id tie-break");

  // Null list -> [].
  expect(sortOcrDocs(null as unknown as OcrDocLike[], { field: "name", dir: "asc" }).length === 0, "sort: null list -> []");
}

// --- Slice 2: cycleOcrSort / defaults / labels -----------------------
{
  expect(ocrDefaultDir("name") === "asc", "default: name -> asc");
  expect(ocrDefaultDir("folder") === "asc", "default: folder -> asc");
  expect(ocrDefaultDir("state") === "asc", "default: state -> asc");
  expect(ocrDefaultDir("pages") === "desc", "default: pages -> desc");

  const start: OcrSort = { field: "name", dir: "asc" };
  // Clicking active column flips direction.
  expect(cycleOcrSort(start, "name").dir === "desc", "cycle: active column flips dir");
  // Clicking a new column switches at its natural default.
  expect(cycleOcrSort(start, "pages").field === "pages", "cycle: new column switches field");
  expect(cycleOcrSort(start, "pages").dir === "desc", "cycle: new column uses default dir");

  expect(OCR_SORT_FIELDS.length === 4, "fields: four sort columns");
  expect(ocrSortLabel("name") === "Name", "label: name");
  expect(ocrSortLabel("folder") === "Folder", "label: folder");
  expect(ocrSortLabel("pages") === "Pages", "label: pages");
  expect(ocrSortLabel("state") === "State", "label: state");
}

// --- Slice 3: canonicalizeOcrError ------------------------------------
{
  expect(canonicalizeOcrError(null) === OCR_REASON_UNKNOWN, "canon: null -> Unknown error");
  expect(canonicalizeOcrError("") === OCR_REASON_UNKNOWN, "canon: blank -> Unknown error");
  expect(canonicalizeOcrError("   ") === OCR_REASON_UNKNOWN, "canon: whitespace -> Unknown error");

  expect(
    canonicalizeOcrError("tesseract: command not found") === "Tesseract not installed",
    "canon: tesseract not found",
  );
  expect(
    canonicalizeOcrError("Error: tesseract is not on PATH") === "Tesseract not installed",
    "canon: tesseract not on PATH",
  );
  expect(
    canonicalizeOcrError("spawn tesseract ENOENT") === "Tesseract not installed",
    "canon: tesseract ENOENT",
  );
  expect(canonicalizeOcrError("OCR timed out after 60s") === "Timed out", "canon: timed out");
  expect(canonicalizeOcrError("operation timeout") === "Timed out", "canon: timeout");
  expect(
    canonicalizeOcrError("PDF is encrypted, password required") === "Encrypted PDF",
    "canon: encrypted",
  );
  expect(
    canonicalizeOcrError("malformed PDF: invalid xref") === "Damaged PDF",
    "canon: damaged",
  );
  expect(
    canonicalizeOcrError("failed to open document") === "Damaged PDF",
    "canon: failed to open -> damaged",
  );
  expect(
    canonicalizeOcrError("Permission denied (os error 13)") === "Permission denied",
    "canon: permission denied",
  );
  expect(
    canonicalizeOcrError("cannot allocate memory") === "Out of memory",
    "canon: out of memory",
  );
  expect(canonicalizeOcrError("ENOSPC: no space left") === "Disk full", "canon: disk full");

  // Unrecognized short error -> first line verbatim.
  expect(
    canonicalizeOcrError("Weird novel failure") === "Weird novel failure",
    "canon: unrecognized short error verbatim",
  );
  // Unrecognized multi-line -> first line only.
  expect(
    canonicalizeOcrError("headline reason\nstack frame 1\nstack frame 2") === "headline reason",
    "canon: multi-line uses first line",
  );
  // Unrecognized long -> truncated with ellipsis.
  const long = "x".repeat(100);
  const canonLong = canonicalizeOcrError(long);
  expect(canonLong.length === 58 && canonLong.endsWith("\u2026"), "canon: long error truncated to 58 with ellipsis");
}

// --- Slice 3: groupFailureReasons / filterByReason -------------------
{
  const failed = [
    doc({ id: 1, path: "/a.pdf", ocr_state: "ocr_failed", ocr_error: "tesseract not found" }),
    doc({ id: 2, path: "/b.pdf", ocr_state: "ocr_failed", ocr_error: "spawn tesseract ENOENT" }),
    doc({ id: 3, path: "/c.pdf", ocr_state: "ocr_failed", ocr_error: "PDF encrypted" }),
    doc({ id: 4, path: "/d.pdf", ocr_state: "ocr_failed", ocr_error: null }),
  ];

  const buckets = groupFailureReasons(failed);
  expect(buckets.length === 3, "group: three distinct reason buckets");
  expect(buckets[0].reason === "Tesseract not installed", "group: dominant reason first");
  expect(buckets[0].count === 2, "group: dominant reason count");
  // Ties broken alphabetically — Encrypted PDF before Unknown error.
  expect(
    buckets[1].reason === "Encrypted PDF" && buckets[2].reason === OCR_REASON_UNKNOWN,
    "group: equal-count buckets sorted alphabetically",
  );
  expect(groupFailureReasons([]).length === 0, "group: empty -> []");
  expect(groupFailureReasons(null as unknown as OcrDocLike[]).length === 0, "group: null -> []");

  // Facet filter — only the tesseract failures.
  const tessOnly = filterByReason(failed, "Tesseract not installed");
  expect(tessOnly.length === 2 && tessOnly.every((d) => d.id === 1 || d.id === 2), "facet: filters to one reason");
  // Null reason passes all through (new array).
  const allRows = filterByReason(failed, null);
  expect(allRows.length === 4 && allRows !== failed, "facet: null reason returns all, new array");
  expect(filterByReason(null as unknown as OcrDocLike[], "x").length === 0, "facet: null list -> []");
}

// --- Slice 3: reconcileReasonFacet / describeDominantReason ----------
{
  const buckets = [
    { reason: "Tesseract not installed", count: 5 },
    { reason: "Encrypted PDF", count: 2 },
  ];
  expect(reconcileReasonFacet("Encrypted PDF", buckets) === "Encrypted PDF", "reconcile: live facet kept");
  expect(reconcileReasonFacet("Gone reason", buckets) === null, "reconcile: stale facet dropped");
  expect(reconcileReasonFacet(null, buckets) === null, "reconcile: null facet -> null");
  expect(reconcileReasonFacet("x", null as unknown as []) === null, "reconcile: null buckets -> null");

  expect(
    describeDominantReason(buckets) === "Most failures: Tesseract not installed (5)",
    "dominant: names top reason with count",
  );
  expect(
    describeDominantReason([{ reason: "Encrypted PDF", count: 3 }]) === "All failures: Encrypted PDF",
    "dominant: single bucket drops count",
  );
  expect(describeDominantReason([]) === "", "dominant: empty -> empty string");
}

// --- Slice 4: flattenOcrRows -----------------------------------------
{
  const failed = [doc({ id: 1, path: "/f1.pdf" }), doc({ id: 2, path: "/f2.pdf" })];
  const pending = [doc({ id: 3, path: "/p1.pdf" })];
  const flat = flattenOcrRows(failed, pending);
  expect(flat.length === 3, "flatten: spans both lists");
  expect(flat[0].section === "failed" && flat[1].section === "failed", "flatten: failures first");
  expect(flat[2].section === "pending" && flat[2].record.id === 3, "flatten: pending after failures");
  expect(flattenOcrRows([], []).length === 0, "flatten: both empty -> []");
  expect(
    flattenOcrRows(null as unknown as OcrDocLike[], pending).length === 1,
    "flatten: null failed tolerated",
  );
}

// --- Slice 4: classifyOcrTableKey ------------------------------------
{
  expect(classifyOcrTableKey({ key: "ArrowDown" })?.kind === "move", "key: ArrowDown -> move");
  const up = classifyOcrTableKey({ key: "ArrowUp" });
  expect(up?.kind === "move" && up.intent === "prev", "key: ArrowUp -> move prev");
  expect(classifyOcrTableKey({ key: "Enter" })?.kind === "activate", "key: Enter -> activate");
  expect(classifyOcrTableKey({ key: "o" })?.kind === "open", "key: o -> open");
  expect(classifyOcrTableKey({ key: "O" })?.kind === "open", "key: O -> open");
  expect(classifyOcrTableKey({ key: "Escape" })?.kind === "clear", "key: Escape -> clear");
  // Modifier chords fall through (app/OS owns them).
  expect(classifyOcrTableKey({ key: "Enter", metaKey: true }) === null, "key: Cmd+Enter falls through");
  expect(classifyOcrTableKey({ key: "ArrowDown", ctrlKey: true }) === null, "key: Ctrl+Arrow falls through");
  expect(classifyOcrTableKey({ key: "o", altKey: true }) === null, "key: Alt+o falls through");
  // Plain text key is not a queue key.
  expect(classifyOcrTableKey({ key: "x" }) === null, "key: bare letter -> null");
  expect(classifyOcrTableKey(null as unknown as { key: string }) === null, "key: null event -> null");
}

// --- Slice 4: nextOcrCursor / clampOcrCursor -------------------------
{
  expect(nextOcrCursor("next", 4, 5) === 0, "cursor: next wraps past end to 0");
  expect(nextOcrCursor("prev", 0, 5) === 4, "cursor: prev wraps before start to last");
  expect(nextOcrCursor("next", 1, 5) === 2, "cursor: next steps forward");
  expect(nextOcrCursor("first", 3, 5) === 0, "cursor: first -> 0");
  expect(nextOcrCursor("last", 1, 5) === 4, "cursor: last -> last index");
  expect(nextOcrCursor("page-down", 0, 5) === 4, "cursor: page-down clamps to last");
  expect(nextOcrCursor("page-up", 4, 5) === 0, "cursor: page-up clamps to 0");
  expect(nextOcrCursor("next", 0, 0) === 0, "cursor: empty list -> 0");

  expect(clampOcrCursor(40, 10) === 9, "clamp: out-of-range snaps to last");
  expect(clampOcrCursor(3, 10) === 3, "clamp: in-range preserved");
  expect(clampOcrCursor(5, 0) === 0, "clamp: empty list -> 0");
  expect(clampOcrCursor(-2, 10) === 0, "clamp: negative floored to 0");
  expect(clampOcrCursor(NaN, 10) === 0, "clamp: NaN -> 0");
  expect(clampOcrCursor(2.9, 10) === 2, "clamp: fractional floored");
}

// --- Slice 5: summarizePending / describeOcrImpact -------------------
{
  const pending = [
    doc({ id: 1, path: "/a.pdf", pages: 10 }),
    doc({ id: 2, path: "/b.pdf", pages: 5 }),
    doc({ id: 3, path: "/c.pdf", pages: null }),
  ];
  const impact = summarizePending(pending);
  expect(impact.docs === 3, "impact: counts all docs");
  expect(impact.pages === 15, "impact: sums pages, null contributes 0");
  expect(summarizePending([]).docs === 0, "impact: empty -> zero docs");
  expect(summarizePending(null as unknown as OcrDocLike[]).pages === 0, "impact: null list -> zero");

  expect(describeOcrImpact(impact) === "3 docs \u00b7 15 pages", "describe: docs and pages");
  expect(
    describeOcrImpact({ docs: 1, pages: 1 }) === "1 doc \u00b7 1 page",
    "describe: singular forms",
  );
  expect(
    describeOcrImpact({ docs: 4, pages: 0 }) === "4 docs",
    "describe: zero pages drops out",
  );
  expect(describeOcrImpact({ docs: 0, pages: 0 }) === "Queue empty", "describe: empty -> Queue empty");
  // Thousands grouping.
  expect(
    describeOcrImpact({ docs: 1200, pages: 3400 }) === "1,200 docs \u00b7 3,400 pages",
    "describe: thousands grouped",
  );
}

// --- Slice 5: describeOcrView ----------------------------------------
{
  // Unfiltered: both buckets named.
  expect(
    describeOcrView({
      shownFailed: 3,
      shownPending: 12,
      totalFailed: 3,
      totalPending: 12,
      inFlight: 0,
      reasonFacet: null,
      query: "",
    }) === "3 failed \u00b7 12 pending",
    "view: unfiltered names both buckets",
  );

  // Only pending.
  expect(
    describeOcrView({
      shownFailed: 0,
      shownPending: 8,
      totalFailed: 0,
      totalPending: 8,
      inFlight: 0,
      reasonFacet: null,
      query: "",
    }) === "8 pending",
    "view: only pending bucket",
  );

  // In flight appended.
  expect(
    describeOcrView({
      shownFailed: 2,
      shownPending: 5,
      totalFailed: 2,
      totalPending: 5,
      inFlight: 1,
      reasonFacet: null,
      query: "",
    }) === "2 failed \u00b7 5 pending \u00b7 1 in flight",
    "view: in-flight appended",
  );

  // Filtering by reason facet.
  expect(
    describeOcrView({
      shownFailed: 2,
      shownPending: 0,
      totalFailed: 5,
      totalPending: 3,
      inFlight: 0,
      reasonFacet: "Tesseract not installed",
      query: "",
    }) === "2 of 8 matching Tesseract not installed",
    "view: reason facet narrows",
  );

  // Filtering by query + facet (both narrows).
  expect(
    describeOcrView({
      shownFailed: 1,
      shownPending: 0,
      totalFailed: 5,
      totalPending: 3,
      inFlight: 0,
      reasonFacet: "Encrypted PDF",
      query: "invoice",
    }) === "1 of 8 matching Encrypted PDF + \u201cinvoice\u201d",
    "view: facet + query narrows",
  );

  // Empty queue.
  expect(
    describeOcrView({
      shownFailed: 0,
      shownPending: 0,
      totalFailed: 0,
      totalPending: 0,
      inFlight: 0,
      reasonFacet: null,
      query: "",
    }) === "Queue empty",
    "view: empty queue",
  );

  // Empty list but in flight.
  expect(
    describeOcrView({
      shownFailed: 0,
      shownPending: 0,
      totalFailed: 0,
      totalPending: 0,
      inFlight: 4,
      reasonFacet: null,
      query: "",
    }) === "4 in flight",
    "view: empty but in flight",
  );
}

// --- Slice 3b: pending-state facet -----------------------------------
{
  const pending = [
    doc({ id: 1, path: "/a/scan1.pdf", ocr_state: "scanned" }),
    doc({ id: 2, path: "/a/scan2.pdf", ocr_state: "scanned" }),
    doc({ id: 3, path: "/a/mix1.pdf", ocr_state: "mixed" }),
    doc({ id: 4, path: "/a/odd.pdf", ocr_state: "" }),
  ];

  // Labels map the two real states; unknown/blank pass through sensibly.
  expect(pendingStateLabel("scanned") === "Image-only", "pstate: scanned label");
  expect(pendingStateLabel("mixed") === "Mixed pages", "pstate: mixed label");
  expect(pendingStateLabel("") === "Unknown", "pstate: blank -> Unknown");
  expect(pendingStateLabel("weird") === "weird", "pstate: novel passes through");

  // Grouping: dominant state first, blank bucketed under 'unknown'.
  const buckets = groupPendingStates(pending);
  expect(buckets.length === 3, "pstate: three buckets");
  expect(buckets[0].state === "scanned" && buckets[0].count === 2, "pstate: scanned dominant");
  expect(buckets.some((b) => b.state === "unknown" && b.count === 1), "pstate: blank -> unknown bucket");
  expect(groupPendingStates([]).length === 0, "pstate: empty -> []");
  // @ts-expect-error — garbage
  expect(groupPendingStates(null).length === 0, "pstate: null -> []");

  // Tie-break is alphabetical by state when counts equal.
  const tie = groupPendingStates([
    doc({ id: 1, path: "/x.pdf", ocr_state: "mixed" }),
    doc({ id: 2, path: "/y.pdf", ocr_state: "scanned" }),
  ]);
  expect(tie[0].state === "mixed" && tie[1].state === "scanned", "pstate: equal counts sort alpha");

  // Filtering to one state; null passes through; blank maps to unknown.
  expect(filterByPendingState(pending, "scanned").length === 2, "pstate: filter scanned");
  expect(filterByPendingState(pending, "mixed").map((d) => d.id).join() === "3", "pstate: filter mixed");
  expect(filterByPendingState(pending, "unknown").map((d) => d.id).join() === "4", "pstate: filter unknown (blank)");
  expect(filterByPendingState(pending, null).length === 4, "pstate: null facet keeps all");
  // Returns a fresh array, never the input reference.
  expect(filterByPendingState(pending, null) !== pending, "pstate: null facet returns a copy");
  // @ts-expect-error — garbage
  expect(filterByPendingState(null, "scanned").length === 0, "pstate: null list -> []");

  // Reconcile: live facet survives, vanished facet clears.
  expect(reconcilePendingStateFacet("scanned", buckets) === "scanned", "pstate: live facet kept");
  expect(reconcilePendingStateFacet("gone", buckets) === null, "pstate: vanished facet cleared");
  expect(reconcilePendingStateFacet(null, buckets) === null, "pstate: null facet stays null");
}

// --- Slice 3c: per-reason "Retry all <reason>" -----------------------
{
  const failed = [
    doc({ id: 1, path: "/a/x.pdf", ocr_state: "ocr_failed", ocr_error: "tesseract: command not found" }),
    doc({ id: 2, path: "/a/y.pdf", ocr_state: "ocr_failed", ocr_error: "tesseract not on PATH" }),
    doc({ id: 3, path: "/a/z.pdf", ocr_state: "ocr_failed", ocr_error: "PDF is encrypted" }),
    doc({ id: 4, path: "/a/w.pdf", ocr_state: "ocr_failed", ocr_error: "timed out after 60s" }),
  ];

  // Collects exactly the ids of the faceted bucket, in input order.
  const tess = collectReasonRetryIds(failed, "Tesseract not installed");
  expect(tess.join() === "1,2", "retry-reason: collects the two tesseract ids in order");
  expect(collectReasonRetryIds(failed, "Encrypted PDF").join() === "3", "retry-reason: encrypted bucket");
  expect(collectReasonRetryIds(failed, "Timed out").join() === "4", "retry-reason: timeout bucket");
  // A reason no failure wears -> [].
  expect(collectReasonRetryIds(failed, "Disk full").length === 0, "retry-reason: absent reason -> []");
  // Null/blank reason -> [] (the blanket Retry all covers everything).
  expect(collectReasonRetryIds(failed, null).length === 0, "retry-reason: null reason -> []");
  expect(collectReasonRetryIds(failed, "").length === 0, "retry-reason: blank reason -> []");
  // The collected set matches filterByReason's membership exactly.
  const viaFilter = filterByReason(failed, "Tesseract not installed").map((d) => d.id);
  expect(viaFilter.join() === tess.join(), "retry-reason: matches filterByReason membership");
  // @ts-expect-error — garbage list
  expect(collectReasonRetryIds(null, "Timed out").length === 0, "retry-reason: null list -> []");

  // Label: names the reason + thousands-grouped count.
  expect(
    describeReasonRetry("Tesseract not installed", 2) === "Retry 2 \u00b7 Tesseract not installed",
    "retry-reason: label names reason + count",
  );
  expect(
    describeReasonRetry("Timed out", 1500) === "Retry 1,500 \u00b7 Timed out",
    "retry-reason: label thousands-groups the count",
  );
  // Hidden when there is nothing to retry or no facet.
  expect(describeReasonRetry("Encrypted PDF", 0) === "", "retry-reason: zero count -> hidden");
  expect(describeReasonRetry(null, 5) === "", "retry-reason: null reason -> hidden");
  expect(describeReasonRetry("X", -3) === "", "retry-reason: negative count -> hidden");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
