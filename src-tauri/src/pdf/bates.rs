// Bates numbering — stamp sequential identifiers onto every page of a PDF
// (or a batch of PDFs), the legal-industry standard for document
// production in litigation discovery.
//
// A Bates label is just a fixed prefix + zero-padded number, e.g.
//   ACME000001, ACME000002, ACME000003 …
//
// Compared to ordinary page numbers, what matters for Bates is:
//   - the counter is monotonically increasing across the whole *production*,
//     not reset per-document;
//   - zero-padded width is fixed so labels sort lexicographically;
//   - position is conventionally bottom-right;
//   - prefix and starting number are caller-controlled.
//
// We model a single PDF here. The caller (CLI or future batch driver)
// threads `start_at` through multiple files to keep the counter
// monotonic across a production set.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BatesPosition {
    BottomRight,
    BottomLeft,
    BottomCenter,
    TopRight,
    TopLeft,
    TopCenter,
}

impl BatesPosition {
    fn xy(self, w: f32, h: f32, margin: f32) -> (f32, f32) {
        match self {
            BatesPosition::TopLeft => (margin, h - margin),
            BatesPosition::TopCenter => (w / 2.0, h - margin),
            BatesPosition::TopRight => (w - margin, h - margin),
            BatesPosition::BottomLeft => (margin, margin),
            BatesPosition::BottomCenter => (w / 2.0, margin),
            BatesPosition::BottomRight => (w - margin, margin),
        }
    }

    fn align(self) -> Align {
        match self {
            BatesPosition::TopLeft | BatesPosition::BottomLeft => Align::Left,
            BatesPosition::TopCenter | BatesPosition::BottomCenter => Align::Center,
            BatesPosition::TopRight | BatesPosition::BottomRight => Align::Right,
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
pub struct BatesOpts {
    /// Prefix string, e.g. `"ACME"`. Empty string is allowed (you get
    /// just the zero-padded number).
    pub prefix: String,
    /// First Bates number for *this* document. The output reports the
    /// next available number so callers can chain across files.
    pub start_at: u64,
    /// Total digits to pad to. e.g. 6 → `000001`. Range 1..=12.
    pub digits: u8,
    /// Bottom-right is the legal convention; we let callers override.
    pub position: BatesPosition,
    /// Helvetica point size.
    pub font_size: f32,
    /// 0..=1 gray; 0 = black, 1 = white.
    pub gray: f32,
}

impl Default for BatesOpts {
    fn default() -> Self {
        BatesOpts {
            prefix: String::new(),
            start_at: 1,
            digits: 6,
            position: BatesPosition::BottomRight,
            font_size: 10.0,
            gray: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BatesReport {
    pub pages_stamped: u32,
    pub first_label: String,
    pub last_label: String,
    /// Next Bates number to use for the *following* document in a
    /// chained production. Callers should pass this in as `start_at`
    /// for the next file.
    pub next_start: u64,
}

/// Format a single Bates label. Pure function; reused by the batch driver
/// and the load-file writer. If `n` needs more digits than `digits`, we
/// widen rather than truncate (silent truncation is a discovery hazard).
pub fn bates_label_for(n: u64, prefix: &str, digits: u8) -> String {
    let body = format!("{:0width$}", n, width = digits as usize);
    format!("{}{}", prefix, body)
}

pub fn apply_bates(input: &Path, output: &Path, opts: &BatesOpts) -> Result<BatesReport, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    if !(1..=12).contains(&opts.digits) {
        return Err(PdfError::Other("digits must be in 1..=12".into()));
    }
    if opts.font_size <= 0.0 {
        return Err(PdfError::Other("font_size must be positive".into()));
    }
    if !(0.0..=1.0).contains(&opts.gray) {
        return Err(PdfError::Other("gray must be in 0..=1".into()));
    }
    if opts.start_at == 0 {
        return Err(PdfError::Other("start_at must be >= 1".into()));
    }

    let mut doc = Document::load(input)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    if total == 0 {
        return Ok(BatesReport {
            pages_stamped: 0,
            first_label: String::new(),
            last_label: String::new(),
            next_start: opts.start_at,
        });
    }

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let mut entries: Vec<(u32, lopdf::ObjectId)> = page_map.iter().map(|(k, v)| (*k, *v)).collect();
    entries.sort_by_key(|(k, _)| *k);

    let mut updates: Vec<(lopdf::ObjectId, lopdf::ObjectId)> = Vec::new();
    let mut first_label: Option<String> = None;
    let mut last_label: Option<String> = None;

    for (idx, (_page_num, page_id)) in entries.iter().enumerate() {
        let bates_num = opts.start_at + idx as u64;
        let label = bates_label_for(bates_num, &opts.prefix, opts.digits);
        if first_label.is_none() {
            first_label = Some(label.clone());
        }
        last_label = Some(label.clone());

        let (w, h) = page_size(&doc, *page_id).unwrap_or((612.0, 792.0));
        let margin = 24.0_f32.max(opts.font_size * 1.4);
        let (x, y) = opts.position.xy(w, h, margin);
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
            Operation::new("Tf", vec!["SlabBates".into(), opts.font_size.into()]),
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

    Ok(BatesReport {
        pages_stamped: total,
        first_label: first_label.unwrap_or_default(),
        last_label: last_label.unwrap_or_default(),
        next_start: opts.start_at + total as u64,
    })
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
        font.set("SlabBates", Object::Reference(font_id));
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
    fn stamps_all_pages_with_padded_label() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 3);

        let report = apply_bates(
            &src,
            &dst,
            &BatesOpts {
                prefix: "ACME".into(),
                start_at: 1,
                digits: 6,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(report.pages_stamped, 3);
        assert_eq!(report.first_label, "ACME000001");
        assert_eq!(report.last_label, "ACME000003");
        assert_eq!(report.next_start, 4);
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 3);
    }

    #[test]
    fn empty_prefix_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 2);

        let report = apply_bates(
            &src,
            &dst,
            &BatesOpts {
                prefix: String::new(),
                start_at: 42,
                digits: 4,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(report.first_label, "0042");
        assert_eq!(report.last_label, "0043");
        assert_eq!(report.next_start, 44);
    }

    #[test]
    fn rejects_zero_start() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        let err = apply_bates(
            &src,
            &dst,
            &BatesOpts {
                start_at: 0,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
        assert!(err.to_string().contains("start_at"));
    }

    #[test]
    fn rejects_oversized_digits() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);

        let err = apply_bates(
            &src,
            &dst,
            &BatesOpts {
                digits: 13,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, PdfError::Other(_)));
    }

    #[test]
    fn missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let dst = tmp.path().join("out.pdf");
        let err = apply_bates(&bogus, &dst, &BatesOpts::default()).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }

    #[test]
    fn bates_label_basic() {
        assert_eq!(bates_label_for(1, "ACME", 6), "ACME000001");
        assert_eq!(bates_label_for(42, "", 4), "0042");
        assert_eq!(bates_label_for(999_999, "X", 6), "X999999");
    }

    #[test]
    fn bates_label_overflow_widens() {
        // Documented behavior: if n needs more digits than `digits`,
        // we widen (never truncate). Truncation would silently corrupt a
        // production set — unacceptable.
        assert_eq!(bates_label_for(1_000_000, "ACME", 6), "ACME1000000");
        assert_eq!(bates_label_for(12_345, "", 3), "12345");
    }
}
