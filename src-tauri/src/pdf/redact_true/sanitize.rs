//! Document-level metadata sanitizer.
//!
//! Even after text streams are excised and annotations scrubbed, a PDF can
//! still leak redacted content via:
//!   * `/Info` dictionary — Title, Author, Subject, Keywords, Producer, etc.
//!   * `/Metadata` XMP stream — same fields plus custom XMP packets.
//!   * `/Names /EmbeddedFiles` — attached source files.
//!   * `/Names /JavaScript` — embedded JS that could exfiltrate via cloud APIs.
//!   * Incremental update history — old object versions still in the file.
//!
//! `sanitize_document` rewrites the document to remove these leak vectors.
//! It does NOT excise text/images/annotations — that's the job of
//! `text_stream` and `annotations`. Always call this LAST in the pipeline.

use crate::pdf::PdfError;
use lopdf::{Document, Object};

/// Report of what was scrubbed from the document.
#[derive(Debug, Default, Clone)]
pub struct SanitizeReport {
    pub info_fields_cleared: u32,
    pub xmp_metadata_removed: bool,
    pub embedded_files_removed: u32,
    pub javascript_removed: u32,
    pub structure_tree_removed: bool,
}

impl SanitizeReport {
    pub fn anything_removed(&self) -> bool {
        self.info_fields_cleared > 0
            || self.xmp_metadata_removed
            || self.embedded_files_removed > 0
            || self.javascript_removed > 0
            || self.structure_tree_removed
    }
}

/// Strip every common document-level leak vector. Returns a report.
pub fn sanitize_document(doc: &mut Document) -> Result<SanitizeReport, PdfError> {
    let mut report = SanitizeReport::default();

    // --- /Info dictionary ---
    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        let info_id = match info_ref {
            Object::Reference(r) => Some(*r),
            _ => None,
        };
        if let Some(id) = info_id {
            if let Ok(Object::Dictionary(d)) = doc.get_object_mut(id) {
                let leaky_keys: &[&[u8]] = &[
                    b"Title",
                    b"Author",
                    b"Subject",
                    b"Keywords",
                    b"Producer",
                    b"Creator",
                    b"CreationDate",
                    b"ModDate",
                    b"Trapped",
                ];
                for k in leaky_keys {
                    if d.remove(k).is_some() {
                        report.info_fields_cleared += 1;
                    }
                }
            }
        }
    }

    // --- Catalog-level scrubs ---
    let catalog_id = doc.trailer.get(b"Root").ok().and_then(|o| {
        if let Object::Reference(r) = o {
            Some(*r)
        } else {
            None
        }
    });

    if let Some(cid) = catalog_id {
        if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(cid) {
            if cat.remove(b"Metadata").is_some() {
                report.xmp_metadata_removed = true;
            }
            if cat.remove(b"StructTreeRoot").is_some() {
                report.structure_tree_removed = true;
            }
            // /MarkInfo references the (now-removed) struct tree.
            cat.remove(b"MarkInfo");
            // /PieceInfo holds app-specific private data, drop it.
            cat.remove(b"PieceInfo");

            // /Names sub-tree: clear EmbeddedFiles + JavaScript.
            let names_obj = cat.get(b"Names").cloned().ok();
            if let Some(names_obj) = names_obj {
                let names_id = if let Object::Reference(r) = names_obj {
                    Some(r)
                } else {
                    None
                };
                if let Some(nid) = names_id {
                    if let Ok(Object::Dictionary(names_dict)) = doc.get_object_mut(nid) {
                        if names_dict.remove(b"EmbeddedFiles").is_some() {
                            report.embedded_files_removed += 1;
                        }
                        if names_dict.remove(b"JavaScript").is_some() {
                            report.javascript_removed += 1;
                        }
                    }
                }
            }

            // /OpenAction can launch JavaScript on document open — kill it.
            if let Ok(Object::Dictionary(cat2)) = doc.get_object_mut(cid) {
                cat2.remove(b"OpenAction");
                cat2.remove(b"AA");
            }
        }
    }

    // --- Drop incremental-update history by forcing a full rewrite ---
    // `Document::compress` followed by `save_to(buffer)` regenerates the xref
    // table from scratch, dropping any prior object versions. The caller is
    // responsible for invoking save (we don't write to disk here).
    doc.compress();

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn doc_with_metadata() -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let contents = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => contents,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let xmp_id = doc.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<xmp:title>SECRET</xmp:title>".to_vec(),
        ));
        let names_id = doc.add_object(dictionary! {
            "EmbeddedFiles" => dictionary! { "Names" => Object::Array(vec![]) },
            "JavaScript"    => dictionary! { "Names" => Object::Array(vec![]) },
        });
        let cat = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Metadata" => xmp_id,
            "Names" => names_id,
            "StructTreeRoot" => dictionary! { "Type" => "StructTreeRoot" },
            "MarkInfo" => dictionary! { "Marked" => true },
            "OpenAction" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert('hi')") },
        });
        doc.trailer.set("Root", cat);

        let info = doc.add_object(dictionary! {
            "Title" => Object::string_literal("SECRET-TITLE"),
            "Author" => Object::string_literal("Alice"),
            "Producer" => Object::string_literal("AcmePDF 1.0"),
        });
        doc.trailer.set("Info", info);

        doc
    }

    #[test]
    fn clears_info_dict_leaky_fields() {
        let mut doc = doc_with_metadata();
        let report = sanitize_document(&mut doc).unwrap();
        assert!(report.info_fields_cleared >= 3);

        let info_ref = doc.trailer.get(b"Info").unwrap();
        let info_id = if let Object::Reference(r) = info_ref {
            *r
        } else {
            panic!()
        };
        let info_dict = doc.get_object(info_id).unwrap().as_dict().unwrap();
        assert!(info_dict.get(b"Title").is_err());
        assert!(info_dict.get(b"Author").is_err());
    }

    #[test]
    fn removes_xmp_metadata() {
        let mut doc = doc_with_metadata();
        let report = sanitize_document(&mut doc).unwrap();
        assert!(report.xmp_metadata_removed);

        let cid = if let Ok(Object::Reference(r)) = doc.trailer.get(b"Root") {
            *r
        } else {
            panic!()
        };
        let cat = doc.get_object(cid).unwrap().as_dict().unwrap();
        assert!(cat.get(b"Metadata").is_err());
    }

    #[test]
    fn removes_struct_tree_and_openaction() {
        let mut doc = doc_with_metadata();
        let report = sanitize_document(&mut doc).unwrap();
        assert!(report.structure_tree_removed);

        let cid = if let Ok(Object::Reference(r)) = doc.trailer.get(b"Root") {
            *r
        } else {
            panic!()
        };
        let cat = doc.get_object(cid).unwrap().as_dict().unwrap();
        assert!(cat.get(b"StructTreeRoot").is_err());
        assert!(cat.get(b"MarkInfo").is_err());
        assert!(cat.get(b"OpenAction").is_err());
    }

    #[test]
    fn removes_embedded_files_and_javascript() {
        let mut doc = doc_with_metadata();
        let report = sanitize_document(&mut doc).unwrap();
        assert!(report.embedded_files_removed >= 1);
        assert!(report.javascript_removed >= 1);
    }

    #[test]
    fn anything_removed_flag() {
        let mut doc = doc_with_metadata();
        let report = sanitize_document(&mut doc).unwrap();
        assert!(report.anything_removed());
    }
}
