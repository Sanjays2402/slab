//! Standard-14 → DejaVu font substitution table.
//!
//! ISO 19005 forbids the PDF spec's "the viewer will substitute Helvetica
//! at display time" loophole — every font must be embedded. When the input
//! PDF only names a Standard-14 base font without supplying a font file,
//! we substitute one of the 12 bundled DejaVu TTF subsets whose metrics
//! match closely enough that reflow / wrapping won't visibly shift.
//!
//! This module is pure data + one lookup function. No I/O, no mutation.

/// Resolved substitute for a Standard-14 font.
#[derive(Debug, Clone, Copy)]
pub struct Substitute {
    /// PostScript name of the substitute we'll write into the
    /// `/FontDescriptor` `/FontName`.
    pub postscript_name: &'static str,
    /// Raw TTF bytes to splice into a `FontFile2` stream.
    pub ttf_bytes: &'static [u8],
    /// Whether the substitute is the bold weight.
    pub bold: bool,
    /// Whether the substitute is the italic / oblique style.
    pub italic: bool,
}

/// The 14 PostScript "Standard-14" base font names that PDF viewers were
/// historically permitted to render without an embedded font file. PDF/A
/// closes that loophole, which is the entire reason this module exists.
pub const STANDARD_14_NAMES: &[&str] = &[
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

// Embed the TTF bytes at compile time. They live in
// src-tauri/resources/fonts/ and are vendored via Task 1 (v3.0.2 Foundry Fonts).
const DEJA_SANS: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSans.ttf");
const DEJA_SANS_BOLD: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSans-Bold.ttf");
const DEJA_SANS_OBL: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSans-Oblique.ttf");
const DEJA_SANS_BOLD_OBL: &[u8] =
    include_bytes!("../../../resources/fonts/DejaVuSans-BoldOblique.ttf");
const DEJA_SERIF: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSerif.ttf");
const DEJA_SERIF_BOLD: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSerif-Bold.ttf");
const DEJA_SERIF_ITALIC: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSerif-Italic.ttf");
const DEJA_SERIF_BOLD_IT: &[u8] =
    include_bytes!("../../../resources/fonts/DejaVuSerif-BoldItalic.ttf");
const DEJA_MONO: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSansMono.ttf");
const DEJA_MONO_BOLD: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSansMono-Bold.ttf");
const DEJA_MONO_OBL: &[u8] = include_bytes!("../../../resources/fonts/DejaVuSansMono-Oblique.ttf");
const DEJA_MONO_BOLD_OBL: &[u8] =
    include_bytes!("../../../resources/fonts/DejaVuSansMono-BoldOblique.ttf");

/// Look up a substitute for a PostScript font name. Returns `None`
/// for unknown / custom fonts (caller should keep using audit gating
/// in that case — we won't fabricate something we don't have metrics for).
pub fn lookup_substitute(postscript_name: &str) -> Option<Substitute> {
    Some(match postscript_name {
        "Times-Roman" => sub("DejaVuSerif", DEJA_SERIF, false, false),
        "Times-Bold" => sub("DejaVuSerif-Bold", DEJA_SERIF_BOLD, true, false),
        "Times-Italic" => sub("DejaVuSerif-Italic", DEJA_SERIF_ITALIC, false, true),
        "Times-BoldItalic" => sub("DejaVuSerif-BoldItalic", DEJA_SERIF_BOLD_IT, true, true),
        "Helvetica" => sub("DejaVuSans", DEJA_SANS, false, false),
        "Helvetica-Bold" => sub("DejaVuSans-Bold", DEJA_SANS_BOLD, true, false),
        "Helvetica-Oblique" => sub("DejaVuSans-Oblique", DEJA_SANS_OBL, false, true),
        "Helvetica-BoldOblique" => sub("DejaVuSans-BoldOblique", DEJA_SANS_BOLD_OBL, true, true),
        "Courier" => sub("DejaVuSansMono", DEJA_MONO, false, false),
        "Courier-Bold" => sub("DejaVuSansMono-Bold", DEJA_MONO_BOLD, true, false),
        "Courier-Oblique" => sub("DejaVuSansMono-Oblique", DEJA_MONO_OBL, false, true),
        "Courier-BoldOblique" => sub("DejaVuSansMono-BoldOblique", DEJA_MONO_BOLD_OBL, true, true),
        // Symbol & ZapfDingbats: DejaVu Sans covers many but not all glyphs.
        // Best-effort substitution — better than refusing to convert.
        "Symbol" => sub("DejaVuSans", DEJA_SANS, false, false),
        "ZapfDingbats" => sub("DejaVuSans", DEJA_SANS, false, false),
        _ => return None,
    })
}

const fn sub(name: &'static str, bytes: &'static [u8], bold: bool, italic: bool) -> Substitute {
    Substitute {
        postscript_name: name,
        ttf_bytes: bytes,
        bold,
        italic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helvetica_maps_to_dejavu_sans() {
        let sub = lookup_substitute("Helvetica").unwrap();
        assert_eq!(sub.postscript_name, "DejaVuSans");
        assert!(!sub.bold);
        assert!(!sub.italic);
    }

    #[test]
    fn helvetica_bold_oblique_maps_correctly() {
        let sub = lookup_substitute("Helvetica-BoldOblique").unwrap();
        assert_eq!(sub.postscript_name, "DejaVuSans-BoldOblique");
        assert!(sub.bold);
        assert!(sub.italic);
    }

    #[test]
    fn times_roman_maps_to_dejavu_serif() {
        let sub = lookup_substitute("Times-Roman").unwrap();
        assert_eq!(sub.postscript_name, "DejaVuSerif");
    }

    #[test]
    fn courier_bold_maps_to_dejavu_mono_bold() {
        let sub = lookup_substitute("Courier-Bold").unwrap();
        assert_eq!(sub.postscript_name, "DejaVuSansMono-Bold");
        assert!(sub.bold);
    }

    #[test]
    fn unknown_font_returns_none() {
        assert!(lookup_substitute("MyCustomFont").is_none());
    }

    #[test]
    fn every_standard14_has_a_substitute() {
        for std14 in STANDARD_14_NAMES {
            assert!(
                lookup_substitute(std14).is_some(),
                "missing sub for {std14}"
            );
        }
    }

    #[test]
    fn every_substitute_resolves_to_bundled_bytes() {
        for std14 in STANDARD_14_NAMES {
            let sub = lookup_substitute(std14).unwrap();
            assert!(
                !sub.ttf_bytes.is_empty(),
                "{std14} -> {} has empty bytes",
                sub.postscript_name
            );
            // TTF magic: 00 01 00 00 (TrueType outlines)
            assert_eq!(
                &sub.ttf_bytes[0..4],
                b"\x00\x01\x00\x00",
                "{std14} -> {} bytes don't start with TTF magic",
                sub.postscript_name
            );
        }
    }
}
