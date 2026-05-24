#!/usr/bin/env node
/**
 * test-pdfops.mjs — black-box smoke test for src/lib/try/pdfOps.ts.
 *
 * The project doesn't have vitest configured, so we run a Node-native
 * round-trip:  generate a 5-page PDF, exercise every export, assert
 * page counts come out right, and that metadata round-trips.
 *
 * Wired into CI via `pnpm check` is overkill; this is intended to run
 * locally + during cron ticks before commit:
 *
 *     node --experimental-strip-types scripts/test-pdfops.mjs
 *
 * Exit code 0 = green, non-zero = red.
 */

import { PDFDocument, StandardFonts } from "pdf-lib";
import {
  rotatePages,
  removePages,
  reorderPages,
  mergeFiles,
  splitAt,
  readMetadata,
  writeMetadata,
} from "../src/lib/try/pdfOps.ts";

let failures = 0;
function assert(cond, msg) {
  if (cond) {
    console.log(`  ok  ${msg}`);
  } else {
    console.error(`  FAIL  ${msg}`);
    failures++;
  }
}

async function makeDoc(pageCount, title = "fixture") {
  const doc = await PDFDocument.create();
  const helv = await doc.embedFont(StandardFonts.Helvetica);
  for (let i = 0; i < pageCount; i++) {
    const p = doc.addPage([300, 400]);
    p.drawText(`Page ${i + 1}`, { x: 30, y: 350, size: 24, font: helv });
  }
  doc.setTitle(title);
  doc.setAuthor("test");
  doc.setSubject("smoke");
  return await doc.save();
}

async function pageCount(bytes) {
  const d = await PDFDocument.load(bytes);
  return d.getPageCount();
}

(async function main() {
  console.log("pdfOps smoke");

  const five = await makeDoc(5);
  assert((await pageCount(five)) === 5, "fixture has 5 pages");

  // rotate
  const rotated = await rotatePages(five, [0, 2], 90);
  const rd = await PDFDocument.load(rotated);
  assert(rd.getPage(0).getRotation().angle === 90, "rotate p0 -> 90");
  assert(rd.getPage(1).getRotation().angle === 0, "rotate p1 unchanged");
  assert(rd.getPage(2).getRotation().angle === 90, "rotate p2 -> 90");

  // remove
  const trimmed = await removePages(five, [1, 3]);
  assert((await pageCount(trimmed)) === 3, "remove 2 pages -> 3 remain");

  // reorder
  const reordered = await reorderPages(five, [4, 3, 2, 1, 0]);
  assert((await pageCount(reordered)) === 5, "reorder preserves count");

  // merge
  const a = await makeDoc(2);
  const b = await makeDoc(3);
  const merged = await mergeFiles([a, b]);
  assert((await pageCount(merged)) === 5, "merge 2+3 -> 5");

  // splitAt
  const chunks = await splitAt(five, [2, 4]);
  assert(chunks.length === 3, "splitAt [2,4] -> 3 chunks");
  assert((await pageCount(chunks[0])) === 2, "chunk 0 has 2 pages");
  assert((await pageCount(chunks[1])) === 2, "chunk 1 has 2 pages");
  assert((await pageCount(chunks[2])) === 1, "chunk 2 has 1 page");

  // metadata round-trip
  const meta = await readMetadata(five);
  assert(meta.title === "fixture", "readMetadata title");
  assert(meta.author === "test", "readMetadata author");
  const reauthored = await writeMetadata(five, {
    title: "rewritten",
    author: "cake",
    keywords: "alpha, beta, gamma",
  });
  const re = await readMetadata(reauthored);
  assert(re.title === "rewritten", "writeMetadata title");
  assert(re.author === "cake", "writeMetadata author");
  assert(/alpha/.test(re.keywords), "writeMetadata keywords contains alpha");

  // error: empty doc
  let threw = false;
  try {
    await removePages(five, [0, 1, 2, 3, 4]);
  } catch {
    threw = true;
  }
  assert(threw, "remove-all rejects");

  // error: bad permutation
  threw = false;
  try {
    await reorderPages(five, [0, 0, 0, 0, 0]);
  } catch {
    threw = true;
  }
  assert(threw, "reorder rejects non-permutation");

  if (failures > 0) {
    console.error(`\n${failures} failure(s)`);
    process.exit(1);
  } else {
    console.log("\nall green");
  }
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
