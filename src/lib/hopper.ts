// Hopper client — v3.20.0
//
// Typed bridge to the Rust Hopper engine in `src-tauri/src/pdf/hopper/`.
// Hopper watches one or more folders for newly-dropped PDFs and runs an
// Atelier recipe + AI-rename + auto-file pipeline on each one. Think
// Hazel × Adobe AutoActions × a local AI paralegal, all offline.
//
// Event stream: every completed pipeline run is broadcast as a Tauri
// event named `hopper://run-completed` carrying a `RunRecord`. The UI
// listens and prepends to the live log; on missed events the
// `slabHopperListRuns` poll fills the gap.
//
// See `src-tauri/src/pdf/hopper/{registry,log,pipeline,watcher,cmds}.rs`
// for canonical shapes.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------
// Wire types — mirror the Rust serde shapes verbatim.
// ---------------------------------------------------------------------

/** A persisted watched-folder configuration. */
export interface Watch {
  id: number;
  source_dir: string;
  output_dir: string;
  recipe_id: string | null;
  rename_pattern: string | null;
  ai_rename: boolean;
  enabled: boolean;
  created_at: string;
}

/** Input payload for `slabHopperAddWatch`. The backend assigns id +
 *  timestamps. */
export interface WatchInput {
  source_dir: string;
  output_dir: string;
  recipe_id: string | null;
  rename_pattern: string | null;
  ai_rename: boolean;
}

/** One row in the run history. */
export interface RunRecord {
  id: number;
  watch_id: number;
  input_path: string;
  output_path: string | null;
  status: "success" | "failed";
  error: string | null;
  duration_ms: number;
  ai_title: string | null;
  started_at: string;
}

/** Service-status payload returned by `slabHopperDescribe`. */
export interface HopperStatus {
  watch_count: number;
  run_count: number;
  version: string;
}

// ---------------------------------------------------------------------
// Tauri command bindings — one-liner wrappers around `invoke`.
// ---------------------------------------------------------------------

export const slabHopperListWatches = (): Promise<Watch[]> =>
  invoke("slab_hopper_list_watches");

export const slabHopperAddWatch = (input: WatchInput): Promise<Watch> =>
  invoke("slab_hopper_add_watch", { input });

export const slabHopperRemoveWatch = (id: number): Promise<void> =>
  invoke("slab_hopper_remove_watch", { id });

export const slabHopperSetEnabled = (
  id: number,
  enabled: boolean,
): Promise<void> => invoke("slab_hopper_set_enabled", { id, enabled });

export const slabHopperListRuns = (limit: number): Promise<RunRecord[]> =>
  invoke("slab_hopper_list_runs", { limit });

export const slabHopperRunNow = (
  watch_id: number,
  path: string,
): Promise<void> => invoke("slab_hopper_run_now", { watchId: watch_id, path });

export const slabHopperDescribe = (): Promise<HopperStatus> =>
  invoke("slab_hopper_describe");

// ---------------------------------------------------------------------
// Event listener — subscribe to `hopper://run-completed`.
// ---------------------------------------------------------------------

/** Subscribe to live pipeline-completion events. Returns an unlisten
 *  function the caller stores and invokes on component unmount. */
export const listenRunCompleted = async (
  handler: (rec: RunRecord) => void,
): Promise<UnlistenFn> => {
  return listen<RunRecord>("hopper://run-completed", (e) => handler(e.payload));
};

// ---------------------------------------------------------------------
// Pure formatting helpers — also used in unit tests.
// ---------------------------------------------------------------------

/** Format a duration in milliseconds as a compact human string. */
export const formatDuration = (ms: number): string => {
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const minutes = Math.floor(seconds / 60);
  const rem = Math.floor(seconds % 60);
  return `${minutes}m ${rem}s`;
};

/** Format a unix-seconds string (what the backend stores) as a short
 *  local time. Returns "—" for malformed inputs. */
export const formatStartedAt = (seconds: string): string => {
  const n = Number(seconds);
  if (!Number.isFinite(n) || n <= 0) return "—";
  const d = new Date(n * 1000);
  if (isNaN(d.valueOf())) return "—";
  return d.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
};

/** Extract just the filename from a full path, robust to win/unix. */
export const basename = (path: string): string => {
  const norm = path.replace(/\\/g, "/");
  const ix = norm.lastIndexOf("/");
  return ix === -1 ? norm : norm.slice(ix + 1);
};

/** Suggest a sensible default rename pattern. Power users can edit. */
export const defaultRenamePattern = (aiRename: boolean): string =>
  aiRename ? "{date}_{ai_title}.pdf" : "{date}_{stem}.pdf";

// ---------------------------------------------------------------------
// v3.21.0 — Hopper Conditions: per-watch routing rules
// ---------------------------------------------------------------------
//
// A `Rule` pairs a `RulePredicate` (when to apply) with a `RuleAction`
// (what to override on the watch). Rules are evaluated in priority
// order; the first match wins; non-matching files fall through to the
// watch defaults. Persisted as a JSON array in `watches.rules_json`.
//
// The discriminated-union shapes mirror Rust serde verbatim
// (`#[serde(tag = "kind", rename_all = "kebab-case")]`).

/** Discriminated union — every variant has its own payload shape. */
export type RulePredicate =
  | { kind: "filename-glob"; pattern: string }
  | { kind: "filename-regex"; pattern: string }
  | { kind: "text-contains-all"; needles: string[] }
  | { kind: "page-count-between"; min: number; max: number }
  | { kind: "size-over"; bytes: number }
  | { kind: "always" };

/** Action overlay — any `null` field inherits from the watch. */
export interface RuleAction {
  recipe_id: string | null;
  output_dir: string | null;
  rename_pattern: string | null;
}

/** One routing rule. Display `name` is what shows in the run log's
 *  `matched_rule` column when this rule wins. */
export interface Rule {
  name: string;
  predicate: RulePredicate;
  action: RuleAction;
}

/** Preview payload for `slabHopperTestRules` — what would happen if a
 *  file with this filename arrived under this watch. */
export interface RuleTestResult {
  matched_index: number | null;
  matched_rule: string | null;
  recipe_id: string | null;
  output_dir: string;
  rename_pattern: string | null;
}

// ---------------------------------------------------------------------
// Rule CRUD + preview command bindings.
// ---------------------------------------------------------------------

/** Load the persisted rule chain for a watch. Empty `[]` means
 *  "no conditional routing; use the watch defaults". */
export const slabHopperGetRules = (watchId: number): Promise<Rule[]> =>
  invoke("slab_hopper_get_rules", { watchId });

/** Atomically replace the rule chain for a watch. Takes effect on the
 *  next file the watcher dispatches; no restart needed. */
export const slabHopperSetRules = (
  watchId: number,
  rules: Rule[],
): Promise<void> => invoke("slab_hopper_set_rules", { watchId, rules });

/** Test how a candidate filename would be routed under a watch with
 *  (optionally) an in-flight, unsaved rule list. Used by the live
 *  preview pane in the rule editor. `sizeBytes` / `pageCount` are
 *  optional hints; when absent the predicate context uses 0 / null. */
export const slabHopperTestRules = (
  watchId: number,
  filename: string,
  opts: {
    sizeBytes?: number;
    pageCount?: number | null;
    candidateRules?: Rule[];
  } = {},
): Promise<RuleTestResult> =>
  invoke("slab_hopper_test_rules", {
    watchId,
    filename,
    sizeBytes: opts.sizeBytes ?? null,
    pageCount: opts.pageCount ?? null,
    candidateRules: opts.candidateRules ?? null,
  });

// ---------------------------------------------------------------------
// v3.40 Slice 81 — rule coverage analyzer
// ---------------------------------------------------------------------

/** One sample file the coverage analyzer evaluates against the rule
 *  chain. Mirrors `pdf::hopper::coverage::RuleSample`. The Rust side
 *  defaults size/page/text to zero/null so the most common call shape
 *  (filename-only from the run log) can omit them. */
export interface RuleSample {
  filename: string;
  size_bytes?: number;
  page_count?: number | null;
  text_sample?: string | null;
}

/** Per-rule coverage counts. Mirrors `pdf::hopper::coverage::RuleCoverage`.
 *  `first_match` is the actual routing volume at runtime; `would_match`
 *  is the count in isolation — when strictly larger than `first_match`
 *  the rule is shadowed by an earlier rule. When `first_match` is zero
 *  but `would_match` is positive, `dead_at_position` is true and the UI
 *  surfaces a dead-rule chip. */
export interface RuleCoverage {
  index: number;
  name: string;
  first_match: number;
  would_match: number;
  dead_at_position: boolean;
}

/** Full coverage report for one chain against one sample set. Mirrors
 *  `pdf::hopper::coverage::RuleCoverageReport`. By construction,
 *  `rules.sum(first_match) + fallthrough === total_samples`. */
export interface RuleCoverageReport {
  rules: RuleCoverage[];
  fallthrough: number;
  total_samples: number;
}

/** Evaluate a rule chain against the watch's recent run log (or an
 *  explicit sample list) and return the per-rule coverage report. When
 *  `candidateRules` is set, evaluates the in-flight unsaved chain (so
 *  the editor can show live coverage without a save round-trip).
 *  When `samples` is set, uses those instead of the log-sourced
 *  default. `sampleLimit` defaults to 100 server-side and is clamped
 *  to [1, 1000]. */
export const slabHopperRuleCoverage = (
  watchId: number,
  opts: {
    candidateRules?: Rule[];
    samples?: RuleSample[];
    sampleLimit?: number;
  } = {},
): Promise<RuleCoverageReport> =>
  invoke("slab_hopper_rule_coverage", {
    watchId,
    candidateRules: opts.candidateRules ?? null,
    samples: opts.samples ?? null,
    sampleLimit: opts.sampleLimit ?? null,
  });

/** Compute the fall-through percentage of a coverage report as a
 *  number in `[0, 100]`. Returns `0` on an empty report so an "0 of 0"
 *  edge case doesn't render NaN. */
export const fallthroughPercent = (report: RuleCoverageReport): number => {
  if (report.total_samples === 0) return 0;
  return (report.fallthrough / report.total_samples) * 100;
};

/** Compute the share of samples a rule actually routes at runtime,
 *  as a number in `[0, 100]`. */
export const ruleMatchPercent = (
  rule: RuleCoverage,
  report: RuleCoverageReport,
): number => {
  if (report.total_samples === 0) return 0;
  return (rule.first_match / report.total_samples) * 100;
};

/** Diagnostic label for a single rule's coverage row. Returns null
 *  when the rule has no notable diagnostic (a "healthy" first-match
 *  count); the UI hides the chip when the helper returns null. */
export const ruleCoverageDiagnostic = (
  rule: RuleCoverage,
): "dead" | "zero" | "shadowed" | null => {
  if (rule.dead_at_position) return "dead";
  if (rule.would_match === 0) return "zero";
  if (rule.would_match > rule.first_match) return "shadowed";
  return null;
};

/** One-line summary copy for the coverage panel header. Returns a
 *  friendly empty-state string when no samples were scanned. */
export const summarizeCoverage = (report: RuleCoverageReport): string => {
  const n = report.total_samples;
  if (n === 0) return "No recent runs to analyse";
  const routed = n - report.fallthrough;
  const pct = Math.round((routed / n) * 100);
  return `${routed} of ${n} samples routed (${pct}%)`;
};

// ─── v3.40 Slice 126 — chain-health summary helper ───────────────────
//
// `summarizeCoverage` (above) answers "how many samples did the
// chain route?". The structurally important follow-up — "is the
// chain HEALTHY, or are rules silently shadowed?" — has no helper
// yet. A 6-rule chain that routes 100% of samples through Rule 1
// looks fine on the routed-percentage summary line but is hiding
// 5 dead-by-shadow rules from the user. The coverage panel
// surfaces per-row "dead" chips today (via `ruleCoverageDiagnostic`),
// but the chain-level health story is buried in those chips.
//
// This helper composes one. Counts dead / shadowed / zero-coverage
// rules in ONE pass (same priority chain as
// `ruleCoverageDiagnostic`: dead > zero > shadowed > healthy —
// mutually exclusive so a rule contributes to AT MOST one count),
// then emits a discriminated copy line + a kind tag the UI can
// gate styling on:
//
//   "healthy"   — every rule fires + low fall-through
//   "warn"      — at least one diagnostic OR fall-through > 25%
//   "critical"  — at least one dead rule
//   "empty"     — no samples scanned yet
//
// The `kind` tag is the load-bearing field. The text is the
// human-facing copy the UI renders next to the routed-percentage
// summary; the kind drives the chip's color (neutral / warn /
// critical / muted) without re-deriving the classification in
// Svelte.
//
// Why the 25% fall-through threshold for "warn": a chain where one
// in four files falls through to defaults is materially under-
// specified — the user almost certainly meant to catch more. Below
// 25% the fall-through is normal-noise (a workstation with a long
// tail of misc files; a watch with an "everything-else" default
// recipe). Tuneable by the caller (`opts.fallthroughWarnPct`) for
// future surfaces with different defaults.

/** Discriminator for the chain-health classification. Drives the
 *  chip's color + icon in the UI. Mutually exclusive — one chain
 *  has exactly one kind. */
export type CoverageHealthKind = "healthy" | "warn" | "critical" | "empty";

/** Chain-health summary derived from a coverage report. The `text`
 *  field is the human copy the UI renders; `kind` drives styling. */
export interface CoverageHealth {
  /** Discriminator the UI gates styling on. */
  kind: CoverageHealthKind;
  /** Display copy ("Chain healthy" / "3 dead rules — reorder or
   *  tighten" / etc). One sentence, no trailing period (the panel
   *  chrome wraps it). */
  text: string;
  /** Count of rules with `dead_at_position == true`. The chain-
   *  critical insight: these rules never fire at their position
   *  but WOULD fire earlier. */
  dead: number;
  /** Count of rules where `would_match > first_match` and NOT dead.
   *  Partial-shadow only — the rule fires sometimes but is losing
   *  matches to earlier rules. */
  shadowed: number;
  /** Count of rules whose predicate matched zero samples
   *  (`would_match == 0`) and NOT dead. The predicate is too
   *  narrow — refine it or drop the rule. */
  zero: number;
  /** Fall-through count carried verbatim from the report so the
   *  caller can render a secondary "X% fall-through" sub-chip
   *  without re-walking the report. */
  fallthrough: number;
  /** Pre-divided fall-through percentage rounded to one decimal so
   *  the consumer doesn't re-compute the ratio. Zero when
   *  `total_samples == 0`. */
  fallthroughPct: number;
}

/** Classify a coverage report's chain-health story. Pure helper —
 *  no I/O, no Tauri.
 *
 *  Priority chain for the `kind` field:
 *  1. `empty`     — `total_samples == 0`. Distinct from healthy
 *                   because an empty corpus has no data to assess.
 *  2. `critical`  — at least one dead rule. The chain is silently
 *                   misrouting; reorder or tighten the shadowing
 *                   rule above.
 *  3. `warn`      — at least one diagnostic (shadowed / zero) OR
 *                   fall-through above the warn threshold.
 *  4. `healthy`   — every rule fires + low fall-through. */
export function summarizeCoverageHealth(
  report: RuleCoverageReport,
  opts: { fallthroughWarnPct?: number } = {},
): CoverageHealth {
  const warnPct = opts.fallthroughWarnPct ?? 25;
  let dead = 0;
  let shadowed = 0;
  let zero = 0;
  for (const r of report.rules) {
    if (r.dead_at_position) {
      dead++;
    } else if (r.would_match === 0) {
      zero++;
    } else if (r.would_match > r.first_match) {
      shadowed++;
    }
  }
  const fallthrough = report.fallthrough;
  const total = report.total_samples;
  const rawPct = total === 0 ? 0 : (fallthrough / total) * 100;
  // Round to one decimal — finer than the chip needs but cheap and
  // useful for a future tooltip.
  const fallthroughPct = Math.round(rawPct * 10) / 10;

  if (total === 0) {
    return {
      kind: "empty",
      text: "No recent runs to assess",
      dead,
      shadowed,
      zero,
      fallthrough,
      fallthroughPct,
    };
  }

  if (dead > 0) {
    const noun = dead === 1 ? "dead rule" : "dead rules";
    // "dead rule — reorder or tighten the shadowing rule above" is
    // the actionable single-noun summary. Multiple dead reads "3
    // dead rules — reorder or tighten the shadowing rules above"
    // so the verb stays imperative either way.
    return {
      kind: "critical",
      text: `${dead} ${noun} — reorder or tighten the shadowing rules`,
      dead,
      shadowed,
      zero,
      fallthrough,
      fallthroughPct,
    };
  }

  // Warn cases — order matters for the copy: shadowed > zero >
  // high fall-through, because shadowed is the more interesting
  // signal (the rule fires sometimes; tightening it brings the
  // routing decisions into the user's intent). Zero-coverage is
  // less urgent — the rule never fires, so it's not actively
  // misrouting anything.
  if (shadowed > 0) {
    const noun = shadowed === 1 ? "rule is partially shadowed" : "rules are partially shadowed";
    return {
      kind: "warn",
      text: `${shadowed} ${noun} — reorder to recover matches`,
      dead,
      shadowed,
      zero,
      fallthrough,
      fallthroughPct,
    };
  }

  if (zero > 0) {
    const noun = zero === 1 ? "rule matches nothing" : "rules match nothing";
    return {
      kind: "warn",
      text: `${zero} ${noun} — refine the predicates or drop them`,
      dead,
      shadowed,
      zero,
      fallthrough,
      fallthroughPct,
    };
  }

  if (fallthroughPct > warnPct) {
    return {
      kind: "warn",
      text: `${fallthroughPct.toFixed(1)}% of files fall through to defaults`,
      dead,
      shadowed,
      zero,
      fallthrough,
      fallthroughPct,
    };
  }

  return {
    kind: "healthy",
    text: "Chain routing healthy",
    dead,
    shadowed,
    zero,
    fallthrough,
    fallthroughPct,
  };
}

// ─── v3.40 Slice 129 — coverage diagnostic filter (TS mirror) ────────
//
// Round 26's chain-health chip surfaces the chain-level story
// ("2 dead rules — reorder or tighten the shadowing rules"); slice
// 128 (backend) shipped a pure-data filter for narrowing the
// coverage report to one diagnostic kind. This TS mirror lets the
// editor compute the same narrowing CLIENT-SIDE without an extra
// Tauri round-trip for the common "click the chip, see only the
// dead rules" path. The wire-side Tauri command (slice 130) is for
// the export path — the rule list rendering uses the local mirror
// so it reacts instantly to a chip click.
//
// Same priority chain as `ruleCoverageDiagnostic` /
// `coverage_diagnostic_str`: dead > zero > shadowed > healthy.
// `All` is the identity filter so the UI can wire one helper for
// both filtered and unfiltered code paths.
//
// Returns a NEW RuleCoverageReport with `fallthrough` /
// `total_samples` preserved verbatim (matches the Rust primitive).
// A filtered export's fall-through accounting still names the same
// number of samples that fell through to the watch defaults.

/** Discriminator for the coverage filter. Mirrors the Rust
 *  `CoverageFilter` enum (slice 128). Mutually exclusive — exactly
 *  one filter applies at a time. */
export type CoverageDiagnosticFilter =
  | "all"
  | "dead"
  | "zero"
  | "shadowed"
  | "healthy";

/** All filter kinds in display order, for the UI's filter-chip row
 *  and the "Showing X of Y" summary's discriminated copy. Kept as
 *  `readonly` so a careless mutation can't reorder the buttons at
 *  runtime. */
export const COVERAGE_FILTER_KINDS: readonly CoverageDiagnosticFilter[] = [
  "all",
  "dead",
  "shadowed",
  "zero",
  "healthy",
] as const;

/** Diagnostic-bucket classification for one rule, matching the
 *  established priority chain. Internal helper — composed by
 *  `filterCoverageByDiagnostic` and `formatCoverageFilterSummary`.
 *  Returns the bucket the rule lives in (`"healthy"` is a real
 *  bucket here, not "no diagnostic"). */
function coverageRuleBucket(
  rule: RuleCoverage,
): "dead" | "zero" | "shadowed" | "healthy" {
  if (rule.dead_at_position) return "dead";
  if (rule.would_match === 0) return "zero";
  if (rule.would_match > rule.first_match) return "shadowed";
  return "healthy";
}

/** True iff `rule` matches `filter`. Composed from
 *  `coverageRuleBucket` so the filter and `ruleCoverageDiagnostic`
 *  share one classifier — a future change to the priority chain
 *  propagates to both without manual mirror-work. */
export function ruleMatchesCoverageFilter(
  rule: RuleCoverage,
  filter: CoverageDiagnosticFilter,
): boolean {
  if (filter === "all") return true;
  return coverageRuleBucket(rule) === filter;
}

/** Apply a `CoverageDiagnosticFilter` to a report, returning a new
 *  report with `rules` narrowed to those passing the filter and
 *  `fallthrough` / `total_samples` preserved verbatim. Pure helper —
 *  no I/O, no Tauri. Mirrors the Rust
 *  `filter_coverage_by_diagnostic` (slice 128) 1:1. */
export function filterCoverageByDiagnostic(
  report: RuleCoverageReport,
  filter: CoverageDiagnosticFilter,
): RuleCoverageReport {
  if (filter === "all") {
    // Return a shallow clone so a downstream mutation doesn't leak
    // into the source. Matches the Rust primitive's purity contract.
    return {
      rules: report.rules.slice(),
      fallthrough: report.fallthrough,
      total_samples: report.total_samples,
    };
  }
  return {
    rules: report.rules.filter((r) => ruleMatchesCoverageFilter(r, filter)),
    fallthrough: report.fallthrough,
    total_samples: report.total_samples,
  };
}

/** One-line summary for the coverage panel's "filter active" sub-line.
 *  Renders the discriminated copy:
 *
 *   - "Showing all 6 rules"                  (filter === "all")
 *   - "Showing 1 of 6 rules — dead"          (filter !== "all", 1)
 *   - "Showing 2 of 6 rules — shadowed"      (filter !== "all", N)
 *   - "Showing 0 of 6 rules — dead"          (no rules match the filter)
 *   - "Showing 0 rules"                      (rule_count === 0)
 *
 *  The trailing diagnostic tag matches the slug exactly so a user
 *  scanning across the chip + the summary + the filtered export's
 *  filename sees one consistent vocabulary.
 *
 *  Pure helper — no I/O. */
export function formatCoverageFilterSummary(
  filter: CoverageDiagnosticFilter,
  shown: number,
  total: number,
): string {
  if (total === 0) return "Showing 0 rules";
  if (filter === "all") {
    const noun = total === 1 ? "rule" : "rules";
    return `Showing all ${total} ${noun}`;
  }
  const noun = total === 1 ? "rule" : "rules";
  return `Showing ${shown} of ${total} ${noun} — ${filter}`;
}

// ─── v3.40 Slice 131 — coverage health -> filter click target ────────
//
// Round 26's chain-health chip surfaces a copy line ("2 dead rules
// — reorder or tighten the shadowing rules"); slices 128/129/130
// shipped the filter machinery. The missing piece is the bridge: a
// pure helper that, given a CoverageHealth, returns the diagnostic
// filter kind whose chip should activate on click. The UI cannot
// safely re-derive this from `health.kind` alone (which is
// "critical" / "warn" / "healthy" / "empty" — broader than the
// filter slugs) without leaking the chain-health priority chain
// into the Svelte component.
//
// This helper composes the priority chain in one place:
//
//   empty                                  -> null (chip hidden)
//   critical (dead > 0)                    -> "dead"
//   warn + shadowed > 0                    -> "shadowed"
//   warn + zero > 0                        -> "zero"
//   warn + high-fall-through (no rule kind) -> null
//                                             (no rule-level filter
//                                             can show fall-through;
//                                             the fall-through row
//                                             is a separate UI affordance)
//   healthy                                -> null (nothing to filter to)
//
// The priority chain (dead > shadowed > zero) matches
// summarizeCoverageHealth EXACTLY so a user clicking the chip
// always lands in the bucket the chip is describing. A future
// change to the chain bumps both helpers together, and the test
// suite pins the agreement.

/** Given a CoverageHealth summary, return the CoverageDiagnosticFilter
 *  kind whose chip should activate on click. Returns null when the
 *  chip is empty/healthy (nothing to drill into) or when the warn
 *  reason has no rule-level filter (high fall-through). Pure helper. */
export function coverageHealthClickTarget(
  health: CoverageHealth | null,
): CoverageDiagnosticFilter | null {
  if (!health || health.kind === "empty" || health.kind === "healthy") {
    return null;
  }
  // Same priority chain as summarizeCoverageHealth's copy generation:
  // dead > shadowed > zero > high-fall-through (no target).
  if (health.dead > 0) return "dead";
  if (health.shadowed > 0) return "shadowed";
  if (health.zero > 0) return "zero";
  // Warn reason is the high-fall-through case. No rule-level filter
  // can show fall-through samples — the fall-through ROW (the
  // synthetic last row in the coverage list) is a separate UI
  // affordance, not a diagnostic the rule filter can express. Return
  // null and let the UI handle that state explicitly.
  return null;
}

// ─── Slice 134 — dead-rule reorder planner TS mirror ─────────────────
//
// Round 27 (slices 128-132) closed the "diagnose + drill in" loop;
// slice 133 added the backend planner primitive that suggests the
// minimal reorder fix for each dead rule. This slice mirrors that
// primitive in TS so the UI can:
//
//   1. Plan client-side from the loaded coverage report (instant —
//      no IPC round-trip needed for the in-panel fix-it chip).
//   2. Apply a proposal optimistically (re-render the chain BEFORE
//      the persistence round-trip completes), and
//   3. Format the proposal's human-facing copy uniformly across the
//      pill, the confirm popover, and the toast.
//
// The Rust planner remains the authority for export-path callers
// (slice 135 wraps it as a Tauri command); the TS mirror exists so
// the in-panel render path doesn't pay an IPC tax per coverage
// refresh.

/** One reorder suggestion. Mirrors `pdf::hopper::coverage::ReorderProposal`.
 *  All counts are read verbatim from the coverage report; the planner
 *  doesn't re-derive them. */
export interface ReorderProposal {
  /** 0-based index of the dead rule in the current chain. */
  rule_index: number;
  /** User-visible name of the dead rule. */
  rule_name: string;
  /** 0-based index to move the rule TO. Always strictly less than
   *  `rule_index`. */
  target_index: number;
  /** User-visible name of the rule at `target_index`. Empty when the
   *  planner picked target_index = 0 as the conservative fallback
   *  (no `Always` shadower identified) — the UI gates on the empty
   *  string to render a generic "Move to the front of the chain"
   *  copy rather than naming a wrong rule. */
  shadowing_rule_name: string;
  /** Number of samples the dead rule would route AFTER the move —
   *  equal to `would_match` from the coverage row. */
  samples_recovered: number;
}

/** Plan minimal-reorder fixes for every dead rule in `report`.
 *  Mirrors `pdf::hopper::coverage::plan_dead_rule_reorder` 1:1.
 *
 *  Per-proposal heuristic for `target_index`:
 *  - If any rule in `rules[0..rule_index]` has predicate `kind ===
 *    "always"`, `target_index` = index of the EARLIEST such rule.
 *    `shadowing_rule_name` is that rule's name.
 *  - Otherwise `target_index = 0` and `shadowing_rule_name` is empty.
 *
 *  Defensive: dead rows whose `rule_index >= rules.length` are
 *  skipped silently (stale-report safety). Pure helper. */
export function planDeadRuleReorder(
  rules: Rule[],
  report: RuleCoverageReport,
): ReorderProposal[] {
  if (rules.length === 0) return [];
  const out: ReorderProposal[] = [];
  for (const cov of report.rules) {
    if (!cov.dead_at_position) continue;
    if (cov.index >= rules.length) continue;
    // Find the EARLIEST Always in [0..rule_index).
    let earliestAlways: number = -1;
    for (let j = 0; j < cov.index; j++) {
      if (rules[j].predicate.kind === "always") {
        earliestAlways = j;
        break;
      }
    }
    const target_index = earliestAlways >= 0 ? earliestAlways : 0;
    const shadowing_rule_name = earliestAlways >= 0 ? rules[earliestAlways].name : "";
    out.push({
      rule_index: cov.index,
      rule_name: cov.name,
      target_index,
      shadowing_rule_name,
      samples_recovered: cov.would_match,
    });
  }
  return out;
}

/** Apply a `ReorderProposal` to a `Rule[]`, returning a NEW array
 *  with the rule at `rule_index` lifted out and re-inserted at
 *  `target_index`. Pure helper — the source array is not mutated;
 *  every rule object is the same reference (shallow copy of the
 *  array, deep references shared) so a downstream renderer's
 *  identity comparisons stay stable on the unmoved rows.
 *
 *  Invariants the planner provides + this helper relies on:
 *  - `target_index < rule_index` (move-earlier only).
 *  - `rule_index < rules.length` (in-range).
 *
 *  Out-of-range indices return the source array UNCHANGED rather
 *  than throwing — the planner skips stale rows, but a caller
 *  hand-rolling a proposal shouldn't crash the UI either. */
export function applyReorderProposal(
  rules: Rule[],
  proposal: ReorderProposal,
): Rule[] {
  const { rule_index, target_index } = proposal;
  if (rule_index < 0 || rule_index >= rules.length) return rules;
  if (target_index < 0 || target_index >= rules.length) return rules;
  if (target_index >= rule_index) return rules;
  const next = rules.slice();
  const [moved] = next.splice(rule_index, 1);
  next.splice(target_index, 0, moved);
  return next;
}

/** Human-facing copy for a `ReorderProposal`. Used by the fix-it
 *  pill's title attribute, the confirm popover's body, and the
 *  applied-toast suffix. Discriminated on whether the proposal
 *  has a named shadower:
 *
 *    With shadower:    "Move 'Tax' before 'Catch-all' to recover 3 matches"
 *    Without shadower: "Move 'Tax' to the front of the chain to recover 3 matches"
 *    Zero recovered:   "Move 'Tax' before 'Catch-all' (predicate now matches 0 samples)"
 *
 *  Plural-aware on the match count noun ("1 match" / "3 matches").
 *  Empty rule name falls back to a positional label ("Rule #4") so
 *  the copy never reads "Move '' before 'Catch-all'". */
export function formatReorderProposal(proposal: ReorderProposal): string {
  const ruleLabel = proposal.rule_name.trim() || `Rule #${proposal.rule_index + 1}`;
  const shadower = proposal.shadowing_rule_name.trim();
  const target = shadower ? `before '${shadower}'` : "to the front of the chain";
  if (proposal.samples_recovered === 0) {
    return `Move '${ruleLabel}' ${target} (predicate now matches 0 samples)`;
  }
  const n = proposal.samples_recovered;
  const noun = n === 1 ? "match" : "matches";
  return `Move '${ruleLabel}' ${target} to recover ${n} ${noun}`;
}

// ─── Slice 136 — reorder-proposal confidence classifier ──────────────
//
// The planner (slice 133) produces ONE proposal per dead rule, but
// not every proposal has equal evidence behind it. Three quality
// levels matter to the user:
//
//   high   — the planner identified a NAMED Always shadower AND the
//            proposed move strictly improves the chain (at least 1
//            sample recovered). "Move 'Tax' before 'Catch-all' to
//            recover 3 matches" reads exactly like a recipe — green
//            chip, the user clicks with confidence.
//   medium — the planner identified an Always shadower BUT the
//            dead rule recovers ZERO samples (predicate too narrow
//            for the current corpus, even though it's structurally
//            shadowed). The reorder is still STRUCTURALLY correct
//            — the rule will fire on future matching files — but
//            the user's gain is theoretical, not retrospective.
//            Orange chip, hesitate before clicking.
//   low    — the planner fell back to target_index = 0 with an
//            empty shadowing_rule_name (no Always identified). The
//            move IS guaranteed-correct in the sense of being
//            move-earlier-only, but it's MORE AGGRESSIVE than
//            necessary (the dead rule jumps the whole chain
//            instead of just the shadower) and may surprise a user
//            who's manually ordered the chain. Muted chip, the
//            user reads the copy carefully before clicking.
//
// The classifier is composed from the proposal alone — no need to
// inspect the report or the rules again. Pure helper; the UI gates
// the pill's color tint and the confirm popover's tone on this
// kind.

/** Confidence tier for a `ReorderProposal`. Drives the fix-it pill's
 *  color treatment (green/orange/muted) and the confirm popover's
 *  copy tone. */
export type ReorderProposalConfidence = "high" | "medium" | "low";

/** Classify the confidence behind a `ReorderProposal`. Pure helper —
 *  no I/O, no Tauri.
 *
 *  - `high`   — named shadower AND samples_recovered > 0.
 *  - `medium` — named shadower but samples_recovered == 0.
 *  - `low`    — no named shadower (fallback to target = 0). */
export function reorderProposalConfidence(
  proposal: ReorderProposal,
): ReorderProposalConfidence {
  const hasShadower = proposal.shadowing_rule_name.trim().length > 0;
  if (!hasShadower) return "low";
  if (proposal.samples_recovered === 0) return "medium";
  return "high";
}

/** Filter a list of `ReorderProposal`s to those with confidence
 *  >= `minConfidence`. Convenience for the UI which wants a "show
 *  only confident fixes" toggle. Pure helper.
 *
 *  Order of priority (high to low): high > medium > low. A min of
 *  "low" returns every proposal; "high" returns only the green ones. */
export function filterProposalsByConfidence(
  proposals: ReorderProposal[],
  minConfidence: ReorderProposalConfidence,
): ReorderProposal[] {
  const rank: Record<ReorderProposalConfidence, number> = {
    high: 3,
    medium: 2,
    low: 1,
  };
  const min = rank[minConfidence];
  return proposals.filter((p) => rank[reorderProposalConfidence(p)] >= min);
}

/** One-line summary copy describing the confidence tier — used by
 *  the confirm popover's tone subline + the no-proposals empty
 *  state. Discriminated and self-contained so the UI doesn't have
 *  to gate copy on the tier string itself. */
export function describeReorderConfidence(
  confidence: ReorderProposalConfidence,
): string {
  switch (confidence) {
    case "high":
      return "Confident fix — the dead rule will fire on its current matches";
    case "medium":
      return "Structurally correct — the rule will fire on future matching files";
    case "low":
      return "Aggressive fix — the dead rule jumps to the front of the chain";
  }
}

// ─── Slice 139 — batch reorder applier (TS mirror of slice 138) ──────
//
// Round 28 shipped the per-row "Fix it" pill that applies ONE
// proposal at a time. The natural next-layer affordance is "Fix
// ALL dead rules" — apply every planner proposal in one click.
//
// The applier walks the proposal list in INPUT order, resolving
// the source rule by NAME at each step against the running chain
// (NOT by index — the index drift from prior moves makes the
// planner's recorded indices stale halfway through the batch).
// The target is resolved by shadower name when present so a
// proposal lands "before the named shadower" even if that
// shadower has itself moved; fallback to target = 0 when the
// shadower name is empty (planner's fallback) or the shadower
// drifted out of the chain.
//
// Mirrors `pdf::hopper::coverage::apply_reorder_proposals_batch`
// 1:1. The wire shapes (BatchReorderSkipReason, SkippedProposal,
// BatchReorderOutcome) use snake_case to match the Rust side; a
// browser-mode caller goes through this helper directly, a
// Tauri-mode caller round-trips through `slabHopperBatchReorderDeadRules`
// (slice 140) and reads the same wire shape.

/** Why a proposal was skipped during a batch apply. Mirrors
 *  `pdf::hopper::coverage::BatchReorderSkipReason`. */
export type BatchReorderSkipReason =
  | { kind: "rule_not_found" }
  | { kind: "already_earlier" };

/** Stable singleton constructors for the two reason variants — saves
 *  per-call object literals when comparing reasons in the UI. */
export const RULE_NOT_FOUND: BatchReorderSkipReason = { kind: "rule_not_found" } as const;
export const ALREADY_EARLIER: BatchReorderSkipReason = { kind: "already_earlier" } as const;

/** A proposal that was not applied by `applyReorderProposalsBatch`,
 *  carried alongside the new chain so the UI can render an audit
 *  breakdown. Mirrors `pdf::hopper::coverage::SkippedProposal`. */
export interface SkippedProposal {
  /** 0-based index into the INPUT proposal list (NOT the chain). */
  input_index: number;
  /** Echo of the proposal that was skipped. */
  proposal: ReorderProposal;
  /** Why the proposal was skipped. */
  reason: BatchReorderSkipReason;
}

/** Outcome of a batch reorder application. Mirrors
 *  `pdf::hopper::coverage::BatchReorderOutcome`. */
export interface BatchReorderOutcome {
  /** The chain AFTER every applied proposal landed. */
  rules: Rule[];
  /** Input-order indices of proposals that landed. */
  applied: number[];
  /** Proposals that did not land, with reason — in input order
   *  (`skipped[i].input_index` is strictly monotonic). */
  skipped: SkippedProposal[];
  /** Total recovered samples across applied proposals. Pre-summed
   *  so the UI doesn't have to re-walk the slice for the toast. */
  total_recovered: number;
}

/** Apply every proposal in `proposals` to `rules` in input order,
 *  resolving the source rule by NAME at each step. Pure helper.
 *
 *  Conservation invariant: `applied.length + skipped.length ===
 *  proposals.length` — every input proposal lands in exactly one
 *  bucket.
 *
 *  Returns a NEW Rule[] (shallow clone of the source array); the
 *  source array is never mutated. Per-rule object identity is
 *  shared with the source (no deep clone). */
export function applyReorderProposalsBatch(
  rules: Rule[],
  proposals: ReorderProposal[],
): BatchReorderOutcome {
  const chain: Rule[] = rules.slice();
  const applied: number[] = [];
  const skipped: SkippedProposal[] = [];
  let total_recovered = 0;
  for (let i = 0; i < proposals.length; i++) {
    const p = proposals[i];
    // Resolve source by name in the CURRENT chain.
    const srcIdx = chain.findIndex((r) => r.name === p.rule_name);
    if (srcIdx < 0) {
      skipped.push({
        input_index: i,
        proposal: p,
        reason: RULE_NOT_FOUND,
      });
      continue;
    }
    // Resolve target by shadower name when possible.
    let target: number;
    if (p.shadowing_rule_name.length > 0) {
      const sIdx = chain.findIndex((r) => r.name === p.shadowing_rule_name);
      target = sIdx >= 0 ? sIdx : 0;
    } else {
      target = 0;
    }
    if (target >= srcIdx) {
      skipped.push({
        input_index: i,
        proposal: p,
        reason: ALREADY_EARLIER,
      });
      continue;
    }
    const [moved] = chain.splice(srcIdx, 1);
    chain.splice(target, 0, moved);
    applied.push(i);
    total_recovered += p.samples_recovered;
  }
  return {
    rules: chain,
    applied,
    skipped,
    total_recovered,
  };
}

/** Human-facing summary copy for a `BatchReorderOutcome`. Used by
 *  the batch-apply confirm popover's preview line + the post-apply
 *  toast. Discriminated on the applied/skipped/recovered counts:
 *
 *    All applied + recovered>0: "Fixed 3 rules — recovered 12 matches"
 *    All applied + recovered=0: "Fixed 3 rules"
 *    Partial:                   "Fixed 2 of 3 rules — recovered 5 matches (1 skipped)"
 *    Nothing applied:           "No rules fixed (3 skipped)"
 *    Empty input:               "No dead rules to fix"
 *
 *  Plural-aware on both nouns (rules/matches) and the parenthetical
 *  count. */
export function summarizeBatchReorderOutcome(outcome: BatchReorderOutcome): string {
  const total = outcome.applied.length + outcome.skipped.length;
  if (total === 0) return "No dead rules to fix";
  const a = outcome.applied.length;
  const s = outcome.skipped.length;
  const rec = outcome.total_recovered;
  const rulesNoun = (n: number) => (n === 1 ? "rule" : "rules");
  const matchesNoun = (n: number) => (n === 1 ? "match" : "matches");
  if (a === 0) {
    return `No rules fixed (${s} skipped)`;
  }
  if (s === 0) {
    if (rec === 0) return `Fixed ${a} ${rulesNoun(a)}`;
    return `Fixed ${a} ${rulesNoun(a)} — recovered ${rec} ${matchesNoun(rec)}`;
  }
  // Partial.
  if (rec === 0) {
    return `Fixed ${a} of ${total} ${rulesNoun(total)} (${s} skipped)`;
  }
  return `Fixed ${a} of ${total} ${rulesNoun(total)} — recovered ${rec} ${matchesNoun(rec)} (${s} skipped)`;
}

/** Human-facing copy for a `BatchReorderSkipReason`. Used by the
 *  per-proposal skipped-list entry in the confirm popover so the
 *  user knows WHY each skipped proposal isn't being applied. */
export function describeSkipReason(reason: BatchReorderSkipReason): string {
  switch (reason.kind) {
    case "rule_not_found":
      return "rule no longer in chain";
    case "already_earlier":
      return "rule already earlier than target";
  }
}

// ─── Slice 141 — batch-confidence bridge helper ─────────────────────
//
// Round 28 shipped per-row fix-it pills that color-code on the
// individual proposal's confidence. The "Fix all" path applies
// EVERY proposal in one click; the button needs ONE color tone
// summarising the batch's mixed evidence.
//
// The user is implicitly accepting ALL proposals when they click
// "Fix all", so the button's color must reflect the WORST tier
// present (worst-case posture — if any low-confidence proposal is
// in the batch, the user must be informed before clicking). A
// batch of one high + one low is NOT a green batch.
//
// The popover also needs a per-tier BREAKDOWN copy ("3 fixes — 1
// high, 2 medium") so a user can decide whether to click "Fix all"
// or open each row individually for finer control. This bridge
// also returns a discriminated copy renderer.
//
// All helpers are pure — composed from the proposal list alone;
// the UI gates the button's color tint + the popover's per-tier
// list on these.

/** Worst (lowest) confidence tier across `proposals`. Returns null
 *  for an empty list (no proposals -> no color). Used to color the
 *  "Fix all" button: even ONE low-confidence proposal demotes the
 *  whole button to the muted tier.
 *
 *  Priority order (worst to best): low > medium > high. */
export function worstReorderConfidence(
  proposals: ReorderProposal[],
): ReorderProposalConfidence | null {
  if (proposals.length === 0) return null;
  // Walk once: short-circuit on "low" since it's the worst.
  let worst: ReorderProposalConfidence = "high";
  const rank: Record<ReorderProposalConfidence, number> = {
    high: 3,
    medium: 2,
    low: 1,
  };
  for (const p of proposals) {
    const tier = reorderProposalConfidence(p);
    if (rank[tier] < rank[worst]) worst = tier;
    if (worst === "low") break;
  }
  return worst;
}

/** Per-tier count breakdown of a proposal list. The `total` field
 *  is pre-summed so the consumer doesn't have to add the three
 *  buckets back together. Empty input returns all-zero counts +
 *  total = 0. */
export interface ProposalTierBreakdown {
  high: number;
  medium: number;
  low: number;
  total: number;
}

/** Count proposals by confidence tier. Pure helper. */
export function summarizeProposalTierBreakdown(
  proposals: ReorderProposal[],
): ProposalTierBreakdown {
  let high = 0;
  let medium = 0;
  let low = 0;
  for (const p of proposals) {
    const tier = reorderProposalConfidence(p);
    if (tier === "high") high++;
    else if (tier === "medium") medium++;
    else low++;
  }
  return { high, medium, low, total: high + medium + low };
}

/** Human-facing breakdown copy. Discriminated on which tiers are
 *  represented so the copy reads naturally:
 *
 *    1 fix:                  "1 fix — high"
 *    2 fixes, one tier:      "2 fixes — high"
 *    2 fixes, two tiers:     "2 fixes — 1 high, 1 medium"
 *    3 fixes, three tiers:   "3 fixes — 1 high, 1 medium, 1 low"
 *    Empty:                  "No fixes"
 *
 *  Order in the comma list mirrors the priority chain: high first,
 *  then medium, then low. Empty tiers are SKIPPED from the
 *  enumeration. Plural-aware on "fix"/"fixes". */
export function describeProposalBatch(proposals: ReorderProposal[]): string {
  const b = summarizeProposalTierBreakdown(proposals);
  if (b.total === 0) return "No fixes";
  const fixNoun = b.total === 1 ? "fix" : "fixes";
  // Single-tier shortcut: avoid "1 fix — 1 high" redundancy when
  // there's only one tier represented.
  const tiersPresent = (b.high > 0 ? 1 : 0) + (b.medium > 0 ? 1 : 0) + (b.low > 0 ? 1 : 0);
  if (tiersPresent === 1) {
    const tier: ReorderProposalConfidence = b.high > 0 ? "high" : b.medium > 0 ? "medium" : "low";
    return `${b.total} ${fixNoun} — ${tier}`;
  }
  const parts: string[] = [];
  if (b.high > 0) parts.push(`${b.high} high`);
  if (b.medium > 0) parts.push(`${b.medium} medium`);
  if (b.low > 0) parts.push(`${b.low} low`);
  return `${b.total} ${fixNoun} — ${parts.join(", ")}`;
}

// ─── Slice 143/144 — reorder-effect summary primitive (round-30) ─────
//
// Round 30's load-bearing primitive for the undo path. Given a chain
// BEFORE a fix-it / fix-all reorder and a chain AFTER, produce a
// structural summary of what changed: which rules moved (by name,
// with from/to positions), which were added or removed, and whether
// the AFTER chain is a PERMUTATION of BEFORE.
//
// The permutation flag is the gate for undo's staleness check. Undo
// can only safely revert when the snapshot's chain hasn't drifted
// in the rule set itself (no add / remove / rename between the
// reorder and the undo click). When permutation is false, the undo
// affordance falls back to a "snapshot stale" disabled state rather
// than silently dropping or duplicating rules.
//
// Mirrors `pdf::hopper::coverage::summarize_reorder_effect` 1:1. The
// wire shapes use snake_case to match Rust; a browser-mode caller
// goes through this helper directly, a Tauri-mode caller round-trips
// through `slabHopperSummarizeReorderEffect` (slice 145).

/** One rule that moved positions between two chains. Both indices
 *  are 0-based into their respective chains. `from === to` is NOT
 *  represented — only genuinely-moved rules appear. Mirrors
 *  `pdf::hopper::coverage::ReorderMove`. */
export interface ReorderMove {
  /** Display name of the rule. By-name resolution is the canonical
   *  identity throughout the reorder pipeline. */
  rule_name: string;
  /** Position in the BEFORE chain. */
  from_index: number;
  /** Position in the AFTER chain. */
  to_index: number;
}

/** Structural summary of how an AFTER chain differs from a BEFORE
 *  chain. Mirrors `pdf::hopper::coverage::ReorderEffect`. */
export interface ReorderEffect {
  /** Rules whose index changed between BEFORE and AFTER. In
   *  AFTER-chain order (ascending `to_index`). A rule that's purely
   *  added or removed does NOT appear here. */
  moved: ReorderMove[];
  /** Rules present in AFTER but absent from BEFORE — by name. In
   *  AFTER-chain order. */
  added: string[];
  /** Rules present in BEFORE but absent from AFTER — by name. In
   *  BEFORE-chain order. */
  removed: string[];
  /** True iff AFTER is a permutation of BEFORE: same length AND
   *  same multiset of rule names. The load-bearing signal for
   *  undo's staleness check. */
  is_permutation: boolean;
}

/** Summarise the structural difference between two chains by NAME.
 *  Pure helper, mirrors the Rust primitive 1:1. */
export function summarizeReorderEffect(
  before: Rule[],
  after: Rule[],
): ReorderEffect {
  // First-occurrence index map for each chain. By-name resolution
  // matches the rest of the reorder pipeline.
  const beforeIdx = new Map<string, number>();
  for (let i = 0; i < before.length; i++) {
    if (!beforeIdx.has(before[i].name)) beforeIdx.set(before[i].name, i);
  }
  const afterIdx = new Map<string, number>();
  for (let i = 0; i < after.length; i++) {
    if (!afterIdx.has(after[i].name)) afterIdx.set(after[i].name, i);
  }

  // Moved: rules present in BOTH whose first-occurrence indices
  // differ. Walk the AFTER chain so the output is in AFTER order.
  const moved: ReorderMove[] = [];
  for (let toIndex = 0; toIndex < after.length; toIndex++) {
    const name = after[toIndex].name;
    if (afterIdx.get(name) !== toIndex) continue;
    const fromIndex = beforeIdx.get(name);
    if (fromIndex === undefined) continue;
    if (fromIndex !== toIndex) {
      moved.push({ rule_name: name, from_index: fromIndex, to_index: toIndex });
    }
  }

  // Added: in AFTER (first occurrence) but not in BEFORE.
  const added: string[] = [];
  for (let i = 0; i < after.length; i++) {
    const name = after[i].name;
    if (afterIdx.get(name) !== i) continue;
    if (!beforeIdx.has(name)) added.push(name);
  }

  // Removed: in BEFORE (first occurrence) but not in AFTER.
  const removed: string[] = [];
  for (let i = 0; i < before.length; i++) {
    const name = before[i].name;
    if (beforeIdx.get(name) !== i) continue;
    if (!afterIdx.has(name)) removed.push(name);
  }

  const is_permutation =
    added.length === 0 && removed.length === 0 && before.length === after.length;

  return { moved, added, removed, is_permutation };
}

/** Human-facing copy for a `ReorderEffect`. Discriminated on the
 *  three buckets so the copy reads naturally for the undo button's
 *  tooltip / popover header:
 *
 *    All buckets empty:      "No changes to undo"
 *    Pure moves, 1 rule:     "Move 1 rule back"
 *    Pure moves, N rules:    "Move 3 rules back"
 *    Added-only:             "Drop 1 added rule" / "Drop N added rules"
 *    Removed-only:           "Restore 1 removed rule" / "Restore N..."
 *    Mixed (add + remove):   "Restore N removed, drop M added"
 *    Mixed (move + add/rem): "Move N, restore K, drop M"
 *
 *  Plural-aware on "rule" / "rules". The copy is from the UNDO
 *  perspective: the effect describes what happened, and the copy
 *  describes what undo would do (move BACK, restore, drop). */
export function describeReorderEffect(effect: ReorderEffect): string {
  const m = effect.moved.length;
  const a = effect.added.length;
  const r = effect.removed.length;
  if (m === 0 && a === 0 && r === 0) return "No changes to undo";
  const ruleNoun = (n: number) => (n === 1 ? "rule" : "rules");
  // Pure moves — the fix-it / fix-all happy path.
  if (a === 0 && r === 0) {
    return `Move ${m} ${ruleNoun(m)} back`;
  }
  // Pure added — undo would drop those rules.
  if (m === 0 && r === 0) {
    return `Drop ${a} added ${ruleNoun(a)}`;
  }
  // Pure removed — undo would restore them.
  if (m === 0 && a === 0) {
    return `Restore ${r} removed ${ruleNoun(r)}`;
  }
  // Mixed — enumerate present buckets.
  const parts: string[] = [];
  if (m > 0) parts.push(`Move ${m}`);
  if (r > 0) parts.push(`restore ${r} removed`);
  if (a > 0) parts.push(`drop ${a} added`);
  return parts.join(", ");
}

/** True iff the effect represents a no-op — BEFORE and AFTER are
 *  pointwise-equal. Convenience predicate for the undo gate: a
 *  no-op effect means the snapshot matches the current chain
 *  exactly, so undo is unnecessary (the toast can hide the button).
 *
 *  This is NOT the same as `is_permutation === false`: an empty
 *  effect is a no-op AND a (trivial) permutation. */
export function isReorderEffectNoop(effect: ReorderEffect): boolean {
  return (
    effect.moved.length === 0 &&
    effect.added.length === 0 &&
    effect.removed.length === 0
  );
}

// ─── Slice 146 — undo-entry bridge primitive (round-30) ──────────────
//
// Bridges slices 143-145's structural primitives to the UI's undo
// affordance. A `ReorderUndoEntry` is the atomic record the UI
// stashes after every fix-it / fix-all apply — a snapshot of the
// rules chain BEFORE the apply landed, plus a label, a timestamp,
// and a structural breadcrumb (`appliedEffect`) describing what
// just changed.
//
// The bridge layer exists because the UI has two distinct concerns
// that need to compose cleanly without leaking into the Svelte
// component:
//
// 1. STALENESS GATE. Between the fix-it apply and the undo click
//    the user might manually edit the chain (add a rule, rename
//    one, drag-reorder). Reverting blindly to the snapshot would
//    silently drop the added rule or rename. The gate composes
//    summarizeReorderEffect(snapshot, currentChain) and refuses
//    when the effect is NOT a pure permutation — the snapshot's
//    rule set no longer matches the current rule set.
//
// 2. UNDO COPY. The button copy "Undo · Move 3 rules back" needs
//    a count derived from the GATE'S effect (not the apply-time
//    effect) so a user who manually moved one rule between apply
//    and undo sees the right count. The bridge composes the
//    copy from the live computeUndoStatus result.
//
// All bridge helpers are pure — no Svelte runes, no Tauri. The
// UI slice (147) holds the $state and the button.

/** One stashed undo entry: a snapshot of the chain BEFORE a
 *  reorder landed, plus enough context for the UI to render the
 *  undo button copy + the audit log. Carried in $state by the UI
 *  slice; created via `captureUndoEntry`. */
export interface ReorderUndoEntry {
  /** Snapshot of `rules` BEFORE the reorder. Reverting = handing
   *  this array back through `slabHopperSetRules`. */
  snapshot: Rule[];
  /** Human-facing label for the action being undone, sourced from
   *  the call site (e.g. "fix-all" / "fix-it: Tax"). Used by the
   *  undo button's accessible label + the toast suffix. */
  label: string;
  /** Unix-ms timestamp of when the snapshot was captured. Used by
   *  the UI's auto-expiry timer (undo affordance dwells with the
   *  toast for 4s) and by audit logging. */
  capturedAt: number;
  /** Structural breadcrumb describing what the reorder DID, in
   *  AFTER-vs-BEFORE terms. Pre-computed at capture time so a
   *  scripted-audit consumer can read the entry without re-running
   *  the diff. Not the same as the live staleness check (which
   *  re-runs against the CURRENT chain at click time). */
  appliedEffect: ReorderEffect;
}

/** Capture a new undo entry for an apply-time `before` + `after`
 *  chain pair. The snapshot is `before` (the chain to revert TO);
 *  the applied effect is summarized once so the entry carries its
 *  own breadcrumb.
 *
 *  Pure helper — takes a `now` injectable so tests don't race the
 *  wall clock. Defaults to `Date.now()` at call site. */
export function captureUndoEntry(
  before: Rule[],
  after: Rule[],
  label: string,
  now: number = Date.now(),
): ReorderUndoEntry {
  return {
    snapshot: before.slice(),
    label,
    capturedAt: now,
    appliedEffect: summarizeReorderEffect(before, after),
  };
}

/** Discriminated status of an undo entry against the LIVE chain.
 *  Computed at click time so a user who edited rules between apply
 *  and undo gets the right behaviour. */
export type ReorderUndoStatus =
  /** Chain is identical to the snapshot — nothing to undo. The UI
   *  hides the button (or disables it). */
  | { kind: "noop" }
  /** Chain has DRIFTED away from a clean permutation of the
   *  snapshot — undo would silently drop / duplicate / rename
   *  rules. The UI shows a disabled "Undo (stale)" badge with the
   *  `reason` as tooltip rather than the live undo button. */
  | { kind: "stale"; reason: string; effect: ReorderEffect }
  /** Chain is a clean permutation of the snapshot — undo is
   *  ready. `effect` describes what undo will DO (move N rules
   *  back, in revert direction). */
  | { kind: "ready"; effect: ReorderEffect };

/** Compose an undo status from an entry + the live chain.
 *
 *  Algorithm:
 *    1. Diff the snapshot against the current chain.
 *    2. If the diff is a no-op -> `noop`.
 *    3. If the diff is NOT a permutation -> `stale` with a
 *       human-facing reason ("rules added since fix-all" /
 *       "rules removed since fix-all" / "rules renamed since
 *       fix-all"). The reason is a SINGLE breadcrumb (the dominant
 *       bucket) so the tooltip stays short; the underlying
 *       `effect` field carries the full breakdown for the rare
 *       caller that wants it.
 *    4. Otherwise -> `ready` with the inverse-direction effect
 *       (the effect of moving FROM the live chain BACK to the
 *       snapshot, which is what undo will produce).
 *
 *  Pure helper — composed entirely from the entry + the live chain. */
export function computeUndoStatus(
  entry: ReorderUndoEntry,
  current: Rule[],
): ReorderUndoStatus {
  // The effect from current -> snapshot is what undo will DO; we
  // want copy / staleness reasoning from THIS direction so the
  // user reads "Move 3 rules back" rather than "Move 3 rules
  // forward".
  const undoDirection = summarizeReorderEffect(current, entry.snapshot);
  if (isReorderEffectNoop(undoDirection)) {
    return { kind: "noop" };
  }
  if (!undoDirection.is_permutation) {
    // Pick the dominant drift bucket for the short reason string.
    // The undoDirection is FROM current TO snapshot, so:
    //   undoDirection.added   = rules in SNAPSHOT but not CURRENT
    //                         = rules the user REMOVED since apply
    //   undoDirection.removed = rules in CURRENT but not SNAPSHOT
    //                         = rules the user ADDED since apply
    const userAdded = undoDirection.removed.length;
    const userRemoved = undoDirection.added.length;
    const isPlural = (n: number) => (n === 1 ? "" : "s");
    let reason: string;
    if (userAdded > 0 && userRemoved > 0) {
      // Mixed: dominant bucket wins; tie -> renamed framing.
      if (userAdded === userRemoved) {
        reason = `${userAdded} rule${isPlural(userAdded)} renamed since ${entry.label}`;
      } else if (userAdded > userRemoved) {
        reason = `${userAdded} rule${isPlural(userAdded)} added since ${entry.label}`;
      } else {
        reason = `${userRemoved} rule${isPlural(userRemoved)} removed since ${entry.label}`;
      }
    } else if (userAdded > 0) {
      reason = `${userAdded} rule${isPlural(userAdded)} added since ${entry.label}`;
    } else {
      reason = `${userRemoved} rule${isPlural(userRemoved)} removed since ${entry.label}`;
    }
    return { kind: "stale", reason, effect: undoDirection };
  }
  return { kind: "ready", effect: undoDirection };
}

/** Human-facing copy for an undo status — the button label + the
 *  short tooltip suffix. Discriminated on the status kind:
 *
 *    noop:    "Nothing to undo"
 *    stale:   "Undo unavailable — <reason>"
 *    ready:   "Undo · Move N rules back"
 *
 *  Pure helper — composes with `describeReorderEffect` to keep the
 *  copy consistent with the rest of the round-30 vocabulary. */
export function describeUndoStatus(status: ReorderUndoStatus): string {
  switch (status.kind) {
    case "noop":
      return "Nothing to undo";
    case "stale":
      return `Undo unavailable — ${status.reason}`;
    case "ready":
      return `Undo · ${describeReorderEffect(status.effect)}`;
  }
}

// ─── Slice 149 — undo-ring summary (round-31) ────────────────────────
//
// Round 30 shipped single-entry undo (one most-recent snapshot at a
// time; a second fix-it would overwrite the snapshot). Round 31
// promotes that to a bounded ring — the UI keeps up to
// UNDO_RING_CAPACITY (slice 152, currently 5) most-recent entries,
// and the user can cascade undos through the ring.
//
// This module owns the SUMMARY view — the compact, snapshot-free
// shape an audit / script consumer reads. The full `ReorderUndoEntry`
// (with its `snapshot: Rule[]`) lives in the UI's $state. The
// summary carries only `label` + `captured_at_ms` + `applied_effect`
// per entry, plus the ring's `capacity` + `full` flag, so the wire
// payload stays small enough to log without bloating disk.
//
// Mirrors `pdf::hopper::coverage::summarize_undo_ring` 1:1. Wire
// shape uses snake_case to match Rust; a browser-mode caller goes
// through this helper directly, a Tauri-mode caller round-trips
// through `slabHopperSummarizeUndoRing` (slice 150).

/** One entry in the undo ring summary — a compact, snapshot-free
 *  view of a stashed undo state. Mirrors Rust `UndoEntrySummary`.
 *
 *  The UI's full `ReorderUndoEntry` (with `snapshot: Rule[]`) lives
 *  in $state and is what `applyUndo` reverts through. This summary
 *  is the audit-friendly projection — it's what a CLI / cron
 *  consumer reads. Wire-shape uses snake_case to match Rust serde
 *  defaults; do NOT rename to camelCase or the round-trip breaks. */
export interface UndoEntrySummary {
  /** Human-facing label from the apply call site (`"fix-all"` /
   *  `"fix-it: Tax"`). */
  label: string;
  /** Unix-ms timestamp of when the entry was captured. Signed in
   *  the wire shape so a test injectable can use 0 / negative values
   *  without serde refusing; the UI never sends < 0 in practice. */
  captured_at_ms: number;
  /** Structural breadcrumb describing what the reorder DID, in
   *  AFTER-vs-BEFORE terms. Pre-computed at capture time. */
  applied_effect: ReorderEffect;
}

/** Structural summary of the entire ring — entries (oldest-trimmed
 *  to capacity) plus capacity / full metadata. Mirrors Rust
 *  `UndoRingSummary`. */
export interface UndoRingSummary {
  /** Entries OLDEST-FIRST after trimming. `entries[0]` is the next
   *  to evict; `entries[entries.length - 1]` is the most-recent
   *  capture (the UI's default undo target). */
  entries: UndoEntrySummary[];
  /** Configured capacity — `UNDO_RING_CAPACITY` in the UI. */
  capacity: number;
  /** True iff `entries.length === capacity` (next push evicts
   *  oldest). */
  full: boolean;
}

/** Summarise a list of undo entries against a ring capacity. 1:1
 *  mirror of Rust `summarize_undo_ring`. Algorithm:
 *
 *    1. If capacity === 0 -> empty entries, full = true.
 *    2. If entries.length > capacity -> trim OLDEST (keep last
 *       `capacity` entries).
 *    3. Otherwise pass entries through.
 *    4. full = trimmed.length === capacity.
 *
 *  Pure helper — does NOT mutate the input array. */
export function summarizeUndoRing(
  entries: UndoEntrySummary[],
  capacity: number,
): UndoRingSummary {
  if (capacity <= 0) {
    return { entries: [], capacity: 0, full: true };
  }
  const kept =
    entries.length > capacity
      ? entries.slice(entries.length - capacity)
      : entries.slice();
  return {
    entries: kept,
    capacity,
    full: kept.length === capacity,
  };
}

/** Human-facing summary of the ring state. Discriminated copy:
 *
 *    Empty:                  "No undo history"
 *    1 entry:                "1 undo step"
 *    N entries under cap:    "3 undo steps (oldest: fix-all)"
 *    Full ring:              "5 undo steps — at capacity"
 *
 *  Plural-aware on "step" / "steps". The "oldest:" suffix is shown
 *  only when there are more than one entries (a single entry IS
 *  the oldest AND newest; saying "oldest:" is redundant) and the
 *  ring is NOT full (the at-capacity copy already telegraphs that
 *  the next push evicts the oldest). The label is the OLDEST
 *  entry's label so the user sees which action will be lost first
 *  when the ring fills. */
export function describeUndoRingSummary(summary: UndoRingSummary): string {
  const n = summary.entries.length;
  if (n === 0) return "No undo history";
  const stepNoun = n === 1 ? "step" : "steps";
  if (summary.full) {
    return `${n} undo ${stepNoun} — at capacity`;
  }
  if (n === 1) {
    return `1 undo step`;
  }
  return `${n} undo ${stepNoun} (oldest: ${summary.entries[0].label})`;
}

/** True iff the ring is at capacity (next push will evict the
 *  oldest entry). Convenience predicate for the UI's counter chip
 *  styling — at-capacity rings render with a darker tint. */
export function isUndoRingFull(summary: UndoRingSummary): boolean {
  return summary.full;
}

// ─── Slice 154 — undo-ring jump-plan summary (round-32) ──────────────
//
// Round 31 shipped a cascade ring + a "Step N of M" chip surfacing
// how many cascading undos are queued. The cascade button always
// targets the NEWEST ready entry — a user with a 5-entry ring who
// wants to revert to the snapshot from 4 clicks ago has to click
// Undo four times. Round 32 promotes the chip into a popover that
// lets the user pick a specific entry to jump to in one click.
//
// This module owns the SUMMARY of the jump operation — what the
// popover surfaces to the user before they confirm: how many
// entries get skipped, which entries those are (labels in newest-
// first order), which entry the jump lands on. The actual ring
// mutation lives in the bridge layer (slice 156, jumpToUndoEntry).
//
// Mirrors `pdf::hopper::coverage::compute_undo_jump_plan` 1:1.
// Wire shape uses snake_case to match Rust; a browser-mode caller
// goes through this helper directly, a Tauri-mode caller round-
// trips through `slabHopperComputeUndoJumpPlan` (slice 155).

/** Structural plan for a "jump to entry N" operation against an
 *  undo ring. Mirrors Rust `UndoJumpPlan`. Wire-shape uses
 *  snake_case to round-trip with Rust serde defaults. */
export interface UndoJumpPlan {
  /** True iff `target_index` was in range AND there's at least one
   *  entry to drop. Anchors the popover's "Jump here" button's
   *  enabled state. */
  is_valid: boolean;
  /** How many entries the jump skips. Equal to
   *  `(entries.length - 1) - target_index` for valid plans; `0`
   *  for invalid (out-of-range / target-equals-newest). */
  skip_count: number;
  /** Labels of the entries that get dropped, in NEWEST-FIRST order
   *  so the popover reads "Skip: fix-all, fix-it: Tax" in user-
   *  readable chronology. Empty for invalid plans. */
  dropped_labels: string[];
  /** Label of the entry the jump lands on (the new newest after
   *  the jump). Surfaced in the popover's "Jump back to <label>"
   *  copy. Empty string when the input ring was empty or the
   *  target was out-of-range; echoes the actual label when the
   *  target is the already-newest entry (so the popover can
   *  render "active target" copy without re-deriving). */
  target_label: string;
  /** Index of the target in the ORIGINAL ring (echoed back). The
   *  UI passes this through to the bridge's jumpToUndoEntry (slice
   *  156) without recomputing. `0` when the input was empty or the
   *  target was out-of-range; the actual index when target-equals-
   *  newest. */
  target_index: number;
}

/** Plan a "jump directly to entry N" operation against an undo
 *  ring. 1:1 mirror of Rust `compute_undo_jump_plan`.
 *
 *  Algorithm:
 *    1. Empty ring -> invalid; zeroed plan.
 *    2. `targetIndex >= entries.length` -> invalid; zeroed plan
 *       (popover disables the button rather than silently
 *       targeting the newest).
 *    3. `targetIndex === entries.length - 1` -> the target IS the
 *       newest entry; the default cascade button already targets
 *       it. Invalid (no jump needed); echo label/index back so the
 *       popover can render "active target" copy without re-deriving.
 *    4. Otherwise -> valid; walk `entries[newest..=target+1]` in
 *       reverse, collecting labels in newest-first order;
 *       `skip_count = (entries.length - 1) - targetIndex`.
 *
 *  Pure helper — does NOT mutate the input array. Accepts negative
 *  / NaN target indices defensively (treated as out-of-range). */
export function computeUndoJumpPlan(
  entries: UndoEntrySummary[],
  targetIndex: number,
): UndoJumpPlan {
  // Defensive normalisation: a negative / NaN / non-integer index
  // is treated as out-of-range. The UI never passes one in
  // practice (the popover surfaces a row per entry with a known
  // integer index), but a future audit consumer might.
  const idx = Number.isInteger(targetIndex) ? targetIndex : -1;
  if (entries.length === 0 || idx < 0 || idx >= entries.length) {
    return {
      is_valid: false,
      skip_count: 0,
      dropped_labels: [],
      target_label: "",
      target_index: 0,
    };
  }
  const newest = entries.length - 1;
  if (idx === newest) {
    // Already the active target — no jump needed. Echo label/index
    // back so the popover can render "active target" copy.
    return {
      is_valid: false,
      skip_count: 0,
      dropped_labels: [],
      target_label: entries[idx].label,
      target_index: idx,
    };
  }
  // Walk newest -> target+1, collecting labels in newest-first
  // order so the popover reads "Skip: <newest>, <next>, ...".
  const droppedLabels: string[] = [];
  for (let i = newest; i > idx; i--) {
    droppedLabels.push(entries[i].label);
  }
  return {
    is_valid: true,
    skip_count: droppedLabels.length,
    dropped_labels: droppedLabels,
    target_label: entries[idx].label,
    target_index: idx,
  };
}

/** Human-facing copy for a jump plan. Discriminated:
 *
 *    Invalid (empty ring / out-of-range): "No jump available"
 *    Invalid (target == newest):           "Already the newest entry"
 *    Valid (skip 1 entry):                 "Skip 1 revert to jump back to <label>"
 *    Valid (skip N entries):               "Skip N reverts to jump back to <label>"
 *
 *  Plural-aware on "revert" / "reverts". Used in the popover's
 *  confirmation tooltip / aria-label. Pure helper. */
export function describeUndoJumpPlan(plan: UndoJumpPlan): string {
  if (!plan.is_valid) {
    // Two invalid sub-cases: target-equals-newest carries a label
    // (we can be specific); empty / out-of-range doesn't.
    if (plan.target_label.length > 0) {
      return "Already the newest entry";
    }
    return "No jump available";
  }
  const revertNoun = plan.skip_count === 1 ? "revert" : "reverts";
  return `Skip ${plan.skip_count} ${revertNoun} to jump back to ${plan.target_label}`;
}

/** True iff the plan represents a real cascade-shortening jump
 *  (valid AND skip_count >= 1). Convenience predicate for the
 *  popover's button enabled state. Mirrors the `is_valid` flag
 *  directly — `is_valid === true` implies `skip_count >= 1` by
 *  construction — but reads more naturally at call sites that
 *  want a boolean for `disabled`. */
export function canApplyUndoJump(plan: UndoJumpPlan): boolean {
  return plan.is_valid && plan.skip_count >= 1;
}

// ─── Slice 151 — undo-ring live-operations bridge (round-31) ─────────
//
// Slice 149 owns the SUMMARY view (snapshot-free, audit-friendly).
// Slice 151 owns the LIVE-RING operations on the full
// `ReorderUndoEntry[]` the UI keeps in $state: push (with oldest-
// trim at capacity), pop (returning the newest entry), and
// selectActiveUndo (walking newest -> oldest to find the first
// "ready" entry the button should target).
//
// The split is deliberate. Summary helpers are pure-data over the
// compact wire shape (`UndoEntrySummary`) and are shared with audit
// consumers. Bridge helpers operate on the live `ReorderUndoEntry`
// (with `snapshot: Rule[]`) and stay UI-side because the snapshots
// aren't worth serialising. Both layers compose without circular
// dependency — the bridge call sites cast `appliedEffect` <-> wire
// shape only at the wire boundary (slice 152).
//
// All bridge helpers are pure (no Svelte runes, no Tauri). The UI
// slice (152) holds the $state and the cascading-undo button.

/** Push a new entry into a live undo ring, trimming the OLDEST when
 *  the ring exceeds `capacity`. Returns a NEW array — the input is
 *  never mutated, matching the rest of the round-29/30/31 pure-data
 *  contract.
 *
 *  Defensive when `capacity <= 0`: returns an empty array (a zero-
 *  capacity ring can't hold any entries, including the new one).
 *  In production the UI passes `UNDO_RING_CAPACITY = 5`, so this
 *  branch is a guard rather than a hot path. */
export function pushUndoEntry(
  ring: ReorderUndoEntry[],
  entry: ReorderUndoEntry,
  capacity: number,
): ReorderUndoEntry[] {
  if (capacity <= 0) return [];
  const next = ring.concat(entry);
  return next.length > capacity ? next.slice(next.length - capacity) : next;
}

/** Result of popping the most-recent entry from a ring. `entry` is
 *  null when the ring was empty; `remaining` is the ring without
 *  the popped entry (always a fresh array). */
export interface UndoRingPop {
  /** The just-popped entry, or null when the ring was empty. */
  entry: ReorderUndoEntry | null;
  /** Ring after the pop — `[]` when ring was empty or had one entry,
   *  otherwise `ring.slice(0, ring.length - 1)`. */
  remaining: ReorderUndoEntry[];
}

/** Pop the most-recent (newest) entry from a ring. Returns the
 *  popped entry plus the ring without it. Idempotent on an empty
 *  ring (returns `{ entry: null, remaining: [] }`). The UI calls
 *  this after a successful undo apply so the next undo click
 *  targets the now-newest entry. */
export function popUndoEntry(ring: ReorderUndoEntry[]): UndoRingPop {
  if (ring.length === 0) return { entry: null, remaining: [] };
  return {
    entry: ring[ring.length - 1],
    remaining: ring.slice(0, ring.length - 1),
  };
}

/** Result of selecting the active undo target from a ring against
 *  the live chain. Discriminates whether there's anything to surface
 *  at all (`active === null` -> empty ring), and carries counters
 *  for the UI's status chip. */
export interface ActiveUndoSelection {
  /** The selected entry and its status against the current chain,
   *  or null when the ring is empty. The selector walks newest ->
   *  oldest and returns the first entry whose status is `ready`,
   *  falling back to the NEWEST entry if every entry is stale (so
   *  the staleness badge surfaces something rather than going
   *  invisible while still nominally non-empty). */
  active: { entry: ReorderUndoEntry; status: ReorderUndoStatus; index: number } | null;
  /** Total entries in the ring (regardless of status). The UI's
   *  counter chip reads this denominator: "Step 2 of 4". */
  totalEntries: number;
  /** Count of entries whose computed status against the live chain
   *  is `ready` — i.e. truly cascade-undoable. The UI uses this to
   *  decide whether to render the chip at all (totalReady < 2 ->
   *  hide the chip; single ready entry is the round-30 surface). */
  totalReady: number;
  /** Count of entries whose status is `stale` (the user edited the
   *  chain between the apply and now). The UI surfaces this only
   *  in a tooltip; the badge copy on the surfaced entry already
   *  carries the staleness reason. */
  totalStale: number;
}

/** Select the active undo target from a ring against the live chain.
 *
 *  Algorithm:
 *    1. If the ring is empty -> `active = null`, all counters = 0.
 *    2. Walk every entry computing `computeUndoStatus(entry, current)`.
 *       Count ready / stale tallies. Track the FIRST ready entry
 *       found (walking newest -> oldest) and its index.
 *    3. If any ready entry was found -> surface IT as active.
 *    4. Otherwise -> surface the NEWEST entry (last in the array)
 *       with its (stale or noop) status, so the UI has something
 *       to render rather than going invisible.
 *
 *  Newest-first walk lets the user cascade undos cleanly: undoing
 *  the newest ready entry pops it, the next-newest ready becomes
 *  the active entry on the next render.
 *
 *  Index is in the ring's own array (0 = oldest, length-1 = newest)
 *  so the UI can render "Step 3 of 5" with the right denominator
 *  (totalEntries - index — newest is "Step 1 of N", oldest is
 *  "Step N of N"). */
export function selectActiveUndo(
  ring: ReorderUndoEntry[],
  current: Rule[],
): ActiveUndoSelection {
  if (ring.length === 0) {
    return { active: null, totalEntries: 0, totalReady: 0, totalStale: 0 };
  }
  let totalReady = 0;
  let totalStale = 0;
  let firstReadyIndex: number | null = null;
  // Walk newest -> oldest so the FIRST ready we find is the
  // most-recent ready entry — the natural cascade target.
  for (let i = ring.length - 1; i >= 0; i--) {
    const status = computeUndoStatus(ring[i], current);
    if (status.kind === "ready") {
      totalReady += 1;
      if (firstReadyIndex === null) firstReadyIndex = i;
    } else if (status.kind === "stale") {
      totalStale += 1;
    }
  }
  const idx = firstReadyIndex !== null ? firstReadyIndex : ring.length - 1;
  const status = computeUndoStatus(ring[idx], current);
  return {
    active: { entry: ring[idx], status, index: idx },
    totalEntries: ring.length,
    totalReady,
    totalStale,
  };
}

/** Default capacity for the Hopper UI's undo ring — the maximum
 *  number of `ReorderUndoEntry` slots retained at any time. Chosen
 *  to cover a typical paralegal workflow (fix-it on three dead
 *  rules, then fix-all on the remaining two, then realise the
 *  original order was better and undo all five) without bloating
 *  memory. The UI surfaces a counter chip when the ring is at
 *  capacity so the user knows the next undo capture will evict
 *  the oldest entry. */
export const UNDO_RING_CAPACITY = 5;

// ─── Slice 156 — undo-ring jump live-bridge (round-32) ───────────────
//
// Slice 154 owns the SUMMARY view of a jump plan (snapshot-free,
// audit-friendly). Slice 156 owns the LIVE-RING operations: pop
// every entry NEWER than the target so the target becomes the new
// newest. The popover (slice 157) calls `applyUndoJumpToIndex` to
// realise the plan after the user confirms.
//
// The split mirrors slices 149/151 (summary vs live-ring helpers):
// summary helpers are pure-data over the compact wire shape and
// shared with audit consumers; bridge helpers operate on the live
// `ReorderUndoEntry[]` (with `snapshot: Rule[]`) and stay UI-side
// because the snapshots aren't worth serialising.
//
// All bridge helpers are pure (no Svelte runes, no Tauri).

/** Result of trimming a live undo ring to a target index. */
export interface UndoRingJump {
  /** True iff the trim is structurally valid: target_index was in
   *  range AND there's at least one entry to drop. Mirrors the
   *  slice-154 plan's `is_valid`. */
  readonly is_valid: boolean;
  /** The trimmed ring — entries [0..=targetIndex] from the input,
   *  in oldest-first order. The target becomes the new newest
   *  (entries.length - 1). For invalid jumps this is a defensive
   *  shallow copy of the input ring (no mutation, no-op). */
  readonly ring: ReorderUndoEntry[];
  /** The entry the jump landed on (the new newest after the trim),
   *  or null when invalid. The UI uses this to surface the
   *  "Reverted to <label>" toast copy after a confirmed jump. */
  readonly target: ReorderUndoEntry | null;
  /** How many entries were dropped from the newest end. Equal to
   *  the slice-154 plan's `skip_count` for valid jumps; 0 for
   *  invalid. */
  readonly dropped: number;
}

/** Apply a jump-to-index trim to a live undo ring. Returns a new
 *  array with entries newer than `targetIndex` dropped; the input
 *  ring is never mutated.
 *
 *  Algorithm:
 *    1. Empty ring -> invalid; ring is a defensive shallow copy
 *       (`[]`); target = null; dropped = 0.
 *    2. `targetIndex < 0` / `targetIndex >= ring.length` /
 *       non-integer -> invalid; ring is a defensive shallow copy;
 *       target = null; dropped = 0.
 *    3. `targetIndex === ring.length - 1` -> the target IS the
 *       current newest entry; nothing to drop. Invalid (no jump
 *       needed), but target = ring[targetIndex] and ring is a
 *       defensive shallow copy. The popover surfaces this as
 *       "active target — use the cascade button" without
 *       confusing the user.
 *    4. Otherwise -> valid; new ring is `ring.slice(0, targetIndex + 1)`;
 *       target = `ring[targetIndex]`; dropped = `(ring.length - 1)
 *       - targetIndex`.
 *
 *  Defensive against non-integer / negative / NaN target indices
 *  the same way `computeUndoJumpPlan` (slice 154) is. Pure helper
 *  — does NOT mutate the input ring (the returned array is always
 *  a fresh slice).
 *
 *  Note: this helper does NOT apply the snapshot to the live chain
 *  — that's the UI slice (157)'s responsibility via the existing
 *  `slabHopperSetRules` path. This helper only TRIMS the ring; the
 *  caller is responsible for applying `target.snapshot` to the
 *  rules state. */
export function jumpToUndoEntry(
  ring: ReorderUndoEntry[],
  targetIndex: number,
): UndoRingJump {
  // Defensive normalisation: a negative / NaN / non-integer index
  // is treated as out-of-range. Mirrors slice 154's defensive
  // handling so the two helpers stay consistent at call sites.
  const idx = Number.isInteger(targetIndex) ? targetIndex : -1;
  if (ring.length === 0 || idx < 0 || idx >= ring.length) {
    return { is_valid: false, ring: ring.slice(), target: null, dropped: 0 };
  }
  const newest = ring.length - 1;
  if (idx === newest) {
    // Already the newest entry — no trim needed. Echo target back
    // so the popover row for this entry can render "active target"
    // copy without re-deriving.
    return {
      is_valid: false,
      ring: ring.slice(),
      target: ring[idx],
      dropped: 0,
    };
  }
  const next = ring.slice(0, idx + 1);
  return {
    is_valid: true,
    ring: next,
    target: ring[idx],
    dropped: newest - idx,
  };
}

/** Build per-entry `UndoEntrySummary[]` from a live ring for the
 *  slice-154 planner. The summary is the compact wire-shape the
 *  planner operates on (snapshot-free) — the live ring carries
 *  `snapshot: Rule[]` which the planner doesn't need.
 *
 *  Used by the slice-157 popover to render the per-row plan copy
 *  via `computeUndoJumpPlan(summarizeRingForJump(ring), index)`.
 *  Pure helper — does NOT mutate the input. */
export function summarizeRingForJump(
  ring: ReorderUndoEntry[],
): UndoEntrySummary[] {
  return ring.map((entry) => ({
    label: entry.label,
    captured_at_ms: entry.capturedAt,
    applied_effect: entry.appliedEffect,
  }));
}

// ─── Slice 85 — sample drilldown TS client (legacy header below) ─────

/** Bucket selector for the drilldown command. Mirrors
 *  `pdf::hopper::coverage::SampleBucket` — a `kind`-tagged union.
 *  Use `ruleBucket(i)` / `FALLTHROUGH_BUCKET` to construct one rather
 *  than hand-rolling the object literal at call sites. */
export type SampleBucket =
  | { kind: "rule"; index: number }
  | { kind: "fallthrough" };

/** Stable singleton for the fall-through bucket — saves a per-call
 *  object literal and keeps the bucket comparison stable across
 *  identity checks. */
export const FALLTHROUGH_BUCKET: SampleBucket = { kind: "fallthrough" } as const;

/** Construct a rule bucket selector. Throws on negative indices —
 *  the Rust side treats out-of-range indices as empty buckets, but
 *  negative numbers indicate a TS bug in the caller, not a real
 *  drilldown the user asked for. */
export function ruleBucket(index: number): SampleBucket {
  if (!Number.isInteger(index) || index < 0) {
    throw new Error(`ruleBucket: index must be a non-negative integer, got ${index}`);
  }
  return { kind: "rule", index };
}

/** The drilldown result for a single bucket. Mirrors
 *  `pdf::hopper::coverage::SampleDrilldown`. `samples` is capped to
 *  `preview_cap` (default 25 server-side); `total_in_bucket` reports
 *  the FULL bucket size pre-cap so the UI can render
 *  "Showing 25 of 47" copy. `truncated` is true iff
 *  `total_in_bucket > samples.length`. */
export interface SampleDrilldown {
  bucket: SampleBucket;
  samples: RuleSample[];
  total_in_bucket: number;
  truncated: boolean;
}

/** Drill into a coverage bucket and return the files in it (capped
 *  to `previewCap`, default 25). The other input axes
 *  (`candidateRules` / `samples` / `sampleLimit`) mirror
 *  `slabHopperRuleCoverage` so the click-through drilldown evaluates
 *  the EXACT same chain + samples the coverage report counted. */
export const slabHopperSampleDrilldown = (
  watchId: number,
  bucket: SampleBucket,
  opts: {
    candidateRules?: Rule[];
    samples?: RuleSample[];
    sampleLimit?: number;
    previewCap?: number;
  } = {},
): Promise<SampleDrilldown> =>
  invoke("slab_hopper_sample_drilldown", {
    watchId,
    bucket,
    candidateRules: opts.candidateRules ?? null,
    samples: opts.samples ?? null,
    sampleLimit: opts.sampleLimit ?? null,
    previewCap: opts.previewCap ?? null,
  });

/** True iff two `SampleBucket`s point at the same logical bucket.
 *  Used by the coverage panel to gate the "this row is open" highlight
 *  without depending on object identity. */
export function sampleBucketEquals(a: SampleBucket, b: SampleBucket): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "rule" && b.kind === "rule") return a.index === b.index;
  return true; // both fallthrough
}

/** Header copy for the drilldown popover. Returns "Showing N of M"
 *  when truncated, plain "N file(s)" otherwise. Empty bucket reads
 *  as "No files in this bucket" so the popover never renders bare. */
export function describeDrilldown(drill: SampleDrilldown): string {
  const total = drill.total_in_bucket;
  if (total === 0) return "No files in this bucket";
  const shown = drill.samples.length;
  if (drill.truncated) return `Showing ${shown} of ${total}`;
  return `${total} ${total === 1 ? "file" : "files"}`;
}

/** Human-readable label for a bucket — used in the popover heading
 *  and the close-button title. Rule buckets show `1-based` indices
 *  to match the coverage panel's `#1 Tax` row labels. */
export function describeBucket(
  bucket: SampleBucket,
  ruleNames?: readonly string[],
): string {
  if (bucket.kind === "fallthrough") return "Fall-through to watch defaults";
  const i = bucket.index;
  const name = ruleNames?.[i];
  if (name && name.trim().length > 0) return `#${i + 1} ${name}`;
  return `Rule #${i + 1}`;
}

// ─── v3.40 Slice 90 — drilldown CSV export TS wrapper ────────────────
//
// Mirrors exportInstallLogCsv / slabHopperExportBackfillCsv: an absolute
// path the caller usually obtains from @tauri-apps/plugin-dialog `save()`
// + the same drilldown shape the popover already has loaded in state,
// plus the rule-names array so the bucket_name column reads exactly
// like the popover header. Returns the byte count written so the toast
// can show "Exported 23 files (1.4 KB)" without re-reading the file.

/** Write a [`SampleDrilldown`] to `path` as RFC-4180 CSV. The path is
 *  an absolute filesystem path the caller usually obtains from
 *  `@tauri-apps/plugin-dialog` `save()` so it bypasses the default
 *  plugin-fs scope. `ruleNames` is the in-flight rule-name array
 *  (typically `rules.map(r => r.name)`) used to resolve a rule
 *  bucket's display label — same `Rule #N` (1-based) fallback as
 *  `describeBucket` for missing/blank/out-of-range names.
 *
 *  Returns the byte count actually written. Returns 0 in browser
 *  mode (no-op). */
export async function slabHopperExportDrilldownCsv(
  drilldown: SampleDrilldown,
  ruleNames: readonly string[],
  path: string,
): Promise<number> {
  // Lazy import keeps the test file (which runs under node) from
  // pulling the Tauri plugin chain just to type-check the helpers.
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return 0;
  return invoke<number>("slab_hopper_export_drilldown_csv", {
    drilldown,
    ruleNames: [...ruleNames],
    path,
  });
}

/** Write a [`SampleDrilldown`] to `path` as a pretty-printed JSON
 *  envelope (slice-93 `DrilldownExportEnvelope` shape on the Rust
 *  side: schema_version + generated_at_iso + bucket metadata +
 *  samples). Same call shape as `slabHopperExportDrilldownCsv`; the
 *  Tauri command picks the serialiser.
 *
 *  `ruleNames` resolves the rule bucket's display label using the
 *  SAME fallback chain the CSV wrapper uses (`Rule #N` 1-based when
 *  missing/blank/out-of-range) so JSON + CSV exports of the same
 *  bucket carry identical labels.
 *
 *  Returns the byte count actually written. Returns 0 in browser
 *  mode (no-op). */
export async function slabHopperExportDrilldownJson(
  drilldown: SampleDrilldown,
  ruleNames: readonly string[],
  path: string,
): Promise<number> {
  // Same lazy-import trick as the CSV wrapper.
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return 0;
  return invoke<number>("slab_hopper_export_drilldown_json", {
    drilldown,
    ruleNames: [...ruleNames],
    path,
  });
}

/** Suggest a default filename for a drilldown CSV/JSON export. Mirrors the
 *  marketplace install-log + hopper backfill conventions so paralegals
 *  see ONE consistent naming pattern across the audit-export surfaces.
 *
 *  Format: `hopper-drilldown_<watch>_<bucket>_<YYYY-MM-DD>.<ext>`.
 *
 *  - `<watch>` slot is the watch id (`watch-7`) when supplied or
 *    `watch` when the caller doesn't have one handy (the popover
 *    always has one; the slot exists so the suggestion remains
 *    well-formed even on a future surface).
 *  - `<bucket>` slot reads `fallthrough` for the catch-all bucket;
 *    for rule buckets it reads `rule-<N>` (1-based, matching the
 *    popover labels), with an optional `_<slug>` of the rule name
 *    when it's available + non-empty. Names are slugified to
 *    `[a-z0-9-]` ASCII (lowercase + dashes for non-alphanumeric runs)
 *    so the suggested filename survives a Windows filesystem.
 *  - `<ext>` defaults to `"csv"` (slice-90 behaviour) so existing
 *    call sites stay green; pass `"json"` for the slice-94 envelope
 *    export. The slot must be a real file extension (no leading
 *    dot, lowercase) — the helper trusts the caller and doesn't
 *    sanitise.
 *  - The trailing date uses the caller's local time (the suggestion
 *    is for a save dialog the user is about to confirm — local-time
 *    matches what their calendar says today is).
 *
 *  Pure helper — no I/O, no Tauri. */
export function suggestDrilldownExportFilename(
  bucket: SampleBucket,
  ruleNames: readonly string[] | null,
  opts: { watchId?: number | null; now?: number; ext?: "csv" | "json" } = {},
): string {
  const watchSlot =
    opts.watchId != null && Number.isFinite(opts.watchId) && opts.watchId >= 0
      ? `watch-${Math.trunc(opts.watchId)}`
      : "watch";

  const bucketSlot = (() => {
    if (bucket.kind === "fallthrough") return "fallthrough";
    const i = bucket.index;
    const base = `rule-${i + 1}`;
    const name = ruleNames?.[i];
    if (!name) return base;
    const slug = slugifyForFilename(name);
    return slug ? `${base}_${slug}` : base;
  })();

  const d = new Date(opts.now ?? Date.now());
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const ext = opts.ext ?? "csv";
  return `hopper-drilldown_${watchSlot}_${bucketSlot}_${y}-${m}-${day}.${ext}`;
}

/** Slugify a rule name for inclusion in a filename. Lowercase ASCII
 *  letters/digits survive; runs of anything else collapse to a single
 *  dash; leading/trailing dashes are trimmed. Returns an empty string
 *  when nothing survives (caller falls back to bare bucket slot). */
function slugifyForFilename(raw: string): string {
  const lower = raw.toLowerCase();
  // Replace any run of non-[a-z0-9] (Unicode-letter included via NFD strip)
  // with a single dash; trim leading/trailing dashes.
  // We don't try to transliterate non-ASCII letters (café -> caf) — that's
  // surprising in a filename. Better to drop them and lean on the watch +
  // bucket numeric slots which are always meaningful.
  const ascii = lower.normalize("NFD").replace(/[\u0300-\u036f]/g, "");
  const dashed = ascii.replace(/[^a-z0-9]+/g, "-");
  return dashed.replace(/^-+|-+$/g, "");
}

// ─── v3.40 Slice 125 — rule coverage CSV+JSON export TS wrappers ─────
//
// Mirrors the drilldown export wrappers (slices 90 + 95) and the
// install-log Export… popover shape: an absolute path the caller
// usually obtains from `@tauri-apps/plugin-dialog` `save()`, the
// in-flight RuleCoverageReport already loaded in panel state, and
// the Tauri command picks the serialiser. Returns the byte count
// written so the toast can read "Exported 8 rules as CSV (1.2 KB)"
// without re-reading the file.
//
// The caller passes the report DIRECTLY (no re-fetch) — exporting
// what the user is looking at matches their mental model and avoids
// a brief race window where the in-flight rule edit + 600ms
// debounce would let a re-run return a slightly different report.

/** Write a [`RuleCoverageReport`] to `path` as RFC-4180 CSV (slice 123
 *  shape, header included). Same lazy-import + browser no-op posture
 *  as `slabHopperExportDrilldownCsv`.
 *
 *  Returns the byte count actually written. Returns 0 in browser
 *  mode (no-op). */
export async function slabHopperExportCoverageCsv(
  report: RuleCoverageReport,
  path: string,
): Promise<number> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return 0;
  return invoke<number>("slab_hopper_export_coverage_csv", { report, path });
}

/** Write a [`RuleCoverageReport`] to `path` as a pretty-printed JSON
 *  envelope (slice 124 `RuleCoverageExportEnvelope` shape on the Rust
 *  side: schema_version + generated_at_iso + envelope-level
 *  chain-health totals + rules). Same call shape as
 *  `slabHopperExportCoverageCsv`; the Tauri command picks the
 *  serialiser.
 *
 *  Returns the byte count actually written. Returns 0 in browser
 *  mode (no-op). */
export async function slabHopperExportCoverageJson(
  report: RuleCoverageReport,
  path: string,
): Promise<number> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return 0;
  return invoke<number>("slab_hopper_export_coverage_json", { report, path });
}

/** Slice 130 — narrow a coverage report to one diagnostic kind via
 *  the backend `slab_hopper_filter_coverage` command. Returns a NEW
 *  [`RuleCoverageReport`] with `rules` filtered per the priority
 *  chain (dead > zero > shadowed > healthy) and `fallthrough` /
 *  `total_samples` preserved verbatim.
 *
 *  Mirrors the TS-only `filterCoverageByDiagnostic` (slice 129)
 *  exactly; the wire version exists so the EXPORT path can compose
 *  the filter + export commands without round-tripping the report
 *  through a TS pre-filter step, and so a future scripted-export
 *  consumer (CLI driver, cron job) gets the filter as a first-class
 *  command. The in-panel render path uses the TS helper directly.
 *
 *  Wraps to the local TS helper in browser-mode so component-level
 *  testing doesn't need a Tauri stub. */
export async function slabHopperFilterCoverage(
  report: RuleCoverageReport,
  filter: CoverageDiagnosticFilter,
): Promise<RuleCoverageReport> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return filterCoverageByDiagnostic(report, filter);
  return invoke<RuleCoverageReport>("slab_hopper_filter_coverage", {
    report,
    filter: { kind: filter },
  });
}

/** Slice 135 — plan minimal-reorder fixes for every dead rule in
 *  `report` via the backend `slab_hopper_plan_dead_rule_reorder`
 *  command. Returns one [`ReorderProposal`] per dead row in input
 *  order; empty when the chain has no dead rules.
 *
 *  Mirrors `planDeadRuleReorder` (slice 134) exactly; the wire
 *  command exists so a future scripted-audit consumer (CLI driver,
 *  cron health-check) gets the planner as a first-class command,
 *  and so the server-side `RulePredicate` variant set is the
 *  authoritative source — a Rust-side predicate kind not yet
 *  mirrored in TS won't silently misclassify the chain.
 *
 *  Wraps to the local TS helper in browser-mode so component-level
 *  testing doesn't need a Tauri stub. */
export async function slabHopperPlanDeadRuleReorder(
  rules: Rule[],
  report: RuleCoverageReport,
): Promise<ReorderProposal[]> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return planDeadRuleReorder(rules, report);
  return invoke<ReorderProposal[]>("slab_hopper_plan_dead_rule_reorder", {
    rules,
    report,
  });
}

/** Slice 140 — apply every proposal in `proposals` to `rules` in
 *  input order via the backend `slab_hopper_batch_reorder_dead_rules`
 *  command. Returns a [`BatchReorderOutcome`] carrying the new chain
 *  plus per-proposal applied/skipped accounting.
 *
 *  Mirrors `applyReorderProposalsBatch` (slice 139) exactly; the
 *  wire command exists so a future scripted-audit consumer (CLI
 *  driver, cron health-check) gets the batch applier as a
 *  first-class command, and so the server-side Rule type is the
 *  authoritative source — a Rust-side Rule field not yet mirrored
 *  in TS won't silently change the by-name equality contract.
 *
 *  Wraps to the local TS helper in browser-mode so component-level
 *  testing doesn't need a Tauri stub. */
export async function slabHopperBatchReorderDeadRules(
  rules: Rule[],
  proposals: ReorderProposal[],
): Promise<BatchReorderOutcome> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return applyReorderProposalsBatch(rules, proposals);
  return invoke<BatchReorderOutcome>("slab_hopper_batch_reorder_dead_rules", {
    rules,
    proposals,
  });
}

/** Slice 145 — produce a structural summary of how the AFTER chain
 *  differs from the BEFORE chain via the backend
 *  `slab_hopper_summarize_reorder_effect` command. Returns a
 *  [`ReorderEffect`] carrying moved entries, added/removed name
 *  lists, and the permutation flag.
 *
 *  Mirrors `summarizeReorderEffect` (slice 144) exactly; the wire
 *  command exists so a future scripted-audit consumer (CLI diff
 *  subcommand, cron health-check) gets the summariser as a
 *  first-class command, and so the server-side Rule type is the
 *  authoritative source for by-name equality.
 *
 *  Wraps to the local TS helper in browser-mode so component-level
 *  testing doesn't need a Tauri stub. */
export async function slabHopperSummarizeReorderEffect(
  before: Rule[],
  after: Rule[],
): Promise<ReorderEffect> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return summarizeReorderEffect(before, after);
  return invoke<ReorderEffect>("slab_hopper_summarize_reorder_effect", {
    before,
    after,
  });
}

/** Summarise the Hopper UI's undo ring against a capacity cap via
 *  the backend `slab_hopper_summarize_undo_ring` command. Returns
 *  an [`UndoRingSummary`] carrying the trimmed entries plus
 *  capacity / full metadata.
 *
 *  Mirrors `summarizeUndoRing` (slice 149) exactly; the wire
 *  command exists so a future scripted-audit consumer (CLI driver,
 *  cron health-check) gets the summariser as a first-class command,
 *  and so the server-side wire shape is the authoritative source
 *  for the trim contract.
 *
 *  Wraps to the local TS helper in browser-mode so component-level
 *  testing doesn't need a Tauri stub. */
export async function slabHopperSummarizeUndoRing(
  entries: UndoEntrySummary[],
  capacity: number,
): Promise<UndoRingSummary> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return summarizeUndoRing(entries, capacity);
  return invoke<UndoRingSummary>("slab_hopper_summarize_undo_ring", {
    entries,
    capacity,
  });
}

/** Plan a jump-to-entry-N operation against the Hopper UI's undo
 *  ring via the backend `slab_hopper_compute_undo_jump_plan`
 *  command. Returns an [`UndoJumpPlan`] carrying validity, skip
 *  count, dropped labels (newest-first), target label, and echoed
 *  target index.
 *
 *  Mirrors `computeUndoJumpPlan` (slice 154) exactly; the wire
 *  command exists so a future scripted-audit consumer (CLI driver,
 *  cron health-check that surfaces deep jumps) gets the planner as
 *  a first-class command, and so the server-side wire shape is the
 *  authoritative source for the newest-first walk contract.
 *
 *  Wraps to the local TS helper in browser-mode so component-level
 *  testing doesn't need a Tauri stub. */
export async function slabHopperComputeUndoJumpPlan(
  entries: UndoEntrySummary[],
  targetIndex: number,
): Promise<UndoJumpPlan> {
  const { isInTauri } = await import("$lib/tauri");
  if (!isInTauri()) return computeUndoJumpPlan(entries, targetIndex);
  return invoke<UndoJumpPlan>("slab_hopper_compute_undo_jump_plan", {
    entries,
    targetIndex,
  });
}

/** Suggest a default filename for a coverage CSV/JSON export.
 *  Mirrors the drilldown filename helper conventions so paralegals
 *  see one consistent naming pattern across the audit-export
 *  surfaces.
 *
 *  Format: `hopper-coverage_<watch>_<filter?>_<YYYY-MM-DD>.<ext>`.
 *
 *  - `<watch>` slot is the watch id (`watch-7`) when supplied or
 *    `watch` when the caller doesn't have one handy. The coverage
 *    panel always has one; the optional slot mirrors
 *    `suggestDrilldownExportFilename` for symmetry.
 *  - `<filter>` slot is the diagnostic filter slug
 *    (`dead`, `zero`, `shadowed`, `healthy`). Omitted entirely
 *    (no slot, no extra underscore) when the filter is `"all"` or
 *    unset — preserving back-compat with the round-26 export
 *    filenames so an unfiltered export still produces
 *    `hopper-coverage_watch-7_2026-06-23.csv` exactly. A filtered
 *    export of the same chain produces
 *    `hopper-coverage_watch-7_dead_2026-06-23.csv` so the filename
 *    itself advertises what's in the file.
 *  - `<ext>` defaults to `"csv"`; pass `"json"` for the envelope
 *    export.
 *  - The trailing date uses the caller's local time (same as the
 *    drilldown filename helper — a save dialog the user is about
 *    to confirm).
 *
 *  Note the coverage export has NO per-bucket slot (unlike the
 *  drilldown filename which carries `fallthrough` or `rule-N`) —
 *  a coverage export covers the WHOLE chain, not a single bucket.
 *  The diagnostic-filter slot is a related but distinct concept:
 *  it tells the consumer what KIND of rules are in the file, not
 *  which bucket of files.
 *
 *  Pure helper — no I/O, no Tauri. */
export function suggestCoverageExportFilename(
  opts: {
    watchId?: number | null;
    now?: number;
    ext?: "csv" | "json";
    filter?: CoverageDiagnosticFilter;
  } = {},
): string {
  const watchSlot =
    opts.watchId != null && Number.isFinite(opts.watchId) && opts.watchId >= 0
      ? `watch-${Math.trunc(opts.watchId)}`
      : "watch";

  const d = new Date(opts.now ?? Date.now());
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const ext = opts.ext ?? "csv";

  // Filter slot is OMITTED on "all" / undefined so round-26 filenames
  // round-trip byte-for-byte. A narrowing filter inserts the slug
  // between the watch and the date.
  const filterSlot =
    opts.filter && opts.filter !== "all" ? `_${opts.filter}` : "";

  return `hopper-coverage_${watchSlot}${filterSlot}_${y}-${m}-${day}.${ext}`;
}

// ---------------------------------------------------------------------
// Predicate helpers — small but worth their weight in keystroke-saving
// when the editor wires up 6 different predicate kinds.
// ---------------------------------------------------------------------

/** All predicate kinds the editor offers, in display order. */
export const PREDICATE_KINDS = [
  "filename-glob",
  "filename-regex",
  "text-contains-all",
  "page-count-between",
  "size-over",
  "always",
] as const satisfies readonly RulePredicate["kind"][];

/** Human label for each predicate kind, for the editor dropdown. */
export const predicateLabel = (kind: RulePredicate["kind"]): string => {
  switch (kind) {
    case "filename-glob":
      return "filename matches glob";
    case "filename-regex":
      return "filename matches regex";
    case "text-contains-all":
      return "text contains all of";
    case "page-count-between":
      return "page count between";
    case "size-over":
      return "size larger than";
    case "always":
      return "always (catch-all)";
  }
};

/** A sensible empty payload for each predicate kind. The editor calls
 *  this when the user picks a new kind from the dropdown. */
export const emptyPredicate = (
  kind: RulePredicate["kind"],
): RulePredicate => {
  switch (kind) {
    case "filename-glob":
      return { kind, pattern: "*.pdf" };
    case "filename-regex":
      return { kind, pattern: "" };
    case "text-contains-all":
      return { kind, needles: [] };
    case "page-count-between":
      return { kind, min: 1, max: 10 };
    case "size-over":
      return { kind, bytes: 1_000_000 };
    case "always":
      return { kind };
  }
};

/** An empty action — every override inherits. */
export const emptyAction = (): RuleAction => ({
  recipe_id: null,
  output_dir: null,
  rename_pattern: null,
});

/** Render a one-line summary of a predicate for compact UI display. */
export const formatPredicate = (p: RulePredicate): string => {
  switch (p.kind) {
    case "filename-glob":
      return `glob: ${p.pattern || "(empty)"}`;
    case "filename-regex":
      return `regex: /${p.pattern || ".*"}/i`;
    case "text-contains-all":
      return p.needles.length
        ? `text ⊇ {${p.needles.join(", ")}}`
        : "text ⊇ {…}";
    case "page-count-between":
      return `pages ∈ [${p.min}, ${p.max}]`;
    case "size-over":
      return `size > ${formatBytes(p.bytes)}`;
    case "always":
      return "always";
  }
};

/** Pretty-print byte counts as KB/MB/GB. Pure helper, used in the
 *  size-over predicate UI and the preview pane. */
export const formatBytes = (n: number): string => {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
};

// ---------------------------------------------------------------------
// v3.22.0 "Hopper Loop" — batch backfill API
// ---------------------------------------------------------------------
//
// Backfill = "apply my current rule chain to every PDF already sitting
// in this watched folder." Two-step contract:
//
//   1. plan  — returns a `BackfillReport` (dry run, no FS mutations).
//   2. apply — pass that report back to commit the moves; returns a
//              `BackfillRun` summary that the UI shows in a toast and
//              that the backend persists to sqlite.
//
// The split mirrors Tauri's `confirm dialog → action` pattern and lets
// the user deselect rows before applying. The plan is cheap (single
// `read_dir` + rule eval per file) and idempotent; calling `plan`
// twice in a row returns the same plan modulo `generated_at`.

/** Discriminator for `PlannedAction.action`. Matches the Rust
 *  `ActionKind` enum with `kebab-case` serde. */
export type BackfillActionKind = "move" | "copy" | "skip" | "no-match";

/** One row in the dry-run preview table. */
export interface PlannedAction {
  source_path: string;
  size_bytes: number;
  matched_rule: string | null;
  destination: string | null;
  action: BackfillActionKind;
  reason: string;
}

/** Full dry-run report — one row per `*.pdf` in the scanned folder. */
export interface BackfillReport {
  folder: string;
  scanned: number;
  planned: PlannedAction[];
  /** Unix-seconds UTC. */
  generated_at: number;
  /** Tally of files per matched rule, plus the synthetic
   *  `__defaults__` (no rule, fell through to watch defaults) and
   *  `__skip__` (plan-time skip) buckets. Rules with zero hits are
   *  absent. Powers the panel's "Tax: 17 · Invoices: 23 · No rule:
   *  4" pre-flight coverage strip. Optional on the wire — pre-v3.39
   *  reports decode with this missing and the UI strip just doesn't
   *  render. */
  per_rule_counts?: Record<string, number>;
}

/** Bucket key in `BackfillReport.per_rule_counts` for plans that fell
 *  through to the watch defaults (no rule matched). */
export const BACKFILL_BUCKET_DEFAULTS = "__defaults__";

/** Bucket key for plan-time skips (probe error, missing metadata). */
export const BACKFILL_BUCKET_SKIP = "__skip__";

/** Pretty-print a per_rule_counts bucket key for UI chips. The
 *  synthetic `__defaults__` / `__skip__` keys are translated to
 *  user-facing labels; any other key is the user-set rule name and
 *  passes through unchanged. */
export const backfillBucketLabel = (key: string): string => {
  if (key === BACKFILL_BUCKET_DEFAULTS) return "No rule";
  if (key === BACKFILL_BUCKET_SKIP) return "Skipped";
  return key;
};

/** Per-file outcome after `executeBackfill` has run. */
export type BackfillOutcomeStatus = "moved" | "skipped" | "failed";
export interface BackfillOutcome {
  source_path: string;
  destination: string | null;
  status: BackfillOutcomeStatus;
  error: string | null;
}

/** Aggregate run summary — persisted to sqlite + shown in history. */
export interface BackfillRun {
  folder: string;
  scanned: number;
  applied: number;
  skipped: number;
  errored: number;
  started_at: number;
  finished_at: number;
  per_file: BackfillOutcome[];
}

/** Per-file progress frame emitted by `slabHopperExecuteBackfillAsync`
 *  on `hopper://backfill-progress`. The UI uses this for the live
 *  progress bar (`processed / total`), running counts, and the
 *  scrolling tail-of-recent-outcomes strip.
 *
 *  Wire contract pinned by `backfill_progress_round_trips_through_json`
 *  in the Rust suite — changing a field name there will break this. */
export interface BackfillProgress {
  /** 1-indexed position of the file just finished. `processed == total`
   *  signals the final frame; UI transitions out of "applying" here. */
  processed: number;
  /** Total file count from `report.planned.length` — constant per run. */
  total: number;
  /** Running tally — `applied + skipped + errored == processed`. */
  applied: number;
  skipped: number;
  errored: number;
  /** The outcome the loop just produced. `null` only on the empty-
   *  report tail-end frame (so the UI gets exactly one completion
   *  signal for an empty backfill). */
  current: BackfillOutcome | null;
}

/** Tauri event envelope — `run_id` lets a future multi-run UI route
 *  events to the right component instance. Today the panel gates to
 *  one run at a time and just matches its own id. */
export interface BackfillProgressEvent {
  run_id: number;
  progress: BackfillProgress;
}

/** Mint a unique run id for the streaming executor. Wall-clock ms is
 *  fine — the only collision risk is two backfills started in the
 *  same millisecond, which the UI already guards against. */
export const newBackfillRunId = (): number => Date.now();

/** Tunables for `slabHopperPlanBackfill`. Defaults match the v3.22
 *  single-level scan so widening the call is fully back-compat.
 *
 *  - `recursive` — walk sub-folders too. Hidden directories are still
 *    skipped (same hostility-to-Spotlight-noise rule as hidden files).
 *  - `maxDepth` — cap on recursion depth. `null` = unbounded, `0`
 *    matches `recursive = false` exactly. Ignored when
 *    `recursive = false`. */
export interface PlanOptions {
  recursive: boolean;
  /** Backend field is `max_depth`; we expose it as a wire-snake-case
   *  field to match Rust serde. */
  max_depth: number | null;
}

/** Empty/default plan options — non-recursive, no depth cap. */
export const emptyPlanOptions = (): PlanOptions => ({
  recursive: false,
  max_depth: null,
});

/** Dry-run: plan the moves the current rule chain would perform on
 *  every PDF in `folder` (defaults to the watch's `source_dir`).
 *  Pure — never touches the filesystem outside `folder`.
 *
 *  `opts` (v3.39 round-10) controls whether sub-folders are swept.
 *  Omit it for legacy single-level behaviour. */
export const slabHopperPlanBackfill = (
  watchId: number,
  folder?: string,
  opts?: PlanOptions,
): Promise<BackfillReport> =>
  invoke("slab_hopper_plan_backfill", {
    watchId,
    folder: folder ?? null,
    opts: opts ?? null,
  });

/** Commit a previously-approved `BackfillReport`. Idempotent — if a
 *  file no longer exists (e.g. user deleted it between plan + apply),
 *  the row is marked `skipped` and the rest still apply. */
export const slabHopperExecuteBackfill = (
  report: BackfillReport,
): Promise<BackfillRun> =>
  invoke("slab_hopper_execute_backfill", { report });

/** Streaming variant of `slabHopperExecuteBackfill` — same return
 *  value (the final `BackfillRun`), but the backend broadcasts a
 *  `hopper://backfill-progress` event after every file so the UI can
 *  render a live progress bar + scrolling outcome tail without
 *  polling. Pair with `listenBackfillProgress()` to receive frames
 *  and `slabHopperCancelBackfill(runId)` for the Cancel button.
 *
 *  `runId` must be unique per call — mint one with `newBackfillRunId()`. */
export const slabHopperExecuteBackfillAsync = (
  report: BackfillReport,
  runId: number,
): Promise<BackfillRun> =>
  invoke("slab_hopper_execute_backfill_async", { report, runId });

/** Flip the cancel token for an in-flight streaming backfill. The
 *  worker checks the token before each file, so cancellation is
 *  near-immediate (next file is stamped `skipped` with reason
 *  "cancelled by user"). Returns `true` if a flag was found + flipped,
 *  `false` if the run had already completed — both are non-error
 *  outcomes from the user's perspective. */
export const slabHopperCancelBackfill = (runId: number): Promise<boolean> =>
  invoke("slab_hopper_cancel_backfill", { runId });

/** Subscribe to live backfill-progress events. Returns an unlisten
 *  function the caller stores and invokes on unmount. The handler
 *  fires once per file processed (plus one tail-end frame for empty
 *  reports). Filter by `e.run_id` when multiple runs are possible. */
export const listenBackfillProgress = async (
  handler: (e: BackfillProgressEvent) => void,
): Promise<UnlistenFn> => {
  return listen<BackfillProgressEvent>(
    "hopper://backfill-progress",
    (e) => handler(e.payload),
  );
};

/** History tail of past backfills, newest first. Pass `folder` to
 *  filter to one watched directory. `sinceUnix` (v3.39 round-10)
 *  filters to runs that *finished* at or after the given unix-seconds
 *  timestamp — powers the "Last 24h / Last 7d / All" history chips
 *  by computing a JS-side cutoff and letting SQL do the row filter. */
export const slabHopperListBackfillRuns = (
  folder?: string,
  limit?: number,
  sinceUnix?: number,
): Promise<BackfillRun[]> =>
  invoke("slab_hopper_list_backfill_runs", {
    folder: folder ?? null,
    sinceUnix: sinceUnix ?? null,
    limit: limit ?? null,
  });

/** Compute the unix-seconds cutoff for a "Last N hours" chip. Pure
 *  helper, used by HopperBackfillPanel's history filter. */
export const backfillSinceUnix = (windowHours: number | null): number | null => {
  if (windowHours === null || !Number.isFinite(windowHours) || windowHours <= 0) {
    return null;
  }
  return Math.floor(Date.now() / 1000) - Math.floor(windowHours * 3600);
};

/** Export a `BackfillReport` to disk as RFC-4180 CSV. The frontend
 *  gathers `path` from the @tauri-apps/plugin-dialog save-as picker;
 *  the Rust side handles the actual write so arbitrary user-chosen
 *  paths bypass the frontend FS scope. Returns the byte count
 *  written, so the UI toast can say "Exported 42 rows (3.1 KB)"
 *  without re-reading the file. */
export const slabHopperExportBackfillCsv = (
  report: BackfillReport,
  path: string,
): Promise<number> =>
  invoke("slab_hopper_export_backfill_csv", { report, path });

/** Suggest a default filename for the CSV export based on the
 *  scanned folder + plan timestamp. Pure — keeps the panel free of
 *  date math. Format: `backfill_<basename>_<YYYY-MM-DD>.csv`. */
export const suggestBackfillCsvFilename = (report: BackfillReport): string => {
  const stem = basename(report.folder).replace(/[^A-Za-z0-9_-]/g, "_") || "folder";
  const d = new Date(report.generated_at * 1000);
  const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  return `backfill_${stem}_${iso}.csv`;
};

// =====================================================================
// v3.40 Slice 158 — Keyboard-shortcut resolver for the cascade-undo
// (round-33)
// =====================================================================
//
// Round 32 (slice 157) shipped a per-entry cascade-jump popover that
// surfaces the full ring with "Jump here" buttons. The popover ONLY
// opens via a mouse click on the cov-undo-chip — power users editing
// a 30-rule chain reach for Cmd-Z and get the browser undo (which
// either reverts a text-input change or no-ops). Slice 158 adds a
// pure-data resolver that turns a KeyboardEvent into one of three
// shortcut intents so the UI can wire Cmd-Z to the cascade Undo
// button, Cmd-Shift-Z to the jump popover, and arrow-keys + Enter
// to navigate / activate rows inside the popover.
//
// The resolver is pure: it inspects the event's key/modifiers but
// does NOT touch any state. The UI dispatches each intent to the
// appropriate handler (applyUndo / toggleUndoPopover / focus
// movement / applyUndoJump). Tests pin every code path including
// platform-aware Cmd-vs-Ctrl and the "no modifier" / "wrong key"
// no-op branches.
//
// Why a pure resolver instead of inline event handling in the
// component: (a) tests can exercise every branch without spinning
// up a Svelte instance + JSDOM, (b) the audit consumer (a future
// "log every keyboard shortcut" debug surface) gets a first-class
// classifier, (c) it documents the gesture vocabulary in one place
// instead of scattered inside an `if (e.key === "z" && ...) else
// if` chain.

/** The platform-specific modifier key for primary shortcuts. macOS
 *  / iPadOS use Cmd; Linux / Windows use Ctrl. The resolver accepts
 *  either at runtime (some Linux users on a Mac keyboard expect Cmd
 *  to work), but the discriminator helps a future settings surface
 *  document the "default" platform binding. */
export type UndoShortcutPlatform = "mac" | "other";

/** The three shortcut intents the cascade-undo surface understands.
 *  Returned by `resolveUndoShortcut`; the UI dispatches each to a
 *  matching handler.
 *
 *  - `"cascade"`: Cmd-Z / Ctrl-Z — fire the cascade Undo button on
 *    the newest ready entry. Equivalent to clicking the inline Undo
 *    button on the toast. Surfaces the round-30/31 cascade UX.
 *  - `"open-popover"`: Cmd-Shift-Z / Ctrl-Shift-Z — open the
 *    cascade-jump popover (or close it if already open). Surfaces
 *    the round-32 per-entry jump UX.
 *  - `"jump-oldest"`: Cmd-Shift-Z / Ctrl-Shift-Z WHILE the popover
 *    is already open — jump directly to the oldest ready entry as
 *    a power-user accelerator. A user who hit Cmd-Shift-Z to open
 *    the popover, saw the rows, and decided to revert all the way
 *    back can hit it AGAIN to jump to the bottom without the
 *    arrow-key walk.
 *  - `"focus-prev"` / `"focus-next"`: ArrowUp / ArrowDown — move
 *    focus inside the popover. Only fires when the popover is
 *    open; otherwise the resolver returns `"none"`.
 *  - `"activate"`: Enter / Space — activate the focused row's
 *    Jump-here button. Only fires when the popover is open.
 *  - `"none"`: any other key + modifier combination. The UI does
 *    nothing — the event continues bubbling. */
export type UndoShortcutIntent =
  | "cascade"
  | "open-popover"
  | "jump-oldest"
  | "focus-prev"
  | "focus-next"
  | "activate"
  | "none";

/** Input shape for `resolveUndoShortcut`. Mirrors the KeyboardEvent
 *  fields the resolver inspects so tests can pass synthetic objects
 *  without instantiating a real event. */
export interface UndoShortcutEvent {
  /** `e.key` — the printable key name. Lowercased on lookup so the
   *  resolver matches "z" / "Z" / "ArrowUp" / etc. */
  key: string;
  /** `e.metaKey` — Cmd on macOS, Win key on others (we ignore
   *  Win-key entirely; the resolver flips to `ctrlKey` on non-mac). */
  metaKey: boolean;
  /** `e.ctrlKey` — Ctrl on every platform. */
  ctrlKey: boolean;
  /** `e.shiftKey` — Shift modifier (gates open-popover vs cascade). */
  shiftKey: boolean;
  /** `e.altKey` — Alt/Option. The resolver requires this to be
   *  FALSE for any intent to fire (Alt-Cmd-Z is a system shortcut
   *  on macOS for "redo style"; we don't intercept it). */
  altKey: boolean;
}

/** Whether the cascade-jump popover is currently open. The resolver
 *  uses this to discriminate between `"open-popover"` (popover
 *  closed -> open it) and `"jump-oldest"` (popover already open ->
 *  shortcut acts as the bottom-row accelerator). Arrow-key + Enter
 *  intents are ONLY returned when the popover is open. */
export interface UndoShortcutContext {
  popoverOpen: boolean;
  /** The detected platform. Pass `"mac"` when navigator.platform
   *  indicates macOS / iPadOS, else `"other"`. Default is `"other"`
   *  (most strict — requires Ctrl on systems we can't classify). */
  platform: UndoShortcutPlatform;
}

/** Detect the platform from a `navigator.platform`-shaped string.
 *  Pure — pass `globalThis.navigator?.platform ?? ""` at call site.
 *  Matches the historical "MacIntel" / "MacPPC" / "iPhone" / "iPad"
 *  values that browsers still emit (even after the deprecation of
 *  `navigator.platform` in modern UA-CH; the legacy field is kept
 *  for compatibility on every major browser as of 2026). */
export function detectUndoShortcutPlatform(
  platformString: string,
): UndoShortcutPlatform {
  if (typeof platformString !== "string") return "other";
  const p = platformString.toLowerCase();
  if (
    p.includes("mac") ||
    p.includes("iphone") ||
    p.includes("ipad") ||
    p.includes("ipod")
  ) {
    return "mac";
  }
  return "other";
}

/** Resolve a KeyboardEvent-shaped input into a cascade-undo intent.
 *  Pure — no DOM access, no event mutation. The UI dispatches the
 *  returned intent to its matching handler.
 *
 *  Modifier vocabulary (platform-aware):
 *    - macOS: Cmd-Z / Cmd-Shift-Z (metaKey).
 *    - Other: Ctrl-Z / Ctrl-Shift-Z (ctrlKey).
 *    - Alt/Option must be FALSE for any modifier intent to fire
 *      (Alt-Cmd-Z is the system "redo style" shortcut on macOS).
 *
 *  Defensive: returns `"none"` for any unrecognised key, missing
 *  required modifier, or wrong-platform modifier (e.g. Ctrl-Z on
 *  macOS — that's an ASCII control char on some keyboards; we
 *  defer to the system default). */
export function resolveUndoShortcut(
  event: UndoShortcutEvent,
  context: UndoShortcutContext,
): UndoShortcutIntent {
  // Alt/Option always disqualifies — too many system shortcuts use it.
  if (event.altKey) return "none";

  const key = (event.key || "").toLowerCase();

  // Arrow-key navigation + activation: only fires when the popover
  // is already open. Modifier keys are forbidden (a user pressing
  // Cmd-ArrowUp on the chain editor would otherwise lose their
  // scroll position).
  if (context.popoverOpen) {
    if (key === "arrowup" && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
      return "focus-prev";
    }
    if (key === "arrowdown" && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
      return "focus-next";
    }
    if (
      (key === "enter" || key === " ") &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.shiftKey
    ) {
      return "activate";
    }
  }

  // Primary modifier check — Cmd on mac, Ctrl on other. Off-platform
  // modifiers no-op so the system default (browser undo) is preserved.
  if (key !== "z") return "none";
  const wantMeta = context.platform === "mac";
  const primaryHeld = wantMeta ? event.metaKey : event.ctrlKey;
  const wrongPrimary = wantMeta ? event.ctrlKey : event.metaKey;
  if (!primaryHeld) return "none";
  // Don't fire if BOTH primary modifiers are pressed (some keyboard
  // remappers swap them; we'd rather no-op than misfire).
  if (wrongPrimary) return "none";

  if (event.shiftKey) {
    return context.popoverOpen ? "jump-oldest" : "open-popover";
  }
  return "cascade";
}

/** Human-readable name for a shortcut intent. Used by the
 *  HopperRulesEditor onboarding tooltip + the audit log surface. */
export function describeUndoShortcutIntent(
  intent: UndoShortcutIntent,
): string {
  switch (intent) {
    case "cascade":
      return "Cascade undo";
    case "open-popover":
      return "Open cascade-jump popover";
    case "jump-oldest":
      return "Jump to oldest ready entry";
    case "focus-prev":
      return "Move focus up";
    case "focus-next":
      return "Move focus down";
    case "activate":
      return "Activate focused row";
    case "none":
      return "No shortcut";
  }
}

/** Format the shortcut chord for display in tooltips / help copy.
 *  Returns the platform-correct primary key + Shift suffix. */
export function formatUndoShortcutChord(
  intent: UndoShortcutIntent,
  platform: UndoShortcutPlatform,
): string {
  const primary = platform === "mac" ? "⌘" : "Ctrl";
  switch (intent) {
    case "cascade":
      return `${primary}Z`;
    case "open-popover":
    case "jump-oldest":
      return `${primary}⇧Z`;
    case "focus-prev":
      return "↑";
    case "focus-next":
      return "↓";
    case "activate":
      return "Enter";
    case "none":
      return "";
  }
}

// =====================================================================
// v3.40 Slice 159 — Popover row builder + focus walker (round-33)
// =====================================================================
//
// Round 32 (slice 157) renders the cascade-jump popover as an inline
// {#each undoRing as entry, idx} loop that computes `entryStatus` +
// `plan` + `stepNumber` per row inside the template. That works for
// rendering, but it leaves NO data structure for the keyboard
// navigation walker — the slice 158 resolver returns "focus-next"
// but the UI has no first-class way to ask "given the focused row,
// what's the next ROW INDEX I should focus, skipping disabled rows
// (active target / stale / noop)?".
//
// Slice 159 lifts the per-row derivation into a pure builder and a
// focus walker. `buildJumpableRows(ring, liveRules, now)` returns a
// `JumpableRow[]` array carrying every piece of UI state per entry
// (step number, label, age copy, status discriminant, plan, focusable
// flag). `nextFocusableJumpIndex(rows, current, direction)` walks
// the array in the requested direction skipping non-focusable rows
// and wrapping at the ends.
//
// The UI then uses these in the popover {#each} (no behavioural
// change to the existing rendering — the row shape is just lifted
// out of the template) AND in the keyboard handler (when slice 158
// returns "focus-next" / "focus-prev", call nextFocusableJumpIndex
// to find the next row to focus).

/** One renderable row in the cascade-jump popover. Pre-computed by
 *  `buildJumpableRows` so the UI's {#each} block reads the shape
 *  verbatim without re-deriving status / plan / age per render. */
export interface JumpableRow {
  /** Index into the source ring (oldest-first). Pass to
   *  `applyUndoJump(targetIndex)` when the user activates the row. */
  ringIndex: number;
  /** Step number for display — newest-first 1-based (Step 1 is the
   *  newest entry). Matches the chip's "Step N of M" copy. */
  stepNumber: number;
  /** Human-facing label echoed from the entry. */
  label: string;
  /** Captured-at timestamp (unix-ms). The UI's relative-age
   *  formatter ingests this; carried through verbatim so the row
   *  shape is render-time agnostic. */
  capturedAt: number;
  /** Pre-computed relative-age copy ("just now" / "12s ago" /
   *  "3m ago" / "2h ago"). Computed at build time so a `now`
   *  injectable makes tests deterministic. */
  ageCopy: string;
  /** Live status against the current chain — same discriminant as
   *  `computeUndoStatus`. The UI renders different badges per
   *  variant. */
  status: ReorderUndoStatus;
  /** Pre-computed jump plan (slice 154) — null for the newest row
   *  because that's the cascade button's territory (no "Jump here"
   *  button rendered; an "Active target" badge instead). */
  plan: UndoJumpPlan | null;
  /** Whether this row is the active cascade target (the newest
   *  ready entry). The UI renders the "Active target" badge here. */
  isActiveTarget: boolean;
  /** Whether this row is keyboard-focusable. True only when the
   *  row has a "Jump here" button (older ready entries with a
   *  valid plan). Used by `nextFocusableJumpIndex` to skip past
   *  active-target / stale / noop rows. */
  isFocusable: boolean;
}

/** Direction for `nextFocusableJumpIndex`. Forward walks oldest to
 *  newest in the array (which is visually newest-first because the
 *  popover renders newest at TOP); reverse walks the other way. */
export type JumpFocusDirection = "forward" | "reverse";

/** Build the renderable rows for the cascade-jump popover.
 *
 *  Inputs:
 *    - `ring`: live `ReorderUndoEntry[]` (oldest-first, newest at end).
 *    - `liveRules`: the current chain — passed to `computeUndoStatus`
 *      per entry so the status reflects the live state.
 *    - `now`: unix-ms injectable for deterministic age copy in tests.
 *      Defaults to `Date.now()` at call site.
 *
 *  Output: `JumpableRow[]` in the SAME order as the source ring
 *  (oldest-first). The UI is responsible for the visual order (the
 *  current template renders ascending so newest appears at the
 *  bottom; if a future round wants newest-at-top, reverse the
 *  array here without changing the underlying ring).
 *
 *  Per-row derivation:
 *    - `stepNumber = (ring.length - ringIndex)` (newest = Step 1).
 *    - `status = computeUndoStatus(entry, liveRules)`.
 *    - `isActiveTarget = (ringIndex === ring.length - 1)` (newest).
 *    - `plan = computeUndoJumpPlan(summarizeRingForJump(ring), idx)`
 *      for non-newest rows; null for the active-target row.
 *    - `isFocusable = (!isActiveTarget && status.kind === "ready"
 *      && plan?.is_valid === true)`.
 *
 *  Pure — no DOM access, no event mutation. Constructs a fresh
 *  array; the input ring is never mutated. */
export function buildJumpableRows(
  ring: ReorderUndoEntry[],
  liveRules: Rule[],
  now: number = Date.now(),
): JumpableRow[] {
  if (!Array.isArray(ring) || ring.length === 0) return [];
  const summaries = summarizeRingForJump(ring);
  const result: JumpableRow[] = [];
  for (let idx = 0; idx < ring.length; idx += 1) {
    const entry = ring[idx];
    const isActiveTarget = idx === ring.length - 1;
    const status = computeUndoStatus(entry, liveRules);
    const plan = isActiveTarget
      ? null
      : computeUndoJumpPlan(summaries, idx);
    const isFocusable =
      !isActiveTarget &&
      status.kind === "ready" &&
      plan !== null &&
      plan.is_valid === true;
    result.push({
      ringIndex: idx,
      stepNumber: ring.length - idx,
      label: entry.label,
      capturedAt: entry.capturedAt,
      ageCopy: formatJumpableRowAge(entry.capturedAt, now),
      status,
      plan,
      isActiveTarget,
      isFocusable,
    });
  }
  return result;
}

/** Format a captured-at timestamp as relative age. Mirrors the
 *  inline helper in HopperRulesEditor.svelte so the row builder
 *  carries deterministic copy. Exported so the UI can use the same
 *  vocabulary in tooltips / aria-label suffixes if needed. */
export function formatJumpableRowAge(
  capturedAt: number,
  now: number = Date.now(),
): string {
  const safeNow = Number.isFinite(now) ? now : Date.now();
  const safeAt = Number.isFinite(capturedAt) ? capturedAt : safeNow;
  const deltaMs = Math.max(0, safeNow - safeAt);
  if (deltaMs < 5_000) return "just now";
  const seconds = Math.floor(deltaMs / 1_000);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}

/** Find the next focusable row index given the current focus and a
 *  direction. Returns the next ring index to focus, OR `null` when
 *  there are no focusable rows at all.
 *
 *  Algorithm:
 *    - If `rows` has no focusable rows: return null.
 *    - If `current === null` (no row focused yet): return the FIRST
 *      focusable row index walking the direction (forward = oldest
 *      first, reverse = newest-non-active first).
 *    - Otherwise walk from `current + step` (step = +1 forward, -1
 *      reverse), wrapping at the ends of the array. Skip rows where
 *      `isFocusable === false`. Stop when we land on a focusable row
 *      OR when we wrap back to `current` (which means no other
 *      focusable rows exist; return null so the UI can keep the
 *      current focus rather than blink it away).
 *
 *  Defensive: empty rows -> null; out-of-range current -> treat as
 *  null (walk from start). Pure — no state mutation. */
export function nextFocusableJumpIndex(
  rows: JumpableRow[],
  current: number | null,
  direction: JumpFocusDirection,
): number | null {
  if (!Array.isArray(rows) || rows.length === 0) return null;
  const focusable = rows.filter((r) => r.isFocusable);
  if (focusable.length === 0) return null;

  const step = direction === "forward" ? 1 : -1;

  // Normalise current — out-of-range treated as "no current".
  const validCurrent =
    current !== null &&
    Number.isInteger(current) &&
    current >= 0 &&
    current < rows.length;

  if (!validCurrent) {
    if (direction === "forward") {
      return focusable[0].ringIndex;
    }
    return focusable[focusable.length - 1].ringIndex;
  }

  // Walk forward/reverse from current + step, wrapping. Stop after
  // at most rows.length iterations to avoid infinite loops on
  // degenerate input.
  const len = rows.length;
  let idx = (current! + step + len) % len;
  for (let i = 0; i < len; i += 1) {
    if (rows[idx].isFocusable) return rows[idx].ringIndex;
    idx = (idx + step + len) % len;
  }
  return null;
}

/** Count focusable rows. Convenience for the popover header's
 *  "N jumpable steps" copy. */
export function countFocusableJumpRows(rows: JumpableRow[]): number {
  if (!Array.isArray(rows)) return 0;
  return rows.filter((r) => r.isFocusable).length;
}

// =====================================================================
// v3.40 Slice 161 — Absolute-timestamp format + ring-health describer
// (round-33)
// =====================================================================
//
// Round 32's closing notes listed two power-user surfaces as next
// candidates: (a) absolute-timestamp toggle for the cascade-jump
// popover (round 32 ships relative-only — "12s ago" / "3m ago" —
// which is great for fresh undo cycles but useless for cross-
// session audit: a user who closed and reopened the panel sees
// "3h ago" but doesn't know if that was last lunch or yesterday
// morning); (b) ring-health summary so the user understands at a
// glance how many ready vs stale vs noop entries the ring holds.
//
// Slice 161 ships both as pure helpers. The UI slice (162) wires
// the toggle + the summary header.

/** Display mode for the cascade-jump popover's per-row timestamp.
 *  Round 32 shipped relative-only ("12s ago"); slice 161 adds
 *  absolute as an opt-in toggle persisted in localStorage. */
export type CaptureTimestampMode = "relative" | "absolute";

/** Format a captured-at timestamp as an absolute clock + date
 *  string. Pure helper — uses `Intl.DateTimeFormat` so locale +
 *  timezone follow the user's environment.
 *
 *  Format vocabulary (chosen for cross-session audit, not for the
 *  fresh-undo case):
 *    - SAME DAY as `now`: "Today 14:23" (uppercase Today,
 *      24-hour clock so 11 PM doesn't ambiguously read as "11").
 *    - YESTERDAY: "Yesterday 14:23".
 *    - SAME YEAR but older: "Apr 15, 14:23" (month + day + clock).
 *    - DIFFERENT YEAR: "Apr 15 2025, 14:23" (year carries so a
 *      paralegal who left the panel open across a Dec/Jan
 *      boundary doesn't see two days that read identically).
 *
 *  Defensive: NaN / non-finite input -> empty string (the popover
 *  row falls back to the relative copy at call site). */
export function formatAbsoluteCapture(
  capturedAt: number,
  now: number = Date.now(),
): string {
  if (!Number.isFinite(capturedAt)) return "";
  const at = new Date(capturedAt);
  const reference = new Date(Number.isFinite(now) ? now : Date.now());

  // Same-day check uses Y/M/D triple — comparing date strings
  // would mis-handle timezones in some Intl configurations.
  const sameYear = at.getFullYear() === reference.getFullYear();
  const sameMonth = at.getMonth() === reference.getMonth();
  const sameDay = at.getDate() === reference.getDate();

  const yesterday = new Date(reference);
  yesterday.setDate(reference.getDate() - 1);
  const isYesterday =
    at.getFullYear() === yesterday.getFullYear() &&
    at.getMonth() === yesterday.getMonth() &&
    at.getDate() === yesterday.getDate();

  const hh = String(at.getHours()).padStart(2, "0");
  const mm = String(at.getMinutes()).padStart(2, "0");
  const clock = `${hh}:${mm}`;

  if (sameYear && sameMonth && sameDay) {
    return `Today ${clock}`;
  }
  if (isYesterday) {
    return `Yesterday ${clock}`;
  }

  const monthAbbr = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ][at.getMonth()];
  const day = at.getDate();

  if (sameYear) {
    return `${monthAbbr} ${day}, ${clock}`;
  }
  return `${monthAbbr} ${day} ${at.getFullYear()}, ${clock}`;
}

/** Format a row's timestamp respecting the user's chosen display
 *  mode. Convenience wrapper around `formatJumpableRowAge` (for
 *  "relative") and `formatAbsoluteCapture` (for "absolute"). */
export function formatJumpableRowTimestamp(
  capturedAt: number,
  mode: CaptureTimestampMode,
  now: number = Date.now(),
): string {
  if (mode === "absolute") {
    return formatAbsoluteCapture(capturedAt, now);
  }
  return formatJumpableRowAge(capturedAt, now);
}

/** A summary breakdown of the cascade-undo ring's health. Counts
 *  every entry by current-chain status (ready / stale / noop) plus
 *  the active-target row count (always 0 or 1). Used by the
 *  popover header copy + the audit log. */
export interface RingHealthSummary {
  /** Total entries in the ring (newest + older combined). */
  total: number;
  /** Number of entries with kind="ready" — the user can jump to
   *  any of them. */
  ready: number;
  /** Number of entries with kind="stale" — chain drifted away from
   *  the snapshot. UI surfaces these as disabled Unavailable badges. */
  stale: number;
  /** Number of entries with kind="noop" — snapshot already matches
   *  the current chain. UI surfaces these as muted No change badges. */
  noop: number;
  /** Number of focusable entries (ready + non-active). The popover
   *  header reads "N jumpable steps" for the user. */
  focusable: number;
}

/** Compute a RingHealthSummary from slice 159's `jumpableRows`
 *  array. Pure — folds the rows by status discriminant. Safe on
 *  empty / null input. */
export function summarizeRingHealth(
  rows: JumpableRow[],
): RingHealthSummary {
  if (!Array.isArray(rows) || rows.length === 0) {
    return { total: 0, ready: 0, stale: 0, noop: 0, focusable: 0 };
  }
  let ready = 0;
  let stale = 0;
  let noop = 0;
  let focusable = 0;
  for (const row of rows) {
    if (row.status.kind === "ready") ready += 1;
    else if (row.status.kind === "stale") stale += 1;
    else if (row.status.kind === "noop") noop += 1;
    if (row.isFocusable) focusable += 1;
  }
  return { total: rows.length, ready, stale, noop, focusable };
}

/** Render a RingHealthSummary as a human-readable copy line for the
 *  popover header. Examples:
 *    - {total: 0}              -> "No undo steps queued"
 *    - {total: 1, ready: 1}    -> "1 undo step ready"
 *    - {total: 3, ready: 3}    -> "3 undo steps ready"
 *    - {total: 5, ready: 3, stale: 2}
 *                              -> "3 of 5 undo steps jumpable (2 stale)"
 *    - {total: 5, ready: 2, stale: 1, noop: 2}
 *                              -> "2 of 5 undo steps jumpable (1 stale, 2 unchanged)"
 *    - {total: 5, ready: 5, noop: 1} (rare)
 *                              -> "5 of 5 undo steps jumpable (1 unchanged)"
 *    - {total: 4, ready: 0, stale: 4}
 *                              -> "No jumpable steps (4 stale)"
 *
 *  Quantities are pluralisation-aware ("step"/"steps" /
 *  "stale"/"stale" — both invariant). Skips zero-count parentheticals
 *  for cleanliness. */
export function describeRingHealth(summary: RingHealthSummary): string {
  if (summary.total === 0) {
    return "No undo steps queued";
  }
  const stepNoun = summary.total === 1 ? "step" : "steps";
  if (summary.ready === 0) {
    const parts: string[] = [];
    if (summary.stale > 0) parts.push(`${summary.stale} stale`);
    if (summary.noop > 0) parts.push(`${summary.noop} unchanged`);
    const suffix = parts.length > 0 ? ` (${parts.join(", ")})` : "";
    return `No jumpable ${stepNoun}${suffix}`;
  }
  if (summary.ready === summary.total) {
    if (summary.total === 1) return `1 undo step ready`;
    return `${summary.total} undo ${stepNoun} ready`;
  }
  // Mixed: ready + (stale and/or noop).
  const parts: string[] = [];
  if (summary.stale > 0) parts.push(`${summary.stale} stale`);
  if (summary.noop > 0) parts.push(`${summary.noop} unchanged`);
  const suffix = parts.length > 0 ? ` (${parts.join(", ")})` : "";
  return `${summary.ready} of ${summary.total} undo ${stepNoun} jumpable${suffix}`;
}

/** Toggle the next CaptureTimestampMode. Convenience for the UI's
 *  toggle button — pure flip between the two values. */
export function toggleCaptureTimestampMode(
  mode: CaptureTimestampMode,
): CaptureTimestampMode {
  return mode === "relative" ? "absolute" : "relative";
}

/** Human-readable label for the toggle button. */
export function describeCaptureTimestampMode(
  mode: CaptureTimestampMode,
): string {
  return mode === "absolute" ? "Absolute time" : "Relative time";
}

// ─── Slice 163 — manual rule reorder primitive (round-34) ────────────
//
// Until now the rule chain could only be reordered one position at a
// time via the per-row up/down arrows (moveUp / moveDown in
// HopperRulesEditor): dragging rule #9 to position #2 took SEVEN
// clicks. Round 34 adds drag-to-reorder (mouse) plus Alt+Arrow
// keyboard reorder (a11y). This slice is the load-bearing pure
// array-move primitive both the drag handler and the keyboard
// handler compose on top of.
//
// The contract is deliberately "final-index" semantics: `to` is the
// index the moved rule ends up at in the RESULT array (0..len-1),
// NOT a raw splice insertion point. That keeps the keyboard path
// trivial (move-up => to = from-1) and gives the drag path one
// well-defined target to resolve to (slice 164's resolveDropIndex
// converts a hover-row + edge into this final index).

/** Result of a manual rule move. `moved` is the load-bearing signal
 *  the UI checks to decide whether to persist + announce: a no-op
 *  move returns the ORIGINAL `rules` reference (=== check, same
 *  convention as applyReorderProposal) with `moved: false`, so the
 *  Svelte caller can skip the slabHopperSetRules round-trip and the
 *  aria-live announcement entirely. */
export interface RuleMoveResult {
  /** The reordered chain. Identical reference to the input when the
   *  move was a no-op (out-of-range / NaN / from === to). */
  rules: Rule[];
  /** True iff the chain actually changed. */
  moved: boolean;
  /** Source index, echoed back (normalised; -1 when the input index
   *  was invalid). */
  from: number;
  /** Destination index in the RESULT array, echoed back (normalised;
   *  -1 when invalid). */
  to: number;
}

/** Move the rule at `from` so it lands at index `to` in the result.
 *
 *  Pure — never mutates the input array. `to` is the FINAL resting
 *  index (0..len-1), not a splice insertion point: moving rule 0 to
 *  `to = 2` in [A,B,C,D] yields [B,C,A,D] (A ends up at index 2).
 *
 *  No-op (returns the SAME array reference, moved:false) when:
 *    - `rules` is empty,
 *    - `from` or `to` is non-integer / NaN / out of [0,len),
 *    - `from === to`.
 *  The reference-equality no-op lets the UI skip persistence + the
 *  reorder announcement, matching applyReorderProposal's convention. */
export function moveRuleToIndex(
  rules: Rule[],
  from: number,
  to: number,
): RuleMoveResult {
  const len = rules.length;
  const f = Number.isInteger(from) ? from : -1;
  const t = Number.isInteger(to) ? to : -1;
  if (len === 0 || f < 0 || f >= len || t < 0 || t >= len || f === t) {
    return {
      rules,
      moved: false,
      from: f >= 0 && f < len ? f : -1,
      to: t >= 0 && t < len ? t : -1,
    };
  }
  const next = rules.slice();
  const [item] = next.splice(f, 1);
  next.splice(t, 0, item);
  return { rules: next, moved: true, from: f, to: t };
}

/** Human-facing announcement copy for a completed rule move, for the
 *  keyboard reorder path's aria-live region + the drag-drop toast.
 *
 *  Returns "" for a no-op move (the caller suppresses the
 *  announcement entirely rather than reading a spurious "no change"
 *  to a screen-reader user). Positions are 1-based for display;
 *  falls back to "Rule N" when the rule has no name. */
export function describeRuleMove(
  ruleName: string,
  result: RuleMoveResult,
  total: number,
): string {
  if (!result.moved) return "";
  const name = ruleName.trim() || `Rule ${result.from + 1}`;
  return `Moved ${name} to position ${result.to + 1} of ${total}`;
}

// ─── Slice 164 — drag drop-index resolver (round-34) ─────────────────
//
// The geometry layer between a live drag gesture and slice 163's
// moveRuleToIndex. During a drag the UI knows two things from the
// dragover event: which row the pointer is over (`hoverIndex`) and
// whether it's nearer that row's TOP or BOTTOM edge (the "drop
// edge"). This slice converts (from, hoverIndex, edge) into the
// FINAL resting index moveRuleToIndex consumes.
//
// The load-bearing subtlety is the source-removal shift. The drop is
// conceptually "insert the dragged rule at gap G in the ORIGINAL
// array", where:
//     edge "before" -> G = hoverIndex
//     edge "after"  -> G = hoverIndex + 1
// But moveRuleToIndex removes the source FIRST, so when dropping
// BELOW the source (G > from) every row after `from` has shifted
// left by one and the final index is G - 1. When dropping at or
// above the source the final index is just G. Getting this wrong
// produces an off-by-one that lands the rule one slot from where
// the user aimed — the classic drag-reorder bug.

/** Which edge of a hovered row the pointer is nearer. "before" =>
 *  drop in the gap ABOVE the row; "after" => the gap BELOW it. */
export type DropEdge = "before" | "after";

/** Classify the drop edge from the pointer's Y offset within a row's
 *  bounding box. Top half => "before", bottom half (>= midpoint) =>
 *  "after". Defensive: a non-finite / non-positive rowHeight falls
 *  back to "before" (the safe gap-above default). */
export function dropEdgeFromOffset(
  offsetY: number,
  rowHeight: number,
): DropEdge {
  if (!Number.isFinite(offsetY) || !Number.isFinite(rowHeight) || rowHeight <= 0) {
    return "before";
  }
  return offsetY >= rowHeight / 2 ? "after" : "before";
}

/** Resolve a drag drop into the FINAL resting index for
 *  moveRuleToIndex. Returns -1 for invalid inputs (empty chain,
 *  out-of-range / NaN `from` or `hoverIndex`).
 *
 *  The returned index may equal `from` (a no-op drop — dropping a
 *  rule back into its own gap); moveRuleToIndex handles that as a
 *  no-op, and `isNoopDrop` lets the UI suppress the drop indicator
 *  for it. Result is always in [0, len-1] for valid inputs. */
export function resolveDropIndex(
  from: number,
  hoverIndex: number,
  edge: DropEdge,
  len: number,
): number {
  if (!Number.isInteger(len) || len <= 0) return -1;
  const f = Number.isInteger(from) ? from : -1;
  const h = Number.isInteger(hoverIndex) ? hoverIndex : -1;
  if (f < 0 || f >= len || h < 0 || h >= len) return -1;
  // Gap index in the ORIGINAL array (0..len).
  const gap = edge === "after" ? h + 1 : h;
  // Source-removal shift: dropping below the source pulls everything
  // after `from` left by one.
  const final = gap > f ? gap - 1 : gap;
  // Clamp defensively — `gap` is bounded by len so this is a no-op in
  // practice, but it pins the contract for a future caller.
  return Math.max(0, Math.min(len - 1, final));
}

/** True iff the drop wouldn't change the chain order — either the
 *  resolved index equals the source, or the inputs are invalid. The
 *  UI uses this to HIDE the drop indicator when hovering the source
 *  rule's own gap (the two gaps flanking the dragged row), so the
 *  user never sees a misleading insertion line for a move that does
 *  nothing. */
export function isNoopDrop(
  from: number,
  hoverIndex: number,
  edge: DropEdge,
  len: number,
): boolean {
  const resolved = resolveDropIndex(from, hoverIndex, edge, len);
  return resolved === -1 || resolved === from;
}
