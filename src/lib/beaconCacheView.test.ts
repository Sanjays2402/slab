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
  reconcileModelFacet,
  BEACON_SORT_FIELDS,
  type BeaconPdfLike,
  type BeaconSort,
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

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
