//! Batch signing — sign every `*.pdf` in a folder via rayon, with progress events.

#![allow(dead_code)] // Scaffolding — fleshed out in Task 5 of the v3.11.0 plan.

use std::path::PathBuf;
use std::time::Duration;

/// One row in a [`BatchReport`].
#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub input: PathBuf,
    pub output: PathBuf,
    pub ok: bool,
    pub error: Option<String>,
    pub elapsed: Duration,
}

/// Aggregated result of [`sign_folder`].
#[derive(Debug, Clone, Default)]
pub struct BatchReport {
    pub entries: Vec<BatchEntry>,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub elapsed: Duration,
}

impl BatchReport {
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.succeeded as f64 / self.total as f64
        }
    }
}
