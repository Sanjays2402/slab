// Beacon Cache Inspector view-core — v3.40.0 "Atlas II" Slice 1.
//
// Pure, DOM-free model for the Beacon Cache Inspector
// (`BeaconCachePanel.svelte`). Round-7 ("Atlas Beacon-Cache") shipped
// the panel as a flat table: every indexed PDF with a hard-coded
// newest/oldest/chunks sort toggle, multi-select + bulk-forget, a
// per-model dashboard, and a stale section. But the table had NO search
// (a 10k-PDF index is a wall of rows you can only scroll), the model
// tiles were inert read-outs, the sort was a fixed 3-button toggle with
// no direction control, and the only empty copy was a single line.
//
// This module owns the find / sort / facet / summarize math as pure
// functions so every branch is unit-tested without a DOM — the same
// pure-core / thin-shell discipline as `paletteSearch.ts`,
// `toastStack.ts`, and `shortcutsOverlay.ts`. Search REUSES the tested
// palette scorer (`scorePaletteField`) rather than rolling a second
// fuzzy matcher, so the highlight + ranking behaviour is identical to
// the command palette and the "?" cheat sheet.

import { scorePaletteField, type PaletteRange } from "./paletteSearch";

/**
 * The fields the view-core reads off an indexed PDF row. Mirrors
 * `IndexedPdfRecord` (beaconCache.ts) but kept structural so the pure
 * helpers stay decoupled from the wire type and trivially testable.
 */
export interface BeaconPdfLike {
  pdf_hash: string;
  pdf_path: string;
  pages: number;
  embed_model: string;
  /** Unix-seconds timestamp the row was first written. */
  indexed_at: number;
  chunks: number;
}

/**
 * Extract the file basename from a path, tolerating both POSIX `/` and
 * Windows `\` separators. Shared by search (match on filename) and the
 * component (display) so the two can never disagree on what "the name"
 * is. A trailing separator or empty path degrades gracefully.
 */
export function beaconBasename(path: string): string {
  if (!path) return "";
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(i + 1) : path;
}

// --- Slice 1: filter-as-you-type search ------------------------------
//
// The inspector had no way to find a PDF by name in a large index. This
// adds a tested fuzzy filter over the row's basename (highlighted),
// with secondary, un-highlighted matches on the full path (folder),
// embed model, and content hash — the same "visible field highlighted,
// hidden fields matched silently" contract the palette uses for its
// title-vs-keywords split.

/** One row that survived the search, with the basename highlight ranges. */
export interface BeaconSearchHit<T extends BeaconPdfLike = BeaconPdfLike> {
  record: T;
  /** Best match score across all searched fields; >0 means it matched. */
  score: number;
  /** Ranges into the BASENAME only (path/model/hash hits highlight nothing). */
  nameRanges: PaletteRange[];
}

// Secondary-field weights: a basename hit always outranks a same-tier
// folder/model/hash hit, mirroring the palette's title-over-keywords
// weighting so ranking feels consistent across surfaces.
const WEIGHT_NAME = 1;
const WEIGHT_PATH = 0.6;
const WEIGHT_MODEL = 0.5;
const WEIGHT_HASH = 0.4;

/**
 * Filter `pdfs` to the rows matching `query`, each carrying the basename
 * highlight ranges. An empty/blank query passes every row through with
 * a neutral score and no highlight (so the caller renders the full list
 * unmarked). Order is preserved from the input — ranking/sorting is the
 * job of the sort slice, this only decides membership + highlight.
 *
 * A row matches if the query hits its basename (highlighted), full path
 * (folder search), embed model, or content hash. The score is the max
 * of the weighted per-field scores; `nameRanges` is populated only when
 * the basename itself matched, so a folder-only hit never paints
 * confusing marks on the filename.
 */
export function searchIndexedPdfs<T extends BeaconPdfLike>(
  pdfs: readonly T[],
  query: string,
): BeaconSearchHit<T>[] {
  if (!Array.isArray(pdfs)) return [];
  const q = (query ?? "").trim();
  if (!q) return pdfs.map((record) => ({ record, score: 1, nameRanges: [] }));

  const out: BeaconSearchHit<T>[] = [];
  for (const record of pdfs) {
    if (!record) continue;
    const name = beaconBasename(record.pdf_path);
    const nameScore = scorePaletteField(q, name);
    const pathScore = scorePaletteField(q, record.pdf_path ?? "");
    const modelScore = scorePaletteField(q, record.embed_model ?? "");
    const hashScore = scorePaletteField(q, record.pdf_hash ?? "");
    const score = Math.max(
      nameScore.score * WEIGHT_NAME,
      pathScore.score * WEIGHT_PATH,
      modelScore.score * WEIGHT_MODEL,
      hashScore.score * WEIGHT_HASH,
    );
    if (score > 0) {
      out.push({
        record,
        score,
        nameRanges: nameScore.score > 0 ? nameScore.ranges : [],
      });
    }
  }
  return out;
}

// --- Slice 2: multi-field sort + direction -----------------------------
//
// Round-7 shipped a fixed 3-button toggle (newest / oldest / chunks)
// with the direction baked into each button. That can't sort by name or
// model, and "newest vs oldest" is really one field (indexed_at) with
// two directions wearing two buttons. This generalizes to a proper
// column model: pick a FIELD, pick a DIRECTION, with a caret showing
// which way. Clicking the active column flips direction; clicking a new
// column switches to it at that field's natural default direction.

/** A sortable column of the indexed-PDF table. */
export type BeaconSortField = "name" | "model" | "chunks" | "pages" | "indexed";

/** Sort direction. */
export type BeaconSortDir = "asc" | "desc";

/** The full sort state: which column, which way. */
export interface BeaconSort {
  field: BeaconSortField;
  dir: BeaconSortDir;
}

/** Every sortable field, in display order (drives the header buttons). */
export const BEACON_SORT_FIELDS: readonly BeaconSortField[] = [
  "name",
  "model",
  "chunks",
  "pages",
  "indexed",
];

/** Human label for a sort field (header button text). */
export function beaconSortLabel(field: BeaconSortField): string {
  switch (field) {
    case "name":
      return "Name";
    case "model":
      return "Model";
    case "chunks":
      return "Chunks";
    case "pages":
      return "Pages";
    case "indexed":
      return "Indexed";
    default:
      return field;
  }
}

/**
 * The natural default direction when a user FIRST selects a column.
 * Text fields read best A->Z (asc); count/recency fields read best
 * biggest/newest first (desc) — the same convention Finder/Linear use.
 */
export function beaconDefaultDir(field: BeaconSortField): BeaconSortDir {
  return field === "name" || field === "model" ? "asc" : "desc";
}

/**
 * Resolve the next sort state when a column header is clicked. Clicking
 * the ACTIVE column flips its direction; clicking a DIFFERENT column
 * switches to it at that field's natural default direction. Pure.
 */
export function cycleBeaconSort(
  current: BeaconSort,
  clicked: BeaconSortField,
): BeaconSort {
  if (current.field === clicked) {
    return { field: clicked, dir: current.dir === "asc" ? "desc" : "asc" };
  }
  return { field: clicked, dir: beaconDefaultDir(clicked) };
}

/**
 * Sort `pdfs` by the given sort state, returning a NEW array (input is
 * never mutated). Every comparison falls back to a stable `pdf_hash`
 * tie-break so equal rows keep a deterministic order across renders.
 * Name sorts case-insensitively on the basename; model on embed_model;
 * chunks/pages/indexed numerically. A null/garbage list -> [].
 */
export function sortIndexedPdfs<T extends BeaconPdfLike>(
  pdfs: readonly T[],
  sort: BeaconSort,
): T[] {
  if (!Array.isArray(pdfs)) return [];
  const out = pdfs.slice();
  const sign = sort.dir === "asc" ? 1 : -1;
  out.sort((a, b) => {
    let primary = 0;
    switch (sort.field) {
      case "name":
        primary = beaconBasename(a.pdf_path).localeCompare(
          beaconBasename(b.pdf_path),
          undefined,
          { sensitivity: "base", numeric: true },
        );
        break;
      case "model":
        primary = (a.embed_model ?? "").localeCompare(b.embed_model ?? "", undefined, {
          sensitivity: "base",
        });
        break;
      case "chunks":
        primary = (a.chunks ?? 0) - (b.chunks ?? 0);
        break;
      case "pages":
        primary = (a.pages ?? 0) - (b.pages ?? 0);
        break;
      case "indexed":
        primary = (a.indexed_at ?? 0) - (b.indexed_at ?? 0);
        break;
    }
    if (primary !== 0) return sign * primary;
    // Stable, direction-independent tie-break so equal rows never jitter.
    return a.pdf_hash.localeCompare(b.pdf_hash);
  });
  return out;
}
