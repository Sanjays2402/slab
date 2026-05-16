// Read and write the PDF Info dictionary (Title, Author, Subject, Keywords,
// Creator, Producer) and a separate "strip all metadata" operation that
// purges identifying fields — useful for the privacy-first ethos of Slab.
//
// We deliberately keep this simple and lossless: we never re-stream pages,
// we only mutate (or remove) the Info dictionary entries. XMP metadata
// streams attached to the catalog are also cleared on strip().

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

/// Read just the Info dictionary fields. Distinct from `info()` which also
/// pulls page count / version / size.
pub fn read_metadata(input: &Path) -> Result<Metadata, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    Ok(read_metadata_doc(&doc))
}

fn read_metadata_doc(doc: &Document) -> Metadata {
    let mut m = Metadata::default();
    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_ref.as_reference() {
            if let Ok(info_obj) = doc.get_object(info_ref) {
                if let Ok(dict) = info_obj.as_dict() {
                    m.title = read_str(dict, b"Title");
                    m.author = read_str(dict, b"Author");
                    m.subject = read_str(dict, b"Subject");
                    m.keywords = read_str(dict, b"Keywords");
                    m.creator = read_str(dict, b"Creator");
                    m.producer = read_str(dict, b"Producer");
                }
            }
        }
    }
    m
}

fn read_str(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key)
        .ok()
        .and_then(|o| match o {
            Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

/// Overwrite the Info dictionary with the provided values. Empty/None fields
/// are removed from the Info dict so they don't linger.
pub fn write_metadata(input: &Path, output: &Path, meta: &Metadata) -> Result<(), PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;

    let info_id = ensure_info_id(&mut doc);
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(info_id) {
        set_or_remove(dict, b"Title", meta.title.as_deref());
        set_or_remove(dict, b"Author", meta.author.as_deref());
        set_or_remove(dict, b"Subject", meta.subject.as_deref());
        set_or_remove(dict, b"Keywords", meta.keywords.as_deref());
        set_or_remove(dict, b"Creator", meta.creator.as_deref());
        set_or_remove(dict, b"Producer", meta.producer.as_deref());
    }

    save(doc, output)
}

/// Remove all Info-dictionary fields and the XMP metadata stream from the
/// document catalog. After this, the file contains no identifying metadata
/// (the Info dict itself is left empty rather than removed because some
/// readers expect its presence).
pub fn strip_metadata(input: &Path, output: &Path) -> Result<(), PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;

    let info_id = ensure_info_id(&mut doc);
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(info_id) {
        for key in [
            &b"Title"[..],
            &b"Author"[..],
            &b"Subject"[..],
            &b"Keywords"[..],
            &b"Creator"[..],
            &b"Producer"[..],
            &b"CreationDate"[..],
            &b"ModDate"[..],
        ] {
            dict.remove(key);
        }
    }

    // Drop /Metadata (XMP stream) from the catalog if present.
    if let Ok(catalog_id) = doc.catalog().map(|c| c as *const _) {
        // We need a mutable borrow — re-fetch through trailer.
        let _ = catalog_id; // silence unused
        if let Ok(root_ref) = doc.trailer.get(b"Root") {
            if let Ok(root_id) = root_ref.as_reference() {
                if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(root_id) {
                    dict.remove(b"Metadata");
                }
            }
        }
    }

    save(doc, output)
}

fn ensure_info_id(doc: &mut Document) -> lopdf::ObjectId {
    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(id) = info_ref.as_reference() {
            return id;
        }
    }
    let id = doc.add_object(lopdf::Dictionary::new());
    doc.trailer.set("Info", Object::Reference(id));
    id
}

fn set_or_remove(dict: &mut lopdf::Dictionary, key: &[u8], value: Option<&str>) {
    match value {
        Some(v) if !v.is_empty() => {
            dict.set(
                std::str::from_utf8(key).unwrap_or(""),
                Object::string_literal(v),
            );
        }
        _ => {
            dict.remove(key);
        }
    }
}

fn save(mut doc: Document, output: &Path) -> Result<(), PdfError> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn write_then_read_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);

        let meta = Metadata {
            title: Some("Hello".into()),
            author: Some("Sanjay".into()),
            subject: Some("Test".into()),
            keywords: Some("a, b".into()),
            creator: Some("Slab".into()),
            producer: Some("Slab".into()),
        };
        write_metadata(&src, &dst, &meta).unwrap();
        let read = read_metadata(&dst).unwrap();
        assert_eq!(read.title.as_deref(), Some("Hello"));
        assert_eq!(read.author.as_deref(), Some("Sanjay"));
        assert_eq!(read.subject.as_deref(), Some("Test"));
        assert_eq!(read.keywords.as_deref(), Some("a, b"));
    }

    #[test]
    fn strip_clears_all_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let mid = tmp.path().join("mid.pdf");
        let dst = tmp.path().join("dst.pdf");
        make_n_page_pdf(&src, 1);

        let meta = Metadata {
            title: Some("Personal Letter".into()),
            author: Some("Sanjay".into()),
            ..Default::default()
        };
        write_metadata(&src, &mid, &meta).unwrap();
        strip_metadata(&mid, &dst).unwrap();
        let read = read_metadata(&dst).unwrap();
        assert!(read.title.is_none());
        assert!(read.author.is_none());
        assert!(read.subject.is_none());
        assert!(read.keywords.is_none());
    }
}
