// Shared test helpers for the pdf submodules.
//
// Builds tiny but real PDFs we can round-trip through every operation.

#![cfg(test)]

use lopdf::{dictionary, Document, Object, Stream};
use std::path::Path;

/// Build a real `n`-page PDF at `path` where every page reads "Slab page <i>".
pub fn make_n_page_pdf(path: &Path, n: u32) {
    assert!(n >= 1, "n must be >= 1");

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

    let mut kids: Vec<Object> = Vec::with_capacity(n as usize);
    for i in 1..=n {
        let label = format!("Slab page {}", i);
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
                lopdf::content::Operation::new("Td", vec![100.into(), 600.into()]),
                lopdf::content::Operation::new("Tj", vec![Object::string_literal(label)]),
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
        kids.push(page_id.into());
    }

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => n as i64,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    // V1/V2 encryption requires /ID in the trailer.
    let file_id_bytes: Vec<u8> = (0..16).map(|i| 0x42u8.wrapping_add(i)).collect();
    let id_obj = lopdf::Object::Array(vec![
        lopdf::Object::string_literal(file_id_bytes.clone()),
        lopdf::Object::string_literal(file_id_bytes),
    ]);
    doc.trailer.set("ID", id_obj);
    doc.save(path).unwrap();
}
