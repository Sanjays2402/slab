//! v3.6.0 Compactor — real PDF size reduction.
//!
//! Distinct from `pdf::compress` (which only re-flates uncompressed streams
//! and saves <2% on a scanned legal PDF). The Compactor's job is the actual
//! "Reduce File Size" feature Adobe Acrobat Pro charges $239/yr for:
//!
//! * Walk every image XObject (`/Subtype /Image`).
//! * Downsample to the preset's target DPI (Screen 72 / eBook 150 /
//!   Printer 300 / Prepress 300, with quality and mono-DPI variants).
//! * Re-encode color/grayscale as JPEG; keep monochrome as-is for now.
//! * Optionally drop `/Thumb` entries, `/Metadata` (XMP), embedded files,
//!   and JS / `/AA` actions.
//! * Run lopdf's stream re-flate at the end so dropped content actually
//!   shrinks the output.
//!
//! Pipeline: `list_image_xobjects` (read-only) → `estimate` (dry-run with
//! a quality-factor heuristic) → `compact` (real decode/resize/re-encode
//! and write).

use lopdf::{dictionary, Document, Object};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ImageRef + walker
// ---------------------------------------------------------------------------

/// One image XObject we may want to recompress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub object_id_num: u32,
    pub object_id_gen: u16,
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    /// `"DCTDecode"`, `"FlateDecode"`, `"CCITTFaxDecode"`, `"JBIG2Decode"`,
    /// `"JPXDecode"`, etc. `None` means the stream had no `/Filter`.
    pub filter: Option<String>,
    /// `"DeviceRGB"`, `"DeviceGray"`, `"DeviceCMYK"`, `"Indexed"`,
    /// `"ICCBased"`, etc. `None` if not a Name / first array element.
    pub color_space: Option<String>,
    /// On-disk byte length of the (encoded) stream.
    pub byte_size: usize,
}

fn first_name(obj: &Object) -> Option<String> {
    match obj {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::Array(a) => a.first().and_then(|x| {
            x.as_name()
                .ok()
                .map(|n| String::from_utf8_lossy(n).into_owned())
        }),
        _ => None,
    }
}

/// Enumerate every image XObject in `doc`. Pure / read-only.
pub fn list_image_xobjects(doc: &Document) -> Vec<ImageRef> {
    let mut out = Vec::new();
    for (id, obj) in &doc.objects {
        let Ok(stream) = obj.as_stream() else {
            continue;
        };
        let dict = &stream.dict;
        let subtype = dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok());
        if subtype != Some(b"Image".as_ref()) {
            continue;
        }
        let width = dict
            .get(b"Width")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0) as u32;
        let height = dict
            .get(b"Height")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0) as u32;
        let bpc = dict
            .get(b"BitsPerComponent")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(8) as u8;
        let filter = dict.get(b"Filter").ok().and_then(first_name);
        let color_space = dict.get(b"ColorSpace").ok().and_then(first_name);
        out.push(ImageRef {
            object_id_num: id.0,
            object_id_gen: id.1,
            width,
            height,
            bits_per_component: bpc,
            filter,
            color_space,
            byte_size: stream.content.len(),
        });
    }
    out
}

/// Sum of encoded image bytes.
pub fn total_image_bytes(refs: &[ImageRef]) -> u64 {
    refs.iter().map(|r| r.byte_size as u64).sum()
}

// ---------------------------------------------------------------------------
// Presets + options
// ---------------------------------------------------------------------------

/// Ghostscript-equivalent quality presets.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompactPreset {
    /// 72 DPI color/gray, JPEG q=60.  Smallest output.
    Screen,
    /// 150 DPI color/gray, JPEG q=75.  Sweet spot for most legal/business PDFs.
    Ebook,
    /// 300 DPI color/gray, JPEG q=85.  Safe for laser printers.
    Printer,
    /// 300 DPI color/gray, JPEG q=90.  Minimal loss; keeps metadata + JS.
    Prepress,
    /// User-supplied `CompactOptions`.
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactOptions {
    pub target_color_dpi: u32,
    pub target_gray_dpi: u32,
    pub target_mono_dpi: u32,
    /// 1..=100. JPEG encoder quality for color/gray rewrites.
    pub jpeg_quality: u8,
    pub drop_thumbnails: bool,
    pub strip_metadata: bool,
    pub strip_embedded_files: bool,
    pub strip_js: bool,
    /// Assumed page render DPI when no `cm` transform is available.
    pub assumed_page_dpi: u32,
}

impl Default for CompactOptions {
    fn default() -> Self {
        Self::from_preset(CompactPreset::Ebook)
    }
}

impl CompactOptions {
    pub fn from_preset(p: CompactPreset) -> Self {
        match p {
            CompactPreset::Screen => Self {
                target_color_dpi: 72,
                target_gray_dpi: 72,
                target_mono_dpi: 300,
                jpeg_quality: 60,
                drop_thumbnails: true,
                strip_metadata: true,
                strip_embedded_files: true,
                strip_js: true,
                assumed_page_dpi: 150,
            },
            CompactPreset::Ebook => Self {
                target_color_dpi: 150,
                target_gray_dpi: 150,
                target_mono_dpi: 300,
                jpeg_quality: 75,
                drop_thumbnails: true,
                strip_metadata: true,
                strip_embedded_files: false,
                strip_js: true,
                assumed_page_dpi: 150,
            },
            CompactPreset::Printer => Self {
                target_color_dpi: 300,
                target_gray_dpi: 300,
                target_mono_dpi: 1200,
                jpeg_quality: 85,
                drop_thumbnails: true,
                strip_metadata: false,
                strip_embedded_files: false,
                strip_js: true,
                assumed_page_dpi: 300,
            },
            CompactPreset::Prepress => Self {
                target_color_dpi: 300,
                target_gray_dpi: 300,
                target_mono_dpi: 1200,
                jpeg_quality: 90,
                drop_thumbnails: false,
                strip_metadata: false,
                strip_embedded_files: false,
                strip_js: false,
                assumed_page_dpi: 300,
            },
            CompactPreset::Custom => Self {
                target_color_dpi: 150,
                target_gray_dpi: 150,
                target_mono_dpi: 300,
                jpeg_quality: 75,
                drop_thumbnails: true,
                strip_metadata: true,
                strip_embedded_files: false,
                strip_js: true,
                assumed_page_dpi: 150,
            },
        }
    }
}

fn target_pixel_width(opts: &CompactOptions, img: &ImageRef) -> u32 {
    let dpi = if img.bits_per_component == 1 {
        opts.target_mono_dpi
    } else if img.color_space.as_deref() == Some("DeviceGray") {
        opts.target_gray_dpi
    } else {
        opts.target_color_dpi
    };
    // 8.5 in page width is the safe upper estimate (US Letter).
    (dpi as f32 * 8.5).round() as u32
}

// ---------------------------------------------------------------------------
// Estimate (dry-run)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEstimate {
    pub object_id_num: u32,
    pub original_bytes: usize,
    pub projected_bytes: usize,
    pub will_resample: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimateReport {
    pub original_bytes: u64,
    pub projected_bytes: u64,
    pub projected_ratio: f32,
    pub images_total: u32,
    pub images_resampled: u32,
    pub per_image: Vec<ImageEstimate>,
}

/// JPEG bytes-per-pixel heuristic (natural photo content, libjpeg-turbo).
/// Replaced by real encoded sizes once `compact()` runs.
fn jpeg_bytes_per_pixel(quality: u8) -> f32 {
    match quality {
        0..=40 => 0.10,
        41..=60 => 0.15,
        61..=75 => 0.25,
        76..=85 => 0.45,
        86..=92 => 0.75,
        _ => 1.20,
    }
}

fn filter_label(img: &ImageRef) -> String {
    img.filter.clone().unwrap_or_else(|| "raw".into())
}

pub fn estimate(
    input: &std::path::Path,
    opts: CompactOptions,
) -> Result<EstimateReport, crate::pdf::PdfError> {
    if !input.exists() {
        return Err(crate::pdf::PdfError::InputMissing(
            input.display().to_string(),
        ));
    }
    let original_bytes = std::fs::metadata(input)?.len();
    let doc = Document::load(input)?;
    let images = list_image_xobjects(&doc);

    let mut per_image = Vec::with_capacity(images.len());
    let mut total_image_orig: i64 = 0;
    let mut total_image_proj: i64 = 0;
    let mut resampled_count = 0u32;

    for img in &images {
        total_image_orig += img.byte_size as i64;
        let target_w = target_pixel_width(&opts, img);
        let (proj, will, reason) = if img.bits_per_component == 1 {
            if img.width > opts.target_mono_dpi * 9 {
                let new_w = target_w;
                let new_h = ((img.height as f32) * (new_w as f32) / (img.width.max(1) as f32))
                    .round() as u32;
                let bytes = (new_w as usize * new_h as usize) / 8 + 100;
                (
                    bytes,
                    true,
                    format!("mono {}x{} -> {}x{}", img.width, img.height, new_w, new_h),
                )
            } else {
                (img.byte_size, false, "mono already small".into())
            }
        } else if img.width <= target_w {
            if img.filter.as_deref() == Some("DCTDecode") {
                (img.byte_size, false, "already small + already JPEG".into())
            } else {
                let bpp = jpeg_bytes_per_pixel(opts.jpeg_quality);
                let bytes = (img.width as f32 * img.height as f32 * bpp / 8.0) as usize;
                (
                    bytes,
                    true,
                    format!(
                        "re-encode {} -> JPEG q{}",
                        filter_label(img),
                        opts.jpeg_quality
                    ),
                )
            }
        } else {
            let new_w = target_w;
            let new_h =
                ((img.height as f32) * (new_w as f32) / (img.width.max(1) as f32)).round() as u32;
            let bpp = jpeg_bytes_per_pixel(opts.jpeg_quality);
            let bytes = (new_w as f32 * new_h as f32 * bpp / 8.0) as usize;
            (
                bytes,
                true,
                format!(
                    "{}x{} -> {}x{} JPEG q{}",
                    img.width, img.height, new_w, new_h, opts.jpeg_quality
                ),
            )
        };
        total_image_proj += proj as i64;
        if will {
            resampled_count += 1;
        }
        per_image.push(ImageEstimate {
            object_id_num: img.object_id_num,
            original_bytes: img.byte_size,
            projected_bytes: proj,
            will_resample: will,
            reason,
        });
    }

    // Non-image savings — coarse heuristic.
    let mut non_image_savings: i64 = 0;
    if opts.drop_thumbnails {
        non_image_savings += 8_000;
    }
    if opts.strip_metadata {
        non_image_savings += 12_000;
    }
    if opts.strip_embedded_files {
        non_image_savings += 5_000;
    }
    if opts.strip_js {
        non_image_savings += 2_000;
    }
    let non_image_max = (original_bytes as i64) - total_image_orig;
    non_image_savings = non_image_savings.min(non_image_max.max(0));

    let projected_signed =
        original_bytes as i64 - (total_image_orig - total_image_proj) - non_image_savings;
    let projected_bytes = projected_signed.max(1) as u64;
    let projected_ratio = projected_bytes as f32 / original_bytes.max(1) as f32;

    Ok(EstimateReport {
        original_bytes,
        projected_bytes,
        projected_ratio,
        images_total: images.len() as u32,
        images_resampled: resampled_count,
        per_image,
    })
}

// ---------------------------------------------------------------------------
// Compact (real rewrite)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactReport {
    pub original_bytes: u64,
    pub new_bytes: u64,
    pub ratio: f32,
    pub images_total: u32,
    pub images_rewritten: u32,
    pub images_skipped: u32,
    pub bytes_saved_images: i64,
    pub thumbnails_dropped: u32,
    pub metadata_stripped: bool,
    pub embedded_files_stripped: bool,
    pub js_stripped: bool,
    pub warnings: Vec<String>,
}

/// Decode an image stream into a `DynamicImage`. Returns `None` for
/// unsupported filters (caller skips the image).
fn decode_image_stream(stream: &lopdf::Stream) -> Option<image::DynamicImage> {
    let filter = stream
        .dict
        .get(b"Filter")
        .ok()
        .and_then(first_name)
        .unwrap_or_default();
    match filter.as_str() {
        "DCTDecode" => {
            image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg).ok()
        }
        "FlateDecode" | "" => {
            let w = stream.dict.get(b"Width").ok()?.as_i64().ok()? as u32;
            let h = stream.dict.get(b"Height").ok()?.as_i64().ok()? as u32;
            let bpc = stream
                .dict
                .get(b"BitsPerComponent")
                .ok()
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(8) as u8;
            if bpc != 8 {
                return None;
            }
            let cs = stream
                .dict
                .get(b"ColorSpace")
                .ok()
                .and_then(first_name)
                .unwrap_or_else(|| "DeviceRGB".into());
            let raw = &stream.content;
            match cs.as_str() {
                "DeviceGray" => image::GrayImage::from_raw(w, h, raw.clone())
                    .map(image::DynamicImage::ImageLuma8),
                _ => {
                    image::RgbImage::from_raw(w, h, raw.clone()).map(image::DynamicImage::ImageRgb8)
                }
            }
        }
        _ => None,
    }
}

fn build_jpeg_image_stream(jpeg: Vec<u8>, w: u32, h: u32, is_gray: bool) -> lopdf::Stream {
    let cs: lopdf::Object = if is_gray {
        "DeviceGray".into()
    } else {
        "DeviceRGB".into()
    };
    let mut s = lopdf::Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "BitsPerComponent" => 8_i64,
            "ColorSpace" => cs,
            "Filter" => "DCTDecode",
        },
        jpeg,
    );
    // Don't let lopdf re-Flate our JPEG bytes on save.
    s.allows_compression = false;
    s
}

fn strip_thumbnails(doc: &mut Document) -> u32 {
    let page_ids: Vec<_> = doc.page_iter().collect();
    let mut count = 0u32;
    for pid in page_ids {
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(pid) {
            if d.has(b"Thumb") {
                d.remove(b"Thumb");
                count += 1;
            }
        }
    }
    count
}

fn root_id(doc: &Document) -> Option<lopdf::ObjectId> {
    doc.trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
}

fn strip_metadata(doc: &mut Document) -> bool {
    let Some(rid) = root_id(doc) else {
        return false;
    };
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(rid) {
        if d.has(b"Metadata") {
            d.remove(b"Metadata");
            return true;
        }
    }
    false
}

fn strip_embedded_files(doc: &mut Document) -> bool {
    let Some(rid) = root_id(doc) else {
        return false;
    };
    let Ok(Object::Dictionary(d)) = doc.get_object_mut(rid) else {
        return false;
    };
    if let Ok(Object::Dictionary(names)) = d.get_mut(b"Names") {
        if names.has(b"EmbeddedFiles") {
            names.remove(b"EmbeddedFiles");
            return true;
        }
    }
    false
}

fn strip_js(doc: &mut Document) -> bool {
    let Some(rid) = root_id(doc) else {
        return false;
    };
    let Ok(Object::Dictionary(d)) = doc.get_object_mut(rid) else {
        return false;
    };
    let mut any = false;
    if d.has(b"AA") {
        d.remove(b"AA");
        any = true;
    }
    if let Ok(Object::Dictionary(names)) = d.get_mut(b"Names") {
        if names.has(b"JavaScript") {
            names.remove(b"JavaScript");
            any = true;
        }
    }
    // OpenAction with /S /JavaScript → drop.
    let drop_oa = matches!(
        d.get(b"OpenAction"),
        Ok(Object::Dictionary(oa)) if oa.get(b"S").ok().and_then(|v| v.as_name().ok())
            == Some(b"JavaScript".as_ref())
    );
    if drop_oa {
        d.remove(b"OpenAction");
        any = true;
    }
    any
}

pub fn compact(
    input: &std::path::Path,
    output: &std::path::Path,
    opts: CompactOptions,
) -> Result<CompactReport, crate::pdf::PdfError> {
    if !input.exists() {
        return Err(crate::pdf::PdfError::InputMissing(
            input.display().to_string(),
        ));
    }
    let original_bytes = std::fs::metadata(input)?.len();
    let mut doc = Document::load(input)?;
    let mut warnings = Vec::new();

    let images = list_image_xobjects(&doc);
    let images_total = images.len() as u32;
    let mut rewritten = 0u32;
    let mut skipped = 0u32;
    let mut bytes_saved_images: i64 = 0;

    for img in &images {
        if img.bits_per_component == 1 {
            // Mono / CCITTFax left alone in v3.6.0 — follow-up tick.
            skipped += 1;
            continue;
        }
        let target_w = target_pixel_width(&opts, img);
        if img.width <= target_w && img.filter.as_deref() == Some("DCTDecode") {
            skipped += 1;
            continue;
        }
        let id = (img.object_id_num, img.object_id_gen);
        let stream_clone = match doc.objects.get(&id).and_then(|o| o.as_stream().ok()) {
            Some(s) => s.clone(),
            None => {
                skipped += 1;
                continue;
            }
        };
        let Some(decoded) = decode_image_stream(&stream_clone) else {
            warnings.push(format!(
                "skipped image obj {}: unsupported filter {}",
                img.object_id_num,
                filter_label(img)
            ));
            skipped += 1;
            continue;
        };

        let new_w = target_w.min(img.width);
        let new_h = ((img.height as f32) * (new_w as f32) / (img.width.max(1) as f32))
            .round()
            .max(1.0) as u32;
        let resized = if new_w < img.width || new_h < img.height {
            decoded.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle)
        } else {
            decoded
        };

        let is_gray = matches!(resized, image::DynamicImage::ImageLuma8(_));
        let mut buf = Vec::new();
        let encode_result = {
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, opts.jpeg_quality);
            if is_gray {
                let gray = resized.to_luma8();
                enc.encode(
                    gray.as_raw(),
                    gray.width(),
                    gray.height(),
                    image::ColorType::L8.into(),
                )
            } else {
                let rgb = resized.to_rgb8();
                enc.encode(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ColorType::Rgb8.into(),
                )
            }
        };
        if let Err(e) = encode_result {
            warnings.push(format!(
                "skipped image obj {}: jpeg encode failed ({e})",
                img.object_id_num
            ));
            skipped += 1;
            continue;
        }

        let old_size = stream_clone.content.len();
        let new_stream = build_jpeg_image_stream(buf.clone(), new_w, new_h, is_gray);
        doc.objects.insert(id, Object::Stream(new_stream));
        bytes_saved_images += old_size as i64 - buf.len() as i64;
        rewritten += 1;
    }

    let thumbnails_dropped = if opts.drop_thumbnails {
        strip_thumbnails(&mut doc)
    } else {
        0
    };
    let metadata_stripped = opts.strip_metadata && strip_metadata(&mut doc);
    let embedded_files_stripped = opts.strip_embedded_files && strip_embedded_files(&mut doc);
    let js_stripped = opts.strip_js && strip_js(&mut doc);

    doc.compress();

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.save(output)?;
    let new_bytes = std::fs::metadata(output)?.len();
    let ratio = new_bytes as f32 / original_bytes.max(1) as f32;

    Ok(CompactReport {
        original_bytes,
        new_bytes,
        ratio,
        images_total,
        images_rewritten: rewritten,
        images_skipped: skipped,
        bytes_saved_images,
        thumbnails_dropped,
        metadata_stripped,
        embedded_files_stripped,
        js_stripped,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::{make_n_page_pdf, make_pdf_with_image};

    #[test]
    fn list_image_xobjects_finds_one() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("i.pdf");
        make_pdf_with_image(&p, 600, 400);
        let doc = Document::load(&p).unwrap();
        let images = list_image_xobjects(&doc);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width, 600);
        assert_eq!(images[0].height, 400);
        assert_eq!(images[0].filter.as_deref(), Some("DCTDecode"));
        assert_eq!(images[0].color_space.as_deref(), Some("DeviceRGB"));
        assert_eq!(images[0].bits_per_component, 8);
        assert!(images[0].byte_size > 100);
    }

    #[test]
    fn list_image_xobjects_empty_for_text_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_n_page_pdf(&p, 2);
        let doc = Document::load(&p).unwrap();
        assert!(list_image_xobjects(&doc).is_empty());
    }

    #[test]
    fn total_image_bytes_sums_all_streams() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("i.pdf");
        make_pdf_with_image(&p, 800, 800);
        let doc = Document::load(&p).unwrap();
        let imgs = list_image_xobjects(&doc);
        let total = total_image_bytes(&imgs);
        assert!(total > 1000, "expected >1KB of image data, got {}", total);
        assert_eq!(total, imgs[0].byte_size as u64);
    }

    #[test]
    fn preset_screen_targets_72dpi_q60() {
        let o = CompactOptions::from_preset(CompactPreset::Screen);
        assert_eq!(o.target_color_dpi, 72);
        assert_eq!(o.jpeg_quality, 60);
        assert!(o.drop_thumbnails);
        assert!(o.strip_metadata);
    }

    #[test]
    fn preset_ebook_targets_150dpi_q75() {
        let o = CompactOptions::from_preset(CompactPreset::Ebook);
        assert_eq!(o.target_color_dpi, 150);
        assert_eq!(o.jpeg_quality, 75);
    }

    #[test]
    fn preset_prepress_preserves_metadata() {
        let o = CompactOptions::from_preset(CompactPreset::Prepress);
        assert!(!o.strip_metadata);
        assert!(!o.strip_js);
        assert!(!o.drop_thumbnails);
    }

    #[test]
    fn estimate_projects_savings_on_oversized_image() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("big.pdf");
        make_pdf_with_image(&p, 2400, 2400);
        let est = estimate(&p, CompactOptions::from_preset(CompactPreset::Screen)).unwrap();
        assert!(est.original_bytes > 10_000);
        assert!(
            est.projected_bytes < est.original_bytes,
            "Screen preset must project savings on 2400x2400: {} -> {}",
            est.original_bytes,
            est.projected_bytes
        );
        assert!(est.projected_ratio < 1.0);
        assert_eq!(est.images_total, 1);
        assert_eq!(est.images_resampled, 1);
    }

    #[test]
    fn estimate_text_only_pdf_no_image_savings() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.pdf");
        make_n_page_pdf(&p, 3);
        let est = estimate(&p, CompactOptions::from_preset(CompactPreset::Ebook)).unwrap();
        assert_eq!(est.images_total, 0);
        assert_eq!(est.images_resampled, 0);
    }

    #[test]
    fn compact_screen_preset_shrinks_image_pdf() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("big.pdf");
        let dst = tmp.path().join("small.pdf");
        make_pdf_with_image(&src, 2400, 2400);
        let original = std::fs::metadata(&src).unwrap().len();
        let report = compact(
            &src,
            &dst,
            CompactOptions::from_preset(CompactPreset::Screen),
        )
        .unwrap();
        let actual = std::fs::metadata(&dst).unwrap().len();
        assert_eq!(actual, report.new_bytes);
        assert_eq!(report.original_bytes, original);
        assert_eq!(report.images_rewritten, 1);
        assert!(
            report.new_bytes < report.original_bytes / 2,
            "Screen preset must at least halve 2400x2400 image PDF: {} -> {}",
            report.original_bytes,
            report.new_bytes
        );
        let out_doc = Document::load(&dst).unwrap();
        let imgs = list_image_xobjects(&out_doc);
        assert_eq!(imgs.len(), 1);
        assert!(
            imgs[0].width <= 700,
            "downsampled width should be <=~612 at 72dpi/8.5in, got {}",
            imgs[0].width
        );
    }

    #[test]
    fn compact_ebook_preset_preserves_page_count() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.pdf");
        let dst = tmp.path().join("b.pdf");
        make_pdf_with_image(&src, 1500, 1000);
        compact(
            &src,
            &dst,
            CompactOptions::from_preset(CompactPreset::Ebook),
        )
        .unwrap();
        assert_eq!(crate::pdf::split::page_count(&dst).unwrap(), 1);
    }

    #[test]
    fn compact_text_only_pdf_does_not_explode_size() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("t.pdf");
        let dst = tmp.path().join("o.pdf");
        make_n_page_pdf(&src, 3);
        let r = compact(
            &src,
            &dst,
            CompactOptions::from_preset(CompactPreset::Ebook),
        )
        .unwrap();
        assert_eq!(r.images_total, 0);
        assert_eq!(r.images_rewritten, 0);
        assert!(
            r.new_bytes < (r.original_bytes as f32 * 1.5) as u64,
            "text-only round-trip exploded: {} -> {}",
            r.original_bytes,
            r.new_bytes
        );
    }

    #[test]
    fn compact_output_is_valid_pdf() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a.pdf");
        let dst = tmp.path().join("b.pdf");
        make_pdf_with_image(&src, 1200, 800);
        compact(
            &src,
            &dst,
            CompactOptions::from_preset(CompactPreset::Ebook),
        )
        .unwrap();
        let doc = Document::load(&dst).unwrap();
        assert!(doc.page_iter().count() >= 1);
    }
}
