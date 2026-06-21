//! v3.40 Slice 79 — Hopper rule **coverage analyzer**.
//!
//! ## Why this exists
//!
//! The Hopper rule editor's existing live-preview pane is great for
//! "did this rule match THIS file?" question for up to 5 sample files
//! (round-15 work in [`crate::pdf::hopper::cmds::slab_hopper_test_rules`]).
//! It does not, however, answer the question users actually have once
//! they have a rule chain longer than two rules:
//!
//!   "Across my last N real files, how many would each rule catch,
//!    and how many would fall through to the watch defaults?"
//!
//! That is the **coverage** question. Without it, a paralegal building
//! a 6-rule chain has no way to spot:
//!
//! - a dead rule that's been shadowed by an earlier `Always` match
//! - a too-narrow predicate that catches 0 of the 100 real files
//! - the fall-through tail — files the chain doesn't catch at all,
//!   which silently route to the watch's default recipe.
//!
//! This module is the pure-data primitive. It walks a candidate rule
//! set against a list of [`RuleSample`]s and returns:
//!
//! - [`RuleCoverage`] per rule (first-match count + total-match count +
//!   dead-rule flag for chains where ordering matters), and
//! - the **fall-through count** of samples that matched no rule, and
//! - the total samples scanned.
//!
//! ## Why two counts per rule (`first_match` vs `would_match`)
//!
//! First-match-wins is the routing semantics, so `first_match` is the
//! correct "actual routing volume" number — it's what the rule will do
//! at runtime. `would_match` answers the question "what would this
//! rule catch in isolation?" — useful for spotting **shadowed** rules:
//! if `first_match == 0` but `would_match > 0`, the rule is dead at
//! its current position; reordering it earlier in the chain (or
//! tightening the rule above it) would make it fire.
//!
//! Shadowed-rule detection is the high-value insight the editor's
//! per-file preview can't surface; a coverage analyzer over the last
//! 100 runs can.
//!
//! ## Pure data
//!
//! The module is **pure data** — no I/O, no DB, no Tauri. The Tauri
//! command surface in [`crate::pdf::hopper::cmds`] is responsible for
//! sourcing the sample list (typically from
//! [`crate::pdf::hopper::log::HopperLog::list_recent`]) and the rule
//! set; this module just answers the analytical question.

use serde::{Deserialize, Serialize};

use super::rules::{Rule, RuleContext};

/// A single file the coverage analyzer evaluates against the rule
/// chain. Mirrors [`RuleContext`] but owned (the sample list lives
/// across the lock guard that pulled it from the log).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSample {
    pub filename: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub page_count: Option<u32>,
    /// Lowercased text sample; only required by `TextContainsAll`
    /// predicates. `None` is a valid sample — text-aware rules will
    /// simply not match (matches the live-preview semantics).
    #[serde(default)]
    pub text_sample: Option<String>,
}

impl RuleSample {
    /// Build a borrowed [`RuleContext`] for predicate evaluation.
    fn as_context<'a>(&'a self) -> RuleContext<'a> {
        RuleContext {
            filename: self.filename.as_str(),
            parent_dir: "",
            size_bytes: self.size_bytes,
            page_count: self.page_count,
            text_sample: self.text_sample.as_deref(),
        }
    }
}

/// Per-rule coverage counts. Sums of all fields plus `fallthrough`
/// equal the total sample count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleCoverage {
    /// 0-based index of the rule in the input chain.
    pub index: usize,
    /// User-visible name of the rule (echoed for the UI).
    pub name: String,
    /// Number of samples for which THIS rule was the first to match —
    /// the actual routing volume at runtime.
    pub first_match: u64,
    /// Number of samples for which this rule's predicate evaluates to
    /// `true` in isolation, regardless of earlier rules. When this is
    /// strictly larger than `first_match`, the rule is partially
    /// shadowed by earlier rules; when `first_match == 0` but
    /// `would_match > 0`, the rule is DEAD at its current position.
    pub would_match: u64,
    /// Convenience flag: `true` iff this rule never wins a sample at
    /// its current position, but would win at least one sample if
    /// moved earlier. The frontend uses this to surface a "dead rule"
    /// chip in the coverage panel without re-deriving the predicate.
    pub dead_at_position: bool,
}

/// The full coverage report for one rule chain against one sample
/// set. The fields are independent — a caller can render only the
/// per-rule strip, only the fall-through count, only the totals,
/// without re-computing anything client-side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleCoverageReport {
    /// Per-rule rows in input order. Length == rules.len().
    pub rules: Vec<RuleCoverage>,
    /// Number of samples that matched NO rule and fell through to the
    /// watch defaults.
    pub fallthrough: u64,
    /// Total samples scanned.
    pub total_samples: u64,
}

/// Compute the coverage report for `rules` against `samples`.
///
/// Cost is `O(rules.len() * samples.len())` predicate evaluations —
/// the analyzer scans each sample through the FULL chain (not stopping
/// at first match) so it can populate `would_match` per rule. For the
/// expected sizes (<= ~500 samples, <= ~20 rules) this is well below
/// 1ms in practice.
///
/// An empty rule set returns an empty `rules` Vec and `fallthrough`
/// equal to the sample count (all samples fall through to defaults).
/// An empty sample set returns all-zero counters.
pub fn compute_coverage(rules: &[Rule], samples: &[RuleSample]) -> RuleCoverageReport {
    let n_rules = rules.len();
    let mut first_match = vec![0u64; n_rules];
    let mut would_match = vec![0u64; n_rules];
    let mut fallthrough: u64 = 0;

    for sample in samples {
        let ctx = sample.as_context();
        let mut first_winner: Option<usize> = None;
        for (i, rule) in rules.iter().enumerate() {
            if rule.predicate.matches(&ctx) {
                would_match[i] += 1;
                if first_winner.is_none() {
                    first_winner = Some(i);
                }
            }
        }
        match first_winner {
            Some(i) => first_match[i] += 1,
            None => fallthrough += 1,
        }
    }

    let coverage = rules
        .iter()
        .enumerate()
        .map(|(i, r)| RuleCoverage {
            index: i,
            name: r.name.clone(),
            first_match: first_match[i],
            would_match: would_match[i],
            // Dead-at-position is the actionable insight: never fires at
            // this index, but WOULD fire if moved earlier. A rule that
            // genuinely matches nothing (would_match == 0) isn't "dead"
            // by position — it's just too narrow. We surface that as a
            // separate diagnostic in the UI (zero-coverage rules) rather
            // than conflating it with shadowing.
            dead_at_position: first_match[i] == 0 && would_match[i] > 0,
        })
        .collect();

    RuleCoverageReport {
        rules: coverage,
        fallthrough,
        total_samples: samples.len() as u64,
    }
}

// ─── Sample drilldown primitive (v3.40 Slice 83) ─────────────────────
//
// The coverage report answers "how many samples did each rule catch?",
// but the natural follow-up question — "which 8 files fell through to
// the watch defaults?" or "show me the 23 samples Rule 3 routed" — has
// no answer without re-walking the chain client-side. This primitive
// fills the gap with a second pure-data pass that groups samples by
// winner (rule index, or "fall through") and returns the per-bucket
// sample list capped to a user-configurable preview cap. The UI then
// renders the bucket as a popover when a coverage row is clicked.
//
// We keep this separate from `compute_coverage` for three reasons:
//
// 1. The drilldown carries the FILES (not just counts), so the payload
//    is meaningfully larger; the coverage panel doesn't always need it.
// 2. The bucket the user wants to drill into is one of N+1 choices
//    (rules + fall-through); returning all buckets every time would
//    waste bandwidth on a 20-rule chain.
// 3. The preview cap (default 25) is independent of the coverage
//    sample cap (default 100) — a user might scan 500 samples but only
//    care about the first 25 fall-throughs.

/// Selector for which bucket of samples to extract. Mirrors the UI
/// choice: a specific rule by 0-based index, or the catch-all
/// fall-through bucket.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SampleBucket {
    /// Samples this rule was the FIRST to match. Matches the
    /// `first_match` count in [`RuleCoverage`].
    Rule { index: usize },
    /// Samples that no rule matched — fell through to the watch's
    /// default recipe.
    Fallthrough,
}

/// The result of a drilldown: which bucket was requested, the
/// matching samples (capped to `preview_cap`), the FULL bucket size
/// (so the UI can render "Showing 25 of 47"), and a `truncated` flag
/// for convenience.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SampleDrilldown {
    pub bucket: SampleBucket,
    /// The samples in this bucket, capped to `preview_cap`. Order is
    /// the input order from `samples` (which is typically
    /// newest-first when sourced from the run log, so the user sees
    /// the most recent first).
    pub samples: Vec<RuleSample>,
    /// Total count of samples in this bucket — equals
    /// `samples.len()` when not truncated, larger when truncated.
    /// The UI uses this to render "Showing N of M" copy.
    pub total_in_bucket: u64,
    /// True iff `total_in_bucket > samples.len()` after the cap was
    /// applied. Convenience flag so the UI doesn't have to compare
    /// the two counts itself.
    pub truncated: bool,
}

/// Compute the drilldown for `bucket` against `rules` + `samples`.
///
/// Cost is `O(rules.len() * samples.len())` — the same shape as
/// [`compute_coverage`]; we walk every sample through the chain to
/// determine its winner. We don't reuse a coverage report because
/// the bucket assignment isn't carried in the coverage shape (only
/// counts are), and rebuilding the winners is cheap enough that two
/// passes is simpler than caching a winners-vec.
///
/// `preview_cap` is the maximum number of samples to copy into the
/// result; the function clamps it to `[1, 5000]` to bound the IPC
/// payload (5000 ≈ a generous full-screen file list — anything more
/// is paging territory, not preview territory).
///
/// An out-of-range `Rule { index }` (greater than or equal to
/// `rules.len()`) yields an empty drilldown — the caller's invariant
/// to keep `index` in range; returning empty instead of panicking
/// matches the rest of the analyzer's lenient stance.
pub fn compute_sample_drilldown(
    rules: &[Rule],
    samples: &[RuleSample],
    bucket: SampleBucket,
    preview_cap: usize,
) -> SampleDrilldown {
    let cap = preview_cap.clamp(1, 5_000);
    let n_rules = rules.len();

    // Resolve out-of-range rule index to an empty bucket up front so
    // we don't waste a scan.
    if let SampleBucket::Rule { index } = bucket {
        if index >= n_rules {
            return SampleDrilldown {
                bucket,
                samples: Vec::new(),
                total_in_bucket: 0,
                truncated: false,
            };
        }
    }

    let mut hits: Vec<&RuleSample> = Vec::new();
    let mut total: u64 = 0;
    for sample in samples {
        let ctx = sample.as_context();
        let winner: Option<usize> = (0..n_rules).find(|&i| rules[i].predicate.matches(&ctx));
        let in_bucket = match (bucket, winner) {
            (SampleBucket::Rule { index }, Some(w)) => w == index,
            (SampleBucket::Fallthrough, None) => true,
            _ => false,
        };
        if in_bucket {
            total += 1;
            if hits.len() < cap {
                hits.push(sample);
            }
        }
    }

    let truncated = total > hits.len() as u64;
    SampleDrilldown {
        bucket,
        samples: hits.into_iter().cloned().collect(),
        total_in_bucket: total,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::hopper::rules::{RuleAction, RulePredicate};

    fn rule(name: &str, predicate: RulePredicate) -> Rule {
        Rule {
            name: name.into(),
            predicate,
            action: RuleAction::default(),
        }
    }

    fn sample(filename: &str) -> RuleSample {
        RuleSample {
            filename: filename.into(),
            size_bytes: 0,
            page_count: None,
            text_sample: None,
        }
    }

    fn samples(filenames: &[&str]) -> Vec<RuleSample> {
        filenames.iter().map(|f| sample(f)).collect()
    }

    // ── Empty inputs ──────────────────────────────────────────────────

    #[test]
    fn empty_rules_returns_all_fallthrough() {
        let report = compute_coverage(&[], &samples(&["a.pdf", "b.pdf", "c.pdf"]));
        assert!(report.rules.is_empty());
        assert_eq!(report.fallthrough, 3);
        assert_eq!(report.total_samples, 3);
    }

    #[test]
    fn empty_samples_returns_all_zero() {
        let rules = vec![
            rule(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
            rule("Catch all", RulePredicate::Always),
        ];
        let report = compute_coverage(&rules, &[]);
        assert_eq!(report.total_samples, 0);
        assert_eq!(report.fallthrough, 0);
        assert_eq!(report.rules.len(), 2);
        for r in report.rules {
            assert_eq!(r.first_match, 0);
            assert_eq!(r.would_match, 0);
            assert!(!r.dead_at_position);
        }
    }

    #[test]
    fn empty_rules_and_samples_returns_zero() {
        let report = compute_coverage(&[], &[]);
        assert!(report.rules.is_empty());
        assert_eq!(report.fallthrough, 0);
        assert_eq!(report.total_samples, 0);
    }

    // ── Single-rule chains ────────────────────────────────────────────

    #[test]
    fn single_rule_counts_first_and_would_equal() {
        let rules = vec![rule(
            "Tax",
            RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
        )];
        let report = compute_coverage(
            &rules,
            &samples(&["tax_2025.pdf", "tax_2026.pdf", "invoice.pdf"]),
        );
        assert_eq!(report.rules.len(), 1);
        let r = &report.rules[0];
        assert_eq!(r.index, 0);
        assert_eq!(r.name, "Tax");
        assert_eq!(r.first_match, 2);
        assert_eq!(r.would_match, 2);
        assert!(!r.dead_at_position);
        assert_eq!(report.fallthrough, 1);
        assert_eq!(report.total_samples, 3);
    }

    #[test]
    fn always_rule_first_match_equals_sample_count() {
        let rules = vec![rule("Catch all", RulePredicate::Always)];
        let report = compute_coverage(&rules, &samples(&["a.pdf", "b.pdf", "c.pdf"]));
        assert_eq!(report.rules[0].first_match, 3);
        assert_eq!(report.rules[0].would_match, 3);
        assert_eq!(report.fallthrough, 0);
    }

    // ── Two-rule chains: first-match wins ─────────────────────────────

    #[test]
    fn first_match_wins_over_later_match() {
        let rules = vec![
            rule(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
            rule("Anything", RulePredicate::Always),
        ];
        let report = compute_coverage(
            &rules,
            &samples(&["tax_2025.pdf", "tax_2026.pdf", "invoice.pdf", "receipt.pdf"]),
        );
        // Tax catches 2 (first); Always catches the other 2 (first);
        // would_match for Always is 4 (it matches everything).
        assert_eq!(report.rules[0].first_match, 2);
        assert_eq!(report.rules[0].would_match, 2);
        assert_eq!(report.rules[1].first_match, 2);
        assert_eq!(report.rules[1].would_match, 4);
        assert_eq!(report.fallthrough, 0);
    }

    // ── Shadowed (dead-at-position) rules ─────────────────────────────

    #[test]
    fn fully_shadowed_rule_is_dead_at_position() {
        // Always is in slot 0, so the Tax rule in slot 1 never fires.
        let rules = vec![
            rule("Catch all first", RulePredicate::Always),
            rule(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
        ];
        let report = compute_coverage(&rules, &samples(&["tax_2026.pdf", "invoice.pdf"]));
        // Always wins every sample.
        assert_eq!(report.rules[0].first_match, 2);
        assert_eq!(report.rules[0].would_match, 2);
        assert!(!report.rules[0].dead_at_position);
        // Tax never wins but would catch 1 in isolation -> dead at position.
        assert_eq!(report.rules[1].first_match, 0);
        assert_eq!(report.rules[1].would_match, 1);
        assert!(report.rules[1].dead_at_position);
    }

    #[test]
    fn partially_shadowed_rule_is_not_dead_at_position() {
        // Tax catches tax_*; Invoice catches invoice_* — disjoint.
        // Both fire; neither is shadowed.
        let rules = vec![
            rule(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
            rule(
                "Invoice",
                RulePredicate::FilenameGlob {
                    pattern: "invoice_*.pdf".into(),
                },
            ),
        ];
        let report = compute_coverage(
            &rules,
            &samples(&["tax_2026.pdf", "invoice_42.pdf", "other.pdf"]),
        );
        assert_eq!(report.rules[0].first_match, 1);
        assert_eq!(report.rules[0].would_match, 1);
        assert!(!report.rules[0].dead_at_position);
        assert_eq!(report.rules[1].first_match, 1);
        assert_eq!(report.rules[1].would_match, 1);
        assert!(!report.rules[1].dead_at_position);
        assert_eq!(report.fallthrough, 1);
    }

    #[test]
    fn zero_coverage_rule_is_not_dead_at_position() {
        // A predicate that matches nothing in the sample set should NOT
        // be flagged dead-at-position; it's just too narrow.
        let rules = vec![rule(
            "Never",
            RulePredicate::FilenameGlob {
                pattern: "this-string-matches-nothing.pdf".into(),
            },
        )];
        let report = compute_coverage(&rules, &samples(&["a.pdf", "b.pdf"]));
        assert_eq!(report.rules[0].first_match, 0);
        assert_eq!(report.rules[0].would_match, 0);
        assert!(!report.rules[0].dead_at_position);
        assert_eq!(report.fallthrough, 2);
    }

    // ── Conservation: sums equal totals ───────────────────────────────

    #[test]
    fn first_match_sum_plus_fallthrough_equals_total() {
        let rules = vec![
            rule(
                "PDFs",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
            rule("Anything", RulePredicate::Always),
        ];
        let report = compute_coverage(
            &rules,
            &samples(&["a.pdf", "b.pdf", "c.txt", "d.pdf", "e.docx"]),
        );
        let first_sum: u64 = report.rules.iter().map(|r| r.first_match).sum();
        assert_eq!(first_sum + report.fallthrough, report.total_samples);
    }

    // ── Predicate axes (smoke that the report wires the predicate path)

    #[test]
    fn page_count_predicate_counts_pages() {
        let rules = vec![rule(
            "Single page",
            RulePredicate::PageCountBetween { min: 1, max: 1 },
        )];
        let s1 = RuleSample {
            filename: "a.pdf".into(),
            size_bytes: 0,
            page_count: Some(1),
            text_sample: None,
        };
        let s2 = RuleSample {
            filename: "b.pdf".into(),
            size_bytes: 0,
            page_count: Some(2),
            text_sample: None,
        };
        let s3 = RuleSample {
            filename: "c.pdf".into(),
            size_bytes: 0,
            page_count: None,
            text_sample: None,
        };
        let report = compute_coverage(&rules, &[s1, s2, s3]);
        assert_eq!(report.rules[0].first_match, 1);
        assert_eq!(report.rules[0].would_match, 1);
        assert_eq!(report.fallthrough, 2);
    }

    #[test]
    fn size_over_predicate_counts_big_files() {
        let rules = vec![rule(
            "Big scans",
            RulePredicate::SizeOver { bytes: 1_000_000 },
        )];
        let make = |size: u64| RuleSample {
            filename: "x.pdf".into(),
            size_bytes: size,
            page_count: None,
            text_sample: None,
        };
        let report = compute_coverage(&rules, &[make(500_000), make(2_000_000), make(1_500_000)]);
        assert_eq!(report.rules[0].first_match, 2);
        assert_eq!(report.fallthrough, 1);
    }

    #[test]
    fn text_contains_all_predicate_uses_text_sample() {
        let rules = vec![rule(
            "Receipts",
            RulePredicate::TextContainsAll {
                needles: vec!["receipt".into(), "total".into()],
            },
        )];
        let s1 = RuleSample {
            filename: "a.pdf".into(),
            size_bytes: 0,
            page_count: None,
            text_sample: Some("receipt total $12".into()),
        };
        let s2 = RuleSample {
            filename: "b.pdf".into(),
            size_bytes: 0,
            page_count: None,
            text_sample: Some("just receipt".into()),
        };
        let s3 = RuleSample {
            filename: "c.pdf".into(),
            size_bytes: 0,
            page_count: None,
            text_sample: None,
        };
        let report = compute_coverage(&rules, &[s1, s2, s3]);
        assert_eq!(report.rules[0].first_match, 1);
        assert_eq!(report.fallthrough, 2);
    }

    // ── Serde wire smoke ──────────────────────────────────────────────

    #[test]
    fn serde_round_trip_keeps_field_names() {
        let report = RuleCoverageReport {
            rules: vec![RuleCoverage {
                index: 0,
                name: "Tax".into(),
                first_match: 3,
                would_match: 5,
                dead_at_position: false,
            }],
            fallthrough: 7,
            total_samples: 10,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"first_match\":3"));
        assert!(json.contains("\"would_match\":5"));
        assert!(json.contains("\"dead_at_position\":false"));
        assert!(json.contains("\"fallthrough\":7"));
        assert!(json.contains("\"total_samples\":10"));

        let back: RuleCoverageReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn rule_sample_serde_defaults_are_lenient() {
        // Frontends should be able to send a minimal payload with just
        // filename; the missing fields default to zero / None.
        let json = r#"{"filename": "x.pdf"}"#;
        let s: RuleSample = serde_json::from_str(json).unwrap();
        assert_eq!(s.filename, "x.pdf");
        assert_eq!(s.size_bytes, 0);
        assert!(s.page_count.is_none());
        assert!(s.text_sample.is_none());
    }

    // ── Slice 83 — sample drilldown primitive ─────────────────────────

    #[test]
    fn drilldown_rule_bucket_returns_first_match_samples() {
        let rules = vec![
            rule(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
            rule("Always", RulePredicate::Always),
        ];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["tax_2025.pdf", "tax_2026.pdf", "invoice.pdf"]),
            SampleBucket::Rule { index: 0 },
            10,
        );
        assert_eq!(drill.total_in_bucket, 2);
        assert_eq!(drill.samples.len(), 2);
        assert_eq!(drill.samples[0].filename, "tax_2025.pdf");
        assert_eq!(drill.samples[1].filename, "tax_2026.pdf");
        assert!(!drill.truncated);
        assert_eq!(drill.bucket, SampleBucket::Rule { index: 0 });
    }

    #[test]
    fn drilldown_fallthrough_bucket_returns_unmatched_samples() {
        let rules = vec![rule(
            "Tax",
            RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
        )];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["tax_2025.pdf", "invoice.pdf", "receipt.pdf"]),
            SampleBucket::Fallthrough,
            10,
        );
        assert_eq!(drill.total_in_bucket, 2);
        assert_eq!(drill.samples.len(), 2);
        assert_eq!(drill.samples[0].filename, "invoice.pdf");
        assert_eq!(drill.samples[1].filename, "receipt.pdf");
        assert!(!drill.truncated);
    }

    #[test]
    fn drilldown_shadowed_rule_returns_empty_first_match_bucket() {
        // Always wins everything so the Tax rule's first_match bucket
        // is empty even though it would match `tax_*.pdf` in isolation.
        let rules = vec![
            rule("Always", RulePredicate::Always),
            rule(
                "Tax (shadowed)",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
        ];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["tax_2025.pdf", "invoice.pdf"]),
            SampleBucket::Rule { index: 1 },
            10,
        );
        assert_eq!(drill.total_in_bucket, 0);
        assert!(drill.samples.is_empty());
        assert!(!drill.truncated);
    }

    #[test]
    fn drilldown_caps_samples_and_sets_truncated_flag() {
        let rules = vec![rule("Always", RulePredicate::Always)];
        let many: Vec<RuleSample> = (0..50).map(|i| sample(&format!("f{i}.pdf"))).collect();
        let drill = compute_sample_drilldown(&rules, &many, SampleBucket::Rule { index: 0 }, 5);
        assert_eq!(drill.total_in_bucket, 50);
        assert_eq!(drill.samples.len(), 5);
        assert_eq!(drill.samples[0].filename, "f0.pdf");
        assert_eq!(drill.samples[4].filename, "f4.pdf");
        assert!(drill.truncated);
    }

    #[test]
    fn drilldown_preview_cap_zero_clamps_to_one() {
        // The cap floor is 1 so a caller can't accidentally ask for
        // zero (which would return an always-empty bucket and look
        // like a bug at the call site).
        let rules = vec![rule("Always", RulePredicate::Always)];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["a.pdf", "b.pdf"]),
            SampleBucket::Rule { index: 0 },
            0,
        );
        assert_eq!(drill.total_in_bucket, 2);
        assert_eq!(drill.samples.len(), 1);
        assert!(drill.truncated);
    }

    #[test]
    fn drilldown_preview_cap_above_ceiling_clamps_to_5000() {
        // The cap ceiling is 5000 so a caller asking for usize::MAX
        // doesn't try to copy the entire input vec.
        let rules = vec![rule("Always", RulePredicate::Always)];
        let many: Vec<RuleSample> = (0..10).map(|i| sample(&format!("f{i}.pdf"))).collect();
        let drill =
            compute_sample_drilldown(&rules, &many, SampleBucket::Rule { index: 0 }, usize::MAX);
        // The clamp ceiling is 5000 but we only had 10 inputs so the
        // actual returned count is 10 (clamp doesn't inflate).
        assert_eq!(drill.total_in_bucket, 10);
        assert_eq!(drill.samples.len(), 10);
        assert!(!drill.truncated);
    }

    #[test]
    fn drilldown_out_of_range_rule_index_returns_empty() {
        // Caller is supposed to pass an index in [0, rules.len()) but
        // we return empty (not panic) on misuse — matches the rest
        // of the analyzer's lenient stance.
        let rules = vec![rule("Always", RulePredicate::Always)];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["a.pdf", "b.pdf"]),
            SampleBucket::Rule { index: 99 },
            10,
        );
        assert_eq!(drill.total_in_bucket, 0);
        assert!(drill.samples.is_empty());
        assert!(!drill.truncated);
        assert_eq!(drill.bucket, SampleBucket::Rule { index: 99 });
    }

    #[test]
    fn drilldown_fallthrough_with_no_rules_returns_all_samples() {
        // No rules => every sample falls through; the bucket holds all.
        let drill = compute_sample_drilldown(
            &[],
            &samples(&["a.pdf", "b.pdf", "c.pdf"]),
            SampleBucket::Fallthrough,
            10,
        );
        assert_eq!(drill.total_in_bucket, 3);
        assert_eq!(drill.samples.len(), 3);
    }

    #[test]
    fn drilldown_fallthrough_with_only_always_rule_is_empty() {
        // An Always rule catches everything, so the fall-through
        // bucket is empty even though there are matching samples.
        let rules = vec![rule("Always", RulePredicate::Always)];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["a.pdf", "b.pdf"]),
            SampleBucket::Fallthrough,
            10,
        );
        assert_eq!(drill.total_in_bucket, 0);
        assert!(drill.samples.is_empty());
    }

    #[test]
    fn drilldown_preserves_input_order() {
        // Input order is preserved so the UI shows samples in the
        // order the caller supplied them (typically newest-first from
        // the run log). Mix matching + non-matching to confirm we
        // don't reshuffle by predicate evaluation order.
        let rules = vec![rule(
            "Tax",
            RulePredicate::FilenameGlob {
                pattern: "tax_*.pdf".into(),
            },
        )];
        let drill = compute_sample_drilldown(
            &rules,
            &samples(&["tax_z.pdf", "invoice.pdf", "tax_a.pdf", "tax_m.pdf"]),
            SampleBucket::Rule { index: 0 },
            10,
        );
        assert_eq!(
            drill
                .samples
                .iter()
                .map(|s| s.filename.as_str())
                .collect::<Vec<_>>(),
            vec!["tax_z.pdf", "tax_a.pdf", "tax_m.pdf"],
        );
    }

    #[test]
    fn drilldown_with_empty_samples_returns_empty_bucket() {
        let rules = vec![rule("Always", RulePredicate::Always)];
        let drill = compute_sample_drilldown(&rules, &[], SampleBucket::Rule { index: 0 }, 10);
        assert_eq!(drill.total_in_bucket, 0);
        assert!(drill.samples.is_empty());
        assert!(!drill.truncated);
    }

    #[test]
    fn drilldown_carries_full_sample_axes() {
        // The bucket samples copy the full RuleSample (filename,
        // size, page count, text). Confirms we don't drop the
        // size/page/text axes during the bucket copy.
        let rules = vec![rule("Big", RulePredicate::SizeOver { bytes: 1_000 })];
        let s = RuleSample {
            filename: "big.pdf".into(),
            size_bytes: 5_000,
            page_count: Some(10),
            text_sample: Some("hello".into()),
        };
        let drill = compute_sample_drilldown(
            &rules,
            std::slice::from_ref(&s),
            SampleBucket::Rule { index: 0 },
            10,
        );
        assert_eq!(drill.samples.len(), 1);
        assert_eq!(drill.samples[0].size_bytes, 5_000);
        assert_eq!(drill.samples[0].page_count, Some(10));
        assert_eq!(drill.samples[0].text_sample.as_deref(), Some("hello"));
    }

    // ── Slice 83 — SampleBucket serde wire shape ──────────────────────

    #[test]
    fn sample_bucket_serde_rule_round_trip() {
        let b = SampleBucket::Rule { index: 3 };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("\"kind\":\"rule\""));
        assert!(json.contains("\"index\":3"));
        let back: SampleBucket = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn sample_bucket_serde_fallthrough_round_trip() {
        let b = SampleBucket::Fallthrough;
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, "{\"kind\":\"fallthrough\"}");
        let back: SampleBucket = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn sample_drilldown_serde_round_trip() {
        let d = SampleDrilldown {
            bucket: SampleBucket::Fallthrough,
            samples: vec![RuleSample {
                filename: "x.pdf".into(),
                size_bytes: 0,
                page_count: None,
                text_sample: None,
            }],
            total_in_bucket: 7,
            truncated: true,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"total_in_bucket\":7"));
        assert!(json.contains("\"truncated\":true"));
        let back: SampleDrilldown = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
