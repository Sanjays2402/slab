//! OOXML SpreadsheetML emitter — `.xlsx` writer.
//!
//! Emits a fully-valid Office Open XML SpreadsheetML workbook from a slice
//! of `TableExtract`s. Cells are typed via [`cells::classify`]: numeric
//! values become `<c t="n">`, dates become Excel serial numbers with a
//! date-format style applied (`s="1"`), everything else is inline-string
//! text (`<c t="inlineStr">`).
//!
//! No external xlsx crate is used — we hand-build the OOXML parts and zip
//! them. This keeps the binary slim and avoids the typical Java-via-Rust
//! Excel libraries' weight + license restrictions.

use std::io::Write;

use super::cells::{self, Cell};
use super::types::{TableExtract, TabulateOptions};
use super::TabulateError;

/// Build a complete `.xlsx` file in memory from the given tables.
pub fn write_xlsx(
    tables: &[TableExtract],
    opts: &TabulateOptions,
) -> Result<Vec<u8>, TabulateError> {
    // We always emit at least one sheet so the workbook is valid.
    let effective: Vec<TableExtract> = if tables.is_empty() {
        vec![TableExtract {
            page: 1,
            rows: Vec::new(),
        }]
    } else {
        tables.to_vec()
    };

    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();

    // [Content_Types].xml — declares MIME types for every part.
    parts.push((
        "[Content_Types].xml".to_string(),
        content_types_xml(effective.len()).into_bytes(),
    ));

    // _rels/.rels — top-level relationships.
    parts.push(("_rels/.rels".to_string(), top_level_rels().into_bytes()));

    // xl/_rels/workbook.xml.rels — workbook → sheets, styles, sharedStrings.
    parts.push((
        "xl/_rels/workbook.xml.rels".to_string(),
        workbook_rels(effective.len()).into_bytes(),
    ));

    // xl/workbook.xml.
    parts.push((
        "xl/workbook.xml".to_string(),
        workbook_xml(&effective, opts).into_bytes(),
    ));

    // xl/styles.xml — declare one cellXfs entry for date formatting.
    parts.push(("xl/styles.xml".to_string(), styles_xml().into_bytes()));

    // xl/sharedStrings.xml — minimal (we use inline strings everywhere).
    parts.push((
        "xl/sharedStrings.xml".to_string(),
        shared_strings_xml().into_bytes(),
    ));

    // One xl/worksheets/sheet{n}.xml per table.
    for (idx, t) in effective.iter().enumerate() {
        let name = format!("xl/worksheets/sheet{}.xml", idx + 1);
        parts.push((name, sheet_xml(t, opts)?.into_bytes()));
    }

    // Pack into a deflate-compressed zip.
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let zopts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in &parts {
            zw.start_file(name, zopts)?;
            zw.write_all(body)?;
        }
        zw.finish()?;
    }
    Ok(buf)
}

fn content_types_xml(sheet_count: usize) -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    s.push_str(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#);
    s.push_str(r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#);
    s.push_str(r#"<Default Extension="xml" ContentType="application/xml"/>"#);
    s.push_str(r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#);
    for i in 1..=sheet_count {
        s.push_str(&format!(
            r#"<Override PartName="/xl/worksheets/sheet{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
        ));
    }
    s.push_str(r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#);
    s.push_str(r#"<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>"#);
    s.push_str("</Types>");
    s
}

fn top_level_rels() -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    s.push_str(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    s.push_str(r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#);
    s.push_str("</Relationships>");
    s
}

fn workbook_rels(sheet_count: usize) -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    s.push_str(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for i in 1..=sheet_count {
        s.push_str(&format!(
            r#"<Relationship Id="rId{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i}.xml"/>"#
        ));
    }
    let styles_id = sheet_count + 1;
    let strings_id = sheet_count + 2;
    s.push_str(&format!(
        r#"<Relationship Id="rId{styles_id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#
    ));
    s.push_str(&format!(
        r#"<Relationship Id="rId{strings_id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#
    ));
    s.push_str("</Relationships>");
    s
}

fn workbook_xml(tables: &[TableExtract], opts: &TabulateOptions) -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    s.push_str(r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#);
    s.push_str("<sheets>");
    for (idx, t) in tables.iter().enumerate() {
        let name = sheet_name(opts, t.page, idx);
        let escaped = escape_xml_attr(&name);
        s.push_str(&format!(
            r#"<sheet name="{escaped}" sheetId="{}" r:id="rId{}"/>"#,
            idx + 1,
            idx + 1
        ));
    }
    s.push_str("</sheets></workbook>");
    s
}

fn styles_xml() -> String {
    // Two cellXfs: index 0 = default, index 1 = date (numFmtId=14, m/d/yyyy).
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    s.push_str(r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#);
    s.push_str(r#"<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>"#);
    s.push_str(r#"<fills count="1"><fill><patternFill patternType="none"/></fill></fills>"#);
    s.push_str(r#"<borders count="1"><border/></borders>"#);
    s.push_str(r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#);
    s.push_str(r#"<cellXfs count="2">"#);
    s.push_str(r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#);
    s.push_str(r#"<xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#);
    s.push_str("</cellXfs>");
    s.push_str(r#"<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>"#);
    s.push_str("</styleSheet>");
    s
}

fn shared_strings_xml() -> String {
    String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0"/>"#,
    )
}

fn sheet_xml(t: &TableExtract, opts: &TabulateOptions) -> Result<String, TabulateError> {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    s.push_str(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#);
    for (r_idx, row) in t.rows.iter().enumerate() {
        let r1 = r_idx + 1;
        s.push_str(&format!(r#"<row r="{r1}">"#));
        for (c_idx, raw) in row.iter().enumerate() {
            let r = format!("{}{}", col_letter(c_idx), r1);
            match cells::classify(raw) {
                Cell::Blank => {}
                Cell::Number(n) if opts.type_numbers => {
                    s.push_str(&format!(
                        r#"<c r="{r}" t="n"><v>{}</v></c>"#,
                        format_f64(n)
                    ));
                }
                Cell::Date(serial) if opts.type_dates => {
                    s.push_str(&format!(
                        r#"<c r="{r}" s="1" t="n"><v>{}</v></c>"#,
                        format_f64(serial)
                    ));
                }
                Cell::Number(_) | Cell::Date(_) | Cell::Text(_) => {
                    let text = escape_xml(raw.trim());
                    s.push_str(&format!(
                        r#"<c r="{r}" t="inlineStr"><is><t xml:space="preserve">{text}</t></is></c>"#
                    ));
                }
            }
        }
        s.push_str("</row>");
    }
    s.push_str("</sheetData></worksheet>");
    Ok(s)
}

/// Convert a zero-based column index into A1 column letters.
pub fn col_letter(mut idx: usize) -> String {
    let mut out = Vec::<u8>::new();
    loop {
        let rem = idx % 26;
        out.push(b'A' + rem as u8);
        if idx < 26 {
            break;
        }
        idx = idx / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

fn format_f64(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        // Trim Rust's default float printing — Excel parses this fine.
        format!("{n}")
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_attr(s: &str) -> String {
    escape_xml(s).replace('"', "&quot;").replace('\'', "&apos;")
}

fn sheet_name(opts: &TabulateOptions, page: u32, idx: usize) -> String {
    let raw = opts.sheet_name_pattern.replace("{n}", &page.to_string());
    // Excel sheet names are bounded at 31 chars and disallow : \ / ? * [ ]
    let mut cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']'))
        .collect();
    if cleaned.is_empty() {
        cleaned = format!("Sheet{}", idx + 1);
    }
    if cleaned.chars().count() > 31 {
        cleaned = cleaned.chars().take(31).collect();
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn col_letter_basics() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(1), "B");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
        assert_eq!(col_letter(27), "AB");
        assert_eq!(col_letter(51), "AZ");
        assert_eq!(col_letter(52), "BA");
        assert_eq!(col_letter(701), "ZZ");
        assert_eq!(col_letter(702), "AAA");
    }

    #[test]
    fn format_f64_trims_integer_dot_zero() {
        assert_eq!(format_f64(42.0), "42");
        assert_eq!(format_f64(-17.0), "-17");
        assert_eq!(format_f64(1234.5), "1234.5");
    }

    #[test]
    fn escape_xml_handles_basics() {
        assert_eq!(escape_xml("a < b & c"), "a &lt; b &amp; c");
        assert_eq!(escape_xml_attr(r#"q"u'o"#), "q&quot;u&apos;o");
    }

    #[test]
    fn sheet_name_truncates_and_strips() {
        let opts = TabulateOptions {
            sheet_name_pattern: "Page/{n}".to_string(),
            ..TabulateOptions::default()
        };
        let n = sheet_name(&opts, 3, 0);
        assert!(!n.contains('/'));
        assert!(n.contains('3'));
    }

    #[test]
    fn write_xlsx_returns_a_valid_zip() {
        let tables = vec![TableExtract {
            page: 1,
            rows: vec![
                vec!["Name".into(), "Q1".into(), "Q2".into()],
                vec!["Acme".into(), "$1,234.50".into(), "12.5%".into()],
                vec!["Globex".into(), "2026-05-24".into(), "42".into()],
            ],
        }];
        let bytes = write_xlsx(&tables, &TabulateOptions::default()).unwrap();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");

        let mut zr = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/_rels/workbook.xml.rels",
            "xl/workbook.xml",
            "xl/worksheets/sheet1.xml",
            "xl/styles.xml",
            "xl/sharedStrings.xml",
        ] {
            zr.by_name(required)
                .unwrap_or_else(|_| panic!("missing {}", required));
        }
        let mut sheet = String::new();
        zr.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet)
            .unwrap();
        assert!(sheet.contains(r#"<c r="B2" t="n"><v>1234.5</v></c>"#));
        assert!(sheet.contains(r#"<c r="C2" t="n"><v>0.125</v></c>"#));
        // Date cell uses a style index that points at a date number format.
        assert!(sheet.contains(r#"<c r="B3" s="1" t="n">"#));
        // Header row is text.
        assert!(sheet.contains(r#"<c r="A1" t="inlineStr">"#));
    }

    #[test]
    fn empty_input_returns_minimal_workbook_with_one_sheet() {
        let bytes = write_xlsx(&[], &TabulateOptions::default()).unwrap();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        let mut zr = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        zr.by_name("xl/worksheets/sheet1.xml").unwrap();
    }

    #[test]
    fn type_numbers_off_yields_text_for_numbers() {
        let tables = vec![TableExtract {
            page: 1,
            rows: vec![vec!["42".into()]],
        }];
        let opts = TabulateOptions {
            type_numbers: false,
            ..TabulateOptions::default()
        };
        let bytes = write_xlsx(&tables, &opts).unwrap();
        let mut zr = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut sheet = String::new();
        zr.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet)
            .unwrap();
        assert!(sheet.contains(r#"t="inlineStr""#));
        assert!(!sheet.contains(r#"t="n""#));
    }
}
