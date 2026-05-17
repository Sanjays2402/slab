// Library Mode client bindings (v0.12.0 Atlas + v0.13.0 Lens Slice 2)
//
// Thin typed wrappers around the `slab_library_*` Tauri commands
// defined in `src-tauri/src/lib.rs`. Slice 1 of Atlas was backend-only;
// these bindings exist so the LibraryPanel can drop in without
// redefining types or hand-coding `invoke` calls. Slice 2 of Lens
// extends them with OCR-queue helpers.
//
// Each wrapper:
//   - returns a Promise that resolves to the unwrapped value, or
//     rejects with `Error(message)` if the backend returned `Err`
//   - is fully typed (no `unknown` leakage to callers)
//
// DO NOT add UI logic here. This file is the IPC seam only.

import { invoke } from "@tauri-apps/api/core";
import type { CmdResult } from "./types";

// ---------- Backend DTO mirrors ----------

/** Mirror of `pdf::library::registry::FolderRecord`. */
export interface FolderRecord {
  id: number;
  path: string;
  added_at: number;
  last_scanned_at: number | null;
}

/** Mirror of `pdf::library::registry::TagRecord`. */
export interface TagRecord {
  id: number;
  name: string;
  color: string | null;
}

/** OCR pipeline state for a document. Mirror of `OCR_STATE_*` constants
 * in `pdf::library::registry`. */
export type OcrState =
  | "unknown"
  | "text_native"
  | "scanned"
  | "mixed"
  | "ocr_pending"
  | "ocr_done"
  | "ocr_failed";

/** Mirror of `pdf::library::registry::DocumentRecord` with eager-loaded tags. */
export interface DocumentRecord {
  id: number;
  folder_id: number | null;
  path: string;
  title: string | null;
  hash: string;
  size_bytes: number;
  mtime_ns: number;
  pages: number | null;
  added_at: number;
  last_seen_at: number;
  /** One of the `OcrState` values; defaults to `unknown` for legacy rows. */
  ocr_state: OcrState;
  /** Where the OCR'd searchable PDF lives, when `ocr_state === "ocr_done"`. */
  ocr_output_path: string | null;
  tags: TagRecord[];
}

/** Mirror of `pdf::library::scanner::ScanReport`. */
export interface ScanReport {
  folder_id: number;
  scanned_files: number;
  added: number;
  updated: number;
  unchanged: number;
  skipped_non_pdf: number;
  skipped_errors: number;
  elapsed_ms: number;
}

/** Mirror of `pdf::library::query::SortBy`. */
export type LibrarySortBy = "added_desc" | "title_asc" | "last_seen_desc";

/** Mirror of `pdf::library::query::LibraryFilter`. */
export interface LibraryFilter {
  folder_id?: number | null;
  tag_ids?: number[];
  title_substring?: string | null;
  limit?: number | null;
  sort?: LibrarySortBy;
}

// ---------- Unwrap helper ----------

function unwrap<T>(res: CmdResult<T>): T {
  if (res.kind === "ok") return res.value;
  throw new Error(res.message);
}

// ---------- Folder management ----------

export async function addFolder(path: string): Promise<FolderRecord> {
  const res = await invoke<CmdResult<FolderRecord>>("slab_library_add_folder", {
    path,
  });
  return unwrap(res);
}

export async function removeFolder(id: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_remove_folder", {
    id,
  });
  unwrap(res);
}

export async function listFolders(): Promise<FolderRecord[]> {
  const res = await invoke<CmdResult<FolderRecord[]>>(
    "slab_library_list_folders",
  );
  return unwrap(res);
}

// ---------- Scanning ----------

export async function scanFolder(folderId: number): Promise<ScanReport> {
  const res = await invoke<CmdResult<ScanReport>>("slab_library_scan", {
    folderId,
  });
  return unwrap(res);
}

// ---------- Document queries ----------

export async function listDocuments(
  filter?: LibraryFilter,
): Promise<DocumentRecord[]> {
  const res = await invoke<CmdResult<DocumentRecord[]>>(
    "slab_library_list_docs",
    { filter: filter ?? null },
  );
  return unwrap(res);
}

// ---------- Tags ----------

export async function listTags(): Promise<TagRecord[]> {
  const res = await invoke<CmdResult<TagRecord[]>>("slab_library_list_tags");
  return unwrap(res);
}

export async function addTag(
  name: string,
  color?: string | null,
): Promise<TagRecord> {
  const res = await invoke<CmdResult<TagRecord>>("slab_library_add_tag", {
    name,
    color: color ?? null,
  });
  return unwrap(res);
}

export async function setDocumentTags(
  docId: number,
  tagIds: number[],
): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_set_doc_tags", {
    docId,
    tagIds,
  });
  unwrap(res);
}

export async function removeDocument(docId: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_remove_document", {
    docId,
  });
  unwrap(res);
}

export async function removeTag(tagId: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_remove_tag", {
    tagId,
  });
  unwrap(res);
}

/** Rescan every registered folder. Returns one report per folder. */
export async function rescanAll(): Promise<ScanReport[]> {
  const res = await invoke<CmdResult<ScanReport[]>>(
    "slab_library_rescan_all",
  );
  return unwrap(res);
}

// ---------- OCR queue (Lens Slice 2) ----------

/** Mirror of `pdf::library::ocr_queue::OcrQueueResult`. */
export interface OcrQueueResult {
  doc_id: number;
  state_after: OcrState;
  output_path: string | null;
  error: string | null;
}

/** Optional Tesseract knobs forwarded to the OCR queue commands. */
export interface OcrOpts {
  lang?: string;
  dpi?: number;
}

/**
 * List documents whose ocr_state is `scanned` or `mixed` — i.e. OCR
 * candidates not yet processed. Ordered by added_at ASC.
 */
export async function ocrQueueListPending(): Promise<DocumentRecord[]> {
  const res = await invoke<CmdResult<DocumentRecord[]>>(
    "slab_library_ocr_queue_list_pending",
  );
  return unwrap(res);
}

/**
 * Run OCR on a single document by id. Returns the queue result
 * (state_after, output_path, error). Resolves even on OCR failure —
 * the failure surface is inside `result.error`, not a thrown Error.
 */
export async function ocrQueueRunOne(
  docId: number,
  opts?: OcrOpts | null,
): Promise<OcrQueueResult> {
  const res = await invoke<CmdResult<OcrQueueResult>>(
    "slab_library_ocr_queue_run_one",
    { docId, opts: opts ?? null },
  );
  return unwrap(res);
}

/**
 * Run OCR on every pending document, in queue order. Returns one
 * result per processed doc. Continues past per-doc OCR failures;
 * only rejects on a DB-level error (e.g. library DB unreadable).
 */
export async function ocrQueueRunAll(
  opts?: OcrOpts | null,
): Promise<OcrQueueResult[]> {
  const res = await invoke<CmdResult<OcrQueueResult[]>>(
    "slab_library_ocr_queue_run_all",
    { opts: opts ?? null },
  );
  return unwrap(res);
}
