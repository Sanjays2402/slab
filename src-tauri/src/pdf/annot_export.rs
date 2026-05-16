//! Read annotations (highlights, sticky notes) out of a PDF and render
//! them as Markdown. Pairs with `pdf::annotations::append` (the writer).
//!
//! Why a separate module? `annotations` is a write-path with a
//! quad-points-aware `Annotation` enum tuned for `append`. The reader
//! has different concerns: tolerate unknown /Subtype values, recover
//! the bounding rect even when QuadPoints are missing, and emit a
//! human-friendly export rather than a perfect round-trip.

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One annotation extracted from a PDF page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedAnnotation {
    /// 1-based page number (Markdown-friendly).
    pub page: u32,
    /// PDF /Subtype — typically "Highlight", "Text", "Underline", "FreeText",
    /// etc. We pass through whatever's in the file.
    pub subtype: String,
    /// Author / title (/T). Empty if unset.
    pub author: String,
    /// Note body (/Contents). Empty if unset.
    pub contents: String,
    /// Bounding rect in PDF user space (left, bottom, right, top).
    /// Use it to anchor the export to the page geometry if needed.
    pub rect: Option<[f64; 4]>,
}

/// Extract every annotation in `input`, in page-order.
pub fn extract(input: &Path) -> Result<Vec<ExtractedAnnotation>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let doc = Document::load(input)?;
    let mut out: Vec<ExtractedAnnotation> = Vec::new();

    // doc.get_pages() returns a BTreeMap so iteration is already in
    // page-order (1, 2, 3, ...).
    for (page_num, page_id) in doc.get_pages() {
        let page_dict = match doc.get_object(page_id).and_then(Object::as_dict) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let annots_ref = match page_dict.get(b"Annots") {
            Ok(a) => a,
            Err(_) => continue,
        };
        // /Annots can be either an inline array or a reference to one.
        let annots_arr = match resolve_array(&doc, annots_ref) {
            Some(a) => a,
            None => continue,
        };

        for annot_ref in annots_arr {
            let annot_dict_id = match annot_ref {
                Object::Reference(id) => *id,
                _ => continue,
            };
            let annot_dict = match doc.get_object(annot_dict_id).and_then(Object::as_dict) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let subtype = annot_dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();

            if subtype.is_empty() {
                continue;
            }

            let author = read_pdf_string(annot_dict.get(b"T").ok());
            let contents = read_pdf_string(annot_dict.get(b"Contents").ok());
            let rect = annot_dict.get(b"Rect").ok().and_then(read_rect);

            out.push(ExtractedAnnotation {
                page: page_num,
                subtype,
                author,
                contents,
                rect,
            });
        }
    }
    Ok(out)
}

/// Render `annotations` as Markdown. Highlights and notes get their own
/// section if any are present; everything else lands under "Other".
pub fn to_markdown(file_label: &str, annotations: &[ExtractedAnnotation]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Annotations — {file_label}\n\n"));
    if annotations.is_empty() {
        s.push_str("_No annotations found._\n");
        return s;
    }
    s.push_str(&format!(
        "_Total: {} annotation(s)._\n\n",
        annotations.len()
    ));

    let highlights: Vec<_> = annotations
        .iter()
        .filter(|a| a.subtype == "Highlight" || a.subtype == "Underline" || a.subtype == "Squiggly")
        .collect();
    let notes: Vec<_> = annotations
        .iter()
        .filter(|a| a.subtype == "Text" || a.subtype == "FreeText")
        .collect();
    let other: Vec<_> = annotations
        .iter()
        .filter(|a| {
            !matches!(
                a.subtype.as_str(),
                "Highlight" | "Underline" | "Squiggly" | "Text" | "FreeText"
            )
        })
        .collect();

    if !highlights.is_empty() {
        s.push_str("## Highlights\n\n");
        for a in &highlights {
            write_block(&mut s, a);
        }
    }
    if !notes.is_empty() {
        s.push_str("## Notes\n\n");
        for a in &notes {
            write_block(&mut s, a);
        }
    }
    if !other.is_empty() {
        s.push_str("## Other annotations\n\n");
        for a in &other {
            write_block(&mut s, a);
        }
    }
    s
}

fn write_block(s: &mut String, a: &ExtractedAnnotation) {
    s.push_str(&format!("**Page {}**", a.page));
    if !a.author.is_empty() {
        s.push_str(&format!(" — _{}_", a.author));
    }
    s.push('\n');
    if !a.contents.is_empty() {
        // Indent each line with a blockquote so multi-line content stays
        // visually attached to the page heading.
        for line in a.contents.lines() {
            s.push_str("> ");
            s.push_str(line);
            s.push('\n');
        }
    } else {
        s.push_str("> _(no content)_\n");
    }
    s.push('\n');
}

fn resolve_array<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Vec<Object>> {
    match obj {
        Object::Array(a) => Some(a),
        Object::Reference(id) => doc.get_object(*id).and_then(Object::as_array).ok(),
        _ => None,
    }
}

fn read_pdf_string(obj: Option<&Object>) -> String {
    match obj {
        Some(Object::String(bytes, _)) => {
            // PDF strings may be PdfDocEncoding or UTF-16BE w/ BOM. Try
            // UTF-16BE first if a BOM is present, else fall back to
            // lossy UTF-8 which works for ASCII (the common case).
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                let pairs: Vec<u16> = bytes[2..]
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&pairs)
            } else {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
        _ => String::new(),
    }
}

fn read_rect(obj: &Object) -> Option<[f64; 4]> {
    let arr = obj.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (i, v) in arr.iter().enumerate() {
        out[i] = match v {
            Object::Integer(n) => *n as f64,
            Object::Real(n) => *n as f64,
            _ => return None,
        };
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::annotations::{append, Annotation};
    use lopdf::{dictionary, Object};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_blank_pdf(out: &std::path::Path) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.save(out).unwrap();
    }

    #[test]
    fn extract_returns_empty_for_blank_pdf() {
        let dir = tempdir().unwrap();
        let path: PathBuf = dir.path().join("blank.pdf");
        make_blank_pdf(&path);
        let extracted = extract(&path).unwrap();
        assert!(extracted.is_empty());
    }

    #[test]
    fn missing_input_errors() {
        let dir = tempdir().unwrap();
        let err = extract(&dir.path().join("nope.pdf")).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }

    #[test]
    fn roundtrip_highlight_and_note() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.pdf");
        let out = dir.path().join("out.pdf");
        make_blank_pdf(&src);

        let annots = vec![
            Annotation::Highlight {
                page_index: 0,
                quads: vec![[100.0, 100.0, 200.0, 100.0, 100.0, 90.0, 200.0, 90.0]],
                contents: "important bit".into(),
                author: "Cake".into(),
                color: None,
            },
            Annotation::Note {
                page_index: 0,
                xy: [50.0, 700.0],
                contents: "remember\nthis".into(),
                author: "Sanjay".into(),
                color: None,
            },
        ];
        let n = append(&src, &out, &annots).unwrap();
        assert_eq!(n, 2);

        let extracted = extract(&out).unwrap();
        assert_eq!(extracted.len(), 2);
        let h = extracted.iter().find(|a| a.subtype == "Highlight").unwrap();
        assert_eq!(h.page, 1);
        assert_eq!(h.author, "Cake");
        assert_eq!(h.contents, "important bit");
        let note = extracted.iter().find(|a| a.subtype == "Text").unwrap();
        assert_eq!(note.contents, "remember\nthis");
        assert_eq!(note.author, "Sanjay");
    }

    #[test]
    fn markdown_renders_sections() {
        let annots = vec![
            ExtractedAnnotation {
                page: 1,
                subtype: "Highlight".into(),
                author: "Cake".into(),
                contents: "lorem".into(),
                rect: None,
            },
            ExtractedAnnotation {
                page: 2,
                subtype: "Text".into(),
                author: "".into(),
                contents: "ipsum\nDolor".into(),
                rect: None,
            },
        ];
        let md = to_markdown("paper.pdf", &annots);
        assert!(md.contains("# Annotations — paper.pdf"));
        assert!(md.contains("## Highlights"));
        assert!(md.contains("## Notes"));
        assert!(md.contains("**Page 1**"));
        assert!(md.contains("_Cake_"));
        assert!(md.contains("> lorem"));
        assert!(md.contains("> ipsum"));
        assert!(md.contains("> Dolor"));
    }

    #[test]
    fn markdown_handles_empty() {
        let md = to_markdown("x.pdf", &[]);
        assert!(md.contains("_No annotations found._"));
    }
}
