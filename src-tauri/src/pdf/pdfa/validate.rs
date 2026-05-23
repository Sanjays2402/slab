//! Pass 6 of the Bedrock pipeline: validate a document against the subset of
//! ISO 19005-2 rules we can check offline in pure Rust.
//!
//! Slice 2 of v3.0.0 ships **structural** validation — the rules that don't
//! require the colour-normalisation / font-embedding / metadata / output-intent
//! passes (Slices 3-5) to have run. Each rule produces a [`ValidationFinding`]
//! with a stable ISO-section reference (e.g. `"6.1.3"`) and a human-readable
//! description.
//!
//! The UI groups findings by [`Severity`]:
//!
//! - [`Severity::Error`]   — document is not PDF/A-compliant.
//! - [`Severity::Warning`] — likely-compliant but worth surfacing.
//! - [`Severity::Info`]    — informational (e.g. attachment count).
//!
//! This pass operates on a document **after** the other Bedrock passes have
//! run, so before Slices 3-5 land the report will include expected-failures
//! for the §6.7.1 (XMP) and §6.2.2 (OutputIntent) rules. That's by design:
//! the tests below pin those expected failures so when Slices 3-5 land they
//! flip green and we know the contract is intact.

use crate::pdf::PdfError;
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::ConformanceLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub severity: Severity,
    /// ISO 19005-2 clause / section reference, e.g. `"6.2.2"`.
    pub iso_section: String,
    /// User-facing description of the issue.
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationReport {
    pub findings: Vec<ValidationFinding>,
}

impl ValidationReport {
    /// `true` when no [`Severity::Error`] findings exist.
    pub fn passed(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count()
    }

    fn push(&mut self, severity: Severity, section: &str, message: impl Into<String>) {
        self.findings.push(ValidationFinding {
            severity,
            iso_section: section.into(),
            message: message.into(),
        });
    }
}

/// Validate a PDF file on disk against the structural subset of ISO 19005-2
/// rules. Returns a report; the caller decides whether errors are fatal
/// (Bedrock's UI surfaces them in the post-flight tab).
pub fn validate_pdfa(path: &Path, level: ConformanceLevel) -> Result<ValidationReport, PdfError> {
    let doc = Document::load(path)?;
    Ok(validate_doc(&doc, level))
}

/// In-memory variant used by tests and by the integrated pipeline.
pub fn validate_doc(doc: &Document, level: ConformanceLevel) -> ValidationReport {
    let mut r = ValidationReport::default();

    // §6.1.3 — encryption is forbidden in any PDF/A.
    if doc.trailer.get(b"Encrypt").is_ok() {
        r.push(
            Severity::Error,
            "6.1.3",
            "document is encrypted — PDF/A forbids encryption",
        );
    }

    // §6.1.7 — PDF header version must be 1.4-1.7 for PDF/A-1/2/3.
    let v = doc.version.as_str();
    if v < "1.4" {
        r.push(
            Severity::Error,
            "6.1.7",
            format!("PDF header version {v} is below PDF/A minimum 1.4"),
        );
    } else if v > "1.7" {
        r.push(
            Severity::Warning,
            "6.1.7",
            format!("PDF header version {v} is above 1.7; PDF/A-2/3 expects 1.7"),
        );
    }

    // §6.6 + §6.7.1 + §6.2.2 — catalog-level rules.
    match doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        Ok(catalog_id) => {
            if let Ok(Object::Dictionary(catalog)) = doc.get_object(catalog_id) {
                // §6.6.1 — forbidden interactive features.
                for key in [b"OpenAction".as_slice(), b"AA".as_slice()] {
                    if catalog.has(key) {
                        r.push(
                            Severity::Error,
                            "6.6.1",
                            format!(
                                "catalog has forbidden /{} entry",
                                String::from_utf8_lossy(key)
                            ),
                        );
                    }
                }

                // §6.7.1 — XMP metadata stream required.
                if !catalog.has(b"Metadata") {
                    r.push(
                        Severity::Error,
                        "6.7.1",
                        "catalog has no /Metadata stream (XMP packet required)",
                    );
                }

                // §6.2.2 — exactly one OutputIntent of subtype GTS_PDFA1
                // is required. We only check presence here; subtype/profile
                // validation lands in Slice 3 alongside the OutputIntent
                // injection pass that produces them.
                let oi_present = matches!(
                    catalog.get(b"OutputIntents"),
                    Ok(Object::Array(_)) | Ok(Object::Reference(_))
                );
                if !oi_present {
                    r.push(
                        Severity::Error,
                        "6.2.2",
                        format!(
                            "catalog has no /OutputIntents (one GTS_PDFA1 entry required for {})",
                            level.label()
                        ),
                    );
                }

                // §6.6.2 — AcroForm without XFA is OK; XFA is not.
                if let Ok(Object::Reference(form_id)) = catalog.get(b"AcroForm") {
                    if let Ok(Object::Dictionary(form)) = doc.get_object(*form_id) {
                        if form.has(b"XFA") {
                            r.push(
                                Severity::Error,
                                "6.6.2",
                                "AcroForm contains /XFA — forbidden in PDF/A",
                            );
                        }
                    }
                } else if let Ok(Object::Dictionary(form)) = catalog.get(b"AcroForm") {
                    if form.has(b"XFA") {
                        r.push(
                            Severity::Error,
                            "6.6.2",
                            "AcroForm contains /XFA — forbidden in PDF/A",
                        );
                    }
                }
            } else {
                r.push(Severity::Error, "7.5", "catalog object is not a dictionary");
            }
        }
        Err(_) => {
            r.push(
                Severity::Error,
                "7.5",
                "trailer has no /Root reference — document is malformed",
            );
        }
    }

    // Informational: total indirect object count, useful in the UI.
    r.push(
        Severity::Info,
        "—",
        format!("document has {} indirect objects", doc.objects.len()),
    );

    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object};

    fn fresh_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![]),
            "Count" => 0,
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn cat_id_of(doc: &Document) -> lopdf::ObjectId {
        doc.trailer.get(b"Root").unwrap().as_reference().unwrap()
    }

    #[test]
    fn slice_2_clean_doc_flags_missing_metadata_and_output_intent() {
        // A sanitized-but-not-yet-metadata-injected document is expected to
        // fail §6.7.1 + §6.2.2 — those passes ship in Slice 3. This test
        // pins the contract so Slice 3 makes these go green.
        let doc = fresh_doc();
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        assert!(!r.passed());
        assert!(r.findings.iter().any(|f| f.iso_section == "6.7.1"));
        assert!(r.findings.iter().any(|f| f.iso_section == "6.2.2"));
        // Conformance label is interpolated into the OutputIntent message.
        assert!(r
            .findings
            .iter()
            .any(|f| f.iso_section == "6.2.2" && f.message.contains("PDF/A-2b")));
    }

    #[test]
    fn validate_flags_encryption() {
        let mut doc = fresh_doc();
        doc.trailer.set("Encrypt", Object::Reference((42, 0)));
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        assert!(r.findings.iter().any(|f| f.iso_section == "6.1.3"));
    }

    #[test]
    fn validate_flags_open_action_in_catalog() {
        let mut doc = fresh_doc();
        let cat_id = cat_id_of(&doc);
        if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(cat_id) {
            cat.set(
                "OpenAction",
                Object::Dictionary(dictionary! {
                    "Type" => "Action",
                    "S" => "JavaScript",
                }),
            );
        }
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        assert!(r
            .findings
            .iter()
            .any(|f| f.iso_section == "6.6.1" && f.message.contains("OpenAction")));
    }

    #[test]
    fn validate_flags_catalog_aa() {
        let mut doc = fresh_doc();
        let cat_id = cat_id_of(&doc);
        if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(cat_id) {
            cat.set("AA", Object::Dictionary(lopdf::Dictionary::new()));
        }
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        assert!(r
            .findings
            .iter()
            .any(|f| f.iso_section == "6.6.1" && f.message.contains("AA")));
    }

    #[test]
    fn validate_flags_acroform_xfa_inline() {
        let mut doc = fresh_doc();
        let cat_id = cat_id_of(&doc);
        if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(cat_id) {
            cat.set(
                "AcroForm",
                Object::Dictionary(dictionary! {
                    "Fields" => Object::Array(vec![]),
                    "XFA" => Object::string_literal("<xdp/>"),
                }),
            );
        }
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        assert!(r.findings.iter().any(|f| f.iso_section == "6.6.2"));
    }

    #[test]
    fn validate_flags_acroform_xfa_via_reference() {
        let mut doc = fresh_doc();
        let form_id = doc.add_object(dictionary! {
            "Fields" => Object::Array(vec![]),
            "XFA" => Object::string_literal("<xdp/>"),
        });
        let cat_id = cat_id_of(&doc);
        if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(cat_id) {
            cat.set("AcroForm", Object::Reference(form_id));
        }
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        assert!(r.findings.iter().any(|f| f.iso_section == "6.6.2"));
    }

    #[test]
    fn validate_passes_a_fully_conformant_doc_skeleton() {
        // Construct a doc with /Metadata + /OutputIntents stubs so validate
        // sees the required hooks (Slice 3 will fill them with real content).
        let mut doc = fresh_doc();
        let cat_id = cat_id_of(&doc);
        let meta_id = doc.add_object(Object::Stream(lopdf::Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<x:xmpmeta/>".to_vec(),
        )));
        let oi_id = doc.add_object(dictionary! {
            "Type" => "OutputIntent",
            "S" => "GTS_PDFA1",
            "OutputConditionIdentifier" => Object::string_literal("sRGB IEC61966-2.1"),
        });
        if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(cat_id) {
            cat.set("Metadata", Object::Reference(meta_id));
            cat.set(
                "OutputIntents",
                Object::Array(vec![Object::Reference(oi_id)]),
            );
        }
        let r = validate_doc(&doc, ConformanceLevel::A2b);
        // No errors expected; warnings/info are OK.
        let errs: Vec<_> = r
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "expected zero errors, got: {errs:?}");
        assert!(r.passed());
    }

    #[test]
    fn severity_counters_match() {
        let mut r = ValidationReport::default();
        r.push(Severity::Error, "x", "e");
        r.push(Severity::Warning, "y", "w");
        r.push(Severity::Warning, "z", "w");
        r.push(Severity::Info, "i", "i");
        assert!(!r.passed());
        assert_eq!(r.errors(), 1);
        assert_eq!(r.warnings(), 2);
    }

    #[test]
    fn validation_finding_serializes_with_lowercase_severity() {
        let f = ValidationFinding {
            severity: Severity::Error,
            iso_section: "6.1.3".into(),
            message: "encrypted".into(),
        };
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"severity\":\"error\""));
        assert!(json.contains("\"iso_section\":\"6.1.3\""));
    }

    #[test]
    fn validate_conformance_label_for_a3b() {
        // The OutputIntent missing message must echo the requested level.
        let doc = fresh_doc();
        let r = validate_doc(&doc, ConformanceLevel::A3b);
        assert!(r
            .findings
            .iter()
            .any(|f| f.iso_section == "6.2.2" && f.message.contains("PDF/A-3b")));
    }
}
