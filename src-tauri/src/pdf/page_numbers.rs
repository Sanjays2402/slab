// Stamp page numbers onto every page (or a chosen range).
//
// Like the watermark op, we don't touch existing content streams — we append
// a small overlay stream that writes the page number in Helvetica at the
// requested position. We support 9 positions (3x3 grid), a starting number
// offset, an optional template like "Page {n} of {total}", and a font size.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NumberPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl NumberPosition {
    fn xy(self, w: f32, h: f32, margin: f32) -> (f32, f32) {
        match self {
            NumberPosition::TopLeft => (margin, h - margin),
            NumberPosition::TopCenter => (w / 2.0, h - margin),
            NumberPosition::TopRight => (w - margin, h - margin),
            NumberPosition::BottomLeft => (margin, margin),
            NumberPosition::BottomCenter => (w / 2.0, margin),
            NumberPosition::BottomRight => (w - margin, margin),
        }
    }

    fn align(self) -> Align {
        match self {
            NumberPosition::TopLeft | NumberPosition::BottomLeft => Align::Left,
            NumberPosition::TopCenter | NumberPosition::BottomCenter => Align::Center,
            NumberPosition::TopRight | NumberPosition::BottomRight => Align::Right,
        }
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PageNumbersOpts {
    /// Template — `{n}` and `{total}` get substituted. e.g. `"Page {n} of {total}"`.
    pub template: String,
    pub position: NumberPosition,
    pub font_size: f32,
    /// Starting number (e.g. 1 prints "1, 2, 3…"; useful when first page is a cover).
    pub start_at: u32,
    /// Skip the first N pages (don't number them at all).
    pub skip_first: u32,
    /// 0..=1 gray.
    pub gray: f32,
}

impl Default for PageNumbersOpts {
    fn default() -> Self {
        PageNumbersOpts {
            template: "{n}".into(),
            position: NumberPosition::BottomCenter,
            font_size: 11.0,
            start_at: 1,
            skip_first: 0,
            gray: 0.2,
        }
    }
}

pub fn add_page_numbers(
    input: &Path,
    output: &Path,
    opts: &PageNumbersOpts,
) -> Result<u32, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if opts.template.is_empty() {
        return Err(PdfError::Other("template is empty".into()));
    }
    if !(0.0..=1.0).contains(&opts.gray) {
        return Err(PdfError::Other("gray must be in 0..=1".into()));
    }
    if opts.font_size <= 0.0 {
        return Err(PdfError::Other("font_size must be positive".into()));
    }

    let mut doc = Document::load(input)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    if total == 0 {
        return Ok(0);
    }
    let numbered_total = total.saturating_sub(opts.skip_first);

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let mut applied = 0u32;
    let mut updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();

    // Sorted iteration so labels match physical order.
    let mut entries: Vec<(u32, lopdf::ObjectId)> = page_map.iter().map(|(k, v)| (*k, *v)).collect();
    entries.sort_by_key(|(k, _)| *k);

    for (idx, (_page_num, page_id)) in entries.iter().enumerate() {
        let one_based = (idx as u32) + 1;
        if one_based <= opts.skip_first {
            continue;
        }
        let logical = one_based - opts.skip_first + opts.start_at - 1;
        let label = opts
            .template
            .replace("{n}", &logical.to_string())
            .replace("{total}", &numbered_total.to_string());

        let (w, h) = page_size(&doc, *page_id).unwrap_or((612.0, 792.0));
        let margin = 24.0_f32.max(opts.font_size * 1.4);
        let (x, y) = opts.position.xy(w, h, margin);

        // Crude width estimate for alignment (Helvetica avg ≈ 0.5em).
        let text_width = label.len() as f32 * opts.font_size * 0.5;
        let tx = match opts.position.align() {
            Align::Left => x,
            Align::Center => x - text_width / 2.0,
            Align::Right => x - text_width,
        };

        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new("g", vec![Object::Real(opts.gray)]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["SlabPN".into(), opts.font_size.into()]),
            Operation::new(
                "Td",
                vec![Object::Real(tx), Object::Real(y - opts.font_size * 0.3)],
            ),
            Operation::new("Tj", vec![Object::string_literal(label.as_bytes())]),
            Operation::new("ET", vec![]),
            Operation::new("Q", vec![]),
        ];
        let content = Content { operations: ops };
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        updates.push((*page_id, stream_id));
        applied += 1;
    }

    for (page_id, stream_id) in updates {
        append_with_font(&mut doc, page_id, stream_id, font_id)?;
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.compress();
    doc.save(output)?;
    Ok(applied)
}

fn page_size(doc: &Document, page_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let page = doc.get_object(page_id).ok()?;
    let dict = page.as_dict().ok()?;
    let mb = dict.get(b"MediaBox").ok()?;
    let arr = mb.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let nums: Vec<f32> = arr
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
            Some(Object::Reference(r)) => doc.get_object(r)?.as_dict()?.clone(),
            _ => lopdf::Dictionary::new(),
        };
        let mut font = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => lopdf::Dictionary::new(),
        };
        font.set("SlabPN", Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(font));

        let page = doc.get_object_mut(page_id)?;
        if let Object::Dictionary(dict) = page {
            dict.set("Resources", Object::Dictionary(resources));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn numbers_every_page() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 5);
        let n = add_page_numbers(&src, &dst, &PageNumbersOpts::default()).unwrap();
        assert_eq!(n, 5);
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 5);
    }

    #[test]
    fn respects_skip_first() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 4);
        let opts = PageNumbersOpts {
            skip_first: 1,
            ..Default::default()
        };
        let n = add_page_numbers(&src, &dst, &opts).unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn rejects_empty_template() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        let opts = PageNumbersOpts {
            template: "".into(),
            ..Default::default()
        };
        assert!(add_page_numbers(&src, &dst, &opts).is_err());
    }
}
