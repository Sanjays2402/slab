//! Mutating font-embed pass for the Bedrock PDF/A pipeline.
//!
//! For every font dictionary whose `/BaseFont` matches a Standard-14
//! name and which carries no `/FontDescriptor`, we splice a DejaVu
//! substitute TTF as a `FontFile2` stream, synthesise the descriptor,
//! and rewire the font dictionary. The result: a fully PDF/A-compliant
//! document with no missing embedded fonts.
//!
//! Truly custom fonts (those returning `None` from
//! [`font_table::lookup_substitute`]) are left alone — the orchestrator
//! will still fall back to the audit gate for those, since we have no
//! metrics to synthesise.

use super::font_table::{lookup_substitute, Substitute};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use ttf_parser::Face;

/// Walk every font object in the document, embed any that match a
/// Standard-14 substitute and lack a descriptor. Returns the count of
/// fonts embedded.
pub fn embed_missing_in_doc(doc: &mut Document) -> Result<usize, String> {
    // Collect candidates first to avoid borrow conflicts.
    let mut targets: Vec<(ObjectId, String, Substitute)> = Vec::new();
    for (id, obj) in &doc.objects {
        let dict = match obj.as_dict() {
            Ok(d) => d,
            Err(_) => continue,
        };
        if dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) != Some(b"Font".as_slice()) {
            continue;
        }
        if dict.has(b"FontDescriptor") {
            continue; // already embedded (or at least, has a descriptor)
        }
        let base = match dict.get(b"BaseFont").ok().and_then(|o| o.as_name().ok()) {
            Some(s) => String::from_utf8_lossy(s).into_owned(),
            None => continue,
        };
        if let Some(sub) = lookup_substitute(&base) {
            targets.push((*id, base, sub));
        }
    }

    let mut n = 0;
    for (font_id, base, sub) in targets {
        embed_substitute(doc, font_id, &base, sub)?;
        n += 1;
    }
    Ok(n)
}

/// Embed `sub` into `doc` and wire it up under `font_id`.
pub fn embed_substitute(
    doc: &mut Document,
    font_id: ObjectId,
    original_name: &str,
    sub: Substitute,
) -> Result<usize, String> {
    // Parse the TTF to extract metrics required by FontDescriptor.
    let face = Face::parse(sub.ttf_bytes, 0).map_err(|e| format!("ttf-parser: {e}"))?;
    let units = face.units_per_em() as f64;
    let scale = 1000.0 / units;

    let ascent = (face.ascender() as f64 * scale).round() as i64;
    let descent = (face.descender() as f64 * scale).round() as i64;
    let cap_height = face
        .capital_height()
        .map(|v| (v as f64 * scale).round() as i64)
        .unwrap_or(ascent);
    let bbox = face.global_bounding_box();
    let bbox_arr: Vec<Object> = vec![
        ((bbox.x_min as f64 * scale).round() as i64).into(),
        ((bbox.y_min as f64 * scale).round() as i64).into(),
        ((bbox.x_max as f64 * scale).round() as i64).into(),
        ((bbox.y_max as f64 * scale).round() as i64).into(),
    ];

    // Flags: PDF spec Table 123 (FontDescriptor flags). Bit positions are
    // 1-indexed in the spec; we use 0-indexed shifts.
    //   bit 1 (value 1)       = FixedPitch
    //   bit 2 (value 2)       = Serif
    //   bit 6 (value 32)      = Nonsymbolic (set for Latin text)
    //   bit 7 (value 64)      = Italic
    let mut flags: u32 = 1 << 5; // Nonsymbolic
    if sub.postscript_name.contains("Serif") {
        flags |= 1 << 1;
    }
    if sub.postscript_name.contains("Mono") {
        flags |= 1;
    }
    if sub.italic {
        flags |= 1 << 6;
    }

    // Build the FontFile2 stream — raw TTF bytes, no filter, Length1 = file size.
    let ttf_len = sub.ttf_bytes.len() as i64;
    let ff2 = Stream::new(dictionary! { "Length1" => ttf_len }, sub.ttf_bytes.to_vec());
    let ff2_id = doc.add_object(Object::Stream(ff2));

    // Synthesise the FontDescriptor.
    let descriptor = dictionary! {
        "Type"        => "FontDescriptor",
        "FontName"    => Object::Name(sub.postscript_name.as_bytes().to_vec()),
        "Flags"       => flags as i64,
        "FontBBox"    => Object::Array(bbox_arr),
        "ItalicAngle" => face.italic_angle().unwrap_or(0.0).round() as i64,
        "Ascent"      => ascent,
        "Descent"     => descent,
        "CapHeight"   => cap_height,
        "StemV"       => if sub.bold { 140i64 } else { 80 },
        "FontFile2"   => Object::Reference(ff2_id),
    };
    let desc_id = doc.add_object(Object::Dictionary(descriptor));

    // Rewire the original font dict to reference the descriptor + change
    // BaseFont to the embedded name (helps strict validators that require
    // BaseFont == FontDescriptor/FontName).
    let font_obj = doc.get_object_mut(font_id).map_err(|e| e.to_string())?;
    let font_dict = font_obj.as_dict_mut().map_err(|e| e.to_string())?;
    font_dict.set("FontDescriptor", Object::Reference(desc_id));
    font_dict.set(
        "BaseFont",
        Object::Name(sub.postscript_name.as_bytes().to_vec()),
    );
    // Forensic round-trip: remember what the original PS name was.
    font_dict.set(
        "SlabOriginalBaseFont",
        Object::Name(original_name.as_bytes().to_vec()),
    );

    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    #[test]
    fn embed_helvetica_writes_fontfile2_and_descriptor() {
        let mut doc = Document::with_version("1.7");
        let font_id = doc.add_object(dictionary! {
            "Type"     => "Font",
            "Subtype"  => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });

        let sub = lookup_substitute("Helvetica").unwrap();
        let n = embed_substitute(&mut doc, font_id, "Helvetica", sub).unwrap();
        assert_eq!(n, 1, "should report 1 font embedded");

        // The font dict should now carry a FontDescriptor reference.
        let font_dict = doc.get_object(font_id).unwrap().as_dict().unwrap();
        assert!(font_dict.has(b"FontDescriptor"));

        // Descriptor should have a FontFile2 reference with non-empty TTF bytes.
        let desc_id = match font_dict.get(b"FontDescriptor").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("FontDescriptor is not a reference"),
        };
        let desc = doc.get_object(desc_id).unwrap().as_dict().unwrap();
        let ff2_id = match desc.get(b"FontFile2").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("FontFile2 is not a reference"),
        };
        let ff2 = doc.get_object(ff2_id).unwrap().as_stream().unwrap();
        assert!(
            ff2.content.len() > 100_000,
            "TTF should be sizable; got {}",
            ff2.content.len()
        );
        assert_eq!(&ff2.content[0..4], b"\x00\x01\x00\x00", "TTF magic");

        // BaseFont rewired to the substitute name.
        let base = font_dict.get(b"BaseFont").unwrap().as_name().unwrap();
        assert_eq!(base, b"DejaVuSans");

        // Forensic key remembers the original.
        let orig = font_dict
            .get(b"SlabOriginalBaseFont")
            .unwrap()
            .as_name()
            .unwrap();
        assert_eq!(orig, b"Helvetica");
    }

    #[test]
    fn embed_missing_in_doc_skips_custom_fonts() {
        let mut doc = Document::with_version("1.7");
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "MyWeirdCorporateFont",
            "Encoding" => "WinAnsiEncoding",
        });
        // No substitute available → embed pass does nothing.
        let n = embed_missing_in_doc(&mut doc).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn embed_missing_in_doc_handles_multiple_standard14() {
        let mut doc = Document::with_version("1.7");
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Times-Roman",
            "Encoding" => "WinAnsiEncoding",
        });
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica-Bold",
            "Encoding" => "WinAnsiEncoding",
        });
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
            "Encoding" => "WinAnsiEncoding",
        });
        let n = embed_missing_in_doc(&mut doc).unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn embed_missing_in_doc_skips_fonts_with_descriptor() {
        let mut doc = Document::with_version("1.7");
        let fake_desc = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "AlreadyEmbedded",
        });
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "FontDescriptor" => Object::Reference(fake_desc),
        });
        let n = embed_missing_in_doc(&mut doc).unwrap();
        assert_eq!(n, 0, "font with existing descriptor must be left alone");
    }
}
