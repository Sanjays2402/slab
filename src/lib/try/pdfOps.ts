/**
 * Pure PDF mutation helpers, backed by pdf-lib.
 *
 * Every function is a referentially-transparent transform from input
 * bytes to output bytes — no DOM access, no Tauri imports, no UI state.
 * This is the layer that has to work identically in /try (browser) and
 * in scripts (Node, when minting samples).
 */
import {
  PDFDocument,
  type PDFEmbeddedPage,
  degrees,
} from "pdf-lib";

/** Allowed rotation deltas (CCW). Multiples of 90 only. */
export type RotationDelta = 90 | 180 | 270;

/**
 * Rotates the given zero-based page indices by `deltaDegrees` (mod 360).
 * Other pages are untouched.  Returns a freshly-serialised PDF.
 */
export async function rotatePages(
  bytes: Uint8Array,
  indices: number[],
  deltaDegrees: RotationDelta,
): Promise<Uint8Array> {
  const doc = await PDFDocument.load(bytes);
  for (const i of indices) {
    if (i < 0 || i >= doc.getPageCount()) {
      throw new Error(`page index out of range: ${i}`);
    }
    const page = doc.getPage(i);
    const current = page.getRotation().angle;
    page.setRotation(degrees((current + deltaDegrees) % 360));
  }
  return await doc.save();
}

/**
 * Removes the given zero-based page indices.  Throws if removing all
 * pages would leave the document empty.  Returns a freshly-serialised PDF.
 */
export async function removePages(
  bytes: Uint8Array,
  indices: number[],
): Promise<Uint8Array> {
  const doc = await PDFDocument.load(bytes);
  const total = doc.getPageCount();
  const unique = Array.from(new Set(indices));
  if (unique.length >= total) {
    throw new Error("cannot remove every page");
  }
  // Remove in descending order so earlier indices stay valid.
  const sorted = unique.slice().sort((a, b) => b - a);
  for (const i of sorted) {
    if (i < 0 || i >= total) {
      throw new Error(`page index out of range: ${i}`);
    }
    doc.removePage(i);
  }
  return await doc.save();
}

/**
 * Reorders pages.  `newOrder` is a permutation of `[0, count)` describing
 * the new sequence (e.g. `[2,0,1]` on a 3-page doc moves page 3 to the
 * front).
 */
export async function reorderPages(
  bytes: Uint8Array,
  newOrder: number[],
): Promise<Uint8Array> {
  const src = await PDFDocument.load(bytes);
  const count = src.getPageCount();
  if (newOrder.length !== count) {
    throw new Error(
      `newOrder length (${newOrder.length}) must equal page count (${count})`,
    );
  }
  const seen = new Set<number>();
  for (const idx of newOrder) {
    if (idx < 0 || idx >= count || seen.has(idx)) {
      throw new Error("newOrder must be a permutation of all page indices");
    }
    seen.add(idx);
  }
  const out = await PDFDocument.create();
  out.setTitle(src.getTitle() ?? "");
  out.setAuthor(src.getAuthor() ?? "");
  out.setSubject(src.getSubject() ?? "");
  out.setCreator("Slab try.slab.app");
  out.setProducer("Slab try.slab.app");
  const copied = await out.copyPages(src, newOrder);
  for (const p of copied) out.addPage(p);
  return await out.save();
}

/**
 * Concatenates multiple PDFs into a single document, preserving page
 * order within each input.
 */
export async function mergeFiles(files: Uint8Array[]): Promise<Uint8Array> {
  if (files.length === 0) throw new Error("mergeFiles: no inputs");
  const out = await PDFDocument.create();
  out.setCreator("Slab try.slab.app");
  out.setProducer("Slab try.slab.app");
  for (const f of files) {
    const doc = await PDFDocument.load(f);
    const copied = await out.copyPages(doc, doc.getPageIndices());
    for (const p of copied) out.addPage(p);
  }
  return await out.save();
}

/**
 * Splits a PDF at the given zero-based boundary indices.  A boundary
 * index `b` means "start a new chunk at page index `b`".
 *
 * Example: splitAt(bytes, [3, 7]) on a 10-page doc yields three PDFs
 * containing pages [0..2], [3..6], and [7..9].
 */
export async function splitAt(
  bytes: Uint8Array,
  boundaries: number[],
): Promise<Uint8Array[]> {
  const src = await PDFDocument.load(bytes);
  const count = src.getPageCount();
  const sorted = Array.from(new Set(boundaries))
    .filter((b) => b > 0 && b < count)
    .sort((a, b) => a - b);
  const ranges: Array<[number, number]> = [];
  let start = 0;
  for (const b of sorted) {
    ranges.push([start, b]);
    start = b;
  }
  ranges.push([start, count]);

  const results: Uint8Array[] = [];
  for (const [from, to] of ranges) {
    const chunk = await PDFDocument.create();
    chunk.setCreator("Slab try.slab.app");
    chunk.setProducer("Slab try.slab.app");
    const indices: number[] = [];
    for (let i = from; i < to; i++) indices.push(i);
    const copied = await chunk.copyPages(src, indices);
    for (const p of copied) chunk.addPage(p);
    results.push(await chunk.save());
  }
  return results;
}

/**
 * Reads the basic metadata block (title / author / subject / keywords)
 * from a PDF.  Any missing field is returned as the empty string.
 */
export interface PdfMetadata {
  title: string;
  author: string;
  subject: string;
  keywords: string;
}

export async function readMetadata(bytes: Uint8Array): Promise<PdfMetadata> {
  const doc = await PDFDocument.load(bytes);
  return {
    title: doc.getTitle() ?? "",
    author: doc.getAuthor() ?? "",
    subject: doc.getSubject() ?? "",
    keywords: (doc.getKeywords() ?? "").toString(),
  };
}

/**
 * Writes the given metadata block onto a PDF and returns the new bytes.
 */
export async function writeMetadata(
  bytes: Uint8Array,
  meta: Partial<PdfMetadata>,
): Promise<Uint8Array> {
  const doc = await PDFDocument.load(bytes);
  if (meta.title !== undefined) doc.setTitle(meta.title);
  if (meta.author !== undefined) doc.setAuthor(meta.author);
  if (meta.subject !== undefined) doc.setSubject(meta.subject);
  if (meta.keywords !== undefined) {
    doc.setKeywords(
      meta.keywords
        .split(/[,;]\s*/)
        .map((k) => k.trim())
        .filter(Boolean),
    );
  }
  doc.setModificationDate(new Date());
  return await doc.save();
}

// Re-export pdf-lib types if downstream callers want them.
export type { PDFEmbeddedPage };
