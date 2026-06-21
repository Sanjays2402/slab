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
  ruleBucket,
  FALLTHROUGH_BUCKET,
  sampleBucketEquals,
  describeDrilldown,
  describeBucket,
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
