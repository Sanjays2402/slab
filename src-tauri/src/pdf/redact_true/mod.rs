//! True (destructive) redaction.
//!
//! Unlike `super::redact`, which only paints opaque rectangles on top of the
//! existing content stream, this module excises the underlying text, image,
//! annotation, and metadata payloads so a recipient cannot recover redacted
//! content with `pdftotext`, `qpdf --qdf`, or any content-stream inspector.

pub mod annotations;
pub mod driver;
pub mod glyph_bbox;
pub mod sanitize;
pub mod text_stream;

pub use self::annotations::scrub_annotations_on_page;
pub use self::driver::{redact_true, SanitizeFlatReport, TrueRedactReport};
pub use self::glyph_bbox::{bbox_of_text_op, collect_text_boxes, GlyphBox, TextState};
pub use self::sanitize::{sanitize_document, SanitizeReport};
pub use self::text_stream::{excise_text_on_page, rect_to_points};
