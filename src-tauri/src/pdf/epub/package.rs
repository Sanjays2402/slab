//! EPUB 3 package "envelope" XML generation.
//!
//! Generates the three structural files every EPUB needs:
//! - `META-INF/container.xml` — points the reader at content.opf.
//! - `OEBPS/content.opf` — manifest + spine + metadata.
//! - `OEBPS/nav.xhtml` — XHTML5 navigation document (a.k.a. ToC).
//!
//! Plus the default stylesheet bundled at `OEBPS/style.css`.

pub struct OpfMetadata {
    pub title: String,
    pub author: String,
    pub language: String,
    pub uuid: String,
}

/// `META-INF/container.xml` — identical for every EPUB.
pub fn container_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#
    .to_string()
}

/// `OEBPS/content.opf` — manifest + spine + Dublin Core metadata.
pub fn content_opf(meta: &OpfMetadata, chapter_ids: &[String]) -> String {
    let modified = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let mut manifest = String::new();
    let mut spine = String::new();
    for id in chapter_ids {
        let esc_id = xml_escape(id);
        manifest.push_str(&format!(
            "    <item id=\"{id}\" href=\"{id}.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
            id = esc_id
        ));
        spine.push_str(&format!("    <itemref idref=\"{}\"/>\n", esc_id));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid" xml:lang="{lang}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:uuid:{uuid}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:language>{lang}</dc:language>
    <meta property="dcterms:modified">{modified}</meta>
    <meta name="generator" content="Slab (offline, github.com/Sanjays2402/slab)"/>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="style" href="style.css" media-type="text/css"/>
{manifest}  </manifest>
  <spine>
{spine}  </spine>
</package>
"#,
        lang = xml_escape(&meta.language),
        uuid = xml_escape(&meta.uuid),
        title = xml_escape(&meta.title),
        author = xml_escape(&meta.author),
    )
}

/// `OEBPS/nav.xhtml` — XHTML5 with `<nav epub:type="toc">`.
pub fn nav_xhtml(chapters: &[(String, String)]) -> String {
    let mut items = String::new();
    for (id, title) in chapters {
        items.push_str(&format!(
            "      <li><a href=\"{id}.xhtml\">{title}</a></li>\n",
            id = xml_escape(id),
            title = xml_escape(title),
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en">
<head><title>Table of Contents</title><meta charset="utf-8"/></head>
<body>
  <nav epub:type="toc" id="toc">
    <h1>Table of Contents</h1>
    <ol>
{items}    </ol>
  </nav>
</body>
</html>
"#
    )
}

/// `OEBPS/style.css` — reflow-friendly defaults; readers may override.
pub fn default_stylesheet() -> &'static str {
    r#"body { font-family: serif; line-height: 1.55; margin: 0 1em; }
h1, h2, h3 { font-family: sans-serif; line-height: 1.2; }
h1 { font-size: 1.6em; margin: 1.2em 0 0.4em; }
h2 { font-size: 1.3em; margin: 1em 0 0.4em; }
h3 { font-size: 1.1em; margin: 0.8em 0 0.3em; }
p  { margin: 0.4em 0; text-indent: 0; }
ul, ol { margin: 0.4em 0 0.4em 1.2em; padding: 0; }
li { margin: 0.15em 0; }
table { border-collapse: collapse; margin: 0.6em 0; }
td, th { border: 1px solid #888; padding: 4px 8px; }
"#
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_xml_points_to_content_opf() {
        let s = container_xml();
        assert!(s.contains(r#"full-path="OEBPS/content.opf""#));
        assert!(s.contains("media-type=\"application/oebps-package+xml\""));
    }

    #[test]
    fn opf_contains_required_metadata() {
        let meta = OpfMetadata {
            title: "My Book".into(),
            author: "Jane Doe".into(),
            language: "en".into(),
            uuid: "abc-123".into(),
        };
        let chapter_ids = vec!["chapter-1".to_string(), "chapter-2".to_string()];
        let s = content_opf(&meta, &chapter_ids);
        assert!(s.contains("<dc:title>My Book</dc:title>"));
        assert!(s.contains("<dc:creator>Jane Doe</dc:creator>"));
        assert!(s.contains("<dc:language>en</dc:language>"));
        assert!(s.contains("urn:uuid:abc-123"));
        assert!(s.contains(r#"id="chapter-1""#));
        assert!(s.contains(r#"id="chapter-2""#));
        assert!(s.contains(r#"id="nav""#));
        assert!(s.contains(r#"properties="nav""#));
        assert!(s.contains(r#"<itemref idref="chapter-1"/>"#));
        assert!(s.contains(r#"<itemref idref="chapter-2"/>"#));
        assert!(s.contains("dcterms:modified"));
    }

    #[test]
    fn nav_xhtml_has_toc_links() {
        let chapters = vec![
            ("chapter-1".to_string(), "Introduction".to_string()),
            ("chapter-2".to_string(), "Methods".to_string()),
        ];
        let s = nav_xhtml(&chapters);
        assert!(s.contains(r#"<nav epub:type="toc""#));
        assert!(s.contains(r#"<a href="chapter-1.xhtml">Introduction</a>"#));
        assert!(s.contains(r#"<a href="chapter-2.xhtml">Methods</a>"#));
    }

    #[test]
    fn xml_escape_handles_specials() {
        assert_eq!(
            xml_escape("a & b < c > d \" e"),
            "a &amp; b &lt; c &gt; d &quot; e"
        );
    }

    #[test]
    fn opf_escapes_title_specials() {
        let meta = OpfMetadata {
            title: "Cats & Dogs <vol 1>".into(),
            author: "A \"Name\"".into(),
            language: "en".into(),
            uuid: "u".into(),
        };
        let s = content_opf(&meta, &[]);
        assert!(s.contains("Cats &amp; Dogs &lt;vol 1&gt;"));
        assert!(s.contains("A &quot;Name&quot;"));
    }

    #[test]
    fn default_stylesheet_non_empty() {
        let css = default_stylesheet();
        assert!(css.contains("body"));
        assert!(css.contains("font-family"));
    }
}
