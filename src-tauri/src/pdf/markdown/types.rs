// Types for the Markdown (PDF → MD + HTML) pipeline.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarkdownFlavour {
    CommonMark,
    Gfm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownOptions {
    pub detect_tables: bool,
    pub detect_lists: bool,
    pub heading_size_ratio: f32,
    pub preserve_page_breaks: bool,
    /// CommonMark or GitHub Flavored Markdown. Default: GFM (tables).
    pub flavour: MarkdownFlavour,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            detect_tables: true,
            detect_lists: true,
            heading_size_ratio: 1.25,
            preserve_page_breaks: false,
            flavour: MarkdownFlavour::Gfm,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownReport {
    pub pages: u32,
    pub headings: u32,
    pub paragraphs: u32,
    pub list_items: u32,
    pub tables: u32,
    pub bytes_written: u64,
    pub duration_ms: u64,
}

impl MarkdownReport {
    pub fn empty() -> Self {
        Self {
            pages: 0,
            headings: 0,
            paragraphs: 0,
            list_items: 0,
            tables: 0,
            bytes_written: 0,
            duration_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlOptions {
    pub detect_tables: bool,
    pub detect_lists: bool,
    pub heading_size_ratio: f32,
    /// Use <article>/<section>/<header>/<nav> semantics. Default: true.
    pub semantic_tags: bool,
    /// Embed a small <style> block for clean rendering. Default: true.
    pub embed_css: bool,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        Self {
            detect_tables: true,
            detect_lists: true,
            heading_size_ratio: 1.25,
            semantic_tags: true,
            embed_css: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlReport {
    pub pages: u32,
    pub headings: u32,
    pub paragraphs: u32,
    pub list_items: u32,
    pub tables: u32,
    pub bytes_written: u64,
    pub duration_ms: u64,
}

impl HtmlReport {
    pub fn empty() -> Self {
        Self {
            pages: 0,
            headings: 0,
            paragraphs: 0,
            list_items: 0,
            tables: 0,
            bytes_written: 0,
            duration_ms: 0,
        }
    }
}
