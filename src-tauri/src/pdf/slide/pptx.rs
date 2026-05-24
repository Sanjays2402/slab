//! Minimal OOXML PresentationML (.pptx) writer.
//!
//! Layout:
//!   [Content_Types].xml
//!   _rels/.rels
//!   ppt/presentation.xml + .rels
//!   ppt/theme/theme1.xml
//!   ppt/slideMasters/slideMaster1.xml + .rels
//!   ppt/slideLayouts/slideLayout1.xml + .rels
//!   ppt/slides/slide<N>.xml + .rels
//!   ppt/notesMasters/notesMaster1.xml + .rels   (only if any notes)
//!   ppt/notesSlides/notesSlide<N>.xml + .rels   (only for slides with notes)
//!
//! Opens cleanly in PowerPoint, Keynote, LibreOffice Impress.

use crate::pdf::slide::errors::SlideError;
use crate::pdf::slide::types::SlideContent;
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

/// Build a `.pptx` byte blob from a list of slides.
pub(crate) fn write_pptx(slides: &[SlideContent]) -> Result<Vec<u8>, SlideError> {
    let buf = Cursor::new(Vec::<u8>::new());
    let mut zip = ZipWriter::new(buf);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // Slide size in EMU (1 pt = 12700 EMU). Use first slide's dimensions.
    let (w_pt, h_pt) = slides
        .first()
        .map(|s| (s.width_pt, s.height_pt))
        .unwrap_or((720.0, 540.0));
    let cx = (w_pt * 12700.0).round() as i64;
    let cy = (h_pt * 12700.0).round() as i64;

    // Which slides have notes?
    let note_indices: Vec<usize> = slides
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            if s.notes.as_ref().is_some_and(|n| !n.trim().is_empty()) {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    let has_notes = !note_indices.is_empty();

    write_entry(
        &mut zip,
        "[Content_Types].xml",
        &content_types_xml(slides.len(), &note_indices, has_notes),
        opts,
    )?;
    write_entry(&mut zip, "_rels/.rels", ROOT_RELS, opts)?;
    write_entry(
        &mut zip,
        "ppt/presentation.xml",
        &presentation_xml(slides.len(), cx, cy, has_notes),
        opts,
    )?;
    write_entry(
        &mut zip,
        "ppt/_rels/presentation.xml.rels",
        &presentation_rels_xml(slides.len(), has_notes),
        opts,
    )?;
    write_entry(&mut zip, "ppt/theme/theme1.xml", THEME_XML, opts)?;
    write_entry(
        &mut zip,
        "ppt/slideMasters/slideMaster1.xml",
        SLIDE_MASTER_XML,
        opts,
    )?;
    write_entry(
        &mut zip,
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        SLIDE_MASTER_RELS,
        opts,
    )?;
    write_entry(
        &mut zip,
        "ppt/slideLayouts/slideLayout1.xml",
        SLIDE_LAYOUT_XML,
        opts,
    )?;
    write_entry(
        &mut zip,
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        SLIDE_LAYOUT_RELS,
        opts,
    )?;

    for (i, slide) in slides.iter().enumerate() {
        let n = i + 1;
        let has_note = slide.notes.as_ref().is_some_and(|n| !n.trim().is_empty());
        write_entry(
            &mut zip,
            &format!("ppt/slides/slide{n}.xml"),
            &slide_xml(slide),
            opts,
        )?;
        write_entry(
            &mut zip,
            &format!("ppt/slides/_rels/slide{n}.xml.rels"),
            &slide_rels_xml(n, has_note),
            opts,
        )?;
    }

    if has_notes {
        write_entry(
            &mut zip,
            "ppt/notesMasters/notesMaster1.xml",
            NOTES_MASTER_XML,
            opts,
        )?;
        write_entry(
            &mut zip,
            "ppt/notesMasters/_rels/notesMaster1.xml.rels",
            NOTES_MASTER_RELS,
            opts,
        )?;
        for &i in &note_indices {
            let n = i + 1;
            let notes_body = slides[i].notes.as_deref().unwrap_or("");
            write_entry(
                &mut zip,
                &format!("ppt/notesSlides/notesSlide{n}.xml"),
                &notes_slide_xml(n, notes_body),
                opts,
            )?;
            write_entry(
                &mut zip,
                &format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                &notes_slide_rels_xml(n),
                opts,
            )?;
        }
    }

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

fn write_entry<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    body: &str,
    opts: SimpleFileOptions,
) -> Result<(), SlideError> {
    zip.start_file(name, opts)?;
    zip.write_all(body.as_bytes())?;
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn content_types_xml(n_slides: usize, note_indices: &[usize], has_notes: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
"#,
    );
    for i in 1..=n_slides {
        out.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>\n"
        ));
    }
    if has_notes {
        out.push_str("<Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\"/>\n");
        for &i in note_indices {
            let n = i + 1;
            out.push_str(&format!(
                "<Override PartName=\"/ppt/notesSlides/notesSlide{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>\n"
            ));
        }
    }
    out.push_str("</Types>");
    out
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#;

fn presentation_xml(n: usize, cx: i64, cy: i64, has_notes: bool) -> String {
    let mut ids = String::new();
    // rId1 = slideMaster, rId2..rId(n+1) = slides, rId(n+2) = notesMaster
    for i in 0..n {
        let rid = i + 2;
        let sid = 256 + i;
        ids.push_str(&format!("<p:sldId id=\"{sid}\" r:id=\"rId{rid}\"/>"));
    }
    let notes_master_lst = if has_notes {
        let rid = n + 2;
        format!("<p:notesMasterIdLst><p:notesMasterId r:id=\"rId{rid}\"/></p:notesMasterIdLst>")
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" saveSubsetFonts="1">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
{notes_master_lst}<p:sldIdLst>{ids}</p:sldIdLst>
<p:sldSz cx="{cx}" cy="{cy}"/>
<p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
    )
}

fn presentation_rels_xml(n: usize, has_notes: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
"#,
    );
    for i in 1..=n {
        let rid = i + 1;
        out.push_str(&format!("<Relationship Id=\"rId{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{i}.xml\"/>\n"));
    }
    if has_notes {
        let rid = n + 2;
        out.push_str(&format!("<Relationship Id=\"rId{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster\" Target=\"notesMasters/notesMaster1.xml\"/>\n"));
    }
    out.push_str("</Relationships>");
    out
}

fn slide_xml(slide: &SlideContent) -> String {
    let title_para = match &slide.title {
        Some(t) => format!(
            r#"<a:p><a:r><a:rPr lang="en-US" sz="3200" b="1"/><a:t>{}</a:t></a:r></a:p>"#,
            escape(t)
        ),
        None => String::from(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#),
    };
    let mut body_paras = String::new();
    for b in &slide.body_bullets {
        body_paras.push_str(&format!(
            r#"<a:p><a:pPr lvl="0"><a:buChar char="&#8226;"/></a:pPr><a:r><a:rPr lang="en-US" sz="1800"/><a:t>{}</a:t></a:r></a:p>"#,
            escape(b)
        ));
    }
    if body_paras.is_empty() {
        body_paras.push_str(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#);
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/>{title_para}</p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Content 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/>{body_paras}</p:txBody>
</p:sp>
</p:spTree></p:cSld>
</p:sld>"#
    )
}

fn slide_rels_xml(n: usize, has_note: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
"#,
    );
    if has_note {
        out.push_str(&format!("<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide{n}.xml\"/>\n"));
    }
    out.push_str("</Relationships>");
    out
}

fn notes_slide_xml(_n: usize, body: &str) -> String {
    let mut paras = String::new();
    for line in body.lines() {
        paras.push_str(&format!(
            r#"<a:p><a:r><a:rPr lang="en-US" sz="1200"/><a:t>{}</a:t></a:r></a:p>"#,
            escape(line)
        ));
    }
    if paras.is_empty() {
        paras.push_str(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#);
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody>
</p:sp>
</p:spTree></p:cSld>
</p:notes>"#
    )
}

fn notes_slide_rels_xml(n: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide{n}.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="../notesMasters/notesMaster1.xml"/>
</Relationships>"#
    )
}

// ---------- Static boilerplate parts ----------

const THEME_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme">
<a:themeElements>
<a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme>
<a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>
</a:themeElements>
<a:objectDefaults/>
<a:extraClrSchemeLst/>
</a:theme>"#;

const SLIDE_MASTER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>
<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
<p:txStyles>
<p:titleStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="4400" b="0"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mj-lt"/></a:defRPr></a:lvl1pPr></p:titleStyle>
<p:bodyStyle><a:lvl1pPr marL="342900" indent="-342900"><a:buChar char="&#8226;"/><a:defRPr sz="2800"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/></a:defRPr></a:lvl1pPr></p:bodyStyle>
<p:otherStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr></p:otherStyle>
</p:txStyles>
</p:sldMaster>"#;

const SLIDE_MASTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#;

const SLIDE_LAYOUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="title" preserve="1">
<p:cSld name="Title and Content"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
</p:sldLayout>"#;

const SLIDE_LAYOUT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#;

const NOTES_MASTER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notesMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>
<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:notesStyle><a:lvl1pPr><a:defRPr sz="1200"/></a:lvl1pPr></p:notesStyle>
</p:notesMaster>"#;

const NOTES_MASTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn sample(n: usize) -> Vec<SlideContent> {
        (0..n)
            .map(|i| SlideContent {
                title: Some(format!("Slide {}", i + 1)),
                body_bullets: vec!["alpha".into(), "beta".into()],
                notes: if i == 0 {
                    Some("note line".into())
                } else {
                    None
                },
                width_pt: 612.0,
                height_pt: 792.0,
            })
            .collect()
    }

    #[test]
    fn writes_minimal_pptx_zip() {
        let bytes = write_pptx(&sample(2)).unwrap();
        let reader = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "ppt/presentation.xml",
            "ppt/_rels/presentation.xml.rels",
            "ppt/theme/theme1.xml",
            "ppt/slideMasters/slideMaster1.xml",
            "ppt/slideLayouts/slideLayout1.xml",
            "ppt/slides/slide1.xml",
            "ppt/slides/slide2.xml",
        ] {
            assert!(
                names.iter().any(|n| n == required),
                "missing {required} in zip; have: {names:?}"
            );
        }
    }

    #[test]
    fn slide_xml_contains_title_and_bullets() {
        let bytes = write_pptx(&sample(1)).unwrap();
        let reader = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let mut s = String::new();
        zip.by_name("ppt/slides/slide1.xml")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert!(s.contains("Slide 1"));
        assert!(s.contains("alpha"));
        assert!(s.contains("beta"));
    }

    #[test]
    fn xml_special_chars_are_escaped() {
        let slides = vec![SlideContent {
            title: Some("A & B < C".into()),
            body_bullets: vec!["x > y".into()],
            notes: None,
            width_pt: 612.0,
            height_pt: 792.0,
        }];
        let bytes = write_pptx(&slides).unwrap();
        let reader = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let mut s = String::new();
        zip.by_name("ppt/slides/slide1.xml")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert!(s.contains("A &amp; B &lt; C"));
        assert!(s.contains("x &gt; y"));
    }

    #[test]
    fn notes_emit_extra_parts() {
        let bytes = write_pptx(&sample(2)).unwrap();
        let reader = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names
            .iter()
            .any(|n| n == "ppt/notesMasters/notesMaster1.xml"));
        assert!(names.iter().any(|n| n == "ppt/notesSlides/notesSlide1.xml"));
        // slide 2 has no notes
        assert!(!names.iter().any(|n| n == "ppt/notesSlides/notesSlide2.xml"));
    }

    #[test]
    fn no_notes_means_no_notes_master() {
        let slides = vec![SlideContent {
            title: Some("T".into()),
            body_bullets: vec!["b".into()],
            notes: None,
            width_pt: 720.0,
            height_pt: 540.0,
        }];
        let bytes = write_pptx(&slides).unwrap();
        let reader = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(!names.iter().any(|n| n.starts_with("ppt/notesMasters/")));
        assert!(!names.iter().any(|n| n.starts_with("ppt/notesSlides/")));
    }

    #[test]
    fn slide_size_reflects_first_slide() {
        let slides = vec![SlideContent {
            title: None,
            body_bullets: vec![],
            notes: None,
            width_pt: 720.0,
            height_pt: 540.0,
        }];
        let bytes = write_pptx(&slides).unwrap();
        let reader = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        let mut s = String::new();
        zip.by_name("ppt/presentation.xml")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        let expected_cx = (720.0_f32 * 12700.0).round() as i64;
        assert!(s.contains(&format!("cx=\"{expected_cx}\"")));
    }
}
