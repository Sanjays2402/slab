// Headers & Footers — adds a text band to the top and/or bottom of every
// (or chosen) page. Supports placeholders: {n}, {total}, {date}, {filename}.
//
// Implementation note: this is essentially page_numbers.rs with two text
// fields (header + footer) and a slightly richer placeholder set. We share
// the overlay-stream + font-resource pattern so the on-disk overhead per
// page stays tiny (one shared font, one stream ref per page).

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HFAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HFOpts {
    /// Optional header text. None = no header.
    pub header: Option<String>,
    pub header_align: HFAlign,
    /// Optional footer text. None = no footer.
    pub footer: Option<String>,
    pub footer_align: HFAlign,
    pub font_size: f32,
    pub margin: f32,
    /// 0..=1 gray. 0 = black.
    pub gray: f32,
    /// Filename for the {filename} placeholder.
    pub filename: String,
    /// ISO date (YYYY-MM-DD) for the {date} placeholder. Empty disables.
    pub date: String,
    /// Page selection (1-based). Empty = every page.
    pub pages: Vec<u32>,
}

pub fn apply(input: &Path, output: &Path, opts: HFOpts) -> Result<u32, PdfError> {
    if opts.header.is_none() && opts.footer.is_none() {
        return Err(PdfError::Other("Provide a header or a footer.".into()));
    }
    let mut doc = Document::load(input)?;
    let page_pairs: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let total = page_pairs.len() as u32;

    // Shared Helvetica font object — one for all pages.
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let mut updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();
    let mut applied = 0u32;

    for (n, page_id) in &page_pairs {
        if !opts.pages.is_empty() && !opts.pages.contains(n) {
            continue;
        }
        let Some((w, h)) = media_box(&doc, *page_id) else {
            continue;
        };

        let mut ops: Vec<Operation> = Vec::new();
        ops.push(Operation::new("q", vec![]));
        ops.push(Operation::new(
            "g",
            vec![Object::Real(opts.gray.clamp(0.0, 1.0))],
        ));

        if let Some(template) = &opts.header {
            let txt = render(template, *n, total, &opts.filename, &opts.date);
            push_text(
                &mut ops,
                &txt,
                opts.font_size,
                w,
                h - opts.margin,
                opts.header_align,
            );
        }
        if let Some(template) = &opts.footer {
            let txt = render(template, *n, total, &opts.filename, &opts.date);
            push_text(
                &mut ops,
                &txt,
                opts.font_size,
                w,
                opts.margin,
                opts.footer_align,
            );
        }
        ops.push(Operation::new("Q", vec![]));

        let content = Content { operations: ops };
        let body = content
            .encode()
            .map_err(|e| PdfError::Other(format!("Failed to encode H/F stream: {e}")))?;
        let stream_id = doc.add_object(Stream::new(dictionary! {}, body));

        updates.push((*page_id, stream_id));
        applied += 1;
    }

    for (page_id, stream_id) in updates {
        append_with_font(&mut doc, page_id, stream_id, font_id)?;
    }

    doc.compress();
    doc.save(output)?;
    Ok(applied)
}

fn push_text(ops: &mut Vec<Operation>, txt: &str, size: f32, width: f32, y: f32, align: HFAlign) {
    // Approx glyph advance for Helvetica: 0.5 * size per char. Good enough
    // for alignment; users can nudge margin if they want.
    let advance = (txt.len() as f32) * size * 0.5;
    let x = match align {
        HFAlign::Left => 36.0_f32,
        HFAlign::Center => (width / 2.0) - (advance / 2.0),
        HFAlign::Right => width - 36.0 - advance,
    };
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new("Tf", vec!["SlabHFF".into(), size.into()]));
    ops.push(Operation::new("Td", vec![Object::Real(x), Object::Real(y)]));
    ops.push(Operation::new("Tj", vec![Object::string_literal(txt)]));
    ops.push(Operation::new("ET", vec![]));
}

fn render(template: &str, n: u32, total: u32, filename: &str, date: &str) -> String {
    template
        .replace("{n}", &n.to_string())
        .replace("{total}", &total.to_string())
        .replace("{filename}", filename)
        .replace("{date}", date)
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

fn append_with_font(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    new_stream_id: lopdf::ObjectId,
    font_id: lopdf::ObjectId,
) -> Result<(), PdfError> {
    let page = doc.get_object_mut(page_id)?;
    if let Object::Dictionary(dict) = page {
        let new_contents = match dict.get(b"Contents") {
            Ok(Object::Reference(r)) => {
                vec![Object::Reference(*r), Object::Reference(new_stream_id)]
            }
            Ok(Object::Array(arr)) => {
                let mut v = arr.clone();
                v.push(Object::Reference(new_stream_id));
                v
            }
            _ => vec![Object::Reference(new_stream_id)],
        };
        dict.set("Contents", new_contents);

        let resources_value = dict.get(b"Resources").ok().cloned();
        let mut resources = match resources_value {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(r)) => match doc.get_object(r) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            },
            _ => lopdf::Dictionary::new(),
        };
        let mut font_dict = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        font_dict.set("SlabHFF", Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(font_dict));

        if let Some(p) = doc.get_object_mut(page_id).ok().and_then(|p| {
            if let Object::Dictionary(d) = p {
                Some(d)
            } else {
                None
            }
        }) {
            p.set("Resources", Object::Dictionary(resources));
        }
    }
    Ok(())
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
    fn header_only_applied() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf(2)).unwrap();
        let n = apply(
            &input,
            &output,
            HFOpts {
                header: Some("Slab — {filename}".to_string()),
                header_align: HFAlign::Center,
                footer: None,
                footer_align: HFAlign::Center,
                font_size: 10.0,
                margin: 24.0,
                gray: 0.2,
                filename: "report.pdf".into(),
                date: "2026-05-15".into(),
                pages: vec![],
            },
        )
        .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn empty_inputs_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf(1)).unwrap();
        let r = apply(
            &input,
            &output,
            HFOpts {
                header: None,
                header_align: HFAlign::Center,
                footer: None,
                footer_align: HFAlign::Center,
                font_size: 10.0,
                margin: 24.0,
                gray: 0.0,
                filename: "x".into(),
                date: "".into(),
                pages: vec![],
            },
        );
        assert!(r.is_err());
    }
}
