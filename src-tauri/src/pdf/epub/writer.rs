//! XHTML5 chapter emitter — produces one `.xhtml` file per `Chapter`.
//!
//! EPUB readers are strict XML parsers, so we emit XHTML (self-closing
//! `<meta/>`, `<link/>`, `<br/>`) with an XML declaration and the
//! `http://www.w3.org/1999/xhtml` namespace. Linked stylesheet points at
//! `style.css` co-located in `OEBPS/`.

use crate::pdf::epub::package::xml_escape;
use crate::pdf::epub::split::Chapter;
use crate::pdf::reflow::types::{Block, ListKind};

pub fn chapter_xhtml(ch: &Chapter) -> String {
    let body = render_blocks(&ch.blocks);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
  <title>{title}</title>
  <meta charset="utf-8"/>
  <link rel="stylesheet" type="text/css" href="style.css"/>
</head>
<body>
{body}</body>
</html>
"#,
        title = xml_escape(&ch.title),
    )
}

fn render_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < blocks.len() {
        match &blocks[i] {
            Block::Heading { level, text } => {
                let l = (*level).clamp(1, 3);
                out.push_str(&format!("  <h{l}>{}</h{l}>\n", xml_escape(text), l = l));
                i += 1;
            }
            Block::Body { text } => {
                out.push_str(&format!("  <p>{}</p>\n", xml_escape(text)));
                i += 1;
            }
            Block::ListItem { kind, .. } => {
                let (tag, end) = match kind {
                    ListKind::Bullet => ("ul", "</ul>"),
                    ListKind::Number => ("ol", "</ol>"),
                };
                out.push_str(&format!("  <{tag}>\n"));
                let outer_kind = *kind;
                while i < blocks.len() {
                    if let Block::ListItem { kind: k2, text, .. } = &blocks[i] {
                        if *k2 == outer_kind {
                            out.push_str(&format!("    <li>{}</li>\n", xml_escape(text)));
                            i += 1;
                            continue;
                        }
                    }
                    break;
                }
                out.push_str(&format!("  {end}\n"));
            }
            Block::TableRow { .. } => {
                out.push_str("  <table>\n");
                while i < blocks.len() {
                    if let Block::TableRow { cells } = &blocks[i] {
                        out.push_str("    <tr>");
                        for c in cells {
                            out.push_str(&format!("<td>{}</td>", xml_escape(c)));
                        }
                        out.push_str("</tr>\n");
                        i += 1;
                        continue;
                    }
                    break;
                }
                out.push_str("  </table>\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::epub::split::Chapter;
    use crate::pdf::reflow::types::{Block, ListKind};

    #[test]
    fn emits_xhtml_doctype_and_xmlns() {
        let ch = Chapter {
            title: "Intro".into(),
            blocks: vec![Block::Body {
                text: "Hello world".into(),
            }],
        };
        let s = chapter_xhtml(&ch);
        assert!(s.starts_with("<?xml version=\"1.0\""));
        assert!(s.contains(r#"<html xmlns="http://www.w3.org/1999/xhtml""#));
        assert!(s.contains("<title>Intro</title>"));
        assert!(s.contains("<p>Hello world</p>"));
        assert!(s.contains(r#"<link rel="stylesheet""#));
    }

    #[test]
    fn renders_heading_list_table() {
        let ch = Chapter {
            title: "Mixed".into(),
            blocks: vec![
                Block::Heading {
                    level: 2,
                    text: "Sub".into(),
                },
                Block::ListItem {
                    kind: ListKind::Bullet,
                    text: "one".into(),
                    indent: 0,
                },
                Block::ListItem {
                    kind: ListKind::Bullet,
                    text: "two".into(),
                    indent: 0,
                },
                Block::TableRow {
                    cells: vec!["A".into(), "B".into()],
                },
                Block::TableRow {
                    cells: vec!["1".into(), "2".into()],
                },
            ],
        };
        let s = chapter_xhtml(&ch);
        assert!(s.contains("<h2>Sub</h2>"));
        assert!(s.contains("<ul>"));
        assert!(s.contains("<li>one</li>"));
        assert!(s.contains("<li>two</li>"));
        assert!(s.contains("</ul>"));
        assert!(s.contains("<table>"));
        assert!(s.contains("<td>A</td>"));
        assert!(s.contains("<td>2</td>"));
        assert!(s.contains("</table>"));
    }

    #[test]
    fn escapes_xml_specials() {
        let ch = Chapter {
            title: "X & Y".into(),
            blocks: vec![Block::Body {
                text: "a < b > c & d".into(),
            }],
        };
        let s = chapter_xhtml(&ch);
        assert!(s.contains("<title>X &amp; Y</title>"));
        assert!(s.contains("a &lt; b &gt; c &amp; d"));
    }

    #[test]
    fn ol_for_numbered_list() {
        let ch = Chapter {
            title: "N".into(),
            blocks: vec![
                Block::ListItem {
                    kind: ListKind::Number,
                    text: "first".into(),
                    indent: 0,
                },
                Block::ListItem {
                    kind: ListKind::Number,
                    text: "second".into(),
                    indent: 0,
                },
            ],
        };
        let s = chapter_xhtml(&ch);
        assert!(s.contains("<ol>"));
        assert!(s.contains("<li>first</li>"));
        assert!(s.contains("</ol>"));
    }

    #[test]
    fn heading_level_clamped_to_three() {
        let ch = Chapter {
            title: "H".into(),
            blocks: vec![Block::Heading {
                level: 9,
                text: "Deep".into(),
            }],
        };
        let s = chapter_xhtml(&ch);
        assert!(s.contains("<h3>Deep</h3>"));
    }
}
