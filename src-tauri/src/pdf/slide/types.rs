// Types for the Slide (PDF → PPTX) pipeline.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideOptions {
    /// If true, extract `/Text` annotation contents as speaker notes.
    pub include_speaker_notes: bool,
    /// If true, detect the largest top-of-page text run as a slide title.
    pub detect_titles: bool,
    /// If true, embed page-rendered images alongside text (Phase 2, default false).
    pub embed_page_images: bool,
}

impl Default for SlideOptions {
    fn default() -> Self {
        Self {
            include_speaker_notes: true,
            detect_titles: true,
            embed_page_images: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideReport {
    pub slides_emitted: u32,
    pub titles_detected: u32,
    pub notes_attached: u32,
    pub elapsed_ms: u64,
}

/// Internal per-slide content fed to the OOXML writer.
#[derive(Debug, Clone)]
pub(crate) struct SlideContent {
    pub title: Option<String>,
    pub body_bullets: Vec<String>,
    pub notes: Option<String>,
    pub width_pt: f32,
    pub height_pt: f32,
}
