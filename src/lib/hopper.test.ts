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
