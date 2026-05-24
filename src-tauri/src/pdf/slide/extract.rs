//! Per-page text-run + speaker-note + page-dimension extraction.

use crate::pdf::reflow::extract::extract_text_runs;
use crate::pdf::reflow::types::TextRun;
use crate::pdf::slide::errors::SlideError;

use lopdf::{Document, Object};

/// Group `reflow::extract` runs by page (1-indexed key in TextRun.page).
pub(crate) fn extract_per_page(doc: &Document) -> Result<Vec<Vec<TextRun>>, SlideError> {
    let runs = extract_text_runs(doc)?;
    // Count pages to size the result so empty pages still get a bucket.
    let n_pages = doc.get_pages().len();
    if n_pages == 0 {
        return Err(SlideError::Empty);
    }
    let mut pages: Vec<Vec<TextRun>> = (0..n_pages).map(|_| Vec::new()).collect();
    for run in runs {
        let idx = (run.page.saturating_sub(1)) as usize;
        if idx < pages.len() {
            pages[idx].push(run);
        }
    }
    Ok(pages)
}

/// Page dimensions in points (MediaBox-derived), in document page order.
/// Falls back to US Letter (612 × 792) when the MediaBox is missing or malformed.
pub(crate) fn page_dimensions(doc: &Document) -> Vec<(f32, f32)> {
    let pages = doc.get_pages();
    let mut out = Vec::with_capacity(pages.len());
    // BTreeMap iteration is in key order so pages come out in numerical order.
    for (_n, pid) in pages {
        let dims = doc
            .get_object(pid)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| {
                // Look up MediaBox, walking the page-tree if inherited.
                if let Ok(mb) = d.get(b"MediaBox") {
                    parse_mediabox(mb)
                } else {
                    // Search parent.
                    let mut cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
                    let mut found: Option<(f32, f32)> = None;
                    while let Some(pref) = cur {
                        if let Ok(pdict) = doc.get_object(pref).and_then(|o| o.as_dict()) {
                            if let Ok(mb) = pdict.get(b"MediaBox") {
                                if let Some(d) = parse_mediabox(mb) {
                                    found = Some(d);
                                    break;
                                }
                            }
                            cur = pdict
                                .get(b"Parent")
                                .ok()
                                .and_then(|p| p.as_reference().ok());
                        } else {
                            break;
                        }
                    }
                    found
                }
            })
            .unwrap_or((612.0, 792.0));
        out.push(dims);
    }
    out
}

fn parse_mediabox(o: &Object) -> Option<(f32, f32)> {
    let arr = o.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let f = |i: usize| -> Option<f32> {
        let v = arr.get(i)?;
        match v {
            Object::Integer(n) => Some(*n as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        }
    };
    let (x0, y0, x1, y1) = (f(0)?, f(1)?, f(2)?, f(3)?);
    Some(((x1 - x0).abs(), (y1 - y0).abs()))
}

/// For each page (document order), Option<String> contents of any `/Text`
/// (sticky-note) annotation found on that page. Concatenated with `"\n\n"`
/// when multiple exist. Returns one entry per page (`None` when no notes).
pub(crate) fn extract_notes_per_page(doc: &Document) -> Result<Vec<Option<String>>, SlideError> {
    let pages = doc.get_pages();
    let mut out = Vec::with_capacity(pages.len());
    for (_n, pid) in pages {
        let mut accum: Vec<String> = Vec::new();
        if let Ok(page) = doc.get_object(pid).and_then(|o| o.as_dict()) {
            if let Ok(annots) = page.get(b"Annots") {
                let annot_refs: Vec<lopdf::ObjectId> = match annots {
                    Object::Array(arr) => {
                        arr.iter().filter_map(|o| o.as_reference().ok()).collect()
                    }
                    Object::Reference(r) => doc
                        .get_object(*r)
                        .ok()
                        .and_then(|o| o.as_array().ok())
                        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                for a in annot_refs {
                    if let Ok(adict) = doc.get_object(a).and_then(|o| o.as_dict()) {
                        let is_text = adict
                            .get(b"Subtype")
                            .ok()
                            .and_then(|o| o.as_name().ok())
                            .map(|n| n == b"Text")
                            .unwrap_or(false);
                        if !is_text {
                            continue;
                        }
                        if let Ok(c) = adict.get(b"Contents") {
                            let s = match c {
                                Object::String(bytes, _) => {
                                    String::from_utf8_lossy(bytes).to_string()
                                }
                                _ => continue,
                            };
                            if !s.trim().is_empty() {
                                accum.push(s);
                            }
                        }
                    }
                }
            }
        }
        out.push(if accum.is_empty() {
            None
        } else {
            Some(accum.join("\n\n"))
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_doc_returns_empty_error() {
        let doc = Document::new();
        let r = extract_per_page(&doc);
        assert!(matches!(r, Err(SlideError::Empty)));
    }

    #[test]
    fn page_dimensions_defaults_when_no_pages() {
        let doc = Document::new();
        let dims = page_dimensions(&doc);
        assert!(dims.is_empty());
    }
}
