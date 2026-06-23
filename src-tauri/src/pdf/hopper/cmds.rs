//! Hopper Tauri command surface.
//!
//! Six commands expose the registry + service over the Tauri IPC
//! boundary; all are thin wrappers around the underlying module and
//! consume the shared [`HopperService`] managed in `setup()`.
//!
//! ## Commands
//!
//! | command                          | what it does                                  |
//! | -------------------------------- | --------------------------------------------- |
//! | `slab_hopper_list_watches`       | return every saved [`Watch`]                  |
//! | `slab_hopper_add_watch`          | persist a new [`Watch`] and re-arm watcher    |
//! | `slab_hopper_remove_watch`       | drop a [`Watch`] by id                        |
//! | `slab_hopper_set_enabled`        | toggle a [`Watch`]'s active flag              |
//! | `slab_hopper_list_runs`          | tail of the [`RunRecord`] history             |
//! | `slab_hopper_run_now`            | manually dispatch a single file through the   |
//! |                                  | pipeline (UI "re-run" button)                 |
//!
//! ## Title provider bridge
//!
//! [`OllamaTitleProvider`] adapts the async [`AiProvider`] trait to
//! the sync [`TitleProvider`] expected by the pipeline. The bridge
//! uses `tokio::runtime::Handle::current().block_on` — safe because
//! the pipeline runs on `spawn_blocking`, never on a tokio worker.
//!
//! The prompt is engineered for short, filename-grade titles
//! ("Acme NDA 2025", not "This document appears to be a confidential
//! non-disclosure agreement between Acme Corp..."). On any error,
//! including Ollama-not-running, we return `None` so the pipeline
//! falls back to the input file's stem — graceful degradation, never
//! a hard failure.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::Manager;

use super::log::RunRecord;
use super::pipeline::TitleProvider;
use super::registry::{Watch, WatchInput};
use super::rules::{evaluate_rules, ResolvedRouting, Rule, RuleContext};
use super::watcher::HopperService;
use crate::ai::{AiProvider, ChatMessage, ChatOpts, ChatRole};

/// Result alias — commands return `Result<T, String>` because Tauri
/// IPC needs a serializable error type and we always surface the
/// human-readable reason to the frontend.
type CmdResult<T> = Result<T, String>;

/// `slab_hopper_list_watches` — return every saved [`Watch`] in
/// insertion order. Cheap (single sqlite scan).
#[tauri::command]
pub fn slab_hopper_list_watches(svc: tauri::State<'_, HopperService>) -> CmdResult<Vec<Watch>> {
    let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
    reg.list().map_err(|e| format!("registry list: {e}"))
}

/// `slab_hopper_add_watch` — persist a new watch, immediately re-arm
/// the underlying [`notify`] watcher so the new directory is live
/// before the IPC call returns.
#[tauri::command]
pub fn slab_hopper_add_watch(
    svc: tauri::State<'_, HopperService>,
    input: WatchInput,
) -> CmdResult<Watch> {
    let id = {
        let mut reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
        reg.add(input).map_err(|e| format!("registry add: {e}"))?
    };
    svc.reload_watches()?;
    let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
    reg.get(id)
        .map_err(|e| format!("registry get: {e}"))?
        .ok_or_else(|| "newly-added watch vanished".to_string())
}

/// `slab_hopper_remove_watch` — delete a watch and refresh the
/// `notify` subscription set.
#[tauri::command]
pub fn slab_hopper_remove_watch(svc: tauri::State<'_, HopperService>, id: i64) -> CmdResult<()> {
    {
        let mut reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
        reg.remove(id)
            .map_err(|e| format!("registry remove: {e}"))?;
    }
    svc.reload_watches()?;
    Ok(())
}

/// `slab_hopper_set_enabled` — toggle a watch's active flag and
/// re-arm the watcher set.
#[tauri::command]
pub fn slab_hopper_set_enabled(
    svc: tauri::State<'_, HopperService>,
    id: i64,
    enabled: bool,
) -> CmdResult<()> {
    {
        let mut reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
        reg.set_enabled(id, enabled)
            .map_err(|e| format!("registry set_enabled: {e}"))?;
    }
    svc.reload_watches()?;
    Ok(())
}

/// `slab_hopper_list_runs` — most recent `limit` [`RunRecord`]s,
/// newest first. The frontend tails this for the live log panel.
#[tauri::command]
pub fn slab_hopper_list_runs(
    svc: tauri::State<'_, HopperService>,
    limit: i64,
) -> CmdResult<Vec<RunRecord>> {
    let log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
    log.list_recent(limit)
        .map_err(|e| format!("log list_recent: {e}"))
}

/// `slab_hopper_run_now` — manually dispatch a single file through
/// the pipeline, bypassing the debounce queue. Used by the "re-run"
/// button on each row of the runs table.
#[tauri::command]
pub fn slab_hopper_run_now(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
    path: String,
) -> CmdResult<()> {
    svc.run_now(watch_id, PathBuf::from(path))
}

/// `slab_hopper_describe` — return a JSON object summarizing the
/// current service state (watch count, run count, started?). Useful
/// for the panel's header row and for "are we live?" status pings.
#[tauri::command]
pub fn slab_hopper_describe(svc: tauri::State<'_, HopperService>) -> CmdResult<HopperStatus> {
    let watch_count = {
        let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
        reg.list().map_err(|e| e.to_string())?.len() as i64
    };
    let run_count = {
        let log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
        log.list_recent(1000).map_err(|e| e.to_string())?.len() as i64
    };
    Ok(HopperStatus {
        watch_count,
        run_count,
        version: "v3.20.0".into(),
    })
}

/// Status payload returned by `slab_hopper_describe`.
#[derive(Debug, Clone, Serialize)]
pub struct HopperStatus {
    pub watch_count: i64,
    pub run_count: i64,
    pub version: String,
}

// ---------------------------------------------------------------------
// v3.21.0 — rule CRUD + live test
// ---------------------------------------------------------------------

/// `slab_hopper_get_rules` — return the ordered list of routing rules
/// for a given watch. Empty `[]` means "no conditional routing; use the
/// watch defaults" (v3.20.0 behaviour).
#[tauri::command]
pub fn slab_hopper_get_rules(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
) -> CmdResult<Vec<Rule>> {
    let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
    reg.get_rules(watch_id)
        .map_err(|e| format!("registry get_rules: {e}"))
}

/// `slab_hopper_set_rules` — atomically replace the rule list for a
/// watch. Order is significant (first-match-wins). The new rules take
/// effect on the next file the watcher dispatches; no restart needed.
#[tauri::command]
pub fn slab_hopper_set_rules(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
    rules: Vec<Rule>,
) -> CmdResult<()> {
    let mut reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
    reg.set_rules(watch_id, &rules)
        .map_err(|e| format!("registry set_rules: {e}"))
}

/// Payload returned by `slab_hopper_test_rules` — what would happen
/// if this `filename` arrived under this watch with these rules.
#[derive(Debug, Clone, Serialize)]
pub struct RuleTestResult {
    /// Index of the matching rule in the input array, or `None` for
    /// the watch defaults.
    pub matched_index: Option<usize>,
    /// Name of the matching rule, or `None` for watch defaults.
    pub matched_rule: Option<String>,
    /// The recipe that would run.
    pub recipe_id: Option<String>,
    /// The output directory the file would land in.
    pub output_dir: String,
    /// The rename pattern that would apply (may be `None`).
    pub rename_pattern: Option<String>,
}

impl RuleTestResult {
    fn from_resolution(rules: &[Rule], r: &ResolvedRouting) -> Self {
        let matched_index = r
            .matched_rule
            .as_deref()
            .and_then(|name| rules.iter().position(|x| x.name == name));
        Self {
            matched_index,
            matched_rule: r.matched_rule.clone(),
            recipe_id: r.recipe_id.clone(),
            output_dir: r.output_dir.clone(),
            rename_pattern: r.rename_pattern.clone(),
        }
    }
}

/// `slab_hopper_test_rules` — given a watch id, a candidate filename,
/// and an optional in-flight rule list, return which rule would match
/// and where the file would file. Used by the rule editor's "Test
/// against last 5 files" live preview — users see green/red ticks per
/// rule before saving.
///
/// `size_bytes` and `page_count` are optional hints from the caller
/// (e.g. for files already known to the run-log); when absent the
/// predicate context uses zero / `None`. `text_sample` is currently
/// passed as `None` — text-aware predicates require a server-side
/// extraction pass which lands in a later slice.
#[tauri::command]
pub fn slab_hopper_test_rules(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
    filename: String,
    size_bytes: Option<u64>,
    page_count: Option<u32>,
    candidate_rules: Option<Vec<Rule>>,
) -> CmdResult<RuleTestResult> {
    let watch = {
        let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
        reg.get(watch_id)
            .map_err(|e| format!("registry get: {e}"))?
            .ok_or_else(|| format!("watch id {watch_id} not found"))?
    };
    let rules = match candidate_rules {
        Some(rs) => rs,
        None => {
            let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.get_rules(watch_id)
                .map_err(|e| format!("registry get_rules: {e}"))?
        }
    };
    let parent = String::new();
    let ctx = RuleContext {
        filename: filename.as_str(),
        parent_dir: parent.as_str(),
        size_bytes: size_bytes.unwrap_or(0),
        page_count,
        text_sample: None,
    };
    let resolved = evaluate_rules(&rules, &watch, &ctx);
    Ok(RuleTestResult::from_resolution(&rules, &resolved))
}

// ---------------------------------------------------------------------
// v3.40 Slice 80 — rule coverage analyzer command
// ---------------------------------------------------------------------
//
// Wraps the pure-data [`super::coverage::compute_coverage`] primitive
// with a one-shot Tauri command that:
//
// 1. Sources the sample list FROM THE WATCH'S RECENT RUN LOG by
//    default (the most useful question is "how would this chain have
//    handled my actual recent traffic?"), or from a caller-supplied
//    list when the editor wants to test against synthesised samples.
// 2. Evaluates the in-flight, possibly-unsaved rule chain (so users
//    see coverage shift live as they edit, no save round-trip).
// 3. Returns a [`super::coverage::RuleCoverageReport`] the UI can
//    render as a per-rule bar strip + a fall-through count + a
//    "dead at position" diagnostic chip.
//
// Default sample source: the latest [`super::log::RunRecord`]s filtered
// to `watch_id`. We pull at most `sample_limit` (default 100, capped at
// 1000 to keep IPC payload bounded even on an enormous log). Each row
// contributes its `input_path`'s basename + the watch's recorded
// `duration_ms` proxy is NOT used; size + page count are unknown in the
// log so we default them to zero / None. Text-aware predicates still
// won't fire — that's a known limitation matching the live preview's
// behaviour and is documented at the call site.

/// `slab_hopper_rule_coverage` — evaluate a rule chain against the
/// watch's recent run log (or a caller-supplied sample list), returning
/// per-rule first-match + would-match counts and the fall-through
/// count.
///
/// `candidate_rules`: optional in-flight rule list; falls back to the
/// watch's persisted rules so the command works on saved chains too.
///
/// `samples`: optional explicit sample list; when `None`, the command
/// pulls the most-recent `sample_limit` runs (default 100, capped at
/// 1000) from the run log filtered to `watch_id`. Each run contributes
/// its `input_path` basename with `size_bytes=0` and `page_count=None`
/// because the run log doesn't persist those — text-aware and size-
/// aware predicates won't fire on log-sourced samples, matching the
/// live preview's existing limitation.
///
/// Returns a [`super::coverage::RuleCoverageReport`] alongside the
/// effective sample count so the UI can render "<rule> X / N matched"
/// without recomputing.
#[tauri::command]
pub fn slab_hopper_rule_coverage(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
    candidate_rules: Option<Vec<Rule>>,
    samples: Option<Vec<super::coverage::RuleSample>>,
    sample_limit: Option<i64>,
) -> CmdResult<super::coverage::RuleCoverageReport> {
    // Resolve the rule chain — caller's in-flight rules win; otherwise
    // we read whatever's persisted for the watch.
    let rules = match candidate_rules {
        Some(rs) => rs,
        None => {
            let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.get_rules(watch_id)
                .map_err(|e| format!("registry get_rules: {e}"))?
        }
    };

    // Resolve the sample list — caller wins; else fall back to recent
    // log entries for this watch. Clamp the sample limit defensively.
    let resolved_samples: Vec<super::coverage::RuleSample> = match samples {
        Some(s) => s,
        None => {
            let cap = clamp_sample_limit(sample_limit);
            let over_read = sample_over_read(cap);
            let runs = {
                let log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
                log.list_recent(over_read)
                    .map_err(|e| format!("hopper log list_recent: {e}"))?
            };
            samples_from_runs(&runs, watch_id, cap as usize)
        }
    };

    Ok(super::coverage::compute_coverage(&rules, &resolved_samples))
}

/// Clamp the caller's `sample_limit` to `[1, 1000]`, defaulting to 100.
/// The 1000 ceiling keeps the IPC payload bounded even on a huge log;
/// the 1 floor stops a caller from accidentally asking for zero (which
/// would return an all-zero report and look like a bug).
fn clamp_sample_limit(input: Option<i64>) -> i64 {
    input.unwrap_or(100).clamp(1, 1000)
}

/// Compute the over-read size for the global recent-tail scan that
/// powers per-watch sampling. The hopper log doesn't expose a per-watch
/// `list_recent`, so we over-fetch and filter. The 4x multiplier keeps
/// the post-filter sample count meaningful when the target watch is a
/// small fraction of total traffic, while the 10_000 ceiling guards
/// against a runaway query on an enormous log.
fn sample_over_read(cap: i64) -> i64 {
    cap.saturating_mul(4).min(10_000)
}

/// Derive a [`super::coverage::RuleSample`] list from a tail of run
/// records: filter to `watch_id`, take the first `cap`, reduce each
/// `input_path` to its basename, and zero out the size/page/text axes
/// (the run log doesn't persist them).
fn samples_from_runs(
    runs: &[super::log::RunRecord],
    watch_id: i64,
    cap: usize,
) -> Vec<super::coverage::RuleSample> {
    runs.iter()
        .filter(|r| r.watch_id == watch_id)
        .take(cap)
        .map(|r| super::coverage::RuleSample {
            // Reduce the absolute path to its basename so glob / regex
            // predicates evaluate against the bare filename, matching
            // how `evaluate_rules` is invoked from the live watcher
            // pipeline.
            filename: std::path::Path::new(&r.input_path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| r.input_path.clone()),
            size_bytes: 0,
            page_count: None,
            text_sample: None,
        })
        .collect()
}

// ─── Slice 84 — sample drilldown command surface ─────────────────────
//
// `slab_hopper_sample_drilldown` lets the coverage panel answer
// "show me the files in this bucket" when the user clicks a row.
// The command:
//
// 1. Resolves the rule chain (caller-supplied in-flight rules win;
//    else reads from the registry, matching slab_hopper_rule_coverage).
// 2. Resolves the samples (caller wins; else pulls from the run log
//    via the same samples_from_runs helper as the coverage command).
// 3. Calls compute_sample_drilldown(rules, samples, bucket, cap).
//
// We deliberately reuse the same sample/limit semantics as the
// coverage command (clamp_sample_limit + sample_over_read +
// samples_from_runs) so a click on a coverage row drills into the
// EXACT same sample set the coverage report counted — anything else
// would surface "27 fall-throughs" in the header but only show 23
// in the drilldown, which would read as a bug.

/// Clamp the caller's preview cap to `[1, 1000]`, defaulting to 25.
/// Lower ceiling than the analyzer's `[1, 5000]` because the IPC
/// payload here is heavier — each sample carries the full filename +
/// the size/page/text axes, vs the coverage report's per-rule
/// counts. 25 default matches a typical popover "first page" and
/// stays well under the dropdown's scroll budget.
fn clamp_preview_cap(input: Option<i64>) -> i64 {
    input.unwrap_or(25).clamp(1, 1000)
}

/// `slab_hopper_sample_drilldown` — evaluate a rule chain against
/// the watch's recent run log (or a caller-supplied sample list)
/// and return the samples in a specific bucket (one rule's
/// first-match list, or the fall-through list).
///
/// `bucket` is a [`super::coverage::SampleBucket`]: either
/// `{kind: "rule", index: N}` to drill into rule N's first_match
/// pool, or `{kind: "fallthrough"}` to see what fell through to the
/// watch defaults.
///
/// `candidate_rules`, `samples`, and `sample_limit` mirror
/// [`slab_hopper_rule_coverage`] so a click on a coverage row drills
/// into the EXACT same sample set the coverage report counted.
///
/// `preview_cap` (default 25, clamped to [1, 1000]) is the maximum
/// number of samples returned in the drilldown payload — independent
/// of `sample_limit` (which caps the chain-walk input size).
#[tauri::command]
pub fn slab_hopper_sample_drilldown(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
    bucket: super::coverage::SampleBucket,
    candidate_rules: Option<Vec<Rule>>,
    samples: Option<Vec<super::coverage::RuleSample>>,
    sample_limit: Option<i64>,
    preview_cap: Option<i64>,
) -> CmdResult<super::coverage::SampleDrilldown> {
    // Resolve the rule chain — same precedence as the coverage cmd.
    let rules = match candidate_rules {
        Some(rs) => rs,
        None => {
            let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.get_rules(watch_id)
                .map_err(|e| format!("registry get_rules: {e}"))?
        }
    };

    // Resolve the sample list — same precedence as the coverage cmd.
    let resolved_samples: Vec<super::coverage::RuleSample> = match samples {
        Some(s) => s,
        None => {
            let cap = clamp_sample_limit(sample_limit);
            let over_read = sample_over_read(cap);
            let runs = {
                let log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
                log.list_recent(over_read)
                    .map_err(|e| format!("hopper log list_recent: {e}"))?
            };
            samples_from_runs(&runs, watch_id, cap as usize)
        }
    };

    let preview = clamp_preview_cap(preview_cap) as usize;
    Ok(super::coverage::compute_sample_drilldown(
        &rules,
        &resolved_samples,
        bucket,
        preview,
    ))
}

// ---------------------------------------------------------------------
// v3.22.0 — Hopper Loop: batch backfill commands
// ---------------------------------------------------------------------
//
// Three thin commands wrap [`backfill::plan_backfill`],
// [`backfill::execute_backfill`], and the new
// `HopperLog::list_backfill_runs` history reader. Plan is pure and
// cheap; execute persists the run after the moves complete so the
// "Recent backfills" disclosure populates immediately.

/// `slab_hopper_plan_backfill` — dry-run the rule chain against an
/// existing folder. Resolves the watch by id, loads its current rules,
/// walks the folder, returns a [`BackfillReport`]. The frontend renders
/// this report as a table; nothing is moved.
///
/// `opts` (v3.39 round-10) controls whether sub-folders are swept.
/// `None` preserves the v3.22 single-level default so pre-v3.39
/// callers stay behaviourally identical. Recursive scans honour the
/// `max_depth` cap when set; an unset `max_depth` walks the whole
/// tree.
#[tauri::command]
pub fn slab_hopper_plan_backfill(
    svc: tauri::State<'_, HopperService>,
    watch_id: i64,
    folder: Option<String>,
    opts: Option<super::backfill::PlanOptions>,
) -> CmdResult<super::backfill::BackfillReport> {
    let (watch, rules) = {
        let reg = svc.registry.lock().unwrap_or_else(|p| p.into_inner());
        let watch = reg
            .get(watch_id)
            .map_err(|e| format!("registry get: {e}"))?
            .ok_or_else(|| format!("watch {watch_id} not found"))?;
        let rules = reg
            .get_rules(watch_id)
            .map_err(|e| format!("registry get_rules: {e}"))?;
        (watch, rules)
    };
    // Default to the watch's configured source_dir — the most common
    // call pattern. The UI also accepts an arbitrary folder picker for
    // "test against a sample folder" workflows.
    let target = folder.unwrap_or_else(|| watch.source_dir.clone());
    let opts = opts.unwrap_or_default();
    let report = super::backfill::plan_backfill_with_options(
        std::path::Path::new(&target),
        &watch,
        &rules,
        &opts,
    );
    Ok(report)
}

/// `slab_hopper_execute_backfill` — commit a previously-approved
/// [`BackfillReport`]. Performs the moves idempotently and writes a
/// [`BackfillRun`] row to the history table before returning.
#[tauri::command]
pub fn slab_hopper_execute_backfill(
    svc: tauri::State<'_, HopperService>,
    report: super::backfill::BackfillReport,
) -> CmdResult<super::backfill::BackfillRun> {
    let run = super::backfill::execute_backfill(&report);
    // Best-effort persist — surfacing an error here would discard the
    // (already-completed) moves info, which is hostile to the user. We
    // log and return the run regardless.
    {
        let mut log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(e) = log.record_backfill_run(&run) {
            eprintln!("hopper: failed to persist backfill run: {e}");
        }
    }
    Ok(run)
}

/// `slab_hopper_execute_backfill_async` — streaming variant. The
/// long-running [`super::backfill::execute_backfill_streaming`] loop
/// runs on a `spawn_blocking` worker so it doesn't block the tokio
/// reactor. Per-file progress is broadcast on
/// `hopper://backfill-progress` via the service's [`super::watcher::RunEmitter`].
/// The user's Cancel button calls
/// [`slab_hopper_cancel_backfill`] which flips the matching token in
/// [`super::watcher::HopperService::backfill_cancels`].
///
/// `run_id` is a frontend-generated unique key (typically `Date.now()`)
/// that ties the executor + cancel + event subscription together. The
/// command awaits the worker so the resolved value still carries the
/// final [`super::backfill::BackfillRun`] — UI code that prefers the
/// imperative shape can use that and ignore the event stream.
#[tauri::command]
pub async fn slab_hopper_execute_backfill_async(
    svc: tauri::State<'_, HopperService>,
    report: super::backfill::BackfillReport,
    run_id: i64,
) -> CmdResult<super::backfill::BackfillRun> {
    // Register the cancel-token BEFORE the worker spawns — guarantees
    // that a Cancel arriving before the worker's first poll is honoured.
    let cancel = svc.register_backfill_cancel(run_id);

    // Clone Arc handles into the worker. The `svc` State borrow can't
    // cross the spawn_blocking boundary, so we pluck what we need.
    let log = svc.log.clone();
    let emitter = svc.emitter.clone();
    let cancels = svc.backfill_cancels.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let emitter_ref = emitter.as_ref();
        let run = super::backfill::execute_backfill_streaming(&report, &cancel, |progress| {
            emitter_ref.emit_backfill_progress(run_id, progress);
        });

        // Persist the run (best-effort, same policy as the sync path —
        // never let a DB hiccup discard the user's completed work).
        {
            let mut log_guard = log.lock().unwrap_or_else(|p| p.into_inner());
            if let Err(e) = log_guard.record_backfill_run(&run) {
                eprintln!("hopper: failed to persist backfill run: {e}");
            }
        }

        // Drop the cancel registration inline so the map stays bounded
        // even if the awaiting caller is dropped before the JoinHandle
        // resolves.
        {
            let mut map = cancels.lock().unwrap_or_else(|p| p.into_inner());
            map.remove(&run_id);
        }

        run
    })
    .await
    .map_err(|e| format!("backfill task join: {e}"))?;

    Ok(result)
}

/// `slab_hopper_cancel_backfill` — flip the cancel token for an
/// in-flight streaming backfill. Returns `true` if the run was still
/// in flight (token was found + flipped), `false` if the run had
/// already completed (no token registered). The frontend treats both
/// outcomes as success — \"the user got what they wanted\".
#[tauri::command]
pub fn slab_hopper_cancel_backfill(
    svc: tauri::State<'_, HopperService>,
    run_id: i64,
) -> CmdResult<bool> {
    Ok(svc.cancel_backfill(run_id))
}

/// `slab_hopper_list_backfill_runs` — tail of historical backfills,
/// newest first. Pass `folder = Some(p)` to filter to a single watched
/// directory (Rules Editor's "Recent backfills" strip), `None` for the
/// global Hopper panel's history.
///
/// `since_unix` (v3.39 round-10) filters to runs that *finished* at or
/// after the given unix-seconds timestamp. `None` disables the
/// temporal filter (matches all previous behaviour). Combines AND with
/// the folder filter. Powers the panel's "Last 24h / Last 7d / All"
/// history chips — filtering happens in SQL so the wire stays slim.
#[tauri::command]
pub fn slab_hopper_list_backfill_runs(
    svc: tauri::State<'_, HopperService>,
    folder: Option<String>,
    since_unix: Option<i64>,
    limit: Option<i64>,
) -> CmdResult<Vec<super::backfill::BackfillRun>> {
    let log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
    log.list_backfill_runs_since(folder.as_deref(), since_unix, limit.unwrap_or(20))
        .map_err(|e| format!("log list_backfill_runs_since: {e}"))
}

/// `slab_hopper_export_backfill_csv` — write a [`super::backfill::BackfillReport`]
/// to disk as RFC-4180 CSV. The frontend gathers the destination from
/// a native save-as dialog and passes the absolute path here so the
/// Tauri layer owns the disk I/O (the frontend's @tauri-apps/plugin-fs
/// scope doesn't cover arbitrary user-chosen paths).
///
/// Returns the byte count actually written so the UI toast can show
/// "Exported 42 rows (3.1 KB)" without re-reading the file.
///
/// Idempotent — overwrites if the target file exists. The frontend's
/// save dialog handles overwrite confirmation, so we don't double-
/// confirm here.
#[tauri::command]
pub fn slab_hopper_export_backfill_csv(
    report: super::backfill::BackfillReport,
    path: String,
) -> CmdResult<u64> {
    let csv = super::backfill::backfill_report_to_csv(&report, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Slice 89 — drilldown CSV export command surface ─────────────────
//
// `slab_hopper_export_drilldown_csv` writes a SampleDrilldown to disk
// as RFC-4180 CSV. The frontend gathers the destination from a
// native save-as dialog and passes the absolute path here so the
// Tauri layer owns the disk I/O - same shape as
// slab_hopper_export_backfill_csv and slab_marketplace_install_log_
// export_csv. Returns the byte count actually written so the toast
// can read "Exported 23 files (1.4 KB)" without re-reading the file.
//
// Idempotent - overwrites if the target exists. The save dialog
// handles overwrite confirmation, so we don't double-confirm.
//
// Why a separate command vs serialising the drilldown client-side
// and shipping it to a generic `write_text_file`:
//
//   1. The frontend's @tauri-apps/plugin-fs scope doesn't cover
//      arbitrary user-chosen paths; the Tauri layer has to own the
//      write. Same constraint that drove the existing two CSV
//      export commands.
//   2. Keeping the CSV rendering in Rust means a future shape
//      change to RuleSample (e.g. adding a parent_dir column) is a
//      one-line edit to sample_drilldown_to_csv that automatically
//      cascades to the export - the frontend doesn't have to remember
//      to update a parallel TS serialiser.

/// `slab_hopper_export_drilldown_csv` - write a
/// [`super::coverage::SampleDrilldown`] to disk as RFC-4180 CSV.
///
/// `rule_names` is the parallel rule-name array used to resolve a
/// rule bucket's display label. Empty / missing / out-of-range
/// names fall back to `"Rule #N"` (1-based) - mirrors the popover
/// header convention.
///
/// Returns the byte count actually written.
#[tauri::command]
pub fn slab_hopper_export_drilldown_csv(
    drilldown: super::coverage::SampleDrilldown,
    rule_names: Vec<String>,
    path: String,
) -> CmdResult<u64> {
    let csv = super::coverage::sample_drilldown_to_csv(&drilldown, &rule_names, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Slice 94 — drilldown JSON export command surface ────────────────
//
// `slab_hopper_export_drilldown_json` writes a SampleDrilldown to disk
// as a pretty-printed JSON envelope (slice 93 shape). The frontend
// gathers the destination from a native save-as dialog and passes the
// absolute path here so the Tauri layer owns the disk I/O - same
// shape as slab_hopper_export_drilldown_csv (slice 89) and
// slab_marketplace_install_log_export_json (slice 61).
//
// Pretty-printed (NOT compact) for the same reason the install-log
// JSON export is pretty-printed: a paralegal opening the file in a
// text editor needs to be able to read it; compactness saves bytes
// that don't matter for a per-bucket drilldown.
//
// Returns the byte count actually written so the toast can read
// "Exported 23 files (2.7 KB)" without re-reading the file.
//
// Idempotent - overwrites if the target exists. The save dialog
// handles overwrite confirmation, so we don't double-confirm.

/// `slab_hopper_export_drilldown_json` - write a
/// [`super::coverage::SampleDrilldown`] to disk as a pretty-printed
/// JSON envelope (slice 93 [`super::coverage::DrilldownExportEnvelope`]
/// shape).
///
/// `rule_names` is the parallel rule-name array used to resolve a
/// rule bucket's display label - same fallback chain as the CSV
/// export and the popover header (`Rule #N` 1-based when
/// missing/blank/out-of-range).
///
/// Returns the byte count actually written.
#[tauri::command]
pub fn slab_hopper_export_drilldown_json(
    drilldown: super::coverage::SampleDrilldown,
    rule_names: Vec<String>,
    path: String,
) -> CmdResult<u64> {
    let envelope = super::coverage::sample_drilldown_to_json(&drilldown, &rule_names);
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Slice 125 — rule coverage CSV+JSON export command surface ──────
//
// Two commands wrapping slice 123's CSV serialiser and slice 124's
// JSON envelope. Same call shape as the drilldown export commands
// (slices 89 + 94): the frontend gathers the absolute destination
// from a native save-as dialog and ships the path here so the Tauri
// layer owns disk I/O (the @tauri-apps/plugin-fs scope doesn't
// cover arbitrary user-chosen paths). Idempotent — overwrites if
// the target exists; the save dialog handles overwrite confirmation
// so we don't double-confirm.
//
// The two commands accept a RuleCoverageReport DIRECTLY rather than
// re-running `slab_hopper_rule_coverage` server-side. The coverage
// panel already has the report loaded in state at click time;
// re-running risks shipping a slightly different report than what
// the user sees (the in-flight rule-edit + 600ms-debounced coverage
// recompute creates a brief window where the panel and the server's
// re-derivation can diverge). Trusting the client-supplied report
// means "export what's visible" matches the user's mental model.

/// `slab_hopper_export_coverage_csv` — write a
/// [`super::coverage::RuleCoverageReport`] to disk as RFC-4180 CSV
/// (slice 123 [`super::coverage::rule_coverage_to_csv`] shape, with
/// header included). Returns the byte count actually written.
#[tauri::command]
pub fn slab_hopper_export_coverage_csv(
    report: super::coverage::RuleCoverageReport,
    path: String,
) -> CmdResult<u64> {
    let csv = super::coverage::rule_coverage_to_csv(&report, true);
    let bytes = csv.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write csv: {e}"))?;
    Ok(bytes.len() as u64)
}

/// `slab_hopper_export_coverage_json` — write a
/// [`super::coverage::RuleCoverageReport`] to disk as a pretty-
/// printed JSON envelope (slice 124
/// [`super::coverage::RuleCoverageExportEnvelope`] shape with the
/// envelope-level diagnostic counts pre-computed). Returns the byte
/// count actually written.
#[tauri::command]
pub fn slab_hopper_export_coverage_json(
    report: super::coverage::RuleCoverageReport,
    path: String,
) -> CmdResult<u64> {
    let envelope = super::coverage::rule_coverage_to_json(&report);
    let json =
        serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialise json: {e}"))?;
    let bytes = json.as_bytes();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir for export: {e}"))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write json: {e}"))?;
    Ok(bytes.len() as u64)
}

// ─── Slice 130 — server-side coverage diagnostic filter command ──────
//
// The TS client computes the filter locally via
// `filterCoverageByDiagnostic` (slice 129) for the in-panel rendering
// path — chip clicks need to react instantly without round-trips. But
// the EXPORT path benefits from a server-side filter for two reasons:
//
// 1. The wire shape stays self-consistent. A filtered export's CSV /
//    JSON envelope is produced by the SAME `rule_coverage_to_csv` /
//    `rule_coverage_to_json` primitives the unfiltered path uses,
//    fed a filtered report — exactly one code path renders the
//    envelope shape, no parallel "filter at the renderer" branch to
//    drift out of sync.
//
// 2. A future scripted-export consumer (e.g. a CLI driver, a
//    cron-scheduled audit dump) gets the filter as a first-class
//    command rather than having to ship a TS pre-filter step.
//
// The command accepts the source `report` directly (matches the
// `slab_hopper_export_coverage_*` shape — "export what's visible"
// semantics) and a `filter` discriminator, and returns the NEW
// filtered report. The caller then pipes that into the existing
// export commands or renders it client-side.
//
// We deliberately do NOT bundle "filter + export" into one command;
// the filter returns the same `RuleCoverageReport` shape so a TS
// caller can reuse it for any rendering / export path without
// expanding the command surface.

/// `slab_hopper_filter_coverage` — narrow a coverage report to one
/// diagnostic kind on the server. Returns a NEW report with `rules`
/// filtered per [`super::coverage::filter_coverage_by_diagnostic`]
/// (slice 128) and `fallthrough` + `total_samples` preserved
/// verbatim. The filter discriminator is the
/// [`super::coverage::CoverageFilter`] enum
/// (`"all"` / `"dead"` / `"zero"` / `"shadowed"` / `"healthy"`).
///
/// Pure-data command — no DB, no I/O. Mirrors the TS-side
/// `filterCoverageByDiagnostic` (slice 129) 1:1 for the export-path
/// callers; the in-panel render path uses the TS mirror directly.
#[tauri::command]
pub fn slab_hopper_filter_coverage(
    report: super::coverage::RuleCoverageReport,
    filter: super::coverage::CoverageFilter,
) -> CmdResult<super::coverage::RuleCoverageReport> {
    Ok(super::coverage::filter_coverage_by_diagnostic(
        &report, filter,
    ))
}

// ---------------------------------------------------------------------
// v3.40 Slice 135 — dead-rule reorder planner Tauri command
// ---------------------------------------------------------------------
//
// Wraps the pure-data [`super::coverage::plan_dead_rule_reorder`]
// primitive (slice 133) 1:1. Reasons for a server-side command at
// all (the TS mirror in slice 134 already handles the in-panel
// fix-it chip rendering):
//
// 1. A future scripted-audit consumer (CLI / cron health-check / a
//    "what would my chain look like fixed?" subcommand) gets the
//    planner as a first-class command rather than having to mirror
//    the heuristic in TS itself.
//
// 2. Server-side guarantees the planner output is computed against
//    the SAME RulePredicate variant set the runtime evaluator uses.
//    A future predicate kind added on Rust but not yet mirrored in
//    the TS `RulePredicate.kind` union would silently fall through
//    to the no-Always fallback in the TS planner; the server-side
//    command would catch the same case authoritatively.
//
// The wire shape is RuleCoverageReport + Rule[] in (matching the
// other coverage commands' pattern of accepting the in-state
// snapshot rather than re-running the underlying analyzer — same
// race-free posture as `slab_hopper_export_coverage_csv`); the
// return is Vec<ReorderProposal>. Pure-data: no DB, no I/O.

/// `slab_hopper_plan_dead_rule_reorder` — produce one reorder
/// suggestion per dead rule in the given coverage report. See
/// [`super::coverage::plan_dead_rule_reorder`] for the heuristic.
///
/// Returns `Vec<ReorderProposal>` in input order (the order of
/// `report.rules`). Empty when the chain has no dead rules.
#[tauri::command]
pub fn slab_hopper_plan_dead_rule_reorder(
    rules: Vec<Rule>,
    report: super::coverage::RuleCoverageReport,
) -> CmdResult<Vec<super::coverage::ReorderProposal>> {
    Ok(super::coverage::plan_dead_rule_reorder(&rules, &report))
}

// ---------------------------------------------------------------------
// Ollama TitleProvider bridge
// ---------------------------------------------------------------------

/// Bridge between the async [`AiProvider`] trait and the sync
/// [`TitleProvider`] expected by the Hopper pipeline. Holds a tokio
/// runtime handle so it can `block_on` the async chat call from
/// inside a `spawn_blocking` worker.
pub struct OllamaTitleProvider {
    provider: Arc<dyn AiProvider>,
    handle: tokio::runtime::Handle,
    model: Option<String>,
}

impl OllamaTitleProvider {
    /// Construct from any [`AiProvider`] (production = Ollama, but the
    /// trait lets us swap for OpenAI-compatible servers too).
    pub fn new(provider: Arc<dyn AiProvider>, handle: tokio::runtime::Handle) -> Self {
        Self {
            provider,
            handle,
            model: None,
        }
    }

    /// Pin a specific model tag (e.g. `llama3.2:3b`). When unset, the
    /// underlying provider's default model is used.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    fn build_messages(snippet: &str) -> Vec<ChatMessage> {
        vec![
            ChatMessage {
                role: ChatRole::System,
                content: TITLE_SYSTEM_PROMPT.into(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: format!(
                    "Document snippet:\n\n{}\n\nReturn ONLY the title, nothing else.",
                    snippet.chars().take(2000).collect::<String>()
                ),
            },
        ]
    }
}

const TITLE_SYSTEM_PROMPT: &str = "\
You are a filename generator for a PDF automation tool. Given a snippet \
from a document, return a 4-6 word title suitable for a filename. Rules: \
Title Case. No quotes, no punctuation except spaces and digits. No file \
extension. Examples: 'Acme NDA 2025', 'Q3 Budget Review', 'Smith Deposition \
Transcript'. Output the title and absolutely nothing else.";

impl TitleProvider for OllamaTitleProvider {
    fn suggest_title(&self, snippet: &str) -> Option<String> {
        let msgs = Self::build_messages(snippet);
        let opts = ChatOpts {
            model: self.model.clone(),
            temperature: Some(0.2),
            max_tokens: Some(32),
        };
        let provider = self.provider.clone();
        let res = self
            .handle
            .block_on(async move { provider.chat(&msgs, &opts).await });
        match res {
            Ok(resp) => {
                let cleaned = sanitize_title(&resp.content);
                if cleaned.is_empty() {
                    None
                } else {
                    Some(cleaned)
                }
            }
            Err(_) => None,
        }
    }
}

/// Strip quotes, extensions, leading/trailing junk, cap at 80 chars.
/// Pure-fn so we can unit-test it without an Ollama server running.
pub fn sanitize_title(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    // Strip surrounding quotes (some models love wrapping output).
    for quote in ['"', '\'', '`', '"', '"'] {
        if s.starts_with(quote) && s.ends_with(quote) && s.len() > 1 {
            s = s[quote.len_utf8()..s.len() - quote.len_utf8()].to_string();
        }
    }
    // Drop trailing file extensions the model might tack on.
    for ext in [".pdf", ".PDF", ".docx", ".txt"] {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
        }
    }
    // Take just the first line — models sometimes add a "Reasoning:" line.
    if let Some(first_line) = s.lines().next() {
        s = first_line.to_string();
    }
    let s = s.trim().trim_matches('.').trim();
    // Replace characters that would break filename templating (the
    // slugifier in rename.rs also runs, but doing it here yields a
    // cleaner ai_title for the log + UI).
    let s: String = s
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '|' | '?' | '*' | '<' | '>'))
        .collect();
    s.chars().take(80).collect::<String>().trim().to_string()
}

// ---------------------------------------------------------------------
// Bootstrap helper (called from lib.rs setup())
// ---------------------------------------------------------------------

/// Construct + start the singleton [`HopperService`], reading recipes
/// from `$APP_CONFIG/atelier/recipes` and using an Ollama-backed
/// title provider. Returns the service so the caller can
/// `app.manage(svc)`. Best-effort: any sub-failure (sqlite open,
/// recipes dir missing) yields a service that's still usable but may
/// be empty / not started.
pub fn build_default_service(app: &tauri::AppHandle) -> Result<HopperService, String> {
    use super::log::HopperLog;
    use super::registry::{default_db_path, HopperRegistry};
    use super::watcher::{null_recipe_loader, RunEmitter};

    let db_path = default_db_path();
    let reg = HopperRegistry::open(&db_path).map_err(|e| format!("open hopper registry: {e}"))?;

    let log_path = db_path.with_file_name("hopper-log.db");
    let log = HopperLog::open(&log_path).map_err(|e| format!("open hopper log: {e}"))?;

    // Recipe loader — best-effort read from the atelier recipes dir.
    let recipes_dir = app
        .path()
        .app_config_dir()
        .ok()
        .map(|p| p.join("atelier").join("recipes"));
    let recipe_loader: super::watcher::RecipeLoader = match recipes_dir {
        Some(dir) => Arc::new(move |rid: &str| {
            let recipes = crate::pdf::atelier::cmds::list_recipes_in_dir(&dir).unwrap_or_default();
            recipes.into_iter().find(|r| r.name == rid)
        }),
        None => null_recipe_loader(),
    };

    // Title provider — Ollama by default; falls back gracefully if
    // the daemon isn't running.
    let provider: Arc<dyn TitleProvider> = match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            let ai: Arc<dyn AiProvider> = Arc::new(crate::ai::ollama::OllamaProvider::new());
            Arc::new(OllamaTitleProvider::new(ai, handle))
        }
        Err(_) => Arc::new(super::pipeline::NullProvider),
    };

    let emitter: Arc<dyn RunEmitter> = Arc::new(app.clone());

    let svc = HopperService::new(reg, log, provider, recipe_loader, emitter);
    if let Err(e) = svc.start() {
        eprintln!("hopper: start failed (will retry on first command): {e}");
    }
    Ok(svc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_title_strips_quotes() {
        assert_eq!(sanitize_title("\"Acme NDA\""), "Acme NDA");
        assert_eq!(sanitize_title("'Q3 Budget'"), "Q3 Budget");
        assert_eq!(sanitize_title("`Foo Bar`"), "Foo Bar");
    }

    #[test]
    fn sanitize_title_strips_extensions() {
        assert_eq!(sanitize_title("Acme NDA.pdf"), "Acme NDA");
        assert_eq!(sanitize_title("Report.PDF"), "Report");
        assert_eq!(sanitize_title("Notes.docx"), "Notes");
    }

    #[test]
    fn sanitize_title_takes_first_line() {
        assert_eq!(sanitize_title("Acme NDA\nReasoning: contract"), "Acme NDA");
    }

    #[test]
    fn sanitize_title_strips_path_chars() {
        assert_eq!(sanitize_title("Foo/Bar:Baz"), "FooBarBaz");
        assert_eq!(sanitize_title("X|Y?Z*"), "XYZ");
    }

    #[test]
    fn sanitize_title_caps_at_80_chars() {
        let long = "A".repeat(200);
        assert_eq!(sanitize_title(&long).len(), 80);
    }

    #[test]
    fn sanitize_title_handles_empty() {
        assert_eq!(sanitize_title(""), "");
        assert_eq!(sanitize_title("   "), "");
        assert_eq!(sanitize_title("...   ..."), "");
    }

    #[test]
    fn build_messages_includes_system_prompt() {
        let msgs = OllamaTitleProvider::build_messages("hello world");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, ChatRole::System);
        assert!(msgs[0].content.contains("filename"));
        assert_eq!(msgs[1].role, ChatRole::User);
        assert!(msgs[1].content.contains("hello world"));
    }

    #[test]
    fn build_messages_truncates_long_snippet() {
        let huge = "X".repeat(10_000);
        let msgs = OllamaTitleProvider::build_messages(&huge);
        // 2000 X's + the surrounding prompt scaffold.
        assert!(msgs[1].content.len() < 2200);
        assert!(msgs[1].content.contains(&"X".repeat(2000)));
    }

    // ── v3.40 Slice 80 — coverage command helper tests ────────────────

    fn run_record(watch_id: i64, input_path: &str) -> super::super::log::RunRecord {
        super::super::log::RunRecord {
            id: 0,
            watch_id,
            input_path: input_path.into(),
            output_path: None,
            status: super::super::log::RunStatus::Success,
            error: None,
            duration_ms: 0,
            ai_title: None,
            started_at: "0".into(),
        }
    }

    #[test]
    fn clamp_sample_limit_defaults_to_one_hundred() {
        assert_eq!(clamp_sample_limit(None), 100);
    }

    #[test]
    fn clamp_sample_limit_clamps_below_one_to_one() {
        assert_eq!(clamp_sample_limit(Some(0)), 1);
        assert_eq!(clamp_sample_limit(Some(-7)), 1);
    }

    #[test]
    fn clamp_sample_limit_clamps_above_ceiling_to_one_thousand() {
        assert_eq!(clamp_sample_limit(Some(10_000)), 1000);
        assert_eq!(clamp_sample_limit(Some(i64::MAX)), 1000);
    }

    #[test]
    fn clamp_sample_limit_passes_in_range_through() {
        assert_eq!(clamp_sample_limit(Some(1)), 1);
        assert_eq!(clamp_sample_limit(Some(50)), 50);
        assert_eq!(clamp_sample_limit(Some(1000)), 1000);
    }

    #[test]
    fn sample_over_read_is_four_times_cap() {
        assert_eq!(sample_over_read(50), 200);
        assert_eq!(sample_over_read(100), 400);
    }

    #[test]
    fn sample_over_read_clamped_to_ceiling() {
        // 1000 * 4 = 4000, well under the 10_000 cap, so the result
        // tracks 4x.
        assert_eq!(sample_over_read(1000), 4000);
        // Even an absurd cap can't push the over-read past 10_000.
        assert_eq!(sample_over_read(100_000), 10_000);
        assert_eq!(sample_over_read(i64::MAX), 10_000);
    }

    #[test]
    fn samples_from_runs_filters_to_watch_id() {
        let runs = vec![
            run_record(1, "/tmp/a.pdf"),
            run_record(2, "/tmp/b.pdf"),
            run_record(1, "/tmp/c.pdf"),
            run_record(3, "/tmp/d.pdf"),
        ];
        let samples = samples_from_runs(&runs, 1, 100);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].filename, "a.pdf");
        assert_eq!(samples[1].filename, "c.pdf");
    }

    #[test]
    fn samples_from_runs_reduces_to_basename() {
        let runs = vec![
            run_record(1, "/Users/sanjay/Documents/tax_2026.pdf"),
            run_record(1, "/var/folders/x/invoice.pdf"),
            run_record(1, "bare-filename.pdf"),
        ];
        let samples = samples_from_runs(&runs, 1, 100);
        assert_eq!(samples[0].filename, "tax_2026.pdf");
        assert_eq!(samples[1].filename, "invoice.pdf");
        assert_eq!(samples[2].filename, "bare-filename.pdf");
    }

    #[test]
    fn samples_from_runs_respects_cap() {
        let runs: Vec<_> = (0..50)
            .map(|i| run_record(1, &format!("/tmp/f{i}.pdf")))
            .collect();
        let samples = samples_from_runs(&runs, 1, 10);
        assert_eq!(samples.len(), 10);
        assert_eq!(samples[0].filename, "f0.pdf");
        assert_eq!(samples[9].filename, "f9.pdf");
    }

    #[test]
    fn samples_from_runs_empty_input_returns_empty() {
        let samples = samples_from_runs(&[], 1, 100);
        assert!(samples.is_empty());
    }

    #[test]
    fn samples_from_runs_no_matches_returns_empty() {
        let runs = vec![run_record(2, "/tmp/a.pdf"), run_record(3, "/tmp/b.pdf")];
        let samples = samples_from_runs(&runs, 99, 100);
        assert!(samples.is_empty());
    }

    #[test]
    fn samples_from_runs_zeroes_size_page_text() {
        let runs = vec![run_record(1, "/tmp/x.pdf")];
        let samples = samples_from_runs(&runs, 1, 100);
        assert_eq!(samples[0].size_bytes, 0);
        assert!(samples[0].page_count.is_none());
        assert!(samples[0].text_sample.is_none());
    }

    #[test]
    fn samples_from_runs_handles_invalid_utf8_basename() {
        // If file_name returns something that doesn't decode as UTF-8
        // (essentially impossible on the runtime input_path String, but
        // belt-and-suspenders), fall back to the full path.
        let runs = vec![run_record(1, "/")];
        let samples = samples_from_runs(&runs, 1, 100);
        // Path::new("/").file_name() returns None -> falls back to "/".
        assert_eq!(samples[0].filename, "/");
    }

    // ── Slice 84 — sample drilldown preview cap helper tests ──────────

    #[test]
    fn clamp_preview_cap_defaults_to_twenty_five() {
        assert_eq!(clamp_preview_cap(None), 25);
    }

    #[test]
    fn clamp_preview_cap_clamps_below_one_to_one() {
        assert_eq!(clamp_preview_cap(Some(0)), 1);
        assert_eq!(clamp_preview_cap(Some(-1)), 1);
        assert_eq!(clamp_preview_cap(Some(i64::MIN)), 1);
    }

    #[test]
    fn clamp_preview_cap_clamps_above_ceiling_to_one_thousand() {
        assert_eq!(clamp_preview_cap(Some(1001)), 1000);
        assert_eq!(clamp_preview_cap(Some(10_000)), 1000);
        assert_eq!(clamp_preview_cap(Some(i64::MAX)), 1000);
    }

    #[test]
    fn clamp_preview_cap_passes_in_range_through() {
        assert_eq!(clamp_preview_cap(Some(1)), 1);
        assert_eq!(clamp_preview_cap(Some(25)), 25);
        assert_eq!(clamp_preview_cap(Some(100)), 100);
        assert_eq!(clamp_preview_cap(Some(1000)), 1000);
    }

    #[test]
    fn clamp_preview_cap_default_is_lower_than_coverage_default() {
        // The drilldown carries the FULL filename + axes per sample
        // (heavier per-row than the coverage report's counts) so its
        // default is a smaller "first page" preview. Pin the relationship
        // so a future tweak that breaks the ordering shows up as a test
        // failure rather than a silent regression.
        assert!(clamp_preview_cap(None) < clamp_sample_limit(None));
    }
}
