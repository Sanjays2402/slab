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
  /** Optional freeform note about the tag (rail + chip tooltip). Trimmed,
   * `null` when unset. v3.51.0 Atlas Tag-Descriptions. */
  description: string | null;
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
  /** When `ocr_state === "ocr_failed"`, the captured reason (e.g. "tesseract
   * not on PATH"). Cleared back to null on a successful re-OCR. Surfaced by
   * the OCR Queue Panel's failure inbox. v3.52.0 Atlas OCR-Queue. */
  ocr_error: string | null;
  /** Per-doc freeform notes shown in the Doc-Inspector drawer. Trimmed,
   * `null` when unset. Cap is 4000 Unicode scalars at the backend.
   * v3.55.0 Atlas Doc-Inspector. */
  notes: string | null;
  /** Whether the user has starred this document. Defaults to `false` for
   * pre-v14 rows. Surfaced as a ★ glyph on the card and filterable via
   * `starred_only` / the `starred` clause variant. v3.55.0 Atlas
   * Doc-Inspector. */
  starred: boolean;
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

/** Mirror of `pdf::library::query::FilterCombinator`. */
export type FilterCombinator = "and" | "or";

/**
 * Mirror of `pdf::library::query::TagMatch`. Governs how the flat
 * `tag_ids` list combines: `"all"` requires every tag (AND/intersection,
 * the historical default), `"any"` requires at least one (OR/union).
 */
export type TagMatch = "all" | "any";

/**
 * Mirror of `pdf::library::query::FilterClause`. Tagged with `type` so
 * the frontend can dispatch with a single switch — much friendlier than
 * sniffing for the presence of fields.
 */
export type FilterClause =
  | { type: "tag"; id: number }
  | { type: "not_tag"; id: number }
  | { type: "folder"; id: number }
  | { type: "not_folder"; id: number }
  | { type: "title_contains"; value: string }
  | { type: "title_not_contains"; value: string }
  | { type: "untagged" }
  | { type: "tagged" }
  | { type: "starred" }
  | { type: "not_starred" }
  | { type: "group"; combinator: FilterCombinator; clauses: FilterClause[] };

/** Mirror of `pdf::library::query::FilterGroup`. */
export interface FilterGroup {
  combinator: FilterCombinator;
  clauses: FilterClause[];
}

/** Mirror of `pdf::library::query::LibraryFilter`. */
export interface LibraryFilter {
  folder_id?: number | null;
  tag_ids?: number[];
  /**
   * v3.48.0 Atlas Tag-Combinator: how `tag_ids` combine. `"all"` (default,
   * omitted on the wire) intersects; `"any"` unions. Absent on legacy
   * filters, which the backend reads as `"all"`.
   */
  tag_match?: TagMatch;
  title_substring?: string | null;
  limit?: number | null;
  sort?: LibrarySortBy;
  /**
   * v3.34.0 Atlas Smart+: nested AND/OR/NOT clause tree. When present,
   * overrides the flat `folder_id`/`tag_ids`/`title_substring` fields.
   * The serde tagged enum on the Rust side means a `FilterClause` is
   * `{ type: "tag", id: 42 }` — NOT `{ tag: { id: 42 } }`. The `group`
   * variant flattens (combinator + clauses on the same object as `type`)
   * so the JSON matches `FilterClause::Group(FilterGroup)`.
   */
  clauses?: FilterGroup | null;
  /**
   * v3.55.0 Atlas Doc-Inspector: when `true`, only starred documents
   * match. AND-combined with every other constraint (flat fields OR
   * clause tree). Defaults to `false`; omit on the wire for legacy
   * compatibility — the backend reads a missing field as `false`.
   */
  starred_only?: boolean;
}

/**
 * Build a sensible default FilterGroup for an empty new smart
 * collection — a single AND group. Used by the recursive builder UI.
 */
export function emptyFilterGroup(): FilterGroup {
  return { combinator: "and", clauses: [] };
}

/**
 * Synthesize a FilterGroup from a legacy flat LibraryFilter. v3.32 /
 * v3.33 stored smart collections without a `clauses` field — when the
 * v3.34+ builder opens one, we hydrate it into an equivalent AND group
 * so the user can edit it in the new UI.
 */
export function migrateFlatFilter(f: LibraryFilter): FilterGroup {
  if (f.clauses) return f.clauses;
  const clauses: FilterClause[] = [];
  if (f.folder_id != null) clauses.push({ type: "folder", id: f.folder_id });
  for (const id of f.tag_ids ?? []) clauses.push({ type: "tag", id });
  if (f.title_substring) {
    clauses.push({ type: "title_contains", value: f.title_substring });
  }
  return { combinator: "and", clauses };
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


/**
 * One row from the rolling library search history.
 * Newest first when returned from {@link recentLibrarySearches}.
 * v3.52.0 Atlas Recent-Searches.
 */
export interface RecentSearch {
  id: number;
  query: string;
  /** Unix seconds. */
  ts: number;
  /** How many hits this query produced the last time it ran. */
  resultCount: number;
}

/**
 * Most-recent N library search rows, newest first. Powers the
 * LibrarySearchPanel's "Recent searches" chip strip — one click to re-run
 * a prior query. `limit` clamps backend-side to `1..=50`; default 8.
 * v3.52.0 Atlas Recent-Searches.
 */
export async function recentLibrarySearches(
  limit?: number,
): Promise<RecentSearch[]> {
  const res = await invoke<
    CmdResult<
      Array<{ id: number; query: string; ts: number; result_count: number }>
    >
  >("slab_library_recent_searches", { limit: limit ?? null });
  const rows = unwrap(res);
  return rows.map((r) => ({
    id: r.id,
    query: r.query,
    ts: r.ts,
    resultCount: r.result_count,
  }));
}

/**
 * Wipe every row from the rolling search log. Returns the number removed
 * (0 if the log was already empty). Cluster-dismissals (the Atlas-suggest
 * "don't show me this again" memory) live in a sibling table and are NOT
 * touched. v3.52.0 Atlas Recent-Searches.
 */
export async function clearLibrarySearchHistory(): Promise<number> {
  const res = await invoke<CmdResult<number>>(
    "slab_library_clear_search_history",
  );
  return unwrap(res);
}

/**
 * Delete a SINGLE recent-search row by id. Backs the per-chip delete
 * affordance (an x on each chip / Backspace on the focused chip) so the
 * user can prune one stray query without nuking the whole history.
 * Resolves true when a row was actually removed, false for a stale id
 * (e.g. a chip already gone after a concurrent clear). v3.57.0 Atlas
 * Recent-Searches.
 */
export async function deleteLibrarySearch(id: number): Promise<boolean> {
  const res = await invoke<CmdResult<boolean>>("slab_library_delete_search", {
    id,
  });
  return unwrap(res);
}

/**
 * Snapshot of the FTS5 library index size. Powers the LibrarySearchPanel
 * status footer so the user can see at-a-glance how many docs + pages
 * are searchable. Two cheap COUNTs server-side; safe to call frequently.
 * v3.55.0 Atlas Index-Status.
 */
export interface LibraryIndexStats {
  /** Distinct doc_ids present in library_fts. */
  docs: number;
  /** Total fts rows (one per indexed page) across every doc. */
  pages: number;
}

export async function libraryIndexStats(): Promise<LibraryIndexStats> {
  const res = await invoke<CmdResult<{ docs: number; pages: number }>>(
    "slab_library_index_stats",
  );
  return unwrap(res);
}


// ---------- Tags ----------

export async function listTags(): Promise<TagRecord[]> {
  const res = await invoke<CmdResult<TagRecord[]>>("slab_library_list_tags");
  return unwrap(res);
}

/**
 * The most recently *applied* tags, newest first (each listed once by its
 * newest application time). Surfaced as "Recently used" quick-chips when
 * tagging a document so the tags you reach for most are one click away.
 * `limit` defaults to 8 on the backend when omitted.
 * v3.44.0 Atlas Recent-Tags.
 */
export async function recentlyUsedTags(limit?: number): Promise<TagRecord[]> {
  const res = await invoke<CmdResult<TagRecord[]>>(
    "slab_library_recently_used_tags",
    { limit: limit ?? null },
  );
  return unwrap(res);
}

/**
 * Document count per tag, as a `Map<tagId, count>`. Every tag in the library
 * is present; a tag attached to no document maps to 0 (the backend LEFT JOINs
 * so unused tags aren't dropped). One GROUP BY round-trip, not N queries.
 * Powers the muted usage count beside each tag in the rail and the
 * "sort by most used" ordering. v3.46.0 Atlas Tag-Usage-Counts.
 */
export async function tagUsageCounts(): Promise<Map<number, number>> {
  const res = await invoke<CmdResult<[number, number][]>>(
    "slab_library_tag_usage_counts",
  );
  return new Map(unwrap(res));
}

/**
 * Delete every tag attached to zero documents, returning the number removed.
 * Reclaims the residue merges and bulk-removes leave behind — a tag whose last
 * document link was detached lingers in the rail at count 0 until something
 * prunes it. Tags carrying even one document are untouched, so no document
 * loses a tag it actually wears. v3.47.0 Atlas Tag-Cleanup.
 */
export async function deleteUnusedTags(): Promise<number> {
  const res = await invoke<CmdResult<number>>(
    "slab_library_delete_unused_tags",
  );
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

/**
 * Update an existing tag's color (pass `null` to clear it back to the default
 * deterministic rendering). Returns the updated tag row. The backend rejects
 * anything that isn't a `#hex` / `hsl()` / `rgb()` color.
 * v3.42.0 Atlas Tag-Color editing.
 */
export async function setTagColor(
  tagId: number,
  color: string | null,
): Promise<TagRecord> {
  const res = await invoke<CmdResult<TagRecord>>("slab_library_set_tag_color", {
    tagId,
    color,
  });
  return unwrap(res);
}

/**
 * Rename a tag everywhere it is used. Documents are linked to tags by id, so
 * the new name shows up on every document and in tag co-occurrence without a
 * migration. Returns the updated tag row so callers can swap it in without a
 * full refetch. The backend rejects an empty name or a name already taken by a
 * different tag. v3.43.0 Atlas Tag-Rename.
 */
export async function renameTag(
  tagId: number,
  newName: string,
): Promise<TagRecord> {
  const res = await invoke<CmdResult<TagRecord>>("slab_library_rename_tag", {
    tagId,
    newName,
  });
  return unwrap(res);
}

/**
 * Set (or clear) the optional freeform description on a tag. Pass `null` — or
 * any string that trims to empty — to clear the column back to `null`. The
 * backend trims the input and rejects oversized text (cap is 500 Unicode
 * scalars). Returns the updated row so callers can swap it in without a
 * refetch. v3.51.0 Atlas Tag-Descriptions.
 */
export async function setTagDescription(
  tagId: number,
  description: string | null,
): Promise<TagRecord> {
  const res = await invoke<CmdResult<TagRecord>>(
    "slab_library_set_tag_description",
    {
      tagId,
      description,
    },
  );
  return unwrap(res);
}

/**
 * Override a library document's displayed `title`. Pass `null` — or any string
 * that trims to empty on the backend — to clear the column back to `null` so
 * the basename fallback resumes. Returns the refreshed `DocumentRecord` (with
 * tags eager-loaded) so the LibraryPanel can splice the card back into the
 * grid without a full `listDocuments` refetch. The backend trims input and
 * rejects oversized text (cap is 500 Unicode scalars). v3.55.0 Atlas
 * Doc-Inspector.
 */
export async function setDocumentTitle(
  docId: number,
  title: string | null,
): Promise<DocumentRecord> {
  const res = await invoke<CmdResult<DocumentRecord>>(
    "slab_library_set_doc_title",
    {
      docId,
      title,
    },
  );
  return unwrap(res);
}

/**
 * Set (or clear) the freeform `notes` on a library document. Pass `null` — or
 * any string that trims to empty on the backend — to clear the column back to
 * `null`. Returns the refreshed `DocumentRecord` (with tags eager-loaded) so
 * the Doc-Inspector drawer can repaint without an extra `listDocuments`
 * round-trip. The backend trims input and rejects oversized text (cap is
 * 4000 Unicode scalars). v3.55.0 Atlas Doc-Inspector.
 */
export async function setDocumentNotes(
  docId: number,
  notes: string | null,
): Promise<DocumentRecord> {
  const res = await invoke<CmdResult<DocumentRecord>>(
    "slab_library_set_doc_notes",
    {
      docId,
      notes,
    },
  );
  return unwrap(res);
}

/**
 * Toggle the `starred` flag on a library document. Idempotent (setting an
 * already-`true` flag to `true` returns the row unchanged). Returns the
 * refreshed `DocumentRecord` (with tags eager-loaded) so the LibraryPanel
 * can splice the card without an extra `listDocuments` round-trip. v3.55.0
 * Atlas Doc-Inspector.
 */
export async function setDocumentStarred(
  docId: number,
  starred: boolean,
): Promise<DocumentRecord> {
  const res = await invoke<CmdResult<DocumentRecord>>(
    "slab_library_set_doc_starred",
    {
      docId,
      starred,
    },
  );
  return unwrap(res);
}

/**
 * Fold the `sourceId` tag into `targetId`: every document that wore the source
 * tag ends up wearing the target, duplicate links collapse keeping the newer
 * `applied_at` (so recently-used order survives), and the source tag row is
 * deleted. Returns the surviving target row. This is the deliberate "these are
 * actually the same tag" path — `renameTag` rejects a name collision rather
 * than merging. The backend errors on an unknown id or merging a tag into
 * itself. v3.45.0 Atlas Tag-Merge.
 */
export async function mergeTags(
  sourceId: number,
  targetId: number,
): Promise<TagRecord> {
  const res = await invoke<CmdResult<TagRecord>>("slab_library_merge_tags", {
    sourceId,
    targetId,
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

/**
 * Atomically attach or detach one tag and return the refreshed document. This
 * preserves unrelated links added by another window while an inline editor is
 * open, unlike replacing the complete tag-id set from a stale snapshot.
 */
export async function setDocumentTag(
  docId: number,
  tagId: number,
  attached: boolean,
): Promise<DocumentRecord> {
  const res = await invoke<CmdResult<DocumentRecord>>(
    "slab_library_set_doc_tag",
    {
      docId,
      tagId,
      attached,
    },
  );
  return unwrap(res);
}

/** Outcome of a bulk tag apply/remove (mirror of `bulk_tag::BulkTagResult`). */
export interface BulkTagResult {
  /** The tag that was applied or removed (resolved / created). */
  tag: TagRecord;
  /** Documents whose tag set actually changed. */
  affected: number;
  /** Document ids in the request, including no-ops and stale ids. */
  total: number;
}

/**
 * Apply a tag (by name, find-or-created) across many documents in one
 * atomic action. Returns the resolved tag plus affected/total counts.
 * v3.41.0 Atlas Bulk Tag-Apply.
 */
export async function bulkApplyTag(
  tagName: string,
  docIds: number[],
): Promise<BulkTagResult> {
  const res = await invoke<CmdResult<BulkTagResult>>(
    "slab_library_bulk_apply_tag",
    { tagName, docIds },
  );
  return unwrap(res);
}

/**
 * Remove a tag (by id) from many documents in one atomic action. The tag
 * row itself is preserved — only the named doc links are detached.
 * v3.41.0 Atlas Bulk Tag-Apply.
 */
export async function bulkRemoveTag(
  tagId: number,
  docIds: number[],
): Promise<BulkTagResult> {
  const res = await invoke<CmdResult<BulkTagResult>>(
    "slab_library_bulk_remove_tag",
    { tagId, docIds },
  );
  return unwrap(res);
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

/**
 * Per-`ocr_state` count snapshot for the OCR Queue Panel's dashboard.
 * v3.52.0 Atlas OCR-Queue Slice 3.
 */
export interface OcrQueueStats {
  /** Pre-classification — legacy import or scanner hasn't seen it. */
  unknown: number;
  /** Scanner decided no OCR needed. */
  text_native: number;
  /** Image-only PDFs awaiting OCR. */
  scanned: number;
  /** Mixed text + scanned pages. */
  mixed: number;
  /** Currently being OCR'd. */
  pending: number;
  /** OCR succeeded — `ocr_output_path` should be set. */
  done: number;
  /** Last OCR attempt failed — `ocr_error` carries the reason. */
  failed: number;
  /** scanned + mixed — what the queue would pull next. */
  pending_total: number;
  /** Every doc, regardless of state. */
  total: number;
}

/** Fetch the queue dashboard counts. Safe to poll; pure read. */
export async function ocrQueueStats(): Promise<OcrQueueStats> {
  const res = await invoke<CmdResult<OcrQueueStats>>(
    "slab_library_ocr_queue_stats",
  );
  return unwrap(res);
}

/**
 * Every `ocr_failed` document, newest first, with `ocr_error`
 * populated. Powers the failure inbox on the OCR Queue Panel.
 * v3.52.0 Atlas OCR-Queue Slice 4.
 */
export async function ocrQueueListFailed(): Promise<DocumentRecord[]> {
  const res = await invoke<CmdResult<DocumentRecord[]>>(
    "slab_library_ocr_queue_list_failed",
  );
  return unwrap(res);
}

/**
 * Re-queue a single document by flipping `ocr_done` / `ocr_failed` /
 * `ocr_pending` back to `scanned` and clearing both the persisted
 * error and any stale output path. Returns the updated row so callers
 * can patch their local doc list in place. v3.52.0 Atlas OCR-Queue
 * Slice 2.
 */
export async function ocrQueueRequeue(
  docId: number,
): Promise<DocumentRecord> {
  const res = await invoke<CmdResult<DocumentRecord>>(
    "slab_library_ocr_queue_requeue",
    { docId },
  );
  return unwrap(res);
}

/**
 * Re-queue every `ocr_failed` document in one shot. Returns the count
 * of rows that flipped. v3.52.0 Atlas OCR-Queue Slice 2 companion.
 */
export async function ocrQueueRequeueAllFailed(): Promise<number> {
  const res = await invoke<CmdResult<number>>(
    "slab_library_ocr_queue_requeue_all_failed",
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
): Promise<CollectionRecord> {
  const res = await invoke<CmdResult<CollectionRecord>>("slab_collection_rename", {
    id,
    name,
  });
  return unwrap(res);
}

export async function collectionDelete(id: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_collection_delete", { id });
  unwrap(res);
}

/**
 * Update a collection's color (or clear it back to `null`). Returns the
 * updated row so the caller can swap it into the rail in place without a
 * round-trip. Pass `null` to clear. v3.53.0 Atlas Collections — Slice 24.
 */
export async function collectionSetColor(
  id: number,
  color: string | null,
): Promise<CollectionRecord> {
  const res = await invoke<CmdResult<CollectionRecord>>("slab_collection_set_color", {
    id,
    color,
  });
  return unwrap(res);
}

/**
 * Persist a new rail order for manual collections. Pass the ids in the
 * order they should appear top-to-bottom. Returns the count of rows whose
 * sort_order actually changed (so the caller can suppress a refresh when
 * nothing moved). Unknown ids are silently skipped — a stale id from a
 * list-vs-reorder race won't crash the UI. v3.53.0 Atlas Collections —
 * Slice 25.
 */
export async function collectionReorder(orderedIds: number[]): Promise<number> {
  const res = await invoke<CmdResult<number>>("slab_collection_reorder", {
    orderedIds,
  });
  return unwrap(res);
}

/**
 * Clone a manual collection — name, icon, color, AND its current document
 * membership — under a new auto-suffixed name (`"X (copy)"`, `"X (copy 2)"`,
 * …). The new row lands at the end of the rail's sort order. Returns the
 * freshly-created row with `doc_count` already populated. v3.53.0 Atlas
 * Collections — Slice 26.
 */
export async function collectionDuplicate(sourceId: number): Promise<CollectionRecord> {
  const res = await invoke<CmdResult<CollectionRecord>>("slab_collection_duplicate", {
    sourceId,
  });
  return unwrap(res);
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

/** Patch for `smartCollectionUpdate`. Each field follows tri-state semantics:
 * - `undefined` (or omitted) = don't touch this field
 * - `null` = clear (only legal for nullable columns: icon, color)
 * - value = set
 */
export interface SmartCollectionPatch {
  name?: string;
  icon?: string | null;
  color?: string | null;
  filter?: LibraryFilter;
}

export async function smartCollectionUpdate(
  id: number,
  patch: SmartCollectionPatch,
): Promise<SmartCollectionRecord> {
  // Only forward fields that were actually provided so the Rust side can
  // tell "omitted" from "explicit null".
  const payload: Record<string, unknown> = {};
  if (patch.name !== undefined) payload.name = patch.name;
  if (patch.icon !== undefined) payload.icon = patch.icon; // null OK
  if (patch.color !== undefined) payload.color = patch.color; // null OK
  if (patch.filter !== undefined) payload.filter = patch.filter;
  const res = await invoke<CmdResult<SmartCollectionRecord>>(
    "slab_smart_collection_update",
    { id, patch: payload },
  );
  return unwrap(res);
}

// ---------------------------------------------------------------
// v3.35.0 "Atlas Presets" — built-in smart-collection templates
// ---------------------------------------------------------------

/** Mirror of `pdf::library::presets::PresetInfo`. */
export interface PresetInfo {
  id: string;
  name: string;
  icon: string;
  color: string;
  description: string;
}

/** List every built-in preset. Static — cheap to call from anywhere. */
export async function presetList(): Promise<PresetInfo[]> {
  const res = await invoke<CmdResult<PresetInfo[]>>("slab_preset_list");
  return unwrap(res);
}

/**
 * Materialize the named preset as a real smart-collection row.
 * Auto-creates any tags the preset references. Throws if the preset
 * name is already used (UNIQUE constraint) — callers should consult
 * `presetAlreadyApplied()` first to grey out the button.
 */
export async function presetApply(
  presetId: string,
): Promise<SmartCollectionRecord> {
  const res = await invoke<CmdResult<SmartCollectionRecord>>(
    "slab_preset_apply",
    { presetId },
  );
  return unwrap(res);
}

/**
 * Return the ids of presets that already exist as smart collections
 * (matched by name). UI uses this to dedupe the picker.
 */
export async function presetAlreadyApplied(): Promise<string[]> {
  const res = await invoke<CmdResult<string[]>>("slab_preset_already_applied");
  return unwrap(res);
}

// ---------------------------------------------------------------
// v3.36.0 "Atlas Personal Presets" — user-saved recipes + .slabpresets
// pack import/export
// ---------------------------------------------------------------

/** Mirror of `pdf::library::personal_presets::PersonalPresetRecord`. */
export interface PersonalPresetRecord {
  id: number;
  name: string;
  icon: string | null;
  color: string | null;
  description: string | null;
  filter: LibraryFilter;
  created_at: number;
  sort_order: number;
}

/** Spec for `personalPresetSave`. Mirrors NewPersonalPreset on Rust side. */
export interface NewPersonalPreset {
  name: string;
  icon: string | null;
  color: string | null;
  description: string | null;
  filter: LibraryFilter;
}

/** Result of importing a `.slabpresets` pack. */
export interface ImportReport {
  imported: number;
  skipped: number;
  renamed: number;
  errors: string[];
}

export async function personalPresetSave(
  spec: NewPersonalPreset,
): Promise<PersonalPresetRecord> {
  const res = await invoke<CmdResult<PersonalPresetRecord>>(
    "slab_personal_preset_save",
    { spec },
  );
  return unwrap(res);
}

export async function personalPresetList(): Promise<PersonalPresetRecord[]> {
  const res = await invoke<CmdResult<PersonalPresetRecord[]>>(
    "slab_personal_preset_list",
  );
  return unwrap(res);
}

export async function personalPresetDelete(id: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_personal_preset_delete", {
    id,
  });
  unwrap(res);
}

export async function personalPresetApply(
  id: number,
): Promise<SmartCollectionRecord> {
  const res = await invoke<CmdResult<SmartCollectionRecord>>(
    "slab_personal_preset_apply",
    { id },
  );
  return unwrap(res);
}

/**
 * Rename a personal preset in place. Trims the new name; empty is
 * rejected with a thrown error; collision with another preset name
 * is rejected by the backend's UNIQUE constraint. Returns the renamed
 * record so the caller can splice it back into its local list without
 * a refetch. v3.40 Slice 76.
 */
export async function personalPresetRename(
  id: number,
  newName: string,
): Promise<PersonalPresetRecord> {
  const res = await invoke<CmdResult<PersonalPresetRecord>>(
    "slab_personal_preset_rename",
    { id, newName },
  );
  return unwrap(res);
}

/**
 * Duplicate a personal preset. The copy gets a fresh sort_order at
 * the bottom of the list and a derived unique name ("<src> (copy)"
 * or "<src> (copy N)"). The copy is INDEPENDENT — editing it doesn't
 * affect the source. v3.40 Slice 76.
 */
export async function personalPresetDuplicate(
  id: number,
): Promise<PersonalPresetRecord> {
  const res = await invoke<CmdResult<PersonalPresetRecord>>(
    "slab_personal_preset_duplicate",
    { id },
  );
  return unwrap(res);
}

/**
 * Export the given personal preset ids (empty = all) to a `.slabpresets`
 * JSON string. Caller is responsible for the file save dialog + write.
 */
export async function personalPresetsExport(
  ids: number[],
): Promise<string> {
  const res = await invoke<CmdResult<string>>("slab_personal_presets_export", {
    ids,
  });
  return unwrap(res);
}

/**
 * Import a `.slabpresets` pack from JSON text. `renameOnConflict = true`
 * appends "(2)", "(3)"... to duplicate names; `false` skips them.
 */
export async function personalPresetsImport(
  packJson: string,
  renameOnConflict = false,
): Promise<ImportReport> {
  const res = await invoke<CmdResult<ImportReport>>(
    "slab_personal_presets_import",
    { packJson, renameOnConflict },
  );
  return unwrap(res);
}

// -----------------------------------------------------------------
// v3.50.0 "Atlas Saved Views" — one-click restorable rail filters.
//
// A saved view is just a NAMED LibraryFilter. Distinct from
// personalPreset (which materializes into a smart collection) and
// from smartCollection (which owns a doc list) — a view simply
// RE-RUNS the saved filter live whenever the user clicks it.
// The whole filter (folder + tag set + match mode + untagged toggle
// + sort) round-trips through serde_json on the backend, so a
// restored view reproduces exactly the same doc set the user pinned.
// -----------------------------------------------------------------

/** Row of `library_saved_views` as decoded for the frontend. */
export interface SavedViewRecord {
  id: number;
  name: string;
  filter: LibraryFilter;
  created_at: number;
  sort_order: number;
  /** v3.56.0 — true when the user has pinned this view to the top of
   *  the rail. Pre-v3.56 snapshots (without the field) decode as false. */
  pinned?: boolean;
}

/** Spec for `savedViewSave`. Mirrors `NewSavedView` on Rust side. */
export interface NewSavedView {
  name: string;
  filter: LibraryFilter;
}

export async function savedViewSave(
  spec: NewSavedView,
): Promise<SavedViewRecord> {
  const res = await invoke<CmdResult<SavedViewRecord>>(
    "slab_library_saved_view_save",
    { spec },
  );
  return unwrap(res);
}

export async function savedViewList(): Promise<SavedViewRecord[]> {
  const res = await invoke<CmdResult<SavedViewRecord[]>>(
    "slab_library_saved_view_list",
  );
  return unwrap(res);
}

export async function savedViewDelete(id: number): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_saved_view_delete", {
    id,
  });
  unwrap(res);
}

/**
 * Rename a saved view. Trims the name; empty rejected; an unchanged
 * name (post-trim) is a no-op returning the existing row; a name that
 * collides with another view's name is rejected by the UNIQUE
 * constraint — both rows are left intact.
 */
export async function savedViewRename(
  id: number,
  newName: string,
): Promise<SavedViewRecord> {
  const res = await invoke<CmdResult<SavedViewRecord>>(
    "slab_library_saved_view_rename",
    { id, newName },
  );
  return unwrap(res);
}

/**
 * Re-pin a saved view's filter in place. Preserves id, name, created_at,
 * and sort_order; only the saved filter blob is swapped. Lets the user
 * tweak the rail (folder / tags / sort / untagged / starred / search) and
 * push the new shape onto an existing pinned view with one click — no
 * delete-and-recreate, no churn on sort_order, no broken id references.
 * Errors if the id no longer exists.
 */
export async function savedViewUpdateFilter(
  id: number,
  filter: LibraryFilter,
): Promise<SavedViewRecord> {
  const res = await invoke<CmdResult<SavedViewRecord>>(
    "slab_library_saved_view_update_filter",
    { id, filter },
  );
  return unwrap(res);
}

/**
 * Duplicate an existing saved view. Filter is copied byte-for-byte; the
 * new row gets a fresh id, fresh created_at, and a fresh sort_order at
 * the bottom of the rail. The duplicate's name is `"<source> (copy)"` (or
 * `"<source> (copy N)"` if the simple "(copy)" is already taken — walked
 * up to 999 to dodge the UNIQUE constraint). The duplicate is independent:
 * editing it later does NOT mutate the source. Errors if the source id no
 * longer exists.
 */
export async function savedViewDuplicate(
  id: number,
): Promise<SavedViewRecord> {
  const res = await invoke<CmdResult<SavedViewRecord>>(
    "slab_library_saved_view_duplicate",
    { id },
  );
  return unwrap(res);
}

/**
 * Pin or unpin a saved view. Pinned views surface at the top of the
 * rail; the API is idempotent (setting the same value twice succeeds
 * without churn). Errors if the id no longer exists. v3.56.0 Atlas
 * Saved-Views-Polish.
 */
export async function savedViewSetPinned(
  id: number,
  pinned: boolean,
): Promise<SavedViewRecord> {
  const res = await invoke<CmdResult<SavedViewRecord>>(
    "slab_library_saved_view_set_pinned",
    { id, pinned },
  );
  return unwrap(res);
}

/**
 * Re-stamp `sort_order` for the supplied view ids: each id's zero-based
 * position becomes its new sort_order. Runs in a single SQLite transaction
 * so partial failures can't leave the rail mid-shuffle. Empty input is a
 * zero no-op. Rejects duplicate or unknown ids (no rows mutated on
 * rejection). The pinned flag is NOT touched — the rail's pinned-first
 * sort order keeps working transparently. v3.56.0 Atlas Saved-Views-Polish.
 */
export async function savedViewReorder(order: number[]): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_saved_view_reorder", {
    order,
  });
  unwrap(res);
}

// -----------------------------------------------------------------
// v3.37.0 "Atlas Smart Folders Hub" — merged built-in + personal preset
// list with persisted display order and pin flags.
// -----------------------------------------------------------------

/**
 * One row in the unified Smart Folders hub. `kind` is `"builtin"` (the id
 * is a built-in preset string like `"invoices"`) or `"personal"` (the id
 * is a numeric personal-preset row id stringified).
 */
export interface SmartFolderEntry {
  kind: "builtin" | "personal";
  id: string;
  name: string;
  icon: string;
  color: string;
  description: string;
  pinned: boolean;
  sort_order: number;
}

/** Reorder spec passed to {@link smartFoldersReorder}. */
export interface SmartFolderOrderItem {
  kind: "builtin" | "personal";
  id: string;
  sort_order: number;
}

/**
 * Fetch every smart folder (built-in + personal) in display order
 * (pinned-first, then persisted order, then alphabetical).
 */
export async function smartFoldersList(): Promise<SmartFolderEntry[]> {
  const res = await invoke<CmdResult<SmartFolderEntry[]>>(
    "slab_smart_folders_list",
  );
  return unwrap(res);
}

/**
 * Persist a new visible order. Caller passes the FULL list; each item's
 * `sort_order` is its zero-based position in the UI. Atomic.
 */
export async function smartFoldersReorder(
  items: SmartFolderOrderItem[],
): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_smart_folders_reorder", {
    items,
  });
  unwrap(res);
}

/** Toggle the pin flag on a single smart folder entry. */
export async function smartFoldersPin(
  kind: "builtin" | "personal",
  id: string,
  pinned: boolean,
): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_smart_folders_pin", {
    kind,
    id,
    pinned,
  });
  unwrap(res);
}

// -----------------------------------------------------------------
// v3.38.0 Atlas Suggest — heuristic Smart Folder suggestions.
// -----------------------------------------------------------------

/**
 * One suggested Smart Folder produced by the local heuristic engine
 * from the user's recent library search history. Mirrors
 * `pdf::library::folder_suggest::Suggestion` 1:1.
 */
export interface FolderSuggestion {
  name: string;
  icon: string;
  color: string;
  query_template: string;
  reason: string;
  cluster_hash: string;
  support: number;
}

/** Up to 3 suggestions; `[]` if the user hasn't searched enough yet. */
export async function librarySuggestionsList(): Promise<FolderSuggestion[]> {
  const res = await invoke<CmdResult<FolderSuggestion[]>>(
    "slab_library_suggestions_list",
  );
  return unwrap(res);
}

/** Dismiss a suggestion permanently by its stable cluster_hash. */
export async function librarySuggestionsDismiss(
  clusterHash: string,
): Promise<void> {
  const res = await invoke<CmdResult<null>>("slab_library_suggestions_dismiss", {
    clusterHash,
  });
  unwrap(res);
}

/** Accept a suggestion: creates a personal preset + auto-dismisses. */
export async function librarySuggestionsAccept(
  suggestion: FolderSuggestion,
): Promise<unknown> {
  const res = await invoke<CmdResult<unknown>>("slab_library_suggestions_accept", {
    suggestion,
  });
  return unwrap(res);
}

/** Total rows in the rolling search log (capped at 500). */
export async function librarySearchLogCount(): Promise<number> {
  const res = await invoke<CmdResult<number>>("slab_library_search_log_count");
  return unwrap(res);
}

// -----------------------------------------------------------------
// v3.39.0 Atlas Tag-Suggest — per-document heuristic tag suggestions.
// -----------------------------------------------------------------

/**
 * One suggested tag for a document, produced locally from the doc's
 * title/filename, the existing tag vocabulary, co-occurrence stats, and
 * a built-in domain dictionary. Mirrors
 * `pdf::library::tag_suggest::TagSuggestion` 1:1.
 */
export interface TagSuggestion {
  tag_name: string;
  score: number;
  source: "vocabulary" | "cooccurrence" | "domain";
  existing: boolean;
}

/** A document plus its suggested tags (bulk endpoint). */
export interface BulkTagSuggestion {
  doc_id: number;
  title: string | null;
  path: string;
  suggestions: TagSuggestion[];
}

/** Up to 5 suggested tags for one document; `[]` if nothing plausible. */
export async function tagSuggestionsForDoc(
  docId: number,
): Promise<TagSuggestion[]> {
  const res = await invoke<CmdResult<TagSuggestion[]>>(
    "slab_library_tag_suggestions_for_doc",
    { docId },
  );
  return unwrap(res);
}

/** Suggest tags for every untagged document (skips zero-suggestion docs). */
export async function tagSuggestionsBulk(
  limit = 50,
): Promise<BulkTagSuggestion[]> {
  const res = await invoke<CmdResult<BulkTagSuggestion[]>>(
    "slab_library_tag_suggestions_bulk_for_untagged",
    { limit },
  );
  return unwrap(res);
}

/** Accept a suggested tag: find-or-create it and attach it to the doc. */
export async function acceptTagSuggestion(
  docId: number,
  tagName: string,
): Promise<TagRecord> {
  const res = await invoke<CmdResult<TagRecord>>(
    "slab_library_tag_suggestion_accept",
    { docId, tagName },
  );
  return unwrap(res);
}

/** Dismiss a suggested tag so it never resurfaces for this doc. */
export async function dismissTagSuggestion(
  docId: number,
  tagName: string,
): Promise<void> {
  const res = await invoke<CmdResult<null>>(
    "slab_library_tag_suggestion_dismiss",
    { docId, tagName },
  );
  unwrap(res);
}

/** Clear all dismissed tag suggestions for a doc (settings escape hatch). */
export async function undismissAllTagSuggestions(
  docId: number,
): Promise<number> {
  const res = await invoke<CmdResult<number>>(
    "slab_library_tag_suggestion_undismiss_all",
    { docId },
  );
  return unwrap(res);
}

/**
 * One element of a bulk-accept request. Mirrors
 * `pdf::library::tag_suggest::AcceptItem`.
 */
export interface TagSuggestionAcceptItem {
  doc_id: number;
  tag_name: string;
}

/**
 * Outcome of a bulk-accept call. `attached` is `[doc_id, TagRecord]` per
 * successful pair; `failed` is `[doc_id, tag_name, reason]` per failure.
 * Per-item failure semantics — a typo in item 12 doesn't undo the 49
 * good accepts. Mirrors `pdf::library::tag_suggest::BulkAcceptResult`.
 */
export interface BulkTagAcceptResult {
  attached: Array<[number, TagRecord]>;
  failed: Array<[number, string, string]>;
}

/**
 * Bulk-accept N (doc_id, tag_name) pairs in one round-trip. Per-item
 * failure semantics; the backend dedupes case- and whitespace-equivalent
 * pairs before applying. Emits a single `library-changed` event after
 * the batch.
 */
export async function acceptTagSuggestionsBulk(
  items: TagSuggestionAcceptItem[],
): Promise<BulkTagAcceptResult> {
  const res = await invoke<CmdResult<BulkTagAcceptResult>>(
    "slab_library_tag_suggestions_accept_bulk",
    { items },
  );
  return unwrap(res);
}

/**
 * One dismissed tag-suggestion row. `dismissed_at` is unix seconds.
 * Mirrors `pdf::library::tag_suggest::DismissedSuggestion`.
 */
export interface DismissedTagSuggestion {
  tag_name: string;
  dismissed_at: number;
}

/** List every dismissed tag-suggestion for a doc, newest first. */
export async function listDismissedTagSuggestions(
  docId: number,
): Promise<DismissedTagSuggestion[]> {
  const res = await invoke<CmdResult<DismissedTagSuggestion[]>>(
    "slab_library_tag_suggestions_list_dismissed",
    { docId },
  );
  return unwrap(res);
}

/**
 * Undo ONE dismissal — the next call to `tagSuggestionsForDoc` will
 * re-include this tag in the candidate set. Returns `true` if a row
 * was actually deleted; `false` if no such dismissal existed.
 */
export async function undismissOneTagSuggestion(
  docId: number,
  tagName: string,
): Promise<boolean> {
  const res = await invoke<CmdResult<boolean>>(
    "slab_library_tag_suggestion_undismiss_one",
    { docId, tagName },
  );
  return unwrap(res);
}

/**
 * Bulk-suggest over any `LibraryFilter`. The untagged-shortcut
 * `tagSuggestionsBulk` covers the lightest case; this is the proper
 * review-surface entry point that lets a saved view, smart-collection
 * clause tree, or starred-only toggle pre-narrow the candidate set.
 *
 * `limit` overrides any filter-embedded limit so the scan stays bounded.
 */
export async function tagSuggestionsBulkForFilter(
  filter: LibraryFilter,
  limit = 50,
): Promise<BulkTagSuggestion[]> {
  const res = await invoke<CmdResult<BulkTagSuggestion[]>>(
    "slab_library_tag_suggestions_bulk_for_filter",
    { filter, limit },
  );
  return unwrap(res);
}

/**
 * Compact stats for the tag-suggest review badge. Mirrors
 * `pdf::library::tag_suggest::TagSuggestionStats`.
 */
export interface TagSuggestionStats {
  untagged_docs_with_suggestions: number;
  dismissed_total: number;
}

/**
 * Cheap probe for the toolbar badge — the count of recently-seen
 * untagged docs that would yield at least one suggestion right now,
 * plus the corpus-wide dismissal count.
 *
 * `sampleCap` defaults to 200 server-side; the UI renders `200+` when
 * the working set saturates the cap.
 */
export async function tagSuggestionStats(
  sampleCap?: number,
): Promise<TagSuggestionStats> {
  const res = await invoke<CmdResult<TagSuggestionStats>>(
    "slab_library_tag_suggestion_stats",
    { sampleCap },
  );
  return unwrap(res);
}
