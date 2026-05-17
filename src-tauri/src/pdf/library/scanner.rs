// Library scanner — walks a registered folder, hashes + page-counts
// PDFs, upserts into the registry. Lives here so the slow file IO is
// kept out of the registry crate.
//
// Implementation lands in Task 3.

use super::registry::{FolderRecord, LibraryDb, LibraryError};
use serde::{Deserialize, Serialize};

/// Per-scan summary the UI can render in the Library panel header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanReport {
    pub folder_id: i64,
    pub files_scanned: u32,
    pub files_added: u32,
    pub files_updated: u32,
    pub files_unchanged: u32,
}

/// Stub — full implementation in Task 3.
pub fn scan_folder(_db: &mut LibraryDb, folder: &FolderRecord) -> Result<ScanReport, LibraryError> {
    Ok(ScanReport {
        folder_id: folder.id,
        files_scanned: 0,
        files_added: 0,
        files_updated: 0,
        files_unchanged: 0,
    })
}
