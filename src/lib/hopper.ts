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
