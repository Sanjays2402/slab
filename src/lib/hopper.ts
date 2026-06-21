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
