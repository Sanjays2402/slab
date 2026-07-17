// Reader outline (TOC) view-core — round 59.
//
// Pure, DOM-free model for filter-as-you-type over the Reader panel's
// bookmark/outline tree. A long PDF (a textbook, a contract, an RFC) can
// carry a hundred-plus nested outline entries; the rail shipped as a plain
// expand/collapse tree with no way to jump to the one heading you remember
// the words of. This adds a palette-grade filter: type a few characters and
// the tree collapses to just the branches that match (a node is kept if it
// matches OR any descendant matches), with the matched span highlighted and
// every ancestor force-expanded so the hit is visible.
//
// Same pure-core/thin-shell discipline as librarySearchView.ts /
// readerThumbView.ts / paletteSearch.ts: every decision (does this subtree
// survive the filter, where are the highlight ranges, how many matched) is a
// pure function unit-tested without a DOM; ReaderPanel.svelte owns the
// imperative edges (the recursive {#snippet}, jumpToOutline, the input).
//
// Reuses the tested palette core (scorePaletteField for ranking the match,
// splitHighlight for the <mark> segments) so there is NO second fuzzy engine
// to drift from Cmd+K / library search / the find bar.

import {
  scorePaletteField,
  splitHighlight,
  type HighlightSegment,
} from "./paletteSearch";

/**
 * The shape this core needs from a Reader outline node. Structural so the
 * component's richer OutlineNode (carrying its pdf.js `dest`, `expanded`,
 * etc.) satisfies it without conversion. `items` is the nested children.
 */
export interface OutlineNodeLike {
  readonly title: string;
  readonly items: readonly OutlineNodeLike[];
}

/** A node after filtering: the surviving title + its filtered children. */
export interface FilteredOutlineNode<T extends OutlineNodeLike> {
  /** The original node (so the caller keeps its `dest` / identity). */
  readonly node: T;
  /** Whether THIS node's own title matched (vs. surviving via a child). */
  readonly selfMatch: boolean;
  /** Highlight segments for the title under the active query. */
  readonly segments: HighlightSegment[];
  /** Filtered + force-expanded children. */
  readonly items: FilteredOutlineNode<T>[];
}

/** Normalize a filter query: collapse whitespace, trim. Pure. */
export function normalizeOutlineQuery(query: string | null | undefined): string {
  if (typeof query !== "string") return "";
  return query.replace(/\s+/g, " ").trim();
}

/**
 * Whether a node's own title matches the (already-normalized) query. Empty
 * query -> false here (the caller short-circuits the whole filter on empty,
 * showing the unfiltered tree); a non-empty query uses the palette scorer so
 * prefix / substring / subsequence all match exactly like the rest of the app.
 */
export function outlineTitleMatches(title: string, normQuery: string): boolean {
  if (!normQuery) return false;
  if (typeof title !== "string" || title.length === 0) return false;
  return scorePaletteField(normQuery, title).score > 0;
}

/**
 * Highlight segments for an outline title under a query. Empty query -> a
 * single non-hit segment covering the whole title (so the caller can render
 * the same way filtered or not). A non-matching title under a live query
 * also yields one non-hit segment (it only survives via a child). Reuses the
 * palette scorer's ranges + splitHighlight so the <mark> spans match Cmd+K.
 */
export function highlightOutlineTitle(title: string, normQuery: string): HighlightSegment[] {
  const text = typeof title === "string" ? title : "";
  if (!normQuery || text.length === 0) {
    return text ? [{ text, hit: false }] : [];
  }
  const { ranges } = scorePaletteField(normQuery, text);
  if (!ranges.length) return [{ text, hit: false }];
  return splitHighlight(text, ranges);
}

/**
 * Filter one node against a normalized query, returning a FilteredOutlineNode
 * if the node OR any descendant matches, else null (the whole subtree is
 * pruned). Children are filtered recursively; a surviving node force-expands
 * (the caller renders `items` unconditionally) so every hit is visible
 * without manual expansion. A node survives when its own title matches even
 * if no child does (its children are then shown filtered, which may be empty).
 */
export function filterOutlineNode<T extends OutlineNodeLike>(
  node: T,
  normQuery: string,
): FilteredOutlineNode<T> | null {
  if (!node) return null;
  const selfMatch = outlineTitleMatches(node.title, normQuery);
  const kids = Array.isArray(node.items) ? node.items : [];
  const filteredKids: FilteredOutlineNode<T>[] = [];
  for (const k of kids) {
    const fk = filterOutlineNode(k as T, normQuery);
    if (fk) filteredKids.push(fk);
  }
  if (!selfMatch && filteredKids.length === 0) return null;
  return {
    node,
    selfMatch,
    segments: highlightOutlineTitle(node.title, normQuery),
    items: filteredKids,
  };
}

/**
 * Filter a whole outline tree against a query. An empty/blank query returns
 * null (a sentinel meaning "no filter active — render the original tree"),
 * keeping the caller's normal expand/collapse semantics. A live query returns
 * the pruned, highlighted, force-expanded forest. Pure; never mutates input.
 */
export function filterOutlineTree<T extends OutlineNodeLike>(
  tree: readonly T[],
  query: string | null | undefined,
): FilteredOutlineNode<T>[] | null {
  const norm = normalizeOutlineQuery(query);
  if (!norm) return null; // no filter active
  if (!Array.isArray(tree)) return [];
  const out: FilteredOutlineNode<T>[] = [];
  for (const n of tree) {
    const fn = filterOutlineNode(n, norm);
    if (fn) out.push(fn);
  }
  return out;
}

/**
 * Count nodes whose OWN title matched (not counting ancestors kept only to
 * reveal a descendant) across a filtered forest — drives the "N matches"
 * summary. A null forest (no filter) -> 0. Pure recursion.
 */
export function countOutlineMatches<T extends OutlineNodeLike>(
  forest: readonly FilteredOutlineNode<T>[] | null,
): number {
  if (!Array.isArray(forest)) return 0;
  let n = 0;
  for (const f of forest) {
    if (f.selfMatch) n++;
    n += countOutlineMatches(f.items);
  }
  return n;
}

/**
 * Summary copy for the outline filter footer. Null forest -> "" (no filter,
 * render nothing). A live filter -> "N matches" / "1 match" / "No matches"
 * (so the empty result reads clearly rather than as a blank rail). Pure.
 */
export function describeOutlineFilter<T extends OutlineNodeLike>(
  forest: readonly FilteredOutlineNode<T>[] | null,
): string {
  if (!Array.isArray(forest)) return "";
  const n = countOutlineMatches(forest);
  if (n === 0) return "No matches";
  return `${n} match${n === 1 ? "" : "es"}`;
}

/**
 * Total number of nodes in an unfiltered outline tree (every node at every
 * depth) — drives the placeholder "Filter N headings…" affordance so the
 * user knows the filter is worth using on a big TOC. Pure recursion.
 */
export function countOutlineNodes<T extends OutlineNodeLike>(
  tree: readonly T[] | null | undefined,
): number {
  if (!Array.isArray(tree)) return 0;
  let n = 0;
  for (const node of tree) {
    if (!node) continue;
    n++;
    n += countOutlineNodes(node.items as T[]);
  }
  return n;
}
