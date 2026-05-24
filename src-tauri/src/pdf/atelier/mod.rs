//! Atelier — Slab's workflow automation engine.
//!
//! A `Recipe` is an ordered list of `Step`s (OCR, auto-redact, Bates, sign,
//! flatten, etc.). `run_recipe` pipes a single input PDF through every step
//! in order, emitting per-step progress events. `run_recipe_batch` runs the
//! same recipe over every PDF in a folder, in parallel, with a per-file ×
//! per-step progress matrix streamed back to the UI.
//!
//! This is the Adobe Acrobat Action Wizard equivalent — but free, offline,
//! and able to chase 200 files at a time without an Adobe seat.

pub mod batch;
pub mod cmds;
pub mod recipe;
pub mod run;

pub use batch::{run_recipe_batch, BatchProgress, BatchReport};
pub use recipe::{Recipe, Step};
pub use run::{run_recipe, Progress, RecipeReport};
