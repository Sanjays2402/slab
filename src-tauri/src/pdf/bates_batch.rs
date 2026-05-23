// Bates batch driver — apply Bates numbering across an ordered list of
// PDF files, threading the counter from one file to the next so the
// whole production set has one monotonically increasing Bates range.
//
// Also emits a "load file" — a CSV or JSON index that legal-discovery
// software (Relativity, Concordance, Everlaw, CaseMap) ingests to map
// Bates labels back to the source documents.
//
// This is the enterprise-paralegal capability Adobe Acrobat Pro DC charges
// $239/yr for. Slab does it free, offline, and in one shot for thousands
// of files.

use crate::pdf::bates::{apply_bates, BatesOpts, BatesReport};
use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct BatchInput {
    /// Ordered list of input PDF paths. Order determines Bates order —
    /// files earlier in the list get the lower Bates labels.
    pub inputs: Vec<PathBuf>,
    /// Where stamped PDFs go. We write `<output_dir>/<basename>.pdf`,
    /// preserving the original filename.
    pub output_dir: PathBuf,
    /// Shared options applied to every file. `start_at` is the FIRST
    /// label of the FIRST file; subsequent files chain off `next_start`.
    pub opts: BatesOpts,
    /// Optional load-file path. `None` → no load file written.
    pub load_file: Option<LoadFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum LoadFile {
    Csv { path: PathBuf },
    Json { path: PathBuf },
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchReport {
    pub files_processed: u32,
    pub pages_stamped: u64,
    pub first_label: String,
    pub last_label: String,
    pub per_file: Vec<FileReport>,
    pub load_file_written: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileReport {
    pub source: PathBuf,
    pub output: PathBuf,
    pub first_label: String,
    pub last_label: String,
    pub pages: u32,
}

pub fn apply_bates_batch(input: &BatchInput) -> Result<BatchReport, PdfError> {
    if input.inputs.is_empty() {
        return Err(PdfError::Other("batch inputs is empty".into()));
    }
    if !input.output_dir.is_dir() {
        return Err(PdfError::Other(format!(
            "output_dir is not a directory: {}",
            input.output_dir.display()
        )));
    }

    let mut per_file = Vec::with_capacity(input.inputs.len());
    let mut next = input.opts.start_at;
    let mut first_label = String::new();
    let mut last_label = String::new();
    let mut total_pages: u64 = 0;

    for src in &input.inputs {
        let basename = src
            .file_name()
            .ok_or_else(|| PdfError::Other(format!("source has no filename: {}", src.display())))?;
        let dst = input.output_dir.join(basename);

        let mut opts = input.opts.clone();
        opts.start_at = next;
        let r: BatesReport = apply_bates(src, &dst, &opts)?;

        if first_label.is_empty() {
            first_label = r.first_label.clone();
        }
        if !r.last_label.is_empty() {
            last_label = r.last_label.clone();
        }
        total_pages += r.pages_stamped as u64;

        per_file.push(FileReport {
            source: src.clone(),
            output: dst,
            first_label: r.first_label,
            last_label: r.last_label,
            pages: r.pages_stamped,
        });
        next = r.next_start;
    }

    let load_file_written = match &input.load_file {
        Some(lf) => Some(write_load_file(lf, &per_file)?),
        None => None,
    };

    Ok(BatchReport {
        files_processed: per_file.len() as u32,
        pages_stamped: total_pages,
        first_label,
        last_label,
        per_file,
        load_file_written,
    })
}

fn write_load_file(lf: &LoadFile, rows: &[FileReport]) -> Result<PathBuf, PdfError> {
    match lf {
        LoadFile::Csv { path } => write_csv(path, rows).map(|_| path.clone()),
        LoadFile::Json { path } => write_json(path, rows).map(|_| path.clone()),
    }
}

fn write_csv(path: &Path, rows: &[FileReport]) -> Result<(), PdfError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "source,output,first_label,last_label,pages")
        .map_err(|e| PdfError::Other(e.to_string()))?;
    for r in rows {
        let s = csv_escape(&r.source.display().to_string());
        let o = csv_escape(&r.output.display().to_string());
        writeln!(
            f,
            "{},{},{},{},{}",
            s, o, r.first_label, r.last_label, r.pages
        )
        .map_err(|e| PdfError::Other(e.to_string()))?;
    }
    Ok(())
}

fn csv_escape(s: &str) -> String {
    let needs_quote = s.contains(',') || s.contains('"') || s.contains('\n');
    if !needs_quote {
        return s.to_string();
    }
    let escaped = s.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn write_json(path: &Path, rows: &[FileReport]) -> Result<(), PdfError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let s = serde_json::to_string_pretty(&rows)
        .map_err(|e| PdfError::Other(format!("serialize: {e}")))?;
    std::fs::write(path, s)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;
    use tempfile::tempdir;

    #[test]
    fn batch_threads_counter_across_files() {
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        let c = tmp.path().join("c.pdf");
        make_n_page_pdf(&a, 3);
        make_n_page_pdf(&b, 2);
        make_n_page_pdf(&c, 4);

        let out_dir = tmp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();

        let input = BatchInput {
            inputs: vec![a, b, c],
            output_dir: out_dir,
            opts: BatesOpts {
                prefix: "ACME".into(),
                start_at: 1,
                digits: 6,
                ..Default::default()
            },
            load_file: None,
        };

        let r = apply_bates_batch(&input).unwrap();
        assert_eq!(r.files_processed, 3);
        assert_eq!(r.pages_stamped, 9);
        assert_eq!(r.first_label, "ACME000001");
        assert_eq!(r.last_label, "ACME000009");
        assert_eq!(r.per_file[0].first_label, "ACME000001");
        assert_eq!(r.per_file[0].last_label, "ACME000003");
        assert_eq!(r.per_file[1].first_label, "ACME000004");
        assert_eq!(r.per_file[1].last_label, "ACME000005");
        assert_eq!(r.per_file[2].first_label, "ACME000006");
        assert_eq!(r.per_file[2].last_label, "ACME000009");
    }

    #[test]
    fn batch_writes_csv_load_file() {
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        make_n_page_pdf(&a, 2);
        let out_dir = tmp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let csv = tmp.path().join("index.csv");

        let r = apply_bates_batch(&BatchInput {
            inputs: vec![a.clone()],
            output_dir: out_dir,
            opts: BatesOpts {
                prefix: "X".into(),
                start_at: 1,
                digits: 4,
                ..Default::default()
            },
            load_file: Some(LoadFile::Csv { path: csv.clone() }),
        })
        .unwrap();

        assert_eq!(r.load_file_written.as_deref(), Some(csv.as_path()));
        let body = std::fs::read_to_string(&csv).unwrap();
        assert!(body.starts_with("source,output,first_label,last_label,pages\n"));
        assert!(body.contains("X0001"));
        assert!(body.contains("X0002"));
    }

    #[test]
    fn batch_writes_json_load_file() {
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        let b = tmp.path().join("b.pdf");
        make_n_page_pdf(&a, 1);
        make_n_page_pdf(&b, 2);
        let out_dir = tmp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let json = tmp.path().join("index.json");

        apply_bates_batch(&BatchInput {
            inputs: vec![a, b],
            output_dir: out_dir,
            opts: BatesOpts {
                prefix: "DOC".into(),
                start_at: 100,
                digits: 5,
                ..Default::default()
            },
            load_file: Some(LoadFile::Json { path: json.clone() }),
        })
        .unwrap();

        let body = std::fs::read_to_string(&json).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["first_label"], "DOC00100");
        assert_eq!(arr[0]["last_label"], "DOC00100");
        assert_eq!(arr[1]["first_label"], "DOC00101");
        assert_eq!(arr[1]["last_label"], "DOC00102");
    }

    #[test]
    fn batch_rejects_empty_input() {
        let tmp = tempdir().unwrap();
        let r = apply_bates_batch(&BatchInput {
            inputs: vec![],
            output_dir: tmp.path().to_path_buf(),
            opts: BatesOpts::default(),
            load_file: None,
        });
        assert!(r.is_err());
    }

    #[test]
    fn batch_rejects_missing_output_dir() {
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("a.pdf");
        make_n_page_pdf(&a, 1);
        let r = apply_bates_batch(&BatchInput {
            inputs: vec![a],
            output_dir: tmp.path().join("does-not-exist"),
            opts: BatesOpts::default(),
            load_file: None,
        });
        assert!(r.is_err());
    }

    #[test]
    fn batch_chains_with_custom_start_at() {
        // Real-world: paralegal already used ACME000001-ACME001000, now
        // adding a supplemental production starting at 1001.
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("supp.pdf");
        make_n_page_pdf(&a, 3);
        let out_dir = tmp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();

        let r = apply_bates_batch(&BatchInput {
            inputs: vec![a],
            output_dir: out_dir,
            opts: BatesOpts {
                prefix: "ACME".into(),
                start_at: 1001,
                digits: 6,
                ..Default::default()
            },
            load_file: None,
        })
        .unwrap();
        assert_eq!(r.first_label, "ACME001001");
        assert_eq!(r.last_label, "ACME001003");
    }
}
