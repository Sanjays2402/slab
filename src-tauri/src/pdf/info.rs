// Read metadata from a PDF: page count, version, title/author/etc.

use crate::pdf::PdfError;
use lopdf::Document;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, Clone)]
pub struct PdfInfo {
    pub page_count: u32,
    pub version: String,
    pub size_bytes: u64,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub encrypted: bool,
}

pub fn info(input: &Path) -> Result<PdfInfo, PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let size_bytes = std::fs::metadata(input)?.len();
    let doc = Document::load(input)?;
    let encrypted = doc.is_encrypted();
    let page_count = doc.get_pages().len() as u32;
    let version = format!("{:.1}", doc.version.parse::<f32>().unwrap_or(1.4));

    let mut info = PdfInfo {
        page_count,
        version,
        size_bytes,
        title: None,
        author: None,
        subject: None,
        creator: None,
        producer: None,
        encrypted,
    };

    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_ref.as_reference() {
            if let Ok(info_obj) = doc.get_object(info_ref) {
                if let Ok(dict) = info_obj.as_dict() {
                    info.title = read_str(dict, b"Title");
                    info.author = read_str(dict, b"Author");
                    info.subject = read_str(dict, b"Subject");
                    info.creator = read_str(dict, b"Creator");
                    info.producer = read_str(dict, b"Producer");
                }
            }
        }
    }
    Ok(info)
}

fn read_str(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key)
        .ok()
        .and_then(|o| match o {
            lopdf::Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn info_reads_page_count_and_version() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.pdf");
        make_n_page_pdf(&p, 4);
        let i = info(&p).unwrap();
        assert_eq!(i.page_count, 4);
        assert_eq!(i.version, "1.5");
        assert!(i.size_bytes > 100);
        assert!(!i.encrypted);
    }
}
