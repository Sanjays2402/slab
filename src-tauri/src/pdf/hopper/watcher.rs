//! Hopper watcher — `notify`-backed filesystem watcher with debounce
//! and parallel pipeline dispatch.
//!
//! ## Architecture
//!
//! One `notify::RecommendedWatcher` instance is shared across every
//! enabled [`Watch`] in the registry. The watcher emits `notify::Event`s
//! into an `std::sync::mpsc` channel; a single coordinator task drains
//! the channel and stamps each affected `*.pdf` path in a `HashMap<
//! PathBuf, Instant>` debounce table. A periodic flush task (200 ms
//! tick) walks the table and, for every entry whose last-stamp is
//! ≥ `DEBOUNCE_MS` old, spawns `tokio::spawn(pipeline::process_one)`.
//!
//! That layering buys two things:
//!
//! 1. **Coalescing** — Word, Acrobat, scanners, browsers all emit
//!    dozens of `Modify`/`Create` events per save. The 700 ms quiet
//!    period collapses them into one pipeline run.
//! 2. **Parallelism** — multiple files dropped at once each get their
//!    own task; a slow recipe on file A doesn't stall file B.
//!
//! ## Restart on registry change
//!
//! [`HopperService::reload_watches`] rebuilds the per-path subscription
//! set. Cheap because `notify::Watcher::watch` is idempotent per path
//! (we unwatch dropped dirs first). Callers fire this after `add`,
//! `remove`, or `set_enabled`.
//!
//! ## Event emission
//!
//! Every completed run is emitted as a `tauri::Emitter` event named
//! `hopper://run-completed` carrying the `RunRecord` JSON, so the
//! frontend live log can `listen()` and tail without polling. When no
//! `AppHandle` is provided (tests, headless), emission is a no-op.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::log::HopperLog;
use super::pipeline::{self, TitleProvider};
use super::registry::{HopperRegistry, Watch};

/// Quiet-period after the last fs event on a path before we treat it
/// as "settled" and run the pipeline. 700 ms comfortably outlasts the
/// rapid-fire event bursts emitted by Acrobat, Word, and macOS scan
/// utilities during a save.
pub const DEBOUNCE_MS: u64 = 700;

/// How often the flush loop wakes up to check the debounce table.
const FLUSH_TICK_MS: u64 = 200;

/// Trait the service uses to emit events to the frontend. Abstracted
/// behind a trait so unit tests can use a no-op emitter without
/// constructing a full `tauri::AppHandle`.
pub trait RunEmitter: Send + Sync + 'static {
    fn emit_run_completed(&self, record: &super::log::RunRecord);
}

/// No-op emitter — used in tests and when the service is constructed
/// without a Tauri context.
pub struct NullEmitter;
impl RunEmitter for NullEmitter {
    fn emit_run_completed(&self, _record: &super::log::RunRecord) {}
}

#[cfg(not(test))]
impl RunEmitter for tauri::AppHandle {
    fn emit_run_completed(&self, record: &super::log::RunRecord) {
        use tauri::Emitter;
        // Best-effort emit — never let a UI delivery failure crash the
        // pipeline. Frontend handles missed events via list-runs polling.
        let _ = self.emit("hopper://run-completed", record);
    }
}

// During unit tests we still need a RunEmitter impl for AppHandle so
// cmds.rs's `Arc::new(app.clone())` compiles. `build_default_service`
// isn't invoked from tests, so this branch is compile-only.
#[cfg(test)]
impl RunEmitter for tauri::AppHandle {
    fn emit_run_completed(&self, _record: &super::log::RunRecord) {
        unreachable!("AppHandle RunEmitter must not be used in tests");
    }
}

/// Pluggable recipe loader. Production wires a closure that reads from
/// `$APP_CONFIG/atelier/recipes`; tests inject a literal recipe map.
pub type RecipeLoader =
    Arc<dyn Fn(&str) -> Option<crate::pdf::atelier::recipe::Recipe> + Send + Sync>;

/// A no-op recipe loader. Returns `None` for every recipe id so the
/// pipeline copies the file through unmodified.
pub fn null_recipe_loader() -> RecipeLoader {
    Arc::new(|_| None)
}

/// Owns the per-watch background tasks plus the shared registry/log.
/// Clone is cheap (Arcs all the way down) so the service can be
/// `app.manage()`d and read from any Tauri command.
#[derive(Clone)]
pub struct HopperService {
    pub registry: Arc<Mutex<HopperRegistry>>,
    pub log: Arc<Mutex<HopperLog>>,
    pub provider: Arc<dyn TitleProvider>,
    pub recipe_loader: RecipeLoader,
    pub emitter: Arc<dyn RunEmitter>,
    inner: Arc<Mutex<ServiceInner>>,
}

struct ServiceInner {
    /// Active `notify` watcher. `None` until `start()` is called.
    watcher: Option<RecommendedWatcher>,
    /// Directories we've currently asked `notify` to watch.
    watched: Vec<PathBuf>,
    /// Debounce table — last fs-event timestamp per affected path.
    pending: Arc<Mutex<HashMap<PathBuf, Pending>>>,
    /// Set true when `start()` has been called and the flush loop is
    /// running. Subsequent calls reload watches in place instead of
    /// spawning a second flush task.
    started: bool,
}

#[derive(Clone)]
struct Pending {
    last_seen: Instant,
    watch: Watch,
}

impl HopperService {
    /// Construct a service handle. Does **not** spawn any tasks yet —
    /// call [`Self::start`] to begin watching.
    pub fn new(
        registry: HopperRegistry,
        log: HopperLog,
        provider: Arc<dyn TitleProvider>,
        recipe_loader: RecipeLoader,
        emitter: Arc<dyn RunEmitter>,
    ) -> Self {
        Self {
            registry: Arc::new(Mutex::new(registry)),
            log: Arc::new(Mutex::new(log)),
            provider,
            recipe_loader,
            emitter,
            inner: Arc::new(Mutex::new(ServiceInner {
                watcher: None,
                watched: Vec::new(),
                pending: Arc::new(Mutex::new(HashMap::new())),
                started: false,
            })),
        }
    }

    /// Boot the watcher + flush loop. Idempotent: subsequent calls
    /// just reload the watch set.
    pub fn start(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if inner.started {
            drop(inner);
            return self.reload_watches();
        }

        // (1) Create the channel + watcher. `notify` uses a sync mpsc
        // because its callbacks may run on a non-tokio thread.
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let watcher = notify::recommended_watcher(move |res| {
            // Drop send errors silently — happens during shutdown.
            let _ = tx.send(res);
        })
        .map_err(|e| format!("create notify watcher: {e}"))?;
        inner.watcher = Some(watcher);
        let pending = inner.pending.clone();

        // (2) Coordinator task — drains the notify channel and stamps
        // affected PDFs into the debounce table. Spawns onto a
        // dedicated blocking thread because `mpsc::Receiver::recv` is
        // sync.
        let registry = self.registry.clone();
        std::thread::Builder::new()
            .name("hopper-notify-coord".into())
            .spawn(move || coordinator_loop(rx, registry, pending))
            .map_err(|e| format!("spawn coordinator: {e}"))?;

        // (3) Flush task — periodically dispatches settled paths into
        // the pipeline. Spawned on the tokio runtime so its
        // `tokio::spawn` calls land on the correct executor.
        let svc = self.clone();
        tokio::spawn(async move { svc.flush_loop().await });

        inner.started = true;
        drop(inner);

        self.reload_watches()
    }

    /// Refresh the set of directories we're subscribed to based on
    /// the current registry contents. Safe to call repeatedly.
    pub fn reload_watches(&self) -> Result<(), String> {
        let enabled: Vec<Watch> = {
            let reg = self.registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.list()
                .map_err(|e| format!("registry list: {e}"))?
                .into_iter()
                .filter(|w| w.enabled)
                .collect()
        };

        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        // Snapshot the current watched-set so we don't double-borrow
        // `inner` while we drive `inner.watcher`.
        let currently_watched = inner.watched.clone();
        let watcher = match inner.watcher.as_mut() {
            Some(w) => w,
            // Service not started yet — nothing to wire.
            None => return Ok(()),
        };

        // Unwatch directories no longer in the enabled set.
        let new_dirs: Vec<PathBuf> = enabled
            .iter()
            .map(|w| PathBuf::from(&w.source_dir))
            .collect();
        for old in &currently_watched {
            if !new_dirs.contains(old) {
                let _ = watcher.unwatch(old);
            }
        }

        // Watch new directories.
        for dir in &new_dirs {
            if currently_watched.contains(dir) {
                continue;
            }
            if !dir.exists() {
                // Skip missing source dirs silently — the registry UI
                // surfaces them in red. We'll pick them up on the
                // next reload after the user creates the folder.
                continue;
            }
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                eprintln!("hopper: watch({}) failed: {e}", dir.display());
            }
        }

        inner.watched = new_dirs;
        Ok(())
    }

    /// Dispatch a single file through the pipeline immediately,
    /// bypassing the debounce queue. Used by the `slab_hopper_run_now`
    /// command for manual re-runs from the UI.
    pub fn run_now(&self, watch_id: i64, path: PathBuf) -> Result<(), String> {
        let watch = {
            let reg = self.registry.lock().unwrap_or_else(|p| p.into_inner());
            reg.get(watch_id)
                .map_err(|e| format!("registry get: {e}"))?
                .ok_or_else(|| format!("watch id {watch_id} not found"))?
        };
        self.spawn_pipeline(watch, path);
        Ok(())
    }

    /// Internal: spawn one pipeline run on the tokio runtime.
    fn spawn_pipeline(&self, watch: Watch, path: PathBuf) {
        let provider = self.provider.clone();
        let recipe_loader = self.recipe_loader.clone();
        let log = self.log.clone();
        let emitter = self.emitter.clone();
        tokio::spawn(async move {
            // `process_one` is CPU/IO-bound and uses blocking sqlite,
            // so we move it onto a blocking-task thread.
            let outcome = tokio::task::spawn_blocking(move || {
                pipeline::process_one(
                    &watch,
                    &path,
                    provider.as_ref(),
                    |rid| (recipe_loader)(rid),
                    &log,
                )
            })
            .await;

            if let Ok(out) = outcome {
                emitter.emit_run_completed(&out.record);
            }
        });
    }

    /// Flush loop — wakes every `FLUSH_TICK_MS`, drains settled
    /// entries from the debounce table, dispatches each through the
    /// pipeline. Runs forever.
    async fn flush_loop(self) {
        let mut ticker = tokio::time::interval(Duration::from_millis(FLUSH_TICK_MS));
        loop {
            ticker.tick().await;

            let ready: Vec<(PathBuf, Watch)> = {
                let pending = {
                    let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
                    inner.pending.clone()
                };
                let mut tbl = pending.lock().unwrap_or_else(|p| p.into_inner());
                let now = Instant::now();
                let mut out = Vec::new();
                tbl.retain(|path, pend| {
                    if now.duration_since(pend.last_seen) >= Duration::from_millis(DEBOUNCE_MS) {
                        out.push((path.clone(), pend.watch.clone()));
                        false
                    } else {
                        true
                    }
                });
                out
            };

            for (path, watch) in ready {
                // Re-check existence at dispatch time — files that
                // were created then deleted within the quiet period
                // are silently dropped.
                if path.exists() {
                    self.spawn_pipeline(watch, path);
                }
            }
        }
    }
}

/// Drain the notify channel and stamp affected `*.pdf` paths into the
/// debounce table. Runs on its own OS thread.
fn coordinator_loop(
    rx: mpsc::Receiver<notify::Result<Event>>,
    registry: Arc<Mutex<HopperRegistry>>,
    pending: Arc<Mutex<HashMap<PathBuf, Pending>>>,
) {
    while let Ok(res) = rx.recv() {
        let ev = match res {
            Ok(e) => e,
            Err(_) => continue,
        };
        // Only care about Create/Modify — Remove/Rename can't yield a
        // new finished PDF in this folder.
        if !matches!(
            ev.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
        ) {
            continue;
        }

        for path in ev.paths {
            if !is_pdf_path(&path) {
                continue;
            }
            // Map the path back to its owning watch — first enabled
            // watch whose source_dir is `path`'s parent. macOS reports
            // canonical paths (e.g. `/private/tmp/...`) while the
            // registry may store `/tmp/...`, so we canonicalize both
            // sides for comparison.
            let parent = match path.parent() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            let parent_canon = std::fs::canonicalize(&parent).unwrap_or(parent);
            let watch = {
                let reg = match registry.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                let all = match reg.list() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                all.into_iter().find(|w| {
                    if !w.enabled {
                        return false;
                    }
                    let src = Path::new(&w.source_dir);
                    let src_canon =
                        std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
                    src_canon == parent_canon
                })
            };
            let watch = match watch {
                Some(w) => w,
                None => continue,
            };

            let mut tbl = pending.lock().unwrap_or_else(|p| p.into_inner());
            tbl.insert(
                path,
                Pending {
                    last_seen: Instant::now(),
                    watch,
                },
            );
        }
    }
}

fn is_pdf_path(p: &Path) -> bool {
    // Skip hidden files (`.DS_Store`, `.foo`) and obvious temporaries.
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') || name.ends_with(".tmp") || name.ends_with(".crdownload") {
            return false;
        }
    }
    p.extension()
        .and_then(|s| s.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::hopper::pipeline::NullProvider;
    use crate::pdf::hopper::registry::WatchInput;
    use std::fs;
    use std::time::Duration;

    fn write_dummy_pdf(path: &Path) {
        use std::io::Write;
        let mut f = fs::File::create(path).unwrap();
        f.write_all(b"%PDF-1.4\n%dummy\n").unwrap();
    }

    fn make_service(source: &Path, output: &Path, db: &Path) -> HopperService {
        let mut reg = HopperRegistry::open(db).unwrap();
        reg.add(WatchInput {
            source_dir: source.display().to_string(),
            output_dir: output.display().to_string(),
            recipe_id: None,
            rename_pattern: None,
            ai_rename: false,
        })
        .unwrap();
        let log_db = db.with_extension("log.db");
        let log = HopperLog::open(&log_db).unwrap();
        HopperService::new(
            reg,
            log,
            Arc::new(NullProvider),
            null_recipe_loader(),
            Arc::new(NullEmitter),
        )
    }

    #[test]
    fn is_pdf_path_filters_correctly() {
        assert!(is_pdf_path(Path::new("/x/foo.pdf")));
        assert!(is_pdf_path(Path::new("/x/Foo.PDF")));
        assert!(!is_pdf_path(Path::new("/x/foo.txt")));
        assert!(!is_pdf_path(Path::new("/x/.hidden.pdf")));
        assert!(!is_pdf_path(Path::new("/x/foo.pdf.tmp")));
        assert!(!is_pdf_path(Path::new("/x/.DS_Store")));
        assert!(!is_pdf_path(Path::new("/x/download.crdownload")));
    }

    #[test]
    fn null_emitter_is_no_op() {
        let emitter = NullEmitter;
        let record = crate::pdf::hopper::log::RunRecord {
            id: 1,
            watch_id: 1,
            input_path: "in".into(),
            output_path: None,
            status: crate::pdf::hopper::log::RunStatus::Success,
            error: None,
            duration_ms: 0,
            ai_title: None,
            started_at: "0".into(),
        };
        // Must not panic.
        emitter.emit_run_completed(&record);
    }

    #[test]
    fn null_recipe_loader_returns_none() {
        let loader = null_recipe_loader();
        assert!(loader("anything").is_none());
        assert!(loader("").is_none());
    }

    #[test]
    fn reload_watches_before_start_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let svc = make_service(
            dir.path(),
            &dir.path().join("out"),
            &dir.path().join("hopper.db"),
        );
        // Calling reload before start must be safe + return Ok.
        assert!(svc.reload_watches().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_now_dispatches_pipeline_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        let output = dir.path().join("out");
        fs::create_dir_all(&source).unwrap();
        let svc = make_service(&source, &output, &dir.path().join("hopper.db"));

        let pdf = source.join("Test Brief.pdf");
        write_dummy_pdf(&pdf);

        // Look up the watch id we just inserted.
        let id = svc
            .registry
            .lock()
            .unwrap()
            .list()
            .unwrap()
            .first()
            .unwrap()
            .id;

        svc.run_now(id, pdf.clone()).unwrap();

        // Wait up to 2s for the file to appear in the output dir.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if output.join("Test_Brief.pdf").exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            output.join("Test_Brief.pdf").exists(),
            "expected output file to materialize"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn watcher_picks_up_new_pdf_after_debounce() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        let output = dir.path().join("out");
        fs::create_dir_all(&source).unwrap();
        let svc = make_service(&source, &output, &dir.path().join("hopper.db"));

        svc.start().expect("start service");

        // Give the watcher a beat to register the dir.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let pdf = source.join("Watched.pdf");
        write_dummy_pdf(&pdf);

        // Wait up to 5s (DEBOUNCE 700 ms + pipeline overhead).
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut materialized = false;
        while Instant::now() < deadline {
            if output.join("Watched.pdf").exists() {
                materialized = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            materialized,
            "watcher did not dispatch file within 5s; check notify backend on this OS"
        );
    }
}
