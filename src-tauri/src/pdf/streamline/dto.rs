//! DTOs for the streamline (linearization) subsystem. Serialized across the
//! Tauri command boundary, so JSON snake_case for the front-end.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearizationStatus {
    /// File is already linearized per PDF 1.4 §F.
    Linearized,
    /// Valid PDF, but not linearized.
    NotLinearized,
    /// PDF could not be parsed.
    Damaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearizeStats {
    /// Bytes the reader needs to fetch before page 1 paints. For a
    /// non-linearized file this equals `total_bytes` (must download
    /// everything to reach the trailing xref). For a linearized file
    /// it's typically a few hundred KB.
    pub first_page_prefix_bytes: u64,
    /// Total file length, bytes.
    pub total_bytes: u64,
    /// Primary hint stream size, bytes (0 if not linearized).
    pub hint_stream_bytes: u64,
    /// Page count, 1-indexed.
    pub page_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearizeReport {
    pub input_path: String,
    /// `None` for inspect-only calls.
    pub output_path: Option<String>,
    pub before: LinearizeStats,
    pub after: Option<LinearizeStats>,
    pub status: LinearizationStatus,
    pub warnings: Vec<String>,
}
