//! Slide — offline PDF → PowerPoint (.pptx) conversion.
//!
//! Pipeline mirrors Reflow:
//!   PDF → text runs (per page) → cluster into `SlideContent` → OOXML PresentationML zip.

pub mod errors;
pub mod extract;
pub mod layout;
pub mod pptx;
pub mod types;

pub use errors::SlideError;
pub use types::{SlideOptions, SlideReport};

use std::path::Path;
use std::time::Instant;

/// Convert a PDF at `input` to a `.pptx` at `output`.
pub fn convert_to_pptx(
    input: &Path,
    output: &Path,
    opts: &SlideOptions,
) -> Result<SlideReport, SlideError> {
    if !input.exists() {
        return Err(SlideError::InputMissing(input.display().to_string()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(SlideError::OutputNotWritable(output.display().to_string()));
        }
    }
    let started = Instant::now();
    let doc = lopdf::Document::load(input)?;

    let pages_runs = extract::extract_per_page(&doc)?;
    let page_dims = extract::page_dimensions(&doc);
    let notes = if opts.include_speaker_notes {
        extract::extract_notes_per_page(&doc).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut slides = Vec::with_capacity(pages_runs.len());
    let mut titles_detected = 0u32;
    let mut notes_attached = 0u32;
    for (i, runs) in pages_runs.iter().enumerate() {
        let (w, h) = page_dims.get(i).copied().unwrap_or((720.0, 540.0));
        let mut sc = layout::cluster(runs, w, h, opts.detect_titles);
        if sc.title.is_some() {
            titles_detected += 1;
        }
        if let Some(n) = notes.get(i).cloned().flatten() {
            if !n.trim().is_empty() {
                sc.notes = Some(n);
                notes_attached += 1;
            }
        }
        slides.push(sc);
    }

    let bytes = pptx::write_pptx(&slides)?;
    std::fs::write(output, bytes)?;

    Ok(SlideReport {
        slides_emitted: slides.len() as u32,
        titles_detected,
        notes_attached,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_returns_error() {
        let r = convert_to_pptx(
            Path::new("/nonexistent/file-xyz123.pdf"),
            Path::new("/tmp/x.pptx"),
            &SlideOptions::default(),
        );
        assert!(matches!(r, Err(SlideError::InputMissing(_))));
    }

    #[test]
    fn output_dir_missing_returns_error() {
        // Create a tiny file we know exists
        let tmp = std::env::temp_dir().join("slab-slide-existing.txt");
        std::fs::write(&tmp, b"hi").unwrap();
        let r = convert_to_pptx(
            &tmp,
            Path::new("/nonexistent-dir-xyz/out.pptx"),
            &SlideOptions::default(),
        );
        std::fs::remove_file(&tmp).ok();
        assert!(matches!(
            r,
            Err(SlideError::OutputNotWritable(_)) | Err(SlideError::Pdf(_))
        ));
    }
}
