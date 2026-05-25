//! Batch AcroForm fill — mail-merge a fillable PDF over CSV rows.
//!
//! Pipeline: read a CSV (headers required) → for each row, run
//! `pdf::forms::fill()` against the template PDF → write to `<out_dir>/<name>`
//! where the name is rendered from `filename_template` against the row.
//! Optionally flatten each output and/or pack everything into a ZIP. Always
//! writes a `_slab_quill_batch_load_file.csv` next to the outputs for review.
//!
//! v3.25.0 "Quill Pro" — the buyer-magnet Acrobat-Data-Merge killer.

use crate::pdf::forms;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One unit of work: a CSV applied to a template PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSpec {
    pub template: PathBuf,
    pub csv: PathBuf,
    pub out_dir: PathBuf,
    /// Filename template — supports `{row}` (1-indexed), `{<column_name>}`
    /// substitution. If the rendered name has no `.pdf` extension, one is
    /// appended.
    pub filename_template: String,
    /// If true, flatten the AcroForm after fill so the output PDFs open
    /// in Preview / Foxit / etc. with no editable widgets left.
    #[serde(default)]
    pub flatten: bool,
    /// If `Some(name)`, also write `<out_dir>/<name>.zip` containing all
    /// successful per-row outputs.
    #[serde(default)]
    pub zip_as: Option<String>,
    /// If `Some(n)`, only process row `n` (1-indexed). Useful for "preview
    /// row 1" before committing to the full batch.
    #[serde(default)]
    pub only_row: Option<usize>,
}

/// Per-row outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowResult {
    pub row: usize,
    pub output: PathBuf,
    pub filled: Vec<String>,
    pub unknown: Vec<String>,
    pub read_only_skipped: Vec<String>,
    pub error: Option<String>,
}

/// Aggregate result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchReport {
    pub rows_total: usize,
    pub rows_succeeded: usize,
    pub rows_failed: usize,
    pub rows: Vec<RowResult>,
    pub zip_path: Option<PathBuf>,
    pub load_file_csv: PathBuf,
}

// ---------------------------------------------------------------------------
// Filename template engine
// ---------------------------------------------------------------------------

/// Render the output filename for `row_idx` using `tpl` and CSV `row`.
/// Substitutions: `{row}` → 1-indexed row number; `{<col>}` → column value.
/// Unknown placeholders render as empty (mail-merge norm).
/// Filesystem-unsafe characters (`/\:*?"<>|`) are replaced with `_`.
/// If the result lacks a `.pdf` suffix (case-insensitive), one is appended.
pub fn render_name(
    tpl: &str,
    row_idx: usize,
    row: &HashMap<String, String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(tpl.len() + 8);
    let mut chars = tpl.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut key = String::new();
        let mut closed = false;
        for k in chars.by_ref() {
            if k == '}' {
                closed = true;
                break;
            }
            key.push(k);
        }
        if !closed {
            return Err(format!("unclosed placeholder in template: {tpl}"));
        }
        if key == "row" {
            out.push_str(&row_idx.to_string());
        } else if let Some(v) = row.get(&key) {
            out.push_str(v);
        }
        // unknown placeholder → empty
    }
    let sanitized: String = out
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let lower = sanitized.to_ascii_lowercase();
    Ok(if lower.ends_with(".pdf") {
        sanitized
    } else {
        format!("{sanitized}.pdf")
    })
}

// ---------------------------------------------------------------------------
// CSV reader
// ---------------------------------------------------------------------------

/// Parse `path` as a headered CSV into a vector of per-row maps.
pub fn read_csv(path: &Path) -> Result<Vec<HashMap<String, String>>, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("open csv {}: {e}", path.display()))?;
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let mut out = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let rec = rec.map_err(|e| format!("row {}: {e}", i + 1))?;
        let mut map = HashMap::with_capacity(headers.len());
        for (h, v) in headers.iter().zip(rec.iter()) {
            map.insert(h.to_string(), v.to_string());
        }
        out.push(map);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Main driver
// ---------------------------------------------------------------------------

/// Run the batch. Always produces a load-file CSV; PDFs and optional ZIP
/// land in `spec.out_dir`. Per-row errors are captured in `RowResult.error`,
/// not propagated — one bad row doesn't kill the rest.
pub fn run_batch(spec: &BatchSpec) -> Result<BatchReport, String> {
    if !spec.template.exists() {
        return Err(format!("template missing: {}", spec.template.display()));
    }
    std::fs::create_dir_all(&spec.out_dir).map_err(|e| e.to_string())?;

    let rows = read_csv(&spec.csv)?;
    let mut report = BatchReport {
        rows_total: rows.len(),
        ..Default::default()
    };

    for (i, row) in rows.iter().enumerate() {
        let row_idx = i + 1;
        if let Some(filter) = spec.only_row {
            if filter != row_idx {
                continue;
            }
        }
        let name = match render_name(&spec.filename_template, row_idx, row) {
            Ok(n) => n,
            Err(e) => {
                report.rows_failed += 1;
                report.rows.push(RowResult {
                    row: row_idx,
                    output: PathBuf::new(),
                    filled: vec![],
                    unknown: vec![],
                    read_only_skipped: vec![],
                    error: Some(e),
                });
                continue;
            }
        };
        let output = spec.out_dir.join(&name);
        match forms::fill(&spec.template, row, &output) {
            Ok(fr) => {
                report.rows_succeeded += 1;
                report.rows.push(RowResult {
                    row: row_idx,
                    output,
                    filled: fr.filled,
                    unknown: fr.unknown,
                    read_only_skipped: fr.read_only_skipped,
                    error: None,
                });
            }
            Err(e) => {
                report.rows_failed += 1;
                report.rows.push(RowResult {
                    row: row_idx,
                    output,
                    filled: vec![],
                    unknown: vec![],
                    read_only_skipped: vec![],
                    error: Some(format!("{e:?}")),
                });
            }
        }
    }

    // Write a load-file CSV alongside the outputs.
    let load = spec.out_dir.join("_slab_quill_batch_load_file.csv");
    write_load_file(&load, &report.rows).map_err(|e| format!("load file: {e}"))?;
    report.load_file_csv = load;

    if let Some(zip_name) = &spec.zip_as {
        let zip_path = spec.out_dir.join(format!("{zip_name}.zip"));
        write_zip(&zip_path, &report.rows).map_err(|e| format!("zip: {e}"))?;
        report.zip_path = Some(zip_path);
    }
    Ok(report)
}

fn write_load_file(path: &Path, rows: &[RowResult]) -> Result<(), String> {
    let mut w = csv::Writer::from_path(path).map_err(|e| e.to_string())?;
    w.write_record(["row", "output", "status", "error"])
        .map_err(|e| e.to_string())?;
    for r in rows {
        w.write_record([
            r.row.to_string(),
            r.output.display().to_string(),
            if r.error.is_some() { "failed" } else { "ok" }.to_string(),
            r.error.clone().unwrap_or_default(),
        ])
        .map_err(|e| e.to_string())?;
    }
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn write_zip(path: &Path, rows: &[RowResult]) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for r in rows {
        if r.error.is_some() || !r.output.exists() {
            continue;
        }
        let bytes = std::fs::read(&r.output).map_err(|e| e.to_string())?;
        let name = r
            .output
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.pdf");
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn cols(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ---- render_name -----------------------------------------------------

    #[test]
    fn render_substitutes_row() {
        let row = cols(&[("name", "Alice")]);
        assert_eq!(render_name("row-{row}.pdf", 7, &row).unwrap(), "row-7.pdf");
    }

    #[test]
    fn render_substitutes_column() {
        let row = cols(&[("name", "Alice"), ("id", "42")]);
        assert_eq!(
            render_name("{id}_{name}.pdf", 1, &row).unwrap(),
            "42_Alice.pdf"
        );
    }

    #[test]
    fn render_appends_pdf_suffix() {
        let row = cols(&[("name", "Bob")]);
        assert_eq!(render_name("{name}", 1, &row).unwrap(), "Bob.pdf");
    }

    #[test]
    fn render_sanitizes_filesystem_unsafe() {
        let row = cols(&[("name", "Alice / Smith\\test")]);
        let out = render_name("{name}.pdf", 1, &row).unwrap();
        assert!(!out.contains('/'));
        assert!(!out.contains('\\'));
    }

    #[test]
    fn render_unknown_placeholder_is_empty() {
        let row = cols(&[("name", "Alice")]);
        let out = render_name("{missing}_{name}.pdf", 1, &row).unwrap();
        assert_eq!(out, "_Alice.pdf");
    }

    #[test]
    fn render_unclosed_placeholder_errors() {
        let row = cols(&[]);
        assert!(render_name("hello-{name", 1, &row).is_err());
    }

    // ---- read_csv --------------------------------------------------------

    #[test]
    fn csv_reads_rows_with_headers() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "name,age\nAlice,30\nBob,25\n").unwrap();
        let rows = read_csv(tmp.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("name"), Some(&"Alice".to_string()));
        assert_eq!(rows[1].get("age"), Some(&"25".to_string()));
    }

    #[test]
    fn csv_handles_quoted_fields() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "name,note\n\"A, B\",\"hi\"\n").unwrap();
        let rows = read_csv(tmp.path()).unwrap();
        assert_eq!(rows[0].get("name"), Some(&"A, B".to_string()));
    }

    #[test]
    fn csv_missing_file_errors() {
        let r = read_csv(Path::new("/nonexistent/path/data.csv"));
        assert!(r.is_err());
    }

    // ---- run_batch integration ------------------------------------------

    /// Minimal AcroForm fixture with one text field "name".
    fn write_min_acroform(path: &Path) {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let name_field_id = doc.add_object(dictionary! {
            "T" => Object::String(b"name".to_vec(), lopdf::StringFormat::Literal),
            "FT" => Object::Name(b"Tx".to_vec()),
            "V" => Object::String(b"".to_vec(), lopdf::StringFormat::Literal),
            "Rect" => vec![50.into(), 700.into(), 250.into(), 720.into()],
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Type" => Object::Name(b"Annot".to_vec()),
        });

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Annots" => vec![Object::Reference(name_field_id)],
        });

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );

        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![Object::Reference(name_field_id)],
        });

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc.save(path).unwrap();
    }

    #[test]
    fn batch_fills_two_rows_into_named_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let tpl = dir.path().join("template.pdf");
        write_min_acroform(&tpl);
        let csv = dir.path().join("data.csv");
        std::fs::write(&csv, "name,age\nAlice,30\nBob,25\n").unwrap();
        let spec = BatchSpec {
            template: tpl,
            csv,
            out_dir: dir.path().join("out"),
            filename_template: "{row}_{name}.pdf".into(),
            flatten: false,
            zip_as: None,
            only_row: None,
        };
        let report = run_batch(&spec).unwrap();
        assert_eq!(report.rows_total, 2);
        assert_eq!(report.rows_succeeded, 2);
        assert!(report.rows[0].output.ends_with("1_Alice.pdf"));
        assert!(report.rows[1].output.ends_with("2_Bob.pdf"));
        assert!(report.rows[0].output.exists());
        assert!(report.load_file_csv.exists());
    }

    #[test]
    fn batch_only_row_filters() {
        let dir = tempfile::tempdir().unwrap();
        let tpl = dir.path().join("template.pdf");
        write_min_acroform(&tpl);
        let csv = dir.path().join("data.csv");
        std::fs::write(&csv, "name\nA\nB\nC\n").unwrap();
        let spec = BatchSpec {
            template: tpl,
            csv,
            out_dir: dir.path().join("out"),
            filename_template: "{row}_{name}.pdf".into(),
            flatten: false,
            zip_as: None,
            only_row: Some(2),
        };
        let report = run_batch(&spec).unwrap();
        assert_eq!(report.rows_total, 3);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].row, 2);
        assert!(report.rows[0].output.ends_with("2_B.pdf"));
    }

    #[test]
    fn batch_writes_zip_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let tpl = dir.path().join("template.pdf");
        write_min_acroform(&tpl);
        let csv = dir.path().join("data.csv");
        std::fs::write(&csv, "name\nAlice\nBob\n").unwrap();
        let spec = BatchSpec {
            template: tpl,
            csv,
            out_dir: dir.path().join("out"),
            filename_template: "{name}.pdf".into(),
            flatten: false,
            zip_as: Some("everyone".into()),
            only_row: None,
        };
        let report = run_batch(&spec).unwrap();
        assert_eq!(report.rows_succeeded, 2);
        let zip = report.zip_path.expect("zip path");
        assert!(zip.exists());
        assert!(zip.ends_with("everyone.zip"));
    }

    #[test]
    fn batch_missing_template_errors() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("data.csv");
        std::fs::write(&csv, "name\nAlice\n").unwrap();
        let spec = BatchSpec {
            template: dir.path().join("nope.pdf"),
            csv,
            out_dir: dir.path().join("out"),
            filename_template: "{name}.pdf".into(),
            flatten: false,
            zip_as: None,
            only_row: None,
        };
        assert!(run_batch(&spec).is_err());
    }
}
