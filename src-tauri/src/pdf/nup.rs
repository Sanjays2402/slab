// N-up — composite N source pages onto each output sheet, scaled to fit.
// Supports 2-up, 4-up, 6-up, 9-up. Great for printing condensed handouts.
//
// Approach: convert each source page into a Form XObject (a reusable
// graphics container — essentially a tiny PDF inside a PDF), then build
// a new page that places those XObjects via "cm" transformation matrices
// at scaled, gridded positions.
//
// We only support 2/4/6/9 to keep the grid math sane. The output sheet
// inherits MediaBox from the first source page.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct NupOpts {
    /// 2, 4, 6, or 9.
    pub n: u32,
    /// Gap between slots, in points. Defaults to 6.
    pub gap: f32,
    /// Outer margin around the sheet, in points. Defaults to 18.
    pub margin: f32,
}

/// Build an N-up output PDF. Returns the number of output pages produced.
pub fn nup(input: &Path, output: &Path, opts: NupOpts) -> Result<u32, PdfError> {
    if !matches!(opts.n, 2 | 4 | 6 | 9) {
        return Err(PdfError::Other("N must be 2, 4, 6, or 9.".into()));
    }
    let (cols, rows) = match opts.n {
        2 => (2u32, 1u32),
        4 => (2, 2),
        6 => (3, 2),
        9 => (3, 3),
        _ => unreachable!(),
    };

    let mut src = Document::load(input)?;
    let src_pages: Vec<lopdf::ObjectId> = src.get_pages().into_values().collect();
    if src_pages.is_empty() {
        return Err(PdfError::Other("Source PDF has no pages.".into()));
    }

    // Use first page's MediaBox for the output sheet (assume uniform pages).
    let (sheet_w, sheet_h) = media_box(&src, src_pages[0])
        .ok_or_else(|| PdfError::Other("Source page has no MediaBox.".into()))?;

    // Convert each source page into an XObject form.
    let xobjects: Vec<(lopdf::ObjectId, (f32, f32))> = src_pages
        .iter()
        .map(|pid| {
            let (w, h) = media_box(&src, *pid).unwrap_or((sheet_w, sheet_h));
            let xid = page_to_xobject(&mut src, *pid, w, h)?;
            Ok((xid, (w, h)))
        })
        .collect::<Result<_, PdfError>>()?;

    // Compose new output document, replacing the page tree with new sheets.
    let inner_w = sheet_w - 2.0 * opts.margin;
    let inner_h = sheet_h - 2.0 * opts.margin;
    let slot_w = (inner_w - opts.gap * (cols as f32 - 1.0)) / cols as f32;
    let slot_h = (inner_h - opts.gap * (rows as f32 - 1.0)) / rows as f32;

    // Collect old catalog so we can replace the Pages tree.
    let catalog_id = src.trailer.get(b"Root")?.as_reference()?;
    let pages_root_id = src
        .get_object(catalog_id)?
        .as_dict()?
        .get(b"Pages")?
        .as_reference()?;

    // Build new sheets.
    let mut new_pages: Vec<lopdf::ObjectId> = Vec::new();
    let chunks: Vec<&[(lopdf::ObjectId, (f32, f32))]> = xobjects.chunks(opts.n as usize).collect();
    for chunk in chunks {
        let mut ops: Vec<Operation> = Vec::new();
        // Resources dict tracks every XObject placed.
        let mut xres = lopdf::Dictionary::new();
        for (i, (xid, (xw, xh))) in chunk.iter().enumerate() {
            let col = (i as u32) % cols;
            let row = (i as u32) / cols;
            // Row 0 at top, like print order.
            let x = opts.margin + (col as f32) * (slot_w + opts.gap);
            let y = sheet_h - opts.margin - ((row + 1) as f32) * slot_h - (row as f32) * opts.gap;

            let scale = (slot_w / xw).min(slot_h / xh);
            let dw = xw * scale;
            let dh = xh * scale;
            let dx = x + (slot_w - dw) / 2.0;
            let dy = y + (slot_h - dh) / 2.0;

            let name = format!("X{i}");
            xres.set(name.clone(), Object::Reference(*xid));

            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new(
                "cm",
                vec![
                    Object::Real(scale),
                    Object::Real(0.0),
                    Object::Real(0.0),
                    Object::Real(scale),
                    Object::Real(dx),
                    Object::Real(dy),
                ],
            ));
            ops.push(Operation::new("Do", vec![name.into()]));
            ops.push(Operation::new("Q", vec![]));
        }
        let content = Content { operations: ops };
        let body = content
            .encode()
            .map_err(|e| PdfError::Other(format!("Failed to encode n-up stream: {e}")))?;
        let stream_id = src.add_object(Stream::new(dictionary! {}, body));

        let new_page = src.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_root_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(sheet_w), Object::Real(sheet_h)],
            "Contents" => Object::Reference(stream_id),
            "Resources" => dictionary! {
                "XObject" => Object::Dictionary(xres),
            },
        });
        new_pages.push(new_page);
    }

    // Replace the old page list with the new one. Old page objects can stay
    // (they're referenced by the XObjects) — only the Pages.Kids list changes.
    let new_count = new_pages.len() as i64;
    if let Object::Dictionary(d) = src.get_object_mut(pages_root_id)? {
        let kids: Vec<Object> = new_pages.iter().map(|id| Object::Reference(*id)).collect();
        d.set("Kids", Object::Array(kids));
        d.set("Count", new_count);
    }

    src.compress();
    src.save(output)?;
    Ok(new_count as u32)
}

fn media_box(doc: &Document, page_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let dict = doc.get_object(page_id).ok()?.as_dict().ok()?;
    let mb = dict.get(b"MediaBox").ok()?.as_array().ok()?;
    if mb.len() < 4 {
        return None;
    }
    let nums: Vec<f32> = mb
        .iter()
        .filter_map(|o| match o {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        })
        .collect();
    if nums.len() < 4 {
        return None;
    }
    Some(((nums[2] - nums[0]).abs(), (nums[3] - nums[1]).abs()))
}

/// Turn a page into a Form XObject so it can be placed via `Do`.
fn page_to_xobject(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    w: f32,
    h: f32,
) -> Result<lopdf::ObjectId, PdfError> {
    // Concatenate page content streams into one bytes blob.
    let body = collect_contents(doc, page_id)?;
    let resources = collect_resources(doc, page_id);

    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1,
        "BBox" => vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)],
        "Resources" => resources,
    };
    if !body.is_empty() {
        dict.set("Length", body.len() as i64);
    }
    let stream = Stream::new(dict, body);
    Ok(doc.add_object(stream))
}

fn collect_contents(doc: &Document, page_id: lopdf::ObjectId) -> Result<Vec<u8>, PdfError> {
    let page = doc.get_object(page_id)?.as_dict()?;
    let mut out: Vec<u8> = Vec::new();
    match page.get(b"Contents") {
        Ok(Object::Reference(r)) => {
            if let Ok(Object::Stream(s)) = doc.get_object(*r) {
                out.extend_from_slice(
                    &s.decompressed_content()
                        .unwrap_or_else(|_| s.content.clone()),
                );
            }
        }
        Ok(Object::Array(arr)) => {
            for item in arr {
                if let Object::Reference(r) = item {
                    if let Ok(Object::Stream(s)) = doc.get_object(*r) {
                        out.extend_from_slice(
                            &s.decompressed_content()
                                .unwrap_or_else(|_| s.content.clone()),
                        );
                        out.push(b'\n');
                    }
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

fn collect_resources(doc: &Document, page_id: lopdf::ObjectId) -> Object {
    let Ok(Object::Dictionary(d)) = doc.get_object(page_id) else {
        return Object::Dictionary(lopdf::Dictionary::new());
    };
    match d.get(b"Resources") {
        Ok(Object::Dictionary(dd)) => Object::Dictionary(dd.clone()),
        Ok(Object::Reference(r)) => match doc.get_object(*r) {
            Ok(Object::Dictionary(dd)) => Object::Dictionary(dd.clone()),
            _ => Object::Dictionary(lopdf::Dictionary::new()),
        },
        _ => Object::Dictionary(lopdf::Dictionary::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn sample_pdf(n: u32) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let contents = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Contents" => contents,
            });
            kids.push(Object::Reference(page_id));
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => n as i64,
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
    fn two_up_halves_pages() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf(4)).unwrap();
        let n = nup(
            &input,
            &output,
            NupOpts {
                n: 2,
                gap: 6.0,
                margin: 18.0,
            },
        )
        .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn four_up_quarters_pages() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf(8)).unwrap();
        let n = nup(
            &input,
            &output,
            NupOpts {
                n: 4,
                gap: 6.0,
                margin: 18.0,
            },
        )
        .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn invalid_n_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf(2)).unwrap();
        let r = nup(
            &input,
            &output,
            NupOpts {
                n: 3,
                gap: 6.0,
                margin: 18.0,
            },
        );
        assert!(r.is_err());
    }
}
