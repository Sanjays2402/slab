// Pure-helper tests for the Reader outline (TOC) filter view-core.
//
// Style matches readerThumbView.test.ts — no test runner, an inline
// `expect`. Run with:  tsx src/lib/readerOutlineView.test.ts

import {
  normalizeOutlineQuery,
  outlineTitleMatches,
  highlightOutlineTitle,
  filterOutlineNode,
  filterOutlineTree,
  countOutlineMatches,
  describeOutlineFilter,
  countOutlineNodes,
  type OutlineNodeLike,
} from "./readerOutlineView";

let passed = 0;
let failed = 0;
function expect(cond: boolean, label: string) {
  if (cond) {
    passed++;
    // eslint-disable-next-line no-console
    console.log(`ok: ${label}`);
  } else {
    failed++;
    // eslint-disable-next-line no-console
    console.error(`FAIL: ${label}`);
  }
}

// A small sample tree: Chapter 1 > {Intro, Methods}, Chapter 2 > {Results}.
type Node = OutlineNodeLike & { title: string; items: Node[]; tag: string };
function n(title: string, tag: string, items: Node[] = []): Node {
  return { title, tag, items };
}
const tree: Node[] = [
  n("Chapter 1: Foundations", "c1", [
    n("Introduction", "c1-intro"),
    n("Methods and Materials", "c1-methods"),
  ]),
  n("Chapter 2: Results", "c2", [n("Statistical Analysis", "c2-stats")]),
  n("Appendix", "appx"),
];

// --- normalizeOutlineQuery -------------------------------------------
{
  expect(normalizeOutlineQuery("  hi   there ") === "hi there", "normalize: collapse + trim");
  expect(normalizeOutlineQuery(null) === "", "normalize: null -> empty");
  expect(normalizeOutlineQuery(undefined) === "", "normalize: undefined -> empty");
  expect(normalizeOutlineQuery(42 as unknown as string) === "", "normalize: non-string -> empty");
}

// --- outlineTitleMatches ---------------------------------------------
{
  expect(outlineTitleMatches("Introduction", "intro") === true, "match: prefix");
  expect(outlineTitleMatches("Methods and Materials", "materials") === true, "match: substring");
  expect(outlineTitleMatches("Statistical Analysis", "stanal") === true, "match: subsequence");
  expect(outlineTitleMatches("Introduction", "xyz") === false, "match: miss");
  expect(outlineTitleMatches("Introduction", "") === false, "match: empty query -> false");
  expect(outlineTitleMatches("", "intro") === false, "match: empty title -> false");
}

// --- highlightOutlineTitle -------------------------------------------
{
  // empty query -> one non-hit segment covering the whole title
  const segNoQ = highlightOutlineTitle("Introduction", "");
  expect(segNoQ.length === 1 && !segNoQ[0].hit && segNoQ[0].text === "Introduction", "hl: empty query -> whole non-hit");
  // empty title -> no segments
  expect(highlightOutlineTitle("", "intro").length === 0, "hl: empty title -> []");
  // prefix match marks the matched span
  const seg = highlightOutlineTitle("Introduction", "intro");
  const hit = seg.find((s) => s.hit);
  expect(!!hit && hit.text.toLowerCase() === "intro", "hl: marks the matched prefix");
  // reconstructs the full title across segments
  expect(seg.map((s) => s.text).join("") === "Introduction", "hl: segments reconstruct title");
  // non-matching title under a live query -> single non-hit segment
  const segMiss = highlightOutlineTitle("Appendix", "zzz");
  expect(segMiss.length === 1 && !segMiss[0].hit, "hl: non-match -> single non-hit");
}

// --- filterOutlineNode -----------------------------------------------
{
  // a leaf node that matches survives with selfMatch=true
  const leaf = filterOutlineNode(n("Introduction", "c1-intro"), "intro");
  expect(leaf !== null && leaf.selfMatch === true, "node: matching leaf survives selfMatch");
  // a leaf that misses is pruned
  expect(filterOutlineNode(n("Introduction", "x"), "zzz") === null, "node: missing leaf pruned");
  // a parent that misses but has a matching child survives via the child
  const parent = filterOutlineNode(tree[0], "materials");
  expect(parent !== null, "node: parent kept to reveal matching child");
  expect(parent !== null && parent.selfMatch === false, "node: parent kept-but-not-self-matched");
  expect(parent !== null && parent.items.length === 1, "node: only the matching child survives");
  expect(parent !== null && parent.items[0].node.tag === "c1-methods", "node: surviving child is Methods");
  // a parent whose OWN title matches survives even if no child does; its
  // (non-matching) children are pruned out
  const ch1 = filterOutlineNode(tree[0], "foundations");
  expect(ch1 !== null && ch1.selfMatch === true, "node: parent self-match survives");
  expect(ch1 !== null && ch1.items.length === 0, "node: self-matched parent's non-matching kids pruned");
  // null node -> null, never throws
  expect(filterOutlineNode(null as unknown as Node, "x") === null, "node: null -> null");
}

// --- filterOutlineTree -----------------------------------------------
{
  // empty / blank query -> null sentinel (no filter active)
  expect(filterOutlineTree(tree, "") === null, "tree: empty query -> null sentinel");
  expect(filterOutlineTree(tree, "   ") === null, "tree: blank query -> null sentinel");
  // "chapter" matches both chapter parents (selfMatch), prunes Appendix
  const ch = filterOutlineTree(tree, "chapter");
  expect(ch !== null && ch.length === 2, "tree: 'chapter' keeps both chapters, drops appendix");
  // "analysis" survives only via Chapter 2 > Statistical Analysis
  const an = filterOutlineTree(tree, "analysis");
  expect(an !== null && an.length === 1 && an[0].node.tag === "c2", "tree: deep hit keeps only its branch");
  expect(an !== null && an[0].items.length === 1 && an[0].items[0].node.tag === "c2-stats", "tree: reveals the deep matching node");
  // a total miss -> empty forest (not null), so caller shows "No matches"
  const none = filterOutlineTree(tree, "zzzznope");
  expect(Array.isArray(none) && none!.length === 0, "tree: total miss -> empty forest");
  // non-array tree under a live query -> empty forest, never throws
  expect(JSON.stringify(filterOutlineTree(null as unknown as Node[], "x")) === "[]", "tree: null tree -> []");
}

// --- countOutlineMatches ---------------------------------------------
{
  expect(countOutlineMatches(null) === 0, "count: null forest -> 0");
  // "chapter" -> 2 self-matches (the two chapter titles)
  expect(countOutlineMatches(filterOutlineTree(tree, "chapter")) === 2, "count: 2 chapter self-matches");
  // "analysis" -> 1 self-match (only the deep node, not the ancestor)
  expect(countOutlineMatches(filterOutlineTree(tree, "analysis")) === 1, "count: ancestor not counted");
  // "intro" -> 1 (the Introduction leaf)
  expect(countOutlineMatches(filterOutlineTree(tree, "intro")) === 1, "count: single leaf");
}

// --- describeOutlineFilter -------------------------------------------
{
  expect(describeOutlineFilter(null) === "", "describe: null -> empty (no filter)");
  expect(describeOutlineFilter(filterOutlineTree(tree, "chapter")) === "2 matches", "describe: plural");
  expect(describeOutlineFilter(filterOutlineTree(tree, "intro")) === "1 match", "describe: singular");
  expect(describeOutlineFilter(filterOutlineTree(tree, "zzz")) === "No matches", "describe: no matches");
}

// --- countOutlineNodes -----------------------------------------------
{
  // 3 top-level + 2 (ch1 kids) + 1 (ch2 kid) = 6 total
  expect(countOutlineNodes(tree) === 6, "nodes: counts every depth");
  expect(countOutlineNodes([]) === 0, "nodes: empty -> 0");
  expect(countOutlineNodes(null) === 0, "nodes: null -> 0");
  expect(countOutlineNodes(undefined) === 0, "nodes: undefined -> 0");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
