// Slab Library Mode — Atlas (v0.12.0)
//
// A sqlite-backed registry of folders the user has told Slab to track,
// plus the documents found inside them and a tag system to organize.
// Lives in its own DB file (`~/.slab/library.sqlite`) — explicitly NOT
// the embedding index DB, so the user can blow away their library
// catalogue without losing the Beacon vector store.
//
// Submodules:
//
// * `registry` — schema migration + CRUD on folders/documents/tags.
//   Synchronous, single Connection. Tests use `:memory:`.
// * `scanner` — walks a folder, hashes + page-counts each new PDF,
//   upserts into the registry. Skips unchanged files by (size, mtime)
//   quick-key so re-scans of a 1000-doc folder are cheap.
// * `query` — list/filter/sort with eager tag loading.

pub mod query;
pub mod registry;
pub mod scanner;

pub use query::{query_documents, LibraryFilter, SortBy};
pub use registry::{
    default_db_path, DocumentRecord, FolderRecord, LibraryDb, LibraryError, TagRecord,
};
pub use scanner::{scan_folder, ScanReport};
