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
