// OCR Queue Panel view-core — v3.56.0 "Atlas VI" Slice 1.
//
// Pure, DOM-free model for the OCR Queue Panel (`OcrQueuePanel.svelte`).
// Round-7-era "Atlas OCR-Queue" shipped the panel as two flat lists: a
// failure inbox (every `ocr_failed` doc, each naming its `ocr_error`)
// and a pending preview (the next 20 scanned/mixed docs the worker would
// pick up), with per-row Run/Retry/Open and header Run-all/Retry-all.
// But the lists had NO search (a 200-failure inbox is a wall of rows you
// can only scroll), NO sort, the failure reasons were repeated noise
// with no grouping (190 docs failing for one root cause read as 190
// separate rows), the pending preview was hard-capped at 20 with a dead
// "+N more" you could never reach, and the whole surface was mouse-only
// with zero pure-core tests.
//
// This module owns the find / sort / reason-grouping / cursor / summarize
// math as pure functions so every branch is unit-tested without a DOM —
// the same pure-core / thin-shell discipline as `beaconCacheView.ts`,
// `librarySearchView.ts`, `paletteSearch.ts`, and `toastStack.ts`.
// Search and cursor REUSE the tested palette core (`scorePaletteField`,
// `classifyPaletteNav`, `nextPaletteIndex`) rather than rolling a second
// fuzzy / nav engine, so highlight + wrap behaviour is identical to the
// command palette, the "?" cheat sheet, and the Beacon Cache Inspector.

import { scorePaletteField, type PaletteRange } from "./paletteSearch";
import {
  classifyPaletteNav,
  nextPaletteIndex,
  type PaletteNavIntent,
} from "./paletteSearch";

/**
 * The fields the view-core reads off an OCR-queue document row. Mirrors
 * `DocumentRecord` (library.ts) but kept structural so the pure helpers
 * stay decoupled from the wire type and trivially testable. Both the
 * failure inbox and the pending queue are lists of these.
 */
export interface OcrDocLike {
  id: number;
  path: string;
  /** User title override; the row falls back to the basename when null. */
  title: string | null;
  /** Page count, or null for an un-probed row. */
  pages: number | null;
  /** One of the `OcrState` values ("scanned" / "mixed" / "ocr_failed"…). */
  ocr_state: string;
  /** Captured failure reason on `ocr_failed` rows; null otherwise. */
  ocr_error: string | null;
}

/**
 * Extract the file basename from a path, tolerating both POSIX `/` and
 * Windows `\` separators. Shared by search (match on filename) and the
 * component (display) so the two can never disagree on what "the name"
 * is. A trailing separator or empty path degrades gracefully.
 */
export function ocrBasename(path: string): string {
  if (!path) return "";
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(i + 1) : path;
}

/**
 * The folder portion of a path (everything before the basename), with
 * both separators tolerated. Used as the secondary "folder" sort key and
 * a silent search field. An empty path or a bare filename -> "".
 */
export function ocrFolder(path: string): string {
  if (!path) return "";
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i > 0 ? path.slice(0, i) : "";
}

// --- Slice 1: filter-as-you-type search ------------------------------
//
// Neither list had a way to find a doc by name. This adds a tested fuzzy
// filter over the row's basename (highlighted), with secondary,
// un-highlighted matches on the full path (folder), the OCR state, and —
// crucially for the failure inbox — the captured error reason, so you can
// type "tesseract" and surface every doc that failed for that reason.
// Same "visible field highlighted, hidden fields matched silently"
// contract the palette uses for its title-vs-keywords split.

/** One row that survived the search, with the basename highlight ranges. */
export interface OcrSearchHit<T extends OcrDocLike = OcrDocLike> {
  record: T;
  /** Best match score across all searched fields; >0 means it matched. */
  score: number;
  /** Ranges into the BASENAME only (path/state/error hits highlight nothing). */
  nameRanges: PaletteRange[];
}

// Secondary-field weights: a basename hit always outranks a same-tier
// folder/state/error hit, mirroring the palette's title-over-keywords
// weighting so ranking feels consistent across surfaces.
const WEIGHT_NAME = 1;
const WEIGHT_PATH = 0.6;
const WEIGHT_ERROR = 0.5;
const WEIGHT_STATE = 0.4;

/**
 * Filter `docs` to the rows matching `query`, each carrying the basename
 * highlight ranges. An empty/blank query passes every row through with a
 * neutral score and no highlight (so the caller renders the full list
 * unmarked). Input order is preserved — ranking/sorting is the sort
 * slice's job; this only decides membership + highlight.
 *
 * A row matches if the query hits its basename (highlighted), full path
 * (folder search), OCR state, or captured error reason. The score is the
 * max of the weighted per-field scores; `nameRanges` is populated only
 * when the basename itself matched, so a reason-only hit never paints
 * confusing marks on the filename.
 */
export function searchOcrDocs<T extends OcrDocLike>(
  docs: readonly T[],
  query: string,
): OcrSearchHit<T>[] {
  if (!Array.isArray(docs)) return [];
  const q = (query ?? "").trim();
  if (!q) return docs.map((record) => ({ record, score: 1, nameRanges: [] }));

  const out: OcrSearchHit<T>[] = [];
  for (const record of docs) {
    if (!record) continue;
    const name = ocrBasename(record.path);
    const nameScore = scorePaletteField(q, name);
    const pathScore = scorePaletteField(q, record.path ?? "");
    const errScore = scorePaletteField(q, record.ocr_error ?? "");
    const stateScore = scorePaletteField(q, record.ocr_state ?? "");
    const score = Math.max(
      nameScore.score * WEIGHT_NAME,
      pathScore.score * WEIGHT_PATH,
      errScore.score * WEIGHT_ERROR,
      stateScore.score * WEIGHT_STATE,
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
// Both lists shipped in a fixed order (failures newest-first, pending in
// worker order) with no control. This adds a proper column model shared
// by both lists: pick a FIELD, pick a DIRECTION, with a caret showing
// which way. Clicking the active column flips direction; clicking a new
// column switches to it at that field's natural default direction.

/** A sortable column of the OCR-queue lists. */
export type OcrSortField = "name" | "folder" | "pages" | "state";

/** Sort direction. */
export type OcrSortDir = "asc" | "desc";

/** The full sort state: which column, which way. */
export interface OcrSort {
  field: OcrSortField;
  dir: OcrSortDir;
}

/** Every sortable field, in display order (drives the header buttons). */
export const OCR_SORT_FIELDS: readonly OcrSortField[] = [
  "name",
  "folder",
  "pages",
  "state",
];

/** Human label for a sort field (header button text). */
export function ocrSortLabel(field: OcrSortField): string {
  switch (field) {
    case "name":
      return "Name";
    case "folder":
      return "Folder";
    case "pages":
      return "Pages";
    case "state":
      return "State";
    default:
      return field;
  }
}

/**
 * The natural default direction when a user FIRST selects a column. Text
 * fields read best A->Z (asc); the page count reads best biggest-first
 * (desc) — the same convention Finder/Linear use.
 */
export function ocrDefaultDir(field: OcrSortField): OcrSortDir {
  return field === "pages" ? "desc" : "asc";
}

/**
 * Resolve the next sort state when a column header is clicked. Clicking
 * the ACTIVE column flips its direction; clicking a DIFFERENT column
 * switches to it at that field's natural default direction. Pure.
 */
export function cycleOcrSort(current: OcrSort, clicked: OcrSortField): OcrSort {
  if (current.field === clicked) {
    return { field: clicked, dir: current.dir === "asc" ? "desc" : "asc" };
  }
  return { field: clicked, dir: ocrDefaultDir(clicked) };
}

/**
 * Whether `x` is a structurally valid OcrSort (a known field + a known
 * direction). The type guard shared by the panel and the localStorage
 * shell (ocrSortStore.ts) so a corrupt or schema-drifted persisted value
 * can never seat itself into the panel's sort state. Pure, garbage-safe.
 */
export function isOcrSort(x: unknown): x is OcrSort {
  if (typeof x !== "object" || x === null) return false;
  const s = x as { field?: unknown; dir?: unknown };
  return (
    OCR_SORT_FIELDS.includes(s.field as OcrSortField) &&
    (s.dir === "asc" || s.dir === "desc")
  );
}

/**
 * Sort `docs` by the given sort state, returning a NEW array (input is
 * never mutated). Every comparison falls back to a stable `id` tie-break
 * so equal rows keep a deterministic order across renders. Name sorts
 * case-insensitively + numeric-aware on the basename (file2 < file10),
 * folder on the dirname, pages numerically, state alphabetically. A
 * null/garbage list -> [].
 */
export function sortOcrDocs<T extends OcrDocLike>(
  docs: readonly T[],
  sort: OcrSort,
): T[] {
  if (!Array.isArray(docs)) return [];
  const out = docs.slice();
  const sign = sort.dir === "asc" ? 1 : -1;
  out.sort((a, b) => {
    let primary = 0;
    switch (sort.field) {
      case "name":
        primary = ocrBasename(a.path).localeCompare(ocrBasename(b.path), undefined, {
          sensitivity: "base",
          numeric: true,
        });
        break;
      case "folder":
        primary = ocrFolder(a.path).localeCompare(ocrFolder(b.path), undefined, {
          sensitivity: "base",
          numeric: true,
        });
        break;
      case "pages":
        primary = (a.pages ?? 0) - (b.pages ?? 0);
        break;
      case "state":
        primary = (a.ocr_state ?? "").localeCompare(b.ocr_state ?? "", undefined, {
          sensitivity: "base",
        });
        break;
    }
    if (primary !== 0) return sign * primary;
    // Stable, direction-independent tie-break so equal rows never jitter.
    return (a.id ?? 0) - (b.id ?? 0);
  });
  return out;
}

// --- Slice 3: failure-reason grouping + facet --------------------------
//
// A real OCR failure inbox is dominated by a handful of ROOT causes:
// tesseract not installed, an encrypted PDF, a timeout. Raw `ocr_error`
// strings are noisy (paths, errno codes, line numbers) so 190 docs that
// all failed because tesseract isn't on PATH read as 190 distinct rows.
// This canonicalizes each error to a short BUCKET label, surfaces the
// buckets as clickable facets with counts (click a reason to triage just
// that cause), and names the dominant reason — turning a wall of rows
// into "190 docs · Tesseract not installed -> one click to select".

/** A normalized failure-reason bucket label. */
export const OCR_REASON_UNKNOWN = "Unknown error";

/**
 * Normalize a raw `ocr_error` string to a short, stable bucket label so
 * docs that failed for the same root cause group together. Matching is
 * case-insensitive and ordered most-specific-first. A null/blank error
 * -> "Unknown error". An unrecognized error collapses to its trimmed
 * first line, capped at 60 chars (so even novel reasons still bucket by
 * their headline rather than scattering on a trailing path/errno).
 */
export function canonicalizeOcrError(err: string | null | undefined): string {
  const raw = (err ?? "").trim();
  if (!raw) return OCR_REASON_UNKNOWN;
  const lc = raw.toLowerCase();

  // Tesseract binary missing — the single most common OCR failure.
  if (
    /tesseract/.test(lc) &&
    /(not on path|not found|command not found|no such file|cannot find|missing|enoent|os error 2)/.test(lc)
  ) {
    return "Tesseract not installed";
  }
  // A timeout / cancellation.
  if (/(time\s?out|timed out|deadline exceeded|cancell?ed)/.test(lc)) {
    return "Timed out";
  }
  // Encrypted / password-protected source PDF.
  if (/(encrypt|password|protected)/.test(lc)) {
    return "Encrypted PDF";
  }
  // Damaged / unparseable source PDF.
  if (/(corrupt|malformed|invalid pdf|damaged|not a pdf|broken|parse error|failed to (open|parse|load))/.test(lc)) {
    return "Damaged PDF";
  }
  // Filesystem permission problem.
  if (/(permission denied|access denied|eacces|not permitted|eperm)/.test(lc)) {
    return "Permission denied";
  }
  // Resource exhaustion.
  if (/(out of memory|oom|cannot allocate|enomem|memory)/.test(lc)) {
    return "Out of memory";
  }
  // Disk full / write failure.
  if (/(no space|disk full|enospc|write failed)/.test(lc)) {
    return "Disk full";
  }

  // Unrecognized: collapse to the first line, capped, so a novel reason
  // still groups by its headline instead of scattering on trailing noise.
  const firstLine = raw.split(/\r?\n/, 1)[0].trim();
  if (firstLine.length <= 60) return firstLine;
  return firstLine.slice(0, 57).trimEnd() + "\u2026";
}

/** One failure-reason bucket: the canonical label + how many docs wear it. */
export interface OcrReasonBucket {
  reason: string;
  count: number;
}

/**
 * Group a failure list by canonical reason, returning buckets sorted by
 * count descending (the dominant cause first), ties broken alphabetically
 * by label so the order is stable. A null/empty list -> []. Rows are
 * canonicalized via `canonicalizeOcrError` so noisy raw strings collapse.
 */
export function groupFailureReasons<T extends OcrDocLike>(
  failed: readonly T[],
): OcrReasonBucket[] {
  if (!Array.isArray(failed)) return [];
  const counts = new Map<string, number>();
  for (const doc of failed) {
    if (!doc) continue;
    const reason = canonicalizeOcrError(doc.ocr_error);
    counts.set(reason, (counts.get(reason) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([reason, count]) => ({ reason, count }))
    .sort((a, b) => b.count - a.count || a.reason.localeCompare(b.reason));
}

/** A reason bucket annotated with its share-of-total for a mini bar. */
export interface OcrReasonShare extends OcrReasonBucket {
  /** This bucket's count over the TOTAL failures, 0..1. */
  fraction: number;
  /** This bucket's count over the LARGEST bucket, 0..1 (bar width). */
  scaled: number;
  /** Rounded whole-percent of total, for the SR label. */
  percent: number;
}

/**
 * Annotate reason buckets with proportional shares so the facet pills can
 * render a mini bar showing each cause's weight at a glance — turning a wall
 * of equal-looking pills into a histogram where the dominant cause is
 * visibly biggest. `fraction` is share of total; `scaled` is share of the
 * largest bucket (so the top cause fills the bar, the rest scale relative).
 * Stable order (reuses groupFailureReasons sorting). Empty/garbage -> [].
 */
export function reasonShareBars(buckets: readonly OcrReasonBucket[]): OcrReasonShare[] {
  if (!Array.isArray(buckets) || buckets.length === 0) return [];
  const total = buckets.reduce((s, b) => s + (b?.count > 0 ? b.count : 0), 0);
  const max = buckets.reduce((m, b) => Math.max(m, b?.count > 0 ? b.count : 0), 0);
  if (total <= 0 || max <= 0) return [];
  return buckets
    .filter((b) => b && b.count > 0)
    .map((b) => ({
      reason: b.reason,
      count: b.count,
      fraction: b.count / total,
      scaled: b.count / max,
      percent: Math.round((b.count / total) * 100),
    }));
}

/**
 * Compose a full hover/focus tooltip for one reason share, e.g.
 * "Tesseract not installed: 190 of 240 failures (79%)". Numbers are
 * grouped (1,234) and the failure noun pluralizes. With a single bucket
 * the "of total" is dropped (it's redundant). A null share -> "". This is
 * the keyboard-reachable detail behind a pill that otherwise shows just
 * its bar + count, so the exact weight reads on hover/focus. Pure.
 */
export function describeReasonShare(
  share: OcrReasonShare | null | undefined,
  total: number,
): string {
  if (!share || !(share.count > 0)) return "";
  const t = Number.isFinite(total) && total > 0 ? Math.floor(total) : share.count;
  const noun = t === 1 ? "failure" : "failures";
  if (share.count >= t) {
    return `${share.reason}: all ${t.toLocaleString()} ${noun}`;
  }
  return `${share.reason}: ${share.count.toLocaleString()} of ${t.toLocaleString()} ${noun} (${share.percent}%)`;
}

/**
 * Filter `failed` to rows whose canonical reason equals `reason`. A
 * null/empty reason (no facet) passes every row through unchanged. The
 * returned array is always a new array (never the input reference) so
 * callers can treat it as an immutable derivation. Null list -> [].
 */
export function filterByReason<T extends OcrDocLike>(
  failed: readonly T[],
  reason: string | null,
): T[] {
  if (!Array.isArray(failed)) return [];
  if (!reason) return failed.slice();
  return failed.filter((d) => d && canonicalizeOcrError(d.ocr_error) === reason);
}

/**
 * Reconcile an active reason facet against the live bucket list. If the
 * faceted reason no longer exists (its last failure was retried, or a
 * refresh dropped it), the facet is stale and must clear so the inbox
 * doesn't silently show zero rows with no way back. Returns the reason to
 * keep, or null to drop the facet. Pure; tolerant of a null/garbage list.
 */
export function reconcileReasonFacet(
  active: string | null,
  buckets: readonly OcrReasonBucket[],
): string | null {
  if (!active) return null;
  if (!Array.isArray(buckets)) return null;
  return buckets.some((b) => b && b.reason === active) ? active : null;
}

/**
 * Compose a one-line headline naming the dominant failure reason, e.g.
 * "Most failures: Tesseract not installed (190)". When a single bucket
 * holds every failure the count is dropped ("All failures: Encrypted
 * PDF"). An empty list -> "". Pure.
 */
export function describeDominantReason(buckets: readonly OcrReasonBucket[]): string {
  if (!Array.isArray(buckets) || buckets.length === 0) return "";
  const top = buckets[0];
  const total = buckets.reduce((n, b) => n + (b?.count ?? 0), 0);
  if (buckets.length === 1) return `All failures: ${top.reason}`;
  return `Most failures: ${top.reason} (${top.count.toLocaleString()})`;
}

// --- Slice 3c: per-reason "Retry all <reason>" ------------------------
//
// The failure facet (Slice 3) lets you SEE just one root cause, but the
// only retry affordances were per-row "Retry" and the blanket header
// "Retry all" (every failure, every reason). When 190 docs failed for
// "Tesseract not installed" and you've just installed tesseract, you
// want to retry exactly that bucket — not the 4 encrypted PDFs that will
// only fail again. This collects the ids of the faceted bucket so the
// component can loop `ocrQueueRequeue` over precisely those rows, and
// composes the honest button label naming the reason + count.

/**
 * Collect the ids of every failure whose canonical reason equals
 * `reason`, in input order. This is the exact set a "Retry all
 * <reason>" action re-queues — the same membership `filterByReason`
 * decides, projected to ids so the component can loop `ocrQueueRequeue`
 * without re-canonicalizing. A null/empty reason yields [] (the blanket
 * "Retry all" already covers "everything"); a null list -> []. Pure.
 */
export function collectReasonRetryIds<T extends OcrDocLike>(
  failed: readonly T[],
  reason: string | null,
): number[] {
  if (!Array.isArray(failed) || !reason) return [];
  const out: number[] = [];
  for (const d of failed) {
    if (!d) continue;
    if (canonicalizeOcrError(d.ocr_error) === reason) out.push(d.id);
  }
  return out;
}

/**
 * Compose the per-reason retry button label, e.g. "Retry 190 ·
 * Tesseract not installed". The count is thousands-grouped + pluralized
 * implicitly via the reason context (the bucket pill already shows the
 * noun). A zero count or blank reason -> "" so the component hides the
 * button. Pure.
 */
export function describeReasonRetry(reason: string | null, count: number): string {
  const n = Math.max(0, Math.floor(Number.isFinite(count) ? count : 0));
  if (!reason || n <= 0) return "";
  return `Retry ${n.toLocaleString()} \u00b7 ${reason}`;
}

// --- Slice 3b: pending-state facet -------------------------------------
//
// The pending list mixes two genuinely different kinds of work: fully
// image-only ("scanned") pages that need a complete OCR pass, and "mixed"
// docs that already have some embedded text and only need the scanned
// pages filled in. They cost differently and a user often wants to triage
// just one kind ("run the cheap mixed ones first"). This mirrors the
// failure-reason facet for the pending queue: group by ocr_state, render
// clickable count pills, filter to one state, and auto-clear a stale
// facet — the same shape as groupFailureReasons / filterByReason /
// reconcileReasonFacet so the two facets behave identically.

/** A pending-state bucket: the raw ocr_state + how many pending docs wear it. */
export interface OcrStateBucket {
  state: string;
  count: number;
}

/**
 * Human label for a pending ocr_state pill. "scanned" reads as
 * "Image-only" and "mixed" as "Mixed pages" (matching the per-row meta
 * line); any other state passes through unchanged so a novel pending
 * state still renders a sensible pill rather than a raw token.
 */
export function pendingStateLabel(state: string): string {
  switch (state) {
    case "scanned":
      return "Image-only";
    case "mixed":
      return "Mixed pages";
    default:
      return state || "Unknown";
  }
}

/**
 * Group a pending list by ocr_state, returning buckets sorted by count
 * descending (dominant kind first), ties broken alphabetically by state
 * so the order is stable. A null/empty list -> []. Rows with a blank
 * state bucket under "unknown".
 */
export function groupPendingStates<T extends OcrDocLike>(
  pending: readonly T[],
): OcrStateBucket[] {
  if (!Array.isArray(pending)) return [];
  const counts = new Map<string, number>();
  for (const doc of pending) {
    if (!doc) continue;
    const state = doc.ocr_state || "unknown";
    counts.set(state, (counts.get(state) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([state, count]) => ({ state, count }))
    .sort((a, b) => b.count - a.count || a.state.localeCompare(b.state));
}

/**
 * Filter `pending` to rows whose ocr_state equals `state`. A null/empty
 * state (no facet) passes every row through unchanged. The returned array
 * is always a new array (never the input reference). Null list -> [].
 */
export function filterByPendingState<T extends OcrDocLike>(
  pending: readonly T[],
  state: string | null,
): T[] {
  if (!Array.isArray(pending)) return [];
  if (!state) return pending.slice();
  return pending.filter((d) => d && (d.ocr_state || "unknown") === state);
}

/** A pending-state bucket annotated with its share-of-total for a mini bar. */
export interface OcrStateShare extends OcrStateBucket {
  /** This bucket's count over the TOTAL pending, 0..1. */
  fraction: number;
  /** This bucket's count over the LARGEST bucket, 0..1 (bar width). */
  scaled: number;
  /** Rounded whole-percent of total, for the SR label. */
  percent: number;
}

/**
 * Annotate pending-state buckets with proportional shares so the kind pills
 * (Image-only / Mixed pages) render the same mini bar the failure-reason
 * pills already carry — turning equal-looking pills into a histogram where
 * the dominant kind reads biggest at a glance. `fraction` is share of total;
 * `scaled` is share of the largest bucket. Stable order (reuses
 * groupPendingStates sorting). Empty/garbage -> [].
 */
export function stateShareBars(buckets: readonly OcrStateBucket[]): OcrStateShare[] {
  if (!Array.isArray(buckets) || buckets.length === 0) return [];
  const total = buckets.reduce((s, b) => s + (b?.count > 0 ? b.count : 0), 0);
  const max = buckets.reduce((m, b) => Math.max(m, b?.count > 0 ? b.count : 0), 0);
  if (total <= 0 || max <= 0) return [];
  return buckets
    .filter((b) => b && b.count > 0)
    .map((b) => ({
      state: b.state,
      count: b.count,
      fraction: b.count / total,
      scaled: b.count / max,
      percent: Math.round((b.count / total) * 100),
    }));
}

/**
 * Reconcile an active pending-state facet against the live bucket list.
 * If the faceted state no longer exists (its last pending doc was run, or
 * a refresh dropped it), the facet is stale and must clear so the pending
 * list doesn't silently show zero rows with no way back. Returns the
 * state to keep, or null to drop the facet. Pure.
 */
export function reconcilePendingStateFacet(
  active: string | null,
  buckets: readonly OcrStateBucket[],
): string | null {
  if (!active) return null;
  if (!Array.isArray(buckets)) return null;
  return buckets.some((b) => b && b.state === active) ? active : null;
}

// --- Slice 4: keyboard navigation --------------------------------------
//
// The panel was mouse-only: you could search, sort, and triage, but
// reaching a row, running/retrying it, or opening it all needed the
// pointer. This adds Raycast-grade keyboard control with ONE virtual
// cursor spanning BOTH lists (failures first, then pending) so a single
// arrow walk crosses the whole queue. Enter activates the focused row
// (the component runs a pending doc or retries a failed one by section);
// "o" opens it; Escape parks the cursor. The arrow/Home/End/paging math
// REUSES the tested palette nav core rather than rolling a second
// implementation, exactly as the Beacon inspector does.

/** Which list a flattened row belongs to. */
export type OcrSection = "failed" | "pending";

/** One row in the unified cursor index space, tagged with its section. */
export interface OcrFlatRow<T extends OcrDocLike = OcrDocLike> {
  section: OcrSection;
  record: T;
}

/**
 * Flatten the (already filtered + sorted) failure and pending lists into
 * one index space — failures first, then pending — so a single cursor
 * walks the whole queue in render order. Each entry is tagged with its
 * section so the caller knows whether Enter should retry or run. Null
 * lists are treated as empty.
 */
export function flattenOcrRows<T extends OcrDocLike>(
  failed: readonly T[],
  pending: readonly T[],
): OcrFlatRow<T>[] {
  const out: OcrFlatRow<T>[] = [];
  if (Array.isArray(failed)) {
    for (const record of failed) if (record) out.push({ section: "failed", record });
  }
  if (Array.isArray(pending)) {
    for (const record of pending) if (record) out.push({ section: "pending", record });
  }
  return out;
}

/** What a keypress over the queue should do. */
export type OcrTableAction =
  | { kind: "move"; intent: PaletteNavIntent }
  | { kind: "activate" }
  | { kind: "open" }
  | { kind: "clear" }
  | null;

/** Minimal keyboard-event shape the table classifier reads. */
export interface OcrKeyEvent {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

/**
 * Classify a keypress over the queue into an action, or null if it isn't
 * a queue key (so typing in the search box falls through). Any modifier
 * (Cmd/Ctrl/Alt) disqualifies the key so app/OS chords keep priority —
 * the queue owns only bare presses. Navigation keys defer to the tested
 * palette classifier so wrap/paging behaviour is identical everywhere.
 * Enter -> "activate" (run-or-retry, the component decides by section);
 * "o"/"O" -> "open"; Escape -> "clear".
 */
export function classifyOcrTableKey(ev: OcrKeyEvent): OcrTableAction {
  if (!ev) return null;
  if (ev.ctrlKey || ev.metaKey || ev.altKey) return null;

  const nav = classifyPaletteNav({ key: ev.key });
  if (nav) return { kind: "move", intent: nav };

  switch (ev.key) {
    case "Enter":
      return { kind: "activate" };
    case "o":
    case "O":
      return { kind: "open" };
    case "Escape":
      return { kind: "clear" };
    default:
      return null;
  }
}

/**
 * Resolve the next cursor index for a move over `count` rows. Thin
 * adapter over the tested `nextPaletteIndex` so the queue and palette
 * share one wrap/clamp/paging contract. Empty list -> 0.
 */
export function nextOcrCursor(
  intent: PaletteNavIntent,
  current: number,
  count: number,
): number {
  return nextPaletteIndex(intent, current, count);
}

/**
 * Clamp a stored cursor index into a freshly-(re)filtered queue. After a
 * search/sort/facet change the row count shrinks or the order moves, so a
 * cursor parked at index 40 must snap back into range. Returns 0 for an
 * empty list, never a negative or out-of-bounds index.
 */
export function clampOcrCursor(current: number, count: number): number {
  if (!Number.isFinite(count) || count <= 0) return 0;
  if (!Number.isFinite(current) || current < 0) return 0;
  const last = count - 1;
  return Math.min(last, Math.floor(current));
}

// --- Slice 5: queue impact + context-aware footer ----------------------
//
// "Run all" kicks off OCR on every pending doc — real CPU time — but the
// button said only "Run all (N)" with no sense of the true workload. This
// sums the pending queue's footprint (docs + pages) so the cost is
// visible BEFORE the click, and adds a context-aware footer narrating the
// live view (what's shown across both buckets, what's filtering, the
// in-flight count) the same way the command palette and Beacon footers do.

/** Aggregate footprint of a set of OCR-queue documents. */
export interface OcrImpact {
  docs: number;
  pages: number;
}

/**
 * Sum the footprint (docs + pages) of a pending list — the real workload
 * "Run all" would process. Rows with a null/garbage page count contribute
 * to the doc total but zero pages. A null list -> all zeros.
 */
export function summarizePending<T extends OcrDocLike>(pending: readonly T[]): OcrImpact {
  const zero: OcrImpact = { docs: 0, pages: 0 };
  if (!Array.isArray(pending)) return zero;
  let docs = 0;
  let pages = 0;
  for (const d of pending) {
    if (!d) continue;
    docs++;
    const p = d.pages;
    if (typeof p === "number" && Number.isFinite(p) && p > 0) pages += p;
  }
  return { docs, pages };
}

/**
 * Compose a one-line "N docs · N pages" impact string, pluralized +
 * thousands-grouped. A zero page count drops out so a queue of un-probed
 * docs reads "12 docs", and an empty impact reads "Queue empty". Pure
 * (locale grouping via toLocaleString).
 */
export function describeOcrImpact(impact: OcrImpact): string {
  if (!impact || impact.docs <= 0) return "Queue empty";
  const parts: string[] = [`${impact.docs.toLocaleString()} doc${impact.docs === 1 ? "" : "s"}`];
  if (impact.pages > 0) {
    parts.push(`${impact.pages.toLocaleString()} page${impact.pages === 1 ? "" : "s"}`);
  }
  return parts.join(" \u00b7 ");
}

// --- Slice 5b: determinate Run-all progress ----------------------------
//
// "Run all" was fire-and-refresh: the button froze on "Running 47…" for
// however many minutes the batch took, with no sense of how far along it
// was. Running the pending docs one at a time (each its own ocrQueueRunOne)
// lets the UI tick a REAL progress bar after every doc. This pure helper
// turns the running tallies (docs done, pages done) against the known
// workload into a determinate progress model — fraction for the bar, a
// human label ("12 / 47 docs · 3,400 pages") for the overlay — so the
// component just renders it.

/** A determinate progress snapshot for the in-flight Run-all batch. */
export interface RunAllProgress {
  /** Docs finished so far. */
  done: number;
  /** Total docs in the batch. */
  total: number;
  /** Pages OCR'd so far. */
  pagesDone: number;
  /** Total pages across the batch. */
  pagesTotal: number;
  /** Completion fraction in [0, 1] (by docs; 0 when total is 0). */
  fraction: number;
  /** Rounded percent in [0, 100]. */
  percent: number;
  /** Overlay label, e.g. "12 / 47 docs · 3,400 pages". */
  label: string;
  /** True once every doc is processed. */
  finished: boolean;
}

/**
 * Build the determinate progress model for a Run-all batch from the
 * running tallies. `done` / `pagesDone` accumulate as each doc finishes;
 * `total` / `pagesTotal` are the workload measured up front (via
 * `summarizePending`). Counts are clamped into range so a stray
 * over-count can't push the bar past 100%. The label always names the
 * doc progress and appends the page progress when a page total is known.
 * Pure; tolerant of garbage numbers.
 */
export function describeRunAllProgress(
  done: number,
  total: number,
  pagesDone: number,
  pagesTotal: number,
): RunAllProgress {
  const t = Math.max(0, Math.floor(Number.isFinite(total) ? total : 0));
  const d = Math.max(0, Math.min(t, Math.floor(Number.isFinite(done) ? done : 0)));
  const pt = Math.max(0, Math.floor(Number.isFinite(pagesTotal) ? pagesTotal : 0));
  const pd = Math.max(0, Math.min(pt, Math.floor(Number.isFinite(pagesDone) ? pagesDone : 0)));
  const fraction = t > 0 ? d / t : 0;
  const percent = Math.round(fraction * 100);
  let label = `${d.toLocaleString()} / ${t.toLocaleString()} doc${t === 1 ? "" : "s"}`;
  if (pt > 0) label += ` \u00b7 ${pd.toLocaleString()} / ${pt.toLocaleString()} pages`;
  return { done: d, total: t, pagesDone: pd, pagesTotal: pt, fraction, percent, label, finished: t > 0 && d >= t };
}

// --- Slice 5c: cancel an in-flight Run-all -----------------------------
//
// The per-doc Run-all loop ticks a determinate bar, but once started it
// ran to the end with no way out — a 5,000-doc batch kicked off by
// mistake meant force-quitting the app. The component now flips a cancel
// flag the loop checks before each doc; this pure helper turns the final
// tallies (succeeded / failed / whether it was canceled) into the
// completion toast so a canceled batch reads honestly ("… canceled
// (13 of 47)") instead of masquerading as a clean finish.

/** The terminal summary of a Run-all batch (drives the completion toast). */
export interface RunAllOutcome {
  /** Docs that OCR'd cleanly. */
  ok: number;
  /** Docs that failed OCR. */
  fail: number;
  /** Docs actually processed (ok + fail, clamped to total). */
  done: number;
  /** Total docs the batch set out to run. */
  total: number;
  /** True when the user canceled the run. */
  canceled: boolean;
  /** True when canceling stopped the loop before every doc ran. */
  partial: boolean;
  /** Toast line, e.g. "OCR queue: 12 succeeded, 1 failed — canceled (13 of 47)". */
  label: string;
}

/**
 * Build the completion summary for a Run-all batch from the running
 * tallies. `canceled` is the component's cancel flag at loop exit; a run
 * that was canceled BEFORE every doc finished is `partial` and its label
 * names how far it got ("… canceled (13 of 47)"), while a run that
 * completed (or was "canceled" on the very last doc) reads as a normal
 * finish ("… (of 47)"). Counts are clamped so a stray tally can't exceed
 * the total. Pure; tolerant of garbage numbers.
 */
export function describeRunAllOutcome(
  ok: number,
  fail: number,
  total: number,
  canceled: boolean,
): RunAllOutcome {
  const t = Math.max(0, Math.floor(Number.isFinite(total) ? total : 0));
  const o = Math.max(0, Math.floor(Number.isFinite(ok) ? ok : 0));
  const f = Math.max(0, Math.floor(Number.isFinite(fail) ? fail : 0));
  const done = Math.min(t, o + f);
  const canceledFlag = !!canceled;
  const partial = canceledFlag && done < t;
  let label = `OCR queue: ${o.toLocaleString()} succeeded, ${f.toLocaleString()} failed`;
  if (partial) {
    label += ` \u2014 canceled (${done.toLocaleString()} of ${t.toLocaleString()})`;
  } else {
    label += ` (of ${t.toLocaleString()})`;
  }
  return { ok: o, fail: f, done, total: t, canceled: canceledFlag, partial, label };
}

// --- Slice 5d: resume a canceled Run-all ("Run remaining") -------------
//
// Canceling a Run-all stops the loop between docs, leaving the un-run
// tail of the snapshotted batch still pending. Before this the only way
// back was a full "Run all" again — which re-measures the LIVE pending
// list (now possibly seeded with docs scanned since) and re-runs from the
// top. This pure helper carves out exactly the remaining tail of the
// ORIGINAL snapshot (the docs the canceled run never reached) plus its
// workload, so the component can offer a one-click "Run remaining (N)"
// that resumes precisely where the cancel left off.

/** The un-run tail of a canceled batch + its measured workload. */
export interface RunRemainingPlan<T> {
  /** The docs the canceled run never reached (the snapshot's tail). */
  remaining: T[];
  /** Footprint (docs + pages) of `remaining`, via summarizePending. */
  impact: OcrImpact;
}

/**
 * Carve the un-run tail out of a canceled Run-all snapshot. `batch` is
 * the docs the run set out to process (captured up front); `alreadyDone`
 * is how many the loop finished before the cancel broke it. Returns the
 * slice from `alreadyDone` to the end plus its measured workload. The
 * cut point is clamped into [0, batch.length] so a stray over/under count
 * can't slice out of range; a fully-finished batch (alreadyDone >= length)
 * yields an empty plan. A null/garbage batch -> empty plan. Pure.
 */
export function planRunRemaining<T extends OcrDocLike>(
  batch: readonly T[],
  alreadyDone: number,
): RunRemainingPlan<T> {
  const list = Array.isArray(batch) ? batch.filter(Boolean) : [];
  const done = Math.max(0, Math.min(list.length, Math.floor(Number.isFinite(alreadyDone) ? alreadyDone : 0)));
  const remaining = list.slice(done);
  return { remaining, impact: summarizePending(remaining) };
}

/**
 * Compose the "Run remaining" affordance label from the un-run plan's
 * impact, e.g. "Run remaining 34" or "Run remaining 34 · 1,200 pages".
 * Returns "" when nothing remains (so the component hides the button).
 * Pure (locale grouping via toLocaleString).
 */
export function describeRunRemaining(impact: OcrImpact): string {
  const docs = Math.max(0, Math.floor(impact?.docs ?? 0));
  if (docs <= 0) return "";
  const pages = Math.max(0, Math.floor(impact?.pages ?? 0));
  let label = `Run remaining ${docs.toLocaleString()}`;
  if (pages > 0) label += ` \u00b7 ${pages.toLocaleString()} page${pages === 1 ? "" : "s"}`;
  return label;
}

// --- Slice 5e: resume a canceled Requeue-all ("Retry remaining") -------
//
// The failure-inbox "Re-queue all failed" loop is the twin of Run-all but
// shipped all-or-nothing: it fired a single blanket backend call with no
// progress, no cancel, and no resume. This round gives it the same per-doc
// loop the Run-all has — so a 500-failure requeue can be canceled and then
// resumed from exactly where it stopped. The CARVE reuses the generic
// `planRunRemaining` (it slices any OcrDocLike tail + measures it); only
// the human label differs ("Retry remaining N" vs "Run remaining N"), so
// there is no second carve engine to drift.

/**
 * Compose the "Retry remaining" resume affordance label from a canceled
 * Requeue-all's un-run plan impact, e.g. "Retry remaining 34" or
 * "Retry remaining 34 \u00b7 1,200 pages". Returns "" when nothing remains
 * (so the component hides the button). The twin of `describeRunRemaining`
 * with the requeue verb. Pure (locale grouping via toLocaleString).
 */
export function describeRequeueRemaining(impact: OcrImpact): string {
  const docs = Math.max(0, Math.floor(impact?.docs ?? 0));
  if (docs <= 0) return "";
  const pages = Math.max(0, Math.floor(impact?.pages ?? 0));
  let label = `Retry remaining ${docs.toLocaleString()}`;
  if (pages > 0) label += ` \u00b7 ${pages.toLocaleString()} page${pages === 1 ? "" : "s"}`;
  return label;
}

/**
 * Build the completion summary for a Requeue-all batch — the requeue twin
 * of `describeRunAllOutcome`. A requeue either re-queues a doc (success)
 * or throws (fail); a run canceled before every doc finished is `partial`
 * and names how far it got ("\u2026 canceled (13 of 47)"), while a finished
 * run reads "Re-queued N of M". Counts are clamped so a stray tally can't
 * exceed the total. Pure; tolerant of garbage numbers.
 */
export function describeRequeueAllOutcome(
  ok: number,
  fail: number,
  total: number,
  canceled: boolean,
): RunAllOutcome {
  const t = Math.max(0, Math.floor(Number.isFinite(total) ? total : 0));
  const o = Math.max(0, Math.floor(Number.isFinite(ok) ? ok : 0));
  const f = Math.max(0, Math.floor(Number.isFinite(fail) ? fail : 0));
  const done = Math.min(t, o + f);
  const canceledFlag = !!canceled;
  const partial = canceledFlag && done < t;
  let label: string;
  if (f > 0) {
    label = `OCR queue: re-queued ${o.toLocaleString()}, ${f.toLocaleString()} failed`;
  } else {
    label = `OCR queue: re-queued ${o.toLocaleString()}`;
  }
  if (partial) {
    label += ` \u2014 canceled (${done.toLocaleString()} of ${t.toLocaleString()})`;
  } else {
    label += ` (of ${t.toLocaleString()})`;
  }
  return { ok: o, fail: f, done, total: t, canceled: canceledFlag, partial, label };
}

/** The live state the footer narrates. */
export interface OcrViewState {
  /** Failure rows currently shown (after reason facet + search). */
  shownFailed: number;
  /** Pending rows currently shown (after search). */
  shownPending: number;
  /** Total failures in the inbox. */
  totalFailed: number;
  /** Total pending docs in the queue. */
  totalPending: number;
  /** Docs currently being OCR'd (in flight). */
  inFlight: number;
  /** Active reason facet, or null. */
  reasonFacet: string | null;
  /** Trimmed search query (""=none). */
  query: string;
}

/**
 * Narrate the current view for the footer: how many rows are shown vs
 * total across both buckets, which filters are narrowing, and the
 * in-flight count. Mirrors the Beacon inspector's `describeBeaconView`
 * "context-aware footer" style. Pure; returns a single human line.
 */
export function describeOcrView(state: OcrViewState): string {
  const totalFailed = Math.max(0, state?.totalFailed ?? 0);
  const totalPending = Math.max(0, state?.totalPending ?? 0);
  const total = totalFailed + totalPending;
  const shownFailed = Math.max(0, Math.min(totalFailed, state?.shownFailed ?? 0));
  const shownPending = Math.max(0, Math.min(totalPending, state?.shownPending ?? 0));
  const shown = shownFailed + shownPending;
  const inFlight = Math.max(0, state?.inFlight ?? 0);
  const facet = state?.reasonFacet ?? null;
  const query = (state?.query ?? "").trim();

  if (total === 0) {
    return inFlight > 0 ? `${inFlight.toLocaleString()} in flight` : "Queue empty";
  }

  const filtering = !!facet || query.length > 0;
  let base: string;
  if (filtering) {
    base = `${shown.toLocaleString()} of ${total.toLocaleString()}`;
    const narrows: string[] = [];
    if (facet) narrows.push(facet);
    if (query) narrows.push(`\u201c${query}\u201d`);
    base += ` matching ${narrows.join(" + ")}`;
  } else {
    const segs: string[] = [];
    if (totalFailed > 0) {
      segs.push(`${totalFailed.toLocaleString()} failed`);
    }
    if (totalPending > 0) {
      segs.push(`${totalPending.toLocaleString()} pending`);
    }
    base = segs.join(" \u00b7 ");
  }
  if (inFlight > 0) base += ` \u00b7 ${inFlight.toLocaleString()} in flight`;
  return base;
}

// --- Slice 1 (round 51): name the in-flight doc under the bulk bar ----
//
// Both the Run-all and the Requeue-all loops tick a determinate bar
// ("12 / 47 docs"), but the bar is ANONYMOUS — it never says WHICH doc
// is being worked right now. On a long batch that lands on one slow,
// scanned PDF the bar can sit visibly stalled for many seconds with no
// hint of what it's chewing on, reading as a hang. The per-doc loops
// already hold the current doc each iteration; this pure helper turns
// that doc's path + the action into a live "Running <name>…" /
// "Re-queuing <name>…" line the component renders under the bar. Reuses
// `ocrBasename` so the name shown matches every other row in the panel.

/** Which bulk action the in-flight line narrates (verb differs only). */
export type OcrBulkAction = "run" | "requeue";

/**
 * Compose the live "current document" line shown under a determinate
 * Run-all / Requeue-all bar, e.g. "Running invoice.pdf\u2026" or
 * "Re-queuing scan 2.pdf\u2026". `action` picks the verb ("run" -> OCR is
 * processing the doc; "requeue" -> the failure inbox is flipping it back
 * to scanned). The display name is the path's basename (via ocrBasename)
 * so it matches the row labels exactly. Returns "" when no doc is in
 * flight (a null/blank path, or a path that is only separators) so the
 * component hides the line cleanly between docs. Pure + DOM-free.
 */
export function describeInFlightDoc(
  path: string | null | undefined,
  action: OcrBulkAction,
): string {
  const name = typeof path === "string" ? ocrBasename(path) : "";
  if (!name) return "";
  const verb = action === "requeue" ? "Re-queuing" : "Running";
  return `${verb} ${name}\u2026`;
}
