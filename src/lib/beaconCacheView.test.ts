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
  type BeaconPdfLike,
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

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
