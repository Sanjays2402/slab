#!/usr/bin/env node
/**
 * mint-samples.mjs — generates the three /try sample PDFs at
 * static/try/samples/*.pdf using pdf-lib (already a project dep).
 *
 * Run: `node scripts/mint-samples.mjs`
 *
 * Deterministic output: setting the creation/modification dates to a
 * fixed value lets us check the files into git without churn.
 */

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { PDFDocument, StandardFonts, rgb } from "pdf-lib";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, "..", "static", "try", "samples");

const FIXED_DATE = new Date("2026-05-24T00:00:00Z");

function pageBackground(page, hex = "#fdfbf7") {
  const { width, height } = page.getSize();
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  page.drawRectangle({ x: 0, y: 0, width, height, color: rgb(r, g, b) });
}

async function mintOffer() {
  const doc = await PDFDocument.create();
  const helv = await doc.embedFont(StandardFonts.Helvetica);
  const helvBold = await doc.embedFont(StandardFonts.HelveticaBold);

  const draw = (page, body) => {
    pageBackground(page);
    const left = 72;
    let y = 760;
    page.drawText("ACME Industries, Inc.", {
      x: left, y, size: 18, font: helvBold, color: rgb(0.1, 0.1, 0.15),
    });
    y -= 28;
    page.drawText("Employment Offer Letter", {
      x: left, y, size: 12, font: helv, color: rgb(0.35, 0.35, 0.4),
    });
    y -= 36;
    for (const line of body) {
      page.drawText(line, { x: left, y, size: 11, font: helv, color: rgb(0.12, 0.12, 0.15) });
      y -= 16;
    }
  };

  draw(doc.addPage([612, 792]), [
    "Dear Jordan Rivera,",
    "",
    "We are pleased to extend an offer of employment for the position of",
    "Senior Engineer at ACME Industries, Inc.  Your start date will be",
    "June 14, 2026.  This letter sets out the principal terms of the offer.",
    "",
    "  - Base salary:  $185,000 / year, paid bi-weekly.",
    "  - Sign-on bonus: $20,000, payable on day one.",
    "  - Equity:        12,000 RSUs vesting over four years.",
    "  - Benefits:      medical, dental, vision, 401(k) match.",
    "",
    "This offer is contingent on a satisfactory background check and on",
    "your signing the attached confidentiality and IP assignment agreement.",
    "",
    "Please confirm acceptance by signing below and returning by May 31.",
    "",
    "Sincerely,",
    "Pat Lin, VP of People",
  ]);

  draw(doc.addPage([612, 792]), [
    "Acceptance",
    "",
    "I, ____________________________, accept the offer set out on page 1",
    "and agree to the terms described therein.",
    "",
    "Signature:  __________________________   Date: ___/___/____",
    "",
    "",
    "Confidential.  Do not distribute outside ACME without written consent.",
  ]);

  doc.setTitle("Employment Offer — Jordan Rivera");
  doc.setAuthor("ACME Industries, Inc.");
  doc.setSubject("Offer of employment");
  doc.setCreator("Slab try.slab.app sample");
  doc.setProducer("Slab try.slab.app sample");
  doc.setCreationDate(FIXED_DATE);
  doc.setModificationDate(FIXED_DATE);
  return await doc.save();
}

async function mintScannedInvoice() {
  // Single-page "scanned" invoice — drawn as if it were a low-res raster,
  // so OCR is the natural next step.  We don't embed a raster; we just
  // draw the page tinted off-white with a slight skew suggestion.
  const doc = await PDFDocument.create();
  const helv = await doc.embedFont(StandardFonts.Helvetica);
  const page = doc.addPage([612, 792]);
  pageBackground(page, "#f3efe6");

  const lines = [
    "                  INVOICE  #INV-2026-0429                ",
    "                                                          ",
    "  From: Riverbend Print Co.       To: Bayview Holdings   ",
    "        1247 Elm St.                   PO Box 8821        ",
    "        Madison, WI 53703             Seattle, WA 98101  ",
    "                                                          ",
    "  Item                              Qty    Rate    Total ",
    "  ----------------------------------------------------- ",
    "  Annual report — color, 80 pg     250   $14.20  $3,550 ",
    "  Trifold brochure — gloss          500    $1.10    $550",
    "  Foil stamping                      1   $215.00   $215 ",
    "                                                          ",
    "                                          TOTAL:  $4,315 ",
    "                                                          ",
    "  Terms: Net 30.  Late fee 1.5%/mo after due date.        ",
  ];
  let y = 720;
  for (const line of lines) {
    page.drawText(line, { x: 60, y, size: 10, font: helv, color: rgb(0.16, 0.16, 0.18) });
    y -= 16;
  }

  doc.setTitle("Riverbend Invoice INV-2026-0429");
  doc.setAuthor("Riverbend Print Co.");
  doc.setSubject("Invoice");
  doc.setCreator("Slab try.slab.app sample");
  doc.setProducer("Slab try.slab.app sample");
  doc.setCreationDate(FIXED_DATE);
  doc.setModificationDate(FIXED_DATE);
  return await doc.save();
}

async function mintMultiChapter() {
  const doc = await PDFDocument.create();
  const helv = await doc.embedFont(StandardFonts.Helvetica);
  const helvBold = await doc.embedFont(StandardFonts.HelveticaBold);
  const chapters = [
    "Cover & Letter from the CEO",
    "Executive summary",
    "Q4 financial highlights",
    "Operating segments",
    "Geographic breakdown",
    "Research & development",
    "People & culture",
    "Risk factors",
    "Forward-looking statements",
    "Auditor's report",
    "Income statement",
    "Balance sheet",
    "Cash flow",
  ];
  // 24 pages: 11 short chapters + 13 detail pages.
  for (let i = 0; i < 24; i++) {
    const page = doc.addPage([612, 792]);
    pageBackground(page);
    const chapter = chapters[i % chapters.length];
    page.drawText(`Chapter ${(i % chapters.length) + 1}`, {
      x: 72, y: 720, size: 11, font: helv, color: rgb(0.45, 0.45, 0.5),
    });
    page.drawText(chapter, {
      x: 72, y: 690, size: 22, font: helvBold, color: rgb(0.1, 0.1, 0.15),
    });
    page.drawText(`(page ${i + 1} of 24)`, {
      x: 72, y: 656, size: 10, font: helv, color: rgb(0.4, 0.4, 0.45),
    });
    // Some lorem-ish body.
    let y = 620;
    const body = [
      "Acme reported steady growth across its three core operating segments,",
      "with international revenue contributing 38% of total net sales.  The",
      "company continues to invest in long-duration projects, balanced against",
      "near-term execution discipline and a conservative capital structure.",
      "",
      "The board notes that segment-level margins improved on average by 120bps",
      "year-over-year, driven primarily by mix and pricing.  Headwinds in the",
      "industrial segment were offset by ongoing strength in services.",
    ];
    for (const line of body) {
      page.drawText(line, { x: 72, y, size: 11, font: helv, color: rgb(0.18, 0.18, 0.22) });
      y -= 16;
    }
  }
  doc.setTitle("Acme Industries — FY2026 Annual Report");
  doc.setAuthor("Acme Industries, Inc.");
  doc.setSubject("Annual report");
  doc.setCreator("Slab try.slab.app sample");
  doc.setProducer("Slab try.slab.app sample");
  doc.setCreationDate(FIXED_DATE);
  doc.setModificationDate(FIXED_DATE);
  return await doc.save();
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true });
  const jobs = [
    ["employment-offer.pdf", await mintOffer()],
    ["scanned-invoice.pdf", await mintScannedInvoice()],
    ["multi-chapter-report.pdf", await mintMultiChapter()],
  ];
  for (const [name, bytes] of jobs) {
    const out = join(OUT_DIR, name);
    await writeFile(out, bytes);
    console.log(`wrote ${out} (${bytes.length} bytes)`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
