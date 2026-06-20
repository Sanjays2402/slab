// TS client for the Beacon Cache Inspector (v3.54.0 round-7).
//
// Wraps the new Tauri commands that round-7 added on top of the
// existing `slab_beacon_index_*` set. Lives in its own module rather
// than `library.ts` because the embedding index is a different DB file
// (`~/.slab/beacon-index.sqlite`) than the library registry
// (`~/.slab/library.sqlite`); keeping the namespaces apart in TS
// matches that separation in Rust.

import { invoke } from "@tauri-apps/api/core";

/** Generic wire shape mirrored from `lib.rs::CmdResult`. */
type CmdResult<T> =
  | { kind: "ok"; value: T }
  | { kind: "err"; message: string };

function unwrap<T>(res: CmdResult<T>): T {
  if (res.kind === "ok") return res.value;
  throw new Error(res.message);
}

/**
 * Mirror of `ai::embedding_index::IndexedPdfRecord`. One row per PDF
 * currently in the embedding index, with chunk count joined in via a
 * LEFT JOIN so a PDF whose chunks got zeroed still surfaces.
 * v3.54.0 Atlas Beacon-Cache — Slice 28.
 */
export interface IndexedPdfRecord {
  pdf_hash: string;
  pdf_path: string;
  pages: number;
  embed_model: string;
  /** Unix-seconds timestamp the row was first written. */
  indexed_at: number;
  chunks: number;
}

/**
 * Mirror of `ai::embedding_index::ModelBucket`. Per-`embed_model`
 * aggregate the inspector's dashboard renders as tiles. Sorted by
 * chunk count DESC, model name ASC tie-break. v3.54.0 Atlas
 * Beacon-Cache — Slice 30.
 */
export interface ModelBucket {
  embed_model: string;
  pdfs: number;
  chunks: number;
}

/**
 * List every PDF currently in the embedding index, newest first.
 * Powers the inspector's main table — one round-trip even on a 10k-PDF
 * index because the chunk count is joined in.
 */
export async function beaconIndexList(): Promise<IndexedPdfRecord[]> {
  const res = await invoke<CmdResult<IndexedPdfRecord[]>>(
    "slab_beacon_index_list",
  );
  return unwrap(res);
}

/**
 * Bulk-delete every PDF whose hash is in `pdfHashes`. Returns the count
 * actually removed — unknown hashes silently skip so a stale id from a
 * list-vs-forget race can't crash the inspector.
 */
export async function beaconIndexForgetMany(
  pdfHashes: string[],
): Promise<number> {
  const res = await invoke<CmdResult<number>>(
    "slab_beacon_index_forget_many",
    { pdfHashes },
  );
  return unwrap(res);
}

/**
 * Per-embed-model bucket counts in one round-trip. The inspector
 * dashboard renders one tile per bucket and surfaces a "mixed model"
 * warning when `buckets.length > 1` — at query time Beacon's
 * dim-mismatch skip silently drops the loser's chunks, so making the
 * split visible matters.
 */
export async function beaconIndexStatsByModel(): Promise<ModelBucket[]> {
  const res = await invoke<CmdResult<ModelBucket[]>>(
    "slab_beacon_index_stats_by_model",
  );
  return unwrap(res);
}
