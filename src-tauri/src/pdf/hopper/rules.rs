//! Hopper rules — conditional routing for watched-folder pipelines.
//!
//! A [`Rule`] pairs a [`RulePredicate`] (when does this rule apply?) with a
//! [`RuleAction`] (what does it change about the default [`Watch`] config?).
//! Rules are evaluated in priority order via [`evaluate_rules`]; the first
//! match wins. Non-matching files fall through to the watch's base recipe /
//! output_dir / rename pattern.
//!
//! ## Why this exists
//!
//! v3.20.0 Hopper shipped "one recipe per watched folder." That's the
//! demo. The product feature is "drop ALL my PDFs in one inbox, Slab
//! routes them by content/filename/size into the right folders." Hazel
//! charges $42 for this; Adobe AutoActions requires enterprise licensing.
//!
//! ## Predicate cookbook
//!
//! - Tax docs by filename: `FilenameGlob { pattern: "tax_*.pdf" }`
//! - Receipts by OCR text: `TextContainsAll { needles: ["receipt", "total"] }`
//! - Single-page scans → flatten: `PageCountBetween { min: 1, max: 1 }`
//! - Big scanned images:        `SizeOver { bytes: 2_000_000 }`
//! - Catch-all fallback:        `Always`
//!
//! ## Rule actions overlay the base [`Watch`]
//!
//! An action with `recipe_id: Some("flatten")` and `output_dir: None`
//! overrides only the recipe; the file still files into the watch's
//! default output. This keeps rule configuration terse — users only
//! state what they're changing.

use globset::Glob;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::registry::Watch;

/// Information about an arrived file that predicates evaluate against.
///
/// All fields are borrowed to avoid copies in the hot path (the watcher
/// fans out a new context per file, but the predicate set is reused).
#[derive(Debug, Clone)]
pub struct RuleContext<'a> {
    pub filename: &'a str,
    pub parent_dir: &'a str,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    /// First ~2KB of extracted text (lowercased). `None` if extraction
    /// failed or was skipped (text extraction is lazy — predicates that
    /// don't need text never trigger it).
    pub text_sample: Option<&'a str>,
}

/// The discriminator for when a [`Rule`] applies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RulePredicate {
    /// Filename matches a glob (e.g. `tax_*.pdf`, `*receipt*`).
    FilenameGlob { pattern: String },
    /// Filename matches a regex (case-insensitive — we always prefix `(?i)`).
    FilenameRegex { pattern: String },
    /// Extracted text contains ALL of these (case-insensitive) substrings.
    TextContainsAll { needles: Vec<String> },
    /// Page count in the given inclusive range.
    PageCountBetween { min: u32, max: u32 },
    /// File size strictly larger than N bytes.
    SizeOver { bytes: u64 },
    /// Always matches — use as the catch-all fallback rule.
    Always,
}

impl Default for RulePredicate {
    fn default() -> Self {
        Self::Always
    }
}

impl RulePredicate {
    /// True iff this predicate matches the given file context.
    ///
    /// Malformed glob/regex patterns evaluate to `false` (rule is silently
    /// skipped); the UI test-against-file preview in the editor surfaces
    /// this to users so a typo doesn't silently route into the wrong
    /// folder.
    pub fn matches(&self, ctx: &RuleContext<'_>) -> bool {
        match self {
            Self::Always => true,
            Self::FilenameGlob { pattern } => Glob::new(pattern)
                .ok()
                .map(|g| g.compile_matcher().is_match(ctx.filename))
                .unwrap_or(false),
            Self::FilenameRegex { pattern } => {
                let p = format!("(?i){pattern}");
                Regex::new(&p)
                    .ok()
                    .map(|r| r.is_match(ctx.filename))
                    .unwrap_or(false)
            }
            Self::TextContainsAll { needles } => {
                let Some(sample) = ctx.text_sample else {
                    return false;
                };
                needles.iter().all(|n| sample.contains(&n.to_lowercase()))
            }
            Self::PageCountBetween { min, max } => {
                ctx.page_count.is_some_and(|p| p >= *min && p <= *max)
            }
            Self::SizeOver { bytes } => ctx.size_bytes > *bytes,
        }
    }
}

/// The overlay applied to a [`Watch`] when a rule matches. `None` fields
/// inherit from the watch.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleAction {
    pub recipe_id: Option<String>,
    pub output_dir: Option<String>,
    pub rename_pattern: Option<String>,
}

/// A named (predicate, action) pair. Rules are listed per-watch and
/// evaluated in array order.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub predicate: RulePredicate,
    pub action: RuleAction,
}

/// The resolved routing decision after rule evaluation. The pipeline reads
/// from this rather than the raw [`Watch`] / matched rule directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRouting {
    pub recipe_id: Option<String>,
    pub output_dir: String,
    pub rename_pattern: Option<String>,
    /// `Some(rule_name)` if a rule matched; `None` for the watch defaults.
    pub matched_rule: Option<String>,
}

impl Rule {
    /// Build the overlay action over the base watch, returning a
    /// [`ResolvedRouting`]. Unset fields in `self.action` fall back to
    /// `base`.
    pub fn apply_over(&self, base: &Watch) -> ResolvedRouting {
        ResolvedRouting {
            recipe_id: self
                .action
                .recipe_id
                .clone()
                .or_else(|| base.recipe_id.clone()),
            output_dir: self
                .action
                .output_dir
                .clone()
                .unwrap_or_else(|| base.output_dir.clone()),
            rename_pattern: self
                .action
                .rename_pattern
                .clone()
                .or_else(|| base.rename_pattern.clone()),
            matched_rule: Some(self.name.clone()),
        }
    }
}

/// Evaluate `rules` in order, returning the routing from the first match,
/// or the watch's defaults (with `matched_rule = None`) when nothing
/// matches.
///
/// This is the only public entry point the pipeline needs to call.
pub fn evaluate_rules(rules: &[Rule], base: &Watch, ctx: &RuleContext<'_>) -> ResolvedRouting {
    for r in rules {
        if r.predicate.matches(ctx) {
            return r.apply_over(base);
        }
    }
    ResolvedRouting {
        recipe_id: base.recipe_id.clone(),
        output_dir: base.output_dir.clone(),
        rename_pattern: base.rename_pattern.clone(),
        matched_rule: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(filename: &str) -> RuleContext<'_> {
        RuleContext {
            filename,
            parent_dir: "/tmp",
            size_bytes: 0,
            page_count: None,
            text_sample: None,
        }
    }

    fn base_watch() -> Watch {
        Watch {
            id: 1,
            source_dir: "/in".into(),
            output_dir: "/out".into(),
            recipe_id: Some("default".into()),
            rename_pattern: None,
            ai_rename: false,
            enabled: true,
            created_at: "0".into(),
        }
    }

    // ─── RulePredicate ────────────────────────────────────────────────

    #[test]
    fn glob_matches_case_sensitive() {
        let p = RulePredicate::FilenameGlob {
            pattern: "tax_*.pdf".into(),
        };
        assert!(p.matches(&ctx("tax_2026.pdf")));
        assert!(!p.matches(&ctx("Tax_2026.pdf")));
        assert!(!p.matches(&ctx("invoice.pdf")));
    }

    #[test]
    fn regex_matches_case_insensitive() {
        let p = RulePredicate::FilenameRegex {
            pattern: "receipt".into(),
        };
        assert!(p.matches(&ctx("My_RECEIPT.pdf")));
        assert!(p.matches(&ctx("receipt-2026.pdf")));
        assert!(!p.matches(&ctx("invoice.pdf")));
    }

    #[test]
    fn text_contains_all_needs_every_needle() {
        let p = RulePredicate::TextContainsAll {
            needles: vec!["invoice".into(), "due date".into()],
        };
        let mut c = ctx("anything.pdf");
        c.text_sample = Some("invoice no. 42 due date 2026-06-01");
        assert!(p.matches(&c));
        c.text_sample = Some("invoice no. 42");
        assert!(!p.matches(&c));
        c.text_sample = None;
        assert!(!p.matches(&c));
    }

    #[test]
    fn text_contains_all_is_lowercase_needle() {
        // We trust the caller to pre-lowercase `text_sample`; needles are
        // lowercased before comparison so users can type "Invoice".
        let p = RulePredicate::TextContainsAll {
            needles: vec!["Invoice".into()],
        };
        let mut c = ctx("x.pdf");
        c.text_sample = Some("invoice no. 42");
        assert!(p.matches(&c));
    }

    #[test]
    fn page_count_between_inclusive() {
        let p = RulePredicate::PageCountBetween { min: 1, max: 3 };
        let mut c = ctx("x.pdf");
        c.page_count = Some(1);
        assert!(p.matches(&c));
        c.page_count = Some(2);
        assert!(p.matches(&c));
        c.page_count = Some(3);
        assert!(p.matches(&c));
        c.page_count = Some(4);
        assert!(!p.matches(&c));
        c.page_count = Some(0);
        assert!(!p.matches(&c));
        c.page_count = None;
        assert!(!p.matches(&c));
    }

    #[test]
    fn size_over_strict() {
        let p = RulePredicate::SizeOver { bytes: 1_000_000 };
        let mut c = ctx("x.pdf");
        c.size_bytes = 1_000_001;
        assert!(p.matches(&c));
        c.size_bytes = 1_000_000;
        assert!(!p.matches(&c));
        c.size_bytes = 0;
        assert!(!p.matches(&c));
    }

    #[test]
    fn always_matches_everything() {
        assert!(RulePredicate::Always.matches(&ctx("any.pdf")));
        assert!(RulePredicate::Always.matches(&ctx("")));
    }

    #[test]
    fn malformed_patterns_no_match_no_panic() {
        let p = RulePredicate::FilenameGlob {
            pattern: "[unclosed".into(),
        };
        assert!(!p.matches(&ctx("anything.pdf")));

        let p = RulePredicate::FilenameRegex {
            pattern: "(?P<unfinished".into(),
        };
        assert!(!p.matches(&ctx("anything.pdf")));
    }

    // ─── Rule chain / overlay ─────────────────────────────────────────

    #[test]
    fn no_rules_returns_base_with_no_matched_rule() {
        let r = evaluate_rules(&[], &base_watch(), &ctx("x.pdf"));
        assert_eq!(r.recipe_id.as_deref(), Some("default"));
        assert_eq!(r.output_dir, "/out");
        assert!(r.rename_pattern.is_none());
        assert!(r.matched_rule.is_none());
    }

    #[test]
    fn first_match_wins() {
        let rules = vec![
            Rule {
                name: "taxes".into(),
                predicate: RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
                action: RuleAction {
                    recipe_id: Some("flatten".into()),
                    output_dir: Some("/taxes".into()),
                    rename_pattern: None,
                },
            },
            Rule {
                name: "catch-all".into(),
                predicate: RulePredicate::Always,
                action: RuleAction {
                    recipe_id: Some("noop".into()),
                    output_dir: Some("/misc".into()),
                    rename_pattern: None,
                },
            },
        ];
        let r = evaluate_rules(&rules, &base_watch(), &ctx("tax_2026.pdf"));
        assert_eq!(r.matched_rule.as_deref(), Some("taxes"));
        assert_eq!(r.output_dir, "/taxes");
        assert_eq!(r.recipe_id.as_deref(), Some("flatten"));
    }

    #[test]
    fn fallthrough_to_catchall_inherits_unspecified_fields() {
        let rules = vec![
            Rule {
                name: "taxes".into(),
                predicate: RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
                action: RuleAction::default(),
            },
            Rule {
                name: "everything-else".into(),
                predicate: RulePredicate::Always,
                action: RuleAction {
                    recipe_id: Some("misc".into()),
                    output_dir: None,
                    rename_pattern: None,
                },
            },
        ];
        let r = evaluate_rules(&rules, &base_watch(), &ctx("invoice.pdf"));
        assert_eq!(r.matched_rule.as_deref(), Some("everything-else"));
        assert_eq!(r.recipe_id.as_deref(), Some("misc"));
        assert_eq!(r.output_dir, "/out"); // inherited from base
    }

    #[test]
    fn action_partial_overlay_inherits_unspecified() {
        let rules = vec![Rule {
            name: "just-output".into(),
            predicate: RulePredicate::Always,
            action: RuleAction {
                recipe_id: None,
                output_dir: Some("/alt".into()),
                rename_pattern: None,
            },
        }];
        let r = evaluate_rules(&rules, &base_watch(), &ctx("any.pdf"));
        assert_eq!(r.recipe_id.as_deref(), Some("default")); // inherited
        assert_eq!(r.output_dir, "/alt"); // overridden
        assert!(r.rename_pattern.is_none());
    }

    #[test]
    fn rename_pattern_can_be_overridden() {
        let mut base = base_watch();
        base.rename_pattern = Some("{date}_{ai_title}.pdf".into());
        let rules = vec![Rule {
            name: "taxes-keep-original-name".into(),
            predicate: RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
            action: RuleAction {
                recipe_id: None,
                output_dir: None,
                rename_pattern: Some("{original}.pdf".into()),
            },
        }];
        let r = evaluate_rules(&rules, &base, &ctx("tax_2026.pdf"));
        assert_eq!(r.rename_pattern.as_deref(), Some("{original}.pdf"));
    }

    // ─── Serde round-trip ─────────────────────────────────────────────

    #[test]
    fn rule_serializes_with_kebab_kind_tag() {
        let r = Rule {
            name: "taxes".into(),
            predicate: RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
            action: RuleAction::default(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            json.contains(r#""kind":"filename-glob""#),
            "expected kebab-case tag, got: {json}"
        );
        let back: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn always_predicate_serializes() {
        let p = RulePredicate::Always;
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, r#"{"kind":"always"}"#);
        let back: RulePredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
