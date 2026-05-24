//! Bind — offline PDF → EPUB 3.
//!
//! Re-uses the v3.14.0 Reflow pipeline's `extract` + `layout` passes to turn
//! a PDF into a `Vec<Block>`, then wraps the result in an EPUB 3 ZIP
//! envelope: `mimetype` (Stored), `META-INF/container.xml`, and the
//! `OEBPS/` directory containing `content.opf`, `nav.xhtml`, `style.css`,
//! and one `chapter-N.xhtml` file per chapter (split on H1 by default).

pub mod errors;
pub mod package;
pub mod split;
pub mod types;
pub mod writer;

pub use errors::EpubError;
pub use types::{EpubOptions, EpubReport};

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::pdf::epub::package::{
    container_xml, content_opf, default_stylesheet, nav_xhtml, OpfMetadata,
};
use crate::pdf::epub::split::{split_into_chapters, Chapter};
use crate::pdf::epub::writer::chapter_xhtml;
use crate::pdf::reflow::types::{Block, ReflowOptions};

/// Write the full EPUB 3 ZIP to `output`. Returns total bytes written.
///
/// EPUB has a strict format requirement: the **first** ZIP entry MUST be
/// `mimetype` and it MUST be stored uncompressed with no extra fields.
/// We honour that here.
pub fn write_epub_zip(
    output: &Path,
    meta: &OpfMetadata,
    chapters: &[Chapter],
) -> Result<u64, EpubError> {
    let file = File::create(output)?;
    let mut zip = ZipWriter::new(file);

    // 1. mimetype — MUST be first, MUST be Stored.
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("mimetype", stored)?;
    zip.write_all(b"application/epub+zip")?;

    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // 2. META-INF/container.xml
    zip.start_file("META-INF/container.xml", deflated)?;
    zip.write_all(container_xml().as_bytes())?;

    // 3. One xhtml per chapter (+ build manifest IDs in parallel).
    let chapter_ids: Vec<String> = (1..=chapters.len())
        .map(|i| format!("chapter-{i}"))
        .collect();
    for (id, ch) in chapter_ids.iter().zip(chapters.iter()) {
        zip.start_file(format!("OEBPS/{id}.xhtml"), deflated)?;
        zip.write_all(chapter_xhtml(ch).as_bytes())?;
    }

    // 4. content.opf
    zip.start_file("OEBPS/content.opf", deflated)?;
    zip.write_all(content_opf(meta, &chapter_ids).as_bytes())?;

    // 5. nav.xhtml
    let nav_entries: Vec<(String, String)> = chapter_ids
        .iter()
        .zip(chapters.iter())
        .map(|(id, ch)| (id.clone(), ch.title.clone()))
        .collect();
    zip.start_file("OEBPS/nav.xhtml", deflated)?;
    zip.write_all(nav_xhtml(&nav_entries).as_bytes())?;

    // 6. style.css
    zip.start_file("OEBPS/style.css", deflated)?;
    zip.write_all(default_stylesheet().as_bytes())?;

    let final_file = zip.finish()?;
    let len = final_file.metadata()?.len();
    Ok(len)
}

/// Convert a PDF to an EPUB 3 file. Top-level entry point.
pub fn convert_to_epub(
    input: &Path,
    output: &Path,
    opts: &EpubOptions,
) -> Result<EpubReport, EpubError> {
    let start = std::time::Instant::now();

    if !input.exists() {
        return Err(EpubError::InvalidPath(format!(
            "input does not exist: {}",
            input.display()
        )));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(EpubError::InvalidPath(format!(
                "output directory does not exist: {}",
                parent.display()
            )));
        }
    }

    // Extract + layout via the existing Reflow pipeline.
    let reflow_opts = ReflowOptions {
        detect_tables: opts.detect_tables,
        detect_lists: opts.detect_lists,
        heading_size_ratio: opts.heading_size_ratio,
        preserve_page_breaks: false,
        locale: opts.language.clone(),
    };
    let doc = lopdf::Document::load(input).map_err(|e| EpubError::Reflow(e.to_string()))?;
    let page_count = doc.get_pages().len() as u32;
    let runs = crate::pdf::reflow::extract::extract_text_runs(&doc)
        .map_err(|e| EpubError::Reflow(e.to_string()))?;
    if runs.is_empty() {
        return Err(EpubError::EmptyDocument);
    }
    let blocks = crate::pdf::reflow::layout::reconstruct_blocks(&runs, &reflow_opts);

    let chapters = split_into_chapters(&blocks, opts.split_on_h1);

    // Derive Dublin Core metadata.
    let title = opts.title.clone().unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Document")
            .to_string()
    });
    let author = opts.author.clone().unwrap_or_else(|| "Unknown".to_string());
    let uuid = make_uuid_v4();
    let meta = OpfMetadata {
        title,
        author,
        language: opts.language.clone(),
        uuid,
    };

    let bytes = write_epub_zip(output, &meta, &chapters)?;

    // Tally block types for the report.
    let mut report = EpubReport {
        pages: page_count,
        chapters: chapters.len() as u32,
        headings: 0,
        paragraphs: 0,
        list_items: 0,
        table_rows: 0,
        bytes_written: bytes,
        duration_ms: start.elapsed().as_millis() as u64,
    };
    for blk in &blocks {
        match blk {
            Block::Heading { .. } => report.headings += 1,
            Block::Body { .. } => report.paragraphs += 1,
            Block::ListItem { .. } => report.list_items += 1,
            Block::TableRow { .. } => report.table_rows += 1,
        }
    }
    Ok(report)
}

/// Produce a UUID-v4-shaped string without pulling in the `uuid` crate.
///
/// Uses 16 bytes from the system clock mixed with a tiny multiplicative
/// hash. EPUB readers only require `dc:identifier` to be unique per book
/// — any 128-bit blob in UUID form satisfies that.
fn make_uuid_v4() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut bytes = [0u8; 16];
    let mut state = (nanos as u64) ^ 0x9E37_79B9_7F4A_7C15;
    for b in bytes.iter_mut() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (state >> 56) as u8;
    }
    // Set version (4) + variant (RFC 4122) bits.
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::epub::package::OpfMetadata;
    use crate::pdf::epub::split::Chapter;
    use crate::pdf::reflow::types::Block;
    use std::io::Read;

    #[test]
    fn writes_valid_epub_zip() {
        let chapters = vec![Chapter {
            title: "Hello".into(),
            blocks: vec![
                Block::Heading {
                    level: 1,
                    text: "Hello".into(),
                },
                Block::Body {
                    text: "World".into(),
                },
            ],
        }];
        let meta = OpfMetadata {
            title: "Test".into(),
            author: "Cake".into(),
            language: "en".into(),
            uuid: "11111111-2222-3333-4444-555555555555".into(),
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let bytes = write_epub_zip(tmp.path(), &meta, &chapters).unwrap();
        assert!(bytes > 100);

        let f = std::fs::File::open(tmp.path()).unwrap();
        let mut zf = zip::ZipArchive::new(f).unwrap();
        let names: Vec<String> = (0..zf.len())
            .map(|i| zf.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names[0], "mimetype");
        {
            let mut mt = zf.by_name("mimetype").unwrap();
            assert_eq!(mt.compression(), zip::CompressionMethod::Stored);
            let mut s = String::new();
            mt.read_to_string(&mut s).unwrap();
            assert_eq!(s, "application/epub+zip");
        }
        assert!(names.iter().any(|n| n == "META-INF/container.xml"));
        assert!(names.iter().any(|n| n == "OEBPS/content.opf"));
        assert!(names.iter().any(|n| n == "OEBPS/nav.xhtml"));
        assert!(names.iter().any(|n| n == "OEBPS/style.css"));
        assert!(names.iter().any(|n| n == "OEBPS/chapter-1.xhtml"));
    }

    #[test]
    fn uuid_v4_is_well_formed() {
        let u = make_uuid_v4();
        let parts: Vec<&str> = u.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        // Version nibble = 4.
        assert_eq!(&parts[2][..1], "4");
        // Variant nibble in [8,9,a,b].
        let v = parts[3].chars().next().unwrap();
        assert!(matches!(v, '8' | '9' | 'a' | 'b'));
    }

    #[test]
    fn convert_to_epub_missing_input_errors() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = convert_to_epub(
            Path::new("/does/not/exist.pdf"),
            tmp.path(),
            &EpubOptions::default(),
        );
        assert!(matches!(result, Err(EpubError::InvalidPath(_))));
    }
}
