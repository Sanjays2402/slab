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
  type RuleCoverageReport,
  type RuleCoverage,
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
