// TextRun extraction from a PDF.
//
// We walk each page's content stream(s), simulating the PDF text-state
// machine (PDF 1.7 §9.4), and emit one `TextRun` per `Tj` / `TJ` / `'` / `"`
// operator with its on-page baseline position, font, size, and weight.
//
// This is a deliberately small subset of a "real" PDF text engine — we only
// care about enough state to (a) place runs in 2D space accurately enough to
// cluster into lines, and (b) read the effective font + size so the layout
// pass can tell heading from body.
//
// What we model:
//   BT / ET           : enter/exit text object, reset Tm + Tlm to identity
//   Tf <font> <size>  : set font + base font size
//   Tm a b c d e f    : absolute text matrix (we use e/f for position, d for vertical scale)
//   Td tx ty          : translate Tlm by (tx, ty); reset Tm := Tlm
//   TD tx ty          : like Td but also sets leading TL := -ty
//   T*                : move to start of next line: Td(0, -TL); reset Tm := Tlm
//   TL                : set leading
//   Tc / Tw           : char / word spacing (we don't apply advance, just record)
//   Tj  <str>         : show string at current position
//   '   <str>         : T* then show string
//   "   aw ac <str>   : set Tw=aw, Tc=ac, then ' (T* then show)
//   TJ  <array>       : show array of strings/kerning; we concatenate strings
//
// We do NOT compute true glyph advance widths — for the heading / paragraph /
// table layout pass, the baseline (x, y) of each *string* is enough.

use std::collections::HashMap;

use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};

use super::errors::ReflowError;
use super::types::TextRun;

/// Extract all positioned `TextRun`s from every page of `doc`, in page order.
pub fn extract_text_runs(doc: &Document) -> Result<Vec<TextRun>, ReflowError> {
    let mut out: Vec<TextRun> = Vec::new();
    let pages = doc.get_pages();
    // Sort by page number so callers get deterministic output regardless of
    // the order lopdf's HashMap happens to yield.
    let mut ordered: Vec<(u32, ObjectId)> = pages.into_iter().collect();
    ordered.sort_by_key(|(n, _)| *n);
    for (page_num, page_id) in ordered {
        let fonts = resolve_page_fonts(doc, page_id);
        let runs = extract_page_runs(doc, page_id, page_num, &fonts)?;
        out.extend(runs);
    }
    Ok(out)
}

/// Extract `TextRun`s for a single page.
pub fn extract_page_runs(
    doc: &Document,
    page_id: ObjectId,
    page_num: u32,
    fonts: &HashMap<String, FontInfo>,
) -> Result<Vec<TextRun>, ReflowError> {
    let mut runs: Vec<TextRun> = Vec::new();
    for sid in content_stream_ids(doc, page_id) {
        let Some(bytes) = decoded_stream_bytes(doc, sid) else {
            continue;
        };
        let Ok(content) = Content::decode(&bytes) else {
            continue;
        };
        walk_content_stream(&content, page_num, fonts, &mut runs);
    }
    Ok(runs)
}

/// Public for tests + reuse by other modules — minimal font descriptor.
#[derive(Debug, Clone, Default)]
pub struct FontInfo {
    pub base_name: String,
    pub bold: bool,
    pub italic: bool,
}

impl FontInfo {
    pub fn from_base_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        let bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
        let italic = lower.contains("italic") || lower.contains("oblique");
        Self {
            base_name: name.to_string(),
            bold,
            italic,
        }
    }
}

fn walk_content_stream(
    content: &Content,
    page_num: u32,
    fonts: &HashMap<String, FontInfo>,
    out: &mut Vec<TextRun>,
) {
    let mut tm = Matrix::identity();
    let mut tlm = Matrix::identity();
    let mut leading: f32 = 0.0;
    let mut font_res = String::new();
    let mut font_size: f32 = 0.0;
    let mut in_text = false;

    for op in &content.operations {
        match op.operator.as_str() {
            "BT" => {
                in_text = true;
                tm = Matrix::identity();
                tlm = Matrix::identity();
            }
            "ET" => {
                in_text = false;
            }
            "Tf" if op.operands.len() == 2 => {
                if let Object::Name(n) = &op.operands[0] {
                    font_res = String::from_utf8_lossy(n).into_owned();
                }
                font_size = num(&op.operands[1]);
            }
            "TL" if op.operands.len() == 1 => {
                leading = num(&op.operands[0]);
            }
            "Td" if in_text && op.operands.len() == 2 => {
                let tx = num(&op.operands[0]);
                let ty = num(&op.operands[1]);
                tlm = Matrix::translation(tx, ty).mul(&tlm);
                tm = tlm;
            }
            "TD" if in_text && op.operands.len() == 2 => {
                let tx = num(&op.operands[0]);
                let ty = num(&op.operands[1]);
                leading = -ty;
                tlm = Matrix::translation(tx, ty).mul(&tlm);
                tm = tlm;
            }
            "Tm" if in_text && op.operands.len() == 6 => {
                tm = Matrix {
                    a: num(&op.operands[0]),
                    b: num(&op.operands[1]),
                    c: num(&op.operands[2]),
                    d: num(&op.operands[3]),
                    e: num(&op.operands[4]),
                    f: num(&op.operands[5]),
                };
                tlm = tm;
            }
            "T*" if in_text => {
                tlm = Matrix::translation(0.0, -leading).mul(&tlm);
                tm = tlm;
            }
            "Tj" if in_text && op.operands.len() == 1 => {
                if let Some(text) = string_operand(&op.operands[0]) {
                    push_run(out, page_num, &tm, font_size, &font_res, fonts, text);
                }
            }
            "'" if in_text && !op.operands.is_empty() => {
                tlm = Matrix::translation(0.0, -leading).mul(&tlm);
                tm = tlm;
                if let Some(text) = string_operand(&op.operands[0]) {
                    push_run(out, page_num, &tm, font_size, &font_res, fonts, text);
                }
            }
            "\"" if in_text && op.operands.len() == 3 => {
                tlm = Matrix::translation(0.0, -leading).mul(&tlm);
                tm = tlm;
                if let Some(text) = string_operand(&op.operands[2]) {
                    push_run(out, page_num, &tm, font_size, &font_res, fonts, text);
                }
            }
            "TJ" if in_text && op.operands.len() == 1 => {
                if let Object::Array(arr) = &op.operands[0] {
                    let mut buf = String::new();
                    for it in arr {
                        if let Some(s) = string_operand(it) {
                            buf.push_str(&s);
                        }
                    }
                    if !buf.is_empty() {
                        push_run(out, page_num, &tm, font_size, &font_res, fonts, buf);
                    }
                }
            }
            _ => {}
        }
    }
}

fn push_run(
    out: &mut Vec<TextRun>,
    page: u32,
    tm: &Matrix,
    font_size: f32,
    font_res: &str,
    fonts: &HashMap<String, FontInfo>,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    let info = fonts.get(font_res).cloned().unwrap_or_default();
    // Effective font size = Tf size * |Tm.d|. (Vertical scale of the text matrix.)
    let eff_size = font_size * tm.d.abs().max(1e-6);
    out.push(TextRun {
        page,
        x: tm.e,
        y: tm.f,
        text,
        font_name: if info.base_name.is_empty() {
            font_res.to_string()
        } else {
            info.base_name
        },
        font_size: eff_size,
        bold: info.bold,
        italic: info.italic,
    });
}

fn string_operand(o: &Object) -> Option<String> {
    match o {
        Object::String(bytes, _) => {
            // Best-effort PDFDocEncoding-ish: ASCII-passthrough, otherwise
            // lossy UTF-8 — good enough for v1 heading/paragraph clustering.
            // (Tasks 5+ may upgrade to ToUnicode CMaps.)
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        _ => None,
    }
}

fn num(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

/// 3x3 affine matrix in PDF row-major form (the bottom row is implicit 0 0 1).
#[derive(Debug, Clone, Copy)]
struct Matrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
    fn translation(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }
    /// `self * rhs`, returning the composition (apply rhs first, then self).
    fn mul(&self, rhs: &Matrix) -> Matrix {
        Matrix {
            a: self.a * rhs.a + self.b * rhs.c,
            b: self.a * rhs.b + self.b * rhs.d,
            c: self.c * rhs.a + self.d * rhs.c,
            d: self.c * rhs.b + self.d * rhs.d,
            e: self.e * rhs.a + self.f * rhs.c + rhs.e,
            f: self.e * rhs.b + self.f * rhs.d + rhs.f,
        }
    }
}

fn content_stream_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut ids: Vec<ObjectId> = Vec::new();
    let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return ids;
    };
    let Ok(contents) = page.get(b"Contents") else {
        return ids;
    };
    match contents {
        Object::Reference(r) => ids.push(*r),
        Object::Array(a) => {
            for it in a {
                if let Object::Reference(r) = it {
                    ids.push(*r);
                }
            }
        }
        _ => {}
    }
    ids
}

fn decoded_stream_bytes(doc: &Document, stream_id: ObjectId) -> Option<Vec<u8>> {
    let obj = doc.get_object(stream_id).ok()?;
    let stream = obj.as_stream().ok()?;
    if let Ok(decoded) = stream.decompressed_content() {
        Some(decoded)
    } else {
        Some(stream.content.clone())
    }
}

/// Resolve the `/Font` resource dictionary on this page into a map of
/// `Tf` resource name → `FontInfo` (BaseFont parsed for bold/italic).
pub fn resolve_page_fonts(doc: &Document, page_id: ObjectId) -> HashMap<String, FontInfo> {
    let mut out: HashMap<String, FontInfo> = HashMap::new();
    let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return out;
    };
    // Inherit `/Resources` if missing — lopdf provides `get_inherited` for this.
    let resources = page
        .get(b"Resources")
        .ok()
        .and_then(|o| match o {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(r) => doc
                .get_object(*r)
                .ok()
                .and_then(|x| x.as_dict().ok().cloned()),
            _ => None,
        })
        .or_else(|| doc.get_dict_in_dict(page, b"Resources").ok().cloned());
    let Some(resources) = resources else {
        return out;
    };
    let fonts_dict = match resources.get(b"Font") {
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .ok()
            .and_then(|x| x.as_dict().ok().cloned()),
        _ => None,
    };
    let Some(fonts_dict) = fonts_dict else {
        return out;
    };
    for (name_bytes, entry) in fonts_dict.iter() {
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        let font_obj = match entry {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(r) => doc
                .get_object(*r)
                .ok()
                .and_then(|x| x.as_dict().ok().cloned()),
            _ => None,
        };
        let Some(font_obj) = font_obj else { continue };
        let base = match font_obj.get(b"BaseFont") {
            Ok(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
            _ => String::new(),
        };
        out.insert(name, FontInfo::from_base_name(&base));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_info_detects_bold_italic_from_base_name() {
        let f = FontInfo::from_base_name("Helvetica-Bold");
        assert!(f.bold && !f.italic);
        let f = FontInfo::from_base_name("Times-Italic");
        assert!(!f.bold && f.italic);
        let f = FontInfo::from_base_name("Arial-BoldItalic");
        assert!(f.bold && f.italic);
        let f = FontInfo::from_base_name("Helvetica");
        assert!(!f.bold && !f.italic);
    }

    #[test]
    fn matrix_translation_composes_correctly() {
        let a = Matrix::translation(10.0, 20.0);
        let b = Matrix::translation(3.0, 4.0);
        // a * b means: apply b first, then a (column-vector convention).
        let c = a.mul(&b);
        assert!((c.e - 13.0).abs() < 1e-5);
        assert!((c.f - 24.0).abs() < 1e-5);
    }

    /// Build a minimal one-page PDF in memory containing the literal "Hello World"
    /// placed at (100, 700) in 12pt Helvetica, then extract and assert.
    #[test]
    fn extract_text_runs_finds_hello_world() {
        use lopdf::content::Operation;
        use lopdf::{dictionary, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![100.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(
                    b"Hello World".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ];
        let content = Content { operations: ops };
        let content_bytes = content.encode().unwrap();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let runs = extract_text_runs(&doc).unwrap();
        assert_eq!(runs.len(), 1, "expected exactly 1 run, got {:?}", runs);
        let r = &runs[0];
        assert_eq!(r.text, "Hello World");
        assert!((r.x - 100.0).abs() < 0.5, "x was {}", r.x);
        assert!((r.y - 700.0).abs() < 0.5, "y was {}", r.y);
        assert!((r.font_size - 12.0).abs() < 0.01);
        assert_eq!(r.font_name, "Helvetica");
        assert!(!r.bold && !r.italic);
        assert_eq!(r.page, 1);
    }

    #[test]
    fn extract_handles_tm_absolute_position() {
        use lopdf::content::Operation;
        use lopdf::{dictionary, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Times-Bold",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 18.into()]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    50.into(),
                    750.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::String(
                    b"Chapter 1".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ];
        let content_bytes = Content { operations: ops }.encode().unwrap();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let runs = extract_text_runs(&doc).unwrap();
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.text, "Chapter 1");
        assert!((r.x - 50.0).abs() < 0.5);
        assert!((r.y - 750.0).abs() < 0.5);
        assert!((r.font_size - 18.0).abs() < 0.01);
        assert!(r.bold, "Times-Bold base name should mark run as bold");
    }

    #[test]
    fn extract_handles_tj_array_concatenation() {
        use lopdf::content::Operation;
        use lopdf::{dictionary, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        // TJ [(Hel) -50 (lo) -100 ( World)]
        let arr = Object::Array(vec![
            Object::String(b"Hel".to_vec(), lopdf::StringFormat::Literal),
            (-50).into(),
            Object::String(b"lo".to_vec(), lopdf::StringFormat::Literal),
            (-100).into(),
            Object::String(b" World".to_vec(), lopdf::StringFormat::Literal),
        ]);
        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 10.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new("TJ", vec![arr]),
            Operation::new("ET", vec![]),
        ];
        let content_bytes = Content { operations: ops }.encode().unwrap();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let runs = extract_text_runs(&doc).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello World");
    }
}
