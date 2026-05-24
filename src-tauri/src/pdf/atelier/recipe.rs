//! Recipe + Step — the serializable data model for a workflow.
//!
//! Recipes are JSON-portable (saved under `$APP_CONFIG/atelier/recipes/`)
//! so users can share, version, and rerun them across machines.

use serde::{Deserialize, Serialize};

/// A named, ordered chain of `Step`s applied to a single PDF (or batch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub steps: Vec<Step>,
}

fn default_version() -> u32 {
    1
}

/// One transformation applied to a PDF. New kinds are added here as the
/// underlying `pdf::*` primitives are wired through `run::apply_step`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Step {
    Ocr {
        #[serde(default = "default_lang")]
        language: String,
    },
    AutoRedact {
        #[serde(default)]
        patterns: Vec<String>,
        #[serde(default)]
        presets: Vec<String>,
    },
    Bates {
        prefix: String,
        #[serde(default = "default_bates_start")]
        start: u64,
        #[serde(default = "default_bates_digits")]
        digits: u8,
    },
    Watermark {
        text: String,
        #[serde(default = "default_opacity")]
        opacity: f32,
    },
    Flatten {
        #[serde(default = "default_flatten_dpi")]
        dpi: u32,
    },
    Compactor,
    /// Optimize the PDF for Fast Web View (PDF 1.4 §F linearization).
    ///
    /// Rewrites the file so the first page + cross-reference hints come
    /// at the front, letting a browser stream-render page 1 before the
    /// rest of the document has finished downloading. Adobe Acrobat Pro
    /// charges $239/yr for this; Slab does it free + offline.
    Linearize,
    /// Convert this PDF to a Word `.docx` file (terminal step — must be
    /// the last step in a recipe). Lets paralegals chain
    /// `OCR → AutoRedact → ConvertToDocx` in batch — the killer flow.
    ///
    /// Adobe Acrobat Pro charges $239/yr for "Export PDF to Word";
    /// Slab ships it free, offline, on every OS.
    ConvertToDocx {
        #[serde(default = "default_detect_tables")]
        detect_tables: bool,
        #[serde(default = "default_detect_lists")]
        detect_lists: bool,
        #[serde(default = "default_heading_size_ratio")]
        heading_size_ratio: f32,
    },
    /// Convert this PDF to an Excel `.xlsx` workbook (terminal step — must
    /// be the last step in a recipe). Detects aligned-column tables, types
    /// numbers and dates, emits one worksheet per page. Adobe Acrobat Pro
    /// charges $239/yr for "Export PDF to Excel" and ships your file to
    /// their cloud; PDF Expert doesn't offer it at all; Foxit charges
    /// $129/yr Pro. Slab ships it free, offline, batchable.
    ConvertToXlsx {
        #[serde(default = "default_type_numbers")]
        type_numbers: bool,
        #[serde(default = "default_type_dates")]
        type_dates: bool,
        #[serde(default = "default_include_non_table_text")]
        include_non_table_text: bool,
    },
}

impl Step {
    /// True when this step changes the output container format (PDF → DOCX/XLSX).
    /// Used by the runner to rewrite the user-supplied output filename extension.
    pub fn changes_extension(&self) -> bool {
        matches!(
            self,
            Step::ConvertToDocx { .. } | Step::ConvertToXlsx { .. }
        )
    }

    /// The output filename extension this step produces. Steps that
    /// don't change extension return `"pdf"`.
    pub fn output_extension(&self) -> &'static str {
        match self {
            Step::ConvertToDocx { .. } => "docx",
            Step::ConvertToXlsx { .. } => "xlsx",
            _ => "pdf",
        }
    }
}

impl Recipe {
    /// The extension the final output file will carry. Looks at the last
    /// step in the chain — if it's `ConvertToDocx`, the recipe yields
    /// `.docx`; otherwise `.pdf`.
    pub fn output_extension(&self) -> &'static str {
        self.steps
            .last()
            .map(Step::output_extension)
            .unwrap_or("pdf")
    }
}

fn default_lang() -> String {
    "eng".into()
}
fn default_bates_start() -> u64 {
    1
}
fn default_bates_digits() -> u8 {
    6
}
fn default_opacity() -> f32 {
    0.25
}
fn default_flatten_dpi() -> u32 {
    150
}
fn default_detect_tables() -> bool {
    true
}
fn default_detect_lists() -> bool {
    true
}
fn default_heading_size_ratio() -> f32 {
    1.25
}
fn default_type_numbers() -> bool {
    true
}
fn default_type_dates() -> bool {
    true
}
fn default_include_non_table_text() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_serializes_round_trip() {
        let r = Recipe {
            name: "Nightly Discovery".into(),
            version: 1,
            steps: vec![
                Step::Ocr {
                    language: "eng".into(),
                },
                Step::AutoRedact {
                    patterns: vec![],
                    presets: vec!["ssn".into(), "email".into()],
                },
                Step::Bates {
                    prefix: "ACME".into(),
                    start: 1,
                    digits: 6,
                },
                Step::Flatten { dpi: 150 },
            ],
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Nightly Discovery");
        assert_eq!(back.steps.len(), 4);
        assert_eq!(back, r);
    }

    #[test]
    fn step_tag_is_kebab_in_json() {
        let s = Step::AutoRedact {
            patterns: vec![],
            presets: vec![],
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["kind"], "auto-redact");
    }

    #[test]
    fn recipe_version_defaults_when_missing() {
        // Forward-compat: older saved recipes without a version field still load.
        let json = r#"{"name":"Old","steps":[{"kind":"compactor"}]}"#;
        let r: Recipe = serde_json::from_str(json).unwrap();
        assert_eq!(r.version, 1);
        assert_eq!(r.steps.len(), 1);
    }

    #[test]
    fn all_step_kinds_round_trip() {
        let steps = vec![
            Step::Ocr {
                language: "deu".into(),
            },
            Step::AutoRedact {
                patterns: vec![r"\bACME-\d+\b".into()],
                presets: vec!["email".into()],
            },
            Step::Bates {
                prefix: "X".into(),
                start: 100,
                digits: 4,
            },
            Step::Watermark {
                text: "CONFIDENTIAL".into(),
                opacity: 0.4,
            },
            Step::Flatten { dpi: 300 },
            Step::Compactor,
            Step::Linearize,
            Step::ConvertToDocx {
                detect_tables: true,
                detect_lists: true,
                heading_size_ratio: 1.25,
            },
            Step::ConvertToXlsx {
                type_numbers: true,
                type_dates: true,
                include_non_table_text: false,
            },
        ];
        for s in steps {
            let j = serde_json::to_string(&s).unwrap();
            let back: Step = serde_json::from_str(&j).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn convert_to_docx_kebab_kind_in_json() {
        let s = Step::ConvertToDocx {
            detect_tables: true,
            detect_lists: true,
            heading_size_ratio: 1.25,
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["kind"], "convert-to-docx");
    }

    #[test]
    fn convert_to_docx_defaults_round_trip() {
        // Forward-compat: a saved recipe that omits the option fields still loads.
        let json = r#"{"kind":"convert-to-docx"}"#;
        let s: Step = serde_json::from_str(json).unwrap();
        match s {
            Step::ConvertToDocx {
                detect_tables,
                detect_lists,
                heading_size_ratio,
            } => {
                assert!(detect_tables);
                assert!(detect_lists);
                assert!((heading_size_ratio - 1.25).abs() < 1e-6);
            }
            _ => panic!("expected ConvertToDocx"),
        }
    }

    #[test]
    fn changes_extension_only_for_convert_to_docx() {
        assert!(Step::ConvertToDocx {
            detect_tables: true,
            detect_lists: true,
            heading_size_ratio: 1.25,
        }
        .changes_extension());
        assert!(!Step::Compactor.changes_extension());
        assert!(!Step::Linearize.changes_extension());
        assert!(!Step::Flatten { dpi: 150 }.changes_extension());
    }

    #[test]
    fn output_extension_pdf_or_docx() {
        assert_eq!(
            Step::ConvertToDocx {
                detect_tables: true,
                detect_lists: true,
                heading_size_ratio: 1.25,
            }
            .output_extension(),
            "docx"
        );
        assert_eq!(Step::Compactor.output_extension(), "pdf");
        assert_eq!(Step::Linearize.output_extension(), "pdf");
    }

    #[test]
    fn convert_to_xlsx_kebab_kind_in_json() {
        let s = Step::ConvertToXlsx {
            type_numbers: true,
            type_dates: true,
            include_non_table_text: false,
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["kind"], "convert-to-xlsx");
    }

    #[test]
    fn convert_to_xlsx_defaults_round_trip() {
        let json = r#"{"kind":"convert-to-xlsx"}"#;
        let s: Step = serde_json::from_str(json).unwrap();
        match s {
            Step::ConvertToXlsx {
                type_numbers,
                type_dates,
                include_non_table_text,
            } => {
                assert!(type_numbers);
                assert!(type_dates);
                assert!(!include_non_table_text);
            }
            _ => panic!("expected ConvertToXlsx"),
        }
    }

    #[test]
    fn convert_to_xlsx_changes_extension_and_outputs_xlsx() {
        let s = Step::ConvertToXlsx {
            type_numbers: true,
            type_dates: true,
            include_non_table_text: false,
        };
        assert!(s.changes_extension());
        assert_eq!(s.output_extension(), "xlsx");
    }

    #[test]
    fn recipe_ending_in_convert_to_xlsx_yields_xlsx_output() {
        let r = Recipe {
            name: "Tabulate".into(),
            version: 1,
            steps: vec![
                Step::Ocr {
                    language: "eng".into(),
                },
                Step::ConvertToXlsx {
                    type_numbers: true,
                    type_dates: true,
                    include_non_table_text: false,
                },
            ],
        };
        assert_eq!(r.output_extension(), "xlsx");
    }
}
