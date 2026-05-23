//! Visible signature appearances — Form XObject (AP/N) rendered into a Widget.
//!
//! When `SignOptions::appearance` is set, `sign_pdf` swaps the invisible
//! Widget for one with `/Rect` + `/AP << /N <xobject> >>` so Acrobat /
//! Preview / Foxit render the signature as a visible stamp on the page.
//!
//! This module is _pure stream building_: given an [`AppearanceSpec`] and a
//! [`SigningIdentity`] it produces the byte content of a PDF Form XObject's
//! content stream plus the dictionary entries that Widget assemblers need
//! (`/BBox`, `/Resources`, `/FormType`). The actual splicing into the PDF
//! document graph happens in `signet::sign::sign_pdf`.

use crate::pdf::signet::SigningIdentity;

/// Caller-supplied knobs for rendering a visible signature appearance.
#[derive(Debug, Clone)]
pub struct AppearanceSpec {
    /// 1-indexed page to place the appearance on.
    pub page: u32,
    /// [llx, lly, urx, ury] in PDF user-space units.
    pub rect: [f32; 4],
    /// Font size for the rendered text. Defaults to 9.0pt.
    pub font_size: f32,
    pub show_name: bool,
    pub show_date: bool,
    pub show_reason: bool,
    pub show_location: bool,
    /// Optional PNG/JPEG bytes embedded as a background logo. (Not yet
    /// rendered — placeholder for the next slice.)
    pub image: Option<Vec<u8>>,
    /// Optional human-readable reason (e.g. "I am the author of this document").
    pub reason: Option<String>,
    /// Optional location string (e.g. "Seattle, WA").
    pub location: Option<String>,
    /// Optional pre-formatted signing time. If None, the appearance builder
    /// will leave the date line off (the timestamp is set by the caller so
    /// tests are deterministic).
    pub signing_time: Option<String>,
}

impl Default for AppearanceSpec {
    fn default() -> Self {
        Self {
            page: 1,
            rect: [50.0, 50.0, 250.0, 120.0],
            font_size: 9.0,
            show_name: true,
            show_date: true,
            show_reason: true,
            show_location: false,
            image: None,
            reason: None,
            location: None,
            signing_time: None,
        }
    }
}

/// One built Form XObject — ready to be wrapped in a stream object and
/// referenced as `/AP << /N <ref> >>` on the Widget annotation.
#[derive(Debug, Clone)]
pub struct Appearance {
    /// `/BBox` array in XObject coordinate space. Origin is (0,0); width and
    /// height come straight from the spec's rect.
    pub bbox: [f32; 4],
    /// Raw PDF content-stream bytes (between `stream`/`endstream`).
    pub content_stream: Vec<u8>,
    /// Font resources keyed by PDF resource name — currently just `/F1` →
    /// Helvetica. The serializer turns this into `/Resources << /Font ... >>`.
    pub fonts: Vec<(String, &'static str)>,
}

impl Appearance {
    /// Width of the appearance in user-space units.
    pub fn width(&self) -> f32 {
        self.bbox[2] - self.bbox[0]
    }
    /// Height of the appearance in user-space units.
    pub fn height(&self) -> f32 {
        self.bbox[3] - self.bbox[1]
    }
}

/// Build a Form XObject content stream for the given identity + spec.
///
/// The layout is deliberately Acrobat-classic — left column "Digitally
/// signed by\n<name>", right column metadata (date, reason, location).
/// All text uses Helvetica (`/F1`) at `spec.font_size`.
pub fn build_appearance(identity: &SigningIdentity, spec: &AppearanceSpec) -> Appearance {
    build_appearance_from_name(&identity.subject_cn, spec)
}

/// Lower-level variant that takes just the signer's display name. Used by
/// tests and by callers that don't have a full [`SigningIdentity`] in hand
/// (e.g. preview rendering in the UI before the user picks an identity).
pub fn build_appearance_from_name(signer_name: &str, spec: &AppearanceSpec) -> Appearance {
    let width = (spec.rect[2] - spec.rect[0]).max(1.0);
    let height = (spec.rect[3] - spec.rect[1]).max(1.0);
    let font_size = if spec.font_size <= 0.0 {
        9.0
    } else {
        spec.font_size
    };

    // We render in XObject space: origin (0,0) is the lower-left of the
    // appearance rectangle. The viewer applies the Widget's CTM to land it
    // on the page at spec.rect.
    let pad = 6.0_f32;
    let leading = font_size * 1.25;
    // Top of the text block, leaving padding.
    let mut y = height - pad - font_size;

    let mut stream = String::new();
    // Optional faint border so users see the appearance even with no
    // background image. Width 0.5pt, RGB grey.
    stream.push_str("q\n");
    stream.push_str("0.7 0.7 0.7 RG\n");
    stream.push_str("0.5 w\n");
    stream.push_str(&format!("0 0 {width:.2} {height:.2} re S\n"));
    stream.push_str("Q\n");

    // Now the text. BT/ET delimits a text object; Tf sets font + size,
    // Td positions, Tj draws a literal string. All values escaped per
    // PDF 1.7 §7.3.4.2.
    stream.push_str("BT\n");
    stream.push_str(&format!("/F1 {font_size:.2} Tf\n"));
    stream.push_str(&format!("{:.2} TL\n", leading));
    stream.push_str(&format!("{pad:.2} {y:.2} Td\n"));

    let mut first_line = true;
    let mut emit_line = |s: &str, stream: &mut String, y: &mut f32| {
        if !first_or(&mut first_line) {
            // Move to next line: T* applies the leading set by TL.
            stream.push_str("T*\n");
        }
        stream.push('(');
        stream.push_str(&escape_pdf_string(s));
        stream.push_str(") Tj\n");
        *y -= leading;
    };

    if spec.show_name {
        emit_line("Digitally signed by", &mut stream, &mut y);
        emit_line(signer_name, &mut stream, &mut y);
    }
    if spec.show_date {
        if let Some(when) = &spec.signing_time {
            emit_line(&format!("Date: {when}"), &mut stream, &mut y);
        }
    }
    if spec.show_reason {
        if let Some(reason) = &spec.reason {
            emit_line(&format!("Reason: {reason}"), &mut stream, &mut y);
        }
    }
    if spec.show_location {
        if let Some(loc) = &spec.location {
            emit_line(&format!("Location: {loc}"), &mut stream, &mut y);
        }
    }
    stream.push_str("ET\n");

    Appearance {
        bbox: [0.0, 0.0, width, height],
        content_stream: stream.into_bytes(),
        fonts: vec![("F1".to_string(), "Helvetica")],
    }
}

/// Per-PDF-spec §7.3.4.2: in a literal string, `(`, `)`, and `\` must be
/// escaped with a leading backslash; other bytes pass through.
fn escape_pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

/// Toggle helper — returns the *current* value of `first_line` and clears it.
/// On the very first emitted line we skip the `T*` (text-cursor move) call.
fn first_or(first: &mut bool) -> bool {
    let was = *first;
    *first = false;
    was
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn build(name: &str, spec: &AppearanceSpec) -> Appearance {
        build_appearance_from_name(name, spec)
    }

    #[test]
    fn appearance_contains_subject_cn() {
        let spec = AppearanceSpec::default();
        let app = build("Sanjay Test", &spec);
        let s = std::str::from_utf8(&app.content_stream).unwrap();
        assert!(s.contains("Digitally signed by"));
        assert!(s.contains("Sanjay Test"));
        assert!(s.contains("BT"));
        assert!(s.contains("ET"));
    }

    #[test]
    fn appearance_respects_rect_dimensions() {
        let spec = AppearanceSpec {
            rect: [10.0, 10.0, 210.0, 110.0],
            ..Default::default()
        };
        let app = build("CN", &spec);
        assert_eq!(app.bbox, [0.0, 0.0, 200.0, 100.0]);
        assert!((app.width() - 200.0).abs() < 0.01);
        assert!((app.height() - 100.0).abs() < 0.01);
    }

    #[test]
    fn appearance_renders_date_when_present() {
        let spec = AppearanceSpec {
            signing_time: Some("2026-05-23 15:30 PT".to_string()),
            ..Default::default()
        };
        let app = build("CN", &spec);
        let s = std::str::from_utf8(&app.content_stream).unwrap();
        assert!(s.contains("Date: 2026-05-23 15:30 PT"));
    }

    #[test]
    fn appearance_skips_optional_fields() {
        let spec = AppearanceSpec {
            show_date: false,
            show_reason: false,
            show_location: false,
            ..Default::default()
        };
        let app = build("CN", &spec);
        let s = std::str::from_utf8(&app.content_stream).unwrap();
        assert!(!s.contains("Date:"));
        assert!(!s.contains("Reason:"));
        assert!(!s.contains("Location:"));
    }

    #[test]
    fn appearance_escapes_parentheses_in_cn() {
        let spec = AppearanceSpec::default();
        let app = build("Acme (Holdings) Ltd.", &spec);
        let s = std::str::from_utf8(&app.content_stream).unwrap();
        // Literal "(Holdings)" must become "\(Holdings\)" inside the PDF
        // literal string.
        assert!(
            s.contains(r"Acme \(Holdings\) Ltd."),
            "parens not escaped: {s}"
        );
    }

    #[test]
    fn appearance_clamps_invalid_font_size() {
        let spec = AppearanceSpec {
            font_size: 0.0,
            ..Default::default()
        };
        let app = build("CN", &spec);
        let s = std::str::from_utf8(&app.content_stream).unwrap();
        // Default is 9.0pt — should appear in the Tf operator.
        assert!(
            s.contains("/F1 9.00 Tf"),
            "expected default font size, got: {s}"
        );
    }

    #[test]
    fn appearance_includes_font_resource() {
        let app = build("CN", &AppearanceSpec::default());
        assert_eq!(app.fonts.len(), 1);
        assert_eq!(app.fonts[0].0, "F1");
        assert_eq!(app.fonts[0].1, "Helvetica");
    }

    #[test]
    fn appearance_renders_reason_and_location() {
        let spec = AppearanceSpec {
            show_location: true,
            reason: Some("Approval".into()),
            location: Some("Seattle, WA".into()),
            ..Default::default()
        };
        let app = build("CN", &spec);
        let s = std::str::from_utf8(&app.content_stream).unwrap();
        assert!(s.contains("Reason: Approval"));
        assert!(s.contains("Location: Seattle, WA"));
    }

    #[test]
    fn escape_handles_backslash_and_newlines() {
        assert_eq!(escape_pdf_string(r"foo\bar"), r"foo\\bar");
        assert_eq!(escape_pdf_string("line\nbreak"), r"line\nbreak");
        assert_eq!(escape_pdf_string("a(b)c"), r"a\(b\)c");
    }
}
