// Lens (OCR + Vision) client bindings — v0.13.0 Slice 1.
//
// Thin typed wrappers around the Lens family of `slab_*` Tauri commands.
// Slice 1 ships only scan_audit; later slices add table extraction, vision
// chat hooks, etc.
//
// Same contract as `library.ts`: each wrapper unwraps `CmdResult` and
// rejects on `err`.

import { invoke } from "@tauri-apps/api/core";
import type { CmdResult } from "./types";

// ---------- Backend DTO mirrors ----------

/** Mirror of `pdf::scan_audit::PageClassification`. */
export type PageClassification = "text" | "image" | "mixed" | "empty";

/** Mirror of `pdf::scan_audit::Recommendation`. */
export type ScanRecommendation = "ocr_all" | "ocr_some" | "none";

/** Mirror of `pdf::scan_audit::ScanAuditReport`. */
export interface ScanAuditReport {
  pages: PageClassification[];
  text_pages: number;
  image_pages: number;
  mixed_pages: number;
  empty_pages: number;
  recommended_action: ScanRecommendation;
}

// ---------- Helpers ----------

function unwrap<T>(res: CmdResult<T>): T {
  if (res.kind === "ok") return res.value;
  throw new Error(res.message);
}

// ---------- Commands ----------

/**
 * Audit a PDF: returns per-page classification + recommendation for whether
 * the document should be sent through OCR. Cheap — no rasterization, just
 * walks the PDF object graph. Safe to call on every Reader open.
 */
export async function slabScanAudit(input: string): Promise<ScanAuditReport> {
  const res = await invoke<CmdResult<ScanAuditReport>>("slab_scan_audit", {
    input,
  });
  return unwrap(res);
}

/** Human label for the recommendation, used in banners and lists. */
export function recommendationLabel(r: ScanRecommendation): string {
  switch (r) {
    case "ocr_all":
      return "This PDF looks fully scanned — make it searchable?";
    case "ocr_some":
      return "Some pages look scanned — OCR them to make text selectable?";
    case "none":
      return "PDF is already text-searchable.";
  }
}

/** Total non-empty pages — useful for banner phrasing. */
export function nonEmptyPages(r: ScanAuditReport): number {
  return r.text_pages + r.image_pages + r.mixed_pages;
}

// ---------- Tables (Slice 3) ----------

/** Mirror of `pdf::table_extract::BBox`. */
export interface BBox {
  x_min: number;
  y_min: number;
  x_max: number;
  y_max: number;
}

/** Mirror of `pdf::table_extract::Table`. */
export interface Table {
  page: number;
  index: number;
  bbox: BBox;
  rows: string[][];
  columns: number;
}

/** Mirror of `pdf::table_extract::TableOpts`. */
export interface TableOpts {
  page: number;
  min_rows?: number;
  min_cols?: number;
}

/**
 * Detect tables on `opts.page` of `input`. Returns an empty array if
 * no candidate meets the min_rows / min_cols thresholds. Requires
 * `pdftotext` (poppler) on PATH.
 */
export async function slabExtractTables(
  input: string,
  opts: TableOpts,
): Promise<Table[]> {
  const res = await invoke<CmdResult<Table[]>>("slab_extract_tables", {
    input,
    opts,
  });
  return unwrap(res);
}

/** Serialize a Table to RFC-4180 CSV (pure backend call, never fails). */
export async function slabTableToCsv(table: Table): Promise<string> {
  const res = await invoke<CmdResult<string>>("slab_table_to_csv", { table });
  return unwrap(res);
}

/** Save a Table as CSV to `output`. Resolves to the saved path. */
export async function slabTableSaveCsv(
  table: Table,
  output: string,
): Promise<string> {
  const res = await invoke<CmdResult<string>>("slab_table_save_csv", {
    table,
    output,
  });
  return unwrap(res);
}
