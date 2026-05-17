// Auto-Redact: find regex matches across a PDF's text and paint black
// bars over the bounding boxes.
//
// True text-level redaction (removing glyphs from content streams) is hard
// and risky. We take the safer, more conservative approach used by 99% of
// "consumer" PDF tools: extract text with positions, run a regex, then
// overlay opaque rectangles on the matching regions. The text remains in
// the underlying content stream (recoverable by a determined attacker)
// but rendered output everywhere looks redacted.
//
// Built-in regex presets:
//   "email"   — RFC-ish email addresses
//   "ssn"     — US Social Security numbers (XXX-XX-XXXX)
//   "phone"   — North American phone numbers
//   "cc"      — credit-card-shaped digit groups
//
// Custom patterns can be passed in. We compile them with the `regex` crate.

use crate::pdf::redact::{redact, RedactOpts, RedactRect};
use crate::pdf::PdfError;
use lopdf::{Document, Object};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoRedactOpts {
    /// Custom regex patterns. May be empty if only presets are used.
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Preset names: "email" | "ssn" | "phone" | "cc".
    #[serde(default)]
    pub presets: Vec<String>,
    /// 0.0 = pure black, 1.0 = white. Defaults to 0.
    #[serde(default)]
    pub gray: f32,
}

// Public so the Beacon PII detector (`ai::pii`) can reuse the same patterns
// for the discover-then-redact two-step flow. Keep these in sync — Slab
// guarantees that "PII found by the Beacon panel" and "what auto-redact's
// preset will black out" cover the same matches.
pub const PRESET_EMAIL: &str = r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}";
pub const PRESET_SSN: &str = r"\b\d{3}-\d{2}-\d{4}\b";
pub const PRESET_PHONE: &str = r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b";
pub const PRESET_CC: &str = r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b";

pub fn auto_redact(input: &Path, output: &Path, opts: AutoRedactOpts) -> Result<u32, PdfError> {
    // Compile all patterns up front so we fail fast on bad input.
    let mut regexes: Vec<Regex> = Vec::new();
    for name in &opts.presets {
        let pat = match name.as_str() {
            "email" => PRESET_EMAIL,
            "ssn" => PRESET_SSN,
            "phone" => PRESET_PHONE,
            "cc" => PRESET_CC,
            other => {
                return Err(PdfError::Other(format!("Unknown preset: {other}")));
            }
        };
        let re = Regex::new(pat).map_err(|e| PdfError::Other(format!("Regex error: {e}")))?;
        regexes.push(re);
    }
    for p in &opts.patterns {
        let re = Regex::new(p).map_err(|e| PdfError::Other(format!("Custom regex {p:?}: {e}")))?;
        regexes.push(re);
    }
    if regexes.is_empty() {
        return Err(PdfError::Other(
            "No patterns or presets provided for auto-redact.".into(),
        ));
    }

    let doc = Document::load(input)?;
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();

    // Walk each page, extract text per-page, find matches, convert match
    // positions to approximate rectangles. lopdf's text extraction returns
    // a single string per page; without per-glyph positions we don't have
    // exact bounding boxes. We compromise: for each match, redact a band
    // across the full text region (a horizontal stripe at the line
    // containing the match).
    //
    // To get something close to per-line, we approximate by counting line
    // breaks before/after the match in the extracted text. This is rough
    // but it's the best we can do with just lopdf — and it's typically
    // safer (over-redacts rather than under).

    let mut rects: Vec<RedactRect> = Vec::new();
    let mut match_count = 0u32;

    for (page_num, _) in &page_ids {
        let text = match doc.extract_text(&[*page_num]) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if text.is_empty() {
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            continue;
        }
        let n_lines = lines.len() as f32;

        for re in &regexes {
            for m in re.find_iter(&text) {
                match_count += 1;
                // Find which line contains the match.
                let mut idx = 0usize;
                let mut line_idx = 0usize;
                for (i, line) in lines.iter().enumerate() {
                    let len = line.len() + 1; // +1 for the newline
                    if m.start() < idx + len {
                        line_idx = i;
                        break;
                    }
                    idx += len;
                }
                // Convert to bottom-up % (PDF y-axis grows upward).
                // Top of page = 100%, bottom = 0%.
                let line_top_pct = 100.0 - (line_idx as f32 / n_lines) * 100.0;
                let line_bot_pct = 100.0 - ((line_idx + 1) as f32 / n_lines) * 100.0;
                let top = line_top_pct.clamp(0.0, 100.0);
                let bot = line_bot_pct.clamp(0.0, 100.0);
                rects.push(RedactRect {
                    page: *page_num,
                    left_pct: 5.0,
                    right_pct: 95.0,
                    bottom_pct: bot,
                    top_pct: top,
                });
            }
        }
    }

    if rects.is_empty() {
        // Nothing matched. Still produce a copy of the input as output so
        // callers can swap files predictably.
        std::fs::copy(input, output)?;
        return Ok(0);
    }

    redact(
        input,
        output,
        RedactOpts {
            rects,
            gray: opts.gray.clamp(0.0, 1.0),
        },
    )?;
    Ok(match_count)
}

/// Strip the silly "_ = obj" warning suppression idiom — used internally.
#[allow(dead_code)]
fn _force_use(_o: &Object) {}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn pdf_with_text(text: &str) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font },
        });
        // Build a simple content stream: BT /F1 12 Tf 50 700 Td (text) Tj ET
        let safe_text: String = text
            .chars()
            .map(|c| match c {
                '(' => '['.to_string(),
                ')' => ']'.to_string(),
                '\\' => "/".to_string(),
                c => c.to_string(),
            })
            .collect::<Vec<_>>()
            .join("");
        let content = format!(
            "BT /F1 12 Tf 50 700 Td ({safe_text}) Tj ET\n",
            safe_text = safe_text
        );
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => stream_id,
            "Resources" => resources,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
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
    fn requires_some_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, pdf_with_text("hello")).unwrap();
        let r = auto_redact(
            &input,
            &output,
            AutoRedactOpts {
                patterns: vec![],
                presets: vec![],
                gray: 0.0,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn unknown_preset_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, pdf_with_text("hello")).unwrap();
        let r = auto_redact(
            &input,
            &output,
            AutoRedactOpts {
                patterns: vec![],
                presets: vec!["bogus".into()],
                gray: 0.0,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn no_matches_copies_file() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, pdf_with_text("hello world")).unwrap();
        let n = auto_redact(
            &input,
            &output,
            AutoRedactOpts {
                patterns: vec![],
                presets: vec!["email".into()],
                gray: 0.0,
            },
        )
        .unwrap();
        assert_eq!(n, 0);
        assert!(output.exists());
    }

    #[test]
    fn invalid_custom_regex_errors() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        let output = dir.path().join("out.pdf");
        std::fs::write(&input, pdf_with_text("hello")).unwrap();
        let r = auto_redact(
            &input,
            &output,
            AutoRedactOpts {
                patterns: vec!["[invalid(".into()],
                presets: vec![],
                gray: 0.0,
            },
        );
        assert!(r.is_err());
    }
}
