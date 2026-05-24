use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubOptions {
    /// Detect tables via column-x clustering. Default: true.
    pub detect_tables: bool,
    /// Detect bullet/numbered lists. Default: true.
    pub detect_lists: bool,
    /// Start a new chapter on every H1. If false, one chapter per source
    /// PDF page. Default: true.
    pub split_on_h1: bool,
    /// Heading size ratio (matches ReflowOptions). Default 1.25.
    pub heading_size_ratio: f32,
    /// EPUB metadata: dc:language. Default "en".
    pub language: String,
    /// EPUB metadata: dc:title. If None, derived from PDF /Info or filename.
    pub title: Option<String>,
    /// EPUB metadata: dc:creator. If None, derived from PDF /Info or "Unknown".
    pub author: Option<String>,
}

impl Default for EpubOptions {
    fn default() -> Self {
        Self {
            detect_tables: true,
            detect_lists: true,
            split_on_h1: true,
            heading_size_ratio: 1.25,
            language: "en".to_string(),
            title: None,
            author: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubReport {
    pub pages: u32,
    pub chapters: u32,
    pub headings: u32,
    pub paragraphs: u32,
    pub list_items: u32,
    pub table_rows: u32,
    pub bytes_written: u64,
    pub duration_ms: u64,
}

impl EpubReport {
    pub fn empty() -> Self {
        Self {
            pages: 0,
            chapters: 0,
            headings: 0,
            paragraphs: 0,
            list_items: 0,
            table_rows: 0,
            bytes_written: 0,
            duration_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epub_options_default_sane() {
        let o = EpubOptions::default();
        assert!(o.detect_tables);
        assert!(o.detect_lists);
        assert!(o.split_on_h1);
        assert_eq!(o.heading_size_ratio, 1.25);
        assert_eq!(o.language, "en");
        assert!(o.title.is_none());
        assert!(o.author.is_none());
    }

    #[test]
    fn epub_report_empty_zeroed() {
        let r = EpubReport::empty();
        assert_eq!(r.pages, 0);
        assert_eq!(r.chapters, 0);
        assert_eq!(r.bytes_written, 0);
    }
}
