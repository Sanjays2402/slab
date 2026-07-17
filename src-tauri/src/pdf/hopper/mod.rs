//! Hopper — watched-folder PDF automation (v3.20.0).
//!
//! Drop a PDF into a configured source directory; Hopper auto-runs a
//! pre-saved Atelier recipe on it, optionally asks Beacon for a 4-6 word
//! AI title, applies a rename template (e.g. `{date}_{ai_title}.pdf`),
//! and files the output into the destination directory. The whole loop
//! is local-first: no cloud, no telemetry, works offline.
//!
//! ## Why this exists
//!
//! Adobe AutoActions are gated behind enterprise Acrobat licensing
//! (~$300+/user/yr). Hazel ($42 one-time) has no PDF-aware steps.
//! PDF Expert ships zero folder automation. Slab v3.20.0 is the only
//! consumer desktop product that bundles drop-folder PDF automation
//! with local AI rename — for free.
//!
//! ## Module layout
//!
//! - [`registry`] — sqlite-backed CRUD over `Watch` configurations.
//! - [`rename`] — pure-fn template substitution + slugification.
//! - [`pipeline`] — single-file orchestration (recipe → AI title → move).
//! - [`watcher`] — `notify`-backed fs watcher + debounce queue.
//! - [`log`] — append-only history of `RunRecord`s for the live tail UI.
//! - [`cmds`] — Tauri command surface (registered in `lib.rs`).
//!
//! ## Data flow
//!
//! ```text
//!   fs event ─► watcher ─► debounce ─► pipeline::process_one
//!                                              │
//!                                              ├─► atelier::run_recipe
//!                                              ├─► ai::ollama (AI title)
//!                                              ├─► rename::apply_pattern
//!                                              └─► move + log::record
//! ```

pub mod backfill;
pub mod cmds;
pub mod coverage;
pub mod log;
pub mod pipeline;
pub mod registry;
pub mod rename;
pub mod rules;
pub mod watcher;

pub use backfill::{
    execute_backfill, plan_backfill, ActionKind, BackfillOutcome, BackfillReport, BackfillRun,
    OutcomeStatus, PlannedAction,
};
pub use log::{HopperLog, RunRecord, RunStatus};
pub use registry::{HopperRegistry, Watch, WatchInput};
