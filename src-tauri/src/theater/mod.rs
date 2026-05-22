//! Theater — presenter mode state machine.
//!
//! Two synchronized windows (audience fullscreen + presenter control) share
//! one [`TheaterState`] over the Tauri event bus. This module owns the
//! state-of-truth: navigation, ink strokes, blackout/whiteout/laser flags,
//! and serde shape pinned by unit tests so the frontend twin stays honest.

pub mod session;
pub mod state;

pub use session::TheaterManager;
pub use state::{InkStroke, TheaterState};
