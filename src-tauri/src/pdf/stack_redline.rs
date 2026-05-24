// Stack Redline PDF export — v3.23.0.
//
// Produces a self-contained, shareable PDF that bakes a word-level
// redline view of two PDFs into one document. Inserts are rendered in
// green with an underline; deletes in red with a strikethrough.
//
// Unlike `diff::export_report`, which round-trips through markdown and
// loses color/word granularity, this module emits PDF content streams
// directly via `lopdf` so we can place per-token colored runs. It uses
// the 14 standard "base" PDF fonts (Helvetica family + Courier) so no
// font embedding is needed — output stays tiny and works in every PDF
// viewer.
//
// The output is intentionally simple and printable:
//   - 612x792 (US Letter) pages, 54pt margins
//   - Cover page (title + old/new paths + summary totals)
//   - One section per changed page; page heading + per-line redline
//   - Per-line gutter showing 1-based line numbers from each side
//
// Buyer hook (Litera Compare $400/seat/yr): "Share the redline as a
// single PDF — recipients don't need Slab installed to read it."

use crate::pdf::diff::{DiffOp, DocDiff, LineDiff, PageDiff, WordOp};
use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use std::path::Path;

const PAGE_W: f32 = 612.0;
const PAGE_H: f32 = 792.0;
const MARGIN: f32 = 54.0;

const BODY_SIZE: f32 = 10.5;
const LINE_HEIGHT: f32 = 14.5;
const HEADING_SIZE: f32 = 16.0;
const PAGE_HEADING_SIZE: f32 = 13.0;
const META_SIZE: f32 = 9.5;
const GUTTER_W: f32 = 56.0;

// Standard 14 font aliases.
const F_REG: &str = "F1";
const F_BOLD: &str = "F2";
const F_IT: &str = "F3";
const F_BIT: &str = "F4";
const F_MONO: &str = "F5";

// Theme colors (RGB 0..1).
const C_INS: [f32; 3] = [0.184, 0.580, 0.275]; // green
const C_DEL: [f32; 3] = [0.760, 0.231, 0.231]; // red
const C_TEXT: [f32; 3] = [0.10, 0.11, 0.12];
const C_MUTED: [f32; 3] = [0.45, 0.46, 0.50];
const C_RULE: [f32; 3] = [0.82, 0.83, 0.85];

/// Result of exporting a redline PDF.
#[derive(Debug, Clone, Copy)]
pub struct RedlineResult {
    pub pages: u32,
    pub inserts: u32,
    pub deletes: u32,
}

/// Render `diff` to a single, shareable redline PDF at `output`.
pub fn export_redline(diff: &DocDiff, output: &Path) -> Result<RedlineResult, PdfError> {
    let mut renderer = Renderer::new();

    // ---- Cover page ----
    renderer.text_left(
        MARGIN,
        PAGE_H - MARGIN - 6.0,
        "Slab Redline",
        F_BOLD,
        HEADING_SIZE,
        C_TEXT,
    );
    renderer.y = PAGE_H - MARGIN - HEADING_SIZE - 8.0;
    renderer.meta_line(&format!(
        "Old: {}",
        path_str(&diff.old_path.display().to_string())
    ));
    renderer.meta_line(&format!(
        "New: {}",
        path_str(&diff.new_path.display().to_string())
    ));
    renderer.meta_line(&format!(
        "Pages: {} (old) -> {} (new)",
        diff.old_page_count, diff.new_page_count
    ));
    renderer.meta_line(&format!(
        "Totals: +{} added  -{} removed  ~{} changed",
        diff.total.added, diff.total.removed, diff.total.changed
    ));
    renderer.y -= 6.0;
    renderer.hr();
    renderer.y -= 8.0;
    renderer.meta_line("Legend:");
    renderer.legend_swatch("inserted", C_INS, true);
    renderer.legend_swatch("deleted", C_DEL, false);

    let (mut ins_count, mut del_count) = (0u32, 0u32);

    let mut wrote_any = false;
    for page in &diff.pages {
        let s = &page.summary;
        if s.added == 0 && s.removed == 0 && s.changed == 0 {
            continue;
        }
        wrote_any = true;
        renderer.ensure_room(PAGE_HEADING_SIZE + LINE_HEIGHT * 2.0);
        renderer.y -= 18.0;
        renderer.text_left(
            MARGIN,
            renderer.y,
            &page_heading(page),
            F_BOLD,
            PAGE_HEADING_SIZE,
            C_TEXT,
        );
        renderer.y -= PAGE_HEADING_SIZE + 2.0;
        renderer.text_left(
            MARGIN,
            renderer.y,
            &format!(
                "+{} added   -{} removed   ~{} changed",
                s.added, s.removed, s.changed
            ),
            F_REG,
            META_SIZE,
            C_MUTED,
        );
        renderer.y -= META_SIZE + 6.0;
        renderer.hr_subtle();
        renderer.y -= 6.0;

        for line in &page.lines {
            if line.op == DiffOp::Equal {
                continue;
            }
            let (i, d) = renderer.draw_line(line);
            ins_count += i;
            del_count += d;
        }
    }

    if !wrote_any {
        renderer.y -= 18.0;
        renderer.text_left(
            MARGIN,
            renderer.y,
            "No differences detected.",
            F_IT,
            BODY_SIZE,
            C_MUTED,
        );
    }

    // Footer on every page.
    let n_pages = renderer.commit_to_pdf(output)?;

    Ok(RedlineResult {
        pages: n_pages,
        inserts: ins_count,
        deletes: del_count,
    })
}

fn page_heading(p: &PageDiff) -> String {
    match (p.old_page, p.new_page) {
        (Some(o), Some(n)) if o == n => format!("Page {o}"),
        (Some(o), Some(n)) => format!("Old p.{o}  ->  New p.{n}"),
        (Some(o), None) => format!("Old p.{o}  (removed)"),
        (None, Some(n)) => format!("New p.{n}  (added)"),
        (None, None) => "(orphan)".into(),
    }
}

fn path_str(s: &str) -> String {
    // Avoid breaking the PDF text encoder on weird control chars; the
    // standard 14 fonts use WinAnsi-ish encoding. Replace anything <0x20.
    s.chars()
        .map(|c| if (c as u32) < 0x20 { ' ' } else { c })
        .collect()
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

struct Renderer {
    pages: Vec<Vec<Operation>>,
    current: Vec<Operation>,
    y: f32,
    in_text: bool,
    cur_font: Option<&'static str>,
    cur_size: f32,
    cur_color: Option<[f32; 3]>,
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
        }
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
            // `rg` (non-stroke RGB) works inside text blocks.
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
        // Use absolute matrix positioning so we can jump anywhere.
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

    fn text_left(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
        font: &'static str,
        size: f32,
        color: [f32; 3],
    ) {
        self.set_font(font, size);
        self.set_fill_rgb(color);
        self.move_to(x, y);
        self.show(text);
    }

    fn meta_line(&mut self, text: &str) {
        self.ensure_room(META_SIZE + 2.0);
        self.text_left(MARGIN, self.y, text, F_REG, META_SIZE, C_TEXT);
        self.y -= META_SIZE + 4.0;
    }

    fn legend_swatch(&mut self, label: &str, color: [f32; 3], underline: bool) {
        self.ensure_room(META_SIZE + 2.0);
        self.text_left(
            MARGIN,
            self.y,
            &format!("  {label}"),
            F_BOLD,
            META_SIZE,
            color,
        );
        if underline {
            self.underline_at(
                MARGIN,
                self.y - 2.0,
                estimate_width(&format!("  {label}"), META_SIZE),
                color,
            );
        } else {
            self.strike_at(
                MARGIN,
                self.y,
                estimate_width(&format!("  {label}"), META_SIZE),
                color,
            );
        }
        self.y -= META_SIZE + 4.0;
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

    fn hr_subtle(&mut self) {
        self.hr();
    }

    fn underline_at(&mut self, x: f32, y: f32, w: f32, color: [f32; 3]) {
        self.close_text();
        self.current.push(Operation::new("q", vec![]));
        self.current.push(Operation::new(
            "RG",
            vec![
                Object::Real(color[0]),
                Object::Real(color[1]),
                Object::Real(color[2]),
            ],
        ));
        self.current
            .push(Operation::new("w", vec![Object::Real(0.7)]));
        self.current
            .push(Operation::new("m", vec![Object::Real(x), Object::Real(y)]));
        self.current.push(Operation::new(
            "l",
            vec![Object::Real(x + w), Object::Real(y)],
        ));
        self.current.push(Operation::new("S", vec![]));
        self.current.push(Operation::new("Q", vec![]));
        self.cur_color = None;
    }

    fn strike_at(&mut self, x: f32, y: f32, w: f32, color: [f32; 3]) {
        // Mid-glyph strike sits ~30% of body-size above baseline.
        let sy = y + BODY_SIZE * 0.30;
        self.close_text();
        self.current.push(Operation::new("q", vec![]));
        self.current.push(Operation::new(
            "RG",
            vec![
                Object::Real(color[0]),
                Object::Real(color[1]),
                Object::Real(color[2]),
            ],
        ));
        self.current
            .push(Operation::new("w", vec![Object::Real(0.9)]));
        self.current
            .push(Operation::new("m", vec![Object::Real(x), Object::Real(sy)]));
        self.current.push(Operation::new(
            "l",
            vec![Object::Real(x + w), Object::Real(sy)],
        ));
        self.current.push(Operation::new("S", vec![]));
        self.current.push(Operation::new("Q", vec![]));
        self.cur_color = None;
    }

    fn ensure_room(&mut self, h: f32) {
        if self.y - h < MARGIN + 24.0 {
            self.page_break();
        }
    }

    fn page_break(&mut self) {
        // Footer: page N
        let footer_y = MARGIN - 18.0;
        let page_no = self.pages.len() as u32 + 1;
        self.text_left(
            MARGIN,
            footer_y,
            &format!("Slab Redline — page {page_no}"),
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
    }

    fn draw_line(&mut self, line: &LineDiff) -> (u32, u32) {
        self.ensure_room(LINE_HEIGHT + 6.0);

        // Gutter: line numbers (old | new).
        let old_s = line.old_line.map(|n| n.to_string()).unwrap_or_default();
        let new_s = line.new_line.map(|n| n.to_string()).unwrap_or_default();
        let gutter_text = format!("{:>3}  {:>3}", old_s, new_s);
        self.text_left(MARGIN, self.y, &gutter_text, F_MONO, 8.5, C_MUTED);

        // Marker.
        let marker = match line.op {
            DiffOp::Insert => "+",
            DiffOp::Delete => "-",
            DiffOp::Equal => " ",
        };
        let marker_color = match line.op {
            DiffOp::Insert => C_INS,
            DiffOp::Delete => C_DEL,
            DiffOp::Equal => C_MUTED,
        };
        self.text_left(
            MARGIN + GUTTER_W - 14.0,
            self.y,
            marker,
            F_BOLD,
            BODY_SIZE,
            marker_color,
        );

        // Body — word-level if present, else whole-line color.
        let body_x = MARGIN + GUTTER_W;
        let max_x = PAGE_W - MARGIN;
        let baseline = self.y;

        let mut ins = 0u32;
        let mut del = 0u32;
        let mut x = body_x;

        let segments: Vec<(WordOp, &str)> = if let Some(words) = &line.words {
            words.iter().map(|w| (w.op, w.text.as_str())).collect()
        } else {
            // No word-level breakdown — color whole line by line op.
            let op = match line.op {
                DiffOp::Insert => WordOp::Insert,
                DiffOp::Delete => WordOp::Delete,
                DiffOp::Equal => WordOp::Equal,
            };
            vec![(op, line.text.as_str())]
        };

        // Track decoration spans so we can stroke them after ET.
        let mut decos: Vec<(f32, f32, WordOp)> = Vec::new();

        for (op, text) in segments {
            if text.is_empty() {
                continue;
            }
            let w = estimate_width(text, BODY_SIZE);

            if x + w > max_x {
                // Soft-wrap: emit a continuation row below the gutter at body_x.
                // Flush current decorations on the existing line first.
                self.flush_decos(&decos);
                decos.clear();
                self.y -= LINE_HEIGHT;
                self.ensure_room(LINE_HEIGHT);
                x = body_x;
                // (no gutter on continuation rows — paralegal-friendly)
            }

            let color = match op {
                WordOp::Insert => C_INS,
                WordOp::Delete => C_DEL,
                WordOp::Equal => C_TEXT,
            };
            self.set_font(F_REG, BODY_SIZE);
            self.set_fill_rgb(color);
            self.move_to(x, self.y);
            self.show(text);

            match op {
                WordOp::Insert => {
                    decos.push((x, x + w, WordOp::Insert));
                    ins += text.split_whitespace().count() as u32;
                }
                WordOp::Delete => {
                    decos.push((x, x + w, WordOp::Delete));
                    del += text.split_whitespace().count() as u32;
                }
                WordOp::Equal => {}
            }
            x += w;
        }

        // Special case: whole-line inserts/deletes still want decorations
        // even when there were no per-token segments and the trimmed token
        // counter under-reports trailing whitespace tokens. Use the line
        // op as a fallback.
        if decos.is_empty() {
            match line.op {
                DiffOp::Insert => {
                    let w = estimate_width(&line.text, BODY_SIZE);
                    decos.push((body_x, body_x + w, WordOp::Insert));
                    if ins == 0 {
                        ins = 1;
                    }
                }
                DiffOp::Delete => {
                    let w = estimate_width(&line.text, BODY_SIZE);
                    decos.push((body_x, body_x + w, WordOp::Delete));
                    if del == 0 {
                        del = 1;
                    }
                }
                _ => {}
            }
        }

        self.flush_decos(&decos);

        // Restore baseline cursor if soft-wrap moved us; advance to next line.
        let _ = baseline; // kept for future use (e.g. background tint)
        self.y -= LINE_HEIGHT;

        (ins, del)
    }

    fn flush_decos(&mut self, decos: &[(f32, f32, WordOp)]) {
        for (x0, x1, op) in decos {
            match op {
                WordOp::Insert => self.underline_at(*x0, self.y - 1.5, x1 - x0, C_INS),
                WordOp::Delete => self.strike_at(*x0, self.y, x1 - x0, C_DEL),
                WordOp::Equal => {}
            }
        }
    }

    fn commit_to_pdf(mut self, output: &Path) -> Result<u32, PdfError> {
        // Footer + close current page.
        if !self.current.is_empty() || self.pages.is_empty() {
            let page_no = self.pages.len() as u32 + 1;
            self.text_left(
                MARGIN,
                MARGIN - 18.0,
                &format!("Slab Redline — page {page_no}"),
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
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let f_bold = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica-Bold",
        });
        let f_it = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica-Oblique",
        });
        let f_bit = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica-BoldOblique",
        });
        let f_mono = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                F_REG => f_reg,
                F_BOLD => f_bold,
                F_IT => f_it,
                F_BIT => f_bit,
                F_MONO => f_mono,
            },
        });

        let mut kids: Vec<Object> = Vec::with_capacity(self.pages.len());
        for ops in self.pages {
            let content = Content { operations: ops };
            let encoded = content
                .encode()
                .map_err(|e| PdfError::Other(format!("stack_redline encode: {e}")))?;
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
    // Conservative average advance for Helvetica at `size`.
    text.chars().count() as f32 * size * 0.52
}

/// Sanitize text for the standard 14 PDF fonts (WinAnsi-ish encoding).
/// Replaces control chars and most non-Latin-1 with a `?` so the PDF
/// stays viewable. Tabs become two spaces.
fn safe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' => out.push_str("  "),
            '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 => out.push(' '),
            c if (c as u32) <= 0xFF => out.push(c),
            // Common typographic punctuation → ASCII fallback.
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201C}' | '\u{201D}' => out.push('"'),
            '\u{2013}' | '\u{2014}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            _ => out.push('?'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::diff::{DiffSummary, WordDiff};

    fn changed_line(old: u32, new: u32, words: Vec<(WordOp, &str)>) -> LineDiff {
        // For the "changed" representation, surface the line as an Insert with
        // word-level breakdown — the renderer uses `words` regardless.
        let text: String = words.iter().map(|(_, t)| *t).collect();
        LineDiff {
            op: DiffOp::Insert,
            old_line: Some(old),
            new_line: Some(new),
            text,
            words: Some(
                words
                    .into_iter()
                    .map(|(op, t)| WordDiff {
                        op,
                        text: t.to_string(),
                    })
                    .collect(),
            ),
        }
    }

    fn one_page_diff(lines: Vec<LineDiff>) -> DocDiff {
        DocDiff {
            old_path: std::path::PathBuf::from("/tmp/old.pdf"),
            new_path: std::path::PathBuf::from("/tmp/new.pdf"),
            old_page_count: 1,
            new_page_count: 1,
            pages: vec![PageDiff {
                old_page: Some(1),
                new_page: Some(1),
                lines,
                summary: DiffSummary {
                    added: 1,
                    removed: 0,
                    changed: 1,
                },
            }],
            total: DiffSummary {
                added: 1,
                removed: 0,
                changed: 1,
            },
        }
    }

    #[test]
    fn export_redline_writes_valid_pdf_with_no_changes() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("redline.pdf");
        let diff = DocDiff {
            old_path: std::path::PathBuf::from("/tmp/a.pdf"),
            new_path: std::path::PathBuf::from("/tmp/b.pdf"),
            old_page_count: 0,
            new_page_count: 0,
            pages: vec![],
            total: DiffSummary::default(),
        };
        let res = export_redline(&diff, &out).unwrap();
        assert!(res.pages >= 1, "expected at least one page (cover)");
        assert_eq!(res.inserts, 0);
        assert_eq!(res.deletes, 0);
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let loaded = Document::load(&out).unwrap();
        assert_eq!(loaded.get_pages().len(), res.pages as usize);
    }

    #[test]
    fn export_redline_inserts_and_deletes_render_words() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("redline.pdf");
        let lines = vec![
            changed_line(
                1,
                1,
                vec![
                    (WordOp::Equal, "The party of the "),
                    (WordOp::Delete, "first part "),
                    (WordOp::Insert, "second part "),
                    (WordOp::Equal, "agrees to indemnify."),
                ],
            ),
            LineDiff {
                op: DiffOp::Insert,
                old_line: None,
                new_line: Some(2),
                text: "Entirely new sentence appears here.".into(),
                words: None,
            },
            LineDiff {
                op: DiffOp::Delete,
                old_line: Some(2),
                new_line: None,
                text: "Entirely old sentence going away.".into(),
                words: None,
            },
        ];
        let diff = one_page_diff(lines);
        let res = export_redline(&diff, &out).unwrap();
        assert!(res.pages >= 1);
        assert!(res.inserts >= 1, "expected at least one insert counted");
        assert!(res.deletes >= 1, "expected at least one delete counted");
        let loaded = Document::load(&out).unwrap();
        assert_eq!(loaded.get_pages().len(), res.pages as usize);
    }

    #[test]
    fn export_redline_handles_long_lines_and_paginates() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("redline.pdf");
        let mut lines = Vec::new();
        for i in 0..120 {
            lines.push(LineDiff {
                op: DiffOp::Insert,
                old_line: None,
                new_line: Some(i + 1),
                text: format!("Inserted line number {i} with some payload text that is reasonably long to exercise wrapping."),
                words: None,
            });
        }
        let diff = one_page_diff(lines);
        let res = export_redline(&diff, &out).unwrap();
        assert!(
            res.pages >= 2,
            "expected pagination, got {} pages",
            res.pages
        );
        let loaded = Document::load(&out).unwrap();
        assert_eq!(loaded.get_pages().len(), res.pages as usize);
    }

    #[test]
    fn safe_strips_control_and_replaces_smart_punct() {
        assert_eq!(safe("a\tb"), "a  b");
        assert_eq!(safe("hi\u{0007}!"), "hi !");
        assert_eq!(safe("\u{201C}quoted\u{201D}"), "\"quoted\"");
        assert_eq!(safe("em\u{2014}dash"), "em-dash");
        assert_eq!(safe("dots\u{2026}"), "dots...");
    }
}
