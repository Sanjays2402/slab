//! HTML emitter — turn `Block` IR into semantic HTML5. Pure function.

use super::types::HtmlOptions;
use crate::pdf::reflow::types::{Block, ListKind};
use std::fmt::Write;

const DEFAULT_CSS: &str = r#"
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;max-width:780px;margin:2rem auto;padding:0 1rem;color:#111;line-height:1.6}
h1,h2,h3,h4,h5,h6{color:#0a0a0a;margin-top:1.8em;line-height:1.25}
table{border-collapse:collapse;width:100%;margin:1em 0}
th,td{border:1px solid #ddd;padding:6px 10px;text-align:left;vertical-align:top}
th{background:#f6f6f6}
ul,ol{padding-left:1.4em}
article{padding:1rem 0}
p{margin:0.6em 0}
"#;

pub fn emit_html(blocks: &[Block], opts: &HtmlOptions) -> String {
    let mut out = String::with_capacity(blocks.len() * 96);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"UTF-8\">\n");
    out.push_str("<meta name=\"generator\" content=\"Slab v3.17.0 Markdown\">\n");
    out.push_str("<title>Document</title>\n");
    if opts.embed_css {
        out.push_str("<style>");
        out.push_str(DEFAULT_CSS);
        out.push_str("</style>\n");
    }
    out.push_str("</head>\n<body>\n");
    if opts.semantic_tags {
        out.push_str("<article>\n");
    }

    let mut i = 0;
    let mut in_list: Option<ListKind> = None;
    while i < blocks.len() {
        if !matches!(blocks[i], Block::ListItem { .. }) && in_list.is_some() {
            out.push_str(close_list_tag(in_list.unwrap()));
            in_list = None;
        }
        match &blocks[i] {
            Block::Body { text } => {
                writeln!(out, "<p>{}</p>", escape(text)).ok();
                i += 1;
            }
            Block::Heading { level, text } => {
                let lvl = (*level as usize).clamp(1, 6);
                writeln!(out, "<h{0}>{1}</h{0}>", lvl, escape(text)).ok();
                i += 1;
            }
            Block::ListItem { kind, text, .. } => {
                if in_list != Some(*kind) {
                    if let Some(prev) = in_list {
                        out.push_str(close_list_tag(prev));
                    }
                    out.push_str(open_list_tag(*kind));
                    in_list = Some(*kind);
                }
                writeln!(out, "  <li>{}</li>", escape(text)).ok();
                i += 1;
            }
            Block::TableRow { .. } => {
                let start = i;
                while i < blocks.len() && matches!(blocks[i], Block::TableRow { .. }) {
                    i += 1;
                }
                emit_table(&mut out, &blocks[start..i]);
            }
        }
    }
    if let Some(prev) = in_list {
        out.push_str(close_list_tag(prev));
    }

    if opts.semantic_tags {
        out.push_str("</article>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn open_list_tag(k: ListKind) -> &'static str {
    match k {
        ListKind::Bullet => "<ul>\n",
        ListKind::Number => "<ol>\n",
    }
}

fn close_list_tag(k: ListKind) -> &'static str {
    match k {
        ListKind::Bullet => "</ul>\n",
        ListKind::Number => "</ol>\n",
    }
}

fn emit_table(out: &mut String, rows: &[Block]) {
    if rows.is_empty() {
        return;
    }
    let cells_of = |b: &Block| -> Vec<String> {
        if let Block::TableRow { cells } = b {
            cells.clone()
        } else {
            Vec::new()
        }
    };
    out.push_str("<table>\n<thead>\n<tr>");
    for c in cells_of(&rows[0]) {
        write!(out, "<th>{}</th>", escape(&c)).ok();
    }
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in &rows[1..] {
        out.push_str("<tr>");
        for c in cells_of(row) {
            write!(out, "<td>{}</td>", escape(&c)).ok();
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::reflow::types::{Block, ListKind};

    #[test]
    fn produces_valid_doctype_and_article() {
        let blocks = vec![Block::Body { text: "Hi".into() }];
        let html = emit_html(&blocks, &HtmlOptions::default());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<article"));
        assert!(html.contains("</article>"));
    }

    #[test]
    fn headings_use_h1_h2_h3() {
        let blocks = vec![
            Block::Heading {
                level: 1,
                text: "T".into(),
            },
            Block::Heading {
                level: 2,
                text: "S".into(),
            },
        ];
        let html = emit_html(&blocks, &HtmlOptions::default());
        assert!(html.contains("<h1>T</h1>"));
        assert!(html.contains("<h2>S</h2>"));
    }

    #[test]
    fn bullet_list_uses_ul() {
        let blocks = vec![
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 0,
                text: "a".into(),
            },
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 0,
                text: "b".into(),
            },
        ];
        let html = emit_html(&blocks, &HtmlOptions::default());
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>a</li>"));
        assert!(html.contains("</ul>"));
    }

    #[test]
    fn numbered_list_uses_ol() {
        let blocks = vec![Block::ListItem {
            kind: ListKind::Number,
            indent: 0,
            text: "a".into(),
        }];
        let html = emit_html(&blocks, &HtmlOptions::default());
        assert!(html.contains("<ol>"));
    }

    #[test]
    fn table_rows_emit_table_with_thead_tbody() {
        let blocks = vec![
            Block::TableRow {
                cells: vec!["A".into(), "B".into()],
            },
            Block::TableRow {
                cells: vec!["1".into(), "2".into()],
            },
        ];
        let html = emit_html(&blocks, &HtmlOptions::default());
        assert!(html.contains("<table>"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("<th>A</th>"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn embed_css_false_skips_style_block() {
        let blocks = vec![Block::Body { text: "Hi".into() }];
        let opts = HtmlOptions {
            embed_css: false,
            ..Default::default()
        };
        let html = emit_html(&blocks, &opts);
        assert!(!html.contains("<style>"));
    }

    #[test]
    fn html_special_chars_escaped() {
        let blocks = vec![Block::Body {
            text: "a<b>c&d\"e".into(),
        }];
        let html = emit_html(&blocks, &HtmlOptions::default());
        assert!(html.contains("a&lt;b&gt;c&amp;d&quot;e"));
    }

    #[test]
    fn list_then_paragraph_closes_list() {
        let blocks = vec![
            Block::ListItem {
                kind: ListKind::Bullet,
                indent: 0,
                text: "x".into(),
            },
            Block::Body { text: "p".into() },
        ];
        let html = emit_html(&blocks, &HtmlOptions::default());
        let ul_close = html.find("</ul>").expect("ul should close");
        let p_pos = html.find("<p>p</p>").expect("p should appear");
        assert!(ul_close < p_pos, "</ul> must come before <p>");
    }
}
