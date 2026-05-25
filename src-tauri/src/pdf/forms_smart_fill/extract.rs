// Source-document text extraction for Quill Smart Fill.
//
// Supports PDF (via the existing `pdf::extract` pipeline), plain-text
// formats (TXT / MD / Markdown / TEXT), and single-row CSV (which we
// flatten to `key: value` lines so the AI sees one record's structure
// clearly).
//
// All extracted text is truncated to `MAX_CHARS` characters so we
// don't blow past the model's context window. 32_000 chars ≈ 8k
// tokens, which leaves headroom for the prompt + JSON response in a
// 16k-context model like `llama3.2:3b`.

use std::path::Path;

use crate::pdf::PdfError;

/// Hard ceiling on extracted text, in characters. Anything longer is
/// truncated from the end (we keep the document head).
const MAX_CHARS: usize = 32_000;

/// Extract plain text from any supported source-doc format.
///
/// Supported extensions (case-insensitive):
///   - `.pdf`               → existing PDF text-extraction pipeline
///   - `.txt`, `.text`      → raw bytes (utf-8 lossy)
///   - `.md`, `.markdown`   → raw bytes (utf-8 lossy)
///   - `.csv`               → first row, flattened as `header: value`
///     lines, one per column
///
/// Any other extension returns `PdfError::Other(...)`.
///
/// The result is guaranteed to be ≤ [`MAX_CHARS`] characters; longer
/// inputs are truncated.
pub fn extract_source_text(path: &Path) -> Result<String, PdfError> {
    if !path.exists() {
        return Err(PdfError::InputMissing(path.display().to_string()));
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let raw = match ext.as_str() {
        "pdf" => crate::pdf::extract::extract_text_concat(path)?,
        "txt" | "text" | "md" | "markdown" => {
            let bytes = std::fs::read(path).map_err(PdfError::Io)?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
        "csv" => extract_csv_as_kv(path)?,
        other => {
            return Err(PdfError::Other(format!(
                "smart-fill: unsupported source extension `.{other}`"
            )));
        }
    };
    Ok(truncate_chars(&raw, MAX_CHARS))
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn extract_csv_as_kv(path: &Path) -> Result<String, PdfError> {
    let mut rdr =
        csv::Reader::from_path(path).map_err(|e| PdfError::Other(format!("csv open: {e}")))?;
    let headers = rdr
        .headers()
        .map_err(|e| PdfError::Other(format!("csv headers: {e}")))?
        .clone();
    let mut out = String::new();
    if let Some(row) = rdr.records().next() {
        let row = row.map_err(|e| PdfError::Other(format!("csv row: {e}")))?;
        for (h, v) in headers.iter().zip(row.iter()) {
            out.push_str(h);
            out.push_str(": ");
            out.push_str(v);
            out.push('\n');
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plaintext_from_txt() {
        let f = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        std::fs::write(f.path(), "Jane Doe\njane@example.com\n+1 555 0100").unwrap();
        let s = extract_source_text(f.path()).unwrap();
        assert!(s.contains("Jane Doe"));
        assert!(s.contains("jane@example.com"));
        assert!(s.contains("+1 555 0100"));
    }

    #[test]
    fn extract_markdown_keeps_formatting() {
        let f = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
        std::fs::write(f.path(), "# Resume\n\n**Name:** Alice").unwrap();
        let s = extract_source_text(f.path()).unwrap();
        assert!(s.contains("# Resume"));
        assert!(s.contains("**Name:** Alice"));
    }

    #[test]
    fn extract_text_extension_works() {
        let f = tempfile::Builder::new().suffix(".text").tempfile().unwrap();
        std::fs::write(f.path(), "hello world").unwrap();
        assert_eq!(extract_source_text(f.path()).unwrap(), "hello world");
    }

    #[test]
    fn extract_csv_flattens_first_row() {
        let f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
        std::fs::write(
            f.path(),
            "name,email,phone\nBob Smith,bob@x.com,+1 555 0199\nCarol,carol@x.com,+1 555 0200",
        )
        .unwrap();
        let s = extract_source_text(f.path()).unwrap();
        assert!(s.contains("name: Bob Smith"));
        assert!(s.contains("email: bob@x.com"));
        assert!(s.contains("phone: +1 555 0199"));
        // Only first record is used — second row's name should not appear.
        assert!(!s.contains("Carol"));
    }

    #[test]
    fn extract_csv_empty_returns_empty_string() {
        let f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
        std::fs::write(f.path(), "name,email\n").unwrap();
        let s = extract_source_text(f.path()).unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn extract_truncates_huge_text() {
        let f = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        std::fs::write(f.path(), "x".repeat(60_000)).unwrap();
        let s = extract_source_text(f.path()).unwrap();
        assert!(s.len() <= MAX_CHARS);
        assert_eq!(s.len(), MAX_CHARS);
    }

    #[test]
    fn extract_returns_input_missing_for_nonexistent_path() {
        let nope = std::path::Path::new("/tmp/__slab_does_not_exist_xyz123.txt");
        match extract_source_text(nope) {
            Err(PdfError::InputMissing(_)) => {}
            other => panic!("expected InputMissing, got {other:?}"),
        }
    }

    #[test]
    fn extract_rejects_unknown_extension() {
        let f = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
        std::fs::write(f.path(), "binary content").unwrap();
        match extract_source_text(f.path()) {
            Err(PdfError::Other(msg)) => assert!(msg.contains("docx")),
            other => panic!("expected Other(...), got {other:?}"),
        }
    }

    #[test]
    fn extract_handles_uppercase_extensions() {
        let f = tempfile::Builder::new().suffix(".TXT").tempfile().unwrap();
        std::fs::write(f.path(), "Hello").unwrap();
        assert_eq!(extract_source_text(f.path()).unwrap(), "Hello");
    }

    #[test]
    fn truncate_chars_is_unicode_safe() {
        let s = "🍰".repeat(20_000);
        let out = truncate_chars(&s, 100);
        assert!(out.chars().count() <= 100);
        // Truncation must not split a multibyte codepoint.
        assert!(out.chars().all(|c| c == '🍰'));
    }
}
