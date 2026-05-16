// Markdown → PDF renderer.
//
// We parse markdown with `pulldown-cmark` into a token stream, then emit
// PDF content stream operators directly via `lopdf`. We use the 14 standard
// "base" PDF fonts (Helvetica family + Courier) so no font embedding is
// needed — the output PDF stays tiny and works in every viewer ever made.
//
// Supported elements:
//   - Headings H1-H6 (sized 24/20/16/14/12/11)
//   - Paragraphs with bold/italic/code inline
//   - Bullet & numbered lists (single level — nested falls back flat)
//   - Block code (Courier, indented)
//   - Hr rules
//   - Blockquotes (left indent)
//
// Layout: configurable page size (A4/Letter/Legal), 50pt margins.
// Auto page-break when cursor drops below the bottom margin.

use crate::pdf::PdfError;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Md2PdfOpts {
    /// Markdown source text.
    pub markdown: String,
    /// "A4" | "Letter" | "Legal". Defaults to A4.
    #[serde(default)]
    pub page_size: String,
}

const PT_A4: (f32, f32) = (595.0, 842.0);
const PT_LETTER: (f32, f32) = (612.0, 792.0);
const PT_LEGAL: (f32, f32) = (612.0, 1008.0);

const MARGIN: f32 = 50.0;
const BASE_FONT_SIZE: f32 = 11.0;
const LINE_HEIGHT_RATIO: f32 = 1.35;

const F_REGULAR: &str = "F1"; // Helvetica
const F_BOLD: &str = "F2"; // Helvetica-Bold
const F_ITALIC: &str = "F3"; // Helvetica-Oblique
const F_BOLDIT: &str = "F4"; // Helvetica-BoldOblique
const F_MONO: &str = "F5"; // Courier

#[derive(Default, Clone, Copy)]
struct InlineState {
    bold: bool,
    italic: bool,
    code: bool,
}

impl InlineState {
    fn font(&self) -> &'static str {
        match (self.bold, self.italic, self.code) {
            (_, _, true) => F_MONO,
            (true, true, _) => F_BOLDIT,
            (true, false, _) => F_BOLD,
            (false, true, _) => F_ITALIC,
            (false, false, _) => F_REGULAR,
        }
    }
}

struct Renderer {
    page_w: f32,
    page_h: f32,
    pages: Vec<Vec<Operation>>,
    current: Vec<Operation>,
    y: f32,
    in_text: bool,
}

impl Renderer {
    fn new(page_w: f32, page_h: f32) -> Self {
        Self {
            page_w,
            page_h,
            pages: Vec::new(),
            current: Vec::new(),
            y: page_h - MARGIN,
            in_text: false,
        }
    }

    fn ensure_text_block(&mut self) {
        if !self.in_text {
            self.current.push(Operation::new("BT", vec![]));
            self.in_text = true;
        }
    }

    fn close_text_block(&mut self) {
        if self.in_text {
            self.current.push(Operation::new("ET", vec![]));
            self.in_text = false;
        }
    }

    fn page_break(&mut self) {
        self.close_text_block();
        let done = std::mem::take(&mut self.current);
        self.pages.push(done);
        self.y = self.page_h - MARGIN;
    }

    fn ensure_room(&mut self, line_h: f32) {
        if self.y - line_h < MARGIN {
            self.page_break();
        }
    }

    fn move_to(&mut self, x: f32, y: f32) {
        self.ensure_text_block();
        self.current
            .push(Operation::new("Td", vec![Object::Real(x), Object::Real(y)]));
    }

    fn set_font(&mut self, name: &str, size: f32) {
        self.ensure_text_block();
        self.current.push(Operation::new(
            "Tf",
            vec![Object::Name(name.as_bytes().to_vec()), Object::Real(size)],
        ));
    }

    fn show(&mut self, text: &str) {
        self.ensure_text_block();
        self.current.push(Operation::new(
            "Tj",
            vec![Object::string_literal(text.to_string())],
        ));
    }

    fn finish(mut self) -> Vec<Vec<Operation>> {
        self.close_text_block();
        if !self.current.is_empty() {
            self.pages.push(std::mem::take(&mut self.current));
        }
        if self.pages.is_empty() {
            self.pages.push(Vec::new());
        }
        self.pages
    }
}

fn flush_runs(r: &mut Renderer, runs: &mut Vec<(InlineState, String)>, size: f32, indent: f32) {
    if runs.is_empty() {
        return;
    }
    let line_h = size * LINE_HEIGHT_RATIO;
    let max_x = r.page_w - MARGIN;
    let words: Vec<(InlineState, String)> = runs
        .drain(..)
        .flat_map(|(st, txt)| {
            txt.split_inclusive(' ')
                .map(|w| (st, w.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();

    r.ensure_room(line_h);
    r.move_to(MARGIN + indent, r.y - size);
    let mut current_x = MARGIN + indent;
    for (i, (st, word)) in words.into_iter().enumerate() {
        let w = estimate_width(&word, size);
        if i > 0 && current_x + w > max_x {
            r.y -= line_h;
            r.ensure_room(line_h);
            r.close_text_block();
            r.move_to(MARGIN + indent, r.y - size);
            current_x = MARGIN + indent;
        }
        r.set_font(st.font(), size);
        r.show(&word);
        current_x += w;
    }
    r.y -= line_h;
}

pub fn render(input_md: &str, output: &Path, opts: Md2PdfOpts) -> Result<u32, PdfError> {
    let (page_w, page_h) = match opts.page_size.as_str() {
        "Letter" => PT_LETTER,
        "Legal" => PT_LEGAL,
        _ => PT_A4,
    };

    let mut parser_opts = Options::empty();
    parser_opts.insert(Options::ENABLE_STRIKETHROUGH);
    parser_opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(input_md, parser_opts);

    let mut r = Renderer::new(page_w, page_h);
    let mut inline = InlineState::default();
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_block_quote = false;
    let mut in_code_block = false;
    let mut current_block: Vec<(InlineState, String)> = Vec::new();
    let mut heading_size: Option<f32> = None;

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                heading_size = Some(heading_size_for(level));
                inline.bold = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                let size = heading_size.take().unwrap_or(BASE_FONT_SIZE);
                flush_runs(&mut r, &mut current_block, size, 0.0);
                inline.bold = false;
                r.y -= size * 0.4;
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                let indent = if in_block_quote { 20.0 } else { 0.0 };
                flush_runs(&mut r, &mut current_block, BASE_FONT_SIZE, indent);
                r.y -= BASE_FONT_SIZE * 0.4;
            }
            Event::Start(Tag::BlockQuote(_)) => in_block_quote = true,
            Event::End(TagEnd::BlockQuote(_)) => in_block_quote = false,
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                inline.code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_runs(&mut r, &mut current_block, BASE_FONT_SIZE, 20.0);
                inline.code = false;
                in_code_block = false;
                r.y -= BASE_FONT_SIZE * 0.5;
            }
            Event::Start(Tag::List(first_num)) => list_stack.push(first_num),
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                r.y -= BASE_FONT_SIZE * 0.3;
            }
            Event::Start(Tag::Item) => {
                let bullet = match list_stack.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{n}. ");
                        *n += 1;
                        s
                    }
                    _ => "• ".to_string(),
                };
                current_block.push((inline, bullet));
            }
            Event::End(TagEnd::Item) => {
                let indent = 20.0 * list_stack.len().max(1) as f32;
                flush_runs(&mut r, &mut current_block, BASE_FONT_SIZE, indent);
            }
            Event::Start(Tag::Emphasis) => inline.italic = true,
            Event::End(TagEnd::Emphasis) => inline.italic = false,
            Event::Start(Tag::Strong) => inline.bold = true,
            Event::End(TagEnd::Strong) => inline.bold = false,
            Event::Code(s) => {
                let mut st = inline;
                st.code = true;
                current_block.push((st, s.to_string()));
            }
            Event::Text(s) => {
                let mut st = inline;
                if in_code_block {
                    st.code = true;
                }
                current_block.push((st, s.to_string()));
            }
            Event::SoftBreak => {
                current_block.push((inline, " ".to_string()));
            }
            Event::HardBreak => {
                let indent = if in_block_quote { 20.0 } else { 0.0 };
                flush_runs(&mut r, &mut current_block, BASE_FONT_SIZE, indent);
            }
            Event::Rule => {
                r.close_text_block();
                let y = r.y - 4.0;
                r.current.push(Operation::new("q", vec![]));
                r.current.push(Operation::new(
                    "rg",
                    vec![0.6.into(), 0.6.into(), 0.6.into()],
                ));
                r.current.push(Operation::new(
                    "re",
                    vec![
                        Object::Real(MARGIN),
                        Object::Real(y),
                        Object::Real(r.page_w - 2.0 * MARGIN),
                        Object::Real(0.6),
                    ],
                ));
                r.current.push(Operation::new("f", vec![]));
                r.current.push(Operation::new("Q", vec![]));
                r.y -= 12.0;
            }
            _ => {}
        }
    }

    flush_runs(&mut r, &mut current_block, BASE_FONT_SIZE, 0.0);

    let pages_ops = r.finish();
    let n_pages = pages_ops.len() as u32;

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let f_regular = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let f_bold = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
    });
    let f_italic = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Oblique",
    });
    let f_boldit = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-BoldOblique",
    });
    let f_mono = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    });
    let font_dict = dictionary! {
        F_REGULAR => f_regular,
        F_BOLD => f_bold,
        F_ITALIC => f_italic,
        F_BOLDIT => f_boldit,
        F_MONO => f_mono,
    };
    let resources_id = doc.add_object(dictionary! {
        "Font" => font_dict,
    });

    let mut kids: Vec<Object> = Vec::with_capacity(pages_ops.len());
    for ops in pages_ops {
        let content = Content { operations: ops };
        let encoded = content
            .encode()
            .map_err(|e| PdfError::Other(format!("md2pdf encode: {e}")))?;
        let stream_id = doc.add_object(Stream::new(dictionary! {}, encoded));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), page_w.into(), page_h.into()],
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

    let _ = input_md;
    doc.compress();
    doc.save(output)?;
    Ok(n_pages)
}

fn heading_size_for(level: HeadingLevel) -> f32 {
    match level {
        HeadingLevel::H1 => 24.0,
        HeadingLevel::H2 => 20.0,
        HeadingLevel::H3 => 16.0,
        HeadingLevel::H4 => 14.0,
        HeadingLevel::H5 => 12.0,
        HeadingLevel::H6 => 11.0,
    }
}

fn estimate_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_md_produces_one_page() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let n = render(
            "",
            &out,
            Md2PdfOpts {
                markdown: "".into(),
                page_size: "A4".into(),
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        let doc = Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn renders_heading_and_paragraph() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let md = "# Hello\n\nThis is **bold** and *italic* and `code`.\n";
        let n = render(
            md,
            &out,
            Md2PdfOpts {
                markdown: md.into(),
                page_size: "A4".into(),
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn renders_list() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let md = "- one\n- two\n- three\n\n1. first\n2. second\n";
        let n = render(
            md,
            &out,
            Md2PdfOpts {
                markdown: md.into(),
                page_size: "Letter".into(),
            },
        )
        .unwrap();
        assert!(n >= 1);
    }

    #[test]
    fn paginates_long_content() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let mut md = String::new();
        for i in 0..200 {
            md.push_str(&format!("Paragraph {i} with some text.\n\n"));
        }
        let n = render(
            &md,
            &out,
            Md2PdfOpts {
                markdown: md.clone(),
                page_size: "A4".into(),
            },
        )
        .unwrap();
        assert!(n >= 2, "Expected pagination, got {n} pages");
    }
}
