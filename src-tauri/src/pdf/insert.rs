// Insert pages — splice another PDF (or N blank pages) into an existing
// document at a chosen 1-based position.
//
// The blank-page path is a one-shot dict insert. The "insert another PDF"
// path uses lopdf's renumbering helper to merge two documents safely without
// object-id collisions, then surgically grafts the donor's Page nodes into
// the host's Pages tree at the requested index.

use crate::pdf::PdfError;
use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertSource {
    /// Insert N blank pages of the given size (points).
    Blank { count: u32, width: f32, height: f32 },
    /// Splice every page of another PDF on disk.
    Pdf { path: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InsertOpts {
    /// 1-based position. `0` or `1` puts the new pages at the start;
    /// `count+1` or larger appends at the end.
    pub at: u32,
    pub source: InsertSource,
}

/// Insert pages, return the new total page count.
pub fn insert(input: &Path, output: &Path, opts: InsertOpts) -> Result<u32, PdfError> {
    let mut host = Document::load(input)?;

    let donor_page_ids: Vec<lopdf::ObjectId> = match &opts.source {
        InsertSource::Blank {
            count,
            width,
            height,
        } => {
            if *count == 0 {
                return Err(PdfError::Other("Insert count is zero.".into()));
            }
            blank_pages(&mut host, *count, *width, *height)?
        }
        InsertSource::Pdf { path } => {
            let donor_path = Path::new(path);
            if !donor_path.exists() {
                return Err(PdfError::InputMissing(path.clone()));
            }
            splice_in(&mut host, donor_path)?
        }
    };

    // Locate Pages root.
    let catalog_id = host.trailer.get(b"Root")?.as_reference()?;
    let pages_root_id = host
        .get_object(catalog_id)?
        .as_dict()?
        .get(b"Pages")?
        .as_reference()?;

    // Insert donor IDs into the Kids array at the desired index.
    let existing: Vec<lopdf::ObjectId> = host.get_pages().into_values().collect();
    let idx = (opts.at.saturating_sub(1) as usize).min(existing.len());

    let mut new_kids: Vec<Object> = Vec::with_capacity(existing.len() + donor_page_ids.len());
    for (i, page_id) in existing.iter().enumerate() {
        if i == idx {
            for d in &donor_page_ids {
                new_kids.push(Object::Reference(*d));
            }
        }
        new_kids.push(Object::Reference(*page_id));
    }
    if idx >= existing.len() {
        for d in &donor_page_ids {
            new_kids.push(Object::Reference(*d));
        }
    }

    // Rewrite Pages dict + reparent each donor page to point at the host root.
    let new_count = new_kids.len() as i64;
    if let Object::Dictionary(d) = host.get_object_mut(pages_root_id)? {
        d.set("Kids", Object::Array(new_kids));
        d.set("Count", new_count);
    }
    for page_id in &donor_page_ids {
        if let Object::Dictionary(d) = host.get_object_mut(*page_id)? {
            d.set("Parent", Object::Reference(pages_root_id));
        }
    }

    host.compress();
    host.save(output)?;
    Ok(new_count as u32)
}

fn blank_pages(
    doc: &mut Document,
    count: u32,
    width: f32,
    height: f32,
) -> Result<Vec<lopdf::ObjectId>, PdfError> {
    // Locate Pages root so each blank page can reference it as Parent.
    let catalog_id = doc.trailer.get(b"Root")?.as_reference()?;
    let pages_root_id = doc
        .get_object(catalog_id)?
        .as_dict()?
        .get(b"Pages")?
        .as_reference()?;

    let mut ids = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let contents = doc.add_object(lopdf::Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_root_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(width), Object::Real(height)],
            "Contents" => contents,
            "Resources" => dictionary! {},
        });
        ids.push(page_id);
    }
    Ok(ids)
}

fn splice_in(host: &mut Document, donor_path: &Path) -> Result<Vec<lopdf::ObjectId>, PdfError> {
    let mut donor = Document::load(donor_path)?;

    // Shift donor's object IDs above the host's max so nothing collides.
    let max_id = host.objects.keys().map(|k| k.0).max().unwrap_or(0);
    donor.renumber_objects_with(max_id + 1);

    // Collect donor's page IDs (in order) before we drain its object map.
    let donor_page_ids: Vec<lopdf::ObjectId> = donor.get_pages().into_values().collect();
    if donor_page_ids.is_empty() {
        return Err(PdfError::Other("Donor PDF has no pages.".into()));
    }

    // Move every object from donor into host.
    let donor_objects: BTreeMap<lopdf::ObjectId, Object> = std::mem::take(&mut donor.objects);
    for (id, obj) in donor_objects {
        host.objects.insert(id, obj);
    }

    Ok(donor_page_ids)
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
    fn insert_blank_at_start() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, sample_pdf(3)).unwrap();

        let total = insert(
            &input,
            &output,
            InsertOpts {
                at: 1,
                source: InsertSource::Blank {
                    count: 2,
                    width: 595.0,
                    height: 842.0,
                },
            },
        )
        .unwrap();
        assert_eq!(total, 5);
    }

    #[test]
    fn insert_pdf_at_end() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.pdf");
        let out = dir.path().join("out.pdf");
        std::fs::write(&a, sample_pdf(2)).unwrap();
        std::fs::write(&b, sample_pdf(3)).unwrap();

        let total = insert(
            &a,
            &out,
            InsertOpts {
                at: 99,
                source: InsertSource::Pdf {
                    path: b.to_string_lossy().to_string(),
                },
            },
        )
        .unwrap();
        assert_eq!(total, 5);
    }
}
