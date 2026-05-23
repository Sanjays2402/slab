//! Compute on-page glyph bounding boxes from a content-stream op slice.
//!
//! This is a *bbox approximator* — without full CIDFont width tables we use a
//! conservative average-width heuristic. For redaction the bbox only needs to
//! be a tight-enough superset of the real glyph bbox; over-redaction is safe,
//! under-redaction is a leak.

use lopdf::content::Operation;
use lopdf::Object;

#[derive(Debug, Clone, Copy)]
pub struct GlyphBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Index into the Operation slice that produced this box (Tj/TJ/'/").
    pub op_index: usize,
}

impl GlyphBox {
    /// True iff this glyph box intersects the axis-aligned rect (l,b,r,t).
    pub fn intersects(&self, l: f32, b: f32, r: f32, t: f32) -> bool {
        let (gl, gb, gr, gt) = (self.x, self.y, self.x + self.width, self.y + self.height);
        !(gr < l || gl > r || gt < b || gb > t)
    }

    /// True iff this glyph box is fully contained within (l,b,r,t).
    pub fn inside(&self, l: f32, b: f32, r: f32, t: f32) -> bool {
        self.x >= l && self.y >= b && (self.x + self.width) <= r && (self.y + self.height) <= t
    }
}

/// Mutable text state during content-stream walk.
#[derive(Debug, Clone)]
pub struct TextState {
    pub font_size: f32,
    pub line_matrix: [f32; 6],
    pub text_matrix: [f32; 6],
    pub leading: f32,
    pub char_space: f32,
    pub word_space: f32,
    pub horiz_scale: f32,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            line_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            leading: 0.0,
            char_space: 0.0,
            word_space: 0.0,
            horiz_scale: 1.0,
        }
    }
}

/// Walk all operations in `ops`, returning a GlyphBox for every Tj/TJ/'/"
/// operator encountered. `default_size` is the font size to assume when none
/// has been set yet (defensive fallback).
pub fn collect_text_boxes(ops: &[Operation], default_size: f32) -> Vec<GlyphBox> {
    let mut state = TextState {
        font_size: default_size,
        ..Default::default()
    };
    let mut out = Vec::new();
    let mut in_text = false;

    for (idx, op) in ops.iter().enumerate() {
        match op.operator.as_str() {
            "BT" => {
                in_text = true;
                state.text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                state.line_matrix = state.text_matrix;
            }
            "ET" => in_text = false,
            "Tf" => {
                if let Some(sz) = op.operands.get(1).and_then(num) {
                    state.font_size = sz;
                }
            }
            "TL" => {
                if let Some(v) = op.operands.first().and_then(num) {
                    state.leading = v;
                }
            }
            "Tc" => {
                if let Some(v) = op.operands.first().and_then(num) {
                    state.char_space = v;
                }
            }
            "Tw" => {
                if let Some(v) = op.operands.first().and_then(num) {
                    state.word_space = v;
                }
            }
            "Tz" => {
                if let Some(v) = op.operands.first().and_then(num) {
                    state.horiz_scale = v / 100.0;
                }
            }
            "Td" | "TD" => {
                let tx = op.operands.first().and_then(num).unwrap_or(0.0);
                let ty = op.operands.get(1).and_then(num).unwrap_or(0.0);
                if op.operator == "TD" {
                    state.leading = -ty;
                }
                translate_line(&mut state, tx, ty);
            }
            "Tm" => {
                let m: Vec<f32> = op.operands.iter().filter_map(num).collect();
                if m.len() == 6 {
                    state.text_matrix = [m[0], m[1], m[2], m[3], m[4], m[5]];
                    state.line_matrix = state.text_matrix;
                }
            }
            "T*" => {
                let l = state.leading;
                translate_line(&mut state, 0.0, -l);
            }
            "Tj" => {
                if !in_text {
                    continue;
                }
                if let Some(s) = op.operands.first().and_then(bytes) {
                    out.push(advance_for_string(&mut state, &s, idx));
                }
            }
            "TJ" => {
                if !in_text {
                    continue;
                }
                if let Some(arr) = op.operands.first().and_then(|o| o.as_array().ok()) {
                    let mut combined: Option<GlyphBox> = None;
                    for el in arr {
                        match el {
                            Object::String(s, _) => {
                                let b = advance_for_string(&mut state, s, idx);
                                combined = Some(match combined {
                                    None => b,
                                    Some(c) => merge(c, b),
                                });
                            }
                            Object::Integer(i) => {
                                let adj =
                                    -(*i as f32) / 1000.0 * state.font_size * state.horiz_scale;
                                state.text_matrix[4] += adj;
                            }
                            Object::Real(r) => {
                                let adj = -*r / 1000.0 * state.font_size * state.horiz_scale;
                                state.text_matrix[4] += adj;
                            }
                            _ => {}
                        }
                    }
                    if let Some(b) = combined {
                        out.push(b);
                    }
                }
            }
            "'" => {
                let l = state.leading;
                translate_line(&mut state, 0.0, -l);
                if let Some(s) = op.operands.first().and_then(bytes) {
                    out.push(advance_for_string(&mut state, &s, idx));
                }
            }
            "\"" => {
                if let Some(v) = op.operands.first().and_then(num) {
                    state.word_space = v;
                }
                if let Some(v) = op.operands.get(1).and_then(num) {
                    state.char_space = v;
                }
                let l = state.leading;
                translate_line(&mut state, 0.0, -l);
                if let Some(s) = op.operands.get(2).and_then(bytes) {
                    out.push(advance_for_string(&mut state, &s, idx));
                }
            }
            _ => {}
        }
    }
    out
}

/// Convenience accessor for the bbox of a single op by index.
pub fn bbox_of_text_op(ops: &[Operation], op_idx: usize) -> Option<GlyphBox> {
    collect_text_boxes(ops, 12.0)
        .into_iter()
        .find(|b| b.op_index == op_idx)
}

fn translate_line(state: &mut TextState, tx: f32, ty: f32) {
    let m = &mut state.line_matrix;
    m[4] += tx * m[0] + ty * m[2];
    m[5] += tx * m[1] + ty * m[3];
    state.text_matrix = *m;
}

fn advance_for_string(state: &mut TextState, s: &[u8], op_index: usize) -> GlyphBox {
    // Average Helvetica glyph width = 0.5 em. We over-estimate to 0.6 em so
    // bboxes are a safe superset.
    let avg_w_em = 0.6_f32;
    let mut width = 0.0;
    for &b in s {
        let w = avg_w_em * state.font_size * state.horiz_scale + state.char_space;
        let w = if b == b' ' { w + state.word_space } else { w };
        width += w;
    }
    let x = state.text_matrix[4];
    let y = state.text_matrix[5] - state.font_size * 0.2; // descender pad
    let h = state.font_size * 1.2;
    state.text_matrix[4] += width;
    GlyphBox {
        x,
        y,
        width,
        height: h,
        op_index,
    }
}

fn merge(a: GlyphBox, b: GlyphBox) -> GlyphBox {
    let l = a.x.min(b.x);
    let bo = a.y.min(b.y);
    let r = (a.x + a.width).max(b.x + b.width);
    let t = (a.y + a.height).max(b.y + b.height);
    GlyphBox {
        x: l,
        y: bo,
        width: r - l,
        height: t - bo,
        op_index: a.op_index,
    }
}

fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn bytes(o: &Object) -> Option<Vec<u8>> {
    if let Object::String(s, _) = o {
        Some(s.clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::Operation;
    use lopdf::Object;

    fn op(name: &str, args: Vec<Object>) -> Operation {
        Operation::new(name, args)
    }

    #[test]
    fn bbox_for_tj_at_origin_in_helvetica_12() {
        let ops = vec![
            op("BT", vec![]),
            op(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
            ),
            op("Td", vec![100.into(), 200.into()]),
            op(
                "Tj",
                vec![Object::String(b"Hi".to_vec(), lopdf::StringFormat::Literal)],
            ),
            op("ET", vec![]),
        ];
        let boxes = collect_text_boxes(&ops, 12.0);
        assert_eq!(boxes.len(), 1);
        let b = &boxes[0];
        assert!((b.y - (200.0 - 12.0 * 0.2)).abs() < 0.5, "y was {}", b.y);
        assert!(b.x >= 100.0 && b.x <= 101.0);
        assert!(b.width > 6.0 && b.width < 30.0);
        assert!((b.height - 12.0 * 1.2).abs() < 1.0);
    }

    #[test]
    fn bbox_for_tm_absolute_placement() {
        let ops = vec![
            op("BT", vec![]),
            op(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
            ),
            op(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    50.into(),
                    700.into(),
                ],
            ),
            op(
                "Tj",
                vec![Object::String(b"X".to_vec(), lopdf::StringFormat::Literal)],
            ),
            op("ET", vec![]),
        ];
        let boxes = collect_text_boxes(&ops, 10.0);
        assert!((boxes[0].x - 50.0).abs() < 0.5);
        assert!((boxes[0].y - (700.0 - 10.0 * 0.2)).abs() < 0.5);
    }

    #[test]
    fn bbox_for_tj_array_with_kerning() {
        let ops = vec![
            op("BT", vec![]),
            op(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
            ),
            op("Td", vec![0.into(), 0.into()]),
            op(
                "TJ",
                vec![Object::Array(vec![
                    Object::String(b"He".to_vec(), lopdf::StringFormat::Literal),
                    Object::Integer(-100),
                    Object::String(b"llo".to_vec(), lopdf::StringFormat::Literal),
                ])],
            ),
            op("ET", vec![]),
        ];
        let boxes = collect_text_boxes(&ops, 12.0);
        assert_eq!(boxes.len(), 1);
        assert!(boxes[0].width > 20.0);
    }

    #[test]
    fn empty_ops_yields_empty() {
        assert!(collect_text_boxes(&[], 12.0).is_empty());
    }

    #[test]
    fn intersects_and_inside() {
        let b = GlyphBox {
            x: 100.0,
            y: 200.0,
            width: 50.0,
            height: 14.0,
            op_index: 0,
        };
        assert!(b.intersects(90.0, 195.0, 160.0, 220.0));
        assert!(!b.intersects(0.0, 0.0, 50.0, 50.0));
        assert!(b.inside(90.0, 195.0, 160.0, 220.0));
        assert!(!b.inside(110.0, 200.0, 140.0, 210.0));
    }

    #[test]
    fn bbox_of_text_op_finds_by_index() {
        let ops = vec![
            op("BT", vec![]),
            op(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
            ),
            op("Td", vec![10.into(), 10.into()]),
            op(
                "Tj",
                vec![Object::String(b"A".to_vec(), lopdf::StringFormat::Literal)],
            ),
            op(
                "Tj",
                vec![Object::String(b"B".to_vec(), lopdf::StringFormat::Literal)],
            ),
            op("ET", vec![]),
        ];
        assert!(bbox_of_text_op(&ops, 3).is_some());
        assert!(bbox_of_text_op(&ops, 4).is_some());
        assert!(bbox_of_text_op(&ops, 0).is_none());
    }
}
