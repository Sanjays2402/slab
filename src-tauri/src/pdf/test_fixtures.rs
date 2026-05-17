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

/// Build an `n`-page PDF where every page contains a single image XObject and
/// NO text-drawing operators. Useful for testing scan-audit / OCR pipelines
/// that need to recognise "this looks like a scanned page".
pub fn make_image_only_pdf(path: &Path, n: u32) {
    assert!(n >= 1, "n must be >= 1");

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    // A trivial 1x1 grayscale image stream — enough for lopdf to treat as an
    // image XObject. We don't need the bytes to decode to anything useful.
    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1_i64,
            "Height" => 1_i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8_i64,
            "Filter" => "ASCIIHexDecode",
        },
        b"FF>".to_vec(),
    ));

    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! { "Im1" => image_id },
    });

    let mut kids: Vec<Object> = Vec::with_capacity(n as usize);
    for _ in 1..=n {
        // Place the image, no text operators at all.
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("q", vec![]),
                lopdf::content::Operation::new(
                    "cm",
                    vec![
                        612.into(),
                        0.into(),
                        0.into(),
                        792.into(),
                        0.into(),
                        0.into(),
                    ],
                ),
                lopdf::content::Operation::new("Do", vec!["Im1".into()]),
                lopdf::content::Operation::new("Q", vec![]),
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
    let file_id_bytes: Vec<u8> = (0..16).map(|i| 0x77u8.wrapping_add(i)).collect();
    let id_obj = lopdf::Object::Array(vec![
        lopdf::Object::string_literal(file_id_bytes.clone()),
        lopdf::Object::string_literal(file_id_bytes),
    ]);
    doc.trailer.set("ID", id_obj);
    doc.save(path).unwrap();
}

/// Build a PDF where page 1 has text, page 2 is image-only, page 3 is both
/// (text + image XObject). Useful for mixed-classification tests.
pub fn make_mixed_pdf(path: &Path) {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1_i64,
            "Height" => 1_i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8_i64,
            "Filter" => "ASCIIHexDecode",
        },
        b"FF>".to_vec(),
    ));
    let resources_text = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let resources_image = doc.add_object(dictionary! {
        "XObject" => dictionary! { "Im1" => image_id },
    });
    let resources_both = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
        "XObject" => dictionary! { "Im1" => image_id },
    });

    let text_content = lopdf::content::Content {
        operations: vec![
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
            lopdf::content::Operation::new("Td", vec![100.into(), 600.into()]),
            lopdf::content::Operation::new(
                "Tj",
                vec![Object::string_literal(
                    "Some actual page text here for audit",
                )],
            ),
            lopdf::content::Operation::new("ET", vec![]),
        ],
    };
    let image_content = lopdf::content::Content {
        operations: vec![
            lopdf::content::Operation::new("q", vec![]),
            lopdf::content::Operation::new(
                "cm",
                vec![
                    612.into(),
                    0.into(),
                    0.into(),
                    792.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            lopdf::content::Operation::new("Do", vec!["Im1".into()]),
            lopdf::content::Operation::new("Q", vec![]),
        ],
    };
    let both_content = lopdf::content::Content {
        operations: vec![
            lopdf::content::Operation::new("q", vec![]),
            lopdf::content::Operation::new(
                "cm",
                vec![
                    612.into(),
                    0.into(),
                    0.into(),
                    792.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            lopdf::content::Operation::new("Do", vec!["Im1".into()]),
            lopdf::content::Operation::new("Q", vec![]),
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
            lopdf::content::Operation::new("Td", vec![100.into(), 100.into()]),
            lopdf::content::Operation::new(
                "Tj",
                vec![Object::string_literal("Caption text under image")],
            ),
            lopdf::content::Operation::new("ET", vec![]),
        ],
    };

    let text_stream = doc.add_object(Stream::new(dictionary! {}, text_content.encode().unwrap()));
    let image_stream = doc.add_object(Stream::new(dictionary! {}, image_content.encode().unwrap()));
    let both_stream = doc.add_object(Stream::new(dictionary! {}, both_content.encode().unwrap()));

    let p1 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => text_stream,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources_text,
    });
    let p2 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => image_stream,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources_image,
    });
    let p3 = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => both_stream,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources_both,
    });

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![p1.into(), p2.into(), p3.into()],
            "Count" => 3_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let file_id_bytes: Vec<u8> = (0..16).map(|i| 0x33u8.wrapping_add(i)).collect();
    let id_obj = lopdf::Object::Array(vec![
        lopdf::Object::string_literal(file_id_bytes.clone()),
        lopdf::Object::string_literal(file_id_bytes),
    ]);
    doc.trailer.set("ID", id_obj);
    doc.save(path).unwrap();
}

/// Build a one-page PDF with a tiny 3-column / 4-row table laid out via
/// absolute Td positioning. Designed so `pdftotext -bbox-layout` emits a
/// clean grid of `<word>` bboxes for `pdf::table_extract` tests.
///
/// Layout (PDF user-space, origin bottom-left):
/// - Col x positions: 100, 250, 400.
/// - Row y positions: 700 (header), 680, 660, 640.
/// - 12-pt Helvetica throughout.
pub fn make_table_pdf(path: &Path) {
    use lopdf::content::{Content, Operation};

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

    // (x, y, text) cells, in drawing order. After each cell we translate
    // back to origin so the next absolute Td works as expected.
    let cells: &[(i64, i64, &str)] = &[
        (100, 700, "Name"),
        (250, 700, "Score"),
        (400, 700, "Note"),
        (100, 680, "Alpha"),
        (250, 680, "10"),
        (400, 680, "ok"),
        (100, 660, "Beta"),
        (250, 660, "20"),
        (400, 660, "fail"),
        (100, 640, "Gamma"),
        (250, 640, "30"),
        (400, 640, "ok"),
    ];

    let mut ops: Vec<Operation> = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 12.into()]),
    ];
    for (x, y, txt) in cells {
        ops.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        ops.push(Operation::new("Tj", vec![Object::string_literal(*txt)]));
        ops.push(Operation::new("Td", vec![(-*x).into(), (-*y).into()]));
    }
    ops.push(Operation::new("ET", vec![]));

    let content = Content { operations: ops };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources_id,
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let file_id_bytes: Vec<u8> = (0..16).map(|i| 0x55u8.wrapping_add(i)).collect();
    let id_obj = lopdf::Object::Array(vec![
        lopdf::Object::string_literal(file_id_bytes.clone()),
        lopdf::Object::string_literal(file_id_bytes),
    ]);
    doc.trailer.set("ID", id_obj);
    doc.save(path).unwrap();
}
