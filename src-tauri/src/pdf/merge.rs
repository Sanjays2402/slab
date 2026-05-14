// Merge multiple PDFs into one.
//
// Uses lopdf, a pure-Rust PDF library — no native dependencies.
// Approach: load all docs, renumber object IDs to avoid collisions,
// concatenate page trees, write out.

use crate::pdf::PdfError;
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::path::Path;

/// Merge the given PDFs in order, writing the result to `output`.
///
/// Returns the number of pages in the resulting document.
pub fn merge_pdfs<P: AsRef<Path>>(inputs: &[P], output: P) -> Result<usize, PdfError> {
    if inputs.is_empty() {
        return Err(PdfError::NoInputs);
    }
    let out = output.as_ref();
    if out.as_os_str().is_empty() {
        return Err(PdfError::EmptyOutput);
    }

    // Validate inputs up-front so we fail before doing any work.
    for p in inputs {
        if !p.as_ref().exists() {
            return Err(PdfError::InputMissing(p.as_ref().display().to_string()));
        }
    }

    let mut max_id: u32 = 1;
    let mut docs: Vec<Document> = Vec::with_capacity(inputs.len());
    for p in inputs {
        let mut doc = Document::load(p.as_ref())?;
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;
        docs.push(doc);
    }

    // The first doc becomes the canvas: we accumulate every other doc's
    // pages into its catalog / page tree.
    let mut documents_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut documents_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();

    for doc in &mut docs {
        documents_pages.extend(
            doc.get_pages()
                .into_iter()
                .map(|(_, object_id)| (object_id, doc.get_object(object_id).unwrap().to_owned())),
        );
        documents_objects.extend(doc.objects.clone());
    }

    // Build a fresh document and re-insert everything.
    let mut document = Document::with_version("1.5");

    // Find the Catalog and Pages dict from the original documents — we'll
    // rebuild them.
    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (object_id, object) in documents_objects.iter() {
        match object.type_name().unwrap_or("") {
            "Catalog" => {
                if catalog_object.is_none() {
                    catalog_object = Some((*object_id, object.clone()));
                }
            }
            "Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref existing)) = pages_object {
                        if let Ok(old_dict) = existing.as_dict() {
                            dictionary.extend(old_dict);
                        }
                    }
                    pages_object = Some((
                        if let Some((id, _)) = pages_object {
                            id
                        } else {
                            *object_id
                        },
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            "Page" | "Outlines" | "Outline" => {} // handled separately or skipped
            _ => {
                document.objects.insert(*object_id, object.clone());
            }
        }
    }

    let (pages_id, pages_obj) = pages_object
        .ok_or_else(|| PdfError::Other("none of the source PDFs has a Pages root".into()))?;

    // Rewrite each page's Parent to point at our merged pages dict.
    for (page_id, page_obj) in documents_pages.iter() {
        if let Ok(dict) = page_obj.as_dict() {
            let mut dict = dict.clone();
            dict.set("Parent", pages_id);
            document.objects.insert(*page_id, Object::Dictionary(dict));
        }
    }

    // Build the new Pages dict with all kids.
    if let Ok(dict) = pages_obj.as_dict() {
        let mut dict = dict.clone();
        let kids: Vec<Object> = documents_pages
            .keys()
            .map(|id| Object::Reference(*id))
            .collect();
        let count = kids.len() as i64;
        dict.set("Kids", kids);
        dict.set("Count", count);
        document.objects.insert(pages_id, Object::Dictionary(dict));
    }

    // Wire up the catalog.
    let (catalog_id, catalog_obj) = catalog_object
        .ok_or_else(|| PdfError::Other("none of the source PDFs has a Catalog root".into()))?;
    if let Ok(dict) = catalog_obj.as_dict() {
        let mut dict = dict.clone();
        dict.set("Pages", pages_id);
        dict.remove(b"Outlines");
        document
            .objects
            .insert(catalog_id, Object::Dictionary(dict));
    }

    document.trailer.set("Root", catalog_id);
    document.max_id = document.objects.len() as u32;
    document.renumber_objects();
    document.compress();
    document.save(out)?;

    let final_count = documents_pages.len();
    Ok(final_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn merge_rejects_empty_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out.pdf");
        let inputs: Vec<&Path> = vec![];
        assert!(matches!(
            merge_pdfs(&inputs, out.as_path()),
            Err(PdfError::NoInputs)
        ));
    }

    #[test]
    fn merge_rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let out = tmp.path().join("out.pdf");
        let inputs = vec![bogus.as_path()];
        assert!(matches!(
            merge_pdfs(&inputs, out.as_path()),
            Err(PdfError::InputMissing(_))
        ));
    }

    /// Build a real minimal one-page PDF with lopdf, then merge two copies
    /// of it. The output should exist, parse, and contain 2 pages.
    #[test]
    fn merge_two_one_page_pdfs() {
        use lopdf::{dictionary, Stream};

        fn make_blank(path: &Path) {
            let mut doc = Document::with_version("1.5");
            let pages_id = doc.new_object_id();
            let font_id = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
            });
            let resources_id = doc.add_object(dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            });
            let content = lopdf::content::Content {
                operations: vec![
                    lopdf::content::Operation::new("BT", vec![]),
                    lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    lopdf::content::Operation::new("Td", vec![100.into(), 600.into()]),
                    lopdf::content::Operation::new("Tj", vec![Object::string_literal("Slab")]),
                    lopdf::content::Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Resources" => resources_id,
            });
            let pages = dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            };
            doc.objects.insert(pages_id, Object::Dictionary(pages));
            let catalog_id = doc.add_object(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            doc.trailer.set("Root", catalog_id);
            doc.compress();
            doc.save(path).unwrap();
        }

        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        let out = tmp.path().join("merged.pdf");
        make_blank(&a);
        make_blank(&b);

        let count = merge_pdfs(&[a.as_path(), b.as_path()], out.as_path()).unwrap();
        assert_eq!(count, 2);
        assert!(out.exists());
        assert!(fs::metadata(&out).unwrap().len() > 100);

        // Parse the output and verify it has 2 pages.
        let merged = Document::load(&out).unwrap();
        assert_eq!(merged.get_pages().len(), 2);
    }
}
