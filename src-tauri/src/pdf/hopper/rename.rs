//! Hopper rename — pure-function template substitution.
//!
//! Given a template like `"{date}_{ai_title}.pdf"` and a triple of
//! `(date, ai_title, stem)`, produce a safe filename. Slugification
//! is conservative: spaces → `_`, non-alphanumeric stripped, runs of
//! `_` collapsed, leading/trailing `_` trimmed. No locale-sensitive
//! casing — we leave alphabetic characters in their input case (the
//! pipeline supplies title-cased input).
//!
//! Substitutions:
//! - `{date}` — YYYY-MM-DD, caller supplies (we don't hit `chrono`).
//! - `{ai_title}` — slugified AI suggestion, or empty if `None`.
//! - `{stem}` — original filename without extension, slugified.
//! - `{ext}` — original extension, lowercased, no leading dot.
//!
//! Unknown tokens are left literal (so `{author}` survives intact)
//! to avoid surprising users while we ship more substitutions later.

/// Substitute `{date}`, `{ai_title}`, `{stem}`, `{ext}` into `template`.
pub fn apply_pattern(
    template: &str,
    date: &str,
    ai_title: Option<&str>,
    stem: &str,
    ext: &str,
) -> String {
    let mut out = template.to_string();
    out = out.replace("{date}", &slugify(date));
    out = out.replace("{ai_title}", &ai_title.map(slugify).unwrap_or_default());
    out = out.replace("{stem}", &slugify(stem));
    out = out.replace("{ext}", &ext.trim_start_matches('.').to_lowercase());
    out
}

/// Lower-fidelity slug: ASCII alphanumerics + `_` + `-` + `.` survive;
/// whitespace becomes `_`; everything else is dropped; runs of `_` are
/// collapsed; leading/trailing `_` trimmed. Note we leave letter case
/// untouched so a title like "NDA Acme" → `NDA_Acme`.
pub fn slugify(input: &str) -> String {
    let mut buf = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '.' {
            buf.push(ch);
        } else if ch.is_whitespace() || ch == '_' {
            buf.push('_');
        }
        // everything else dropped
    }
    // Collapse runs of `_`
    let mut collapsed = String::with_capacity(buf.len());
    let mut prev_us = false;
    for ch in buf.chars() {
        if ch == '_' {
            if !prev_us {
                collapsed.push('_');
            }
            prev_us = true;
        } else {
            collapsed.push(ch);
            prev_us = false;
        }
    }
    collapsed.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_substitution_typical() {
        let out = apply_pattern(
            "{date}_{ai_title}.pdf",
            "2026-05-24",
            Some("NDA Acme Corp"),
            "scan_001",
            "pdf",
        );
        assert_eq!(out, "2026-05-24_NDA_Acme_Corp.pdf");
    }

    #[test]
    fn pattern_missing_ai_title_collapses() {
        // `{ai_title}` substitutes to empty when None. Resulting
        // double-underscores are NOT collapsed (apply_pattern is a
        // raw substitute; pipeline post-processes if desired).
        let out = apply_pattern("{date}_{ai_title}.pdf", "2026-05-24", None, "scan", "pdf");
        assert_eq!(out, "2026-05-24_.pdf");
    }

    #[test]
    fn pattern_stem_fallback() {
        let out = apply_pattern(
            "{stem}_archived.{ext}",
            "ignored",
            None,
            "Quarterly Report Q4!!!",
            "PDF",
        );
        assert_eq!(out, "Quarterly_Report_Q4_archived.pdf");
    }

    #[test]
    fn unknown_tokens_pass_through() {
        let out = apply_pattern("{date}_{author}.pdf", "2026-05-24", None, "x", "pdf");
        // `{author}` survives literally — we may wire it later.
        assert_eq!(out, "2026-05-24_{author}.pdf");
    }

    #[test]
    fn slugify_strips_punctuation_and_unicode() {
        assert_eq!(slugify("Hello, World!"), "Hello_World");
        assert_eq!(
            slugify("  spaces   are   collapsed  "),
            "spaces_are_collapsed"
        );
        // Non-ASCII letters dropped, em-dash dropped (not in allow-list).
        assert_eq!(slugify("résumé—2026"), "rsum2026");
        assert_eq!(slugify("___"), "");
        assert_eq!(slugify("name.v2.pdf"), "name.v2.pdf");
    }

    #[test]
    fn slugify_preserves_case() {
        assert_eq!(slugify("NDA Acme"), "NDA_Acme");
    }
}
