// DOCX writer — emit a valid OOXML `.docx` from a `Vec<Block>`.
//
// A `.docx` file is a ZIP archive ("OPC package") with this minimum layout:
//
//   [Content_Types].xml           — MIME map for the parts inside
//   _rels/.rels                   — package-level relationships (points at /word/document.xml)
//   word/document.xml             — the actual content (paragraphs, tables, runs)
//   word/styles.xml               — style definitions (Normal, Heading1-3, ListBullet, ListNumber, TableNormal)
//   word/numbering.xml            — bullet + numbered list definitions
//   word/_rels/document.xml.rels  — relationships from document.xml (we ship styles + numbering)
//
// We hand-roll the XML rather than pulling a new dep — OOXML is small and
// stable enough at the subset we emit. All user-supplied text passes through
// `xml_escape` before being written.
//
// References:
//   ECMA-376 Part 1 §17 — WordprocessingML
//   https://learn.microsoft.com/openspecs/office_standards/ms-docx/

use std::io::{Cursor, Seek, Write};

use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use super::errors::ReflowError;
use super::types::{Block, ListKind};

/// Numbering ID we point bullet `ListItem`s at.
const NUM_ID_BULLET: u32 = 1;
/// Numbering ID we point numbered `ListItem`s at.
const NUM_ID_NUMBER: u32 = 2;

/// Build a full `.docx` byte blob from a sequence of `Block`s.
///
/// The returned `Vec<u8>` starts with the ZIP local-file signature (`PK\x03\x04`)
/// and opens cleanly in Word, LibreOffice, Pages, and Google Docs.
pub fn write_docx(blocks: &[Block]) -> Result<Vec<u8>, ReflowError> {
    let buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let cursor = Cursor::new(buf);
    let mut zw = ZipWriter::new(cursor);

    let opts_stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let opts_deflated =
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    write_entry(
        &mut zw,
        "[Content_Types].xml",
        &content_types(),
        opts_stored,
    )?;
    write_entry(&mut zw, "_rels/.rels", &rels_root(), opts_stored)?;
    write_entry(
        &mut zw,
        "word/_rels/document.xml.rels",
        &rels_document(),
        opts_stored,
    )?;
    write_entry(&mut zw, "word/styles.xml", &styles_xml(), opts_deflated)?;
    write_entry(
        &mut zw,
        "word/numbering.xml",
        &numbering_xml(),
        opts_deflated,
    )?;
    write_entry(
        &mut zw,
        "word/document.xml",
        &document_xml(blocks),
        opts_deflated,
    )?;

    let cursor = zw.finish().map_err(|e| ReflowError::Zip(e.to_string()))?;
    Ok(cursor.into_inner())
}

fn write_entry<W: Write + Seek>(
    zw: &mut ZipWriter<W>,
    name: &str,
    body: &str,
    opts: SimpleFileOptions,
) -> Result<(), ReflowError> {
    zw.start_file(name, opts)
        .map_err(|e| ReflowError::Zip(e.to_string()))?;
    zw.write_all(body.as_bytes())?;
    Ok(())
}

fn content_types() -> String {
    // The `Default` extensions cover most static parts; we add overrides for
    // the WordprocessingML parts which need very specific content-types.
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>
"#
    .to_string()
}

fn rels_root() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>
"#
    .to_string()
}

fn rels_document() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
</Relationships>
"#
    .to_string()
}

fn styles_xml() -> String {
    // 6 styles, dead-simple: Normal (default body), Heading1-3, ListBullet,
    // ListNumber, TableNormal. Word reads them by `w:styleId`.
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault>
      <w:rPr>
        <w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/>
        <w:sz w:val="22"/>
      </w:rPr>
    </w:rPrDefault>
    <w:pPrDefault>
      <w:pPr>
        <w:spacing w:after="160" w:line="276" w:lineRule="auto"/>
      </w:pPr>
    </w:pPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
    <w:name w:val="Normal"/>
    <w:qFormat/>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:basedOn w:val="Normal"/>
    <w:next w:val="Normal"/>
    <w:qFormat/>
    <w:pPr><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="0"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="36"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/>
    <w:basedOn w:val="Normal"/>
    <w:next w:val="Normal"/>
    <w:qFormat/>
    <w:pPr><w:spacing w:before="200" w:after="60"/><w:outlineLvl w:val="1"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="30"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading3">
    <w:name w:val="heading 3"/>
    <w:basedOn w:val="Normal"/>
    <w:next w:val="Normal"/>
    <w:qFormat/>
    <w:pPr><w:spacing w:before="160" w:after="40"/><w:outlineLvl w:val="2"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="26"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="ListBullet">
    <w:name w:val="List Bullet"/>
    <w:basedOn w:val="Normal"/>
    <w:qFormat/>
    <w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="ListNumber">
    <w:name w:val="List Number"/>
    <w:basedOn w:val="Normal"/>
    <w:qFormat/>
    <w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="table" w:styleId="TableNormal">
    <w:name w:val="Normal Table"/>
    <w:tblPr>
      <w:tblBorders>
        <w:top w:val="single" w:sz="4" w:color="auto"/>
        <w:left w:val="single" w:sz="4" w:color="auto"/>
        <w:bottom w:val="single" w:sz="4" w:color="auto"/>
        <w:right w:val="single" w:sz="4" w:color="auto"/>
        <w:insideH w:val="single" w:sz="4" w:color="auto"/>
        <w:insideV w:val="single" w:sz="4" w:color="auto"/>
      </w:tblBorders>
    </w:tblPr>
  </w:style>
</w:styles>
"#
    .to_string()
}

fn numbering_xml() -> String {
    // Two abstract numberings (bullet + decimal), each pointed at by one numId.
    // Word requires the indirection abstractNumId -> numId for list paragraphs.
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="bullet"/>
      <w:lvlText w:val="•"/>
      <w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
    </w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>
"#
    .to_string()
}

fn document_xml(blocks: &[Block]) -> String {
    let mut s = String::with_capacity(2048 + blocks.len() * 64);
    s.push_str(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
"#,
    );

    // Coalesce consecutive `TableRow`s into a single `<w:tbl>` so Word renders
    // them as one table (not N adjacent single-row tables).
    let mut i = 0;
    while i < blocks.len() {
        match &blocks[i] {
            Block::TableRow { .. } => {
                let start = i;
                while i < blocks.len() && matches!(blocks[i], Block::TableRow { .. }) {
                    i += 1;
                }
                emit_table(&blocks[start..i], &mut s);
            }
            Block::Body { text } => {
                emit_paragraph(&mut s, None, None, text);
                i += 1;
            }
            Block::Heading { level, text } => {
                let style = match *level {
                    1 => "Heading1",
                    2 => "Heading2",
                    _ => "Heading3",
                };
                emit_paragraph(&mut s, Some(style), None, text);
                i += 1;
            }
            Block::ListItem { kind, text, .. } => {
                let (style, num_id) = match kind {
                    ListKind::Bullet => ("ListBullet", NUM_ID_BULLET),
                    ListKind::Number => ("ListNumber", NUM_ID_NUMBER),
                };
                emit_paragraph(&mut s, Some(style), Some(num_id), text);
                i += 1;
            }
        }
    }

    // A `sectPr` at the bottom of the body is required for Word not to repair.
    s.push_str(r#"<w:sectPr>
  <w:pgSz w:w="12240" w:h="15840"/>
  <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>
</w:sectPr>
</w:body></w:document>
"#);
    s
}

fn emit_paragraph(out: &mut String, style: Option<&str>, num_id: Option<u32>, text: &str) {
    out.push_str("<w:p>");
    if style.is_some() || num_id.is_some() {
        out.push_str("<w:pPr>");
        if let Some(st) = style {
            out.push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, st));
        }
        if let Some(n) = num_id {
            out.push_str(&format!(
                r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="{}"/></w:numPr>"#,
                n
            ));
        }
        out.push_str("</w:pPr>");
    }
    out.push_str("<w:r><w:t xml:space=\"preserve\">");
    out.push_str(&xml_escape(text));
    out.push_str("</w:t></w:r></w:p>\n");
}

fn emit_table(rows: &[Block], out: &mut String) {
    out.push_str(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableNormal"/><w:tblW w:w="0" w:type="auto"/></w:tblPr>"#,
    );
    // Compute column count: max cells across rows. Pad short rows with empty cells.
    let mut col_count = 0usize;
    for r in rows {
        if let Block::TableRow { cells } = r {
            col_count = col_count.max(cells.len());
        }
    }
    if col_count == 0 {
        out.push_str("</w:tbl>\n");
        return;
    }
    // tblGrid — equal-width columns.
    out.push_str("<w:tblGrid>");
    for _ in 0..col_count {
        out.push_str(r#"<w:gridCol w:w="2000"/>"#);
    }
    out.push_str("</w:tblGrid>");
    for r in rows {
        if let Block::TableRow { cells } = r {
            out.push_str("<w:tr>");
            for c in 0..col_count {
                let text = cells.get(c).map(|s| s.as_str()).unwrap_or("");
                out.push_str(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr>"#);
                out.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
                out.push_str(&xml_escape(text));
                out.push_str("</w:t></w:r></w:p></w:tc>");
            }
            out.push_str("</w:tr>");
        }
    }
    out.push_str("</w:tbl>\n");
}

/// XML 1.0 escape for character data. Replaces the five predefined entities
/// and strips C0 control bytes that XML 1.0 does not allow (other than
/// tab/LF/CR).
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(ch),
            c if (c as u32) < 0x20 => { /* drop disallowed C0 controls */ }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn unzip_entry(bytes: &[u8], name: &str) -> Option<String> {
        let mut zr = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).ok()?;
        let mut f = zr.by_name(name).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        Some(s)
    }

    #[test]
    fn writes_pk_signature_and_valid_zip() {
        let blocks = vec![Block::Body {
            text: "hello".into(),
        }];
        let bytes = write_docx(&blocks).unwrap();
        assert_eq!(&bytes[0..4], b"PK\x03\x04", "missing ZIP local-file sig");
        // Open as ZipArchive — fails if structure is malformed.
        zip::ZipArchive::new(Cursor::new(bytes)).expect("docx is not a valid zip");
    }

    #[test]
    fn document_contains_two_body_paragraphs() {
        let blocks = vec![
            Block::Body {
                text: "First paragraph.".into(),
            },
            Block::Body {
                text: "Second paragraph.".into(),
            },
        ];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        let p_count = doc.matches("<w:p>").count();
        assert_eq!(p_count, 2, "document.xml = {}", doc);
        assert!(doc.contains("First paragraph."));
        assert!(doc.contains("Second paragraph."));
    }

    #[test]
    fn heading1_emits_pstyle_reference() {
        let blocks = vec![Block::Heading {
            level: 1,
            text: "Chapter 1".into(),
        }];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        assert!(
            doc.contains(r#"<w:pStyle w:val="Heading1"/>"#),
            "missing Heading1 pStyle in {}",
            doc
        );
    }

    #[test]
    fn heading_levels_2_and_3_emit_correct_pstyle() {
        let blocks = vec![
            Block::Heading {
                level: 2,
                text: "Sec".into(),
            },
            Block::Heading {
                level: 3,
                text: "Sub".into(),
            },
        ];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        assert!(doc.contains(r#"<w:pStyle w:val="Heading2"/>"#));
        assert!(doc.contains(r#"<w:pStyle w:val="Heading3"/>"#));
    }

    #[test]
    fn bullet_list_item_emits_numpr_pointing_at_num_id_1() {
        let blocks = vec![Block::ListItem {
            kind: ListKind::Bullet,
            text: "First bullet".into(),
            indent: 0,
        }];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        assert!(doc.contains(r#"<w:numId w:val="1"/>"#), "doc = {}", doc);
        assert!(doc.contains("First bullet"));
    }

    #[test]
    fn numbered_list_item_emits_numpr_pointing_at_num_id_2() {
        let blocks = vec![Block::ListItem {
            kind: ListKind::Number,
            text: "Item one".into(),
            indent: 0,
        }];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        assert!(doc.contains(r#"<w:numId w:val="2"/>"#));
    }

    #[test]
    fn consecutive_table_rows_collapse_into_single_tbl() {
        let blocks = vec![
            Block::TableRow {
                cells: vec!["a".into(), "b".into()],
            },
            Block::TableRow {
                cells: vec!["c".into(), "d".into()],
            },
            Block::TableRow {
                cells: vec!["e".into(), "f".into()],
            },
        ];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        let tbl_count = doc.matches("<w:tbl>").count();
        let tr_count = doc.matches("<w:tr>").count();
        assert_eq!(tbl_count, 1, "expected 1 <w:tbl>, doc = {}", doc);
        assert_eq!(tr_count, 3, "expected 3 <w:tr>");
        assert!(doc.contains("a") && doc.contains("f"));
    }

    #[test]
    fn xml_escape_handles_special_chars_in_body_text() {
        let blocks = vec![Block::Body {
            text: "5 < 10 & \"safe\" > 'ok'".into(),
        }];
        let bytes = write_docx(&blocks).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        assert!(doc.contains("&lt;"));
        assert!(doc.contains("&amp;"));
        assert!(doc.contains("&quot;"));
        assert!(doc.contains("&gt;"));
        assert!(doc.contains("&apos;"));
        // Make sure the resulting XML is at least well-formed enough to re-parse.
        // We rely on zip + the structural test above; here we just sanity-check
        // that no raw '<' from the user's text leaked through into the body.
        assert!(!doc.contains("5 < 10"));
    }

    #[test]
    fn package_contains_all_required_parts() {
        let bytes = write_docx(&[Block::Body { text: "ok".into() }]).unwrap();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/_rels/document.xml.rels",
            "word/document.xml",
            "word/styles.xml",
            "word/numbering.xml",
        ] {
            assert!(
                unzip_entry(&bytes, required).is_some(),
                "missing required part: {}",
                required
            );
        }
    }

    #[test]
    fn document_includes_sectpr_at_end_of_body() {
        let bytes = write_docx(&[Block::Body { text: "x".into() }]).unwrap();
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        assert!(doc.contains("<w:sectPr>"));
        // sectPr must be the last child of body, immediately before </w:body>.
        let sect_idx = doc.find("<w:sectPr>").unwrap();
        let body_close = doc.find("</w:body>").unwrap();
        assert!(sect_idx < body_close);
    }

    #[test]
    fn empty_blocks_still_produces_valid_docx() {
        let bytes = write_docx(&[]).unwrap();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        let doc = unzip_entry(&bytes, "word/document.xml").unwrap();
        // No <w:p> elements but body + sectPr present.
        assert!(doc.contains("<w:body>"));
        assert!(doc.contains("<w:sectPr>"));
    }

    #[test]
    fn xml_escape_drops_disallowed_control_chars() {
        let s = xml_escape("hi\x01there\x7fend");
        assert!(s.contains("hi"));
        assert!(s.contains("there"));
        // \x01 stripped; \x7f is DEL (>= 0x20) so it's kept.
        assert!(!s.contains('\x01'));
    }
}
