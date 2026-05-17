// Library scanner — walks a registered folder, hashes + page-counts
// PDFs, upserts into the registry.
//
// Performance design:
//
// * **Quick-key skip**: every PDF in the folder gets its `(size,
//   mtime_ns)` fingerprint compared against the row already in the
//   library. If it matches, we bump `last_seen_at` and move on — no
//   hash, no PDF parse. Re-scanning a 1000-doc library should be
//   ~filesystem-stat-fast.
// * **Hash only on first sight or mismatch**: SHA-256 of the file
//   bytes is the canonical identity used by the rest of Slab (matches
//   the Beacon embedding-index key). Cheap enough we always compute
//   it when something has changed.
// * **Page count from lopdf**: fast (we only parse the trailer + root
//   catalog Pages count, not the whole stream).
// * **Symlinks not followed**: keeps the scan bounded for users who
//   point Slab at their home directory and have ~/Downloads linked
//   from a network share.
// * **Bounded depth 12**: stops a runaway scan on infinite symlink
//   loops outside the home dir.

use super::registry::{FolderRecord, LibraryDb, LibraryError};
use lopdf::Document;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MAX_DEPTH: usize = 12;

/// Per-scan summary the UI can render in the Library panel header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ScanReport {
    pub folder_id: i64,
    pub files_scanned: u32,
    pub files_added: u32,
    pub files_updated: u32,
    pub files_unchanged: u32,
}

/// Walk `folder.path` recursively for `*.pdf` files and bring the
/// library DB up to date. Returns counts for the UI.
pub fn scan_folder(db: &mut LibraryDb, folder: &FolderRecord) -> Result<ScanReport, LibraryError> {
    let mut report = ScanReport {
        folder_id: folder.id,
        ..Default::default()
    };

    let walker = WalkDir::new(&folder.path)
        .max_depth(MAX_DEPTH)
        .follow_links(false);

    for entry in walker.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !looks_like_pdf(path) {
            continue;
        }
        report.files_scanned += 1;

        let (size_bytes, mtime_ns) = match read_quick_key(path) {
            Ok(v) => v,
            Err(_) => continue, // unreadable metadata — skip silently
        };
        let path_str = path.to_string_lossy().into_owned();

        let existing = db.find_document_by_path(&path_str)?;
        let matches_quick_key = existing
            .as_ref()
            .is_some_and(|e| e.size_bytes == size_bytes && e.mtime_ns == mtime_ns);

        if let Some(ref e) = existing {
            if matches_quick_key {
                db.touch_document(e.id)?;
                report.files_unchanged += 1;
                continue;
            }
        }

        // First sight or quick-key mismatch — do the heavy work.
        let (hash, pages) = match heavy_inspect(path) {
            Ok(v) => v,
            Err(_) => continue, // corrupt PDF — skip rather than fail the whole scan
        };
        let title = infer_title(path);
        let was_new = existing.is_none();
        db.upsert_document(
            Some(folder.id),
            &path_str,
            title.as_deref(),
            &hash,
            size_bytes,
            mtime_ns,
            Some(pages as i64),
        )?;
        if was_new {
            report.files_added += 1;
        } else {
            report.files_updated += 1;
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    db.mark_folder_scanned(folder.id, now)?;
    Ok(report)
}

fn looks_like_pdf(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

fn read_quick_key(p: &Path) -> Result<(i64, i64), LibraryError> {
    let meta = std::fs::metadata(p)?;
    let size = meta.len() as i64;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    Ok((size, mtime_ns))
}

/// SHA-256 of file contents + page count from lopdf. Performed only
/// when the quick-key indicates a new or changed file.
fn heavy_inspect(p: &Path) -> Result<(String, u32), LibraryError> {
    let hash = sha256_file(p)?;
    let pages = match Document::load(p) {
        Ok(doc) => doc.get_pages().len() as u32,
        Err(_) => 0,
    };
    Ok((hash, pages))
}

fn sha256_file(p: &Path) -> Result<String, LibraryError> {
    let mut file = std::fs::File::open(p)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode_lower(hasher.finalize()))
}

/// Derive a friendly title from the file stem. The PDF Info dict's
/// /Title key is *unreliable* — most PDFs leave it empty or stuff it
/// with generator goop ("untitled document"). Filename is more useful
/// 95% of the time. We let the user override later.
fn infer_title(p: &Path) -> Option<String> {
    p.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.replace('_', " "))
}

mod hex {
    // Tiny inline hex encoder — sha2 returns a GenericArray, not a
    // string, and we don't want to pull `hex = "0.4"` for one
    // function.
    pub fn encode_lower(bytes: impl AsRef<[u8]>) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let b = bytes.as_ref();
        let mut out = String::with_capacity(b.len() * 2);
        for &c in b {
            out.push(HEX[(c >> 4) as usize] as char);
            out.push(HEX[(c & 0x0f) as usize] as char);
        }
        out
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_pdf(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        make_n_page_pdf(&p, 1);
        p
    }

    fn write_non_pdf(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, b"hello world").expect("write non-pdf");
        p
    }

    fn fresh() -> (LibraryDb, TempDir, FolderRecord) {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let dir = TempDir::new().unwrap();
        let folder = db.add_folder(dir.path().to_str().unwrap()).unwrap();
        (db, dir, folder)
    }

    #[test]
    fn scans_empty_folder() {
        let (mut db, _dir, folder) = fresh();
        let r = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r.files_scanned, 0);
        assert_eq!(r.files_added, 0);
        assert_eq!(r.files_updated, 0);
        assert_eq!(r.files_unchanged, 0);
    }

    #[test]
    fn finds_pdfs_in_root() {
        let (mut db, dir, folder) = fresh();
        write_pdf(dir.path(), "a.pdf");
        write_pdf(dir.path(), "b.pdf");
        let r = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r.files_scanned, 2);
        assert_eq!(r.files_added, 2);
        assert_eq!(r.files_unchanged, 0);
    }

    #[test]
    fn finds_pdfs_in_subdir() {
        let (mut db, dir, folder) = fresh();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        write_pdf(&sub, "nested.pdf");
        let r = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r.files_scanned, 1);
        assert_eq!(r.files_added, 1);
    }

    #[test]
    fn skips_non_pdf_files() {
        let (mut db, dir, folder) = fresh();
        write_non_pdf(dir.path(), "notes.txt");
        write_non_pdf(dir.path(), "data.csv");
        write_pdf(dir.path(), "ok.pdf");
        let r = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r.files_scanned, 1);
        assert_eq!(r.files_added, 1);
    }

    #[test]
    fn second_scan_unchanged_is_idempotent() {
        let (mut db, dir, folder) = fresh();
        write_pdf(dir.path(), "a.pdf");
        let r1 = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r1.files_added, 1);
        let r2 = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r2.files_added, 0);
        assert_eq!(r2.files_unchanged, 1);
        assert_eq!(r2.files_updated, 0);
    }

    #[test]
    fn second_scan_after_change_re_hashes() {
        let (mut db, dir, folder) = fresh();
        let p = write_pdf(dir.path(), "a.pdf");
        let r1 = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r1.files_added, 1);

        // Touch + grow the file so the quick-key changes.
        std::thread::sleep(std::time::Duration::from_millis(15));
        let mut bytes = fs::read(&p).unwrap();
        bytes.extend_from_slice(b"%%MORE\n");
        fs::write(&p, &bytes).unwrap();

        let r2 = scan_folder(&mut db, &folder).unwrap();
        assert_eq!(r2.files_added, 0);
        assert_eq!(r2.files_updated, 1);
        assert_eq!(r2.files_unchanged, 0);
    }

    #[test]
    fn folder_last_scanned_at_is_set() {
        let (mut db, _dir, folder) = fresh();
        assert!(folder.last_scanned_at.is_none());
        scan_folder(&mut db, &folder).unwrap();
        let f2 = db.find_folder_by_path(&folder.path).unwrap().unwrap();
        assert!(f2.last_scanned_at.is_some());
    }

    #[test]
    fn looks_like_pdf_is_case_insensitive() {
        assert!(looks_like_pdf(Path::new("a.pdf")));
        assert!(looks_like_pdf(Path::new("a.PDF")));
        assert!(looks_like_pdf(Path::new("a.Pdf")));
        assert!(!looks_like_pdf(Path::new("a.txt")));
        assert!(!looks_like_pdf(Path::new("a")));
    }

    #[test]
    fn infer_title_strips_extension_and_underscores() {
        assert_eq!(
            infer_title(Path::new("/x/lecture_notes_01.pdf")),
            Some("lecture notes 01".to_string())
        );
    }

    #[test]
    fn hex_encoder_roundtrip() {
        assert_eq!(hex::encode_lower([0u8]), "00");
        assert_eq!(hex::encode_lower([255u8]), "ff");
        assert_eq!(hex::encode_lower([0xab, 0xcd]), "abcd");
    }
}
