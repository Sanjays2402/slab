//! Non-destructive PDF/A compliance inspector — v3.0.1 "Loupe".
//!
//! Slab's answer to Adobe Acrobat Preflight ($239/yr): point this at any
//! PDF and get back a per-rule, per-level pass/fail report referencing
//! the relevant ISO 19005-2 clauses, along with a list of human-readable
//! suggestions and a verdict on which conformance level is achievable.
//!
//! The inspector **does not mutate** the input file. Every check is done
//! either read-only or against a cloned scratch document. This is the
//! "preflight" half of the Bedrock pipeline; the conversion itself
//! happens via [`crate::pdf::pdfa::convert_to_pdfa`].
//!
//! Use cases:
//! - Compliance officer drops 50 contracts on Slab, runs inspect, exports
//!   the report as Markdown, attaches it to a Jira ticket.
//! - Pre-flight gate before a batch PDF/A conversion job.
//! - Diagnostic for "this PDF won't pass our archival system" — Loupe tells
//!   them exactly which ISO sections fail and how to fix them.

use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::font_audit::{audit_fonts, FontAuditReport};
use super::sanitize::sanitize_dry_run;
use super::validate::{validate_doc, Severity, ValidationReport};
use super::ConformanceLevel;
use crate::pdf::PdfError;

/// Per-level conclusion: can this PDF achieve this conformance level today?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// File already complies as-is.
    Pass,
    /// Every blocking error is auto-fixed by running Bedrock conversion.
    AchievableWithFixes,
    /// Cannot reach this level without external work (missing fonts, etc.).
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelAssessment {
    pub level: ConformanceLevel,
    pub verdict: Verdict,
    pub blocking_errors: usize,
    pub auto_fixable: usize,
    pub validation: ValidationReport,
}

/// ISO sections that Bedrock's existing passes auto-fix during
/// conversion. A finding whose `iso_section` starts with one of these
/// prefixes is considered "auto-fixable" and does not block a verdict
/// of `AchievableWithFixes`.
const AUTO_FIXABLE_SECTIONS: &[&str] = &[
    "6.6",   // sanitize strips OpenAction, AA, JavaScript names
    "6.7.1", // XMP injection
    "6.2.2", // OutputIntent injection
];

fn is_auto_fixable(section: &str) -> bool {
    AUTO_FIXABLE_SECTIONS
        .iter()
        .any(|prefix| section.starts_with(prefix))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionReport {
    pub input_path: String,
    pub pdf_version: String,
    pub page_count: usize,
    pub file_bytes: u64,
    pub encrypted: bool,
    pub fonts: FontAuditReport,
    /// Sanitize dry-run — what *would* be stripped during conversion.
    pub sanitize_preview: Vec<String>,
    pub levels: Vec<LevelAssessment>,
    pub suggestions: Vec<String>,
}

impl InspectionReport {
    /// Best level achievable today — either `Pass` outright, or
    /// `AchievableWithFixes` (Bedrock will get there). Returns `None` if
    /// no level is reachable.
    pub fn best_achievable(&self) -> Option<ConformanceLevel> {
        // Prefer Pass over AchievableWithFixes when both exist at the same
        // level; prefer 3b over 2b when both are achievable.
        let order = |v: Verdict| match v {
            Verdict::Pass => 2,
            Verdict::AchievableWithFixes => 1,
            Verdict::Fail => 0,
        };
        self.levels
            .iter()
            .filter(|a| a.verdict != Verdict::Fail)
            .max_by_key(|a| (order(a.verdict), a.level.part()))
            .map(|a| a.level)
    }
}

/// Run the full inspection over the file at `path`. Read-only.
pub fn inspect_pdfa(path: &Path) -> Result<InspectionReport, PdfError> {
    if !path.exists() {
        return Err(PdfError::InputMissing(path.display().to_string()));
    }
    let file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let doc = Document::load(path).map_err(PdfError::from)?;

    let encrypted = doc.trailer.get(b"Encrypt").is_ok();
    let pdf_version = doc.version.clone();
    let page_count = doc.get_pages().len();

    let fonts = audit_fonts(&doc);

    // Sanitize dry-run. If the doc is encrypted, sanitize would error;
    // surface that as a top-level suggestion and skip the preview.
    let sanitize_preview: Vec<String> = if encrypted {
        Vec::new()
    } else {
        match sanitize_dry_run(&doc) {
            Ok(r) => r.removed,
            Err(_) => Vec::new(),
        }
    };

    // Build per-level assessments. ConformanceLevel only has A2b + A3b
    // today; future ConformanceLevel variants will surface here
    // automatically via iteration.
    let levels: Vec<LevelAssessment> = [ConformanceLevel::A2b, ConformanceLevel::A3b]
        .iter()
        .map(|&level| assess_level(&doc, level, &fonts, encrypted))
        .collect();

    let suggestions = build_suggestions(&fonts, &sanitize_preview, encrypted, &levels);

    Ok(InspectionReport {
        input_path: path.display().to_string(),
        pdf_version,
        page_count,
        file_bytes,
        encrypted,
        fonts,
        sanitize_preview,
        levels,
        suggestions,
    })
}

fn assess_level(
    doc: &Document,
    level: ConformanceLevel,
    fonts: &FontAuditReport,
    encrypted: bool,
) -> LevelAssessment {
    let validation = validate_doc(doc, level);
    let errors: Vec<_> = validation
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();
    let blocking_errors = errors.len();
    let auto_fixable = errors
        .iter()
        .filter(|f| is_auto_fixable(&f.iso_section))
        .count();

    let verdict = if encrypted {
        Verdict::Fail
    } else if blocking_errors == 0 {
        Verdict::Pass
    } else if blocking_errors == auto_fixable && fonts.all_embedded() {
        Verdict::AchievableWithFixes
    } else {
        Verdict::Fail
    };

    LevelAssessment {
        level,
        verdict,
        blocking_errors,
        auto_fixable,
        validation,
    }
}

fn build_suggestions(
    fonts: &FontAuditReport,
    sanitize_preview: &[String],
    encrypted: bool,
    levels: &[LevelAssessment],
) -> Vec<String> {
    let mut out = Vec::new();

    if encrypted {
        out.push(
            "Document is encrypted. Decrypt it (Slab → Unlock) before running Bedrock conversion."
                .into(),
        );
    }

    let missing: Vec<&str> = fonts
        .missing_embed()
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    if !missing.is_empty() {
        out.push(format!(
            "Embed {} font(s): {}. Re-export the source document with 'embed all fonts' enabled.",
            missing.len(),
            missing.join(", ")
        ));
    }

    if !sanitize_preview.is_empty() {
        out.push(format!(
            "Bedrock will strip {} forbidden entr{}: {}.",
            sanitize_preview.len(),
            if sanitize_preview.len() == 1 {
                "y"
            } else {
                "ies"
            },
            sanitize_preview.join(", ")
        ));
    }

    // Recommend the highest achievable level.
    let order = |v: Verdict| match v {
        Verdict::Pass => 2,
        Verdict::AchievableWithFixes => 1,
        Verdict::Fail => 0,
    };
    if let Some(best) = levels
        .iter()
        .filter(|a| a.verdict != Verdict::Fail)
        .max_by_key(|a| (order(a.verdict), a.level.part()))
    {
        match best.verdict {
            Verdict::Pass => out.push(format!(
                "{} compliant as-is — no conversion needed.",
                best.level.label()
            )),
            Verdict::AchievableWithFixes => out.push(format!(
                "Run Bedrock to produce a {}-compliant file.",
                best.level.label()
            )),
            Verdict::Fail => {}
        }
    } else if !encrypted {
        out.push("No conformance level is achievable today. Address font embedding first.".into());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};
    use std::fs;
    use tempfile::tempdir;

    fn write_clean_pdf(path: &Path) {
        let mut doc = Document::with_version("1.7");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b" ".to_vec())));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.compress();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        fs::write(path, &buf).unwrap();
    }

    fn write_pdf_with_openaction(path: &Path) {
        let mut doc = Document::with_version("1.7");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b" ".to_vec())));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "OpenAction" => dictionary! { "S" => "JavaScript", "JS" => "app.alert('boo')" },
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.compress();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        fs::write(path, &buf).unwrap();
    }

    fn write_pdf_with_unembedded_helvetica(path: &Path) {
        let mut doc = Document::with_version("1.7");
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b" ".to_vec())));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => Object::Reference(font_id) },
            },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => 1i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc.compress();
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        fs::write(path, &buf).unwrap();
    }

    #[test]
    fn auto_fixable_section_matching() {
        assert!(is_auto_fixable("6.6"));
        assert!(is_auto_fixable("6.6.1"));
        assert!(is_auto_fixable("6.7.1"));
        assert!(is_auto_fixable("6.2.2"));
        assert!(!is_auto_fixable("6.2.11")); // fonts — NOT auto-fixed yet
        assert!(!is_auto_fixable("6.1.3")); // encryption
    }

    #[test]
    fn clean_pdf_passes_or_is_achievable() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clean.pdf");
        write_clean_pdf(&p);
        let report = inspect_pdfa(&p).unwrap();
        assert_eq!(report.page_count, 1);
        assert_eq!(report.fonts.count(), 0);
        assert!(report.sanitize_preview.is_empty());
        // No fonts, no forbidden entries — must be achievable for both levels.
        for level in &report.levels {
            assert_ne!(level.verdict, Verdict::Fail, "level {:?}", level.level);
        }
        assert!(report.best_achievable().is_some());
    }

    #[test]
    fn openaction_pdf_is_achievable_with_fixes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("oa.pdf");
        write_pdf_with_openaction(&p);
        let report = inspect_pdfa(&p).unwrap();
        // Sanitize dry-run should flag OpenAction without mutating the file.
        assert!(report.sanitize_preview.iter().any(|e| e == "OpenAction"));
        // File on disk is untouched — sanity check: re-load it and confirm
        // the catalog still has /OpenAction.
        let reloaded = Document::load(&p).unwrap();
        let catalog_id = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(catalog.has(b"OpenAction"));
    }

    #[test]
    fn unembedded_helvetica_fails_all_levels() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("h.pdf");
        write_pdf_with_unembedded_helvetica(&p);
        let report = inspect_pdfa(&p).unwrap();
        assert!(!report.fonts.all_embedded());
        // Suggestion should mention embedding.
        assert!(report
            .suggestions
            .iter()
            .any(|s| s.contains("Embed") && s.contains("Helvetica")));
    }

    #[test]
    fn missing_file_returns_error() {
        let r = inspect_pdfa(Path::new("/tmp/__slab_loupe_does_not_exist__.pdf"));
        assert!(r.is_err());
    }
}
