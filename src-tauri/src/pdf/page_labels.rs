// Set PDF Page Labels — visible page numbers in the viewer's UI.
//
// PDF spec §8.3.1: the catalog's /PageLabels entry is a number tree whose
// values are dictionaries with /S (style), /P (prefix), and /St (start).
// Style codes:
//   D       Decimal arabic (1, 2, 3)
//   R       Uppercase roman (I, II, III)
//   r       Lowercase roman (i, ii, iii)
//   A       Uppercase letters (A, B, C…AA, BB)
//   a       Lowercase letters (a, b, c…aa, bb)
//   omit    Prefix only (no numeric part)
//
// We expose a `ranges` API where each range owns a starting 0-based page
// index. The PDF spec puts them in ascending order in /Nums.

use crate::pdf::PdfError;
use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabelRange {
    /// 0-based page index where this range starts.
    pub start_page: u32,
    /// One of "D" | "R" | "r" | "A" | "a" | "" (prefix only).
    pub style: String,
    /// Optional prefix prepended to each generated label, e.g. "Ch-".
    pub prefix: String,
    /// Numeric value of the first page in this range. Defaults to 1.
    pub start: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PageLabelsOpts {
    pub ranges: Vec<LabelRange>,
}

pub fn apply(input: &Path, output: &Path, opts: PageLabelsOpts) -> Result<u32, PdfError> {
    if opts.ranges.is_empty() {
        return Err(PdfError::Other("No label ranges supplied.".into()));
    }
    let mut doc = Document::load(input)?;

    // Sort ranges by start_page; PDF spec requires ascending order in /Nums.
    let mut ranges = opts.ranges.clone();
    ranges.sort_by_key(|r| r.start_page);

    // Build /Nums array: [key value key value ...].
    let mut nums: Vec<Object> = Vec::with_capacity(ranges.len() * 2);
    for r in &ranges {
        nums.push(Object::Integer(r.start_page as i64));
        let mut entry = dictionary! {};
        match r.style.as_str() {
            "D" => entry.set("S", Object::Name(b"D".to_vec())),
            "R" => entry.set("S", Object::Name(b"R".to_vec())),
            "r" => entry.set("S", Object::Name(b"r".to_vec())),
            "A" => entry.set("S", Object::Name(b"A".to_vec())),
            "a" => entry.set("S", Object::Name(b"a".to_vec())),
            "" => {} // prefix-only: omit /S
            other => {
                return Err(PdfError::Other(format!(
                    "Invalid page label style: {other:?}"
                )))
            }
        }
        if !r.prefix.is_empty() {
            entry.set("P", Object::string_literal(r.prefix.clone()));
        }
        if r.start != 1 {
            entry.set("St", Object::Integer(r.start as i64));
        }
        nums.push(Object::Dictionary(entry));
    }

    let page_labels = dictionary! { "Nums" => nums };

    // Find catalog and set /PageLabels.
    let root_id = match doc.trailer.get(b"Root")? {
        Object::Reference(r) => *r,
        _ => return Err(PdfError::Other("Trailer /Root not a reference".into())),
    };
    let catalog = doc.get_object_mut(root_id)?;
    if let Object::Dictionary(dict) = catalog {
        dict.set("PageLabels", Object::Dictionary(page_labels));
    } else {
        return Err(PdfError::Other("Catalog is not a dictionary".into()));
    }

    doc.compress();
    doc.save(output)?;
    Ok(ranges.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn sample_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..3 {
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
                "Count" => 3,
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
    fn label_ranges_applied() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        let n = apply(
            &input,
            &output,
            PageLabelsOpts {
                ranges: vec![
                    LabelRange {
                        start_page: 0,
                        style: "r".into(),
                        prefix: "".into(),
                        start: 1,
                    },
                    LabelRange {
                        start_page: 2,
                        style: "D".into(),
                        prefix: "".into(),
                        start: 1,
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(n, 2);

        let reloaded = Document::load(&output).unwrap();
        let root_id = match reloaded.trailer.get(b"Root").unwrap() {
            Object::Reference(r) => *r,
            _ => panic!("trailer root not ref"),
        };
        let cat = reloaded.get_object(root_id).unwrap().as_dict().unwrap();
        let pl = cat.get(b"PageLabels").unwrap().as_dict().unwrap();
        let nums = pl.get(b"Nums").unwrap().as_array().unwrap();
        assert_eq!(nums.len(), 4); // 2 ranges × 2 entries each
    }

    #[test]
    fn empty_ranges_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        let r = apply(&input, &output, PageLabelsOpts { ranges: vec![] });
        assert!(r.is_err());
    }

    #[test]
    fn invalid_style_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf()).unwrap();
        let r = apply(
            &input,
            &output,
            PageLabelsOpts {
                ranges: vec![LabelRange {
                    start_page: 0,
                    style: "Z".into(),
                    prefix: "".into(),
                    start: 1,
                }],
            },
        );
        assert!(r.is_err());
    }
}
