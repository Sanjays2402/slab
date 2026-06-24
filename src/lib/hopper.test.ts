// Pure-helper tests for the Hopper TS client.
//
// Style matches quill.test.ts / marketplace/fuzzy.test.ts — no test
// runner dep, just a tiny inline `expect` so the contract reads at a
// glance. The Tauri-side `invoke` is not exercised here; we only pin
// the pure helpers (percentages, diagnostics, summary).
//
// Run with:
//   node --import tsx src/lib/hopper.test.ts
// or copy assertions into the browser console after importing $lib/hopper.

import {
  fallthroughPercent,
  ruleMatchPercent,
  ruleCoverageDiagnostic,
  summarizeCoverage,
  summarizeCoverageHealth,
  ruleBucket,
  FALLTHROUGH_BUCKET,
  sampleBucketEquals,
  describeDrilldown,
  describeBucket,
  suggestDrilldownExportFilename,
  suggestCoverageExportFilename,
  filterCoverageByDiagnostic,
  ruleMatchesCoverageFilter,
  formatCoverageFilterSummary,
  coverageHealthClickTarget,
  planDeadRuleReorder,
  applyReorderProposal,
  formatReorderProposal,
  slabHopperPlanDeadRuleReorder,
  reorderProposalConfidence,
  filterProposalsByConfidence,
  describeReorderConfidence,
  applyReorderProposalsBatch,
  summarizeBatchReorderOutcome,
  describeSkipReason,
  slabHopperBatchReorderDeadRules,
  worstReorderConfidence,
  summarizeProposalTierBreakdown,
  describeProposalBatch,
  summarizeReorderEffect,
  describeReorderEffect,
  isReorderEffectNoop,
  type ProposalTierBreakdown,
  type ReorderEffect,
  type ReorderMove,
  RULE_NOT_FOUND,
  ALREADY_EARLIER,
  type ReorderProposalConfidence,
  type BatchReorderOutcome,
  type BatchReorderSkipReason,
  type SkippedProposal,
  COVERAGE_FILTER_KINDS,
  type CoverageDiagnosticFilter,
  type CoverageHealth,
  type ReorderProposal,
  type Rule,
  type RuleCoverageReport,
  type RuleCoverage,
  type RuleSample,
  type SampleBucket,
  type SampleDrilldown,
} from "./hopper";

function expect(cond: boolean, label: string): void {
  if (!cond) {
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    // eslint-disable-next-line no-console
    console.log("ok:", label);
  }
}

function rule(
  index: number,
  name: string,
  first_match: number,
  would_match: number,
  dead_at_position = false,
): RuleCoverage {
  return { index, name, first_match, would_match, dead_at_position };
}

function report(
  rules: RuleCoverage[],
  fallthrough: number,
  total_samples: number,
): RuleCoverageReport {
  return { rules, fallthrough, total_samples };
}

// ── fallthroughPercent ───────────────────────────────────────────────

{
  const r = report([rule(0, "A", 7, 7)], 3, 10);
  expect(fallthroughPercent(r) === 30, "fallthroughPercent 3/10 == 30");
}
{
  const r = report([rule(0, "A", 10, 10)], 0, 10);
  expect(fallthroughPercent(r) === 0, "fallthroughPercent 0/10 == 0");
}
{
  const r = report([], 5, 5);
  expect(fallthroughPercent(r) === 100, "fallthroughPercent 5/5 == 100");
}
{
  const r = report([], 0, 0);
  // Edge case: empty report. Without the guard the helper would NaN.
  expect(fallthroughPercent(r) === 0, "fallthroughPercent 0/0 == 0 (guard)");
}

// ── ruleMatchPercent ─────────────────────────────────────────────────

{
  const r = report([rule(0, "A", 7, 7), rule(1, "B", 0, 0)], 3, 10);
  expect(ruleMatchPercent(r.rules[0], r) === 70, "ruleMatchPercent A 7/10 == 70");
  expect(ruleMatchPercent(r.rules[1], r) === 0, "ruleMatchPercent B 0/10 == 0");
}
{
  // Empty report guard.
  const r = report([], 0, 0);
  expect(
    ruleMatchPercent(rule(0, "A", 0, 0), r) === 0,
    "ruleMatchPercent 0/0 == 0 (guard)",
  );
}

// ── ruleCoverageDiagnostic ───────────────────────────────────────────

{
  // Dead rule: first_match=0, would_match>0, dead_at_position=true.
  const r = rule(0, "Tax shadowed", 0, 4, true);
  expect(
    ruleCoverageDiagnostic(r) === "dead",
    "ruleCoverageDiagnostic: dead-at-position wins",
  );
}
{
  // Zero-coverage: first_match=0, would_match=0. Not flagged dead.
  const r = rule(0, "Never matches", 0, 0, false);
  expect(
    ruleCoverageDiagnostic(r) === "zero",
    "ruleCoverageDiagnostic: zero coverage",
  );
}
{
  // Partially shadowed: would_match strictly larger than first_match
  // (and dead_at_position false because first_match > 0).
  const r = rule(0, "Mixed", 3, 5, false);
  expect(
    ruleCoverageDiagnostic(r) === "shadowed",
    "ruleCoverageDiagnostic: shadowed when would > first",
  );
}
{
  // Healthy: would_match === first_match, no shadow, no zero, no dead.
  const r = rule(0, "Healthy", 5, 5, false);
  expect(
    ruleCoverageDiagnostic(r) === null,
    "ruleCoverageDiagnostic: null when healthy",
  );
}
{
  // Dead flag wins over any other signal — the diagnostic is the
  // most-actionable insight, not the most-narrow predicate.
  const r = rule(0, "Edge", 0, 1, true);
  expect(
    ruleCoverageDiagnostic(r) === "dead",
    "ruleCoverageDiagnostic: dead beats shadowed when flag set",
  );
}

// ── summarizeCoverage ────────────────────────────────────────────────

{
  const r = report([rule(0, "A", 7, 7)], 3, 10);
  expect(
    summarizeCoverage(r) === "7 of 10 samples routed (70%)",
    "summarizeCoverage: 7/10 routed copy",
  );
}
{
  const r = report([rule(0, "A", 5, 5), rule(1, "B", 5, 5)], 0, 10);
  expect(
    summarizeCoverage(r) === "10 of 10 samples routed (100%)",
    "summarizeCoverage: all routed copy",
  );
}
{
  const r = report([], 10, 10);
  expect(
    summarizeCoverage(r) === "0 of 10 samples routed (0%)",
    "summarizeCoverage: zero routed copy",
  );
}
{
  const r = report([], 0, 0);
  expect(
    summarizeCoverage(r) === "No recent runs to analyse",
    "summarizeCoverage: empty-state copy",
  );
}
{
  // Rounding: 1/3 routed -> 33% (Math.round).
  const r = report([rule(0, "A", 1, 1)], 2, 3);
  expect(
    summarizeCoverage(r) === "1 of 3 samples routed (33%)",
    "summarizeCoverage: rounds 33.33 -> 33",
  );
}

// ── ruleBucket / FALLTHROUGH_BUCKET (Slice 85) ───────────────────────

{
  // Valid index round-trips.
  const b = ruleBucket(3);
  expect(b.kind === "rule", "ruleBucket: kind is 'rule'");
  expect(b.kind === "rule" && b.index === 3, "ruleBucket: index preserved");
}
{
  // Index 0 is valid (first rule).
  const b = ruleBucket(0);
  expect(b.kind === "rule" && b.index === 0, "ruleBucket: index 0 is valid");
}
{
  // Negative index throws — TS bug guard, not a real bucket.
  let threw = false;
  try {
    ruleBucket(-1);
  } catch {
    threw = true;
  }
  expect(threw, "ruleBucket: negative index throws");
}
{
  // Non-integer throws — same guard.
  let threw = false;
  try {
    ruleBucket(1.5);
  } catch {
    threw = true;
  }
  expect(threw, "ruleBucket: float index throws");
}
{
  // The fall-through singleton is a stable object.
  expect(FALLTHROUGH_BUCKET.kind === "fallthrough", "FALLTHROUGH_BUCKET kind");
}

// ── sampleBucketEquals (Slice 85) ────────────────────────────────────

{
  expect(
    sampleBucketEquals(FALLTHROUGH_BUCKET, FALLTHROUGH_BUCKET),
    "sampleBucketEquals: fallthrough == fallthrough",
  );
  expect(
    sampleBucketEquals(FALLTHROUGH_BUCKET, { kind: "fallthrough" }),
    "sampleBucketEquals: fallthrough == new fallthrough",
  );
  expect(
    sampleBucketEquals(ruleBucket(2), ruleBucket(2)),
    "sampleBucketEquals: rule(2) == rule(2)",
  );
  expect(
    !sampleBucketEquals(ruleBucket(2), ruleBucket(3)),
    "sampleBucketEquals: rule(2) != rule(3)",
  );
  expect(
    !sampleBucketEquals(ruleBucket(0), FALLTHROUGH_BUCKET),
    "sampleBucketEquals: rule(0) != fallthrough",
  );
  expect(
    !sampleBucketEquals(FALLTHROUGH_BUCKET, ruleBucket(0)),
    "sampleBucketEquals: fallthrough != rule(0) (commutative)",
  );
}

// ── describeDrilldown (Slice 85) ─────────────────────────────────────

function sample(filename: string): RuleSample {
  return { filename };
}

function drilldown(
  bucket: SampleBucket,
  samples: RuleSample[],
  total_in_bucket: number,
  truncated: boolean,
): SampleDrilldown {
  return { bucket, samples, total_in_bucket, truncated };
}

{
  const d = drilldown(FALLTHROUGH_BUCKET, [], 0, false);
  expect(
    describeDrilldown(d) === "No files in this bucket",
    "describeDrilldown: empty bucket empty-state copy",
  );
}
{
  const d = drilldown(FALLTHROUGH_BUCKET, [sample("a.pdf")], 1, false);
  expect(
    describeDrilldown(d) === "1 file",
    "describeDrilldown: singular file copy",
  );
}
{
  const d = drilldown(
    FALLTHROUGH_BUCKET,
    [sample("a.pdf"), sample("b.pdf"), sample("c.pdf")],
    3,
    false,
  );
  expect(
    describeDrilldown(d) === "3 files",
    "describeDrilldown: plural files copy",
  );
}
{
  // Truncated: show count / total
  const samples = Array.from({ length: 25 }, (_, i) => sample(`f${i}.pdf`));
  const d = drilldown(ruleBucket(0), samples, 47, true);
  expect(
    describeDrilldown(d) === "Showing 25 of 47",
    "describeDrilldown: truncated copy",
  );
}
{
  // total > 0 but samples empty (defensive — shouldn't happen but
  // make sure we don't divide by zero or render NaN)
  const d = drilldown(ruleBucket(0), [], 5, true);
  expect(
    describeDrilldown(d) === "Showing 0 of 5",
    "describeDrilldown: defensive zero-shown branch",
  );
}

// ── describeBucket (Slice 85) ────────────────────────────────────────

{
  expect(
    describeBucket(FALLTHROUGH_BUCKET) === "Fall-through to watch defaults",
    "describeBucket: fallthrough copy",
  );
}
{
  // Named rule with names array.
  expect(
    describeBucket(ruleBucket(2), ["Tax", "Invoices", "Receipts"]) ===
      "#3 Receipts",
    "describeBucket: named rule (1-based index)",
  );
}
{
  // First rule shows as #1 not #0.
  expect(
    describeBucket(ruleBucket(0), ["First", "Second"]) === "#1 First",
    "describeBucket: zero-indexed rule renders as #1",
  );
}
{
  // No names array — fall back to "Rule #N".
  expect(
    describeBucket(ruleBucket(3)) === "Rule #4",
    "describeBucket: no names array fallback",
  );
}
{
  // Empty name in array — fall back to "Rule #N" so the popover
  // never reads as "#1 " with a trailing space.
  expect(
    describeBucket(ruleBucket(0), [""]) === "Rule #1",
    "describeBucket: empty-name fallback",
  );
}
{
  // Whitespace-only name — same fallback.
  expect(
    describeBucket(ruleBucket(0), ["   "]) === "Rule #1",
    "describeBucket: whitespace-name fallback",
  );
}
{
  // Out-of-range index against the names array — fall back, don't
  // render "undefined" or empty.
  expect(
    describeBucket(ruleBucket(99), ["Tax"]) === "Rule #100",
    "describeBucket: out-of-range index fallback",
  );
}

// ── suggestDrilldownExportFilename (Slice 90) ────────────────────────

// Use a fixed unix-millis for deterministic date assertions. 2026-06-21
// (Pacific date, but the helper uses LOCAL time so the assertion will
// hold for any reasonable test machine TZ assuming the unix-millis
// rounds inside that local day).
const FIXED_NOW = new Date("2026-06-21T19:00:00Z").getTime();

{
  // Fallthrough bucket, no watch id - reads "watch" + "fallthrough".
  const name = suggestDrilldownExportFilename(FALLTHROUGH_BUCKET, null, {
    now: FIXED_NOW,
  });
  expect(
    /^hopper-drilldown_watch_fallthrough_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestDrilldownExportFilename: fallthrough no-watch shape (${name})`,
  );
}
{
  // Fallthrough bucket, with watch id.
  const name = suggestDrilldownExportFilename(FALLTHROUGH_BUCKET, null, {
    watchId: 7,
    now: FIXED_NOW,
  });
  expect(
    /^hopper-drilldown_watch-7_fallthrough_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestDrilldownExportFilename: fallthrough watch-7 shape (${name})`,
  );
}
{
  // Rule bucket, no names -> bare rule-N slot.
  const name = suggestDrilldownExportFilename(ruleBucket(2), null, {
    watchId: 3,
    now: FIXED_NOW,
  });
  expect(
    /^hopper-drilldown_watch-3_rule-3_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestDrilldownExportFilename: rule no-names shape (${name})`,
  );
}
{
  // Rule bucket with a clean name -> rule-N_<slug>.
  const name = suggestDrilldownExportFilename(
    ruleBucket(0),
    ["Tax Forms 2026"],
    { watchId: 1, now: FIXED_NOW },
  );
  expect(
    /^hopper-drilldown_watch-1_rule-1_tax-forms-2026_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestDrilldownExportFilename: rule with slug (${name})`,
  );
}
{
  // Rule bucket with messy chars -> collapses to single dashes.
  const name = suggestDrilldownExportFilename(
    ruleBucket(0),
    ["  Tax/Forms  &  Stuff!!  "],
    { watchId: 1, now: FIXED_NOW },
  );
  expect(
    /_rule-1_tax-forms-stuff_/.test(name),
    `suggestDrilldownExportFilename: collapses messy chars to dashes (${name})`,
  );
}
{
  // Rule bucket with non-ASCII name -> NFD strips diacritics so the
  // slug stays portable. "café" -> "cafe".
  const name = suggestDrilldownExportFilename(ruleBucket(0), ["café"], {
    watchId: 1,
    now: FIXED_NOW,
  });
  expect(
    /_rule-1_cafe_/.test(name),
    `suggestDrilldownExportFilename: NFD strips diacritics (${name})`,
  );
}
{
  // Rule bucket with a name that's ALL non-ASCII / punctuation ->
  // slug is empty so the helper falls back to bare rule-N (no
  // double underscore between rule-N and the date).
  const name = suggestDrilldownExportFilename(ruleBucket(0), ["★★★"], {
    watchId: 1,
    now: FIXED_NOW,
  });
  expect(
    /^hopper-drilldown_watch-1_rule-1_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestDrilldownExportFilename: empty-slug falls back to bare rule-N (${name})`,
  );
}
{
  // Rule bucket with whitespace-only name -> bare rule-N.
  const name = suggestDrilldownExportFilename(ruleBucket(0), ["   "], {
    watchId: 1,
    now: FIXED_NOW,
  });
  expect(
    /_rule-1_\d{4}/.test(name),
    `suggestDrilldownExportFilename: whitespace-only name falls back (${name})`,
  );
}
{
  // Negative watch id falls back to bare "watch" (defensive — the
  // popover never has one but the contract should hold).
  const name = suggestDrilldownExportFilename(
    FALLTHROUGH_BUCKET,
    null,
    { watchId: -1, now: FIXED_NOW },
  );
  expect(
    /^hopper-drilldown_watch_fallthrough_/.test(name),
    `suggestDrilldownExportFilename: negative watch id falls back (${name})`,
  );
}
{
  // 1-based rule index — rule index 9 should read as rule-10, not rule-9.
  const name = suggestDrilldownExportFilename(ruleBucket(9), null, {
    watchId: 1,
    now: FIXED_NOW,
  });
  expect(
    /_rule-10_/.test(name),
    `suggestDrilldownExportFilename: rule index is 1-based (${name})`,
  );
}
{
  // Filename ends in .csv (every audit-export suggestion does).
  const name = suggestDrilldownExportFilename(
    FALLTHROUGH_BUCKET,
    null,
    { watchId: 1, now: FIXED_NOW },
  );
  expect(
    name.endsWith(".csv"),
    `suggestDrilldownExportFilename: ends with .csv (${name})`,
  );
}

// ── suggestDrilldownExportFilename ext slot (Slice 95) ───────────────

{
  // Default ext is "csv" — backwards compatibility with slice 90 call
  // sites that don't pass the option.
  const name = suggestDrilldownExportFilename(FALLTHROUGH_BUCKET, null, {
    now: FIXED_NOW,
  });
  expect(
    name.endsWith(".csv"),
    `suggestDrilldownExportFilename: default ext stays csv (${name})`,
  );
}
{
  // Explicit ext: "csv" produces the same shape as the implicit
  // default — round-tripping a deliberate caller.
  const a = suggestDrilldownExportFilename(FALLTHROUGH_BUCKET, null, {
    watchId: 7,
    now: FIXED_NOW,
  });
  const b = suggestDrilldownExportFilename(FALLTHROUGH_BUCKET, null, {
    watchId: 7,
    now: FIXED_NOW,
    ext: "csv",
  });
  expect(
    a === b,
    `suggestDrilldownExportFilename: explicit ext:"csv" matches default (${a} vs ${b})`,
  );
}
{
  // ext: "json" produces a .json suffix; everything else in the name
  // stays identical to the .csv form. This is the slice-94 JSON
  // export wrapper's primary call shape.
  const name = suggestDrilldownExportFilename(FALLTHROUGH_BUCKET, null, {
    watchId: 7,
    now: FIXED_NOW,
    ext: "json",
  });
  expect(
    /^hopper-drilldown_watch-7_fallthrough_\d{4}-\d{2}-\d{2}\.json$/.test(name),
    `suggestDrilldownExportFilename: ext json shape (${name})`,
  );
  expect(
    name.endsWith(".json"),
    `suggestDrilldownExportFilename: ext json ends in .json (${name})`,
  );
  expect(
    !name.endsWith(".csv"),
    `suggestDrilldownExportFilename: ext json doesn't end in .csv (${name})`,
  );
}
{
  // ext switch ONLY affects the suffix — bucket slot, slug, watch
  // slot, date all stay identical between the two ext forms. Diff
  // assertion: the names are identical apart from the .csv/.json
  // suffix.
  const csv = suggestDrilldownExportFilename(
    ruleBucket(2),
    ["Receipts Q1"],
    { watchId: 4, now: FIXED_NOW, ext: "csv" },
  );
  const json = suggestDrilldownExportFilename(
    ruleBucket(2),
    ["Receipts Q1"],
    { watchId: 4, now: FIXED_NOW, ext: "json" },
  );
  expect(
    csv.endsWith(".csv") && json.endsWith(".json"),
    `suggestDrilldownExportFilename: paired ext suffixes (${csv}, ${json})`,
  );
  expect(
    csv.slice(0, -4) === json.slice(0, -5),
    `suggestDrilldownExportFilename: only suffix differs between ext forms (${csv}, ${json})`,
  );
}
{
  // Rule bucket + ext:"json" still slugifies the name (same slug
  // behaviour as ext:"csv") — the ext switch is purely cosmetic and
  // doesn't touch any other slot.
  const name = suggestDrilldownExportFilename(
    ruleBucket(0),
    ["Tax Forms 2026"],
    { watchId: 1, now: FIXED_NOW, ext: "json" },
  );
  expect(
    /^hopper-drilldown_watch-1_rule-1_tax-forms-2026_\d{4}-\d{2}-\d{2}\.json$/.test(name),
    `suggestDrilldownExportFilename: rule slug + ext json (${name})`,
  );
}

// ── suggestCoverageExportFilename (Slice 125) ────────────────────────
// Same shape conventions as suggestDrilldownExportFilename but for a
// chain-wide coverage export — no per-bucket slot. Pin the format
// and the ext/watch slot behaviour.

const FIXED_COVERAGE_NOW = new Date("2026-06-22T10:30:00").getTime();

{
  const name = suggestCoverageExportFilename({ now: FIXED_COVERAGE_NOW });
  expect(
    /^hopper-coverage_watch_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestCoverageExportFilename: no-watch shape (${name})`,
  );
}

{
  const name = suggestCoverageExportFilename({
    watchId: 7,
    now: FIXED_COVERAGE_NOW,
  });
  expect(
    /^hopper-coverage_watch-7_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `suggestCoverageExportFilename: watch-7 shape (${name})`,
  );
}

{
  const name = suggestCoverageExportFilename({
    watchId: 3,
    now: FIXED_COVERAGE_NOW,
    ext: "json",
  });
  expect(
    /^hopper-coverage_watch-3_\d{4}-\d{2}-\d{2}\.json$/.test(name),
    `suggestCoverageExportFilename: json ext shape (${name})`,
  );
}

{
  // Same-epoch reproducibility: identical inputs produce identical
  // output strings (no internal date randomisation).
  const a = suggestCoverageExportFilename({
    watchId: 7,
    now: FIXED_COVERAGE_NOW,
  });
  const b = suggestCoverageExportFilename({
    watchId: 7,
    now: FIXED_COVERAGE_NOW,
  });
  expect(a === b, `suggestCoverageExportFilename: same-epoch reproducibility (${a})`);
}

{
  // ext switch ONLY affects the suffix — watch slot + date stay
  // identical between the two ext forms. Mirrors the parallel
  // contract on the drilldown filename helper.
  const csv = suggestCoverageExportFilename({
    watchId: 5,
    now: FIXED_COVERAGE_NOW,
    ext: "csv",
  });
  const json = suggestCoverageExportFilename({
    watchId: 5,
    now: FIXED_COVERAGE_NOW,
    ext: "json",
  });
  expect(
    csv.endsWith(".csv") && json.endsWith(".json"),
    `suggestCoverageExportFilename: paired ext suffixes (${csv}, ${json})`,
  );
  expect(
    csv.slice(0, -4) === json.slice(0, -5),
    `suggestCoverageExportFilename: only suffix differs between ext forms (${csv}, ${json})`,
  );
}

{
  // Invalid watch id slots fall back to the bare `watch` slot
  // matching the drilldown helper's defensive posture (negative or
  // NaN watch ids would otherwise produce ugly `watch--1` slots).
  const negative = suggestCoverageExportFilename({
    watchId: -1,
    now: FIXED_COVERAGE_NOW,
  });
  expect(
    /^hopper-coverage_watch_\d{4}-\d{2}-\d{2}\.csv$/.test(negative),
    `suggestCoverageExportFilename: negative watch id falls back (${negative})`,
  );

  const nullWatch = suggestCoverageExportFilename({
    watchId: null,
    now: FIXED_COVERAGE_NOW,
  });
  expect(
    /^hopper-coverage_watch_\d{4}-\d{2}-\d{2}\.csv$/.test(nullWatch),
    `suggestCoverageExportFilename: null watch id falls back (${nullWatch})`,
  );
}

{
  // Coverage filename has NO per-bucket slot (unlike the drilldown
  // filename helper). Pin that the chain-wide export name doesn't
  // accidentally pick up a fallthrough/rule slot from a future copy-
  // paste regression.
  const name = suggestCoverageExportFilename({
    watchId: 1,
    now: FIXED_COVERAGE_NOW,
  });
  expect(
    !name.includes("fallthrough") && !name.includes("rule-"),
    `suggestCoverageExportFilename: no bucket slot in chain-wide export (${name})`,
  );
}

// ── summarizeCoverageHealth (Slice 126) ──────────────────────────────
// Chain-health classifier composing per-row diagnostics into a
// single chain-level summary kind + copy line. Mutually exclusive
// kinds: empty | critical | warn | healthy. Pin the priority chain
// (dead > shadowed > zero > high-fall-through) and the
// pluralisation contract.

function cov(
  index: number,
  name: string,
  first_match: number,
  would_match: number,
  dead_at_position: boolean,
): RuleCoverage {
  return { index, name, first_match, would_match, dead_at_position };
}

function healthReport(
  rules: RuleCoverage[],
  fallthrough: number,
  total_samples: number,
): RuleCoverageReport {
  return { rules, fallthrough, total_samples };
}

{
  // Empty corpus — no samples at all reads as `empty` (distinct
  // from healthy so the UI can render a muted state, not a green
  // chip that suggests the chain is doing well).
  const h: CoverageHealth = summarizeCoverageHealth(healthReport([], 0, 0));
  expect(h.kind === "empty", `health: empty corpus -> 'empty' (got ${h.kind})`);
  expect(
    h.text === "No recent runs to assess",
    `health: empty text (${h.text})`,
  );
  expect(h.dead === 0 && h.shadowed === 0 && h.zero === 0, "health: empty counters");
  expect(h.fallthroughPct === 0, "health: empty fallthroughPct is 0 not NaN");
}

{
  // Healthy chain — every rule fires, low fall-through.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "Tax", 5, 5, false), cov(1, "Inv", 3, 3, false)], 2, 10),
  );
  expect(h.kind === "healthy", `health: healthy chain -> 'healthy' (got ${h.kind})`);
  expect(h.text === "Chain routing healthy", `health: healthy text (${h.text})`);
  expect(h.dead === 0 && h.shadowed === 0 && h.zero === 0, "health: healthy counters");
}

{
  // Critical — at least one dead rule wins priority over every
  // other classification.
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 10, 10, false),
        cov(1, "Dead Tax", 0, 5, true),
      ],
      0,
      10,
    ),
  );
  expect(h.kind === "critical", `health: dead -> 'critical' (got ${h.kind})`);
  expect(h.dead === 1, `health: dead count (${h.dead})`);
  expect(
    h.text === "1 dead rule — reorder or tighten the shadowing rules",
    `health: critical singular text (${h.text})`,
  );
}

{
  // Critical pluralisation — multiple dead rules pluralises the noun
  // ("3 dead rules") but the trailing imperative stays "rules"
  // either way (singular/plural verb already plural in the imperative).
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 10, 10, false),
        cov(1, "Dead A", 0, 5, true),
        cov(2, "Dead B", 0, 4, true),
        cov(3, "Dead C", 0, 3, true),
      ],
      0,
      10,
    ),
  );
  expect(h.dead === 3, `health: dead pluralisation count (${h.dead})`);
  expect(
    h.text === "3 dead rules — reorder or tighten the shadowing rules",
    `health: critical plural text (${h.text})`,
  );
}

{
  // Warn — partially-shadowed rule (would_match > first_match,
  // NOT dead). Singular noun phrase: "rule is partially shadowed".
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 7, 7, false),
        cov(1, "Sometimes Tax", 3, 6, false),
      ],
      0,
      10,
    ),
  );
  expect(h.kind === "warn", `health: shadowed -> 'warn' (got ${h.kind})`);
  expect(h.shadowed === 1, `health: shadowed count (${h.shadowed})`);
  expect(
    h.text === "1 rule is partially shadowed — reorder to recover matches",
    `health: shadowed singular text (${h.text})`,
  );
}

{
  // Warn pluralisation — multiple shadowed rules pluralises both
  // noun and verb: "2 rules are partially shadowed".
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "First", 5, 5, false),
        cov(1, "Shadow A", 2, 4, false),
        cov(2, "Shadow B", 1, 3, false),
      ],
      0,
      10,
    ),
  );
  expect(h.shadowed === 2, `health: shadowed plural count (${h.shadowed})`);
  expect(
    h.text === "2 rules are partially shadowed — reorder to recover matches",
    `health: shadowed plural text (${h.text})`,
  );
}

{
  // Warn — zero-coverage rule (would_match == 0). Lower priority
  // than shadowed; only surfaces when there are no shadowed rules.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "TooNarrow", 0, 0, false)], 0, 5),
  );
  expect(h.kind === "warn", `health: zero -> 'warn' (got ${h.kind})`);
  expect(h.zero === 1, `health: zero count (${h.zero})`);
  expect(
    h.text === "1 rule matches nothing — refine the predicates or drop them",
    `health: zero singular text (${h.text})`,
  );
}

{
  // Shadowed beats zero in the priority chain — pin the precedence.
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 5, 5, false),
        cov(1, "Shadow", 1, 3, false),
        cov(2, "Zero", 0, 0, false),
      ],
      0,
      10,
    ),
  );
  expect(h.kind === "warn", `health: mixed shadow+zero -> 'warn' (got ${h.kind})`);
  expect(h.shadowed === 1 && h.zero === 1, `health: both counters set (${h.shadowed}/${h.zero})`);
  expect(
    h.text.includes("partially shadowed"),
    `health: shadowed wins over zero in text (${h.text})`,
  );
}

{
  // High fall-through warn — no rule diagnostics, but more than
  // 25% of samples fell through to defaults.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "Tax", 5, 5, false)], 5, 10),
  );
  expect(h.kind === "warn", `health: high fallthrough -> 'warn' (got ${h.kind})`);
  expect(
    /\d+\.\d% of files fall through/.test(h.text),
    `health: high fallthrough text (${h.text})`,
  );
  expect(h.fallthroughPct === 50, `health: fallthroughPct one decimal (${h.fallthroughPct})`);
}

{
  // 25% fall-through is the threshold — STRICTLY greater than
  // triggers warn, so exactly 25% (4/16) stays healthy.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "Most", 12, 12, false)], 4, 16),
  );
  expect(h.kind === "healthy", `health: exactly 25% fallthrough stays healthy (got ${h.kind})`);
  expect(h.fallthroughPct === 25, `health: 25% pct (${h.fallthroughPct})`);
}

{
  // Override the warn threshold — a caller targeting stricter
  // chains (e.g. a compliance audit surface) bumps the threshold
  // down to 10% to flag earlier.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "Most", 4, 4, false)], 1, 5),
    { fallthroughWarnPct: 10 },
  );
  expect(h.kind === "warn", `health: tuneable threshold -> 'warn' at 20% (got ${h.kind})`);
}

{
  // Dead rule with high fall-through is STILL critical, NOT warn —
  // dead is higher priority than the fall-through threshold.
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 6, 6, false),
        cov(1, "Dead", 0, 2, true),
      ],
      4,
      10,
    ),
  );
  expect(
    h.kind === "critical",
    `health: dead beats high-fallthrough (got ${h.kind})`,
  );
}

{
  // Fall-through counter is carried verbatim from the report
  // (the UI can render a secondary "X% fall-through" sub-chip
  // without re-walking the report).
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "Tax", 7, 7, false)], 3, 10),
  );
  expect(h.fallthrough === 3, `health: fallthrough verbatim (${h.fallthrough})`);
  expect(h.fallthroughPct === 30, `health: fallthroughPct (${h.fallthroughPct})`);
}

// ── Slice 129 — filterCoverageByDiagnostic + helpers ────────────────────
//
// Mirror the Rust filter primitive's test coverage: priority chain,
// totals preservation, input-order preservation, identity transform,
// and the formatCoverageFilterSummary copy contract. Uses the same
// mixed-diagnostic 5-rule fixture for parity with the Rust side so a
// regression in either layer surfaces against the same data.

function mixedDiagnosticReport(): RuleCoverageReport {
  return healthReport(
    [
      // Healthy: predicate matches some samples + no shadow.
      cov(0, "Healthy A", 4, 4, false),
      // Dead at position: never wins here but would win earlier.
      // Priority dead > shadowed > zero applies.
      cov(1, "Dead B", 0, 3, true),
      // Partially shadowed: would_match > first_match, first_match > 0.
      cov(2, "Shadowed C", 1, 5, false),
      // Zero coverage: predicate matches nothing at all.
      cov(3, "Zero D", 0, 0, false),
      // Another healthy row so the All / Healthy filter counts are
      // non-trivial.
      cov(4, "Healthy E", 2, 2, false),
    ],
    7,
    17,
  );
}

// ── COVERAGE_FILTER_KINDS array shape ───────────────────────────────────

{
  // Every variant present + display order is "all > dead > shadowed
  // > zero > healthy" — pinned so the filter-chip row's button
  // ordering can't drift silently.
  expect(
    COVERAGE_FILTER_KINDS.length === 5,
    `COVERAGE_FILTER_KINDS: 5 variants (got ${COVERAGE_FILTER_KINDS.length})`,
  );
  expect(
    COVERAGE_FILTER_KINDS[0] === "all",
    `COVERAGE_FILTER_KINDS: 'all' first (got ${COVERAGE_FILTER_KINDS[0]})`,
  );
  expect(
    COVERAGE_FILTER_KINDS[1] === "dead",
    `COVERAGE_FILTER_KINDS: 'dead' second (got ${COVERAGE_FILTER_KINDS[1]})`,
  );
  expect(
    COVERAGE_FILTER_KINDS[2] === "shadowed",
    `COVERAGE_FILTER_KINDS: 'shadowed' third (got ${COVERAGE_FILTER_KINDS[2]})`,
  );
  expect(
    COVERAGE_FILTER_KINDS[3] === "zero",
    `COVERAGE_FILTER_KINDS: 'zero' fourth (got ${COVERAGE_FILTER_KINDS[3]})`,
  );
  expect(
    COVERAGE_FILTER_KINDS[4] === "healthy",
    `COVERAGE_FILTER_KINDS: 'healthy' last (got ${COVERAGE_FILTER_KINDS[4]})`,
  );
}

// ── ruleMatchesCoverageFilter ───────────────────────────────────────────

{
  // All filter matches every rule — the identity predicate.
  const r = cov(0, "Any", 5, 5, false);
  expect(
    ruleMatchesCoverageFilter(r, "all"),
    "ruleMatchesCoverageFilter: 'all' matches every rule",
  );
}
{
  const r = cov(0, "Dead", 0, 3, true);
  expect(
    ruleMatchesCoverageFilter(r, "dead"),
    "ruleMatchesCoverageFilter: dead rule matches 'dead'",
  );
  expect(
    !ruleMatchesCoverageFilter(r, "shadowed"),
    "ruleMatchesCoverageFilter: dead rule does NOT match 'shadowed' (priority)",
  );
  expect(
    !ruleMatchesCoverageFilter(r, "healthy"),
    "ruleMatchesCoverageFilter: dead rule does NOT match 'healthy'",
  );
}
{
  const r = cov(0, "Shadowed", 1, 5, false);
  expect(
    ruleMatchesCoverageFilter(r, "shadowed"),
    "ruleMatchesCoverageFilter: shadowed rule matches 'shadowed'",
  );
  expect(
    !ruleMatchesCoverageFilter(r, "dead"),
    "ruleMatchesCoverageFilter: shadowed-not-dead does NOT match 'dead'",
  );
}
{
  const r = cov(0, "Zero", 0, 0, false);
  expect(
    ruleMatchesCoverageFilter(r, "zero"),
    "ruleMatchesCoverageFilter: zero-coverage rule matches 'zero'",
  );
  expect(
    !ruleMatchesCoverageFilter(r, "shadowed"),
    "ruleMatchesCoverageFilter: zero rule does NOT match 'shadowed'",
  );
}
{
  const r = cov(0, "Healthy", 5, 5, false);
  expect(
    ruleMatchesCoverageFilter(r, "healthy"),
    "ruleMatchesCoverageFilter: healthy rule matches 'healthy'",
  );
  expect(
    !ruleMatchesCoverageFilter(r, "dead"),
    "ruleMatchesCoverageFilter: healthy rule does NOT match 'dead'",
  );
}

// ── filterCoverageByDiagnostic ──────────────────────────────────────────

{
  // 'all' is the identity transform — same rules, same counts.
  const src = mixedDiagnosticReport();
  const got = filterCoverageByDiagnostic(src, "all");
  expect(
    got.rules.length === src.rules.length,
    `filter 'all': preserves rule count (got ${got.rules.length})`,
  );
  expect(
    got.fallthrough === src.fallthrough && got.total_samples === src.total_samples,
    "filter 'all': preserves totals",
  );
}
{
  const src = mixedDiagnosticReport();
  const got = filterCoverageByDiagnostic(src, "dead");
  expect(got.rules.length === 1, `filter 'dead': 1 row (got ${got.rules.length})`);
  expect(got.rules[0].name === "Dead B", `filter 'dead': Dead B (got ${got.rules[0].name})`);
  // Totals preserved verbatim.
  expect(
    got.fallthrough === 7 && got.total_samples === 17,
    "filter 'dead': preserves fallthrough + total_samples",
  );
}
{
  // Shadowed filter EXCLUDES dead rules even though a dead rule's
  // would_match > first_match satisfies the shadowed predicate.
  // Pin the priority chain end-to-end matching the Rust side.
  const src = mixedDiagnosticReport();
  const got = filterCoverageByDiagnostic(src, "shadowed");
  expect(
    got.rules.length === 1,
    `filter 'shadowed': 1 row (got ${got.rules.length})`,
  );
  expect(
    got.rules[0].name === "Shadowed C",
    `filter 'shadowed': Shadowed C (got ${got.rules[0].name})`,
  );
  expect(
    got.rules.every((r) => !r.dead_at_position),
    "filter 'shadowed': excludes dead rules (priority)",
  );
}
{
  const src = mixedDiagnosticReport();
  const got = filterCoverageByDiagnostic(src, "zero");
  expect(got.rules.length === 1, `filter 'zero': 1 row (got ${got.rules.length})`);
  expect(got.rules[0].name === "Zero D", `filter 'zero': Zero D`);
}
{
  const src = mixedDiagnosticReport();
  const got = filterCoverageByDiagnostic(src, "healthy");
  expect(
    got.rules.length === 2,
    `filter 'healthy': 2 rows (got ${got.rules.length})`,
  );
  expect(
    got.rules[0].name === "Healthy A" && got.rules[1].name === "Healthy E",
    "filter 'healthy': order preserved (A then E, not resorted)",
  );
}
{
  // Conservation invariant — sum of the four bucket counts ==
  // total rule count. Mirrors the Rust side's
  // filter_envelope_counts_agree_with_filter_results test.
  const src = mixedDiagnosticReport();
  const dead = filterCoverageByDiagnostic(src, "dead").rules.length;
  const zero = filterCoverageByDiagnostic(src, "zero").rules.length;
  const shadowed = filterCoverageByDiagnostic(src, "shadowed").rules.length;
  const healthy = filterCoverageByDiagnostic(src, "healthy").rules.length;
  expect(
    dead + zero + shadowed + healthy === src.rules.length,
    `filter conservation: ${dead}+${zero}+${shadowed}+${healthy} == ${src.rules.length}`,
  );
}
{
  // Empty input -> empty rules, totals preserved.
  const src = healthReport([], 5, 5);
  for (const f of COVERAGE_FILTER_KINDS) {
    const got = filterCoverageByDiagnostic(src, f);
    expect(
      got.rules.length === 0,
      `filter '${f}' on empty rules: 0 rows (got ${got.rules.length})`,
    );
    expect(
      got.fallthrough === 5 && got.total_samples === 5,
      `filter '${f}' on empty rules: totals preserved`,
    );
  }
}
{
  // Purity: source must not mutate after filtering. A regression
  // in the identity-clone path would surface here.
  const src = mixedDiagnosticReport();
  const beforeLen = src.rules.length;
  const beforeName = src.rules[0].name;
  filterCoverageByDiagnostic(src, "dead");
  filterCoverageByDiagnostic(src, "all"); // identity transform
  expect(src.rules.length === beforeLen, "filter purity: rules.length unchanged");
  expect(src.rules[0].name === beforeName, "filter purity: rules[0].name unchanged");
}
{
  // Identity transform returns a SHALLOW CLONE, not the same array
  // reference — pinning so a future "return report" shortcut can't
  // leak mutations back through `.slice()`.
  const src = mixedDiagnosticReport();
  const got = filterCoverageByDiagnostic(src, "all");
  expect(
    got.rules !== src.rules,
    "filter 'all': returns a NEW rules array (shallow clone)",
  );
  // But the per-rule object identity IS shared (shallow clone).
  expect(
    got.rules[0] === src.rules[0],
    "filter 'all': per-rule object identity preserved (shallow)",
  );
}

// ── formatCoverageFilterSummary ─────────────────────────────────────────

{
  // 'all' filter with multiple rules — "Showing all N rules".
  const s: CoverageDiagnosticFilter = "all";
  expect(
    formatCoverageFilterSummary(s, 6, 6) === "Showing all 6 rules",
    "filter summary 'all': 'Showing all 6 rules'",
  );
}
{
  // 'all' filter with one rule — singular noun.
  expect(
    formatCoverageFilterSummary("all", 1, 1) === "Showing all 1 rule",
    "filter summary 'all' singular: 'Showing all 1 rule'",
  );
}
{
  // Non-'all' filter — "Showing X of Y rules — <kind>".
  expect(
    formatCoverageFilterSummary("dead", 2, 6) === "Showing 2 of 6 rules — dead",
    "filter summary 'dead' plural",
  );
  expect(
    formatCoverageFilterSummary("shadowed", 3, 7)
      === "Showing 3 of 7 rules — shadowed",
    "filter summary 'shadowed' plural",
  );
  expect(
    formatCoverageFilterSummary("zero", 1, 5) === "Showing 1 of 5 rules — zero",
    "filter summary 'zero' plural still uses 'rules' noun",
  );
  expect(
    formatCoverageFilterSummary("healthy", 4, 4)
      === "Showing 4 of 4 rules — healthy",
    "filter summary 'healthy' all-match",
  );
}
{
  // Zero-rule chain — distinct empty-state copy that doesn't
  // pluralise weirdly.
  expect(
    formatCoverageFilterSummary("all", 0, 0) === "Showing 0 rules",
    "filter summary empty chain: 'Showing 0 rules'",
  );
  expect(
    formatCoverageFilterSummary("dead", 0, 0) === "Showing 0 rules",
    "filter summary empty chain non-'all': same empty copy",
  );
}
{
  // No rules match the filter — "Showing 0 of 6 rules — dead".
  // Honest copy; the UI surfaces this beside the chain-health chip
  // so a user clicking through to a kind with zero matches sees
  // exactly why the list is empty.
  expect(
    formatCoverageFilterSummary("dead", 0, 6)
      === "Showing 0 of 6 rules — dead",
    "filter summary no matches",
  );
}
{
  // Single-rule total still uses 'rule' (singular) noun on a
  // narrowing filter — matches the 'all' singular branch above.
  expect(
    formatCoverageFilterSummary("healthy", 1, 1)
      === "Showing 1 of 1 rule — healthy",
    "filter summary singular total",
  );
}

// ── Slice 130 — suggestCoverageExportFilename filter slot ──────────────
//
// The filename helper gains an optional `filter` parameter carrying
// the diagnostic slug. Back-compat: `"all"` or unset omits the slot
// so the round-26 export filenames round-trip byte-for-byte.

{
  // No filter / "all" filter — original round-26 shape preserved.
  const noFilter = suggestCoverageExportFilename({
    watchId: 7,
    now: FIXED_COVERAGE_NOW,
  });
  const allFilter = suggestCoverageExportFilename({
    watchId: 7,
    now: FIXED_COVERAGE_NOW,
    filter: "all",
  });
  expect(
    noFilter === allFilter,
    `coverage filename: unset filter == 'all' filter (${noFilter} vs ${allFilter})`,
  );
  expect(
    /^hopper-coverage_watch-7_\d{4}-\d{2}-\d{2}\.csv$/.test(noFilter),
    `coverage filename: 'all'/unset matches round-26 shape (${noFilter})`,
  );
  expect(
    !noFilter.includes("_all_"),
    `coverage filename: 'all' is omitted (not '_all_' literal) (${noFilter})`,
  );
}

{
  // Narrowing filter inserts the slug between watch + date.
  const name = suggestCoverageExportFilename({
    watchId: 7,
    now: FIXED_COVERAGE_NOW,
    filter: "dead",
  });
  expect(
    /^hopper-coverage_watch-7_dead_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `coverage filename: 'dead' filter slot (${name})`,
  );
}

{
  // Every narrowing filter slug appears verbatim in the filename so
  // a consumer grepping for `_dead_` / `_shadowed_` / etc finds the
  // right files. Pin every slug so a future rename surfaces here.
  for (const slug of ["dead", "shadowed", "zero", "healthy"] as const) {
    const name = suggestCoverageExportFilename({
      watchId: 7,
      now: FIXED_COVERAGE_NOW,
      filter: slug,
    });
    expect(
      name.includes(`_${slug}_`),
      `coverage filename: '${slug}' slug present (${name})`,
    );
  }
}

{
  // Filter slot composes with the json ext switch.
  const name = suggestCoverageExportFilename({
    watchId: 3,
    now: FIXED_COVERAGE_NOW,
    filter: "shadowed",
    ext: "json",
  });
  expect(
    /^hopper-coverage_watch-3_shadowed_\d{4}-\d{2}-\d{2}\.json$/.test(name),
    `coverage filename: filter + json ext (${name})`,
  );
}

{
  // Filter slot composes with the watch-fallback path — a paralegal
  // exporting from a context without a watch id and a narrowing
  // filter still gets the slug in the filename.
  const name = suggestCoverageExportFilename({
    now: FIXED_COVERAGE_NOW,
    filter: "zero",
  });
  expect(
    /^hopper-coverage_watch_zero_\d{4}-\d{2}-\d{2}\.csv$/.test(name),
    `coverage filename: filter + watch-fallback (${name})`,
  );
}

// ── Slice 131 — coverageHealthClickTarget ──────────────────────────────
//
// Bridge from CoverageHealth (chain-level chip state) to the
// CoverageDiagnosticFilter kind whose chip the click-through should
// activate. Pin the priority chain (dead > shadowed > zero > high-
// fall-through has no rule-level target) end-to-end against the
// summarizeCoverageHealth classifier so a future change to either
// helper surfaces here when the two drift.

{
  // null health -> null target. UI calls this before the report
  // loads, when there's nothing to click.
  expect(
    coverageHealthClickTarget(null) === null,
    "coverageHealthClickTarget: null -> null",
  );
}

{
  // Empty corpus -> null target. The chip is hidden upstream but
  // pin the contract anyway so a stale chip can't click into a
  // filter view that has nothing to show.
  const h = summarizeCoverageHealth(healthReport([], 0, 0));
  expect(
    coverageHealthClickTarget(h) === null,
    `coverageHealthClickTarget: empty -> null (kind=${h.kind})`,
  );
}

{
  // Healthy chain -> null target. There's no diagnostic to drill
  // into; the chip itself is informational only.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "OK", 5, 5, false)], 0, 5),
  );
  expect(
    coverageHealthClickTarget(h) === null,
    `coverageHealthClickTarget: healthy -> null (kind=${h.kind})`,
  );
}

{
  // Critical (dead rule) -> "dead". Mirror the
  // summarizeCoverageHealth priority chain — dead is the
  // most-actionable insight and the chip's copy points there
  // ("1 dead rule — reorder or tighten the shadowing rules").
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 5, 5, false),
        cov(1, "Dead", 0, 2, true),
      ],
      0,
      5,
    ),
  );
  expect(h.kind === "critical", `coverageHealthClickTarget: setup critical`);
  expect(
    coverageHealthClickTarget(h) === "dead",
    `coverageHealthClickTarget: critical -> 'dead' (got ${coverageHealthClickTarget(h)})`,
  );
}

{
  // Warn (shadowed only) -> "shadowed". Priority chain:
  // shadowed > zero > high-fall-through.
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Tax", 3, 5, false), // partially shadowed
        cov(1, "OK", 2, 2, false),
      ],
      0,
      5,
    ),
  );
  expect(h.kind === "warn", `coverageHealthClickTarget: setup warn (shadowed)`);
  expect(
    coverageHealthClickTarget(h) === "shadowed",
    `coverageHealthClickTarget: warn shadowed -> 'shadowed' (got ${coverageHealthClickTarget(h)})`,
  );
}

{
  // Warn (zero only) -> "zero".
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Never", 0, 0, false), // zero-coverage
        cov(1, "OK", 5, 5, false),
      ],
      0,
      5,
    ),
  );
  expect(h.kind === "warn", `coverageHealthClickTarget: setup warn (zero)`);
  expect(
    coverageHealthClickTarget(h) === "zero",
    `coverageHealthClickTarget: warn zero -> 'zero' (got ${coverageHealthClickTarget(h)})`,
  );
}

{
  // Warn (mixed: 2 shadowed + 1 zero) -> "shadowed" (priority).
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Tax", 3, 5, false), // shadowed
        cov(1, "Inv", 2, 4, false), // shadowed
        cov(2, "Never", 0, 0, false), // zero
        cov(3, "OK", 2, 2, false),
      ],
      0,
      7,
    ),
  );
  expect(h.kind === "warn", `coverageHealthClickTarget: setup warn (mixed)`);
  expect(
    coverageHealthClickTarget(h) === "shadowed",
    `coverageHealthClickTarget: warn mixed -> 'shadowed' (got ${coverageHealthClickTarget(h)})`,
  );
}

{
  // Warn (high fall-through with no dead/shadowed/zero) -> null.
  // No rule-level filter expresses "this percentage of files fell
  // through" — the fall-through ROW is a separate UI affordance.
  const h = summarizeCoverageHealth(
    healthReport([cov(0, "Tax", 5, 5, false)], 5, 10),
  );
  expect(h.kind === "warn", `coverageHealthClickTarget: setup warn (high-ft)`);
  expect(h.dead === 0 && h.shadowed === 0 && h.zero === 0, "warn-only fall-through");
  expect(
    coverageHealthClickTarget(h) === null,
    `coverageHealthClickTarget: warn high-ft -> null (got ${coverageHealthClickTarget(h)})`,
  );
}

{
  // Critical-with-mixed: dead rule + shadowed rule -> "dead" wins.
  // Pin the cross-kind priority so a critical chain ALWAYS goes
  // to dead, regardless of other diagnostics.
  const h = summarizeCoverageHealth(
    healthReport(
      [
        cov(0, "Always", 6, 6, false),
        cov(1, "Dead", 0, 2, true),
        cov(2, "Shadowed", 1, 4, false),
      ],
      0,
      7,
    ),
  );
  expect(h.kind === "critical", `coverageHealthClickTarget: setup critical-mixed`);
  expect(
    coverageHealthClickTarget(h) === "dead",
    `coverageHealthClickTarget: critical with shadowed too -> 'dead' (got ${coverageHealthClickTarget(h)})`,
  );
}

{
  // Cross-helper agreement: every health.kind reachable via
  // summarizeCoverageHealth has a click-target the filter helper
  // can actually return. Pin the contract: the click target is
  // ALWAYS one of "all"-less filter kinds (or null) — never "all"
  // (which would be a no-op transition).
  for (const target of [
    coverageHealthClickTarget(summarizeCoverageHealth(healthReport([], 0, 0))),
    coverageHealthClickTarget(
      summarizeCoverageHealth(
        healthReport([cov(0, "Dead", 0, 2, true), cov(1, "OK", 5, 5, false)], 0, 5),
      ),
    ),
    coverageHealthClickTarget(
      summarizeCoverageHealth(
        healthReport([cov(0, "Sh", 3, 5, false), cov(1, "OK", 2, 2, false)], 0, 5),
      ),
    ),
    coverageHealthClickTarget(
      summarizeCoverageHealth(
        healthReport([cov(0, "Z", 0, 0, false), cov(1, "OK", 5, 5, false)], 0, 5),
      ),
    ),
    coverageHealthClickTarget(
      summarizeCoverageHealth(healthReport([cov(0, "OK", 5, 5, false)], 0, 5)),
    ),
  ]) {
    expect(
      target !== "all",
      `coverageHealthClickTarget: never returns "all" (got ${target})`,
    );
    expect(
      target === null
        || target === "dead"
        || target === "shadowed"
        || target === "zero",
      `coverageHealthClickTarget: only returns dead/shadowed/zero/null (got ${target})`,
    );
  }
}

// ─── Slice 134 — planDeadRuleReorder / applyReorderProposal / formatReorderProposal ──

/** Build a Rule with the given name + predicate kind. The fix-it
 *  planner only inspects predicate.kind ("always" vs the rest) and
 *  the name, so a minimal rule object suffices. */
function ruleFor(
  name: string,
  kind: "always" | "filename-glob" = "filename-glob",
  pattern = "*.pdf",
): Rule {
  const action = {
    recipe_id: null,
    output_dir: null,
    rename_pattern: null,
  };
  if (kind === "always") {
    return { name, predicate: { kind: "always" }, action };
  }
  return {
    name,
    predicate: { kind: "filename-glob", pattern },
    action,
  };
}

{
  // planDeadRuleReorder: empty rules -> empty proposals.
  const proposals = planDeadRuleReorder([], healthReport([], 0, 0));
  expect(proposals.length === 0, "planDeadRuleReorder: empty rules -> empty");
}

{
  // planDeadRuleReorder: healthy chain -> empty proposals (no dead).
  const rules = [ruleFor("Tax"), ruleFor("All", "always")];
  const r = healthReport(
    [cov(0, "Tax", 3, 3, false), cov(1, "All", 7, 7, false)],
    0,
    10,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 0, "planDeadRuleReorder: healthy -> empty");
}

{
  // planDeadRuleReorder: classic Always-shadows-Tax case.
  const rules = [ruleFor("Catch-all", "always"), ruleFor("Tax", "filename-glob", "tax_*")];
  const r = healthReport(
    [cov(0, "Catch-all", 10, 10, false), cov(1, "Tax", 0, 3, true)],
    0,
    10,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 1, "planDeadRuleReorder: one dead -> one proposal");
  const p = proposals[0];
  expect(p.rule_index === 1, `planDeadRuleReorder: rule_index=1 (got ${p.rule_index})`);
  expect(p.rule_name === "Tax", `planDeadRuleReorder: rule_name=Tax`);
  expect(p.target_index === 0, `planDeadRuleReorder: target_index=0`);
  expect(
    p.shadowing_rule_name === "Catch-all",
    `planDeadRuleReorder: shadowing_rule_name=Catch-all`,
  );
  expect(p.samples_recovered === 3, `planDeadRuleReorder: samples_recovered=3`);
}

{
  // planDeadRuleReorder: earliest Always selected across multiple.
  const rules = [
    ruleFor("Early all", "always"),
    ruleFor("Spec", "filename-glob", "s_*"),
    ruleFor("Late all", "always"),
    ruleFor("Tax", "filename-glob", "t_*"),
  ];
  const r = healthReport(
    [
      cov(0, "Early all", 10, 10, false),
      cov(1, "Spec", 0, 2, true),
      cov(2, "Late all", 0, 0, false),
      cov(3, "Tax", 0, 3, true),
    ],
    0,
    10,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 2, "planDeadRuleReorder: 2 dead -> 2 proposals");
  for (const p of proposals) {
    expect(
      p.target_index === 0,
      `planDeadRuleReorder: earliest Always (got ${p.target_index})`,
    );
    expect(
      p.shadowing_rule_name === "Early all",
      `planDeadRuleReorder: shadower=Early all`,
    );
  }
}

{
  // planDeadRuleReorder: no Always shadower -> target=0 + empty
  // shadowing_rule_name (UI generic-copy path).
  const rules = [
    ruleFor("Wide", "filename-glob", "*.pdf"),
    ruleFor("Tax", "filename-glob", "tax_*"),
  ];
  const r = healthReport(
    [cov(0, "Wide", 5, 5, false), cov(1, "Tax", 0, 2, true)],
    0,
    5,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 1, "planDeadRuleReorder: no-Always one proposal");
  const p = proposals[0];
  expect(p.target_index === 0, `planDeadRuleReorder: fallback target=0`);
  expect(
    p.shadowing_rule_name === "",
    `planDeadRuleReorder: fallback empty shadower (got '${p.shadowing_rule_name}')`,
  );
}

{
  // planDeadRuleReorder: input order preserved (not severity-sorted).
  const rules = [
    ruleFor("All", "always"),
    ruleFor("Low", "filename-glob", "*.pdf"),
    ruleFor("Healthy", "always"),
    ruleFor("High", "filename-glob", "*.pdf"),
  ];
  const r = healthReport(
    [
      cov(0, "All", 5, 5, false),
      cov(1, "Low", 0, 1, true),
      cov(2, "Healthy", 0, 0, false),
      cov(3, "High", 0, 99, true),
    ],
    0,
    5,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(
    proposals.length === 2 && proposals[0].rule_index === 1 && proposals[1].rule_index === 3,
    `planDeadRuleReorder: input order preserved`,
  );
  expect(
    proposals[0].samples_recovered === 1 && proposals[1].samples_recovered === 99,
    `planDeadRuleReorder: counts preserved`,
  );
}

{
  // planDeadRuleReorder: stale row (index > rules.length) skipped.
  const rules = [ruleFor("All", "always"), ruleFor("A", "filename-glob", "a_*")];
  const r = healthReport(
    [
      cov(0, "All", 5, 5, false),
      cov(1, "A", 0, 1, true),
      cov(5, "Stale", 0, 99, true),
    ],
    0,
    5,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 1, "planDeadRuleReorder: stale row skipped");
  expect(proposals[0].rule_index === 1, "planDeadRuleReorder: in-range row kept");
}

{
  // planDeadRuleReorder: target_index < rule_index invariant.
  const rules = [
    ruleFor("All", "always"),
    ruleFor("A", "filename-glob", "a_*"),
    ruleFor("B", "filename-glob", "b_*"),
  ];
  const r = healthReport(
    [
      cov(0, "All", 5, 5, false),
      cov(1, "A", 0, 1, true),
      cov(2, "B", 0, 2, true),
    ],
    0,
    5,
  );
  const proposals = planDeadRuleReorder(rules, r);
  for (const p of proposals) {
    expect(
      p.target_index < p.rule_index,
      `planDeadRuleReorder: target_index < rule_index (got t=${p.target_index} r=${p.rule_index})`,
    );
  }
}

{
  // applyReorderProposal: moves rule from rule_index to target_index.
  const rules = [
    ruleFor("All", "always"),
    ruleFor("Tax", "filename-glob", "tax_*"),
  ];
  const proposal: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "All",
    samples_recovered: 3,
  };
  const next = applyReorderProposal(rules, proposal);
  expect(next[0].name === "Tax", "applyReorderProposal: Tax now at index 0");
  expect(next[1].name === "All", "applyReorderProposal: All pushed to index 1");
  expect(next.length === 2, "applyReorderProposal: length preserved");
  // Pure function — source unmodified.
  expect(rules[0].name === "All", "applyReorderProposal: source unmutated");
}

{
  // applyReorderProposal: returns NEW array (not same reference).
  const rules = [ruleFor("A", "always"), ruleFor("B")];
  const proposal: ReorderProposal = {
    rule_index: 1,
    rule_name: "B",
    target_index: 0,
    shadowing_rule_name: "A",
    samples_recovered: 1,
  };
  const next = applyReorderProposal(rules, proposal);
  expect(next !== rules, "applyReorderProposal: NEW array, not same reference");
  // Shared rule object identity — moved object is still ===.
  expect(next[0] === rules[1], "applyReorderProposal: moved rule object identity shared");
}

{
  // applyReorderProposal: target >= rule_index is a no-op (returns source).
  const rules = [ruleFor("A"), ruleFor("B")];
  const noop: ReorderProposal = {
    rule_index: 0,
    rule_name: "A",
    target_index: 1,
    shadowing_rule_name: "",
    samples_recovered: 0,
  };
  const next = applyReorderProposal(rules, noop);
  expect(next === rules, "applyReorderProposal: target>=rule_index is no-op");
}

{
  // applyReorderProposal: out-of-range rule_index is a no-op.
  const rules = [ruleFor("A"), ruleFor("B")];
  const stale: ReorderProposal = {
    rule_index: 5,
    rule_name: "Stale",
    target_index: 0,
    shadowing_rule_name: "",
    samples_recovered: 1,
  };
  const next = applyReorderProposal(rules, stale);
  expect(next === rules, "applyReorderProposal: stale index is no-op");
}

{
  // applyReorderProposal: three-rule chain, middle moves to front.
  const rules = [ruleFor("Wide", "always"), ruleFor("Mid"), ruleFor("Last")];
  const proposal: ReorderProposal = {
    rule_index: 1,
    rule_name: "Mid",
    target_index: 0,
    shadowing_rule_name: "Wide",
    samples_recovered: 2,
  };
  const next = applyReorderProposal(rules, proposal);
  expect(
    next[0].name === "Mid" && next[1].name === "Wide" && next[2].name === "Last",
    `applyReorderProposal: middle->front (got ${next.map((r) => r.name).join(",")})`,
  );
}

{
  // formatReorderProposal: with shadower, plural matches.
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "Catch-all",
    samples_recovered: 3,
  };
  const copy = formatReorderProposal(p);
  expect(
    copy === "Move 'Tax' before 'Catch-all' to recover 3 matches",
    `formatReorderProposal: with-shadower plural: ${copy}`,
  );
}

{
  // formatReorderProposal: with shadower, singular match.
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "Catch-all",
    samples_recovered: 1,
  };
  const copy = formatReorderProposal(p);
  expect(
    copy === "Move 'Tax' before 'Catch-all' to recover 1 match",
    `formatReorderProposal: with-shadower singular: ${copy}`,
  );
}

{
  // formatReorderProposal: without shadower (front-of-chain copy).
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "",
    samples_recovered: 2,
  };
  const copy = formatReorderProposal(p);
  expect(
    copy === "Move 'Tax' to the front of the chain to recover 2 matches",
    `formatReorderProposal: without-shadower: ${copy}`,
  );
}

{
  // formatReorderProposal: zero recovered (predicate now matches 0).
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "Catch-all",
    samples_recovered: 0,
  };
  const copy = formatReorderProposal(p);
  expect(
    copy === "Move 'Tax' before 'Catch-all' (predicate now matches 0 samples)",
    `formatReorderProposal: zero recovered: ${copy}`,
  );
}

{
  // formatReorderProposal: empty rule name falls back to positional.
  const p: ReorderProposal = {
    rule_index: 3,
    rule_name: "",
    target_index: 0,
    shadowing_rule_name: "Catch-all",
    samples_recovered: 5,
  };
  const copy = formatReorderProposal(p);
  expect(
    copy === "Move 'Rule #4' before 'Catch-all' to recover 5 matches",
    `formatReorderProposal: empty name -> Rule #N: ${copy}`,
  );
}

{
  // Cross-helper round-trip: plan + apply + format compose cleanly.
  const rules = [
    ruleFor("Catch-all", "always"),
    ruleFor("Tax", "filename-glob", "tax_*"),
  ];
  const r = healthReport(
    [cov(0, "Catch-all", 10, 10, false), cov(1, "Tax", 0, 3, true)],
    0,
    10,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 1, "round-trip: one proposal");
  const next = applyReorderProposal(rules, proposals[0]);
  expect(next[0].name === "Tax", "round-trip: Tax now first");
  expect(
    formatReorderProposal(proposals[0]).includes("Move 'Tax'"),
    "round-trip: copy mentions Tax",
  );
}

// ─── Slice 135 — slabHopperPlanDeadRuleReorder browser-mode wrapper ──
//
// The wrapper delegates to the TS planner when isInTauri() is false.
// The test process runs outside Tauri (no @tauri-apps/api/core ctx),
// so the wrapper MUST take the local-helper branch.

{
  // Browser-mode wrapper returns the SAME shape as the TS planner.
  const rules = [
    ruleFor("Catch-all", "always"),
    ruleFor("Tax", "filename-glob", "tax_*"),
  ];
  const r = healthReport(
    [cov(0, "Catch-all", 10, 10, false), cov(1, "Tax", 0, 3, true)],
    0,
    10,
  );
  // Wrapper is async; await + compare to the synchronous TS planner.
  slabHopperPlanDeadRuleReorder(rules, r).then((wrapperOut) => {
    const directOut = planDeadRuleReorder(rules, r);
    expect(
      wrapperOut.length === directOut.length,
      `slabHopperPlanDeadRuleReorder: same length as direct planner`,
    );
    expect(
      wrapperOut[0].rule_index === directOut[0].rule_index,
      `slabHopperPlanDeadRuleReorder: same rule_index as direct`,
    );
    expect(
      wrapperOut[0].target_index === directOut[0].target_index,
      `slabHopperPlanDeadRuleReorder: same target_index as direct`,
    );
    expect(
      wrapperOut[0].shadowing_rule_name === directOut[0].shadowing_rule_name,
      `slabHopperPlanDeadRuleReorder: same shadowing_rule_name as direct`,
    );
    expect(
      wrapperOut[0].samples_recovered === directOut[0].samples_recovered,
      `slabHopperPlanDeadRuleReorder: same samples_recovered as direct`,
    );
  });
}

{
  // Browser-mode wrapper: empty proposals on a healthy chain (the
  // wrapper takes the local-helper branch and returns []).
  const rules = [ruleFor("Tax"), ruleFor("All", "always")];
  const r = healthReport(
    [cov(0, "Tax", 3, 3, false), cov(1, "All", 7, 7, false)],
    0,
    10,
  );
  slabHopperPlanDeadRuleReorder(rules, r).then((wrapperOut) => {
    expect(
      wrapperOut.length === 0,
      `slabHopperPlanDeadRuleReorder: healthy chain -> empty`,
    );
  });
}

// ─── Slice 136 — reorderProposalConfidence / filter / describe ───────

{
  // High confidence: named shadower + samples_recovered > 0.
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "Catch-all",
    samples_recovered: 3,
  };
  expect(
    reorderProposalConfidence(p) === "high",
    `confidence: named shadower + recovered > 0 -> high (got ${reorderProposalConfidence(p)})`,
  );
}

{
  // Medium confidence: named shadower BUT zero samples recovered.
  // The reorder is structurally correct; the gain is theoretical.
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Future-tax",
    target_index: 0,
    shadowing_rule_name: "Catch-all",
    samples_recovered: 0,
  };
  expect(
    reorderProposalConfidence(p) === "medium",
    `confidence: named shadower + recovered=0 -> medium (got ${reorderProposalConfidence(p)})`,
  );
}

{
  // Low confidence: NO named shadower (fallback to target=0).
  // Aggressive jump-to-front; the user should read carefully.
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "",
    samples_recovered: 3,
  };
  expect(
    reorderProposalConfidence(p) === "low",
    `confidence: no shadower -> low (got ${reorderProposalConfidence(p)})`,
  );
}

{
  // Whitespace-only shadower name treated as empty (low).
  // A future regression that didn't .trim() would mis-classify
  // these as "high" — pin the trim contract.
  const p: ReorderProposal = {
    rule_index: 1,
    rule_name: "Tax",
    target_index: 0,
    shadowing_rule_name: "   ",
    samples_recovered: 3,
  };
  expect(
    reorderProposalConfidence(p) === "low",
    `confidence: whitespace-only shadower -> low`,
  );
}

{
  // filterProposalsByConfidence: min='low' returns everything.
  const proposals: ReorderProposal[] = [
    {
      rule_index: 1,
      rule_name: "A",
      target_index: 0,
      shadowing_rule_name: "All",
      samples_recovered: 3,
    },
    {
      rule_index: 2,
      rule_name: "B",
      target_index: 0,
      shadowing_rule_name: "All",
      samples_recovered: 0,
    },
    {
      rule_index: 3,
      rule_name: "C",
      target_index: 0,
      shadowing_rule_name: "",
      samples_recovered: 1,
    },
  ];
  expect(
    filterProposalsByConfidence(proposals, "low").length === 3,
    `filterProposalsByConfidence: min=low keeps all`,
  );
  expect(
    filterProposalsByConfidence(proposals, "medium").length === 2,
    `filterProposalsByConfidence: min=medium drops low (keeps high+medium)`,
  );
  expect(
    filterProposalsByConfidence(proposals, "high").length === 1,
    `filterProposalsByConfidence: min=high keeps only high`,
  );
}

{
  // filterProposalsByConfidence: input order preserved within the
  // filtered subset (filter doesn't re-sort).
  const proposals: ReorderProposal[] = [
    {
      rule_index: 5,
      rule_name: "High1",
      target_index: 0,
      shadowing_rule_name: "All",
      samples_recovered: 3,
    },
    {
      rule_index: 2,
      rule_name: "Low",
      target_index: 0,
      shadowing_rule_name: "",
      samples_recovered: 1,
    },
    {
      rule_index: 7,
      rule_name: "High2",
      target_index: 0,
      shadowing_rule_name: "All",
      samples_recovered: 5,
    },
  ];
  const high = filterProposalsByConfidence(proposals, "high");
  expect(
    high.length === 2 && high[0].rule_name === "High1" && high[1].rule_name === "High2",
    `filterProposalsByConfidence: input order preserved`,
  );
}

{
  // describeReorderConfidence: every tier has unique non-empty copy.
  const tiers: ReorderProposalConfidence[] = ["high", "medium", "low"];
  const copies = tiers.map(describeReorderConfidence);
  for (const c of copies) {
    expect(c.length > 0, `describeReorderConfidence: non-empty copy for every tier`);
  }
  expect(
    new Set(copies).size === 3,
    `describeReorderConfidence: every tier has unique copy (no two reuse the same line)`,
  );
}

{
  // describeReorderConfidence: tier-discriminative phrasing pinned.
  // The UI relies on the substring "Confident" appearing only for
  // the high tier; pin that so a future copy refactor surfaces here.
  expect(
    describeReorderConfidence("high").includes("Confident"),
    `describeReorderConfidence: high includes 'Confident'`,
  );
  expect(
    describeReorderConfidence("medium").includes("Structurally"),
    `describeReorderConfidence: medium includes 'Structurally'`,
  );
  expect(
    describeReorderConfidence("low").includes("Aggressive"),
    `describeReorderConfidence: low includes 'Aggressive'`,
  );
}

{
  // Cross-helper: every proposal from a real planner run is
  // classifiable (no proposal escapes the three-tier union).
  const rules = [
    ruleFor("Catch-all", "always"),
    ruleFor("High", "filename-glob", "h_*"),
    ruleFor("Medium", "filename-glob", "m_*"),
    ruleFor("Low", "filename-glob", "l_*"),
  ];
  const r = healthReport(
    [
      cov(0, "Catch-all", 10, 10, false),
      cov(1, "High", 0, 3, true),
      cov(2, "Medium", 0, 0, true),
      cov(3, "Low", 0, 0, true),
    ],
    0,
    10,
  );
  const proposals = planDeadRuleReorder(rules, r);
  for (const p of proposals) {
    const tier = reorderProposalConfidence(p);
    expect(
      tier === "high" || tier === "medium" || tier === "low",
      `cross-helper: every proposal has valid confidence tier (got ${tier})`,
    );
  }
  // High: index 1 (named shadower + recovered>0)
  // Medium: index 2 (named shadower + recovered=0)
  // Medium: index 3 (named shadower + recovered=0; note: shadower
  //   is "Catch-all" for both 2 and 3 because earliest-Always
  //   is index 0 in both cases)
  expect(
    reorderProposalConfidence(proposals[0]) === "high",
    `cross-helper: proposals[0] is high`,
  );
  expect(
    reorderProposalConfidence(proposals[1]) === "medium",
    `cross-helper: proposals[1] is medium (zero recovered)`,
  );
  expect(
    reorderProposalConfidence(proposals[2]) === "medium",
    `cross-helper: proposals[2] is medium (zero recovered)`,
  );
}

// ─── Slice 139 — applyReorderProposalsBatch / summarize / describeSkip ──

/** Build a proposal directly. Mirrors the batch tests in coverage.rs;
 *  the applier doesn't care which planner produced the proposal so
 *  these tests can hand-roll inputs that exercise edge cases the
 *  planner wouldn't normally emit. */
function proposalFor(
  rule_index: number,
  rule_name: string,
  target_index: number,
  shadowing_rule_name: string,
  samples_recovered: number,
): ReorderProposal {
  return {
    rule_index,
    rule_name,
    target_index,
    shadowing_rule_name,
    samples_recovered,
  };
}

{
  // applyReorderProposalsBatch: empty proposals -> source chain
  // verbatim (per-rule object identity shared with source).
  const rules = [ruleFor("A", "always"), ruleFor("B")];
  const outcome = applyReorderProposalsBatch(rules, []);
  expect(
    outcome.rules.length === 2 && outcome.rules[0] === rules[0] && outcome.rules[1] === rules[1],
    "batch: empty proposals returns source chain verbatim with shared identity",
  );
  expect(outcome.applied.length === 0, "batch: empty proposals -> applied empty");
  expect(outcome.skipped.length === 0, "batch: empty proposals -> skipped empty");
  expect(outcome.total_recovered === 0, "batch: empty proposals -> 0 recovered");
}

{
  // applyReorderProposalsBatch: empty rules -> every proposal
  // skipped as RuleNotFound.
  const proposals = [proposalFor(1, "Tax", 0, "Catch-all", 3)];
  const outcome = applyReorderProposalsBatch([], proposals);
  expect(outcome.rules.length === 0, "batch: empty rules -> empty chain");
  expect(outcome.applied.length === 0, "batch: empty rules -> applied empty");
  expect(outcome.skipped.length === 1, "batch: empty rules -> all skipped");
  expect(
    outcome.skipped[0].reason.kind === "rule_not_found",
    "batch: empty rules -> reason rule_not_found",
  );
  expect(outcome.skipped[0].input_index === 0, "batch: skipped input_index = 0");
}

{
  // applyReorderProposalsBatch: single proposal moves Tax before
  // Catch-all. Classic case mirroring the Rust test.
  const rules = [ruleFor("Catch-all", "always"), ruleFor("Tax")];
  const proposals = [proposalFor(1, "Tax", 0, "Catch-all", 3)];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(outcome.applied.length === 1 && outcome.applied[0] === 0, "batch: single applied");
  expect(outcome.skipped.length === 0, "batch: single -> no skipped");
  expect(outcome.rules[0].name === "Tax", "batch: single -> Tax now first");
  expect(outcome.rules[1].name === "Catch-all", "batch: single -> Catch-all now second");
  expect(outcome.total_recovered === 3, "batch: single -> recovered 3");
}

{
  // applyReorderProposalsBatch: KEY invariant — source resolved by
  // NAME after a prior move. Original [Catch-all, Tax, Receipts];
  // both proposals target_index=0 against the ORIGINAL chain, but
  // after Tax moves, Receipts' rule_index=2 is stale. The applier
  // resolves by name so it lands correctly.
  const rules = [
    ruleFor("Catch-all", "always"),
    ruleFor("Tax", "filename-glob", "t_*"),
    ruleFor("Receipts", "filename-glob", "r_*"),
  ];
  const proposals = [
    proposalFor(1, "Tax", 0, "Catch-all", 3),
    proposalFor(2, "Receipts", 0, "Catch-all", 5),
  ];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(
    outcome.applied.length === 2 && outcome.applied[0] === 0 && outcome.applied[1] === 1,
    "batch: both proposals applied",
  );
  expect(outcome.skipped.length === 0, "batch: no skipped");
  const names = outcome.rules.map((r) => r.name).join(",");
  expect(
    names === "Tax,Receipts,Catch-all",
    `batch: by-name resolution lands correct order (got ${names})`,
  );
  expect(outcome.total_recovered === 8, "batch: total_recovered = 3 + 5");
}

{
  // applyReorderProposalsBatch: rule renamed -> skipped as
  // RuleNotFound, chain unchanged.
  const rules = [ruleFor("All", "always"), ruleFor("Renamed")];
  const proposals = [proposalFor(1, "Tax", 0, "All", 2)];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(outcome.applied.length === 0, "batch: rename -> applied empty");
  expect(outcome.skipped.length === 1, "batch: rename -> skipped 1");
  expect(
    outcome.skipped[0].reason.kind === "rule_not_found",
    "batch: rename -> reason rule_not_found",
  );
  expect(
    outcome.rules.length === 2 && outcome.rules[0].name === "All",
    "batch: rename -> chain unchanged",
  );
}

{
  // applyReorderProposalsBatch: duplicate proposal -> first
  // applies, second skipped as AlreadyEarlier.
  const rules = [ruleFor("A", "always"), ruleFor("B"), ruleFor("C")];
  const proposals = [
    proposalFor(1, "B", 0, "A", 2),
    proposalFor(1, "B", 0, "A", 2),
  ];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(outcome.applied.length === 1 && outcome.applied[0] === 0, "batch: first dup applied");
  expect(outcome.skipped.length === 1, "batch: dup -> one skipped");
  expect(outcome.skipped[0].input_index === 1, "batch: dup skipped is index 1");
  expect(
    outcome.skipped[0].reason.kind === "already_earlier",
    "batch: dup -> reason already_earlier",
  );
}

{
  // applyReorderProposalsBatch: empty shadower name -> fallback to
  // target = 0.
  const rules = [
    ruleFor("First"),
    ruleFor("Second"),
    ruleFor("Dead"),
  ];
  const proposals = [proposalFor(2, "Dead", 0, "", 0)];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(outcome.applied.length === 1, "batch: empty-shadower fallback applies");
  expect(outcome.rules[0].name === "Dead", "batch: empty-shadower -> Dead moved to front");
}

{
  // applyReorderProposalsBatch: shadower drifted out -> fallback
  // to target = 0.
  const rules = [ruleFor("First"), ruleFor("Dead")];
  const proposals = [proposalFor(1, "Dead", 0, "Catch-all-gone", 0)];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(outcome.applied.length === 1, "batch: drifted-shadower fallback applies");
  expect(outcome.rules[0].name === "Dead", "batch: drifted-shadower -> Dead moved to front");
}

{
  // applyReorderProposalsBatch: mixed applied + skipped preserves
  // input order. Three proposals, middle is missing rule.
  const rules = [ruleFor("All", "always"), ruleFor("Tax"), ruleFor("Receipts")];
  const proposals = [
    proposalFor(1, "Tax", 0, "All", 2),
    proposalFor(99, "ghost", 0, "All", 7),
    proposalFor(2, "Receipts", 0, "All", 4),
  ];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(
    outcome.applied.length === 2 && outcome.applied[0] === 0 && outcome.applied[1] === 2,
    "batch: mixed -> applied [0, 2]",
  );
  expect(outcome.skipped.length === 1 && outcome.skipped[0].input_index === 1, "batch: mixed -> skipped index 1");
  expect(outcome.total_recovered === 6, "batch: mixed -> recovered 6 (skipped 7 excluded)");
}

{
  // applyReorderProposalsBatch: conservation invariant —
  // applied.length + skipped.length === proposals.length across a
  // moderately busy mixed batch.
  const rules = [
    ruleFor("All", "always"),
    ruleFor("Tax"),
    ruleFor("Receipts"),
    ruleFor("Invoices"),
  ];
  const proposals = [
    proposalFor(1, "Tax", 0, "All", 2),
    proposalFor(2, "Receipts", 0, "All", 4),
    proposalFor(99, "ghost", 0, "All", 7),
    proposalFor(3, "Invoices", 0, "All", 1),
  ];
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(
    outcome.applied.length + outcome.skipped.length === proposals.length,
    "batch: conservation — applied + skipped == input count",
  );
  // Skipped input_indices strictly monotonic.
  let prev = -1;
  let monotonic = true;
  for (const s of outcome.skipped) {
    if (s.input_index <= prev) {
      monotonic = false;
      break;
    }
    prev = s.input_index;
  }
  expect(monotonic, "batch: skipped input_indices strictly monotonic");
}

{
  // applyReorderProposalsBatch: source array is NOT mutated.
  const rules = [ruleFor("All", "always"), ruleFor("Tax")];
  const snapshotNames = rules.map((r) => r.name).join(",");
  const proposals = [proposalFor(1, "Tax", 0, "All", 3)];
  const _ = applyReorderProposalsBatch(rules, proposals);
  expect(
    rules.map((r) => r.name).join(",") === snapshotNames,
    "batch: source array names unchanged",
  );
  expect(rules.length === 2, "batch: source array length unchanged");
}

{
  // applyReorderProposalsBatch: per-rule object identity is SHARED
  // with the source array (no deep clone). Mirrors slice 134's
  // applyReorderProposal identity contract.
  const all = ruleFor("All", "always");
  const tax = ruleFor("Tax");
  const proposals = [proposalFor(1, "Tax", 0, "All", 3)];
  const outcome = applyReorderProposalsBatch([all, tax], proposals);
  expect(outcome.rules[0] === tax, "batch: identity — tax in outcome === source tax");
  expect(outcome.rules[1] === all, "batch: identity — all in outcome === source all");
}

{
  // summarizeBatchReorderOutcome: empty input -> "No dead rules to fix".
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [],
    skipped: [],
    total_recovered: 0,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) === "No dead rules to fix",
    "summarize: empty -> No dead rules to fix",
  );
}

{
  // summarizeBatchReorderOutcome: all applied + recovered > 0.
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [0, 1, 2],
    skipped: [],
    total_recovered: 12,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) === "Fixed 3 rules — recovered 12 matches",
    `summarize: all applied + 12 recovered (got ${summarizeBatchReorderOutcome(outcome)})`,
  );
}

{
  // summarizeBatchReorderOutcome: all applied + recovered = 0.
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [0, 1],
    skipped: [],
    total_recovered: 0,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) === "Fixed 2 rules",
    "summarize: all applied + 0 recovered -> no match clause",
  );
}

{
  // summarizeBatchReorderOutcome: singular rule + singular match.
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [0],
    skipped: [],
    total_recovered: 1,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) === "Fixed 1 rule — recovered 1 match",
    "summarize: plural-aware — 1 rule + 1 match (singular both)",
  );
}

{
  // summarizeBatchReorderOutcome: partial with recovered > 0.
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [0, 2],
    skipped: [
      {
        input_index: 1,
        proposal: proposalFor(0, "x", 0, "", 0),
        reason: RULE_NOT_FOUND,
      },
    ],
    total_recovered: 5,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) ===
      "Fixed 2 of 3 rules — recovered 5 matches (1 skipped)",
    `summarize: partial + recovered (got ${summarizeBatchReorderOutcome(outcome)})`,
  );
}

{
  // summarizeBatchReorderOutcome: partial with recovered = 0.
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [0],
    skipped: [
      {
        input_index: 1,
        proposal: proposalFor(0, "x", 0, "", 0),
        reason: ALREADY_EARLIER,
      },
    ],
    total_recovered: 0,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) === "Fixed 1 of 2 rules (1 skipped)",
    "summarize: partial + 0 recovered -> no match clause",
  );
}

{
  // summarizeBatchReorderOutcome: nothing applied.
  const outcome: BatchReorderOutcome = {
    rules: [],
    applied: [],
    skipped: [
      {
        input_index: 0,
        proposal: proposalFor(0, "x", 0, "", 0),
        reason: RULE_NOT_FOUND,
      },
      {
        input_index: 1,
        proposal: proposalFor(0, "y", 0, "", 0),
        reason: RULE_NOT_FOUND,
      },
      {
        input_index: 2,
        proposal: proposalFor(0, "z", 0, "", 0),
        reason: RULE_NOT_FOUND,
      },
    ],
    total_recovered: 0,
  };
  expect(
    summarizeBatchReorderOutcome(outcome) === "No rules fixed (3 skipped)",
    "summarize: nothing applied -> No rules fixed",
  );
}

{
  // describeSkipReason: both variants.
  expect(
    describeSkipReason(RULE_NOT_FOUND) === "rule no longer in chain",
    "describeSkipReason: rule_not_found",
  );
  expect(
    describeSkipReason(ALREADY_EARLIER) === "rule already earlier than target",
    "describeSkipReason: already_earlier",
  );
}

{
  // Stable singleton identity — RULE_NOT_FOUND is one object across
  // helper calls.
  expect(
    RULE_NOT_FOUND === RULE_NOT_FOUND,
    "RULE_NOT_FOUND singleton identity",
  );
  expect(
    ALREADY_EARLIER === ALREADY_EARLIER,
    "ALREADY_EARLIER singleton identity",
  );
  expect(
    RULE_NOT_FOUND.kind === "rule_not_found",
    "RULE_NOT_FOUND.kind discriminator",
  );
  expect(
    ALREADY_EARLIER.kind === "already_earlier",
    "ALREADY_EARLIER.kind discriminator",
  );
}

{
  // Cross-helper: planDeadRuleReorder feeding applyReorderProposalsBatch
  // — the canonical end-to-end batch path. Three dead rules + Always
  // shadower at index 0 -> three proposals -> all three applied,
  // dead rules now at the front.
  const rules = [
    ruleFor("Catch-all", "always"),
    ruleFor("Tax", "filename-glob", "t_*"),
    ruleFor("Receipts", "filename-glob", "r_*"),
    ruleFor("Invoices", "filename-glob", "i_*"),
  ];
  const r = healthReport(
    [
      cov(0, "Catch-all", 10, 10, false),
      cov(1, "Tax", 0, 3, true),
      cov(2, "Receipts", 0, 5, true),
      cov(3, "Invoices", 0, 2, true),
    ],
    0,
    10,
  );
  const proposals = planDeadRuleReorder(rules, r);
  expect(proposals.length === 3, "cross-helper: 3 dead -> 3 proposals");
  const outcome = applyReorderProposalsBatch(rules, proposals);
  expect(outcome.applied.length === 3, "cross-helper: all 3 applied");
  expect(outcome.skipped.length === 0, "cross-helper: none skipped");
  // Dead rules now at the front (input order), Catch-all last.
  const names = outcome.rules.map((r) => r.name).join(",");
  expect(
    names === "Tax,Receipts,Invoices,Catch-all",
    `cross-helper: chain order after batch (got ${names})`,
  );
  expect(outcome.total_recovered === 10, "cross-helper: recovered 3 + 5 + 2 = 10");
  expect(
    summarizeBatchReorderOutcome(outcome) === "Fixed 3 rules — recovered 10 matches",
    "cross-helper: summary copy matches",
  );
}

// ─── Slice 140 — slabHopperBatchReorderDeadRules browser-mode wrapper ──
//
// Browser-mode (no Tauri) the wrapper should delegate to the local
// applier verbatim. Pin every BatchReorderOutcome field
// (rules, applied, skipped, total_recovered) on a representative
// mixed batch so a future drift between the wrapper's wire shape
// and the local TS shape surfaces here.

await (async () => {
  const rules = [
    ruleFor("All", "always"),
    ruleFor("Tax", "filename-glob", "t_*"),
    ruleFor("Receipts", "filename-glob", "r_*"),
  ];
  const proposals = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(99, "ghost", 0, "All", 7),
    proposalFor(2, "Receipts", 0, "All", 5),
  ];
  const fromWrapper = await slabHopperBatchReorderDeadRules(rules, proposals);
  const fromLocal = applyReorderProposalsBatch(rules, proposals);
  expect(
    fromWrapper.rules.length === fromLocal.rules.length,
    "slabHopperBatchReorderDeadRules: same rules length",
  );
  expect(
    fromWrapper.rules.map((r) => r.name).join(",") ===
      fromLocal.rules.map((r) => r.name).join(","),
    "slabHopperBatchReorderDeadRules: same rules order by name",
  );
  expect(
    fromWrapper.applied.length === fromLocal.applied.length,
    "slabHopperBatchReorderDeadRules: same applied length",
  );
  expect(
    fromWrapper.applied.every((v, i) => v === fromLocal.applied[i]),
    "slabHopperBatchReorderDeadRules: same applied indices",
  );
  expect(
    fromWrapper.skipped.length === fromLocal.skipped.length,
    "slabHopperBatchReorderDeadRules: same skipped length",
  );
  expect(
    fromWrapper.skipped[0].input_index === fromLocal.skipped[0].input_index,
    "slabHopperBatchReorderDeadRules: same skipped input_index",
  );
  expect(
    fromWrapper.skipped[0].reason.kind === fromLocal.skipped[0].reason.kind,
    "slabHopperBatchReorderDeadRules: same skipped reason.kind",
  );
  expect(
    fromWrapper.skipped[0].proposal.rule_name === fromLocal.skipped[0].proposal.rule_name,
    "slabHopperBatchReorderDeadRules: same skipped proposal.rule_name",
  );
  expect(
    fromWrapper.total_recovered === fromLocal.total_recovered,
    "slabHopperBatchReorderDeadRules: same total_recovered",
  );
})();

await (async () => {
  // Empty proposals path — wrapper still returns a healthy
  // BatchReorderOutcome with the source chain echoed back.
  const rules = [ruleFor("All", "always"), ruleFor("Tax")];
  const outcome = await slabHopperBatchReorderDeadRules(rules, []);
  expect(
    outcome.rules.length === 2 && outcome.rules[0].name === "All",
    "slabHopperBatchReorderDeadRules: empty proposals -> source echoed",
  );
  expect(outcome.applied.length === 0, "slabHopperBatchReorderDeadRules: empty applied");
  expect(outcome.skipped.length === 0, "slabHopperBatchReorderDeadRules: empty skipped");
  expect(outcome.total_recovered === 0, "slabHopperBatchReorderDeadRules: empty recovered");
})();

// ─── Slice 141 — worst / breakdown / describeProposalBatch ──────────

{
  // worstReorderConfidence: empty -> null.
  expect(
    worstReorderConfidence([]) === null,
    "worstReorderConfidence: empty -> null",
  );
}

{
  // worstReorderConfidence: single high.
  const p = proposalFor(1, "Tax", 0, "All", 3);
  expect(
    worstReorderConfidence([p]) === "high",
    "worstReorderConfidence: single high",
  );
}

{
  // worstReorderConfidence: single medium (named shadower, 0 recovered).
  const p = proposalFor(1, "Tax", 0, "All", 0);
  expect(
    worstReorderConfidence([p]) === "medium",
    "worstReorderConfidence: single medium",
  );
}

{
  // worstReorderConfidence: single low (no shadower).
  const p = proposalFor(1, "Tax", 0, "", 5);
  expect(
    worstReorderConfidence([p]) === "low",
    "worstReorderConfidence: single low",
  );
}

{
  // worstReorderConfidence: high + high -> high.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(2, "Receipts", 0, "All", 5),
  ];
  expect(
    worstReorderConfidence(ps) === "high",
    "worstReorderConfidence: all high -> high",
  );
}

{
  // worstReorderConfidence: high + medium -> medium.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(2, "Receipts", 0, "All", 0),
  ];
  expect(
    worstReorderConfidence(ps) === "medium",
    "worstReorderConfidence: high + medium -> medium (worst wins)",
  );
}

{
  // worstReorderConfidence: high + medium + low -> low.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(2, "Receipts", 0, "All", 0),
    proposalFor(3, "Dead", 0, "", 5),
  ];
  expect(
    worstReorderConfidence(ps) === "low",
    "worstReorderConfidence: any low present -> low",
  );
}

{
  // worstReorderConfidence: medium + low -> low.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 0),
    proposalFor(2, "Dead", 0, "", 5),
  ];
  expect(
    worstReorderConfidence(ps) === "low",
    "worstReorderConfidence: medium + low -> low",
  );
}

{
  // worstReorderConfidence: input order independent — same answer
  // regardless of position of the worst proposal.
  const high = proposalFor(1, "Tax", 0, "All", 3);
  const low = proposalFor(2, "Dead", 0, "", 5);
  expect(
    worstReorderConfidence([high, low]) === "low",
    "worstReorderConfidence: low after high -> low",
  );
  expect(
    worstReorderConfidence([low, high]) === "low",
    "worstReorderConfidence: low before high -> low (order-independent)",
  );
}

{
  // summarizeProposalTierBreakdown: empty.
  const b = summarizeProposalTierBreakdown([]);
  expect(b.high === 0 && b.medium === 0 && b.low === 0 && b.total === 0, "tier breakdown: empty -> all zeros");
}

{
  // summarizeProposalTierBreakdown: counts per tier.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3), // high
    proposalFor(2, "Tax2", 0, "All", 3), // high
    proposalFor(3, "Recz", 0, "All", 0), // medium
    proposalFor(4, "Dead1", 0, "", 5), // low
    proposalFor(5, "Dead2", 0, "", 5), // low
  ];
  const b = summarizeProposalTierBreakdown(ps);
  expect(b.high === 2, "tier breakdown: 2 high");
  expect(b.medium === 1, "tier breakdown: 1 medium");
  expect(b.low === 2, "tier breakdown: 2 low");
  expect(b.total === 5, "tier breakdown: total = 5");
  expect(b.total === b.high + b.medium + b.low, "tier breakdown: total invariant");
}

{
  // describeProposalBatch: empty.
  expect(
    describeProposalBatch([]) === "No fixes",
    "describeProposalBatch: empty -> No fixes",
  );
}

{
  // describeProposalBatch: 1 fix, single tier (high).
  expect(
    describeProposalBatch([proposalFor(1, "Tax", 0, "All", 3)]) === "1 fix — high",
    `describeProposalBatch: 1 fix high (got ${describeProposalBatch([proposalFor(1, "Tax", 0, "All", 3)])})`,
  );
}

{
  // describeProposalBatch: 1 fix, single tier (low).
  expect(
    describeProposalBatch([proposalFor(1, "Dead", 0, "", 5)]) === "1 fix — low",
    "describeProposalBatch: 1 fix low",
  );
}

{
  // describeProposalBatch: 2 fixes, one tier -> no enumeration.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(2, "Receipts", 0, "All", 5),
  ];
  expect(
    describeProposalBatch(ps) === "2 fixes — high",
    `describeProposalBatch: 2 fixes one tier (got ${describeProposalBatch(ps)})`,
  );
}

{
  // describeProposalBatch: 2 fixes, two tiers -> enumeration.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(2, "Receipts", 0, "All", 0),
  ];
  expect(
    describeProposalBatch(ps) === "2 fixes — 1 high, 1 medium",
    `describeProposalBatch: 2 fixes two tiers (got ${describeProposalBatch(ps)})`,
  );
}

{
  // describeProposalBatch: 3 fixes, three tiers -> full enumeration.
  const ps = [
    proposalFor(1, "Tax", 0, "All", 3),
    proposalFor(2, "Receipts", 0, "All", 0),
    proposalFor(3, "Dead", 0, "", 5),
  ];
  expect(
    describeProposalBatch(ps) === "3 fixes — 1 high, 1 medium, 1 low",
    `describeProposalBatch: 3 fixes three tiers (got ${describeProposalBatch(ps)})`,
  );
}

{
  // describeProposalBatch: order in the comma list is high > medium > low
  // regardless of input order.
  const ps = [
    proposalFor(1, "Dead", 0, "", 5), // low
    proposalFor(2, "Tax", 0, "All", 3), // high
    proposalFor(3, "Receipts", 0, "All", 0), // medium
  ];
  expect(
    describeProposalBatch(ps) === "3 fixes — 1 high, 1 medium, 1 low",
    `describeProposalBatch: order-independent enumeration (got ${describeProposalBatch(ps)})`,
  );
}

{
  // describeProposalBatch: plural-aware "fix"/"fixes". Already
  // exercised above; pin "1 fix" + "2 fixes" explicitly.
  expect(
    describeProposalBatch([proposalFor(1, "x", 0, "All", 3)]).startsWith("1 fix "),
    "describeProposalBatch: singular -> '1 fix '",
  );
  expect(
    describeProposalBatch([
      proposalFor(1, "x", 0, "All", 3),
      proposalFor(2, "y", 0, "All", 3),
    ]).startsWith("2 fixes "),
    "describeProposalBatch: plural -> '2 fixes '",
  );
}

{
  // Cross-helper: worstReorderConfidence agrees with the
  // per-proposal reorderProposalConfidence under every priority
  // ordering. Build five different combinations and assert the
  // worst-priority element is the worst result.
  const high = proposalFor(1, "Tax", 0, "All", 3);
  const medium = proposalFor(2, "Receipts", 0, "All", 0);
  const low = proposalFor(3, "Dead", 0, "", 5);
  expect(worstReorderConfidence([high]) === "high", "cross-helper worst: only high");
  expect(worstReorderConfidence([medium]) === "medium", "cross-helper worst: only medium");
  expect(worstReorderConfidence([low]) === "low", "cross-helper worst: only low");
  expect(worstReorderConfidence([high, medium]) === "medium", "cross-helper worst: high+medium");
  expect(worstReorderConfidence([medium, low]) === "low", "cross-helper worst: medium+low");
  expect(worstReorderConfidence([high, low]) === "low", "cross-helper worst: high+low");
  expect(
    worstReorderConfidence([high, medium, low]) === "low",
    "cross-helper worst: high+medium+low",
  );
}

// ── Slice 144 — summarizeReorderEffect + describeReorderEffect ───────

function makeRule(name: string): Rule {
  return { name, predicate: { kind: "always" }, action: { recipe_id: null, output_dir: null, rename_pattern: null } };
}

{
  // Empty inputs.
  const empty = summarizeReorderEffect([], []);
  expect(empty.moved.length === 0, "effect: empty inputs -> no moves");
  expect(empty.added.length === 0 && empty.removed.length === 0, "effect: empty inputs -> no added/removed");
  expect(empty.is_permutation === true, "effect: empty inputs -> trivially permutation");
}

{
  // Identity.
  const rules = [makeRule("A"), makeRule("B"), makeRule("C")];
  const effect = summarizeReorderEffect(rules, rules);
  expect(effect.moved.length === 0, "effect: identical chains -> no moves");
  expect(effect.is_permutation === true, "effect: identical chains -> permutation");
}

{
  // Single swap [A,B] -> [B,A].
  const before = [makeRule("A"), makeRule("B")];
  const after = [makeRule("B"), makeRule("A")];
  const effect = summarizeReorderEffect(before, after);
  expect(effect.moved.length === 2, "effect: swap -> two moves");
  expect(effect.moved[0].rule_name === "B", "effect: swap -> first entry in AFTER order is B");
  expect(effect.moved[0].from_index === 1 && effect.moved[0].to_index === 0, "effect: swap -> B 1->0");
  expect(effect.moved[1].rule_name === "A", "effect: swap -> second entry is A");
  expect(effect.moved[1].from_index === 0 && effect.moved[1].to_index === 1, "effect: swap -> A 0->1");
  expect(effect.is_permutation === true, "effect: swap -> permutation");
}

{
  // Lift one rule.
  const before = [makeRule("A"), makeRule("B"), makeRule("C"), makeRule("Dead")];
  const after = [makeRule("Dead"), makeRule("A"), makeRule("B"), makeRule("C")];
  const effect = summarizeReorderEffect(before, after);
  expect(effect.moved.length === 4, "effect: lift -> all four rules moved");
  const names = effect.moved.map((m) => m.rule_name);
  expect(JSON.stringify(names) === JSON.stringify(["Dead", "A", "B", "C"]), "effect: lift -> moved in AFTER order");
  // After-order: to_index strictly ascending.
  for (let i = 0; i < effect.moved.length; i++) {
    expect(effect.moved[i].to_index === i, `effect: lift -> to_index ${i}`);
  }
  expect(effect.is_permutation === true, "effect: lift -> permutation");
}

{
  // Added rule.
  const before = [makeRule("A"), makeRule("B")];
  const after = [makeRule("A"), makeRule("B"), makeRule("C")];
  const effect = summarizeReorderEffect(before, after);
  expect(effect.moved.length === 0, "effect: pure add -> no moves");
  expect(JSON.stringify(effect.added) === JSON.stringify(["C"]), "effect: pure add -> ['C']");
  expect(effect.removed.length === 0, "effect: pure add -> no removed");
  expect(effect.is_permutation === false, "effect: pure add -> NOT a permutation");
}

{
  // Removed rule.
  const before = [makeRule("A"), makeRule("B"), makeRule("C")];
  const after = [makeRule("A"), makeRule("B")];
  const effect = summarizeReorderEffect(before, after);
  expect(effect.moved.length === 0, "effect: pure remove -> no moves");
  expect(effect.added.length === 0, "effect: pure remove -> no added");
  expect(JSON.stringify(effect.removed) === JSON.stringify(["C"]), "effect: pure remove -> ['C']");
  expect(effect.is_permutation === false, "effect: pure remove -> NOT a permutation");
}

{
  // Renamed = add + remove.
  const before = [makeRule("A"), makeRule("B")];
  const after = [makeRule("A"), makeRule("B-renamed")];
  const effect = summarizeReorderEffect(before, after);
  expect(JSON.stringify(effect.added) === JSON.stringify(["B-renamed"]), "effect: rename -> added [B-renamed]");
  expect(JSON.stringify(effect.removed) === JSON.stringify(["B"]), "effect: rename -> removed [B]");
  expect(effect.is_permutation === false, "effect: rename -> NOT a permutation");
}

{
  // Source not mutated.
  const before = [makeRule("A"), makeRule("B")];
  const after = [makeRule("B"), makeRule("A")];
  const beforeSnap = JSON.stringify(before);
  const afterSnap = JSON.stringify(after);
  summarizeReorderEffect(before, after);
  expect(JSON.stringify(before) === beforeSnap, "effect: BEFORE array not mutated");
  expect(JSON.stringify(after) === afterSnap, "effect: AFTER array not mutated");
}

{
  // Composes with applyReorderProposalsBatch end-to-end.
  const before = [makeRule("All"), makeRule("Tax"), makeRule("Receipts")];
  const props: ReorderProposal[] = [
    { rule_index: 1, rule_name: "Tax", target_index: 0, shadowing_rule_name: "All", samples_recovered: 2 },
    { rule_index: 2, rule_name: "Receipts", target_index: 0, shadowing_rule_name: "All", samples_recovered: 4 },
  ];
  const outcome = applyReorderProposalsBatch(before, props);
  const effect = summarizeReorderEffect(before, outcome.rules);
  expect(effect.moved.length === 3, "effect: cross-helper -> all three rules moved");
  expect(effect.is_permutation === true, "effect: cross-helper -> permutation");
  // Undo round-trip: feed the BEFORE back, summarize against AFTER.
  const undo = summarizeReorderEffect(outcome.rules, before);
  expect(undo.is_permutation === true, "effect: undo round-trip -> permutation");
  // Inverse: each moved entry now points from its reordered position
  // back to its original position.
  for (const m of undo.moved) {
    const origPos = before.findIndex((r) => r.name === m.rule_name);
    const reordPos = outcome.rules.findIndex((r) => r.name === m.rule_name);
    expect(m.from_index === reordPos && m.to_index === origPos, `effect: undo round-trip ${m.rule_name} reverses positions`);
  }
}

{
  // Duplicate-name canonical first-occurrence handling.
  const before = [makeRule("A"), makeRule("Dup"), makeRule("Dup")];
  const after = [makeRule("Dup"), makeRule("A"), makeRule("Dup")];
  const effect = summarizeReorderEffect(before, after);
  const names = effect.moved.map((m) => m.rule_name);
  expect(JSON.stringify(names) === JSON.stringify(["Dup", "A"]), "effect: duplicate-name first-occurrence canonical");
}

// ── describeReorderEffect ───────────────────────────────────────────

{
  const noop: ReorderEffect = { moved: [], added: [], removed: [], is_permutation: true };
  expect(describeReorderEffect(noop) === "No changes to undo", "describe: noop");
}

{
  const oneMove: ReorderEffect = {
    moved: [{ rule_name: "Tax", from_index: 3, to_index: 0 }],
    added: [],
    removed: [],
    is_permutation: true,
  };
  expect(describeReorderEffect(oneMove) === "Move 1 rule back", "describe: 1 rule");
}

{
  const threeMoves: ReorderEffect = {
    moved: [
      { rule_name: "Tax", from_index: 3, to_index: 0 },
      { rule_name: "Receipts", from_index: 4, to_index: 1 },
      { rule_name: "All", from_index: 0, to_index: 2 },
    ],
    added: [],
    removed: [],
    is_permutation: true,
  };
  expect(describeReorderEffect(threeMoves) === "Move 3 rules back", "describe: 3 rules");
}

{
  const onlyAdded: ReorderEffect = {
    moved: [],
    added: ["NewRule"],
    removed: [],
    is_permutation: false,
  };
  expect(describeReorderEffect(onlyAdded) === "Drop 1 added rule", "describe: only added 1");
  const moreAdded: ReorderEffect = { ...onlyAdded, added: ["A", "B"] };
  expect(describeReorderEffect(moreAdded) === "Drop 2 added rules", "describe: only added plural");
}

{
  const onlyRemoved: ReorderEffect = {
    moved: [],
    added: [],
    removed: ["GoneRule"],
    is_permutation: false,
  };
  expect(describeReorderEffect(onlyRemoved) === "Restore 1 removed rule", "describe: only removed 1");
  const moreRemoved: ReorderEffect = { ...onlyRemoved, removed: ["A", "B"] };
  expect(describeReorderEffect(moreRemoved) === "Restore 2 removed rules", "describe: only removed plural");
}

{
  const mixed: ReorderEffect = {
    moved: [{ rule_name: "Tax", from_index: 3, to_index: 0 }],
    added: ["NewRule"],
    removed: ["GoneRule"],
    is_permutation: false,
  };
  expect(describeReorderEffect(mixed) === "Move 1, restore 1 removed, drop 1 added", "describe: mixed move + add + remove");
}

{
  const addAndRemove: ReorderEffect = {
    moved: [],
    added: ["NewRule"],
    removed: ["GoneRule"],
    is_permutation: false,
  };
  expect(describeReorderEffect(addAndRemove) === "restore 1 removed, drop 1 added", "describe: only add + remove (no moves)");
}

// ── isReorderEffectNoop ─────────────────────────────────────────────

{
  expect(
    isReorderEffectNoop({ moved: [], added: [], removed: [], is_permutation: true }) === true,
    "noop: empty permutation",
  );
  expect(
    isReorderEffectNoop({ moved: [], added: [], removed: [], is_permutation: false }) === true,
    "noop: empty NOT-permutation (treated as no-op too — buckets are what matter)",
  );
  expect(
    isReorderEffectNoop({
      moved: [{ rule_name: "Tax", from_index: 3, to_index: 0 }],
      added: [],
      removed: [],
      is_permutation: true,
    }) === false,
    "noop: one move -> NOT noop",
  );
  expect(
    isReorderEffectNoop({ moved: [], added: ["X"], removed: [], is_permutation: false }) === false,
    "noop: one added -> NOT noop",
  );
  expect(
    isReorderEffectNoop({ moved: [], added: [], removed: ["X"], is_permutation: false }) === false,
    "noop: one removed -> NOT noop",
  );
}

// ── Slice 145 — slabHopperSummarizeReorderEffect wrapper-delegation ──

import { slabHopperSummarizeReorderEffect } from "./hopper";

{
  // In browser-mode (no Tauri global), the wrapper delegates to the
  // local TS mirror — exercise that path and pin every ReorderEffect
  // field round-trips correctly.
  const before = [makeRule("All"), makeRule("Tax"), makeRule("Receipts")];
  const after = [makeRule("Tax"), makeRule("Receipts"), makeRule("All")];
  const effect = await slabHopperSummarizeReorderEffect(before, after);
  expect(Array.isArray(effect.moved), "wrapper: moved is an array");
  expect(effect.moved.length === 3, "wrapper: moved.length === 3");
  expect(typeof effect.moved[0].rule_name === "string", "wrapper: moved[0].rule_name is string");
  expect(typeof effect.moved[0].from_index === "number", "wrapper: moved[0].from_index is number");
  expect(typeof effect.moved[0].to_index === "number", "wrapper: moved[0].to_index is number");
  expect(Array.isArray(effect.added) && effect.added.length === 0, "wrapper: added is empty array");
  expect(Array.isArray(effect.removed) && effect.removed.length === 0, "wrapper: removed is empty array");
  expect(effect.is_permutation === true, "wrapper: is_permutation true for permutation");
}

{
  // Browser-mode delegation: identical chains -> no-op effect.
  const rules = [makeRule("A"), makeRule("B")];
  const effect = await slabHopperSummarizeReorderEffect(rules, rules);
  expect(effect.moved.length === 0, "wrapper: identical -> no moves");
  expect(effect.added.length === 0, "wrapper: identical -> no added");
  expect(effect.removed.length === 0, "wrapper: identical -> no removed");
  expect(effect.is_permutation === true, "wrapper: identical -> permutation");
}

{
  // Browser-mode delegation: not-a-permutation pinned through.
  const before = [makeRule("A"), makeRule("B")];
  const after = [makeRule("A"), makeRule("B"), makeRule("C")];
  const effect = await slabHopperSummarizeReorderEffect(before, after);
  expect(effect.is_permutation === false, "wrapper: pure-add not a permutation");
  expect(JSON.stringify(effect.added) === JSON.stringify(["C"]), "wrapper: added=[C]");
  expect(effect.removed.length === 0, "wrapper: removed empty");
}

{
  // Browser-mode delegation: removed bucket pinned.
  const before = [makeRule("A"), makeRule("B"), makeRule("C")];
  const after = [makeRule("A"), makeRule("B")];
  const effect = await slabHopperSummarizeReorderEffect(before, after);
  expect(JSON.stringify(effect.removed) === JSON.stringify(["C"]), "wrapper: removed=[C]");
  expect(effect.added.length === 0, "wrapper: added empty");
  expect(effect.is_permutation === false, "wrapper: pure-remove not a permutation");
}

{
  // Browser-mode delegation: empty inputs round-trip.
  const effect = await slabHopperSummarizeReorderEffect([], []);
  expect(effect.moved.length === 0 && effect.added.length === 0 && effect.removed.length === 0, "wrapper: empty round-trip");
  expect(effect.is_permutation === true, "wrapper: empty trivially permutation");
}

// ── Slice 146 — captureUndoEntry / computeUndoStatus / describeUndoStatus ──

import {
  captureUndoEntry,
  computeUndoStatus,
  describeUndoStatus,
  type ReorderUndoEntry,
  type ReorderUndoStatus,
} from "./hopper";

{
  // captureUndoEntry copies the snapshot defensively.
  const before = [makeRule("A"), makeRule("B")];
  const after = [makeRule("B"), makeRule("A")];
  const entry = captureUndoEntry(before, after, "fix-it: Tax", 1700000000000);
  expect(entry.label === "fix-it: Tax", "capture: label round-trips");
  expect(entry.capturedAt === 1700000000000, "capture: capturedAt round-trips");
  expect(entry.snapshot.length === 2 && entry.snapshot[0].name === "A", "capture: snapshot length + content");
  // Mutating the source after capture must not affect the entry.
  before[0] = makeRule("Mutated");
  expect(entry.snapshot[0].name === "A", "capture: snapshot is defensively copied");
  // Applied effect is pre-computed.
  expect(entry.appliedEffect.moved.length === 2, "capture: appliedEffect has correct moves");
  expect(entry.appliedEffect.is_permutation === true, "capture: appliedEffect.is_permutation");
}

{
  // captureUndoEntry: now defaults to Date.now() when not passed.
  const before = [makeRule("A")];
  const after = [makeRule("A")];
  const start = Date.now();
  const entry = captureUndoEntry(before, after, "test");
  const end = Date.now();
  expect(entry.capturedAt >= start && entry.capturedAt <= end, "capture: default now is current time");
}

{
  // computeUndoStatus: chain matches snapshot exactly -> noop.
  const snapshot = [makeRule("A"), makeRule("B"), makeRule("C")];
  const entry: ReorderUndoEntry = {
    snapshot,
    label: "fix-all",
    capturedAt: 0,
    appliedEffect: { moved: [], added: [], removed: [], is_permutation: true },
  };
  const status = computeUndoStatus(entry, snapshot);
  expect(status.kind === "noop", "status: snapshot === current -> noop");
}

{
  // computeUndoStatus: chain is a clean permutation of snapshot -> ready.
  // Apply path was [All, Tax, Receipts] -> [Tax, Receipts, All]; the
  // entry's snapshot is the BEFORE chain.
  const snapshot = [makeRule("All"), makeRule("Tax"), makeRule("Receipts")];
  const current = [makeRule("Tax"), makeRule("Receipts"), makeRule("All")];
  const entry = captureUndoEntry(snapshot, current, "fix-all", 0);
  const status = computeUndoStatus(entry, current);
  expect(status.kind === "ready", "status: permutation -> ready");
  if (status.kind === "ready") {
    expect(status.effect.is_permutation === true, "status: ready effect is permutation");
    expect(status.effect.moved.length === 3, "status: ready effect has 3 moves");
  }
}

{
  // computeUndoStatus: user ADDED a rule since the apply -> stale.
  const snapshot = [makeRule("All"), makeRule("Tax")];
  const current = [makeRule("Tax"), makeRule("All"), makeRule("NewRule")];
  const entry = captureUndoEntry(snapshot, [makeRule("Tax"), makeRule("All")], "fix-it", 0);
  const status = computeUndoStatus(entry, current);
  expect(status.kind === "stale", "status: added rule -> stale");
  if (status.kind === "stale") {
    expect(status.reason.includes("added"), "status: stale reason includes 'added'");
    expect(status.reason.includes("fix-it"), "status: stale reason includes label");
    expect(status.reason.includes("1 rule"), "status: stale reason includes correct singular count");
  }
}

{
  // computeUndoStatus: user REMOVED a rule since the apply -> stale.
  const snapshot = [makeRule("All"), makeRule("Tax"), makeRule("Receipts")];
  const current = [makeRule("Tax"), makeRule("All")];
  const entry = captureUndoEntry(snapshot, [makeRule("Tax"), makeRule("All"), makeRule("Receipts")], "fix-all", 0);
  const status = computeUndoStatus(entry, current);
  expect(status.kind === "stale", "status: removed rule -> stale");
  if (status.kind === "stale") {
    expect(status.reason.includes("removed"), "status: stale reason includes 'removed'");
    expect(status.reason.includes("1 rule"), "status: stale reason singular");
  }
}

{
  // computeUndoStatus: user ADDED multiple rules -> plural reason.
  const snapshot = [makeRule("A"), makeRule("B")];
  const current = [makeRule("B"), makeRule("A"), makeRule("X"), makeRule("Y")];
  const entry = captureUndoEntry(snapshot, [makeRule("B"), makeRule("A")], "fix-it", 0);
  const status = computeUndoStatus(entry, current);
  expect(status.kind === "stale", "status: multi-add -> stale");
  if (status.kind === "stale") {
    expect(status.reason.includes("2 rules"), "status: plural rules count");
  }
}

{
  // computeUndoStatus: renamed rule (equal add + remove) -> "renamed" framing.
  const snapshot = [makeRule("A"), makeRule("B"), makeRule("C")];
  const current = [makeRule("A"), makeRule("B-renamed"), makeRule("C")];
  const entry = captureUndoEntry(snapshot, [makeRule("A"), makeRule("B"), makeRule("C")], "fix-it", 0);
  const status = computeUndoStatus(entry, current);
  expect(status.kind === "stale", "status: rename -> stale");
  if (status.kind === "stale") {
    expect(status.reason.includes("renamed"), "status: rename -> 'renamed' framing");
  }
}

{
  // describeUndoStatus: noop branch.
  const status: ReorderUndoStatus = { kind: "noop" };
  expect(describeUndoStatus(status) === "Nothing to undo", "describeUndoStatus: noop");
}

{
  // describeUndoStatus: stale branch.
  const status: ReorderUndoStatus = {
    kind: "stale",
    reason: "1 rule added since fix-it",
    effect: { moved: [], added: ["X"], removed: [], is_permutation: false },
  };
  expect(
    describeUndoStatus(status) === "Undo unavailable — 1 rule added since fix-it",
    "describeUndoStatus: stale",
  );
}

{
  // describeUndoStatus: ready branch.
  const status: ReorderUndoStatus = {
    kind: "ready",
    effect: {
      moved: [
        { rule_name: "A", from_index: 1, to_index: 0 },
        { rule_name: "B", from_index: 0, to_index: 1 },
      ],
      added: [],
      removed: [],
      is_permutation: true,
    },
  };
  expect(describeUndoStatus(status) === "Undo · Move 2 rules back", "describeUndoStatus: ready");
}

{
  // describeUndoStatus: ready singular.
  const status: ReorderUndoStatus = {
    kind: "ready",
    effect: {
      moved: [{ rule_name: "A", from_index: 1, to_index: 0 }],
      added: [],
      removed: [],
      is_permutation: true,
    },
  };
  expect(
    describeUndoStatus(status) === "Undo · Move 1 rule back",
    "describeUndoStatus: ready singular",
  );
}

{
  // End-to-end: capture -> compute -> describe a fix-all undo round.
  const original = [makeRule("All"), makeRule("Tax"), makeRule("Receipts")];
  const props: ReorderProposal[] = [
    { rule_index: 1, rule_name: "Tax", target_index: 0, shadowing_rule_name: "All", samples_recovered: 2 },
    { rule_index: 2, rule_name: "Receipts", target_index: 0, shadowing_rule_name: "All", samples_recovered: 4 },
  ];
  const outcome = applyReorderProposalsBatch(original, props);
  const entry = captureUndoEntry(original, outcome.rules, "fix-all", 1700000000000);
  // No subsequent edit -> ready.
  const ready = computeUndoStatus(entry, outcome.rules);
  expect(ready.kind === "ready", "end-to-end: post-apply -> ready");
  expect(describeUndoStatus(ready).startsWith("Undo · Move "), "end-to-end: ready copy");
  // After undoing (= snapshot back) -> noop.
  const afterUndo = computeUndoStatus(entry, original);
  expect(afterUndo.kind === "noop", "end-to-end: post-undo -> noop");
}

// ── Slice 149 — summarizeUndoRing / describeUndoRingSummary / isUndoRingFull ──

import {
  summarizeUndoRing,
  describeUndoRingSummary,
  isUndoRingFull,
  type UndoEntrySummary,
  type UndoRingSummary,
} from "./hopper";

function ringEntry(label: string, ms: number): UndoEntrySummary {
  return {
    label,
    captured_at_ms: ms,
    applied_effect: {
      moved: [{ rule_name: label, from_index: 1, to_index: 0 }],
      added: [],
      removed: [],
      is_permutation: true,
    },
  };
}

{
  // Empty ring under capacity -> empty entries, not full.
  const summary = summarizeUndoRing([], 5);
  expect(summary.entries.length === 0, "ring: empty entries");
  expect(summary.capacity === 5, "ring: capacity round-trip");
  expect(summary.full === false, "ring: empty -> not full");
}

{
  // One entry under capacity -> pass-through.
  const entry = ringEntry("fix-it: Tax", 1000);
  const summary = summarizeUndoRing([entry], 5);
  expect(summary.entries.length === 1, "ring: single entry length");
  expect(summary.entries[0].label === "fix-it: Tax", "ring: label round-trip");
  expect(summary.entries[0].captured_at_ms === 1000, "ring: ts round-trip");
  expect(summary.full === false, "ring: single under cap -> not full");
}

{
  // At capacity -> full.
  const entries = [ringEntry("a", 1), ringEntry("b", 2), ringEntry("c", 3)];
  const summary = summarizeUndoRing(entries, 3);
  expect(summary.entries.length === 3, "ring: at-capacity length");
  expect(summary.full === true, "ring: at-capacity -> full");
  expect(summary.entries[0].label === "a", "ring: oldest first preserved");
  expect(summary.entries[2].label === "c", "ring: newest last preserved");
}

{
  // Over capacity -> trim oldest, keep most-recent.
  const entries = [
    ringEntry("a", 1),
    ringEntry("b", 2),
    ringEntry("c", 3),
    ringEntry("d", 4),
    ringEntry("e", 5),
    ringEntry("f", 6),
    ringEntry("g", 7),
  ];
  const summary = summarizeUndoRing(entries, 5);
  expect(summary.entries.length === 5, "ring: over-cap trimmed to 5");
  expect(summary.full === true, "ring: trimmed -> full");
  expect(summary.entries[0].label === "c", "ring: oldest kept is c");
  expect(summary.entries[4].label === "g", "ring: newest kept is g");
  expect(
    !summary.entries.some((e) => e.label === "a" || e.label === "b"),
    "ring: a/b dropped",
  );
}

{
  // Capacity 0 -> always full, no entries (defensive).
  const entries = [ringEntry("a", 1), ringEntry("b", 2)];
  const summary = summarizeUndoRing(entries, 0);
  expect(summary.entries.length === 0, "ring: cap=0 empty");
  expect(summary.capacity === 0, "ring: cap=0 round-trip");
  expect(summary.full === true, "ring: cap=0 always full");
}

{
  // Negative capacity -> same as 0 (defensive — UI bug guard).
  const summary = summarizeUndoRing([ringEntry("a", 1)], -3);
  expect(summary.entries.length === 0, "ring: negative cap empty");
  expect(summary.capacity === 0, "ring: negative cap normalised to 0");
  expect(summary.full === true, "ring: negative cap always full");
}

{
  // Capacity 1 -> keeps only newest.
  const entries = [ringEntry("old", 100), ringEntry("new", 200)];
  const summary = summarizeUndoRing(entries, 1);
  expect(summary.entries.length === 1, "ring: cap=1 single entry");
  expect(summary.entries[0].label === "new", "ring: cap=1 keeps newest");
  expect(summary.full === true, "ring: cap=1 single entry -> full");
}

{
  // Pinned: input array NOT mutated by summarise.
  const entries = [ringEntry("a", 1), ringEntry("b", 2), ringEntry("c", 3)];
  const before = entries.length;
  const snapshotLabels = entries.map((e) => e.label).join(",");
  summarizeUndoRing(entries, 1);
  expect(entries.length === before, "ring: input length not mutated");
  expect(entries.map((e) => e.label).join(",") === snapshotLabels, "ring: input labels not mutated");
}

{
  // describeUndoRingSummary: empty.
  const summary: UndoRingSummary = { entries: [], capacity: 5, full: false };
  expect(describeUndoRingSummary(summary) === "No undo history", "describe: empty");
}

{
  // describeUndoRingSummary: 1 entry under cap -> no "oldest:" suffix.
  const summary = summarizeUndoRing([ringEntry("fix-all", 1)], 5);
  expect(describeUndoRingSummary(summary) === "1 undo step", "describe: 1 entry");
}

{
  // describeUndoRingSummary: 3 entries under cap -> "oldest:" suffix.
  const summary = summarizeUndoRing(
    [ringEntry("fix-all", 1), ringEntry("fix-it: Tax", 2), ringEntry("fix-it: Rent", 3)],
    5,
  );
  expect(
    describeUndoRingSummary(summary) === "3 undo steps (oldest: fix-all)",
    "describe: multi entries with oldest label",
  );
}

{
  // describeUndoRingSummary: at capacity -> "at capacity" copy.
  const summary = summarizeUndoRing(
    [
      ringEntry("a", 1),
      ringEntry("b", 2),
      ringEntry("c", 3),
      ringEntry("d", 4),
      ringEntry("e", 5),
    ],
    5,
  );
  expect(
    describeUndoRingSummary(summary) === "5 undo steps — at capacity",
    "describe: at-capacity",
  );
}

{
  // describeUndoRingSummary: cap=1 single entry IS full -> at-capacity copy.
  const summary = summarizeUndoRing([ringEntry("a", 1)], 1);
  expect(
    describeUndoRingSummary(summary) === "1 undo step — at capacity",
    "describe: cap=1 single full",
  );
}

{
  // isUndoRingFull predicate matches the .full flag.
  const empty = summarizeUndoRing([], 5);
  expect(isUndoRingFull(empty) === false, "isFull: empty");
  const full = summarizeUndoRing([ringEntry("a", 1), ringEntry("b", 2)], 2);
  expect(isUndoRingFull(full) === true, "isFull: full");
  const cap0 = summarizeUndoRing([], 0);
  expect(isUndoRingFull(cap0) === true, "isFull: cap=0 always full");
}

{
  // End-to-end: ring fills, then a subsequent push trims oldest.
  // Mirrors the UI's pushUndoEntry pattern (slice 151).
  let ring: UndoEntrySummary[] = [];
  const capacity = 3;
  for (const ts of [1, 2, 3, 4, 5]) {
    ring.push(ringEntry(`step-${ts}`, ts));
    // UI would call slice 151 trimmer here; simulate via summariser.
    const summary = summarizeUndoRing(ring, capacity);
    ring = summary.entries.slice();
  }
  expect(ring.length === 3, "e2e: ring stabilises at capacity");
  expect(ring[0].label === "step-3", "e2e: oldest is step-3 after eviction");
  expect(ring[2].label === "step-5", "e2e: newest is step-5");
}

{
  // Pinned: serialising and re-parsing summary preserves snake_case.
  // This is the contract that lets the Tauri command (slice 150) and
  // the TS mirror agree on the wire shape.
  const summary = summarizeUndoRing([ringEntry("fix-all", 1700000000000)], 5);
  const json = JSON.stringify(summary);
  expect(json.includes('"captured_at_ms":1700000000000'), "wire: snake_case captured_at_ms");
  expect(json.includes('"applied_effect":'), "wire: snake_case applied_effect");
  expect(json.includes('"capacity":5'), "wire: capacity field");
  expect(json.includes('"full":false'), "wire: full field");
}

// ── Slice 150 — slabHopperSummarizeUndoRing wrapper-delegation ─────

import { slabHopperSummarizeUndoRing } from "./hopper";

{
  // Browser-mode delegation: empty entries pass-through.
  const summary = await slabHopperSummarizeUndoRing([], 5);
  expect(Array.isArray(summary.entries), "wrapper: entries is array");
  expect(summary.entries.length === 0, "wrapper: empty entries");
  expect(summary.capacity === 5, "wrapper: capacity round-trip");
  expect(summary.full === false, "wrapper: empty not full");
}

{
  // Browser-mode delegation: under-capacity ring passes through.
  const entries = [ringEntry("a", 1), ringEntry("b", 2)];
  const summary = await slabHopperSummarizeUndoRing(entries, 5);
  expect(summary.entries.length === 2, "wrapper: under-cap length");
  expect(summary.entries[0].label === "a", "wrapper: label[0]");
  expect(summary.entries[1].label === "b", "wrapper: label[1]");
  expect(summary.full === false, "wrapper: under-cap not full");
}

{
  // Browser-mode delegation: at-capacity ring marked full.
  const entries = [ringEntry("a", 1), ringEntry("b", 2), ringEntry("c", 3)];
  const summary = await slabHopperSummarizeUndoRing(entries, 3);
  expect(summary.entries.length === 3, "wrapper: at-cap length");
  expect(summary.full === true, "wrapper: at-cap full");
  expect(summary.capacity === 3, "wrapper: capacity round-trip");
}

{
  // Browser-mode delegation: over-capacity trims oldest.
  const entries = [
    ringEntry("a", 1),
    ringEntry("b", 2),
    ringEntry("c", 3),
    ringEntry("d", 4),
    ringEntry("e", 5),
    ringEntry("f", 6),
  ];
  const summary = await slabHopperSummarizeUndoRing(entries, 4);
  expect(summary.entries.length === 4, "wrapper: trimmed to 4");
  expect(summary.entries[0].label === "c", "wrapper: oldest after trim is c");
  expect(summary.entries[3].label === "f", "wrapper: newest is f");
  expect(summary.full === true, "wrapper: trimmed -> full");
}

{
  // Browser-mode delegation: pins every UndoEntrySummary field
  // round-trips through the wrapper path.
  const entry = ringEntry("fix-it: Tax", 1700000000000);
  const summary = await slabHopperSummarizeUndoRing([entry], 5);
  expect(summary.entries[0].label === "fix-it: Tax", "wrapper: label pinned");
  expect(summary.entries[0].captured_at_ms === 1700000000000, "wrapper: captured_at_ms pinned");
  expect(summary.entries[0].applied_effect.moved.length === 1, "wrapper: applied_effect.moved pinned");
  expect(summary.entries[0].applied_effect.is_permutation === true, "wrapper: applied_effect.is_permutation pinned");
}

{
  // Browser-mode delegation: capacity 0 -> always full, empty.
  const summary = await slabHopperSummarizeUndoRing([ringEntry("a", 1)], 0);
  expect(summary.entries.length === 0, "wrapper: cap=0 empty");
  expect(summary.full === true, "wrapper: cap=0 full");
  expect(summary.capacity === 0, "wrapper: cap=0 round-trip");
}

// ── Slice 151 — pushUndoEntry / popUndoEntry / selectActiveUndo ──

import {
  pushUndoEntry,
  popUndoEntry,
  selectActiveUndo,
  UNDO_RING_CAPACITY,
} from "./hopper";

function liveEntry(label: string, snapshot: Rule[], ts: number = 0): ReorderUndoEntry {
  return {
    snapshot: snapshot.slice(),
    label,
    capturedAt: ts,
    appliedEffect: { moved: [], added: [], removed: [], is_permutation: true },
  };
}

{
  // UNDO_RING_CAPACITY constant pinned at 5.
  expect(UNDO_RING_CAPACITY === 5, "constant: UNDO_RING_CAPACITY === 5");
}

{
  // pushUndoEntry: empty ring + first entry -> single-entry array.
  const e1 = liveEntry("a", [makeRule("A")]);
  const ring = pushUndoEntry([], e1, 5);
  expect(ring.length === 1, "push: empty + push -> length 1");
  expect(ring[0] === e1, "push: entry reference preserved");
}

{
  // pushUndoEntry: under capacity -> append.
  const e1 = liveEntry("a", [makeRule("A")]);
  const e2 = liveEntry("b", [makeRule("B")]);
  const ring = pushUndoEntry([e1], e2, 5);
  expect(ring.length === 2, "push: under-cap appended");
  expect(ring[0].label === "a" && ring[1].label === "b", "push: order preserved");
}

{
  // pushUndoEntry: at capacity -> drop oldest, append new.
  const e1 = liveEntry("a", [makeRule("A")]);
  const e2 = liveEntry("b", [makeRule("B")]);
  const e3 = liveEntry("c", [makeRule("C")]);
  const e4 = liveEntry("d", [makeRule("D")]);
  const ring = pushUndoEntry([e1, e2, e3], e4, 3);
  expect(ring.length === 3, "push: at-cap stays at capacity");
  expect(ring[0].label === "b", "push: oldest (a) evicted");
  expect(ring[2].label === "d", "push: new entry is newest");
}

{
  // pushUndoEntry: capacity 0 -> empty.
  const ring = pushUndoEntry([], liveEntry("a", [makeRule("A")]), 0);
  expect(ring.length === 0, "push: cap=0 -> empty");
}

{
  // pushUndoEntry: negative capacity -> empty (defensive).
  const ring = pushUndoEntry([liveEntry("a", [makeRule("A")])], liveEntry("b", [makeRule("B")]), -1);
  expect(ring.length === 0, "push: cap<0 -> empty");
}

{
  // pushUndoEntry: input ring NOT mutated.
  const e1 = liveEntry("a", [makeRule("A")]);
  const e2 = liveEntry("b", [makeRule("B")]);
  const input = [e1];
  pushUndoEntry(input, e2, 5);
  expect(input.length === 1, "push: input ring untouched");
  expect(input[0] === e1, "push: input ring identity preserved");
}

{
  // pushUndoEntry: simulating cascade fills then trims oldest.
  let ring: ReorderUndoEntry[] = [];
  for (let i = 1; i <= 7; i++) {
    ring = pushUndoEntry(ring, liveEntry(`step-${i}`, [makeRule(`R${i}`)]), 5);
  }
  expect(ring.length === 5, "push e2e: ring stable at 5");
  expect(ring[0].label === "step-3", "push e2e: oldest after trim is step-3");
  expect(ring[4].label === "step-7", "push e2e: newest is step-7");
}

{
  // popUndoEntry: empty -> { entry: null, remaining: [] }.
  const result = popUndoEntry([]);
  expect(result.entry === null, "pop: empty -> null entry");
  expect(result.remaining.length === 0, "pop: empty -> empty remaining");
}

{
  // popUndoEntry: single -> entry + empty remaining.
  const e1 = liveEntry("a", [makeRule("A")]);
  const result = popUndoEntry([e1]);
  expect(result.entry === e1, "pop: single -> e1 returned");
  expect(result.remaining.length === 0, "pop: single -> empty remaining");
}

{
  // popUndoEntry: multi -> newest entry + rest.
  const e1 = liveEntry("a", [makeRule("A")]);
  const e2 = liveEntry("b", [makeRule("B")]);
  const e3 = liveEntry("c", [makeRule("C")]);
  const result = popUndoEntry([e1, e2, e3]);
  expect(result.entry === e3, "pop: multi -> newest popped");
  expect(result.remaining.length === 2, "pop: remaining length");
  expect(result.remaining[0] === e1 && result.remaining[1] === e2, "pop: remaining order");
}

{
  // popUndoEntry: input array NOT mutated.
  const e1 = liveEntry("a", [makeRule("A")]);
  const e2 = liveEntry("b", [makeRule("B")]);
  const input = [e1, e2];
  popUndoEntry(input);
  expect(input.length === 2, "pop: input length untouched");
  expect(input[0] === e1 && input[1] === e2, "pop: input identity preserved");
}

{
  // selectActiveUndo: empty ring -> null active + zero counters.
  const sel = selectActiveUndo([], [makeRule("A")]);
  expect(sel.active === null, "select: empty -> null active");
  expect(sel.totalEntries === 0, "select: empty -> 0 entries");
  expect(sel.totalReady === 0, "select: empty -> 0 ready");
  expect(sel.totalStale === 0, "select: empty -> 0 stale");
}

{
  // selectActiveUndo: single ready entry -> surfaced as active.
  const original = [makeRule("All"), makeRule("Tax")];
  const reordered = [makeRule("Tax"), makeRule("All")];
  const entry = captureUndoEntry(original, reordered, "fix-it: Tax", 0);
  const sel = selectActiveUndo([entry], reordered);
  expect(sel.active !== null, "select: single ready -> active not null");
  if (sel.active !== null) {
    expect(sel.active.status.kind === "ready", "select: single ready -> ready status");
    expect(sel.active.index === 0, "select: single -> index 0");
  }
  expect(sel.totalEntries === 1, "select: single -> totalEntries 1");
  expect(sel.totalReady === 1, "select: single ready -> totalReady 1");
  expect(sel.totalStale === 0, "select: single ready -> totalStale 0");
}

{
  // selectActiveUndo: multi ready -> walks newest first, picks
  // the newest ready entry.
  const original1 = [makeRule("A"), makeRule("B")];
  const reordered1 = [makeRule("B"), makeRule("A")];
  const entry1 = captureUndoEntry(original1, reordered1, "fix-it: 1", 0);
  // After undoing entry1 in our heads, the chain is back to original1.
  // Now apply another reorder.
  const reordered2 = [makeRule("A"), makeRule("B")];  // identity for simplicity
  const original2 = [makeRule("B"), makeRule("A")];
  const entry2 = captureUndoEntry(original2, reordered2, "fix-it: 2", 0);
  // Live chain matches entry2's snapshot's reversed state == reordered2.
  const sel = selectActiveUndo([entry1, entry2], reordered2);
  expect(sel.active !== null, "select: multi -> active not null");
  if (sel.active !== null) {
    // Newest (index 1) is the natural target.
    expect(sel.active.index === 1, "select: multi -> newest index");
    expect(sel.active.entry.label === "fix-it: 2", "select: multi -> newest label");
  }
}

{
  // selectActiveUndo: when newest is STALE but older is READY,
  // surface the older ready entry (newest-first walk picks first
  // ready, not first stale).
  const ruleA = makeRule("A");
  const ruleB = makeRule("B");
  // Older entry (snapshot is [A, B], applied [B, A]): live chain
  // could still be the reorder, making it ready.
  const olderEntry = captureUndoEntry([ruleA, ruleB], [ruleB, ruleA], "older", 0);
  // Newer entry: snapshot has an extra rule the live chain now lacks
  // (we simulate user removing a rule between applies).
  const newerEntry = captureUndoEntry(
    [ruleA, ruleB, makeRule("PHANTOM")],
    [makeRule("PHANTOM"), ruleA, ruleB],
    "newer",
    0,
  );
  // Live chain matches reorder of older entry, but lacks PHANTOM -> newer is stale.
  const sel = selectActiveUndo([olderEntry, newerEntry], [ruleB, ruleA]);
  expect(sel.totalReady === 1, "select: 1 ready");
  expect(sel.totalStale === 1, "select: 1 stale");
  expect(sel.active !== null, "select: active not null");
  if (sel.active !== null) {
    expect(sel.active.entry.label === "older", "select: skipped stale newer, picked ready older");
    expect(sel.active.status.kind === "ready", "select: surfaced ready status");
  }
}

{
  // selectActiveUndo: all stale -> surface newest with stale status.
  const ruleA = makeRule("A");
  const ruleB = makeRule("B");
  // Both entries' snapshots include a rule the live chain no longer has.
  const stale1 = captureUndoEntry(
    [ruleA, ruleB, makeRule("X1")],
    [makeRule("X1"), ruleA, ruleB],
    "stale1",
    0,
  );
  const stale2 = captureUndoEntry(
    [ruleA, ruleB, makeRule("X2")],
    [makeRule("X2"), ruleA, ruleB],
    "stale2",
    0,
  );
  const sel = selectActiveUndo([stale1, stale2], [ruleA, ruleB]);
  expect(sel.totalReady === 0, "select: all stale -> 0 ready");
  expect(sel.totalStale === 2, "select: all stale -> 2 stale");
  expect(sel.active !== null, "select: all stale -> active still surfaced");
  if (sel.active !== null) {
    expect(sel.active.entry.label === "stale2", "select: all stale -> newest surfaced");
    expect(sel.active.status.kind === "stale", "select: status is stale");
  }
}

{
  // selectActiveUndo: noop entries (snapshot matches live) -> not
  // counted as ready, not surfaced if other ready entries exist.
  const ruleA = makeRule("A");
  const ruleB = makeRule("B");
  // First entry: snapshot IS the current chain -> noop.
  const noopEntry = captureUndoEntry([ruleA, ruleB], [ruleA, ruleB], "noop", 0);
  // Second entry: snapshot differs by permutation -> ready.
  const readyEntry = captureUndoEntry([ruleB, ruleA], [ruleA, ruleB], "ready", 0);
  const sel = selectActiveUndo([noopEntry, readyEntry], [ruleA, ruleB]);
  expect(sel.totalReady === 1, "select: ready count excludes noop");
  expect(sel.totalStale === 0, "select: noop count excludes stale");
  if (sel.active !== null) {
    // Newer (ready) entry is at index 1; surfaced.
    expect(sel.active.index === 1, "select: noop skipped, ready surfaced");
  }
}

{
  // End-to-end cascade: push 3 ready entries, undo one (= pop newest),
  // then verify selectActiveUndo finds the next-newest ready.
  const original = [makeRule("A"), makeRule("B"), makeRule("C")];
  const step1 = [makeRule("B"), makeRule("A"), makeRule("C")];
  const step2 = [makeRule("C"), makeRule("B"), makeRule("A")];
  const step3 = [makeRule("A"), makeRule("C"), makeRule("B")];
  let ring: ReorderUndoEntry[] = [];
  ring = pushUndoEntry(ring, captureUndoEntry(original, step1, "step1", 0), UNDO_RING_CAPACITY);
  ring = pushUndoEntry(ring, captureUndoEntry(step1, step2, "step2", 0), UNDO_RING_CAPACITY);
  ring = pushUndoEntry(ring, captureUndoEntry(step2, step3, "step3", 0), UNDO_RING_CAPACITY);
  expect(ring.length === 3, "cascade: 3 entries pushed");
  // Live chain is step3 -> newest entry (step3 snapshot is step2) is ready.
  // Since all snapshots are permutations of the same {A,B,C} rule set,
  // ALL three entries are technically "ready" against any live chain;
  // selectActiveUndo correctly picks the newest one.
  let sel = selectActiveUndo(ring, step3);
  expect(sel.active?.entry.label === "step3", "cascade: step3 entry active");
  expect(sel.totalReady === 3, "cascade: all 3 entries ready (same rule set)");
  // Simulate undo: chain reverts to step2, pop the newest entry.
  const pop1 = popUndoEntry(ring);
  ring = pop1.remaining;
  expect(ring.length === 2, "cascade: ring after pop has 2 entries");
  // Live chain now step2; step2 entry (snapshot=step1) should be ready.
  sel = selectActiveUndo(ring, step2);
  expect(sel.active?.entry.label === "step2", "cascade: step2 entry now active");
  // Undo again: chain -> step1, pop.
  const pop2 = popUndoEntry(ring);
  ring = pop2.remaining;
  sel = selectActiveUndo(ring, step1);
  expect(sel.active?.entry.label === "step1", "cascade: step1 entry now active");
  // Final undo: chain -> original, ring -> empty.
  const pop3 = popUndoEntry(ring);
  ring = pop3.remaining;
  expect(ring.length === 0, "cascade: ring fully drained");
  sel = selectActiveUndo(ring, original);
  expect(sel.active === null, "cascade: empty ring -> null active");
}

// ── Slice 154 — computeUndoJumpPlan / describeUndoJumpPlan / canApplyUndoJump ──

import {
  computeUndoJumpPlan,
  describeUndoJumpPlan,
  canApplyUndoJump,
} from "./hopper";

{
  // computeUndoJumpPlan: empty ring -> invalid; zeroed plan.
  const plan = computeUndoJumpPlan([], 0);
  expect(plan.is_valid === false, "jump: empty ring invalid");
  expect(plan.skip_count === 0, "jump: empty ring skip=0");
  expect(plan.dropped_labels.length === 0, "jump: empty ring no labels");
  expect(plan.target_label === "", "jump: empty ring no label");
  expect(plan.target_index === 0, "jump: empty ring index=0");
}

{
  // computeUndoJumpPlan: out-of-range index -> invalid.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = computeUndoJumpPlan(entries, 5);
  expect(plan.is_valid === false, "jump: oor invalid");
  expect(plan.skip_count === 0, "jump: oor skip=0");
  expect(plan.target_label === "", "jump: oor no label");
}

{
  // computeUndoJumpPlan: target == newest -> invalid noop, label echoed.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = computeUndoJumpPlan(entries, 2);
  expect(plan.is_valid === false, "jump: target=newest invalid");
  expect(plan.skip_count === 0, "jump: target=newest skip=0");
  expect(plan.target_label === "c", "jump: target=newest label echoed");
  expect(plan.target_index === 2, "jump: target=newest index echoed");
  expect(plan.dropped_labels.length === 0, "jump: target=newest no dropped");
}

{
  // computeUndoJumpPlan: skip one entry.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = computeUndoJumpPlan(entries, 1);
  expect(plan.is_valid === true, "jump: skip-1 valid");
  expect(plan.skip_count === 1, "jump: skip-1 count");
  expect(plan.dropped_labels.length === 1, "jump: skip-1 labels len");
  expect(plan.dropped_labels[0] === "c", "jump: skip-1 dropped is newest 'c'");
  expect(plan.target_label === "b", "jump: skip-1 target 'b'");
  expect(plan.target_index === 1, "jump: skip-1 index echoed");
}

{
  // computeUndoJumpPlan: skip to oldest, all newer dropped newest-first.
  const entries = [
    ringEntry("a", 100),
    ringEntry("b", 200),
    ringEntry("c", 300),
    ringEntry("d", 400),
    ringEntry("e", 500),
  ];
  const plan = computeUndoJumpPlan(entries, 0);
  expect(plan.is_valid === true, "jump: skip-to-oldest valid");
  expect(plan.skip_count === 4, "jump: skip-to-oldest count=4");
  expect(plan.dropped_labels.length === 4, "jump: skip-to-oldest labels len");
  // Newest-first order.
  expect(plan.dropped_labels[0] === "e", "jump: dropped[0]=e (newest first)");
  expect(plan.dropped_labels[1] === "d", "jump: dropped[1]=d");
  expect(plan.dropped_labels[2] === "c", "jump: dropped[2]=c");
  expect(plan.dropped_labels[3] === "b", "jump: dropped[3]=b");
  expect(plan.target_label === "a", "jump: skip-to-oldest target 'a'");
}

{
  // computeUndoJumpPlan: single-entry ring -> noop (only entry IS newest).
  const entries = [ringEntry("only", 100)];
  const plan = computeUndoJumpPlan(entries, 0);
  expect(plan.is_valid === false, "jump: single-entry invalid noop");
  expect(plan.target_label === "only", "jump: single-entry label echoed");
}

{
  // computeUndoJumpPlan: NaN target index defensively treated as oor.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = computeUndoJumpPlan(entries, NaN);
  expect(plan.is_valid === false, "jump: NaN index invalid");
  expect(plan.skip_count === 0, "jump: NaN skip=0");
}

{
  // computeUndoJumpPlan: negative target index defensively treated as oor.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = computeUndoJumpPlan(entries, -1);
  expect(plan.is_valid === false, "jump: negative index invalid");
  expect(plan.skip_count === 0, "jump: negative skip=0");
}

{
  // computeUndoJumpPlan: non-integer (e.g. 1.5) defensively treated as oor.
  // Avoids floor-vs-round ambiguity at call sites.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = computeUndoJumpPlan(entries, 1.5);
  expect(plan.is_valid === false, "jump: non-integer index invalid");
  expect(plan.skip_count === 0, "jump: non-integer skip=0");
}

{
  // computeUndoJumpPlan: no input mutation.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const snapshot = JSON.stringify(entries);
  computeUndoJumpPlan(entries, 1);
  expect(JSON.stringify(entries) === snapshot, "jump: input unchanged");
}

{
  // computeUndoJumpPlan: skip_count invariant matches dropped_labels.length
  // for every valid in-range target.
  const entries = [
    ringEntry("a", 100),
    ringEntry("b", 200),
    ringEntry("c", 300),
    ringEntry("d", 400),
    ringEntry("e", 500),
  ];
  for (let target = 0; target < entries.length - 1; target++) {
    const plan = computeUndoJumpPlan(entries, target);
    expect(plan.is_valid === true, `jump: target=${target} valid`);
    expect(
      plan.skip_count === plan.dropped_labels.length,
      `jump: skip_count === dropped_labels.length at target=${target}`,
    );
    expect(plan.target_index === target, `jump: target_index echoes input at target=${target}`);
  }
}

{
  // describeUndoJumpPlan: empty ring -> "No jump available".
  const plan = computeUndoJumpPlan([], 0);
  expect(describeUndoJumpPlan(plan) === "No jump available", "describe: empty");
}

{
  // describeUndoJumpPlan: out-of-range -> "No jump available".
  const entries = [ringEntry("a", 100)];
  const plan = computeUndoJumpPlan(entries, 99);
  expect(describeUndoJumpPlan(plan) === "No jump available", "describe: oor");
}

{
  // describeUndoJumpPlan: target=newest -> "Already the newest entry".
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = computeUndoJumpPlan(entries, 1);
  expect(
    describeUndoJumpPlan(plan) === "Already the newest entry",
    "describe: target=newest",
  );
}

{
  // describeUndoJumpPlan: skip 1 -> "Skip 1 revert to jump back to <label>".
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = computeUndoJumpPlan(entries, 1);
  expect(
    describeUndoJumpPlan(plan) === "Skip 1 revert to jump back to b",
    "describe: skip-1 singular",
  );
}

{
  // describeUndoJumpPlan: skip N>1 -> "Skip N reverts to jump back to <label>".
  const entries = [
    ringEntry("fix-it: Tax", 100),
    ringEntry("fix-all", 200),
    ringEntry("fix-it: Receipts", 300),
    ringEntry("fix-it: Misc", 400),
  ];
  const plan = computeUndoJumpPlan(entries, 0);
  expect(
    describeUndoJumpPlan(plan) === "Skip 3 reverts to jump back to fix-it: Tax",
    "describe: skip-N plural with real label",
  );
}

{
  // canApplyUndoJump: invalid -> false.
  const plan = computeUndoJumpPlan([], 0);
  expect(canApplyUndoJump(plan) === false, "canApply: empty false");
}

{
  // canApplyUndoJump: target=newest -> false.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = computeUndoJumpPlan(entries, 1);
  expect(canApplyUndoJump(plan) === false, "canApply: target=newest false");
}

{
  // canApplyUndoJump: valid skip>=1 -> true.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = computeUndoJumpPlan(entries, 0);
  expect(canApplyUndoJump(plan) === true, "canApply: valid jump true");
}

{
  // Pinned: wire shape uses snake_case for round-tripping with Rust.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = computeUndoJumpPlan(entries, 0);
  const json = JSON.stringify(plan);
  expect(json.includes('"is_valid":true'), "wire: snake_case is_valid");
  expect(json.includes('"skip_count":2'), "wire: snake_case skip_count");
  expect(json.includes('"dropped_labels":'), "wire: snake_case dropped_labels");
  expect(json.includes('"target_label":"a"'), "wire: snake_case target_label");
  expect(json.includes('"target_index":0'), "wire: snake_case target_index");
}

{
  // End-to-end composition with summarizeUndoRing: feed trimmed
  // entries into computeUndoJumpPlan. A 7-entry ring trimmed to
  // capacity 5 (oldest two dropped) leaves c..g; jumping to index 0
  // of the summary should target "c".
  const raw = [
    ringEntry("a", 100),
    ringEntry("b", 200),
    ringEntry("c", 300),
    ringEntry("d", 400),
    ringEntry("e", 500),
    ringEntry("f", 600),
    ringEntry("g", 700),
  ];
  const trimmed = summarizeUndoRing(raw, 5);
  const plan = computeUndoJumpPlan(trimmed.entries, 0);
  expect(plan.is_valid === true, "compose: trimmed jump valid");
  expect(plan.target_label === "c", "compose: target post-trim is 'c'");
  expect(plan.skip_count === 4, "compose: skip post-trim is 4");
  expect(plan.dropped_labels[0] === "g", "compose: newest dropped is 'g'");
  expect(plan.dropped_labels[3] === "d", "compose: oldest dropped is 'd'");
}

// ── Slice 155 — slabHopperComputeUndoJumpPlan wrapper-delegation ────

import { slabHopperComputeUndoJumpPlan } from "./hopper";

{
  // Browser-mode delegation: empty ring -> invalid plan.
  const plan = await slabHopperComputeUndoJumpPlan([], 0);
  expect(plan.is_valid === false, "wrapper: empty invalid");
  expect(plan.skip_count === 0, "wrapper: empty skip=0");
  expect(plan.dropped_labels.length === 0, "wrapper: empty no labels");
  expect(plan.target_label === "", "wrapper: empty no label");
  expect(plan.target_index === 0, "wrapper: empty index=0");
}

{
  // Browser-mode delegation: out-of-range index -> invalid plan.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 5);
  expect(plan.is_valid === false, "wrapper: oor invalid");
  expect(plan.skip_count === 0, "wrapper: oor skip=0");
  expect(plan.target_label === "", "wrapper: oor no label");
}

{
  // Browser-mode delegation: target == newest -> invalid noop, label echoed.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 2);
  expect(plan.is_valid === false, "wrapper: target=newest invalid");
  expect(plan.skip_count === 0, "wrapper: target=newest skip=0");
  expect(plan.target_label === "c", "wrapper: target=newest label echoed");
  expect(plan.target_index === 2, "wrapper: target=newest index echoed");
}

{
  // Browser-mode delegation: valid skip-1 plan.
  const entries = [ringEntry("a", 100), ringEntry("b", 200), ringEntry("c", 300)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 1);
  expect(plan.is_valid === true, "wrapper: skip-1 valid");
  expect(plan.skip_count === 1, "wrapper: skip-1 count");
  expect(plan.dropped_labels.length === 1, "wrapper: skip-1 labels len");
  expect(plan.dropped_labels[0] === "c", "wrapper: skip-1 dropped 'c'");
  expect(plan.target_label === "b", "wrapper: skip-1 target 'b'");
  expect(plan.target_index === 1, "wrapper: skip-1 index echoed");
}

{
  // Browser-mode delegation: valid skip-to-oldest plan,
  // newest-first dropped order pinned through the wrapper.
  const entries = [
    ringEntry("a", 100),
    ringEntry("b", 200),
    ringEntry("c", 300),
    ringEntry("d", 400),
    ringEntry("e", 500),
  ];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 0);
  expect(plan.is_valid === true, "wrapper: skip-to-oldest valid");
  expect(plan.skip_count === 4, "wrapper: skip-to-oldest count=4");
  expect(plan.dropped_labels[0] === "e", "wrapper: dropped[0]=e (newest)");
  expect(plan.dropped_labels[1] === "d", "wrapper: dropped[1]=d");
  expect(plan.dropped_labels[2] === "c", "wrapper: dropped[2]=c");
  expect(plan.dropped_labels[3] === "b", "wrapper: dropped[3]=b");
  expect(plan.target_label === "a", "wrapper: target=a");
  expect(plan.target_index === 0, "wrapper: target_index=0 echoed");
}

{
  // Browser-mode delegation: defensive negative index.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, -1);
  expect(plan.is_valid === false, "wrapper: negative invalid");
}

{
  // Browser-mode delegation: defensive NaN index.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, NaN);
  expect(plan.is_valid === false, "wrapper: NaN invalid");
}

{
  // Browser-mode delegation: single-entry ring is a noop.
  const entries = [ringEntry("only", 100)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 0);
  expect(plan.is_valid === false, "wrapper: single-entry noop");
  expect(plan.target_label === "only", "wrapper: single-entry label echoed");
}

{
  // Browser-mode delegation: skip_count matches dropped_labels.length
  // through the wrapper (pinned-invariant check via wrapper path).
  const entries = [
    ringEntry("a", 100),
    ringEntry("b", 200),
    ringEntry("c", 300),
    ringEntry("d", 400),
  ];
  for (let target = 0; target < entries.length - 1; target++) {
    const plan = await slabHopperComputeUndoJumpPlan(entries, target);
    expect(plan.is_valid === true, `wrapper: target=${target} valid`);
    expect(
      plan.skip_count === plan.dropped_labels.length,
      `wrapper: skip_count===dropped_labels.length at target=${target}`,
    );
  }
}

{
  // Browser-mode delegation: real fix-it / fix-all labels round-trip
  // through the wrapper without truncation / escaping.
  const entries = [
    ringEntry("fix-it: Tax", 100),
    ringEntry("fix-all", 200),
    ringEntry("fix-it: Receipts", 300),
  ];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 0);
  expect(plan.is_valid === true, "wrapper: real-labels valid");
  expect(plan.target_label === "fix-it: Tax", "wrapper: target label round-trip");
  expect(plan.dropped_labels[0] === "fix-it: Receipts", "wrapper: dropped[0] label round-trip");
  expect(plan.dropped_labels[1] === "fix-all", "wrapper: dropped[1] label round-trip");
}

{
  // Wrapper preserves the wire-shape snake_case fields exactly.
  // The Tauri-mode path round-trips JSON through serde so this is
  // the strongest guarantee a future audit consumer needs.
  const entries = [ringEntry("a", 100), ringEntry("b", 200)];
  const plan = await slabHopperComputeUndoJumpPlan(entries, 0);
  expect(typeof plan.is_valid === "boolean", "wrapper: is_valid type");
  expect(typeof plan.skip_count === "number", "wrapper: skip_count type");
  expect(Array.isArray(plan.dropped_labels), "wrapper: dropped_labels array");
  expect(typeof plan.target_label === "string", "wrapper: target_label type");
  expect(typeof plan.target_index === "number", "wrapper: target_index type");
}

// ── Slice 156 — jumpToUndoEntry / summarizeRingForJump bridge ──────

import {
  jumpToUndoEntry,
  summarizeRingForJump,
} from "./hopper";

{
  // jumpToUndoEntry: empty ring -> invalid; defensive copy; target null.
  const result = jumpToUndoEntry([], 0);
  expect(result.is_valid === false, "jump-live: empty invalid");
  expect(result.ring.length === 0, "jump-live: empty ring stays empty");
  expect(result.target === null, "jump-live: empty target null");
  expect(result.dropped === 0, "jump-live: empty dropped=0");
}

{
  // jumpToUndoEntry: out-of-range -> invalid; defensive shallow copy.
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const ring = [e1];
  const result = jumpToUndoEntry(ring, 99);
  expect(result.is_valid === false, "jump-live: oor invalid");
  expect(result.ring.length === 1, "jump-live: oor ring preserved");
  expect(result.target === null, "jump-live: oor target null");
  expect(result.dropped === 0, "jump-live: oor dropped=0");
  // Defensive copy: result.ring is NOT the same array as input.
  expect(result.ring !== ring, "jump-live: oor returns fresh array");
}

{
  // jumpToUndoEntry: target == newest -> invalid noop; target echoed.
  const a = makeRule("A"), b = makeRule("B"), c = makeRule("C");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 1);
  const e3 = captureUndoEntry([a, b], [c, a, b], "e3", 2);
  const ring = [e1, e2, e3];
  const result = jumpToUndoEntry(ring, 2);
  expect(result.is_valid === false, "jump-live: target=newest invalid");
  expect(result.ring.length === 3, "jump-live: target=newest ring preserved");
  expect(result.target?.label === "e3", "jump-live: target=newest echoed");
  expect(result.dropped === 0, "jump-live: target=newest dropped=0");
}

{
  // jumpToUndoEntry: valid skip-1 trims newest entry.
  const a = makeRule("A"), b = makeRule("B"), c = makeRule("C");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 1);
  const e3 = captureUndoEntry([a, b], [c, a, b], "e3", 2);
  const ring = [e1, e2, e3];
  const result = jumpToUndoEntry(ring, 1);
  expect(result.is_valid === true, "jump-live: skip-1 valid");
  expect(result.ring.length === 2, "jump-live: skip-1 ring len");
  expect(result.ring[0].label === "e1", "jump-live: skip-1 ring[0] preserved");
  expect(result.ring[1].label === "e2", "jump-live: skip-1 new newest is e2");
  expect(result.target?.label === "e2", "jump-live: skip-1 target is e2");
  expect(result.dropped === 1, "jump-live: skip-1 dropped=1");
}

{
  // jumpToUndoEntry: valid skip-to-oldest keeps only target.
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 1);
  const e3 = captureUndoEntry([a, b], [b, a], "e3", 2);
  const e4 = captureUndoEntry([b, a], [a, b], "e4", 3);
  const ring = [e1, e2, e3, e4];
  const result = jumpToUndoEntry(ring, 0);
  expect(result.is_valid === true, "jump-live: skip-to-oldest valid");
  expect(result.ring.length === 1, "jump-live: skip-to-oldest keeps only target");
  expect(result.ring[0].label === "e1", "jump-live: skip-to-oldest ring is [e1]");
  expect(result.target?.label === "e1", "jump-live: skip-to-oldest target e1");
  expect(result.dropped === 3, "jump-live: skip-to-oldest dropped=3");
}

{
  // jumpToUndoEntry: defensive negative index.
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const ring = [e1];
  const result = jumpToUndoEntry(ring, -1);
  expect(result.is_valid === false, "jump-live: negative invalid");
  expect(result.ring.length === 1, "jump-live: negative ring preserved");
  expect(result.target === null, "jump-live: negative target null");
}

{
  // jumpToUndoEntry: defensive NaN index.
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const ring = [e1];
  const result = jumpToUndoEntry(ring, NaN);
  expect(result.is_valid === false, "jump-live: NaN invalid");
  expect(result.target === null, "jump-live: NaN target null");
}

{
  // jumpToUndoEntry: defensive non-integer (1.5) index.
  const a = makeRule("A"), b = makeRule("B"), c = makeRule("C");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 1);
  const e3 = captureUndoEntry([a, b], [c, a, b], "e3", 2);
  const ring = [e1, e2, e3];
  const result = jumpToUndoEntry(ring, 1.5);
  expect(result.is_valid === false, "jump-live: non-integer invalid");
  expect(result.ring.length === 3, "jump-live: non-integer ring preserved");
  expect(result.target === null, "jump-live: non-integer target null");
}

{
  // jumpToUndoEntry: no input mutation.
  const a = makeRule("A"), b = makeRule("B"), c = makeRule("C");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 1);
  const e3 = captureUndoEntry([a, b], [c, a, b], "e3", 2);
  const ring = [e1, e2, e3];
  const before = ring.length;
  jumpToUndoEntry(ring, 0);
  expect(ring.length === before, "jump-live: input ring length unchanged");
  expect(ring[0].label === "e1", "jump-live: ring[0] unchanged");
  expect(ring[2].label === "e3", "jump-live: ring[2] unchanged");
}

{
  // jumpToUndoEntry: dropped + new ring.length = original length
  // for valid jumps. This is the load-bearing invariant the popover's
  // toast copy depends on ("Reverted N rules · ring drained to step M").
  const a = makeRule("A"), b = makeRule("B");
  const ring = [
    captureUndoEntry([a, b], [b, a], "e1", 0),
    captureUndoEntry([b, a], [a, b], "e2", 1),
    captureUndoEntry([a, b], [b, a], "e3", 2),
    captureUndoEntry([b, a], [a, b], "e4", 3),
    captureUndoEntry([a, b], [b, a], "e5", 4),
  ];
  for (let target = 0; target < ring.length - 1; target++) {
    const result = jumpToUndoEntry(ring, target);
    expect(result.is_valid === true, `invariant: target=${target} valid`);
    expect(
      result.ring.length + result.dropped === ring.length,
      `invariant: ring.length + dropped === original at target=${target}`,
    );
    expect(result.target?.label === `e${target + 1}`, `invariant: target label at ${target}`);
  }
}

{
  // jumpToUndoEntry: trimmed ring preserves snapshot reference
  // identity for retained entries. The UI's applyJump path passes
  // ring[targetIndex].snapshot to slabHopperSetRules; if the trim
  // dropped or rebuilt entries, the snapshot would silently lose
  // its identity for downstream consumers (audit logging,
  // selectActiveUndo).
  const a = makeRule("A"), b = makeRule("B"), c = makeRule("C");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 0);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 1);
  const e3 = captureUndoEntry([a, b], [c, a, b], "e3", 2);
  const ring = [e1, e2, e3];
  const result = jumpToUndoEntry(ring, 0);
  expect(result.target === e1, "jump-live: target is reference-identical to ring[0]");
  expect(result.ring[0] === e1, "jump-live: ring[0] preserves reference identity");
}

{
  // summarizeRingForJump: empty ring -> empty array.
  const summaries = summarizeRingForJump([]);
  expect(summaries.length === 0, "summarize: empty");
}

{
  // summarizeRingForJump: each entry maps to its compact wire shape.
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "fix-it: Tax", 100);
  const e2 = captureUndoEntry([b, a], [a, b], "fix-all", 200);
  const summaries = summarizeRingForJump([e1, e2]);
  expect(summaries.length === 2, "summarize: 2-entry length");
  expect(summaries[0].label === "fix-it: Tax", "summarize: label[0]");
  expect(summaries[0].captured_at_ms === 100, "summarize: captured_at_ms[0]");
  expect(summaries[0].applied_effect === e1.appliedEffect, "summarize: effect[0] reference");
  expect(summaries[1].label === "fix-all", "summarize: label[1]");
  expect(summaries[1].captured_at_ms === 200, "summarize: captured_at_ms[1]");
}

{
  // summarizeRingForJump: no input mutation; result is fresh array.
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 100);
  const ring = [e1];
  const beforeLen = ring.length;
  const summaries = summarizeRingForJump(ring);
  expect(ring.length === beforeLen, "summarize: input length unchanged");
  expect(summaries !== ring as unknown, "summarize: fresh array");
}

{
  // End-to-end: summarize -> compute plan -> apply jump.
  // Mirrors what the popover does: take live ring, render plan
  // via slice-154 planner, user confirms, apply trim.
  const a = makeRule("A"), b = makeRule("B"), c = makeRule("C");
  const e1 = captureUndoEntry([a, b, c], [b, a, c], "e1", 100);
  const e2 = captureUndoEntry([b, a, c], [c, b, a], "e2", 200);
  const e3 = captureUndoEntry([c, b, a], [a, c, b], "e3", 300);
  const ring = [e1, e2, e3];
  // Render plan for "jump to oldest" (index 0).
  const summaries = summarizeRingForJump(ring);
  const plan = computeUndoJumpPlan(summaries, 0);
  expect(plan.is_valid === true, "e2e: plan valid");
  expect(plan.skip_count === 2, "e2e: plan skip_count");
  expect(plan.target_label === "e1", "e2e: plan target");
  // User confirms; bridge trims the live ring.
  const result = jumpToUndoEntry(ring, plan.target_index);
  expect(result.is_valid === true, "e2e: bridge valid");
  expect(result.ring.length === 1, "e2e: bridge ring trimmed to 1");
  expect(result.target?.label === "e1", "e2e: bridge target matches plan");
  expect(result.dropped === plan.skip_count, "e2e: bridge dropped matches plan");
}

{
  // End-to-end: plan-then-apply round-trips snapshot identity.
  // The applyJump path passes target.snapshot to slabHopperSetRules;
  // verify the snapshot is identical to the snapshot at the
  // pre-trim entry (no copying/serialising in the bridge layer).
  const a = makeRule("A"), b = makeRule("B");
  const e1 = captureUndoEntry([a, b], [b, a], "e1", 100);
  const e2 = captureUndoEntry([b, a], [a, b], "e2", 200);
  const ring = [e1, e2];
  const result = jumpToUndoEntry(ring, 0);
  expect(result.target?.snapshot === e1.snapshot, "e2e: snapshot reference preserved");
}
