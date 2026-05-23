// PDF operations for Slab.
//
// Each operation lives in its own submodule so the surface stays small and
// each feature can be tested in isolation.

pub mod annot_export;
pub mod annotations;
pub mod auto_redact;
pub mod bates;
pub mod bates_batch;
pub mod booklet;
pub mod compactor;
pub mod compress;
pub mod crop;
pub mod diff;
pub mod duplicate;
pub mod edit_text;
pub mod encrypt;
pub mod extract;
pub mod flatten;
pub mod grayscale;
pub mod header_footer;
pub mod info;
pub mod insert;
pub mod invert;
pub mod legal_stamp;
pub mod library;
pub mod loom;
pub mod md2pdf;
pub mod merge;
pub mod metadata;
pub mod nup;
pub mod ocr;
pub mod outline;
pub mod page_labels;
pub mod page_numbers;
pub mod pages;
pub mod pages_build;
pub mod pages_undo;
pub mod pdfa;
pub mod polyglot;
pub mod preflight;
pub mod press;
pub mod redact;
pub mod redact_true;
pub mod repair;
pub mod reverse;
pub mod sanitize;
pub mod scan_audit;
pub mod slides;
pub mod split;
pub mod split_pattern;
pub mod stamp_annotations;
pub mod table_extract;
pub mod visual_diff;
pub mod watermark;

#[cfg(test)]
pub mod test_fixtures;

use std::io;
use thiserror::Error;

/// Top-level error type. All PDF ops produce one of these; they convert
/// cleanly into the JSON `CmdResult::Err` the Svelte side renders.
#[derive(Debug, Error)]
pub enum PdfError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("pdf parse/serialize error: {0}")]
    Lopdf(#[from] lopdf::Error),

    #[error("input file does not exist: {0}")]
    InputMissing(String),

    #[error("wrong password")]
    WrongPassword,

    #[error("no input files provided")]
    NoInputs,

    #[error("output path is empty")]
    EmptyOutput,

    #[error("operation failed: {0}")]
    Other(String),
}

/// Atomic write: stage to `<dir>/.slab-tmp.<pid>.<file>`, fsync, then
/// `rename(2)` over the target. POSIX rename is atomic on the same
/// filesystem, so a crash between `write` and `rename` leaves the
/// original file untouched. Closes issue #26 acceptance criterion #4
/// (drag-reorder updates page tree atomically — no half-written files).
pub fn atomic_save(target: &std::path::Path, bytes: &[u8]) -> Result<(), PdfError> {
    use std::io::Write;

    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| PdfError::Other("atomic_save target has no parent dir".into()))?;
    let stem = target.file_name().and_then(|s| s.to_str()).unwrap_or("out");
    let tmp = parent.join(format!(".slab-tmp.{}.{}", std::process::id(), stem));

    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, target) {
        // Best-effort cleanup; the original file is untouched either way.
        let _ = std::fs::remove_file(&tmp);
        return Err(PdfError::Io(e));
    }
    Ok(())
}

#[cfg(test)]
mod atomic_save_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn atomic_save_writes_then_renames() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.pdf");
        fs::write(&target, b"old contents").unwrap();
        atomic_save(&target, b"new contents").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new contents");
    }

    #[test]
    fn atomic_save_leaves_no_tempfile_on_success() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.pdf");
        atomic_save(&target, b"hi").unwrap();
        let leftover: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".slab-tmp"))
            .collect();
        assert!(leftover.is_empty(), "tempfile leaked: {leftover:?}");
    }

    #[test]
    fn atomic_save_creates_new_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("brand-new.pdf");
        assert!(!target.exists());
        atomic_save(&target, b"fresh").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"fresh");
    }

    #[test]
    fn atomic_save_rejects_target_without_parent() {
        // An empty path has no parent; should error rather than panic.
        let err = atomic_save(std::path::Path::new(""), b"x").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("parent"), "got: {msg}");
    }
}
