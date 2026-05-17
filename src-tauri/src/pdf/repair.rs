// Rebuild a PDF — prune unreachable objects, re-serialize with a clean
// xref. Useful for partially-corrupt files (broken xref tables, dangling
// indirect refs) and for shrinking PDFs that accumulated cruft from
// incremental edits.
//
// Strategy: load the document via lopdf (which is permissive about xref
// errors — it'll scan-rebuild a broken xref), call `prune_objects()` to
// drop unreachable indirect objects, compress streams, and save. The
// resulting file has a freshly-built xref table and no orphan objects.
//
// This is NOT a full PDF "validator" — we don't fix malformed content
// streams or repair encrypted-but-no-key files. It IS what people
// usually mean when they say "open this in Acrobat and Save As to fix
// it." Most "this PDF won't open" complaints are xref-table problems
// that this handles.

use crate::pdf::PdfError;
use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepairReport {
    /// Total indirect objects in the loaded document before pruning.
    pub objects_before: u32,
    /// Indirect objects after pruning unreachable ones.
    pub objects_after: u32,
    /// File size in bytes before repair.
    pub bytes_before: u64,
    /// File size in bytes after repair.
    pub bytes_after: u64,
    /// How many objects were pruned (= objects_before - objects_after).
    pub objects_pruned: u32,
}

pub fn repair(input: &Path, output: &Path) -> Result<RepairReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let bytes_before = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);

    let mut doc = Document::load(input)?;
    let objects_before = doc.objects.len() as u32;

    let pruned = doc.prune_objects();
    let objects_after = doc.objects.len() as u32;

    // Compress streams + reserialize with a clean xref.
    doc.compress();

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.save(output)?;

    let bytes_after = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);

    Ok(RepairReport {
        objects_before,
        objects_after,
        objects_pruned: pruned.len() as u32,
        bytes_before,
        bytes_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use lopdf::{dictionary, Object, Stream};

    #[test]
    fn repair_round_trips_valid_pdf() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let report = repair(&src, &dst).unwrap();
        assert!(report.objects_before > 0);
        // The fixture may carry one or two trivially-unreachable objects
        // after lopdf's load (depending on version); we just want repair
        // to succeed without exploding the count.
        assert!(report.objects_after <= report.objects_before);
        // Output exists and is a valid 3-page PDF.
        assert!(dst.exists());
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn repair_prunes_unreachable_objects() {
        // Build a PDF that has indirect objects no one references.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("bloated.pdf");
        let dst = tmp.path().join("clean.pdf");

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        // Reachable: one minimal page.
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        // Add 5 indirect objects that NOTHING references — pure cruft.
        for i in 0..5 {
            let payload = format!("cruft object {}", i).into_bytes();
            doc.add_object(Stream::new(dictionary! {}, payload));
        }

        let id_bytes: Vec<u8> = (0..16).map(|i| 0x42u8.wrapping_add(i)).collect();
        doc.trailer.set(
            "ID",
            Object::Array(vec![
                Object::string_literal(id_bytes.clone()),
                Object::string_literal(id_bytes),
            ]),
        );
        doc.save(&src).unwrap();

        let report = repair(&src, &dst).unwrap();
        // At minimum the 5 cruft streams should be pruned. lopdf may also
        // drop other transient bookkeeping objects, so use >=.
        assert!(
            report.objects_pruned >= 5,
            "expected at least 5 pruned, got {}",
            report.objects_pruned
        );
        assert!(report.objects_after < report.objects_before);
        // Output still loads + is a 1-page PDF.
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 1);
    }

    #[test]
    fn repair_rejects_missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("out.pdf");
        let err = repair(&tmp.path().join("nope.pdf"), &dst).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
