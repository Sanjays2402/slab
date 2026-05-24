//! Three-way redline PDF exporter — v3.24.0 "Stack Pro".
//!
//! Bakes a `ThreeWayDiff` into a single, shareable PDF: each page has a
//! three-column layout (Base | Mine | Theirs) with rows colour-coded by
//! classification (unchanged, mine-only, theirs-only, both-agree, conflict).
//!
//! Companion to `stack_redline.rs`. Whereas that module ships the 2-way
//! redline that competes with Litera/DeltaXML, this one ships the 3-way
//! redline — the canonical legal/dev-team feature Litera Compare charges
//! $400/seat/year for. Acrobat doesn't ship it at all.
//!
//! Uses the standard 14 base-PDF fonts via `lopdf` so output stays small
//! and renders in any viewer (no font embedding, no Slab required to read).

use crate::pdf::diff3::{ThreeWayDiff, ThreeWayKind, ThreeWayLine};
use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use std::path::Path;

// ---- Page geometry (landscape US Letter to fit 3 columns comfortably). ----
const PAGE_W: f32 = 792.0;
const PAGE_H: f32 = 612.0;
const MARGIN: f32 = 36.0;
const HEADER_H: f32 = 96.0;

const COL_GAP: f32 = 10.0;
const ROW_PAD: f32 = 4.0;

const BODY_SIZE: f32 = 9.0;
const LINE_HEIGHT: f32 = 11.5;
const HEADING_SIZE: f32 = 16.0;
const META_SIZE: f32 = 9.0;
const COL_HEADING_SIZE: f32 = 10.0;

// Base-14 font aliases.
const F_REG: &str = "F1";
const F_BOLD: &str = "F2";
const F_MONO: &str = "F5";

// Theme colours (RGB 0..1) — mirror Diff3Panel.svelte semantics.
const C_UNCHANGED: [f32; 3] = [0.45, 0.46, 0.50];
const C_MINE_ONLY: [f32; 3] = [0.17, 0.40, 0.85];
const C_THEIRS_ONLY: [f32; 3] = [0.55, 0.22, 0.75];
const C_AGREE: [f32; 3] = [0.18, 0.58, 0.27];
const C_CONFLICT: [f32; 3] = [0.80, 0.20, 0.20];
const C_TEXT: [f32; 3] = [0.10, 0.11, 0.12];
const C_MUTED: [f32; 3] = [0.45, 0.46, 0.50];
const C_RULE: [f32; 3] = [0.82, 0.83, 0.85];
const C_CONFLICT_TINT: [f32; 3] = [0.99, 0.93, 0.93];

/// Summary returned to the caller once the export succeeds.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Diff3ExportResult {
    pub pages: u32,
    pub conflicts: u32,
    pub mine_only: u32,
    pub theirs_only: u32,
    pub both_agree: u32,
}

/// Render the three-way diff `d` to a shareable PDF at `output`.
pub fn export_diff3_pdf(d: &ThreeWayDiff, output: &Path) -> Result<Diff3ExportResult, PdfError> {
    let mut r = Renderer::new();

    let col_w = (PAGE_W - 2.0 * MARGIN - 2.0 * COL_GAP) / 3.0;
    let col_x = [
        MARGIN,
        MARGIN + col_w + COL_GAP,
        MARGIN + 2.0 * (col_w + COL_GAP),
    ];

    r.header_meta = HeaderMeta {
        base: path_str(&d.base_path.display().to_string()),
        mine: path_str(&d.mine_path.display().to_string()),
        theirs: path_str(&d.theirs_path.display().to_string()),
        total_conflicts: d.total.conflicts,
        total_mine: d.total.mine_only,
        total_theirs: d.total.theirs_only,
        total_agree: d.total.both_agree,
        total_unchanged: d.total.unchanged,
        col_x,
    };
    r.start_page();

    for page in &d.pages {
        r.ensure_room(HEADING_SIZE + LINE_HEIGHT * 2.0);
        r.y -= 6.0;
        let heading = format!(
            "Page {}   ({} unchanged · {} mine-only · {} theirs-only · {} agreed · {} conflict)",
            page.page,
            page.summary.unchanged,
            page.summary.mine_only,
            page.summary.theirs_only,
            page.summary.both_agree,
            page.summary.conflicts,
        );
        r.text_left(MARGIN, r.y, &heading, F_BOLD, COL_HEADING_SIZE, C_TEXT);
        r.y -= COL_HEADING_SIZE + 4.0;
        r.hr();
        r.y -= 4.0;

        for line in &page.lines {
            r.draw_row(line, col_w, col_x);
        }
    }

    let n_pages = r.finalize(output)?;
    Ok(Diff3ExportResult {
        pages: n_pages,
        conflicts: d.total.conflicts,
        mine_only: d.total.mine_only,
        theirs_only: d.total.theirs_only,
        both_agree: d.total.both_agree,
    })
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct HeaderMeta {
    base: String,
    mine: String,
    theirs: String,
    total_conflicts: u32,
    total_mine: u32,
    total_theirs: u32,
    total_agree: u32,
    total_unchanged: u32,
    col_x: [f32; 3],
}

struct Renderer {
    pages: Vec<Vec<Operation>>,
    current: Vec<Operation>,
    y: f32,
    in_text: bool,
    cur_font: Option<&'static str>,
    cur_size: f32,
    cur_color: Option<[f32; 3]>,
    header_meta: HeaderMeta,
}

impl Renderer {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            current: Vec::new(),
            y: PAGE_H - MARGIN,
            in_text: false,
            cur_font: None,
            cur_size: 0.0,
            cur_color: None,
            header_meta: HeaderMeta::default(),
        }
    }

    fn start_page(&mut self) {
        // Title.
        self.text_left(
            MARGIN,
            PAGE_H - MARGIN - HEADING_SIZE,
            "Slab Three-Way Compare",
            F_BOLD,
            HEADING_SIZE,
            C_TEXT,
        );
        let totals = format!(
            "{} conflicts · {} mine-only · {} theirs-only · {} agreed · {} unchanged",
            self.header_meta.total_conflicts,
            self.header_meta.total_mine,
            self.header_meta.total_theirs,
            self.header_meta.total_agree,
            self.header_meta.total_unchanged,
        );
        let tw = estimate_width(&totals, META_SIZE);
        self.text_left(
            PAGE_W - MARGIN - tw,
            PAGE_H - MARGIN - HEADING_SIZE + 2.0,
            &totals,
            F_REG,
            META_SIZE,
            C_MUTED,
        );

        // File paths.
        let y0 = PAGE_H - MARGIN - HEADING_SIZE - 14.0;
        self.text_left(
            MARGIN,
            y0,
            &format!("Base:   {}", self.header_meta.base),
            F_REG,
            META_SIZE,
            C_MUTED,
        );
        self.text_left(
            MARGIN,
            y0 - (META_SIZE + 3.0),
            &format!("Mine:   {}", self.header_meta.mine),
            F_REG,
            META_SIZE,
            C_MINE_ONLY,
        );
        self.text_left(
            MARGIN,
            y0 - 2.0 * (META_SIZE + 3.0),
            &format!("Theirs: {}", self.header_meta.theirs),
            F_REG,
            META_SIZE,
            C_THEIRS_ONLY,
        );

        // Column headers.
        let col_y = PAGE_H - HEADER_H + 4.0;
        let labels = ["Base", "Mine", "Theirs"];
        let colors = [C_TEXT, C_MINE_ONLY, C_THEIRS_ONLY];
        for i in 0..3 {
            self.text_left(
                self.header_meta.col_x[i],
                col_y,
                labels[i],
                F_BOLD,
                COL_HEADING_SIZE,
                colors[i],
            );
        }
        self.y = PAGE_H - HEADER_H - 4.0;
        self.hr();
        self.y -= 6.0;
    }

    fn draw_row(&mut self, line: &ThreeWayLine, col_w: f32, col_x: [f32; 3]) {
        let cells = [
            line.base_text.as_str(),
            line.mine_text.as_deref().unwrap_or("(deleted)"),
            line.theirs_text.as_deref().unwrap_or("(deleted)"),
        ];
        let wrapped: Vec<Vec<String>> = cells
            .iter()
            .map(|t| wrap_to_width(t, col_w - 2.0 * ROW_PAD, BODY_SIZE))
            .collect();
        let row_lines = wrapped.iter().map(|v| v.len()).max().unwrap_or(1) as f32;
        let row_h = row_lines * LINE_HEIGHT + 2.0 * ROW_PAD;
        self.ensure_room(row_h + 2.0);

        // Soft red wash behind conflict rows.
        if line.kind == ThreeWayKind::Conflict {
            self.fill_rect(
                MARGIN - 4.0,
                self.y - row_h + ROW_PAD,
                PAGE_W - 2.0 * MARGIN + 8.0,
                row_h,
                C_CONFLICT_TINT,
            );
        }

        let (pill, pill_color) = match line.kind {
            ThreeWayKind::Unchanged => ("=", C_UNCHANGED),
            ThreeWayKind::MineOnly => ("M", C_MINE_ONLY),
            ThreeWayKind::TheirsOnly => ("T", C_THEIRS_ONLY),
            ThreeWayKind::BothAgree => ("A", C_AGREE),
            ThreeWayKind::Conflict => ("!", C_CONFLICT),
        };
        self.text_left(
            MARGIN - 18.0,
            self.y - ROW_PAD - BODY_SIZE,
            pill,
            F_BOLD,
            BODY_SIZE,
            pill_color,
        );

        let col_colors = [
            match line.kind {
                ThreeWayKind::Conflict => C_CONFLICT,
                _ => C_UNCHANGED,
            },
            match line.kind {
                ThreeWayKind::MineOnly | ThreeWayKind::Conflict => C_MINE_ONLY,
                ThreeWayKind::BothAgree => C_AGREE,
                _ => C_UNCHANGED,
            },
            match line.kind {
                ThreeWayKind::TheirsOnly | ThreeWayKind::Conflict => C_THEIRS_ONLY,
                ThreeWayKind::BothAgree => C_AGREE,
                _ => C_UNCHANGED,
            },
        ];
        for ci in 0..3 {
            let mut yy = self.y - ROW_PAD - BODY_SIZE;
            for ln in &wrapped[ci] {
                self.text_left(
                    col_x[ci] + ROW_PAD,
                    yy,
                    ln,
                    F_REG,
                    BODY_SIZE,
                    col_colors[ci],
                );
                yy -= LINE_HEIGHT;
            }
        }
        self.y -= row_h;
    }

    fn open_text(&mut self) {
        if !self.in_text {
            self.current.push(Operation::new("BT", vec![]));
            self.in_text = true;
            self.cur_font = None;
            self.cur_size = 0.0;
            self.cur_color = None;
        }
    }
    fn close_text(&mut self) {
        if self.in_text {
            self.current.push(Operation::new("ET", vec![]));
            self.in_text = false;
        }
    }
    fn set_font(&mut self, name: &'static str, size: f32) {
        if self.cur_font != Some(name) || (self.cur_size - size).abs() > 0.01 {
            self.open_text();
            self.current.push(Operation::new(
                "Tf",
                vec![Object::Name(name.as_bytes().to_vec()), Object::Real(size)],
            ));
            self.cur_font = Some(name);
            self.cur_size = size;
        }
    }
    fn set_fill_rgb(&mut self, c: [f32; 3]) {
        if self.cur_color != Some(c) {
            self.open_text();
            self.current.push(Operation::new(
                "rg",
                vec![Object::Real(c[0]), Object::Real(c[1]), Object::Real(c[2])],
            ));
            self.cur_color = Some(c);
        }
    }
    fn move_to(&mut self, x: f32, y: f32) {
        self.open_text();
        self.current.push(Operation::new(
            "Tm",
            vec![
                Object::Real(1.0),
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(1.0),
                Object::Real(x),
                Object::Real(y),
            ],
        ));
    }
    fn show(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.open_text();
        self.current.push(Operation::new(
            "Tj",
            vec![Object::string_literal(safe(text))],
        ));
    }
    fn text_left(&mut self, x: f32, y: f32, t: &str, f: &'static str, s: f32, c: [f32; 3]) {
        self.set_font(f, s);
        self.set_fill_rgb(c);
        self.move_to(x, y);
        self.show(t);
    }
    fn hr(&mut self) {
        self.close_text();
        let y = self.y;
        self.current.push(Operation::new("q", vec![]));
        self.current.push(Operation::new(
            "RG",
            vec![
                Object::Real(C_RULE[0]),
                Object::Real(C_RULE[1]),
                Object::Real(C_RULE[2]),
            ],
        ));
        self.current
            .push(Operation::new("w", vec![Object::Real(0.6)]));
        self.current.push(Operation::new(
            "m",
            vec![Object::Real(MARGIN), Object::Real(y)],
        ));
        self.current.push(Operation::new(
            "l",
            vec![Object::Real(PAGE_W - MARGIN), Object::Real(y)],
        ));
        self.current.push(Operation::new("S", vec![]));
        self.current.push(Operation::new("Q", vec![]));
        self.cur_color = None;
    }
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 3]) {
        self.close_text();
        self.current.push(Operation::new("q", vec![]));
        self.current.push(Operation::new(
            "rg",
            vec![Object::Real(c[0]), Object::Real(c[1]), Object::Real(c[2])],
        ));
        self.current.push(Operation::new(
            "re",
            vec![
                Object::Real(x),
                Object::Real(y),
                Object::Real(w),
                Object::Real(h),
            ],
        ));
        self.current.push(Operation::new("f", vec![]));
        self.current.push(Operation::new("Q", vec![]));
        self.cur_color = None;
    }
    fn ensure_room(&mut self, h: f32) {
        if self.y - h < MARGIN + 18.0 {
            self.page_break();
        }
    }
    fn page_break(&mut self) {
        let footer_y = MARGIN - 14.0;
        let n = self.pages.len() as u32 + 1;
        self.text_left(
            MARGIN,
            footer_y,
            &format!("Slab Three-Way Compare — page {n}"),
            F_REG,
            8.0,
            C_MUTED,
        );
        self.close_text();
        let done = std::mem::take(&mut self.current);
        self.pages.push(done);
        self.y = PAGE_H - MARGIN;
        self.cur_color = None;
        self.cur_font = None;
        self.start_page();
    }

    fn finalize(mut self, output: &Path) -> Result<u32, PdfError> {
        if !self.current.is_empty() || self.pages.is_empty() {
            let footer_y = MARGIN - 14.0;
            let n = self.pages.len() as u32 + 1;
            self.text_left(
                MARGIN,
                footer_y,
                &format!("Slab Three-Way Compare — page {n}"),
                F_REG,
                8.0,
                C_MUTED,
            );
            self.close_text();
            let done = std::mem::take(&mut self.current);
            self.pages.push(done);
        }

        let n_pages = self.pages.len() as u32;
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let f_reg = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let f_bold = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold",
        });
        let f_mono = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                F_REG => f_reg,
                F_BOLD => f_bold,
                F_MONO => f_mono,
            },
        });

        let mut kids: Vec<Object> = Vec::with_capacity(self.pages.len());
        for ops in self.pages {
            let content = Content { operations: ops };
            let encoded = content
                .encode()
                .map_err(|e| PdfError::Other(format!("stack_diff3_export encode: {e}")))?;
            let stream_id = doc.add_object(Stream::new(dictionary! {}, encoded));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
                "Contents" => stream_id,
                "Resources" => resources_id,
            });
            kids.push(Object::Reference(page_id));
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => n_pages as i64,
            }),
        );
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog);
        doc.compress();
        doc.save(output)?;
        Ok(n_pages)
    }
}

fn estimate_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.52
}

fn wrap_to_width(text: &str, max_w: f32, size: f32) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if estimate_width(&candidate, size) <= max_w {
            cur = candidate;
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if estimate_width(word, size) > max_w {
                let mut chunk = String::new();
                for c in word.chars() {
                    chunk.push(c);
                    if estimate_width(&chunk, size) > max_w {
                        chunk.pop();
                        out.push(std::mem::take(&mut chunk));
                        chunk.push(c);
                    }
                }
                if !chunk.is_empty() {
                    cur = chunk;
                }
            } else {
                cur = word.to_string();
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn safe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' => out.push_str("  "),
            '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 => out.push(' '),
            c if (c as u32) <= 0xFF => out.push(c),
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201C}' | '\u{201D}' => out.push('"'),
            '\u{2013}' | '\u{2014}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            _ => out.push('?'),
        }
    }
    out
}

fn path_str(s: &str) -> String {
    s.chars()
        .map(|c| if (c as u32) < 0x20 { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::diff::{DiffOp, DiffSummary, DocDiff, LineDiff, PageDiff};
    use crate::pdf::diff3::{three_way_from_diffs, ThreeWayKind};

    fn line(op: DiffOp, old_line: Option<u32>, new_line: Option<u32>, text: &str) -> LineDiff {
        LineDiff {
            op,
            old_line,
            new_line,
            text: text.to_string(),
            words: None,
        }
    }

    fn one_page_doc(lines: Vec<LineDiff>) -> DocDiff {
        let summary = DiffSummary {
            added: lines.iter().filter(|l| l.op == DiffOp::Insert).count() as u32,
            removed: lines.iter().filter(|l| l.op == DiffOp::Delete).count() as u32,
            changed: 0,
        };
        DocDiff {
            old_path: "/b".into(),
            new_path: "/n".into(),
            old_page_count: 1,
            new_page_count: 1,
            pages: vec![PageDiff {
                old_page: Some(1),
                new_page: Some(1),
                summary: summary.clone(),
                lines,
            }],
            total: summary,
        }
    }

    fn make_diff() -> crate::pdf::diff3::ThreeWayDiff {
        let bm = one_page_doc(vec![
            line(DiffOp::Equal, Some(1), Some(1), "shared header"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA-MINE has been edited"),
            line(DiffOp::Equal, Some(3), Some(3), "shared footer"),
        ]);
        let bt = one_page_doc(vec![
            line(DiffOp::Equal, Some(1), Some(1), "shared header"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA-THEIRS got changed too"),
            line(DiffOp::Equal, Some(3), Some(3), "shared footer"),
        ]);
        three_way_from_diffs(&bm, &bt, "/b.pdf".into(), "/m.pdf".into(), "/t.pdf".into())
    }

    #[test]
    fn exports_non_empty_pdf_with_pages() {
        let d = make_diff();
        let out = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let r = export_diff3_pdf(&d, out.path()).unwrap();
        assert!(r.pages >= 1, "got {} pages", r.pages);
        assert_eq!(r.conflicts, 1);
        let bytes = std::fs::metadata(out.path()).unwrap().len();
        assert!(bytes > 400, "PDF suspiciously small: {bytes} bytes");
        let head = std::fs::read(out.path()).unwrap();
        assert!(head.starts_with(b"%PDF-"));
    }

    #[test]
    fn exports_handles_empty_diff() {
        let empty = crate::pdf::diff3::ThreeWayDiff {
            base_path: "/b.pdf".into(),
            mine_path: "/m.pdf".into(),
            theirs_path: "/t.pdf".into(),
            pages: vec![],
            total: Default::default(),
        };
        let out = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let r = export_diff3_pdf(&empty, out.path()).unwrap();
        assert_eq!(r.pages, 1);
        assert_eq!(r.conflicts, 0);
    }

    #[test]
    fn result_totals_match_diff_totals() {
        let d = make_diff();
        let out = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let r = export_diff3_pdf(&d, out.path()).unwrap();
        assert_eq!(r.conflicts, d.total.conflicts);
        assert_eq!(r.mine_only, d.total.mine_only);
        assert_eq!(r.theirs_only, d.total.theirs_only);
        assert_eq!(r.both_agree, d.total.both_agree);
    }

    #[test]
    fn wrap_to_width_handles_long_words() {
        let w = wrap_to_width("supercalifragilisticexpialidocious tail", 30.0, 9.0);
        assert!(w.len() >= 2, "got {w:?}");
        assert!(w.iter().any(|l| l.contains("tail")));
    }

    #[test]
    fn safe_strips_control_chars() {
        let s = safe("hello\tworld\nline\u{0007}bell");
        assert!(!s.contains('\t'));
        assert!(!s.contains('\n'));
        assert!(!s.contains('\u{0007}'));
        assert!(s.contains("hello"));
        assert!(s.contains("bell"));
    }

    #[test]
    fn renders_each_classification_without_panic() {
        let kinds = [
            ThreeWayKind::Unchanged,
            ThreeWayKind::MineOnly,
            ThreeWayKind::TheirsOnly,
            ThreeWayKind::BothAgree,
            ThreeWayKind::Conflict,
        ];
        let lines: Vec<ThreeWayLine> = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| ThreeWayLine {
                kind: *k,
                base_line: Some(i as u32 + 1),
                mine_line: Some(i as u32 + 1),
                theirs_line: Some(i as u32 + 1),
                base_text: format!("base {i}"),
                mine_text: Some(format!("mine {i}")),
                theirs_text: Some(format!("theirs {i}")),
            })
            .collect();
        let d = crate::pdf::diff3::ThreeWayDiff {
            base_path: "/b".into(),
            mine_path: "/m".into(),
            theirs_path: "/t".into(),
            pages: vec![crate::pdf::diff3::ThreeWayPage {
                page: 1,
                lines,
                summary: Default::default(),
            }],
            total: Default::default(),
        };
        let out = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        let r = export_diff3_pdf(&d, out.path()).unwrap();
        assert!(r.pages >= 1);
    }
}
