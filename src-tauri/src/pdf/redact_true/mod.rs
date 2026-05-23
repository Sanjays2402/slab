//! True (destructive) redaction.
//!
//! Unlike `super::redact`, which only paints opaque rectangles on top of the
//! existing content stream, this module excises the underlying text, image,
//! annotation, and metadata payloads so a recipient cannot recover redacted
//! content with `pdftotext`, `qpdf --qdf`, or any content-stream inspector.

pub mod glyph_bbox;

pub use self::glyph_bbox::{bbox_of_text_op, collect_text_boxes, GlyphBox, TextState};
