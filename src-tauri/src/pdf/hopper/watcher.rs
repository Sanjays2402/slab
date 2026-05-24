//! Hopper watcher — `notify`-backed filesystem watcher (skeleton).
//!
//! **Status: scaffold only — full implementation lands in Task 4 of
//! the v3.20.0 Hopper plan.** This file currently provides the public
//! type surface so the rest of the module (registry, pipeline, log,
//! cmds) compiles and can be exercised by unit tests. The async
//! service that spawns `tokio::task` per enabled `Watch`, debounces
//! events, and dispatches to `pipeline::process_one` is filed for
//! the next cron tick.
//!
//! The `notify` crate is already in `Cargo.toml` so adding the real
//! implementation in Task 4 won't touch the dep graph.

use std::sync::Arc;
use std::sync::Mutex;

use super::log::HopperLog;
use super::pipeline::TitleProvider;
use super::registry::HopperRegistry;

/// Owns the per-watch background tasks. Calling `start` (Task 4) will
/// spawn one `tokio::task` per enabled watch. For now the struct just
/// holds the shared handles so `cmds.rs` can register it via Tauri's
/// `app.manage()` and we have a stable type from day one.
pub struct HopperService {
    pub registry: Arc<Mutex<HopperRegistry>>,
    pub log: Arc<Mutex<HopperLog>>,
    pub provider: Arc<dyn TitleProvider>,
}

impl HopperService {
    /// Construct a service handle. Does **not** spawn any tasks yet —
    /// the watcher loop is wired in Task 4.
    pub fn new(registry: HopperRegistry, log: HopperLog, provider: Arc<dyn TitleProvider>) -> Self {
        Self {
            registry: Arc::new(Mutex::new(registry)),
            log: Arc::new(Mutex::new(log)),
            provider,
        }
    }
}
