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
#[tauri::command]
pub fn slab_hopper_list_backfill_runs(
    svc: tauri::State<'_, HopperService>,
    folder: Option<String>,
    limit: Option<i64>,
) -> CmdResult<Vec<super::backfill::BackfillRun>> {
    let log = svc.log.lock().unwrap_or_else(|p| p.into_inner());
    log.list_backfill_runs(folder.as_deref(), limit.unwrap_or(20))
        .map_err(|e| format!("log list_backfill_runs: {e}"))
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
}
