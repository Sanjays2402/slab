// Library query — filter / sort / list documents with tags eager-loaded.
//
// Implementation lands in Task 4.

use super::registry::{DocumentRecord, LibraryDb, LibraryError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryFilter {
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    pub title_substring: Option<String>,
    pub limit: Option<u32>,
    #[serde(default)]
    pub sort: SortBy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    AddedDesc,
    TitleAsc,
    LastSeenDesc,
}

/// Stub — full implementation in Task 4.
pub fn query_documents(
    _db: &LibraryDb,
    _filter: &LibraryFilter,
) -> Result<Vec<DocumentRecord>, LibraryError> {
    Ok(Vec::new())
}
