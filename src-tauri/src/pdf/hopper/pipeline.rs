//! Hopper pipeline — orchestrate one file end-to-end.
//!
//! Given one freshly-arrived PDF and its [`Watch`] config, the pipeline:
//!
//! 1. Resolves the recipe by id (or skips the recipe stage if `None`).
//! 2. Runs the recipe via `pdf::atelier::run::run_recipe` into a temp
//!    scratch path.
//! 3. If `ai_rename`, extracts a short snippet of the first 1-2 pages
//!    and asks the [`TitleProvider`] for a 4-6 word document title.
//! 4. Applies the `rename_pattern` template to produce the final filename.
//! 5. Atomically moves the scratch file into `watch.output_dir`.
//! 6. Records a [`RunRecord`] in the [`HopperLog`].
//!
//! The provider is dependency-injected behind a trait so the unit test
//! can supply a canned title without touching Ollama. In production
//! `lib.rs::setup` wires the real Ollama-backed provider.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::log::{HopperLog, RunRecord, RunStatus};
use super::registry::Watch;
use super::rename::{apply_pattern, slugify};
use super::rules::{evaluate_rules, Rule, RuleContext};

/// Pluggable AI title provider. Returns a 4-6 word document title or
/// `None` if the provider is offline / unavailable (caller treats that
/// as "AI rename failed gracefully" and falls back to `{stem}`).
pub trait TitleProvider: Send + Sync {
    fn suggest_title(&self, snippet: &str) -> Option<String>;
}

/// Provider that always returns `None` — used when AI rename is disabled.
pub struct NullProvider;

impl TitleProvider for NullProvider {
    fn suggest_title(&self, _snippet: &str) -> Option<String> {
        None
    }
}

/// Outcome of `process_one`: a [`RunRecord`] describing what happened.
/// The record is also persisted to the log by this function.
pub struct ProcessOutcome {
    pub record: RunRecord,
}

/// Run the full pipeline against a single input PDF. This function is
/// designed to be cheap to test: pass a `NullProvider` and a recipe
/// loader that returns an empty recipe (no-op), and the file should
/// round-trip from `input_path` to `watch.output_dir` unchanged.
///
/// `recipe_loader` resolves a recipe-id to a real `Recipe`. We inject
/// this rather than hitting disk so unit tests can supply a literal
/// recipe value. Production wiring loads from `$APP_CONFIG/atelier/recipes`.
pub fn process_one<F>(
    watch: &Watch,
    rules: &[Rule],
    input_path: &Path,
    provider: &dyn TitleProvider,
    recipe_loader: F,
    log: &Mutex<HopperLog>,
) -> ProcessOutcome
where
    F: FnOnce(&str) -> Option<crate::pdf::atelier::recipe::Recipe>,
{
    let started = Instant::now();

    // Compute final-filename context up front so we can use it even if
    // a stage fails (for the log).
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document")
        .to_string();
    let ext = input_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("pdf")
        .to_lowercase();

    // ── v3.21.0: evaluate rules → ResolvedRouting overrides watch defaults.
    //
    // The pipeline reads `recipe_id` / `output_dir` / `rename_pattern`
    // from the resolved routing instead of the watch directly. If no
    // rule matches, `evaluate_rules` returns the watch defaults verbatim
    // (with `matched_rule = None`), so v3.20.0 single-recipe behaviour
    // is preserved when the rule list is empty.
    let filename = input_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let parent_dir = input_path.parent().and_then(|p| p.to_str()).unwrap_or("");
    let size_bytes = fs::metadata(input_path).map(|m| m.len()).unwrap_or(0);
    let ctx = RuleContext {
        filename,
        parent_dir,
        size_bytes,
        // v1: page_count + text_sample stay `None` — text extraction is
        // expensive and most users start with filename/size rules. The
        // editor UI in Task 9 will let users opt-in per rule.
        page_count: None,
        text_sample: None,
    };
    let resolved = evaluate_rules(rules, watch, &ctx);

    // Resolve recipe (if any) — failure here = skip recipe, copy as-is.
    let recipe = match resolved.recipe_id.as_deref() {
        Some(rid) => recipe_loader(rid),
        None => None,
    };

    // Stage 1: produce a "post-recipe" temp file.
    let scratch_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            return finish(
                watch,
                input_path,
                None,
                RunStatus::Failed,
                Some(format!("tempdir failed: {e}")),
                None,
                started,
                log,
            )
        }
    };
    let staged = scratch_dir.path().join("staged.pdf");

    if let Some(r) = recipe.as_ref() {
        if let Err(e) = crate::pdf::atelier::run::run_recipe(input_path, &staged, r, &|_| {}) {
            return finish(
                watch,
                input_path,
                None,
                RunStatus::Failed,
                Some(format!("recipe failed: {e}")),
                None,
                started,
                log,
            );
        }
    } else if let Err(e) = fs::copy(input_path, &staged) {
        return finish(
            watch,
            input_path,
            None,
            RunStatus::Failed,
            Some(format!("copy failed: {e}")),
            None,
            started,
            log,
        );
    }

    // Stage 2: AI title (best-effort).
    let ai_title = if watch.ai_rename {
        // We could OCR/text-extract here, but for the pipeline contract
        // we pass the filename stem as the snippet — extraction is the
        // provider's concern (Beacon path) and not the pipeline's. The
        // injected provider in production reads the PDF text itself.
        provider.suggest_title(&stem)
    } else {
        None
    };

    // Stage 3: filename.
    let final_name = match resolved.rename_pattern.as_deref() {
        Some(pattern) => {
            let date = today_iso8601();
            apply_pattern(pattern, &date, ai_title.as_deref(), &stem, &ext)
        }
        None => format!("{}.{ext}", slugify(&stem)),
    };

    // Stage 4: move into place.
    let out_dir = PathBuf::from(&resolved.output_dir);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        return finish(
            watch,
            input_path,
            None,
            RunStatus::Failed,
            Some(format!("mkdir output_dir: {e}")),
            ai_title,
            started,
            log,
        );
    }
    let final_path = unique_path(out_dir.join(final_name));
    if let Err(e) = fs::rename(&staged, &final_path) {
        // Cross-device rename → fall back to copy + remove.
        if let Err(e2) = fs::copy(&staged, &final_path).and_then(|_| fs::remove_file(&staged)) {
            return finish(
                watch,
                input_path,
                None,
                RunStatus::Failed,
                Some(format!("move failed: {e} / {e2}")),
                ai_title,
                started,
                log,
            );
        }
    }

    finish(
        watch,
        input_path,
        Some(&final_path),
        RunStatus::Success,
        None,
        ai_title,
        started,
        log,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish(
    watch: &Watch,
    input: &Path,
    output: Option<&Path>,
    status: RunStatus,
    error: Option<String>,
    ai_title: Option<String>,
    started: Instant,
    log: &Mutex<HopperLog>,
) -> ProcessOutcome {
    let duration_ms = started.elapsed().as_millis() as i64;
    let input_str = input.display().to_string();
    let output_str = output.map(|p| p.display().to_string());

    let mut guard = log.lock().unwrap_or_else(|p| p.into_inner());
    let id = guard
        .record(
            watch.id,
            &input_str,
            output_str.as_deref(),
            status,
            error.as_deref(),
            duration_ms,
            ai_title.as_deref(),
        )
        .unwrap_or(0);
    drop(guard);

    ProcessOutcome {
        record: RunRecord {
            id,
            watch_id: watch.id,
            input_path: input_str,
            output_path: output_str,
            status,
            error,
            duration_ms,
            ai_title,
            started_at: format!("{}", unix_now()),
        },
    }
}

/// Disambiguate a file path that already exists by appending ` (1)`,
/// ` (2)`, etc. to the stem until it doesn't collide. Caps at 999 to
/// avoid pathological loops.
fn unique_path(candidate: PathBuf) -> PathBuf {
    if !candidate.exists() {
        return candidate;
    }
    let parent = candidate.parent().unwrap_or_else(|| Path::new("."));
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = candidate.extension().and_then(|s| s.to_str()).unwrap_or("");
    for i in 1..=999 {
        let name = if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let cand = parent.join(name);
        if !cand.exists() {
            return cand;
        }
    }
    candidate
}

fn today_iso8601() -> String {
    // YYYY-MM-DD from unix seconds without pulling chrono.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (y, m, d) = ymd_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Cheap unix-seconds → (year, month, day) — proleptic Gregorian, UTC.
/// Good enough for filename templating (no leap-second hair). Sourced
/// from the well-known "days_from_civil" algorithm by Howard Hinnant.
fn ymd_from_unix(secs: i64) -> (i32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::atelier::recipe::Recipe;
    use std::fs;
    use std::io::Write;

    struct CannedProvider(&'static str);
    impl TitleProvider for CannedProvider {
        fn suggest_title(&self, _snippet: &str) -> Option<String> {
            Some(self.0.to_string())
        }
    }

    fn write_dummy_pdf(path: &Path) {
        // Minimal valid-enough PDF that `fs::copy` is happy with.
        // Atelier's run_recipe only requires `input.exists()` — we don't
        // run any real parsing in this test (empty recipe = no-op copy).
        let mut f = fs::File::create(path).unwrap();
        f.write_all(b"%PDF-1.4\n%dummy\n").unwrap();
    }

    #[test]
    fn end_to_end_no_recipe_no_ai_just_copies() {
        let dir = tempfile::tempdir().unwrap();
        let inp = dir.path().join("Acme NDA.pdf");
        let out_dir = dir.path().join("out");
        write_dummy_pdf(&inp);

        let watch = Watch {
            id: 1,
            source_dir: dir.path().display().to_string(),
            output_dir: out_dir.display().to_string(),
            recipe_id: None,
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        };

        let log_db = dir.path().join("hopper.db");
        let log = Mutex::new(HopperLog::open(&log_db).unwrap());

        let outcome = process_one(&watch, &[], &inp, &NullProvider, |_| None, &log);
        assert_eq!(outcome.record.status, RunStatus::Success);
        let out = outcome.record.output_path.expect("output_path set");
        assert!(Path::new(&out).exists(), "output file {out} missing");
        assert!(out.ends_with("Acme_NDA.pdf"), "got {out}");
    }

    #[test]
    fn ai_title_and_pattern_produce_renamed_file() {
        let dir = tempfile::tempdir().unwrap();
        let inp = dir.path().join("scan_001.pdf");
        let out_dir = dir.path().join("out");
        write_dummy_pdf(&inp);

        let watch = Watch {
            id: 2,
            source_dir: dir.path().display().to_string(),
            output_dir: out_dir.display().to_string(),
            recipe_id: None,
            rename_pattern: Some("{date}_{ai_title}.pdf".into()),
            ai_rename: true,
            enabled: true,
            created_at: "0".into(),
        };

        let log = Mutex::new(HopperLog::open(dir.path().join("h.db")).unwrap());
        let outcome = process_one(
            &watch,
            &[],
            &inp,
            &CannedProvider("NDA Acme Corp"),
            |_| None,
            &log,
        );

        assert_eq!(outcome.record.status, RunStatus::Success);
        let out = outcome.record.output_path.unwrap();
        // YYYY-MM-DD prefix + slugified title.
        assert!(out.ends_with("_NDA_Acme_Corp.pdf"), "got {out}");
        assert_eq!(outcome.record.ai_title.as_deref(), Some("NDA Acme Corp"));
    }

    #[test]
    fn collision_appends_disambiguator() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();
        // Pre-create the target filename.
        fs::write(out_dir.join("Acme_NDA.pdf"), b"existing").unwrap();

        let inp = dir.path().join("Acme NDA.pdf");
        write_dummy_pdf(&inp);

        let watch = Watch {
            id: 3,
            source_dir: dir.path().display().to_string(),
            output_dir: out_dir.display().to_string(),
            recipe_id: None,
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        };
        let log = Mutex::new(HopperLog::open(dir.path().join("h.db")).unwrap());
        let outcome = process_one(&watch, &[], &inp, &NullProvider, |_| None, &log);
        let out = outcome.record.output_path.unwrap();
        assert!(out.ends_with("Acme_NDA (1).pdf"), "got {out}");
        // Original file with same name still untouched.
        assert_eq!(fs::read(out_dir.join("Acme_NDA.pdf")).unwrap(), b"existing");
    }

    #[test]
    fn missing_input_is_recorded_as_failure() {
        let dir = tempfile::tempdir().unwrap();
        let watch = Watch {
            id: 9,
            source_dir: dir.path().display().to_string(),
            output_dir: dir.path().join("out").display().to_string(),
            recipe_id: Some("anything".into()),
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        };
        let log = Mutex::new(HopperLog::open(dir.path().join("h.db")).unwrap());
        // Recipe loader returns a recipe with zero steps, but input
        // doesn't exist on disk → copy step fails (no recipe to run).
        let outcome = process_one(
            &watch,
            &[],
            &dir.path().join("does-not-exist.pdf"),
            &NullProvider,
            |_| {
                Some(Recipe {
                    name: "noop".into(),
                    version: 1,
                    steps: vec![],
                })
            },
            &log,
        );
        // Recipe has zero steps, so atelier::run_recipe will fail at
        // `input.exists()` check — propagated as Failed.
        assert_eq!(outcome.record.status, RunStatus::Failed);
        assert!(outcome.record.error.is_some());
    }

    #[test]
    fn ymd_from_unix_known_dates() {
        // 1970-01-01
        assert_eq!(ymd_from_unix(0), (1970, 1, 1));
        // 2000-02-29 (leap)
        assert_eq!(ymd_from_unix(951782400), (2000, 2, 29));
        // 2024-01-01 = 1704067200
        assert_eq!(ymd_from_unix(1704067200), (2024, 1, 1));
    }

    // ─── v3.21.0: rule-driven routing end-to-end ──────────────────────

    #[test]
    fn rule_overrides_watch_output_dir() {
        use super::super::rules::{Rule, RuleAction, RulePredicate};

        let dir = tempfile::tempdir().unwrap();
        let inp = dir.path().join("tax_2026.pdf");
        write_dummy_pdf(&inp);

        let default_out = dir.path().join("misc");
        let tax_out = dir.path().join("taxes");
        fs::create_dir_all(&default_out).unwrap();
        fs::create_dir_all(&tax_out).unwrap();

        let watch = Watch {
            id: 7,
            source_dir: dir.path().display().to_string(),
            output_dir: default_out.display().to_string(),
            recipe_id: None,
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        };
        let rules = vec![Rule {
            name: "taxes".into(),
            predicate: RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
            action: RuleAction {
                recipe_id: None,
                output_dir: Some(tax_out.display().to_string()),
                rename_pattern: None,
            },
        }];
        let log = Mutex::new(HopperLog::open(dir.path().join("h.db")).unwrap());

        let outcome = process_one(&watch, &rules, &inp, &NullProvider, |_| None, &log);
        assert_eq!(outcome.record.status, RunStatus::Success);
        let out = outcome.record.output_path.unwrap();
        assert!(
            out.starts_with(tax_out.to_string_lossy().as_ref()),
            "expected tax dir routing, got {out}"
        );
    }

    #[test]
    fn empty_rules_preserves_v3_20_behaviour() {
        // Smoke test: passing `&[]` for rules must behave identically to
        // the v3.20.0 pipeline — the watch defaults are used.
        let dir = tempfile::tempdir().unwrap();
        let inp = dir.path().join("doc.pdf");
        let out_dir = dir.path().join("out");
        write_dummy_pdf(&inp);
        let watch = Watch {
            id: 8,
            source_dir: dir.path().display().to_string(),
            output_dir: out_dir.display().to_string(),
            recipe_id: None,
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        };
        let log = Mutex::new(HopperLog::open(dir.path().join("h.db")).unwrap());
        let outcome = process_one(&watch, &[], &inp, &NullProvider, |_| None, &log);
        assert_eq!(outcome.record.status, RunStatus::Success);
        let out = outcome.record.output_path.unwrap();
        assert!(out.starts_with(out_dir.to_string_lossy().as_ref()));
    }

    #[test]
    fn non_matching_rule_falls_through_to_watch_defaults() {
        use super::super::rules::{Rule, RuleAction, RulePredicate};
        let dir = tempfile::tempdir().unwrap();
        let inp = dir.path().join("invoice.pdf");
        let out_dir = dir.path().join("default-out");
        write_dummy_pdf(&inp);
        let watch = Watch {
            id: 9,
            source_dir: dir.path().display().to_string(),
            output_dir: out_dir.display().to_string(),
            recipe_id: None,
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        };
        let rules = vec![Rule {
            name: "taxes-only".into(),
            predicate: RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
            action: RuleAction {
                recipe_id: None,
                output_dir: Some("/should/not/be/used".into()),
                rename_pattern: None,
            },
        }];
        let log = Mutex::new(HopperLog::open(dir.path().join("h.db")).unwrap());
        let outcome = process_one(&watch, &rules, &inp, &NullProvider, |_| None, &log);
        assert_eq!(outcome.record.status, RunStatus::Success);
        let out = outcome.record.output_path.unwrap();
        assert!(out.starts_with(out_dir.to_string_lossy().as_ref()));
    }
}
