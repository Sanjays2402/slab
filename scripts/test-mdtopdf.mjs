#!/usr/bin/env node
/**
 * test-mdtopdf.mjs — black-box smoke test for src/lib/try/mdToPdf.ts.
 *
 * Mirrors scripts/test-pdfops.mjs: no vitest, just a Node round-trip.
 * We render markdown via mdToPdf(), reload with pdf-lib, and assert
 * structural properties (page count, metadata, multi-page on long input).
 *
 *     node --experimental-strip-types scripts/test-mdtopdf.mjs
 *
 * Exit code 0 = green, non-zero = red.
 */

import { PDFDocument } from "pdf-lib";
import { mdToPdf, lexBlocks, lexInline } from "../src/lib/try/mdToPdf.ts";

let failures = 0;
function assert(cond, msg) {
  if (cond) {
    console.log(`  ✓ ${msg}`);
  } else {
    console.error(`  ✗ ${msg}`);
    failures++;
  }
}

async function main() {
  console.log("─── lexBlocks ───");

  const blocks1 = lexBlocks("# Hello\n\nA paragraph.\n\n- one\n- two\n");
  assert(blocks1.some((b) => b.kind === "heading"), "heading detected");
  assert(blocks1.some((b) => b.kind === "paragraph"), "paragraph detected");
  assert(blocks1.some((b) => b.kind === "ul"), "unordered list detected");

  const blocks2 = lexBlocks("```js\nconst x = 1;\nconst y = 2;\n```\n");
  const code = blocks2.find((b) => b.kind === "code");
  assert(!!code, "fenced code block detected");
  assert(code && code.text.includes("const x"), "code preserves contents");
  assert(code && code.lang === "js", "code language captured");

  const blocks3 = lexBlocks("> a quote line\n> continued\n");
  assert(blocks3.some((b) => b.kind === "blockquote"), "blockquote detected");

  const blocks4 = lexBlocks("1. first\n2. second\n3. third\n");
  const ol = blocks4.find((b) => b.kind === "ol");
  assert(!!ol && ol.items.length === 3, "ordered list with 3 items");

  const blocks5 = lexBlocks("Para A.\n\n---\n\nPara B.\n");
  assert(blocks5.some((b) => b.kind === "hr"), "horizontal rule detected");

  console.log("\n─── lexInline ───");

  const spans1 = lexInline("hello **world** *italic* and `code`");
  assert(spans1.some((s) => s.bold && s.text.includes("world")), "bold span");
  assert(spans1.some((s) => s.italic && s.text.includes("italic")), "italic span");
  assert(spans1.some((s) => s.code && s.text === "code"), "code span");

  const spans2 = lexInline("escaped \\*not italic\\*");
  assert(
    spans2.every((s) => !s.italic),
    "backslash escapes asterisks",
  );

  console.log("\n─── mdToPdf basic render ───");

  const empty = await mdToPdf("");
  const emptyDoc = await PDFDocument.load(empty);
  assert(emptyDoc.getPageCount() === 1, "empty input → 1 page");

  const short = await mdToPdf("# Title\n\nA short paragraph.\n", {
    title: "Test Doc",
    author: "Cake",
  });
  const shortDoc = await PDFDocument.load(short);
  assert(shortDoc.getPageCount() === 1, "short input → 1 page");
  assert(shortDoc.getTitle() === "Test Doc", "title metadata round-trips");
  assert(shortDoc.getAuthor() === "Cake", "author metadata round-trips");
  const producer = shortDoc.getProducer() || "";
  const creator = shortDoc.getCreator() || "";
  assert(
    producer.includes("Slab") || creator.includes("Slab"),
    `producer/creator mentions Slab (producer=${JSON.stringify(producer)}, creator=${JSON.stringify(creator)})`,
  );

  console.log("\n─── mdToPdf pagination ───");

  // Force pagination: 200 paragraphs of generous text.
  let long = "# Long Doc\n\n";
  for (let i = 0; i < 200; i++) {
    long += `Paragraph number ${i + 1}. ${"Lorem ipsum dolor sit amet, ".repeat(8)}\n\n`;
  }
  const longBytes = await mdToPdf(long);
  const longDoc = await PDFDocument.load(longBytes);
  assert(longDoc.getPageCount() >= 5, `200 paragraphs → ${longDoc.getPageCount()} pages (≥5)`);

  console.log("\n─── mdToPdf page sizes ───");
  const a4 = await mdToPdf("# A4", { pageSize: "A4" });
  const a4doc = await PDFDocument.load(a4);
  const a4size = a4doc.getPage(0).getSize();
  assert(
    Math.abs(a4size.width - 595.28) < 0.5 && Math.abs(a4size.height - 841.89) < 0.5,
    `A4 page is 595×842 (got ${a4size.width.toFixed(1)}×${a4size.height.toFixed(1)})`,
  );

  const legal = await mdToPdf("# Legal", { pageSize: "Legal" });
  const legalDoc = await PDFDocument.load(legal);
  const lsize = legalDoc.getPage(0).getSize();
  assert(
    Math.abs(lsize.width - 612) < 0.5 && Math.abs(lsize.height - 1008) < 0.5,
    `Legal page is 612×1008 (got ${lsize.width.toFixed(1)}×${lsize.height.toFixed(1)})`,
  );

  console.log("\n─── mdToPdf code blocks ───");
  const codeBytes = await mdToPdf("# Code\n\n```\nhello\nworld\n```\n");
  const codeDoc = await PDFDocument.load(codeBytes);
  assert(codeDoc.getPageCount() === 1, "code block renders inline (1 page for tiny doc)");

  console.log("\n─── summary ───");
  if (failures > 0) {
    console.error(`${failures} test(s) FAILED`);
    process.exit(1);
  }
  console.log("all green");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
