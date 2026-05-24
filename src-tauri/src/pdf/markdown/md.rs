//! Markdown emitter — turn the shared reflow `Block` IR into a Markdown
//! string. Pure function; no I/O.

use super::types::{MarkdownFlavour, MarkdownOptions};
use crate::pdf::reflow::types::{Block, ListKind};
use std::fmt::Write;

/// Convert a `Block` slice into a Markdown string.
pub fn emit_markdown(blocks: &[Block], opts: &MarkdownOptions) -> String {
    let mut out = String::with_capacity(blocks.len() * 64);
    let mut table_buf: Vec<Vec<String>> = Vec::new();
    let mut prev_was_list = false;

    for block in blocks {
        // Flush pending table on any non-TableRow block.
        if !matches!(block, Block::TableRow { .. }) && !table_buf.is_empty() {
            flush_table(&mut out, &table_buf, opts);
            table_buf.clear();
        }

        match block {
            Block::Body { text } => {
                if prev_was_list {
                    out.push('\n');
                }
                writeln!(out, "{}\n", escape(text)).ok();
                prev_was_list = false;
            }
            Block::Heading { level, text } => {
                if prev_was_list {
                    out.push('\n');
                }
                let hashes = "#".repeat((*level as usize).clamp(1, 6));
                writeln!(out, "{} {}\n", hashes, escape(text)).ok();
                prev_was_list = false;
            }
            Block::ListItem { kind, indent, text } => {
                let pad = "  ".repeat(*indent as usize);
                let marker = match kind {
                    ListKind::Bullet => "-",
                    ListKind::Number => "1.",
                };
                writeln!(out, "{}{} {}", pad, marker, escape(text)).ok();
                prev_was_list = true;
            }
            Block::TableRow { cells } => {
                table_buf.push(cells.clone());
                prev_was_list = false;
            }
        }
    }

    if !table_buf.is_empty() {
        flush_table(&mut out, &table_buf, opts);
    }

    out
}

fn flush_table(out: &mut String, rows: &[Vec<String>], opts: &MarkdownOptions) {
    if rows.is_empty() {
        return;
    }
    match opts.flavour {
        MarkdownFlavour::Gfm => {
            let header = &rows[0];
            writeln!(
                out,
                "| {} |",
                header
                    .iter()
                    .map(|c| escape_cell(c))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
            .ok();
            writeln!(out, "| {} |", vec!["---"; header.len()].join(" | ")).ok();
            for row in &rows[1..] {
                writeln!(
                    out,
                    "| {} |",
                    row.iter()
                        .map(|c| escape_cell(c))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
                .ok();
            }
            out.push('\n');
        }
        MarkdownFlavour::CommonMark => {
            // CommonMark has no native pipe-table syntax — fall back to a fenced
            // code block so the data is preserved verbatim.
            out.push_str("```\n");
            for row in rows {
                writeln!(out, "{}", row.join("\t")).ok();
            }
            out.push_str("```\n\n");
        }
    }
}

fn escape(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '<' | '>' => {
                s.push('\\');
                s.push(c);
            }
            _ => s.push(c),
        }
    }
    s
}

fn escape_cell(text: &str) -> String {
    text.replace('|', r"\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::reflow::types::{Block, ListKind};

    #[test]
    fn body_paragraph_round_trips_as_paragraph() {
        let blocks = vec![Block::Body {
            text: "Hello world".into(),
        }];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        // 'Hello world' contains no special markdown chars, so escape is no-op.
        assert!(md.trim().starts_with("Hello world"));
    }

    #[test]
    fn heading_renders_with_hashes() {
        let blocks = vec![
            Block::Heading {
                level: 1,
                text: "Title".into(),
            },
            Block::Heading {
                level: 2,
                text: "Section".into(),
            },
            Block::Heading {
                level: 3,
                text: "Sub".into(),
            },
        ];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains("# Title"));
        assert!(md.contains("## Section"));
        assert!(md.contains("### Sub"));
    }

    #[test]
    fn list_items_use_dash_for_bullets() {
        let blocks = vec![
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 0,
                text: "one".into(),
            },
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 0,
                text: "two".into(),
            },
        ];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains("- one"));
        assert!(md.contains("- two"));
    }

    #[test]
    fn list_items_numbered_use_one_dot() {
        let blocks = vec![
            Block::ListItem {
                kind: ListKind::Number,
                indent: 0,
                text: "a".into(),
            },
            Block::ListItem {
                kind: ListKind::Number,
                indent: 0,
                text: "b".into(),
            },
        ];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains("1. a"));
        assert!(md.contains("1. b"));
    }

    #[test]
    fn nested_list_uses_indentation() {
        let blocks = vec![
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 0,
                text: "outer".into(),
            },
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 1,
                text: "inner".into(),
            },
        ];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains("- outer"));
        assert!(md.contains("  - inner"));
    }

    #[test]
    fn table_rows_emit_gfm_pipe_table_with_header_separator() {
        let blocks = vec![
            Block::TableRow {
                cells: vec!["A".into(), "B".into()],
            },
            Block::TableRow {
                cells: vec!["1".into(), "2".into()],
            },
        ];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains("| A | B |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| 1 | 2 |"));
    }

    #[test]
    fn commonmark_flavour_uses_code_fallback_for_tables() {
        let blocks = vec![Block::TableRow {
            cells: vec!["A".into(), "B".into()],
        }];
        let opts = MarkdownOptions {
            flavour: MarkdownFlavour::CommonMark,
            ..Default::default()
        };
        let md = emit_markdown(&blocks, &opts);
        assert!(md.contains("```"));
        assert!(md.contains("A\tB"));
    }

    #[test]
    fn special_chars_escaped() {
        let blocks = vec![Block::Body {
            text: "a*b_c[d]e".into(),
        }];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains(r"a\*b\_c\[d\]e"));
    }

    #[test]
    fn pipe_in_cell_escaped() {
        let blocks = vec![Block::TableRow {
            cells: vec!["A|B".into(), "C".into()],
        }];
        let md = emit_markdown(&blocks, &MarkdownOptions::default());
        assert!(md.contains(r"A\|B"));
    }
}
