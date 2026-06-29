// Pure-helper tests for the Beacon Cache Inspector view-core.
//
// Style matches paletteSearch.test.ts / shortcutsOverlay.test.ts — no
// test runner, just an inline `expect` so the contract reads at a
// glance.
//
// Run with:
//   tsx src/lib/beaconCacheView.test.ts

import {
  beaconBasename,
  searchIndexedPdfs,
  sortIndexedPdfs,
  cycleBeaconSort,
  beaconDefaultDir,
  beaconSortLabel,
  filterByModel,
  filterDeadWeight,
  deadWeightImpact,
  dominantModel,
  reconcileModelFacet,
  isModelPinned,
  toggleModelPin,
  livePinnedModels,
  movePinnedModel,
  summarizeSelection,
  describeImpact,
  describeBeaconView,
  classifyBeaconTableKey,
  nextBeaconCursor,
  clampBeaconCursor,
  classifyPinReorderKey,
  nextPinIndex,
  BEACON_SORT_FIELDS,
  type BeaconPdfLike,
  type BeaconSort,
  type BeaconKeyEvent,
} from "./beaconCacheView";

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
const pdf = (over: Partial<BeaconPdfLike> = {}): BeaconPdfLike => ({
  pdf_hash: over.pdf_hash ?? `hash${seq++}`,
  pdf_path: over.pdf_path ?? `/docs/file${seq}.pdf`,
  pages: over.pages ?? 10,
  embed_model: over.embed_model ?? "nomic-embed-text",
  indexed_at: over.indexed_at ?? 1_700_000_000,
  chunks: over.chunks ?? 5,
});

// --- beaconBasename --------------------------------------------------
{
  expect(beaconBasename("/a/b/c.pdf") === "c.pdf", "basename: posix path");
  expect(
    beaconBasename("C:\\Users\\x\\report.pdf") === "report.pdf",
    "basename: windows path",
  );
  expect(beaconBasename("bare.pdf") === "bare.pdf", "basename: no separator");
  expect(beaconBasename("") === "", "basename: empty -> empty");
  expect(beaconBasename("/trailing/") === "", "basename: trailing slash -> empty");
  expect(
    beaconBasename("/mixed\\sep/deep\\name.pdf") === "name.pdf",
    "basename: mixed separators take the last of either",
  );
}

// --- searchIndexedPdfs: empty query passes all -----------------------
{
  const rows = [pdf({ pdf_path: "/a/one.pdf" }), pdf({ pdf_path: "/a/two.pdf" })];
  const all = searchIndexedPdfs(rows, "");
  expect(all.length === 2, "search: empty query keeps every row");
  expect(all.every((h) => h.score === 1), "search: empty query neutral score");
  expect(all.every((h) => h.nameRanges.length === 0), "search: empty query no marks");
  // Whitespace-only is treated as empty.
  expect(searchIndexedPdfs(rows, "   ").length === 2, "search: blank query keeps all");
  // Order preserved.
  expect(
    all[0].record.pdf_path === "/a/one.pdf" && all[1].record.pdf_path === "/a/two.pdf",
    "search: empty query preserves input order",
  );
}

// --- searchIndexedPdfs: basename match + highlight -------------------
{
  const rows = [
    pdf({ pdf_path: "/docs/Invoice-2024.pdf" }),
    pdf({ pdf_path: "/docs/Contract.pdf" }),
    pdf({ pdf_path: "/docs/notes.txt.pdf" }),
  ];
  const hits = searchIndexedPdfs(rows, "invoice");
  expect(hits.length === 1, "search: basename narrows to the match");
  expect(hits[0].record.pdf_path === "/docs/Invoice-2024.pdf", "search: right row");
  expect(hits[0].nameRanges.length > 0, "search: basename hit carries highlight ranges");
  // The highlighted span maps onto the basename, not the full path.
  const name = "Invoice-2024.pdf";
  const r = hits[0].nameRanges[0];
  expect(
    name.slice(r.start, r.end).toLowerCase() === "invoice",
    "search: highlight range indexes into the basename",
  );
}

// --- searchIndexedPdfs: folder/path match, no name highlight ---------
{
  const rows = [
    pdf({ pdf_path: "/Taxes/2024/return.pdf" }),
    pdf({ pdf_path: "/Photos/cat.pdf" }),
  ];
  const hits = searchIndexedPdfs(rows, "taxes");
  expect(hits.length === 1, "search: matches on folder portion of path");
  expect(hits[0].record.pdf_path === "/Taxes/2024/return.pdf", "search: folder match row");
  expect(
    hits[0].nameRanges.length === 0,
    "search: folder-only hit paints NO marks on the basename",
  );
}

// --- searchIndexedPdfs: model + hash secondary fields ----------------
{
  const rows = [
    pdf({ pdf_path: "/a/x.pdf", embed_model: "mxbai-embed-large", pdf_hash: "deadbeef01" }),
    pdf({ pdf_path: "/a/y.pdf", embed_model: "nomic-embed-text", pdf_hash: "cafef00d99" }),
  ];
  const byModel = searchIndexedPdfs(rows, "mxbai");
  expect(byModel.length === 1 && byModel[0].record.pdf_path === "/a/x.pdf", "search: model field");
  expect(byModel[0].nameRanges.length === 0, "search: model hit -> no name marks");
  const byHash = searchIndexedPdfs(rows, "cafef00d");
  expect(byHash.length === 1 && byHash[0].record.pdf_path === "/a/y.pdf", "search: hash field");
}

// --- searchIndexedPdfs: name outranks folder (weighting) -------------
{
  const rows = [
    pdf({ pdf_path: "/report/old.pdf" }), // "report" in folder only
    pdf({ pdf_path: "/x/report.pdf" }), // "report" in basename
  ];
  const hits = searchIndexedPdfs(rows, "report");
  expect(hits.length === 2, "search: both rows match 'report'");
  const nameHit = hits.find((h) => h.record.pdf_path === "/x/report.pdf");
  const folderHit = hits.find((h) => h.record.pdf_path === "/report/old.pdf");
  expect(
    !!nameHit && !!folderHit && nameHit.score > folderHit.score,
    "search: a basename hit outscores a folder-only hit",
  );
}

// --- searchIndexedPdfs: no match + defensive -------------------------
{
  const rows = [pdf({ pdf_path: "/a/one.pdf" })];
  expect(searchIndexedPdfs(rows, "zzzzz").length === 0, "search: no match -> empty");
  // @ts-expect-error null tolerance
  expect(searchIndexedPdfs(null, "x").length === 0, "search: null list -> empty");
  expect(searchIndexedPdfs([], "x").length === 0, "search: empty list -> empty");
  // A null row in the list is skipped, not thrown on.
  const dirty = [pdf({ pdf_path: "/a/keep.pdf" }), null as unknown as BeaconPdfLike];
  expect(searchIndexedPdfs(dirty, "keep").length === 1, "search: skips null rows");
}

// --- beaconSortLabel + fields + defaults -----------------------------
{
  expect(BEACON_SORT_FIELDS.length === 5, "sort: five sortable fields");
  expect(
    new Set(BEACON_SORT_FIELDS).size === BEACON_SORT_FIELDS.length,
    "sort: field list has no duplicates",
  );
  expect(beaconSortLabel("name") === "Name", "sort: name label");
  expect(beaconSortLabel("indexed") === "Indexed", "sort: indexed label");
  expect(beaconDefaultDir("name") === "asc", "sort: name defaults A->Z");
  expect(beaconDefaultDir("model") === "asc", "sort: model defaults A->Z");
  expect(beaconDefaultDir("chunks") === "desc", "sort: chunks defaults biggest-first");
  expect(beaconDefaultDir("pages") === "desc", "sort: pages defaults biggest-first");
  expect(beaconDefaultDir("indexed") === "desc", "sort: indexed defaults newest-first");
}

// --- cycleBeaconSort: flip vs switch ---------------------------------
{
  const start: BeaconSort = { field: "indexed", dir: "desc" };
  // Click the active column -> flip direction.
  const flipped = cycleBeaconSort(start, "indexed");
  expect(
    flipped.field === "indexed" && flipped.dir === "asc",
    "cycle: clicking active column flips direction",
  );
  const flippedBack = cycleBeaconSort(flipped, "indexed");
  expect(flippedBack.dir === "desc", "cycle: flips back on second click");
  // Click a different column -> switch at its natural default.
  const switched = cycleBeaconSort(start, "name");
  expect(
    switched.field === "name" && switched.dir === "asc",
    "cycle: switching column uses that field's default direction",
  );
  const toChunks = cycleBeaconSort(start, "chunks");
  expect(toChunks.dir === "desc", "cycle: switching to chunks defaults desc");
}

// --- sortIndexedPdfs: name (case-insensitive, numeric-aware) ---------
{
  const rows = [
    pdf({ pdf_path: "/x/banana.pdf", pdf_hash: "h1" }),
    pdf({ pdf_path: "/x/Apple.pdf", pdf_hash: "h2" }),
    pdf({ pdf_path: "/x/cherry.pdf", pdf_hash: "h3" }),
  ];
  const asc = sortIndexedPdfs(rows, { field: "name", dir: "asc" });
  expect(
    asc.map((r) => beaconBasename(r.pdf_path)).join(",") === "Apple.pdf,banana.pdf,cherry.pdf",
    "sort: name asc is case-insensitive A->Z",
  );
  const desc = sortIndexedPdfs(rows, { field: "name", dir: "desc" });
  expect(
    beaconBasename(desc[0].pdf_path) === "cherry.pdf",
    "sort: name desc reverses",
  );
  // Numeric-aware: file2 before file10.
  const nums = [
    pdf({ pdf_path: "/x/file10.pdf", pdf_hash: "n10" }),
    pdf({ pdf_path: "/x/file2.pdf", pdf_hash: "n2" }),
  ];
  const numAsc = sortIndexedPdfs(nums, { field: "name", dir: "asc" });
  expect(
    beaconBasename(numAsc[0].pdf_path) === "file2.pdf",
    "sort: name asc is numeric-aware (file2 < file10)",
  );
}

// --- sortIndexedPdfs: numeric fields + tie-break ---------------------
{
  const rows = [
    pdf({ chunks: 5, pages: 3, indexed_at: 100, pdf_hash: "a" }),
    pdf({ chunks: 20, pages: 1, indexed_at: 300, pdf_hash: "b" }),
    pdf({ chunks: 12, pages: 9, indexed_at: 200, pdf_hash: "c" }),
  ];
  expect(
    sortIndexedPdfs(rows, { field: "chunks", dir: "desc" }).map((r) => r.chunks).join() === "20,12,5",
    "sort: chunks desc",
  );
  expect(
    sortIndexedPdfs(rows, { field: "pages", dir: "asc" }).map((r) => r.pages).join() === "1,3,9",
    "sort: pages asc",
  );
  expect(
    sortIndexedPdfs(rows, { field: "indexed", dir: "desc" }).map((r) => r.indexed_at).join() === "300,200,100",
    "sort: indexed desc (newest first)",
  );
  // Stable tie-break: equal chunks -> ascending hash regardless of dir.
  const tied = [
    pdf({ chunks: 7, pdf_hash: "zzz" }),
    pdf({ chunks: 7, pdf_hash: "aaa" }),
  ];
  const tiedDesc = sortIndexedPdfs(tied, { field: "chunks", dir: "desc" });
  expect(
    tiedDesc[0].pdf_hash === "aaa" && tiedDesc[1].pdf_hash === "zzz",
    "sort: equal rows break ties by ascending hash (stable, dir-independent)",
  );
}

// --- sortIndexedPdfs: purity + defensive -----------------------------
{
  const rows = [pdf({ chunks: 2 }), pdf({ chunks: 9 })];
  const snapshot = rows.map((r) => r.chunks).join();
  sortIndexedPdfs(rows, { field: "chunks", dir: "desc" });
  expect(rows.map((r) => r.chunks).join() === snapshot, "sort: input array not mutated");
  // @ts-expect-error null tolerance
  expect(sortIndexedPdfs(null, { field: "name", dir: "asc" }).length === 0, "sort: null -> []");
  expect(sortIndexedPdfs([], { field: "name", dir: "asc" }).length === 0, "sort: empty -> []");
}

// --- filterByModel ---------------------------------------------------
{
  const rows = [
    pdf({ pdf_hash: "a", embed_model: "nomic-embed-text" }),
    pdf({ pdf_hash: "b", embed_model: "mxbai-embed-large" }),
    pdf({ pdf_hash: "c", embed_model: "nomic-embed-text" }),
  ];
  const nomic = filterByModel(rows, "nomic-embed-text");
  expect(nomic.length === 2, "facet: filters to the chosen model");
  expect(
    nomic.every((r) => r.embed_model === "nomic-embed-text"),
    "facet: every kept row matches the model",
  );
  const mxbai = filterByModel(rows, "mxbai-embed-large");
  expect(mxbai.length === 1 && mxbai[0].pdf_hash === "b", "facet: other model isolates one row");
  // No facet -> passthrough (new array, not the same ref).
  const all = filterByModel(rows, null);
  expect(all.length === 3, "facet: null model passes all rows");
  expect(all !== rows, "facet: passthrough returns a new array, not the input ref");
  expect(filterByModel(rows, "").length === 3, "facet: empty model passes all rows");
  // Unknown model -> empty.
  expect(filterByModel(rows, "ghost-model").length === 0, "facet: unknown model -> empty");
  // Defensive.
  // @ts-expect-error null tolerance
  expect(filterByModel(null, "x").length === 0, "facet: null list -> []");
}

// --- dominantModel + filterDeadWeight --------------------------------
{
  const mixed = [
    pdf({ pdf_hash: "a", embed_model: "nomic-embed-text", chunks: 100 }),
    pdf({ pdf_hash: "b", embed_model: "mxbai-embed-large", chunks: 20 }),
    pdf({ pdf_hash: "c", embed_model: "nomic-embed-text", chunks: 80 }),
  ];
  expect(dominantModel(mixed) === "nomic-embed-text", "dominant: most-chunks model wins");
  const dead = filterDeadWeight(mixed);
  expect(dead.length === 1 && dead[0].pdf_hash === "b", "deadweight: only non-dominant rows");
  expect(dead !== mixed, "deadweight: new array");
  // Single-model index: nothing is dead weight.
  const single = [pdf({ pdf_hash: "a", embed_model: "nomic-embed-text", chunks: 5 })];
  expect(dominantModel(single) === null, "dominant: single model -> null");
  expect(filterDeadWeight(single).length === 0, "deadweight: single model -> []");
  expect(filterDeadWeight([]).length === 0, "deadweight: empty -> []");
  // @ts-expect-error null tolerance
  expect(dominantModel(null) === null, "dominant: null -> null");
}

// --- deadWeightImpact ------------------------------------------------
{
  const mixed = [
    pdf({ pdf_hash: "a", embed_model: "nomic-embed-text", chunks: 100, pages: 12 }),
    pdf({ pdf_hash: "b", embed_model: "mxbai-embed-large", chunks: 20, pages: 8 }),
    pdf({ pdf_hash: "c", embed_model: "mxbai-embed-large", chunks: 15, pages: 4 }),
  ];
  const imp = deadWeightImpact(mixed);
  expect(imp.pdfs === 2, "dw-impact: sums non-dominant PDFs");
  expect(imp.chunks === 35, "dw-impact: sums dead chunks");
  expect(imp.pages === 12, "dw-impact: sums dead pages");
  const single = [pdf({ pdf_hash: "a", embed_model: "nomic-embed-text", chunks: 5, pages: 3 })];
  expect(deadWeightImpact(single).pdfs === 0, "dw-impact: single model -> zero");
  expect(deadWeightImpact([]).chunks === 0, "dw-impact: empty -> zero");
  // @ts-expect-error null tolerance
  expect(deadWeightImpact(null).pages === 0, "dw-impact: garbage -> zero");
}

// --- reconcileModelFacet ---------------------------------------------
{
  const models = ["nomic-embed-text", "mxbai-embed-large"];
  expect(
    reconcileModelFacet("nomic-embed-text", models) === "nomic-embed-text",
    "reconcile: a live facet is kept",
  );
  expect(
    reconcileModelFacet("removed-model", models) === null,
    "reconcile: a facet whose model vanished clears to null",
  );
  expect(reconcileModelFacet(null, models) === null, "reconcile: no facet stays null");
  expect(reconcileModelFacet("x", []) === null, "reconcile: empty model list clears any facet");
  // @ts-expect-error null tolerance
  expect(reconcileModelFacet("x", null) === null, "reconcile: null model list clears facet");
}

// --- pinned model facets ---------------------------------------------
{
  expect(isModelPinned(["nomic"], "nomic") === true, "pin: member is pinned");
  expect(isModelPinned(["nomic"], "mxbai") === false, "pin: non-member not pinned");
  expect(isModelPinned([], "") === false, "pin: blank never pinned");
  expect(toggleModelPin(["a"], "b").join(",") === "a,b", "pin: append oldest-first");
  expect(toggleModelPin(["a", "b"], "a").join(",") === "b", "pin: toggle off removes");
  expect(toggleModelPin(["a", "a"], "b").join(",") === "a,b", "pin: dedupes input");
  expect(toggleModelPin(["a"], "").join(",") === "a", "pin: blank toggle no-op");
  // livePinnedModels drops pins whose model vanished, keeps pin order.
  expect(livePinnedModels(["a", "b", "c"], ["c", "a"]).join(",") === "a,c", "pin: only live, pin order");
  expect(livePinnedModels(["a", "a", "b"], ["a", "b"]).join(",") === "a,b", "pin: dedupe live");
  expect(livePinnedModels(["x"], []).length === 0, "pin: no live models -> []");
  // @ts-expect-error garbage
  expect(livePinnedModels(null, ["a"]).length === 0, "pin: null pinned -> []");
}

// --- summarizeSelection ----------------------------------------------
{
  const rows = [
    pdf({ pdf_hash: "a", chunks: 10, pages: 4 }),
    pdf({ pdf_hash: "b", chunks: 25, pages: 8 }),
    pdf({ pdf_hash: "c", chunks: 7, pages: 2 }),
  ];
  const some = summarizeSelection(rows, new Set(["a", "c"]));
  expect(some.pdfs === 2, "impact: counts selected PDFs");
  expect(some.chunks === 17, "impact: sums chunks of the selection (10+7)");
  expect(some.pages === 6, "impact: sums pages of the selection (4+2)");
  // Unknown hashes ignored.
  const withGhost = summarizeSelection(rows, new Set(["a", "ghost"]));
  expect(withGhost.pdfs === 1 && withGhost.chunks === 10, "impact: unknown hash ignored");
  // Empty selection -> all zeros.
  const none = summarizeSelection(rows, new Set());
  expect(none.pdfs === 0 && none.chunks === 0 && none.pages === 0, "impact: empty set -> zeros");
  // Defensive.
  // @ts-expect-error null tolerance
  expect(summarizeSelection(null, new Set(["a"])).pdfs === 0, "impact: null list -> zeros");
  // @ts-expect-error null tolerance
  expect(summarizeSelection(rows, null).pdfs === 0, "impact: null set -> zeros");
}

// --- describeImpact --------------------------------------------------
{
  expect(
    describeImpact({ pdfs: 12, chunks: 3418, pages: 240 }) === "12 PDFs \u00b7 3,418 chunks \u00b7 240 pages",
    "describeImpact: full line, pluralized + grouped",
  );
  expect(
    describeImpact({ pdfs: 1, chunks: 1, pages: 1 }) === "1 PDF \u00b7 1 chunk \u00b7 1 page",
    "describeImpact: singular forms",
  );
  expect(
    describeImpact({ pdfs: 3, chunks: 40, pages: 0 }) === "3 PDFs \u00b7 40 chunks",
    "describeImpact: zero pages drops out",
  );
  expect(
    describeImpact({ pdfs: 2, chunks: 0, pages: 0 }) === "2 PDFs",
    "describeImpact: only PDFs when chunks+pages zero",
  );
  expect(describeImpact({ pdfs: 0, chunks: 0, pages: 0 }) === "Nothing selected", "describeImpact: empty");
}

// --- describeBeaconView ----------------------------------------------
{
  // Empty index.
  expect(
    describeBeaconView({ shown: 0, total: 0, modelFacet: null, query: "", selected: 0 }) === "No PDFs indexed",
    "view: empty index",
  );
  // Unfiltered.
  expect(
    describeBeaconView({ shown: 50, total: 50, modelFacet: null, query: "", selected: 0 }) === "50 PDFs",
    "view: unfiltered total",
  );
  expect(
    describeBeaconView({ shown: 1, total: 1, modelFacet: null, query: "", selected: 0 }) === "1 PDF",
    "view: singular total",
  );
  // Search only.
  expect(
    describeBeaconView({ shown: 3, total: 50, modelFacet: null, query: "tax", selected: 0 }) ===
      "3 of 50 PDFs matching \u201ctax\u201d",
    "view: search narrows with quoted query",
  );
  // Facet only.
  expect(
    describeBeaconView({ shown: 20, total: 50, modelFacet: "nomic-embed-text", query: "", selected: 0 }) ===
      "20 of 50 PDFs matching nomic-embed-text",
    "view: facet narrows with model name",
  );
  // Facet + search combined.
  expect(
    describeBeaconView({ shown: 2, total: 50, modelFacet: "mxbai", query: "tax", selected: 0 }) ===
      "2 of 50 PDFs matching mxbai + \u201ctax\u201d",
    "view: facet + search combined",
  );
  // Selection appended.
  expect(
    describeBeaconView({ shown: 50, total: 50, modelFacet: null, query: "", selected: 4 }) ===
      "50 PDFs \u00b7 4 selected",
    "view: selection count appended",
  );
  // Defensive clamping: shown can't exceed total; negatives floored.
  expect(
    describeBeaconView({ shown: 999, total: 10, modelFacet: "m", query: "", selected: -3 }) ===
      "10 of 10 PDFs matching m",
    "view: shown clamps to total, negative selection floored away",
  );
}

// --- classifyBeaconTableKey ------------------------------------------
{
  // Navigation keys defer to the palette classifier.
  const down = classifyBeaconTableKey({ key: "ArrowDown" });
  expect(down?.kind === "move" && down.intent === "next", "key: ArrowDown -> move next");
  const up = classifyBeaconTableKey({ key: "ArrowUp" });
  expect(up?.kind === "move" && up.intent === "prev", "key: ArrowUp -> move prev");
  const home = classifyBeaconTableKey({ key: "Home" });
  expect(home?.kind === "move" && home.intent === "first", "key: Home -> move first");
  const pgdn = classifyBeaconTableKey({ key: "PageDown" });
  expect(pgdn?.kind === "move" && pgdn.intent === "page-down", "key: PageDown -> move page-down");
  // Action keys.
  expect(classifyBeaconTableKey({ key: " " })?.kind === "toggle", "key: Space -> toggle");
  expect(classifyBeaconTableKey({ key: "Spacebar" })?.kind === "toggle", "key: legacy Spacebar -> toggle");
  expect(classifyBeaconTableKey({ key: "Enter" })?.kind === "forget", "key: Enter -> forget");
  expect(classifyBeaconTableKey({ key: "a" })?.kind === "select-all", "key: a -> select-all");
  expect(classifyBeaconTableKey({ key: "A" })?.kind === "select-all", "key: A -> select-all");
  expect(classifyBeaconTableKey({ key: "Escape" })?.kind === "clear", "key: Escape -> clear");
  // Non-table keys fall through.
  expect(classifyBeaconTableKey({ key: "x" }) === null, "key: plain letter -> null (falls to search)");
  // Modifiers disqualify so app/OS chords keep priority.
  expect(classifyBeaconTableKey({ key: "a", metaKey: true }) === null, "key: Cmd+a -> null");
  expect(classifyBeaconTableKey({ key: "ArrowDown", ctrlKey: true }) === null, "key: Ctrl+Down -> null");
  expect(classifyBeaconTableKey({ key: "Enter", altKey: true }) === null, "key: Alt+Enter -> null");
  // @ts-expect-error null tolerance
  expect(classifyBeaconTableKey(null) === null, "key: null event -> null");
}

// --- nextBeaconCursor (adapter over palette nav) ---------------------
{
  // Wrapping arrows.
  expect(nextBeaconCursor("next", 4, 5) === 0, "cursor: next wraps past the end to 0");
  expect(nextBeaconCursor("prev", 0, 5) === 4, "cursor: prev wraps before the start to last");
  expect(nextBeaconCursor("next", 1, 5) === 2, "cursor: next steps forward");
  // Home/End.
  expect(nextBeaconCursor("first", 3, 5) === 0, "cursor: first -> 0");
  expect(nextBeaconCursor("last", 1, 5) === 4, "cursor: last -> last index");
  // Paging clamps (no wrap).
  expect(nextBeaconCursor("page-down", 0, 5) === 4, "cursor: page-down clamps to last");
  expect(nextBeaconCursor("page-up", 4, 5) === 0, "cursor: page-up clamps to 0");
  // Empty list.
  expect(nextBeaconCursor("next", 0, 0) === 0, "cursor: empty list -> 0");
}

// --- clampBeaconCursor -----------------------------------------------
{
  expect(clampBeaconCursor(40, 10) === 9, "clamp: out-of-range cursor snaps to last");
  expect(clampBeaconCursor(3, 10) === 3, "clamp: in-range cursor preserved");
  expect(clampBeaconCursor(5, 0) === 0, "clamp: empty list -> 0");
  expect(clampBeaconCursor(-2, 10) === 0, "clamp: negative cursor floored to 0");
  expect(clampBeaconCursor(NaN, 10) === 0, "clamp: NaN cursor -> 0");
  expect(clampBeaconCursor(2.9, 10) === 2, "clamp: fractional cursor floored");
}

// --- movePinnedModel (drag-reorder pinned strip) ---------------------
{
  expect(
    JSON.stringify(movePinnedModel(["a", "b", "c"], 0, 2)) === JSON.stringify(["b", "c", "a"]),
    "move: front to back",
  );
  expect(
    JSON.stringify(movePinnedModel(["a", "b", "c"], 2, 0)) === JSON.stringify(["c", "a", "b"]),
    "move: back to front",
  );
  expect(
    JSON.stringify(movePinnedModel(["a", "b", "c"], 1, 1)) === JSON.stringify(["a", "b", "c"]),
    "move: no-op move preserved",
  );
  expect(
    JSON.stringify(movePinnedModel(["a", "b", "c"], 0, 9)) === JSON.stringify(["b", "c", "a"]),
    "move: to clamps to last",
  );
  expect(
    JSON.stringify(movePinnedModel(["a", "a", "b"], 0, 1)) === JSON.stringify(["b", "a"]),
    "move: de-dupes before reordering",
  );
  expect(JSON.stringify(movePinnedModel(["a"], 0, 0)) === JSON.stringify(["a"]), "move: single stays");
  expect(JSON.stringify(movePinnedModel(null as never, 0, 1)) === "[]", "move: garbage -> []");
}

// --- classifyPinReorderKey / nextPinIndex (Alt+Arrow chip reorder) ---
{
  const k = (key: string, m: Partial<BeaconKeyEvent> = {}) =>
    ({ key, altKey: true, metaKey: false, ctrlKey: false, shiftKey: false, ...m }) as BeaconKeyEvent;
  expect(classifyPinReorderKey(k("ArrowLeft"))?.dir === -1, "pin: Alt+Left -> -1");
  expect(classifyPinReorderKey(k("ArrowRight"))?.dir === 1, "pin: Alt+Right -> +1");
  expect(classifyPinReorderKey(k("ArrowUp"))?.dir === -1, "pin: Alt+Up twin -1");
  expect(classifyPinReorderKey(k("ArrowDown"))?.dir === 1, "pin: Alt+Down twin +1");
  expect(classifyPinReorderKey(k("ArrowLeft", { altKey: false })) === null, "pin: no Alt -> null");
  expect(classifyPinReorderKey(k("ArrowLeft", { metaKey: true })) === null, "pin: meta disqualifies");
  expect(classifyPinReorderKey(k("ArrowLeft", { shiftKey: true })) === null, "pin: shift disqualifies");
  expect(classifyPinReorderKey(k("a")) === null, "pin: other key null");
  expect(classifyPinReorderKey(null as never) === null, "pin: garbage null");
  expect(nextPinIndex(0, 3, 1) === 1, "pin idx: 0 +1 -> 1");
  expect(nextPinIndex(2, 3, 1) === 2, "pin idx: last +1 clamps");
  expect(nextPinIndex(0, 3, -1) === 0, "pin idx: first -1 clamps");
  expect(nextPinIndex(1, 3, -1) === 0, "pin idx: 1 -1 -> 0");
  expect(nextPinIndex(0, 1, 1) === 0, "pin idx: single no-op");
  expect(nextPinIndex(5, 3, 1) === 2, "pin idx: out-of-range to last");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
