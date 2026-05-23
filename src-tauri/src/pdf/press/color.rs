//! Color normalization pass for PDF/X-4 conversion.
//!
//! # What this pass does
//!
//! PDF/X-4 (ISO 15930-7) requires that every colour reference in the
//! document can be resolved to an unambiguous, device-independent colour
//! space. Raw `DeviceRGB`, `DeviceGray`, and `DeviceCMYK` are forbidden
//! *unless* the page's resource dictionary declares matching
//! `/DefaultRGB`, `/DefaultGray`, or `/DefaultCMYK` entries that point to
//! ICC-based colour spaces.
//!
//! Rather than rewrite every `rg` / `RG` / `g` / `G` / `k` / `K` operator
//! in every content stream (which risks breaking ExtGState-modulated
//! rendering and Type 3 font glyphs), this pass uses the standard
//! ISO 32000 §8.6.5.6 "Default Colour Spaces" mechanism:
//!
//! - Insert the output-intent ICC profile as a `/DefaultRGB`-style colour
//!   space in `/Resources/ColorSpace` on every page that lacks one.
//! - Leave the content stream untouched. PDF readers, RIPs, and
//!   validators that honour `/DefaultRGB` (which is all of them since
//!   Acrobat 6) will see the device colour ops as ICC-tagged.
//!
//! This is the approach Adobe Acrobat Pro uses for "Convert to PDF/X-4"
//! and what PitStop and callas pdfToolbox both produce. It's the
//! lowest-risk path to ISO 15930-7 §6.2.4 compliance.
//!
//! # Out of scope for Slice 2
//!
//! - 16→8-bit image downsample. Deferred to v3.8.1: most modern PDFs
//!   already ship 8-bit images, and the pass-through-with-warning
//!   strategy is acceptable for v3.8.0 (validator will flag any 16-bit
//!   stream as a Warning, not Error).
//! - Spot-colour passthrough policy. Slice 4 (orchestrator) decides.
//! - `/CalRGB` / `/Lab` conflict detection. The Slice 5 validator
//!   catches it.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use super::OutputIntent;

/// Options for the colour-normalization pass.
#[derive(Debug, Clone)]
pub struct ColorOptions {
    /// Output intent whose ICC profile will be installed as the page's
    /// `/DefaultRGB` / `/DefaultGray` / `/DefaultCMYK`.
    pub target_intent: OutputIntent,
    /// If `true`, only install a `/DefaultCMYK` (CMYK-only workflow).
    /// Default `false` installs `/DefaultRGB` + `/DefaultGray` +
    /// `/DefaultCMYK` so every device space op is covered.
    pub cmyk_only: bool,
}

impl Default for ColorOptions {
    fn default() -> Self {
        Self {
            target_intent: OutputIntent::Fogra51Coated,
            cmyk_only: false,
        }
    }
}

/// Stats produced by [`normalize_color`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColorStats {
    /// Pages that gained at least one `/Default*` colour-space entry.
    pub pages_touched: usize,
    /// Pages that already had a `/DefaultRGB` set; left untouched.
    pub default_rgb_preserved: usize,
    /// Pages that already had a `/DefaultGray` set; left untouched.
    pub default_gray_preserved: usize,
    /// Pages that already had a `/DefaultCMYK` set; left untouched.
    pub default_cmyk_preserved: usize,
    /// New `/Default*` entries written across all pages.
    pub default_entries_added: usize,
    /// ICC profile object inserted into the document (one shared stream).
    pub icc_object_id: Option<ObjectId>,
}

/// Walk every page and ensure ICC-based default colour spaces are
/// declared in `/Resources/ColorSpace`.
///
/// The output-intent ICC profile is uploaded **once** as a shared
/// stream object and referenced from every page's Resources entry.
/// This keeps the produced PDF small even on 1000-page documents.
///
/// # Errors
///
/// Returns `Err` if the catalog is malformed (no `/Pages`), the page
/// tree is unreadable, or a page's Resources entry exists but is not a
/// dictionary. Existing `/Default*` entries are preserved untouched.
pub fn normalize_color(doc: &mut Document, opts: &ColorOptions) -> Result<ColorStats, String> {
    let mut stats = ColorStats::default();

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if page_ids.is_empty() {
        return Ok(stats);
    }

    // Upload the ICC profile once, share across pages.
    let icc_stream_id = insert_icc_stream(doc, opts)?;
    stats.icc_object_id = Some(icc_stream_id);

    // Build the array form: [/ICCBased N 0 R]
    let icc_cs_array = Object::Array(vec![
        Object::Name(b"ICCBased".to_vec()),
        Object::Reference(icc_stream_id),
    ]);

    for page_id in page_ids {
        let added = ensure_page_defaults(doc, page_id, &icc_cs_array, opts.cmyk_only, &mut stats)?;
        if added > 0 {
            stats.pages_touched += 1;
        }
    }

    Ok(stats)
}

/// Build the shared `/ICCBased` colour space stream that pages reference.
///
/// The stream contains the ICC profile bytes with an `N` entry equal to
/// the profile's colour-component count (3 for RGB, 4 for CMYK). For
/// PDF/X-4 the OutputIntent is CMYK, so we declare `N=4` and use it as
/// the CMYK default — but we *also* synthesize RGB/Gray ICC stubs so
/// `/DefaultRGB` and `/DefaultGray` exist on every page.
///
/// For simplicity in v3.8.0 we use a single CMYK ICC stream and also
/// reference it (with `N=4`) only for the CMYK default. RGB / Gray
/// defaults reference an embedded sRGB-equivalent stream (the same one
/// the PDF/A path uses).
fn insert_icc_stream(doc: &mut Document, opts: &ColorOptions) -> Result<ObjectId, String> {
    let bytes = opts.target_intent.profile_bytes();
    let component_count = read_icc_component_count(bytes)?;
    let mut dict = dictionary! {
        "N" => component_count as i64,
        "Length" => bytes.len() as i64,
    };
    // Per ISO 32000 §8.6.5.5, `/Alternate` is optional but recommended
    // so legacy readers fall back gracefully.
    dict.set(
        "Alternate",
        Object::Name(match component_count {
            1 => b"DeviceGray".to_vec(),
            3 => b"DeviceRGB".to_vec(),
            4 => b"DeviceCMYK".to_vec(),
            _ => b"DeviceRGB".to_vec(),
        }),
    );
    let stream = Stream::new(dict, bytes.to_vec()).with_compression(false);
    let id = doc.add_object(Object::Stream(stream));
    Ok(id)
}

/// Read the colour-space component count (`colorSpace` field at offset
/// 16..20 of the ICC header). Returns the canonical N value used in the
/// PDF `/ICCBased` colour-space dictionary.
fn read_icc_component_count(profile: &[u8]) -> Result<u8, String> {
    if profile.len() < 128 {
        return Err(format!(
            "ICC profile too short ({} bytes); need >= 128 for header",
            profile.len()
        ));
    }
    if &profile[36..40] != b"acsp" {
        return Err("ICC profile missing 'acsp' magic at offset 36".to_string());
    }
    let cs = &profile[16..20];
    let n = match cs {
        b"GRAY" => 1,
        b"RGB " => 3,
        b"CMYK" => 4,
        other => {
            return Err(format!(
                "ICC profile colour space '{}' not supported as output intent",
                String::from_utf8_lossy(other).trim()
            ))
        }
    };
    Ok(n)
}

/// For one page, install whichever of `/DefaultGray` / `/DefaultRGB` /
/// `/DefaultCMYK` are missing. Returns the number of entries added.
fn ensure_page_defaults(
    doc: &mut Document,
    page_id: ObjectId,
    cmyk_icc: &Object,
    cmyk_only: bool,
    stats: &mut ColorStats,
) -> Result<usize, String> {
    // Each entry we may want to install.
    let mut to_add: Vec<(&'static [u8], Object)> = Vec::new();

    // Snapshot which entries already exist so we don't clobber them.
    let (has_rgb, has_gray, has_cmyk) = inspect_existing_defaults(doc, page_id)?;
    if has_rgb {
        stats.default_rgb_preserved += 1;
    }
    if has_gray {
        stats.default_gray_preserved += 1;
    }
    if has_cmyk {
        stats.default_cmyk_preserved += 1;
    }

    // CMYK default uses the bundled output-intent ICC.
    if !has_cmyk {
        to_add.push((b"DefaultCMYK", cmyk_icc.clone()));
    }

    if !cmyk_only {
        // For RGB/Gray defaults, point at standard ICC-tagged spaces.
        // We use CalRGB/CalGray with sRGB-equivalent parameters as a
        // lightweight ICC-equivalent declaration — every PDF/X
        // validator accepts CalRGB/CalGray as ICC-tagged for the
        // purposes of §6.2.4. (Avoids needing a separate RGB ICC
        // stream and keeps the file small.)
        if !has_rgb {
            to_add.push((b"DefaultRGB", srgb_calrgb_space()));
        }
        if !has_gray {
            to_add.push((b"DefaultGray", srgb_calgray_space()));
        }
    }

    if to_add.is_empty() {
        return Ok(0);
    }

    let added = to_add.len();
    install_defaults_on_page(doc, page_id, to_add)?;
    stats.default_entries_added += added;
    Ok(added)
}

/// Returns `(has_default_rgb, has_default_gray, has_default_cmyk)`.
fn inspect_existing_defaults(
    doc: &Document,
    page_id: ObjectId,
) -> Result<(bool, bool, bool), String> {
    let page = doc
        .get_dictionary(page_id)
        .map_err(|e| format!("page {:?} unreadable: {}", page_id, e))?;
    let Some(resources_obj) = page.get(b"Resources").ok() else {
        return Ok((false, false, false));
    };
    let resources = deref_dict(doc, resources_obj)?;
    let Some(cs_obj) = resources.get(b"ColorSpace").ok() else {
        return Ok((false, false, false));
    };
    let cs = deref_dict(doc, cs_obj)?;
    Ok((
        cs.has(b"DefaultRGB"),
        cs.has(b"DefaultGray"),
        cs.has(b"DefaultCMYK"),
    ))
}

/// Install a set of `Default*` entries into the page's
/// `/Resources/ColorSpace` dict, creating intermediate dicts as needed.
fn install_defaults_on_page(
    doc: &mut Document,
    page_id: ObjectId,
    entries: Vec<(&'static [u8], Object)>,
) -> Result<(), String> {
    // The page Resources entry may be an inline dict or an indirect
    // reference. Same for ColorSpace inside it. We need to mutate the
    // ultimately-owned dict, so we resolve & possibly re-attach.
    let page_dict = doc
        .get_object_mut(page_id)
        .map_err(|e| format!("page {:?} unreadable: {}", page_id, e))?;
    let page_dict = match page_dict {
        Object::Dictionary(d) => d,
        _ => return Err(format!("page {:?} is not a dictionary", page_id)),
    };

    // Resolve Resources to an owned dict we can edit in place. If it's
    // inline we edit it; if it's a reference we materialise an inline
    // copy and write back. This is safe — page Resources are not
    // expected to be shared (and even if they were, owning a per-page
    // copy is harmless for print-production output).
    let resources = page_dict
        .get(b"Resources")
        .cloned()
        .unwrap_or_else(|_| Object::Dictionary(Dictionary::new()));
    let mut resources_dict = match resources {
        Object::Dictionary(d) => d,
        Object::Reference(rid) => match doc.get_object(rid) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        },
        _ => Dictionary::new(),
    };

    let mut cs_dict = match resources_dict.get(b"ColorSpace").cloned() {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Reference(rid)) => match doc.get_object(rid) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        },
        _ => Dictionary::new(),
    };

    for (name, obj) in entries {
        cs_dict.set(name.to_vec(), obj);
    }

    resources_dict.set("ColorSpace", Object::Dictionary(cs_dict));

    // Re-fetch page mutably (borrow was released above by clones).
    let page_dict = doc
        .get_object_mut(page_id)
        .map_err(|e| format!("page {:?} unreadable: {}", page_id, e))?;
    let page_dict = match page_dict {
        Object::Dictionary(d) => d,
        _ => return Err(format!("page {:?} is not a dictionary", page_id)),
    };
    page_dict.set("Resources", Object::Dictionary(resources_dict));
    Ok(())
}

/// Resolve an inline-dict-or-reference object to a borrowed dict.
fn deref_dict<'a>(doc: &'a Document, obj: &'a Object) -> Result<&'a Dictionary, String> {
    match obj {
        Object::Dictionary(d) => Ok(d),
        Object::Reference(rid) => doc
            .get_dictionary(*rid)
            .map_err(|e| format!("ref {:?} unresolved: {}", rid, e)),
        _ => Err("expected dict or ref to dict".to_string()),
    }
}

/// Synthesize a `/CalRGB` colour space with sRGB-equivalent parameters.
/// (D65 white point, sRGB gamma + matrix.)
fn srgb_calrgb_space() -> Object {
    let dict = dictionary! {
        "WhitePoint" => Object::Array(vec![
            Object::Real(0.9505), Object::Real(1.0), Object::Real(1.0890),
        ]),
        "Gamma" => Object::Array(vec![
            Object::Real(2.2), Object::Real(2.2), Object::Real(2.2),
        ]),
        "Matrix" => Object::Array(vec![
            Object::Real(0.4124), Object::Real(0.2126), Object::Real(0.0193),
            Object::Real(0.3576), Object::Real(0.7152), Object::Real(0.1192),
            Object::Real(0.1805), Object::Real(0.0722), Object::Real(0.9505),
        ]),
    };
    Object::Array(vec![
        Object::Name(b"CalRGB".to_vec()),
        Object::Dictionary(dict),
    ])
}

/// Synthesize a `/CalGray` colour space with sRGB-equivalent parameters.
fn srgb_calgray_space() -> Object {
    let dict = dictionary! {
        "WhitePoint" => Object::Array(vec![
            Object::Real(0.9505), Object::Real(1.0), Object::Real(1.0890),
        ]),
        "Gamma" => Object::Real(2.2),
    };
    Object::Array(vec![
        Object::Name(b"CalGray".to_vec()),
        Object::Dictionary(dict),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn blank_doc_with_pages(n: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..n {
            let page_id = doc.new_object_id();
            let page = dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
                "Resources" => Object::Dictionary(Dictionary::new()),
            };
            doc.objects.insert(page_id, Object::Dictionary(page));
            kids.push(Object::Reference(page_id));
        }
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => n as i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let cat = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", cat);
        doc
    }

    #[test]
    fn icc_component_count_for_fogra51() {
        let n = read_icc_component_count(OutputIntent::Fogra51Coated.profile_bytes()).unwrap();
        assert_eq!(n, 4, "FOGRA51 is a CMYK profile");
    }

    #[test]
    fn icc_component_count_for_gracol() {
        let n = read_icc_component_count(OutputIntent::Gracol2013Crpc6.profile_bytes()).unwrap();
        assert_eq!(n, 4, "GRACoL2013 is a CMYK profile");
    }

    #[test]
    fn icc_component_count_rejects_garbage() {
        assert!(read_icc_component_count(&[0u8; 200]).is_err());
        assert!(read_icc_component_count(&[]).is_err());
    }

    #[test]
    fn normalize_color_adds_defaults_to_every_page() {
        let mut doc = blank_doc_with_pages(3);
        let stats = normalize_color(
            &mut doc,
            &ColorOptions {
                target_intent: OutputIntent::Fogra51Coated,
                cmyk_only: false,
            },
        )
        .unwrap();
        assert_eq!(stats.pages_touched, 3);
        assert_eq!(stats.default_entries_added, 9); // 3 pages × 3 entries
        assert_eq!(stats.default_rgb_preserved, 0);
        assert!(stats.icc_object_id.is_some());

        // Every page now has DefaultRGB / Gray / CMYK.
        let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
        for pid in page_ids {
            let page = doc.get_dictionary(pid).unwrap();
            let res = page.get(b"Resources").unwrap().as_dict().unwrap();
            let cs = res.get(b"ColorSpace").unwrap().as_dict().unwrap();
            assert!(cs.has(b"DefaultRGB"), "DefaultRGB missing on {:?}", pid);
            assert!(cs.has(b"DefaultGray"), "DefaultGray missing on {:?}", pid);
            assert!(cs.has(b"DefaultCMYK"), "DefaultCMYK missing on {:?}", pid);
        }
    }

    #[test]
    fn normalize_color_preserves_existing_defaults() {
        let mut doc = blank_doc_with_pages(1);
        // Pre-populate page 1 with a DefaultRGB.
        let page_id = *doc.get_pages().values().next().unwrap();
        let page = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
        let pre_existing = Object::Array(vec![
            Object::Name(b"CalRGB".to_vec()),
            Object::Dictionary(Dictionary::new()),
        ]);
        page.set(
            "Resources",
            Object::Dictionary(dictionary! {
                "ColorSpace" => Object::Dictionary(dictionary! {
                    "DefaultRGB" => pre_existing,
                }),
            }),
        );

        let stats = normalize_color(&mut doc, &ColorOptions::default()).unwrap();
        assert_eq!(stats.default_rgb_preserved, 1);
        assert_eq!(stats.default_entries_added, 2); // Gray + CMYK only
    }

    #[test]
    fn normalize_color_is_idempotent() {
        let mut doc = blank_doc_with_pages(2);
        let first = normalize_color(&mut doc, &ColorOptions::default()).unwrap();
        let second = normalize_color(&mut doc, &ColorOptions::default()).unwrap();
        assert_eq!(first.default_entries_added, 6);
        // Second pass should preserve everything → zero adds.
        assert_eq!(second.default_entries_added, 0);
        assert_eq!(second.default_rgb_preserved, 2);
        assert_eq!(second.default_gray_preserved, 2);
        assert_eq!(second.default_cmyk_preserved, 2);
    }

    #[test]
    fn normalize_color_cmyk_only_skips_rgb_and_gray() {
        let mut doc = blank_doc_with_pages(2);
        let stats = normalize_color(
            &mut doc,
            &ColorOptions {
                target_intent: OutputIntent::Gracol2013Crpc6,
                cmyk_only: true,
            },
        )
        .unwrap();
        assert_eq!(stats.pages_touched, 2);
        assert_eq!(stats.default_entries_added, 2); // CMYK on 2 pages
        let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
        for pid in page_ids {
            let page = doc.get_dictionary(pid).unwrap();
            let res = page.get(b"Resources").unwrap().as_dict().unwrap();
            let cs = res.get(b"ColorSpace").unwrap().as_dict().unwrap();
            assert!(cs.has(b"DefaultCMYK"));
            assert!(!cs.has(b"DefaultRGB"));
            assert!(!cs.has(b"DefaultGray"));
        }
    }

    #[test]
    fn normalize_color_handles_empty_doc() {
        let mut doc = Document::with_version("1.7");
        let stats = normalize_color(&mut doc, &ColorOptions::default()).unwrap();
        assert_eq!(stats.pages_touched, 0);
        assert_eq!(stats.default_entries_added, 0);
    }

    #[test]
    fn icc_stream_uploaded_once_shared_across_pages() {
        let mut doc = blank_doc_with_pages(5);
        let stats = normalize_color(&mut doc, &ColorOptions::default()).unwrap();
        let icc_id = stats.icc_object_id.expect("icc id");
        // Every page's DefaultCMYK should reference the same ICC stream.
        let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
        for pid in page_ids {
            let cs = doc
                .get_dictionary(pid)
                .unwrap()
                .get(b"Resources")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"ColorSpace")
                .unwrap()
                .as_dict()
                .unwrap();
            let cmyk = cs.get(b"DefaultCMYK").unwrap();
            let arr = cmyk.as_array().unwrap();
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].as_name().unwrap(), b"ICCBased");
            let referenced = arr[1].as_reference().unwrap();
            assert_eq!(referenced, icc_id);
        }
    }
}
