//! Read-only font audit for the PDF/A pipeline. Walks every page's
//! `/Resources/Font/*` map and reports which fonts are *not* embedded
//! and which lack a `/ToUnicode` CMap.
//!
//! ISO 19005-2 §6.2.11 requires every font used in a PDF/A document to
//! be embedded as a subset (or full) `FontFile`, `FontFile2`, or
//! `FontFile3` byte stream referenced from its `/FontDescriptor`. Slice 3
//! will add a *mutating* `embed_fonts` pass that rewrites missing
//! descriptors when subsets can be synthesised; this Slice 2 module
//! ships only the read-only audit so the pre-flight UI can warn the user
//! before the conversion runs ("3 fonts can't be embedded — convert
//! anyway?").
//!
//! ToUnicode CMaps are required for PDF/A-2u (unicode) but only
//! recommended for PDF/A-2b. We track them either way so the audit
//! report tells the user which conformance level is achievable.

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single font referenced by the document. Identifier is the
/// `BaseFont` name (or `Type` + object id as a fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontEntry {
    /// `BaseFont` (PostScript name) when available, else a synthetic name.
    pub name: String,
    /// `Type` from the font dictionary, e.g. `"Type0"`, `"TrueType"`.
    pub subtype: String,
    /// True when the font's `/FontDescriptor` references at least one of
    /// `FontFile`, `FontFile2`, `FontFile3`.
    pub embedded: bool,
    /// True when the font dictionary carries a `/ToUnicode` stream.
    pub has_to_unicode: bool,
    /// One of the standard 14 PDF base fonts (Helvetica, Times-Roman, ...)
    /// which need not be embedded per PDF spec but DO need to be embedded
    /// for PDF/A. Flagged here so the pre-flight surfaces them prominently.
    pub is_standard14: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FontAuditReport {
    /// Every distinct font found, keyed by name.
    pub fonts: BTreeMap<String, FontEntry>,
}

impl FontAuditReport {
    pub fn count(&self) -> usize {
        self.fonts.len()
    }

    pub fn missing_embed(&self) -> Vec<&FontEntry> {
        self.fonts.values().filter(|f| !f.embedded).collect()
    }

    pub fn missing_to_unicode(&self) -> Vec<&FontEntry> {
        self.fonts.values().filter(|f| !f.has_to_unicode).collect()
    }

    /// `true` when every font is embedded — required for any PDF/A level.
    pub fn all_embedded(&self) -> bool {
        self.fonts.values().all(|f| f.embedded)
    }

    /// `true` when every font is embedded AND has ToUnicode — required for
    /// PDF/A-2u opportunistic upgrade.
    pub fn unicode_ready(&self) -> bool {
        self.fonts.values().all(|f| f.embedded && f.has_to_unicode)
    }
}

/// The 14 PostScript base fonts that PDF readers are required to substitute
/// at display time. PDF/A explicitly forbids relying on substitution.
const STANDARD_14: &[&str] = &[
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

/// Run the font audit over `doc`. Pure inspection — does not mutate.
pub fn audit_fonts(doc: &Document) -> FontAuditReport {
    let mut report = FontAuditReport::default();

    for (_page_num, page_id) in doc.get_pages() {
        let fonts = collect_page_fonts(doc, page_id);
        for font_id in fonts {
            if let Some(entry) = inspect_font(doc, font_id) {
                // Dedup by name — same font referenced from multiple
                // pages should appear once.
                report.fonts.entry(entry.name.clone()).or_insert(entry);
            }
        }
    }

    report
}

fn collect_page_fonts(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let resources = match resolve_resources(doc, page_id) {
        Some(dict) => dict,
        None => return out,
    };
    let fonts = match resources.get(b"Font") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return out,
        },
        _ => return out,
    };
    for (_k, v) in fonts.iter() {
        if let Object::Reference(id) = v {
            out.push(*id);
        }
    }
    out
}

fn resolve_resources(doc: &Document, page_id: ObjectId) -> Option<lopdf::Dictionary> {
    let page = match doc.get_object(page_id) {
        Ok(Object::Dictionary(d)) => d,
        _ => return None,
    };
    match page.get(b"Resources") {
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn inspect_font(doc: &Document, font_id: ObjectId) -> Option<FontEntry> {
    let font = match doc.get_object(font_id) {
        Ok(Object::Dictionary(d)) => d,
        _ => return None,
    };

    let base_font = font
        .get(b"BaseFont")
        .ok()
        .and_then(|o| match o {
            Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
            _ => None,
        })
        .unwrap_or_else(|| format!("font_{}_{}", font_id.0, font_id.1));

    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|o| match o {
            Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
            _ => None,
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let has_to_unicode = font.has(b"ToUnicode");

    // Embedded? Resolve /FontDescriptor and check for FontFile*.
    let embedded = check_embedded(doc, font);

    // Trim potential subset prefix "ABCDEF+RealName" → "RealName" only
    // for the standard-14 check.
    let canonical = base_font
        .split_once('+')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| base_font.clone());
    let is_standard14 = STANDARD_14.iter().any(|n| *n == canonical.as_str());

    Some(FontEntry {
        name: base_font,
        subtype,
        embedded,
        has_to_unicode,
        is_standard14,
    })
}

fn check_embedded(doc: &Document, font: &lopdf::Dictionary) -> bool {
    let descriptor = match font.get(b"FontDescriptor") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return false,
        },
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => {
            // For Type0 fonts the descriptor lives on the CIDFont in
            // /DescendantFonts.
            if let Ok(Object::Array(arr)) = font.get(b"DescendantFonts") {
                for o in arr {
                    if let Object::Reference(id) = o {
                        if let Ok(Object::Dictionary(cid)) = doc.get_object(*id) {
                            if check_embedded(doc, cid) {
                                return true;
                            }
                        }
                    }
                }
            }
            return false;
        }
    };
    for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
        if descriptor.has(key) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};

    fn build_doc_with_font(font: lopdf::Dictionary) -> Document {
        let mut doc = Document::with_version("1.7");
        let font_id = doc.add_object(font);
        let resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font_id),
            }),
        };
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Resources" => Object::Dictionary(resources),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), 612.into(), 792.into(),
            ]),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1,
        });
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", cat_id);
        doc
    }

    #[test]
    fn empty_doc_audits_zero_fonts() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![]),
            "Count" => 0,
        });
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", cat_id);
        let r = audit_fonts(&doc);
        assert_eq!(r.count(), 0);
        assert!(r.all_embedded(), "vacuously true");
        assert!(r.unicode_ready(), "vacuously true");
    }

    #[test]
    fn standard14_unembedded_is_flagged() {
        let doc = build_doc_with_font(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let r = audit_fonts(&doc);
        assert_eq!(r.count(), 1);
        let f = r.fonts.get("Helvetica").unwrap();
        assert!(!f.embedded);
        assert!(f.is_standard14);
        assert!(!r.all_embedded());
    }

    #[test]
    fn embedded_font_via_fontfile2_is_detected() {
        let mut doc = Document::with_version("1.7");
        // Build a font with a FontDescriptor that references a FontFile2 stream.
        let stream_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Length1" => 1024 },
            vec![0u8; 16],
        )));
        let descriptor_id = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "MyFont",
            "FontFile2" => Object::Reference(stream_id),
        });
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "MyFont",
            "FontDescriptor" => Object::Reference(descriptor_id),
            "ToUnicode" => Object::Reference(stream_id),
        });
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), 612.into(), 792.into(),
            ]),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1,
        });
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", cat_id);

        let r = audit_fonts(&doc);
        let f = r.fonts.get("MyFont").unwrap();
        assert!(f.embedded);
        assert!(f.has_to_unicode);
        assert!(r.all_embedded());
        assert!(r.unicode_ready());
    }

    #[test]
    fn subset_prefix_strips_for_standard14_check() {
        let doc = build_doc_with_font(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "ABCDEF+Helvetica",
        });
        let r = audit_fonts(&doc);
        let f = r.fonts.get("ABCDEF+Helvetica").unwrap();
        assert!(f.is_standard14);
    }

    #[test]
    fn type0_descendant_font_descriptor_is_checked() {
        let mut doc = Document::with_version("1.7");
        let stream_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Length1" => 4096 },
            vec![0u8; 32],
        )));
        let descriptor_id = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "CIDFont",
            "FontFile2" => Object::Reference(stream_id),
        });
        let cid_font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "CIDFont",
            "FontDescriptor" => Object::Reference(descriptor_id),
        });
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "CIDFont",
            "DescendantFonts" => Object::Array(vec![Object::Reference(cid_font_id)]),
        });
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), 612.into(), 792.into(),
            ]),
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => 1,
        });
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
        let cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", cat_id);

        let r = audit_fonts(&doc);
        let f = r.fonts.get("CIDFont").unwrap();
        assert!(
            f.embedded,
            "Type0 font's CIDFont descendant descriptor must be probed"
        );
    }

    #[test]
    fn missing_embed_filter_works() {
        let doc = build_doc_with_font(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let r = audit_fonts(&doc);
        assert_eq!(r.missing_embed().len(), 1);
        assert_eq!(r.missing_to_unicode().len(), 1);
    }
}
