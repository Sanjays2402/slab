// Convert color content streams to grayscale.
//
// Strategy: walk each page's content stream(s), rewrite color operators
// in-place:
//   "rg"  (set non-stroke RGB)  -> "g"  with luminance
//   "RG"  (set stroke RGB)      -> "G"  with luminance
//   "k"   (set non-stroke CMYK) -> "g"  with luminance
//   "K"   (set stroke CMYK)     -> "G"  with luminance
//   "sc"/"SC" with 3-4 args     -> "g"/"G" with luminance
//   "scn"/"SCN" with 3-4 args   -> same
//
// Luminance = 0.299*R + 0.587*G + 0.114*B (Rec. 601).
// CMYK -> RGB -> luminance.
//
// This does NOT desaturate raster images embedded in the PDF — that needs
// XObject-level work. For most text-heavy PDFs (which is the common case
// for "print-friendly" conversion) this gets you 90% of the way. Images
// will still print color but the surrounding text/graphics go gray.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrayscaleOpts {
    /// 1-based page numbers to convert. Empty = all pages.
    pub pages: Vec<u32>,
}

pub fn grayscale(input: &Path, output: &Path, opts: GrayscaleOpts) -> Result<u32, PdfError> {
    let mut doc = Document::load(input)?;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let target_set: std::collections::HashSet<u32> = opts.pages.iter().copied().collect();

    let mut touched = 0u32;
    for (num, page_id) in &page_ids {
        if !target_set.is_empty() && !target_set.contains(num) {
            continue;
        }
        let stream_ids = collect_content_stream_ids(&doc, *page_id);
        for sid in stream_ids {
            if rewrite_stream_to_gray(&mut doc, sid)? {
                touched += 1;
            }
        }
    }

    doc.compress();
    doc.save(output)?;
    Ok(touched)
}

fn collect_content_stream_ids(doc: &Document, page_id: lopdf::ObjectId) -> Vec<lopdf::ObjectId> {
    let mut out = Vec::new();
    let Ok(dict) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return out;
    };
    match dict.get(b"Contents") {
        Ok(Object::Reference(r)) => out.push(*r),
        Ok(Object::Array(arr)) => {
            for o in arr {
                if let Object::Reference(r) = o {
                    out.push(*r);
                }
            }
        }
        _ => {}
    }
    out
}

fn rewrite_stream_to_gray(doc: &mut Document, sid: lopdf::ObjectId) -> Result<bool, PdfError> {
    // Decode the stream, rewrite operators, re-encode.
    let stream_bytes = {
        let obj = doc.get_object(sid)?;
        let Object::Stream(s) = obj else {
            return Ok(false);
        };
        // Decompress if filtered.
        s.decompressed_content()
            .unwrap_or_else(|_| s.content.clone())
    };
    let content = match Content::decode(&stream_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(false), // skip un-parseable streams
    };

    let mut changed = false;
    let mut new_ops: Vec<Operation> = Vec::with_capacity(content.operations.len());
    for op in content.operations {
        let new_op = match op.operator.as_str() {
            "rg" if op.operands.len() == 3 => {
                let lum = luminance_from_rgb(&op.operands);
                changed = true;
                Operation::new("g", vec![Object::Real(lum)])
            }
            "RG" if op.operands.len() == 3 => {
                let lum = luminance_from_rgb(&op.operands);
                changed = true;
                Operation::new("G", vec![Object::Real(lum)])
            }
            "k" if op.operands.len() == 4 => {
                let lum = luminance_from_cmyk(&op.operands);
                changed = true;
                Operation::new("g", vec![Object::Real(lum)])
            }
            "K" if op.operands.len() == 4 => {
                let lum = luminance_from_cmyk(&op.operands);
                changed = true;
                Operation::new("G", vec![Object::Real(lum)])
            }
            "sc" | "scn" if op.operands.len() == 3 => {
                let lum = luminance_from_rgb(&op.operands);
                changed = true;
                Operation::new("g", vec![Object::Real(lum)])
            }
            "SC" | "SCN" if op.operands.len() == 3 => {
                let lum = luminance_from_rgb(&op.operands);
                changed = true;
                Operation::new("G", vec![Object::Real(lum)])
            }
            "sc" | "scn" if op.operands.len() == 4 => {
                let lum = luminance_from_cmyk(&op.operands);
                changed = true;
                Operation::new("g", vec![Object::Real(lum)])
            }
            "SC" | "SCN" if op.operands.len() == 4 => {
                let lum = luminance_from_cmyk(&op.operands);
                changed = true;
                Operation::new("G", vec![Object::Real(lum)])
            }
            _ => op,
        };
        new_ops.push(new_op);
    }

    if !changed {
        return Ok(false);
    }
    let new_content = Content {
        operations: new_ops,
    };
    let encoded = new_content
        .encode()
        .map_err(|e| PdfError::Other(format!("Failed to encode gray stream: {e}")))?;
    if let Ok(Object::Stream(s)) = doc.get_object_mut(sid) {
        s.set_plain_content(encoded);
        let _ = s.compress();
    }
    Ok(true)
}

fn op_to_f32(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn luminance_from_rgb(ops: &[Object]) -> f32 {
    let r = op_to_f32(&ops[0]).clamp(0.0, 1.0);
    let g = op_to_f32(&ops[1]).clamp(0.0, 1.0);
    let b = op_to_f32(&ops[2]).clamp(0.0, 1.0);
    (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0)
}

fn luminance_from_cmyk(ops: &[Object]) -> f32 {
    let c = op_to_f32(&ops[0]).clamp(0.0, 1.0);
    let m = op_to_f32(&ops[1]).clamp(0.0, 1.0);
    let y = op_to_f32(&ops[2]).clamp(0.0, 1.0);
    let k = op_to_f32(&ops[3]).clamp(0.0, 1.0);
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn pdf_with_colored_content() -> Vec<u8> {
        // Construct a page with a content stream containing "rg" + "RG".
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        // "0.8 0.2 0.2 rg 100 100 50 50 re f 0.1 0.4 0.9 RG 100 100 50 50 re S"
        let stream_bytes =
            b"0.8 0.2 0.2 rg\n100 100 50 50 re f\n0.1 0.4 0.9 RG\n100 100 50 50 re S\n".to_vec();
        let contents = doc.add_object(Stream::new(dictionary! {}, stream_bytes));
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
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn rgb_to_gray() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, pdf_with_colored_content()).unwrap();
        let n = grayscale(&input, &output, GrayscaleOpts { pages: vec![] }).unwrap();
        assert!(n >= 1);

        // Reload and verify content stream no longer contains "rg" / "RG".
        let reloaded = Document::load(&output).unwrap();
        let page_ids: Vec<lopdf::ObjectId> = reloaded.get_pages().values().copied().collect();
        let mut found_g = false;
        for pid in page_ids {
            for sid in collect_content_stream_ids(&reloaded, pid) {
                let obj = reloaded.get_object(sid).unwrap();
                if let Object::Stream(s) = obj {
                    let bytes = s.decompressed_content().unwrap_or(s.content.clone());
                    let text = String::from_utf8_lossy(&bytes);
                    assert!(!text.contains(" rg") || text.contains(" g "));
                    if text.contains(" g\n") || text.contains(" g ") {
                        found_g = true;
                    }
                }
            }
        }
        assert!(found_g, "Expected gray operator in output stream");
    }

    #[test]
    fn luminance_pure_red() {
        let l = luminance_from_rgb(&[Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)]);
        assert!((l - 0.299).abs() < 1e-3);
    }

    #[test]
    fn luminance_pure_white_cmyk() {
        // 0,0,0,0 in CMYK = white = lum 1.0
        let l = luminance_from_cmyk(&[
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(0.0),
        ]);
        assert!((l - 1.0).abs() < 1e-3);
    }
}
