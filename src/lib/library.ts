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

// ---------- Atlas (v2.2.0) — cross-doc FTS5 search ----------

export interface SearchHit {
  docId: number;
  path: string;
  title: string | null;
  pageIndex: number;
  /** Snippet pre-wrapped with `<mark>…</mark>` around matches. */
  snippet: string;
  /** bm25 rank — lower is better in FTS5. */
  rank: number;
}

/**
 * Run a cross-document full-text search against the Atlas FTS5 index.
 *
 * @param query natural-language query (we sanitise + quote it ourselves)
 * @param limit hard cap on returned hits (default 50, clamped server-side to 1..500)
 * @param folderId optionally restrict to one library folder
 */
export async function librarySearch(
  query: string,
  limit: number = 50,
  folderId: number | null = null,
): Promise<SearchHit[]> {
  if (!query || !query.trim()) return [];
  const res = await invoke<
    CmdResult<
      Array<{
        doc_id: number;
        path: string;
        title: string | null;
        page_index: number;
        snippet: string;
        rank: number;
      }>
    >
  >("slab_library_search", { query, limit, folderId });
  const hits = unwrap(res);
  return hits.map((h) => ({
    docId: h.doc_id,
    path: h.path,
    title: h.title,
    pageIndex: h.page_index,
    snippet: h.snippet,
    rank: h.rank,
  }));
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

// ---------- Auto-tag (Lens Slice 6) ----------

/**
 * Mirror of `ai::auto_tag::AutoTagOpts`. Knobs forwarded to the auto-tag
 * commands. All fields are optional; backend uses sensible defaults
 * (`max_tags = 5`, `max_context_chars = 6_000`).
 */
export interface AutoTagOpts {
  /** Hard cap on tags returned. Backend clamps to 1..=10. Default 5. */
  max_tags?: number;
  /** Max characters of doc text included in the prompt. Default 6_000. */
  max_context_chars?: number;
}

/**
 * Per-document result from auto-tag. Resolves even on failure — the
 * failure surface is `result.error`, not a thrown Error.
 *
 * `tags_assigned` is the FINAL tag set on the doc after the run —
 * existing user-set tags are preserved (additive union, never
 * replaces). `tag_ids` parallels `tags_assigned`.
 */
export interface AutoTagRunResult {
  doc_id: number;
  tags_assigned: string[];
  tag_ids: number[];
  error: string | null;
}

/**
 * Suggest and apply tags to a single library document. Reads the doc's
 * text, asks the configured Beacon provider for 3–5 topical tags, and
 * unions them with any tags the user already set by hand. Never removes
 * user-added tags. Resolves with the final tag set (or `error` set).
 */
export async function autoTagRunOne(
  docId: number,
  opts?: AutoTagOpts | null,
): Promise<AutoTagRunResult> {
  const res = await invoke<CmdResult<AutoTagRunResult>>(
    "slab_library_auto_tag_one",
    { docId, opts: opts ?? null },
  );
  return unwrap(res);
}

/**
 * Run auto-tag across many docs sequentially. Per-doc failures collapse
 * into `result.error` on each entry and do NOT abort the batch.
 *
 * Only rejects on backend wiring errors (e.g. missing provider config).
 */
export async function autoTagRunMany(
  docIds: number[],
  opts?: AutoTagOpts | null,
): Promise<AutoTagRunResult[]> {
  const res = await invoke<CmdResult<AutoTagRunResult[]>>(
    "slab_library_auto_tag_many",
    { docIds, opts: opts ?? null },
  );
  return unwrap(res);
}

// ---------- v3.32.0 "Atlas" — Collections & Smart Collections ----------

/** Mirror of `pdf::library::collections::CollectionRecord`. */
export interface CollectionRecord {
  id: number;
  name: string;
  icon: string | null;
  color: string | null;
  created_at: number;
  sort_order: number;
  doc_count: number;
}

/** Mirror of `pdf::library::collections::SmartCollectionRecord`. */
export interface SmartCollectionRecord {
  id: number;
  name: string;
  icon: string | null;
  color: string | null;
  query_json: string;
  created_at: number;
  sort_order: number;
}

/** Mirror of `pdf::library::collections::NewSmartCollection`. */
export interface NewSmartCollection {
  name: string;
  icon: string | null;
  color: string | null;
  filter: LibraryFilter;
}

export async function collectionCreate(
  name: string,
  icon?: string | null,
  color?: string | null,
): Promise<CollectionRecord> {
  const res = await invoke<CmdResult<CollectionRecord>>(
    "slab_collection_create",
    { name, icon: icon ?? null, color: color ?? null },
  );
  return unwrap(res);
}

export async function collectionList(): Promise<CollectionRecord[]> {
  const res = await invoke<CmdResult<CollectionRecord[]>>(
    "slab_collection_list",
  );
  return unwrap(res);
}

export async function collectionRename(
  id: number,
  name: string,
): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_collection_rename", {
    id,
    name,
  });
  unwrap(res);
}

export async function collectionDelete(id: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_collection_delete", { id });
  unwrap(res);
}

export async function collectionAddDocs(
  collectionId: number,
  docIds: number[],
): Promise<number> {
  const res = await invoke<CmdResult<number>>("slab_collection_add_docs", {
    collectionId,
    docIds,
  });
  return unwrap(res);
}

export async function collectionRemoveDocs(
  collectionId: number,
  docIds: number[],
): Promise<number> {
  const res = await invoke<CmdResult<number>>("slab_collection_remove_docs", {
    collectionId,
    docIds,
  });
  return unwrap(res);
}

export async function collectionListDocs(
  collectionId: number,
): Promise<DocumentRecord[]> {
  const res = await invoke<CmdResult<DocumentRecord[]>>(
    "slab_collection_list_docs",
    { collectionId },
  );
  return unwrap(res);
}

export async function smartCollectionCreate(
  spec: NewSmartCollection,
): Promise<SmartCollectionRecord> {
  const res = await invoke<CmdResult<SmartCollectionRecord>>(
    "slab_smart_collection_create",
    { spec },
  );
  return unwrap(res);
}

export async function smartCollectionList(): Promise<SmartCollectionRecord[]> {
  const res = await invoke<CmdResult<SmartCollectionRecord[]>>(
    "slab_smart_collection_list",
  );
  return unwrap(res);
}

export async function smartCollectionDelete(id: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_smart_collection_delete", {
    id,
  });
  unwrap(res);
}

export async function smartCollectionExpand(
  id: number,
): Promise<DocumentRecord[]> {
  const res = await invoke<CmdResult<DocumentRecord[]>>(
    "slab_smart_collection_expand",
    { id },
  );
  return unwrap(res);
}
