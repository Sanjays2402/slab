// src-tauri/src/pdf/loom/layout.rs
//
// Slab Loom — Slice 1: LayoutTree extraction.
//
// Goal: pure, side-effect-free transform from PDF bytes to a `LayoutTree`
// — every text run and image XObject placement on every page, with bbox +
// font size + font name. This is the input to the segments / classify
// stages later in the pipeline.
//
// Why not reuse `pdf::extract::text`? That collapses everything into a
// linear string. For PDF/UA tagging we need the geometry: heading detection
// is a font-size + position problem, reading order is a column-detection
// problem, figure detection needs image bboxes.
//
// What we deliberately do NOT do here:
//   * font glyph metrics — we approximate widths from the font size and
//     character count. The downstream segment grouper only needs relative
//     positions, not pixel-perfect bboxes. When we ship Slice 3 (column
//     detection) we can swap in real font metrics if needed.
//   * CMap decoding for CID / Type0 fonts — text drawn with these fonts is
//     emitted as the raw byte string with `font_name` left empty, and
//     downstream stages can fall back to OCR for those runs.
//   * clip / mask interaction — we treat every Do XObject Image as a
//     placement at the current CTM, no occlusion analysis.

use lopdf::{
    content::{Content, Operation},
    Document, Object, ObjectId,
};
use serde::{Deserialize, Serialize};

/// Bounding box in PDF user space (origin lower-left, y grows upward).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Bbox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Bbox {
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
}

/// A single text-showing operator (Tj / TJ / ' / ") resolved to a string
/// with its geometry. One PDF page typically has dozens to hundreds of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    pub font_size: f32,
    pub font_name: String,
    pub bbox: Bbox,
}

/// An image placement (Do operator referencing an /XObject /Subtype /Image).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePlacement {
    pub xobject_name: String,
    pub bbox: Bbox,
}

/// Per-page layout. `width` and `height` are MediaBox dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageLayout {
    pub page_number: u32,
    pub width: f32,
    pub height: f32,
    pub runs: Vec<TextRun>,
    pub images: Vec<ImagePlacement>,
}

impl PageLayout {
    pub fn run_count(&self) -> usize {
        self.runs.len()
    }
    pub fn image_count(&self) -> usize {
        self.images.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutTree {
    pub pages: Vec<PageLayout>,
}

impl LayoutTree {
    pub fn total_runs(&self) -> usize {
        self.pages.iter().map(|p| p.runs.len()).sum()
    }
    pub fn total_images(&self) -> usize {
        self.pages.iter().map(|p| p.images.len()).sum()
    }
}

/// Top-level entry point. Parse a PDF byte slice and return a LayoutTree.
pub fn extract_layout(pdf_bytes: &[u8]) -> Result<LayoutTree, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| format!("load: {e}"))?;
    extract_layout_from_doc(&doc)
}

/// Same as `extract_layout` but takes an already-parsed `Document`.
/// Useful when other slices have a doc handle already.
pub fn extract_layout_from_doc(doc: &Document) -> Result<LayoutTree, String> {
    let mut tree = LayoutTree::default();
    // Iterate page numbers in order so `tree.pages[i].page_number == i + 1`.
    let pages = doc.get_pages();
    let mut keys: Vec<u32> = pages.keys().copied().collect();
    keys.sort();
    for page_num in keys {
        let page_id = pages[&page_num];
        let layout =
            extract_page(doc, page_id, page_num).map_err(|e| format!("page {page_num}: {e}"))?;
        tree.pages.push(layout);
    }
    Ok(tree)
}

// ---------------------------------------------------------------------------
// Per-page extraction
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Matrix3 {
    // PDF 2-d affine: [a b 0; c d 0; e f 1]. We store the 6 active entries.
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix3 {
    fn identity() -> Self {
        Matrix3 {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self * other` — PDF convention applies operations right-to-left on
    /// points, so a `cm` operator post-multiplies the CTM.
    fn mul(self, o: Matrix3) -> Self {
        Matrix3 {
            a: o.a * self.a + o.b * self.c,
            b: o.a * self.b + o.b * self.d,
            c: o.c * self.a + o.d * self.c,
            d: o.c * self.b + o.d * self.d,
            e: o.e * self.a + o.f * self.c + self.e,
            f: o.e * self.b + o.f * self.d + self.f,
        }
    }

    /// Map a point (x, y) through this matrix.
    fn transform(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Scale factor on the y axis — used as the rendered font-size proxy.
    fn y_scale(self) -> f32 {
        (self.b * self.b + self.d * self.d).sqrt()
    }

    /// Scale factor on the x axis.
    fn x_scale(self) -> f32 {
        (self.a * self.a + self.c * self.c).sqrt()
    }
}

#[derive(Debug, Default, Clone)]
struct GraphicsState {
    ctm: Option<Matrix3>,
}

impl GraphicsState {
    fn ctm(&self) -> Matrix3 {
        self.ctm.unwrap_or_else(Matrix3::identity)
    }
}

#[derive(Debug, Clone)]
struct TextState {
    // Text matrix and text-line matrix.
    tm: Matrix3,
    tlm: Matrix3,
    // Font resource name (e.g. "F1") and size (Tf operand 2 — the pre-scale size).
    font_name: String,
    font_size: f32,
    // Leading (TL) and horizontal scaling (Tz, default 100 == 1.0 multiplier).
    leading: f32,
    horizontal_scaling: f32,
    char_space: f32,
    word_space: f32,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            tm: Matrix3::identity(),
            tlm: Matrix3::identity(),
            font_name: String::new(),
            font_size: 0.0,
            leading: 0.0,
            horizontal_scaling: 1.0,
            char_space: 0.0,
            word_space: 0.0,
        }
    }
}

fn extract_page(doc: &Document, page_id: ObjectId, page_num: u32) -> Result<PageLayout, String> {
    let (width, height) = page_media_box(doc, page_id);
    let mut layout = PageLayout {
        page_number: page_num,
        width,
        height,
        runs: Vec::new(),
        images: Vec::new(),
    };

    // A page's content can be either a single stream or an array of streams.
    let content_data = doc
        .get_page_content(page_id)
        .map_err(|e| format!("get_page_content: {e}"))?;
    let content = Content::decode(&content_data).map_err(|e| format!("Content::decode: {e}"))?;

    // Build a name -> xobject-subtype lookup so we can flag Do operators as
    // image vs form. Missing entries are treated as form (ignored).
    let xobject_subtypes = collect_xobject_subtypes(doc, page_id);

    let mut gs_stack: Vec<GraphicsState> = Vec::new();
    let mut gs = GraphicsState::default();
    let mut ts = TextState::default();
    let mut in_text = false;

    for op in &content.operations {
        match op.operator.as_str() {
            "q" => gs_stack.push(gs.clone()),
            "Q" => {
                if let Some(prev) = gs_stack.pop() {
                    gs = prev;
                }
            }
            "cm" if op.operands.len() == 6 => {
                let m = matrix_from_operands(&op.operands);
                gs.ctm = Some(gs.ctm().mul(m));
            }
            "BT" => {
                in_text = true;
                ts.tm = Matrix3::identity();
                ts.tlm = Matrix3::identity();
            }
            "ET" => {
                in_text = false;
            }
            "Tf" if in_text && op.operands.len() == 2 => {
                if let Object::Name(n) = &op.operands[0] {
                    ts.font_name = String::from_utf8_lossy(n).into_owned();
                }
                ts.font_size = op_to_f32(&op.operands[1]);
            }
            "TL" if op.operands.len() == 1 => {
                ts.leading = op_to_f32(&op.operands[0]);
            }
            "Tz" if op.operands.len() == 1 => {
                ts.horizontal_scaling = op_to_f32(&op.operands[0]) / 100.0;
            }
            "Tc" if op.operands.len() == 1 => {
                ts.char_space = op_to_f32(&op.operands[0]);
            }
            "Tw" if op.operands.len() == 1 => {
                ts.word_space = op_to_f32(&op.operands[0]);
            }
            "Td" if in_text && op.operands.len() == 2 => {
                let tx = op_to_f32(&op.operands[0]);
                let ty = op_to_f32(&op.operands[1]);
                let m = Matrix3 {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: tx,
                    f: ty,
                };
                ts.tlm = ts.tlm.mul(m);
                ts.tm = ts.tlm;
            }
            "TD" if in_text && op.operands.len() == 2 => {
                let tx = op_to_f32(&op.operands[0]);
                let ty = op_to_f32(&op.operands[1]);
                ts.leading = -ty;
                let m = Matrix3 {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: tx,
                    f: ty,
                };
                ts.tlm = ts.tlm.mul(m);
                ts.tm = ts.tlm;
            }
            "Tm" if in_text && op.operands.len() == 6 => {
                let m = matrix_from_operands(&op.operands);
                ts.tm = m;
                ts.tlm = m;
            }
            "T*" if in_text => {
                let m = Matrix3 {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: -ts.leading,
                };
                ts.tlm = ts.tlm.mul(m);
                ts.tm = ts.tlm;
            }
            "Tj" if in_text && op.operands.len() == 1 => {
                if let Some(run) = run_from_show(&op.operands[0], &ts, &gs) {
                    let aw = run_advance_width(&run, &ts);
                    advance_tm(&mut ts, aw);
                    layout.runs.push(run);
                }
            }
            "TJ" if in_text && op.operands.len() == 1 => {
                let runs = runs_from_tj_array(&op.operands[0], &mut ts, &gs);
                for run in runs {
                    layout.runs.push(run);
                }
            }
            "'" if in_text && !op.operands.is_empty() => {
                // Move to next line and show string.
                let m = Matrix3 {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: -ts.leading,
                };
                ts.tlm = ts.tlm.mul(m);
                ts.tm = ts.tlm;
                if let Some(run) = run_from_show(&op.operands[0], &ts, &gs) {
                    let aw = run_advance_width(&run, &ts);
                    advance_tm(&mut ts, aw);
                    layout.runs.push(run);
                }
            }
            "\"" if in_text && op.operands.len() == 3 => {
                ts.word_space = op_to_f32(&op.operands[0]);
                ts.char_space = op_to_f32(&op.operands[1]);
                let m = Matrix3 {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: -ts.leading,
                };
                ts.tlm = ts.tlm.mul(m);
                ts.tm = ts.tlm;
                if let Some(run) = run_from_show(&op.operands[2], &ts, &gs) {
                    let aw = run_advance_width(&run, &ts);
                    advance_tm(&mut ts, aw);
                    layout.runs.push(run);
                }
            }
            "Do" if op.operands.len() == 1 => {
                if let Object::Name(n) = &op.operands[0] {
                    let name = String::from_utf8_lossy(n).into_owned();
                    if xobject_subtypes.get(&name).map(|s| s.as_str()) == Some("Image") {
                        let ctm = gs.ctm();
                        // PDF XObject Image is drawn into the unit square
                        // (0,0)–(1,1) mapped by the CTM.
                        let (x0, y0) = ctm.transform(0.0, 0.0);
                        let (x1, y1) = ctm.transform(1.0, 1.0);
                        layout.images.push(ImagePlacement {
                            xobject_name: name,
                            bbox: Bbox {
                                x0: x0.min(x1),
                                y0: y0.min(y1),
                                x1: x0.max(x1),
                                y1: y0.max(y1),
                            },
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Ok(layout)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn matrix_from_operands(ops: &[Object]) -> Matrix3 {
    Matrix3 {
        a: op_to_f32(&ops[0]),
        b: op_to_f32(&ops[1]),
        c: op_to_f32(&ops[2]),
        d: op_to_f32(&ops[3]),
        e: op_to_f32(&ops[4]),
        f: op_to_f32(&ops[5]),
    }
}

fn op_to_f32(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn decode_show_string(o: &Object) -> Option<String> {
    let bytes = match o {
        Object::String(b, _) => b.clone(),
        _ => return None,
    };
    // First try UTF-8 (well-behaved producers) then fall back to
    // PDFDocEncoding-ish: pass through ASCII, replace anything else with '?'.
    if let Ok(s) = std::str::from_utf8(&bytes) {
        return Some(s.to_string());
    }
    let mut s = String::with_capacity(bytes.len());
    for b in bytes {
        if b.is_ascii() && !b.is_ascii_control() {
            s.push(b as char);
        } else if b == b'\n' || b == b'\r' || b == b'\t' {
            s.push(b as char);
        } else {
            s.push('?');
        }
    }
    Some(s)
}

fn run_from_show(operand: &Object, ts: &TextState, gs: &GraphicsState) -> Option<TextRun> {
    let text = decode_show_string(operand)?;
    if text.is_empty() {
        return None;
    }
    Some(make_run(text, ts, gs))
}

fn runs_from_tj_array(operand: &Object, ts: &mut TextState, gs: &GraphicsState) -> Vec<TextRun> {
    let mut out = Vec::new();
    let items = match operand {
        Object::Array(a) => a,
        _ => return out,
    };
    let mut buf = String::new();
    for item in items {
        match item {
            Object::String(_, _) => {
                if let Some(s) = decode_show_string(item) {
                    buf.push_str(&s);
                }
            }
            Object::Integer(_) | Object::Real(_) => {
                // Numeric adjustment in thousandths of a glyph unit. We
                // treat large negative kerning (more space) as a soft break
                // between runs so downstream segmentation can see word
                // boundaries; otherwise we just absorb it into the current run.
                let adj = op_to_f32(item);
                if adj.abs() >= 200.0 && !buf.is_empty() {
                    // Flush.
                    let run = make_run(std::mem::take(&mut buf), ts, gs);
                    let aw = run_advance_width(&run, ts);
                    out.push(run);
                    advance_tm(ts, aw);
                }
                // Move TM by adj * size / 1000 to keep next run positioned.
                let dx = -adj * ts.font_size / 1000.0 * ts.horizontal_scaling;
                advance_tm(ts, dx);
            }
            _ => {}
        }
    }
    if !buf.is_empty() {
        let run = make_run(buf, ts, gs);
        let aw = run_advance_width(&run, ts);
        out.push(run);
        advance_tm(ts, aw);
    }
    out
}

fn make_run(text: String, ts: &TextState, gs: &GraphicsState) -> TextRun {
    // Render-space size = ts.tm.y_scale() * ts.font_size * ctm.y_scale().
    let ctm = gs.ctm();
    let render_y_scale = ts.tm.y_scale() * ctm.y_scale();
    let font_size = ts.font_size * render_y_scale;
    // Glyph width approximation: 0.5 * size * len for proportional fonts
    // averaging ≈0.5 em. This is good enough for column / heading detection;
    // exact widths come later from font metrics if we need them.
    let glyph_count = text.chars().count() as f32;
    let approx_text_width =
        ts.font_size * ts.horizontal_scaling * 0.5 * glyph_count + ts.char_space * glyph_count;
    let approx_text_height = ts.font_size; // baseline → top-of-cap proxy
                                           // Anchor point: current Tm origin transformed through CTM.
    let (x0, y0) = ctm.transform(ts.tm.e, ts.tm.f);
    let (x1, y1) = ctm.transform(ts.tm.e + approx_text_width, ts.tm.f + approx_text_height);
    let bbox = Bbox {
        x0: x0.min(x1),
        y0: y0.min(y1),
        x1: x0.max(x1),
        y1: y0.max(y1),
    };
    TextRun {
        text,
        font_size,
        font_name: ts.font_name.clone(),
        bbox,
    }
}

fn run_advance_width(run: &TextRun, ts: &TextState) -> f32 {
    // Width in text space (pre-CTM): approx 0.5em per glyph + char/word spacing.
    let glyphs = run.text.chars().count() as f32;
    let spaces = run.text.chars().filter(|c| *c == ' ').count() as f32;
    ts.font_size * ts.horizontal_scaling * 0.5 * glyphs
        + ts.char_space * glyphs
        + ts.word_space * spaces
}

fn advance_tm(ts: &mut TextState, dx_text_space: f32) {
    // Move TM right by dx in text space. The text space basis is the TM
    // itself, so adding (dx, 0) in TM-local coords means stepping `e` by
    // dx*a and `f` by dx*b.
    ts.tm.e += dx_text_space * ts.tm.a;
    ts.tm.f += dx_text_space * ts.tm.b;
}

fn page_media_box(doc: &Document, page_id: ObjectId) -> (f32, f32) {
    // MediaBox can be inherited from a Pages parent.
    fn lookup(doc: &Document, id: ObjectId, depth: u8) -> Option<(f32, f32)> {
        if depth > 8 {
            return None;
        }
        let dict = doc.get_object(id).ok()?.as_dict().ok()?;
        if let Ok(Object::Array(arr)) = dict.get(b"MediaBox") {
            if arr.len() == 4 {
                let x0 = op_to_f32(&arr[0]);
                let y0 = op_to_f32(&arr[1]);
                let x1 = op_to_f32(&arr[2]);
                let y1 = op_to_f32(&arr[3]);
                return Some((x1 - x0, y1 - y0));
            }
        }
        let parent = dict.get(b"Parent").ok()?;
        if let Object::Reference(pid) = parent {
            return lookup(doc, *pid, depth + 1);
        }
        None
    }
    lookup(doc, page_id, 0).unwrap_or((612.0, 792.0))
}

fn collect_xobject_subtypes(
    doc: &Document,
    page_id: ObjectId,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return map;
    };
    // Resources may be inherited.
    let resources = resolve_resources(doc, page).unwrap_or(None);
    let Some(resources) = resources else {
        return map;
    };
    let Ok(xobject) = resources.get(b"XObject").and_then(|o| resolve_dict(doc, o)) else {
        return map;
    };
    for (name, value) in xobject.iter() {
        let name = String::from_utf8_lossy(name).into_owned();
        let subtype = match value {
            Object::Reference(rid) => doc
                .get_object(*rid)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.dict.get(b"Subtype").ok().cloned()),
            Object::Stream(s) => s.dict.get(b"Subtype").ok().cloned(),
            _ => None,
        };
        if let Some(Object::Name(n)) = subtype {
            map.insert(name, String::from_utf8_lossy(&n).into_owned());
        }
    }
    map
}

fn resolve_resources<'a>(
    doc: &'a Document,
    page: &'a lopdf::Dictionary,
) -> Result<Option<&'a lopdf::Dictionary>, lopdf::Error> {
    if let Ok(r) = page.get(b"Resources") {
        return match r {
            Object::Dictionary(d) => Ok(Some(d)),
            Object::Reference(rid) => Ok(doc.get_object(*rid).ok().and_then(|o| o.as_dict().ok())),
            _ => Ok(None),
        };
    }
    if let Ok(Object::Reference(pid)) = page.get(b"Parent") {
        let parent = doc.get_object(*pid)?.as_dict()?;
        return resolve_resources(doc, parent);
    }
    Ok(None)
}

fn resolve_dict<'a>(
    doc: &'a Document,
    obj: &'a Object,
) -> Result<&'a lopdf::Dictionary, lopdf::Error> {
    match obj {
        Object::Dictionary(d) => Ok(d),
        Object::Reference(rid) => doc.get_object(*rid)?.as_dict(),
        _ => Err(lopdf::Error::Syntax("expected dictionary".into())),
    }
}

// Suppress unused-import warning for Operation when read_operations not in use.
#[allow(dead_code)]
fn _force_use(_o: &Operation) {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn pdf_with_stream(content_bytes: &[u8]) -> Vec<u8> {
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
        let contents = doc.add_object(Stream::new(dictionary! {}, content_bytes.to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => contents,
            "Resources" => resources,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page)],
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

    fn pdf_with_image_xobject(content_bytes: &[u8]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        // Minimal /Image XObject (we never actually decode the pixels).
        let image = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 1,
                "Height" => 1,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![0u8],
        ));
        let resources = doc.add_object(dictionary! {
            "XObject" => dictionary! { "Im1" => image },
        });
        let contents = doc.add_object(Stream::new(dictionary! {}, content_bytes.to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => contents,
            "Resources" => resources,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page)],
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
    fn extracts_single_text_run() {
        let pdf = pdf_with_stream(b"BT /F1 12 Tf 50 700 Td (Hello world) Tj ET\n");
        let tree = extract_layout(&pdf).expect("extract");
        assert_eq!(tree.pages.len(), 1);
        let p = &tree.pages[0];
        assert_eq!(p.page_number, 1);
        assert_eq!(p.width, 612.0);
        assert_eq!(p.height, 792.0);
        assert_eq!(p.runs.len(), 1);
        let r = &p.runs[0];
        assert_eq!(r.text, "Hello world");
        assert!((r.font_size - 12.0).abs() < 0.01);
        assert_eq!(r.font_name, "F1");
        // Anchor was (50, 700) and we extend right + up.
        assert!((r.bbox.x0 - 50.0).abs() < 0.5);
        assert!((r.bbox.y0 - 700.0).abs() < 0.5);
        assert!(r.bbox.x1 > r.bbox.x0);
        assert!(r.bbox.y1 > r.bbox.y0);
    }

    #[test]
    fn extracts_heading_above_body_with_correct_sizes() {
        // 24pt heading at y=720, 12pt body at y=680.
        let stream =
            b"BT /F1 24 Tf 50 720 Td (Title) Tj ET\nBT /F1 12 Tf 50 680 Td (Body text.) Tj ET\n";
        let pdf = pdf_with_stream(stream);
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.runs.len(), 2);
        let title = p
            .runs
            .iter()
            .find(|r| r.text.contains("Title"))
            .expect("title run");
        let body = p
            .runs
            .iter()
            .find(|r| r.text.contains("Body"))
            .expect("body run");
        assert!((title.font_size - 24.0).abs() < 0.5);
        assert!((body.font_size - 12.0).abs() < 0.5);
        assert!(
            title.bbox.y0 > body.bbox.y0,
            "title ({}) should be above body ({})",
            title.bbox.y0,
            body.bbox.y0,
        );
    }

    #[test]
    fn handles_tj_array_with_kerning() {
        // ( He ) -200 ( llo ) — should yield one or two runs whose joined
        // text equals "Hello".
        let pdf = pdf_with_stream(b"BT /F1 12 Tf 100 700 Td [(He) -300 (llo)] TJ ET\n");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert!(!p.runs.is_empty());
        let joined: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(joined, "Hello");
    }

    #[test]
    fn tm_overrides_td_position() {
        // Set Tm directly to (200, 500) — Td above it should be reset.
        let pdf = pdf_with_stream(b"BT /F1 14 Tf 50 100 Td 1 0 0 1 200 500 Tm (Anchor) Tj ET\n");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.runs.len(), 1);
        let r = &p.runs[0];
        assert!(
            (r.bbox.x0 - 200.0).abs() < 0.5,
            "expected x≈200, got {}",
            r.bbox.x0,
        );
        assert!(
            (r.bbox.y0 - 500.0).abs() < 0.5,
            "expected y≈500, got {}",
            r.bbox.y0,
        );
    }

    #[test]
    fn ctm_scaling_doubles_font_size() {
        // q 2 0 0 2 0 0 cm BT /F1 12 Tf 50 100 Td (Big) Tj ET Q
        let pdf = pdf_with_stream(b"q 2 0 0 2 0 0 cm BT /F1 12 Tf 50 100 Td (Big) Tj ET Q\n");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.runs.len(), 1);
        let r = &p.runs[0];
        assert!(
            (r.font_size - 24.0).abs() < 0.5,
            "expected rendered size 24, got {}",
            r.font_size,
        );
        // CTM also moves the anchor from (50, 100) to (100, 200).
        assert!((r.bbox.x0 - 100.0).abs() < 1.0);
        assert!((r.bbox.y0 - 200.0).abs() < 1.0);
    }

    #[test]
    fn q_q_stack_isolates_ctm() {
        // Outer page draws at default; inner save-state scales; outer
        // resumes at 12pt.
        let pdf = pdf_with_stream(
            b"BT /F1 12 Tf 50 700 Td (Plain) Tj ET\n\
              q 3 0 0 3 0 0 cm BT /F1 12 Tf 0 0 Td (Big) Tj ET Q\n\
              BT /F1 12 Tf 50 600 Td (PlainAgain) Tj ET\n",
        );
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        let plain = p
            .runs
            .iter()
            .find(|r| r.text == "Plain")
            .expect("plain run");
        let big = p.runs.iter().find(|r| r.text == "Big").expect("big run");
        let plain_again = p
            .runs
            .iter()
            .find(|r| r.text == "PlainAgain")
            .expect("plain_again run");
        assert!((plain.font_size - 12.0).abs() < 0.5);
        assert!((big.font_size - 36.0).abs() < 0.5);
        assert!((plain_again.font_size - 12.0).abs() < 0.5);
    }

    #[test]
    fn t_star_advances_by_leading() {
        // Set leading to 20, then T* twice — each line drops 20 units.
        let pdf =
            pdf_with_stream(b"BT /F1 12 Tf 20 TL 100 700 Td (L1) Tj T* (L2) Tj T* (L3) Tj ET\n");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.runs.len(), 3);
        let l1 = p.runs.iter().find(|r| r.text == "L1").unwrap();
        let l2 = p.runs.iter().find(|r| r.text == "L2").unwrap();
        let l3 = p.runs.iter().find(|r| r.text == "L3").unwrap();
        assert!((l1.bbox.y0 - 700.0).abs() < 0.5);
        assert!((l2.bbox.y0 - 680.0).abs() < 0.5);
        assert!((l3.bbox.y0 - 660.0).abs() < 0.5);
    }

    #[test]
    fn empty_page_yields_no_runs() {
        let pdf = pdf_with_stream(b"");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.runs.len(), 0);
        assert_eq!(p.images.len(), 0);
    }

    #[test]
    fn detects_image_placement() {
        let pdf = pdf_with_image_xobject(b"q 100 0 0 50 200 300 cm /Im1 Do Q\n");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.images.len(), 1);
        let img = &p.images[0];
        assert_eq!(img.xobject_name, "Im1");
        // CTM (a=100, b=0, c=0, d=50, e=200, f=300) maps (0,0)→(200,300) and (1,1)→(300,350).
        assert!((img.bbox.x0 - 200.0).abs() < 0.5);
        assert!((img.bbox.y0 - 300.0).abs() < 0.5);
        assert!((img.bbox.x1 - 300.0).abs() < 0.5);
        assert!((img.bbox.y1 - 350.0).abs() < 0.5);
    }

    #[test]
    fn layout_tree_aggregate_helpers() {
        let pdf =
            pdf_with_stream(b"BT /F1 12 Tf 50 700 Td (A) Tj 0 -20 Td (B) Tj 0 -20 Td (C) Tj ET\n");
        let tree = extract_layout(&pdf).expect("extract");
        assert_eq!(tree.pages.len(), 1);
        assert_eq!(tree.total_runs(), 3);
        assert_eq!(tree.total_images(), 0);
    }

    #[test]
    fn quoted_show_apostrophe_moves_to_next_line() {
        let pdf = pdf_with_stream(b"BT /F1 12 Tf 18 TL 100 700 Td (L1) Tj (L2) ' (L3) ' ET\n");
        let tree = extract_layout(&pdf).expect("extract");
        let p = &tree.pages[0];
        assert_eq!(p.runs.len(), 3);
        let l1 = p.runs.iter().find(|r| r.text == "L1").unwrap();
        let l2 = p.runs.iter().find(|r| r.text == "L2").unwrap();
        let l3 = p.runs.iter().find(|r| r.text == "L3").unwrap();
        assert!((l1.bbox.y0 - 700.0).abs() < 0.5);
        assert!((l2.bbox.y0 - 682.0).abs() < 0.5);
        assert!((l3.bbox.y0 - 664.0).abs() < 0.5);
    }
}
