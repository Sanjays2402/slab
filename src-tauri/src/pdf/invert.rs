// Invert page colors — content-stream level luminance inversion.
//
// Strategy mirrors `grayscale.rs`: walk each page's content streams and
// rewrite color operators in-place. For each color set:
//   "rg" / "RG" (RGB)     -> RGB' = (1-r, 1-g, 1-b)
//   "k"  / "K"  (CMYK)    -> CMYK' = (1-c, 1-m, 1-y, k)  (invert CMY, keep K)
//   "g"  / "G"  (gray)    -> 1-g
//   "sc"/"SC"/"scn"/"SCN" with 1/3/4 numeric args treated like above
//
// This produces a "dark mode" / "negative" copy of the PDF, useful for:
//   - eye-saver reading mode of light-background documents
//   - extracting white-on-black presentations from black-on-white sources
//   - troubleshooting print artifacts (light text on light paper)
//
// As with `grayscale`, this does NOT invert raster image XObjects —
// only vector content. For most text-heavy PDFs that's the right
// trade-off (small output, no quality loss). A future raster-aware
// pass can land separately.

// Walking lopdf operators is naturally a match-then-if shape;
// clippy's collapsible-* lints fight readability here.
#![allow(clippy::collapsible_if, clippy::collapsible_match)]

use crate::pdf::PdfError;
use lopdf::content::Content;
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

// Pattern: walking lopdf operators is naturally a match-then-if-let
// shape; clippy's collapsible-* lints fight readability here.

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InvertOpts {
    /// 1-based page numbers to invert. Empty = all pages.
    pub pages: Vec<u32>,
}

pub fn invert_colors(input: &Path, output: &Path, opts: InvertOpts) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let target: std::collections::HashSet<u32> = opts.pages.iter().copied().collect();

    let mut touched = 0u32;
    for (num, page_id) in &page_ids {
        if !target.is_empty() && !target.contains(num) {
            continue;
        }
        let stream_ids = collect_content_stream_ids(&doc, *page_id);
        for sid in stream_ids {
            if rewrite_stream_inverted(&mut doc, sid)? {
                touched += 1;
            }
        }
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
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

fn rewrite_stream_inverted(doc: &mut Document, sid: lopdf::ObjectId) -> Result<bool, PdfError> {
    let stream_bytes = {
        let obj = doc.get_object(sid)?;
        let Object::Stream(s) = obj else {
            return Ok(false);
        };
        s.decompressed_content()
            .unwrap_or_else(|_| s.content.clone())
    };

    let mut content = match Content::decode(&stream_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };
    let mut changed = false;

    for op in content.operations.iter_mut() {
        match op.operator.as_str() {
            "g" => {
                if let Some(v) = num(&op.operands, 0) {
                    op.operands = vec![Object::Real(1.0 - v)];
                    changed = true;
                }
            }
            "G" => {
                if let Some(v) = num(&op.operands, 0) {
                    op.operands = vec![Object::Real(1.0 - v)];
                    changed = true;
                }
            }
            "rg" | "RG" => {
                if op.operands.len() >= 3 {
                    let r = num(&op.operands, 0).unwrap_or(0.0);
                    let g = num(&op.operands, 1).unwrap_or(0.0);
                    let b = num(&op.operands, 2).unwrap_or(0.0);
                    op.operands = vec![
                        Object::Real(1.0 - r),
                        Object::Real(1.0 - g),
                        Object::Real(1.0 - b),
                    ];
                    changed = true;
                }
            }
            "k" | "K" => {
                if op.operands.len() >= 4 {
                    let c = num(&op.operands, 0).unwrap_or(0.0);
                    let m = num(&op.operands, 1).unwrap_or(0.0);
                    let y = num(&op.operands, 2).unwrap_or(0.0);
                    let k = num(&op.operands, 3).unwrap_or(0.0);
                    op.operands = vec![
                        Object::Real(1.0 - c),
                        Object::Real(1.0 - m),
                        Object::Real(1.0 - y),
                        Object::Real(k),
                    ];
                    changed = true;
                }
            }
            "sc" | "SC" | "scn" | "SCN" => match op.operands.len() {
                1 => {
                    let v = num(&op.operands, 0).unwrap_or(0.0);
                    op.operands = vec![Object::Real(1.0 - v)];
                    changed = true;
                }
                3 => {
                    let r = num(&op.operands, 0).unwrap_or(0.0);
                    let g = num(&op.operands, 1).unwrap_or(0.0);
                    let b = num(&op.operands, 2).unwrap_or(0.0);
                    op.operands = vec![
                        Object::Real(1.0 - r),
                        Object::Real(1.0 - g),
                        Object::Real(1.0 - b),
                    ];
                    changed = true;
                }
                4 => {
                    let c = num(&op.operands, 0).unwrap_or(0.0);
                    let m = num(&op.operands, 1).unwrap_or(0.0);
                    let y = num(&op.operands, 2).unwrap_or(0.0);
                    let k = num(&op.operands, 3).unwrap_or(0.0);
                    op.operands = vec![
                        Object::Real(1.0 - c),
                        Object::Real(1.0 - m),
                        Object::Real(1.0 - y),
                        Object::Real(k),
                    ];
                    changed = true;
                }
                _ => {}
            },
            _ => {}
        }
    }

    if !changed {
        return Ok(false);
    }

    let encoded = content
        .encode()
        .map_err(|e| PdfError::Other(format!("re-encode content stream: {e}")))?;

    if let Ok(obj) = doc.get_object_mut(sid) {
        if let Object::Stream(s) = obj {
            s.set_plain_content(encoded);
            let _ = s.compress();
        }
    }
    Ok(true)
}

fn num(ops: &[Object], idx: usize) -> Option<f32> {
    match ops.get(idx)? {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn invert_round_trip_preserves_page_count() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("inv.pdf");
        make_n_page_pdf(&src, 4);
        let _ = invert_colors(&src, &dst, InvertOpts { pages: vec![] }).unwrap();
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 4);
    }

    #[test]
    fn invert_specific_pages_only() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("inv.pdf");
        make_n_page_pdf(&src, 3);
        // Specifying pages [2] should not error and should keep page count.
        let _ = invert_colors(&src, &dst, InvertOpts { pages: vec![2] }).unwrap();
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");
        let err = invert_colors(&bogus, &dst, InvertOpts::default()).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
