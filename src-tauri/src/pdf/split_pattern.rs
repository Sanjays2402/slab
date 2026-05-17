// Split a PDF into chapters by regex match on per-page text, or — if no
// regex is provided — by top-level outline headings.
//
// This is the "chapter splitter" feature: point Slab at a 400-page book and
// it produces one PDF per chapter automatically.

use crate::pdf::extract::extract_text;
use crate::pdf::outline::{read_outline, OutlineNode};
use crate::pdf::split::{split_by_ranges, PageRange};
use crate::pdf::PdfError;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Find the pages on which `pattern` matches the page's extracted text.
/// Returns the 1-indexed page numbers in ascending order.
pub fn find_matching_pages(input: &Path, pattern: &str) -> Result<Vec<u32>, PdfError> {
    if pattern.trim().is_empty() {
        return Err(PdfError::Other("empty regex pattern".into()));
    }
    let re = Regex::new(pattern).map_err(|e| PdfError::Other(format!("invalid regex: {e}")))?;
    let pages = extract_text(input)?;
    let mut hits = Vec::new();
    for (i, text) in pages.iter().enumerate() {
        if re.is_match(text) {
            hits.push((i + 1) as u32);
        }
    }
    Ok(hits)
}

/// Walk the outline tree, return the 1-indexed page numbers of every
/// top-level entry that points to a page destination. Used as a fallback
/// when no regex is supplied to `split_by_pattern`.
pub fn outline_top_level_pages(input: &Path) -> Result<Vec<u32>, PdfError> {
    let nodes = read_outline(input)?;
    let mut pages = Vec::new();
    for n in &nodes {
        if let Some(p) = first_page(n) {
            pages.push(p);
        }
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

fn first_page(node: &OutlineNode) -> Option<u32> {
    if let Some(p) = node.page_index {
        return Some(p + 1);
    }
    for c in &node.children {
        if let Some(p) = first_page(c) {
            return Some(p);
        }
    }
    None
}

/// Build the ranges defined by `chapter_starts`: each entry is the first
/// page of a chapter. Pages before the first start are merged into the
/// first chunk (which keeps cover pages, prefaces, etc. together).
pub fn ranges_from_chapter_starts(
    chapter_starts: &[u32],
    total_pages: u32,
) -> Result<Vec<PageRange>, PdfError> {
    if total_pages == 0 {
        return Err(PdfError::Other("document is empty".into()));
    }
    if chapter_starts.is_empty() {
        return Err(PdfError::Other("no chapter starts found".into()));
    }
    // Defensive copy + dedupe + sort + bounds-check.
    let mut starts: Vec<u32> = chapter_starts.to_vec();
    starts.sort_unstable();
    starts.dedup();
    for &s in &starts {
        if s == 0 || s > total_pages {
            return Err(PdfError::Other(format!(
                "chapter-start page {s} out of range (1..={total_pages})"
            )));
        }
    }
    // Build ranges from the (de-duped, sorted, validated) chapter starts.
    // The first range always begins at page 1, so any pages before the first
    // chapter-start (cover, preface, TOC) get bundled into the first chunk.
    let mut ranges = Vec::with_capacity(starts.len());
    let after_one: Vec<u32> = starts.iter().copied().filter(|&p| p > 1).collect();
    let mut prev = 1u32;
    for s in &after_one {
        if *s > prev {
            ranges.push(PageRange::new(prev, *s - 1)?);
        }
        prev = *s;
    }
    ranges.push(PageRange::new(prev, total_pages)?);
    Ok(ranges)
}

/// Split a PDF by regex match. If `pattern` is `None` or whitespace, falls
/// back to top-level outline entries. Writes one PDF per chapter into
/// `out_dir`, returns the paths.
pub fn split_by_pattern(
    input: &Path,
    pattern: Option<&str>,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let starts = match pattern {
        Some(p) if !p.trim().is_empty() => find_matching_pages(input, p)?,
        _ => outline_top_level_pages(input)?,
    };
    if starts.is_empty() {
        return Err(PdfError::Other(
            "no chapter starts found (regex matched nothing and outline is empty)".into(),
        ));
    }
    let total = crate::pdf::split::page_count(input)?;
    let ranges = ranges_from_chapter_starts(&starts, total)?;
    split_by_ranges(input, &ranges, out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn ranges_three_chapters_starting_mid_doc() {
        // 10-page book, chapters start at pages 3 and 7 (cover/preface = 1-2)
        let ranges = ranges_from_chapter_starts(&[3, 7], 10).unwrap();
        assert_eq!(ranges.len(), 3);
        assert_eq!((ranges[0].start, ranges[0].end), (1, 2));
        assert_eq!((ranges[1].start, ranges[1].end), (3, 6));
        assert_eq!((ranges[2].start, ranges[2].end), (7, 10));
    }

    #[test]
    fn ranges_first_chapter_starts_on_page_1() {
        let ranges = ranges_from_chapter_starts(&[1, 5], 8).unwrap();
        assert_eq!(ranges.len(), 2);
        assert_eq!((ranges[0].start, ranges[0].end), (1, 4));
        assert_eq!((ranges[1].start, ranges[1].end), (5, 8));
    }

    #[test]
    fn ranges_single_chapter() {
        let ranges = ranges_from_chapter_starts(&[1], 5).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].start, ranges[0].end), (1, 5));
    }

    #[test]
    fn ranges_dedupes_and_sorts() {
        let ranges = ranges_from_chapter_starts(&[7, 3, 7, 3], 10).unwrap();
        assert_eq!(ranges.len(), 3);
        assert_eq!((ranges[1].start, ranges[1].end), (3, 6));
        assert_eq!((ranges[2].start, ranges[2].end), (7, 10));
    }

    #[test]
    fn ranges_rejects_out_of_range_start() {
        let err = ranges_from_chapter_starts(&[99], 10).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn ranges_rejects_empty_starts() {
        let err = ranges_from_chapter_starts(&[], 10).unwrap_err();
        assert!(err.to_string().contains("no chapter starts"));
    }

    #[test]
    fn ranges_rejects_zero_total() {
        let err = ranges_from_chapter_starts(&[1], 0).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn find_matching_pages_rejects_empty_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        make_n_page_pdf(&src, 3);
        let err = find_matching_pages(&src, "  ").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn find_matching_pages_rejects_invalid_regex() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        make_n_page_pdf(&src, 3);
        let err = find_matching_pages(&src, "[invalid").unwrap_err();
        assert!(err.to_string().contains("invalid regex"));
    }

    #[test]
    fn find_matching_pages_returns_pages_with_text() {
        // make_n_page_pdf stamps "Slab page {n}" on each page — so a pattern
        // like "Slab page 2" should match exactly one page.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        make_n_page_pdf(&src, 5);
        let hits = find_matching_pages(&src, "Slab page 2").unwrap();
        assert_eq!(hits, vec![2u32]);
    }

    #[test]
    fn split_by_pattern_with_regex_writes_chunks() {
        // 5 pages each labeled "Slab page {n}". Pattern "Slab page [13]\b"
        // matches pages 1 and 3 → 2 chunks: [1..=2], [3..=5].
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("split");
        make_n_page_pdf(&src, 5);
        let paths = split_by_pattern(&src, Some(r"Slab page [13]\b"), &out).unwrap();
        assert_eq!(paths.len(), 2);
        for p in &paths {
            assert!(p.exists(), "missing chunk: {}", p.display());
        }
    }

    #[test]
    fn split_by_pattern_no_match_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let out = tmp.path().join("split");
        make_n_page_pdf(&src, 3);
        let err = split_by_pattern(&src, Some("ZZZZZZ_NOMATCH"), &out).unwrap_err();
        assert!(err.to_string().contains("no chapter starts"));
    }

    #[test]
    fn split_by_pattern_missing_input() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nope.pdf");
        let out = tmp.path().join("split");
        let err = split_by_pattern(&bogus, Some("anything"), &out).unwrap_err();
        assert!(matches!(err, PdfError::InputMissing(_)));
    }
}
