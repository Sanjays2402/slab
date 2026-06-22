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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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

// ─── Slice 88 — drilldown CSV export primitive ───────────────────────
//
// Pure-data serialiser turning a [`SampleDrilldown`] into RFC-4180
// CSV so the round-18 drilldown popover can offer "Save this list as
// CSV…" — paralegals click into the fall-through bucket, see 23
// orphaned files, then need to email that list to a partner before
// tightening the rules. Mirrors `backfill_report_to_csv` (slice 13
// audit-trail export) so paralegals see ONE consistent CSV idiom
// across the audit surfaces.
//
// Columns:
//   filename, size_bytes, page_count, text_sample, bucket_kind, bucket_name
//
// `bucket_kind` is the discriminator (`"fallthrough"` / `"rule"`)
// matching the [`SampleBucket`] serde tag, so a script reading the
// CSV can re-derive the bucket without guessing.
//
// `bucket_name` is the human label — `"Fall-through"` for the
// catch-all bucket, the rule's display name (or `"Rule #N"` 1-based
// fallback when the name is missing/blank) for rule buckets. Same
// vocabulary as the TS `describeBucket` helper so the CSV reads like
// the popover header reads.
//
// Header is opt-in (mirror backfill's signature) so an export that
// appends to an existing audit log can suppress it.

const SAMPLE_DRILLDOWN_CSV_HEADER: &str =
    "filename,size_bytes,page_count,text_sample,bucket_kind,bucket_name";

/// Render a [`SampleDrilldown`] as RFC-4180-compliant CSV.
///
/// `rule_names` is the parallel name array used to resolve a rule
/// bucket's display label — pass the rule chain's names in input
/// order. The function handles empty/whitespace/out-of-range names
/// with a `Rule #N` (1-based) fallback so the CSV never reads as
/// `,,` with an empty bucket label. Mirrors the TS `describeBucket`
/// helper's fallback chain verbatim.
///
/// Pure function — never touches the filesystem; the Tauri command
/// layer owns disk I/O. Same RFC-4180 escaping policy as
/// [`crate::pdf::hopper::backfill::backfill_report_to_csv`]:
/// fields containing `,`, `"`, `\r`, `\n` are quote-wrapped with
/// embedded quotes doubled.
pub fn sample_drilldown_to_csv(
    drill: &SampleDrilldown,
    rule_names: &[String],
    include_header: bool,
) -> String {
    let mut out = String::new();
    if include_header {
        out.push_str(SAMPLE_DRILLDOWN_CSV_HEADER);
        out.push('\n');
    }
    let (kind, name) = bucket_csv_labels(drill.bucket, rule_names);
    for s in &drill.samples {
        let row = [
            csv_escape(&s.filename),
            s.size_bytes.to_string(),
            s.page_count.map(|n| n.to_string()).unwrap_or_default(),
            csv_escape(s.text_sample.as_deref().unwrap_or("")),
            kind.to_string(),
            csv_escape(&name),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// Resolve the `(bucket_kind, bucket_name)` pair for a bucket plus an
/// optional rule-names array. Same fallback chain as the TS
/// `describeBucket` helper:
///
/// - `Fallthrough` → `("fallthrough", "Fall-through")`.
/// - `Rule { index }` → kind is `"rule"`; name is the trimmed
///   `rule_names[index]` when present + non-empty, else `Rule #N`
///   with `N = index + 1`.
fn bucket_csv_labels(bucket: SampleBucket, rule_names: &[String]) -> (&'static str, String) {
    match bucket {
        SampleBucket::Fallthrough => ("fallthrough", "Fall-through".into()),
        SampleBucket::Rule { index } => {
            let resolved = rule_names
                .get(index)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Rule #{}", index + 1));
            ("rule", resolved)
        }
    }
}

/// RFC-4180 field escape. Same policy as
/// [`crate::pdf::hopper::backfill`]'s private helper; duplicated
/// rather than re-exported so the two CSV emitters stay independent
/// (a future change to one shouldn't silently affect the other).
fn csv_escape(field: &str) -> String {
    let needs_quoting =
        field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r');
    if !needs_quoting {
        return field.to_string();
    }
    let escaped = field.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

// ─── Slice 93 — drilldown JSON export envelope ───────────────────────
//
// Mirrors the install-log `InstallLogExportEnvelope` shape (slice 60)
// so paralegals get ONE consistent audit-export idiom across surfaces:
// a self-describing envelope with `schema_version` + `generated_at_iso`
// + bucket metadata + the samples themselves. The CSV emitter from
// slice 88 covers the spreadsheet path; this envelope covers the
// "feed it to a downstream pipeline / archive it as a record" path,
// where the consumer benefits from explicit schema versioning and a
// generated-at stamp that doesn't need a second cover-letter document.
//
// Why a separate primitive vs `serde_json::to_string_pretty` over the
// raw `SampleDrilldown`:
//
// 1. The envelope precomputes `bucket_kind` / `bucket_name` via the
//    same `bucket_csv_labels` chain the CSV uses, so the JSON and CSV
//    exports agree exactly on the bucket label. A downstream script
//    that reads either format never has to re-derive the name.
// 2. A bare `SampleDrilldown` JSON has no provenance — a consumer
//    reading the file two years later can't tell when it was
//    generated, what schema version it follows, or what bucket it
//    was filtered to without looking at the filename.
// 3. The schema-version field gives us a forward-compatibility hook:
//    a future shape change (e.g. adding rule predicate JSON to the
//    envelope) bumps the version, and v1 consumers can skip the
//    unknown sections gracefully.

/// Schema version of the drilldown JSON export envelope. Bumped on
/// non-additive shape changes only — adding a new optional field is
/// backward-compatible at v1. Matches the install-log export's
/// `INSTALL_LOG_EXPORT_SCHEMA_VERSION = 1` convention so a downstream
/// reader can recognise "Slab audit export v1" across both
/// envelopes without checking which surface produced it.
pub const DRILLDOWN_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Wire shape for the JSON drilldown export. Lifts the raw
/// [`SampleDrilldown`] into a self-describing envelope so a downstream
/// pipeline reading the file at archive-recovery time knows what
/// schema version it's looking at, when it was generated, which
/// bucket the export was filtered to (label + discriminator), and
/// how many samples are in the bucket vs in this slice of it.
///
/// The envelope is what `slab_hopper_export_drilldown_json` (slice 94)
/// writes to disk. It deliberately mirrors the install-log
/// `InstallLogExportEnvelope` shape (`schema_version` +
/// `generated_at_iso` + filter-context fields + body) so paralegals
/// and downstream scripts see ONE consistent envelope across the two
/// audit surfaces.
///
/// Bucket identity is captured TWICE:
///
/// - `bucket` carries the raw [`SampleBucket`] (kind-discriminated)
///   so a script can pattern-match on `kind === "rule" | "fallthrough"`
///   without parsing the human label.
/// - `bucket_kind` + `bucket_name` carry the same `(kind, name)` pair
///   the CSV emits, precomputed via the same fallback chain
///   (`Rule #N` 1-based when names are missing/blank/out-of-range)
///   so the JSON and CSV exports agree on labels exactly.
///
/// Sample count is also captured TWICE for the same reason: the
/// `sample_count` field is the row count actually carried in
/// `samples` (post-cap, matches `samples.len()`), and
/// `total_in_bucket` is the FULL bucket size pre-cap (matches the
/// underlying [`SampleDrilldown`]). A consumer can detect truncation
/// by comparing the two, or by reading the explicit `truncated`
/// flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DrilldownExportEnvelope {
    /// Schema version of the envelope itself (NOT of the hopper
    /// run-log schema). Bumped on non-additive shape changes.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of when the export was produced
    /// (`"2026-06-21T22:14:07Z"`).
    pub generated_at_iso: String,
    /// The raw bucket selector — `{"kind":"rule","index":1}` or
    /// `{"kind":"fallthrough"}`. Same shape as
    /// [`SampleBucket`]'s serde representation.
    pub bucket: SampleBucket,
    /// Discriminator string matching `bucket.kind` —
    /// `"fallthrough"` or `"rule"`. Redundant with `bucket.kind` but
    /// saves a consumer from having to descend the nested object.
    pub bucket_kind: String,
    /// Human label for the bucket — `"Fall-through"` for the
    /// catch-all, the rule's display name (or `Rule #N` 1-based
    /// fallback when missing/blank/out-of-range) for rule buckets.
    /// Same fallback chain as `bucket_csv_labels`.
    pub bucket_name: String,
    /// The number of samples in `samples`. Redundant with
    /// `samples.len()` but cheap and saves consumers a parse step.
    pub sample_count: usize,
    /// FULL bucket size pre-cap — matches the underlying
    /// [`SampleDrilldown::total_in_bucket`]. Equals `sample_count`
    /// when the export wasn't truncated.
    pub total_in_bucket: u64,
    /// True iff `total_in_bucket > sample_count`. Convenience flag
    /// so the consumer doesn't have to compare the two counts itself.
    pub truncated: bool,
    /// The samples themselves — each row is a verbatim [`RuleSample`]
    /// (filename + size + page count + text sample). Order matches
    /// the input order, which is typically newest-first when the
    /// drilldown was sourced from the run log.
    pub samples: Vec<RuleSample>,
}

/// Build the JSON export envelope from a [`SampleDrilldown`] + the
/// parallel rule-names array. The envelope's `generated_at_iso`
/// stamp uses the wall clock at call time; tests pass a fixed
/// timestamp via [`sample_drilldown_to_json_with_now`].
pub fn sample_drilldown_to_json(
    drill: &SampleDrilldown,
    rule_names: &[String],
) -> DrilldownExportEnvelope {
    sample_drilldown_to_json_with_now(drill, rule_names, drilldown_unix_now())
}

/// Same as [`sample_drilldown_to_json`] but takes an explicit
/// unix-seconds "now" so unit tests don't race the wall clock.
pub fn sample_drilldown_to_json_with_now(
    drill: &SampleDrilldown,
    rule_names: &[String],
    now_unix: i64,
) -> DrilldownExportEnvelope {
    let (kind, name) = bucket_csv_labels(drill.bucket, rule_names);
    DrilldownExportEnvelope {
        schema_version: DRILLDOWN_EXPORT_SCHEMA_VERSION,
        generated_at_iso: drilldown_iso8601_utc(now_unix),
        bucket: drill.bucket,
        bucket_kind: kind.to_string(),
        bucket_name: name,
        sample_count: drill.samples.len(),
        total_in_bucket: drill.total_in_bucket,
        truncated: drill.truncated,
        samples: drill.samples.clone(),
    }
}

/// Wall-clock unix-seconds. Duplicated from
/// [`crate::marketplace::install_log`]'s private helper so the
/// hopper coverage module doesn't take a cross-subsystem dep.
fn drilldown_unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Render a unix-seconds value as a canonical ISO-8601 UTC string
/// (`"2026-06-21T22:14:07Z"`). Used as the envelope's
/// `generated_at_iso` field. Falls back to the empty string for the
/// pathological case where the value can't be represented — keeps
/// the field shape consistent.
fn drilldown_iso8601_utc(unix_seconds: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix_seconds, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
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

    // ── Slice 88 — sample_drilldown_to_csv ───────────────────────────

    fn drill(bucket: SampleBucket, samples: Vec<RuleSample>) -> SampleDrilldown {
        let n = samples.len() as u64;
        SampleDrilldown {
            bucket,
            samples,
            total_in_bucket: n,
            truncated: false,
        }
    }

    #[test]
    fn drilldown_csv_header_when_requested() {
        let d = drill(SampleBucket::Fallthrough, vec![]);
        let with_header = sample_drilldown_to_csv(&d, &[], true);
        assert_eq!(
            with_header.trim(),
            "filename,size_bytes,page_count,text_sample,bucket_kind,bucket_name"
        );
        let without = sample_drilldown_to_csv(&d, &[], false);
        assert!(without.is_empty(), "bare empty drilldown emits nothing");
    }

    #[test]
    fn drilldown_csv_fallthrough_bucket_renders_label() {
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "orphan.pdf".into(),
                size_bytes: 1024,
                page_count: Some(3),
                text_sample: None,
                ..Default::default()
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &[], false);
        // No header → one data row.
        let lines: Vec<&str> = csv.trim().lines().collect();
        assert_eq!(lines.len(), 1);
        let cols: Vec<&str> = lines[0].split(',').collect();
        assert_eq!(cols[0], "orphan.pdf");
        assert_eq!(cols[1], "1024");
        assert_eq!(cols[2], "3");
        assert_eq!(cols[3], ""); // no text sample
        assert_eq!(cols[4], "fallthrough");
        assert_eq!(cols[5], "Fall-through");
    }

    #[test]
    fn drilldown_csv_rule_bucket_uses_rule_name() {
        let d = drill(
            SampleBucket::Rule { index: 1 },
            vec![RuleSample {
                filename: "tax_2025.pdf".into(),
                ..Default::default()
            }],
        );
        let names = vec!["Invoices".to_string(), "Tax forms".to_string()];
        let csv = sample_drilldown_to_csv(&d, &names, false);
        let cols: Vec<&str> = csv.trim().split(',').collect();
        assert_eq!(cols[4], "rule");
        assert_eq!(cols[5], "Tax forms");
    }

    #[test]
    fn drilldown_csv_rule_bucket_falls_back_when_name_missing() {
        // Empty names array → "Rule #N" (1-based) per the UI convention
        // mirroring describeBucket in hopper.ts.
        let d = drill(
            SampleBucket::Rule { index: 2 },
            vec![RuleSample {
                filename: "a.pdf".into(),
                ..Default::default()
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &[], false);
        let cols: Vec<&str> = csv.trim().split(',').collect();
        assert_eq!(cols[5], "Rule #3");
    }

    #[test]
    fn drilldown_csv_rule_bucket_falls_back_when_name_blank() {
        // Whitespace-only / empty name → same "Rule #N" fallback so the
        // CSV never reads as ",rule,," with a missing bucket label.
        let d = drill(
            SampleBucket::Rule { index: 0 },
            vec![RuleSample {
                filename: "a.pdf".into(),
                ..Default::default()
            }],
        );
        for name in ["", "   "] {
            let names = vec![name.to_string()];
            let csv = sample_drilldown_to_csv(&d, &names, false);
            let cols: Vec<&str> = csv.trim().split(',').collect();
            assert_eq!(cols[5], "Rule #1", "blank name {:?} -> Rule #1", name);
        }
    }

    #[test]
    fn drilldown_csv_rule_bucket_out_of_range_falls_back() {
        // Out-of-range index (more rules in the bucket index than the
        // names array — possible if the UI passes a stale name list).
        let d = drill(
            SampleBucket::Rule { index: 99 },
            vec![RuleSample {
                filename: "a.pdf".into(),
                ..Default::default()
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &["Tax".to_string()], false);
        let cols: Vec<&str> = csv.trim().split(',').collect();
        assert_eq!(cols[5], "Rule #100");
    }

    #[test]
    fn drilldown_csv_escapes_commas_and_quotes_in_filename() {
        // RFC-4180 escaping — same convention as backfill_report_to_csv.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "weird, \"file\".pdf".into(),
                ..Default::default()
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &[], false);
        // The field gets wrapped in quotes; embedded quotes doubled.
        assert!(csv.contains("\"weird, \"\"file\"\".pdf\""));
    }

    #[test]
    fn drilldown_csv_escapes_newlines_in_text_sample() {
        // Text samples may carry embedded newlines from PDF extraction;
        // the field has to be quote-wrapped so the CSV stays parsable.
        let d = drill(
            SampleBucket::Rule { index: 0 },
            vec![RuleSample {
                filename: "a.pdf".into(),
                text_sample: Some("first line\nsecond line".into()),
                ..Default::default()
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &["A".to_string()], false);
        assert!(csv.contains("\"first line\nsecond line\""));
    }

    #[test]
    fn drilldown_csv_omits_optional_columns_when_none() {
        // Log-sourced samples have size=0 + page_count=None + text=None.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "log_only.pdf".into(),
                size_bytes: 0,
                page_count: None,
                text_sample: None,
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &[], false);
        let cols: Vec<&str> = csv.trim().split(',').collect();
        assert_eq!(cols[1], "0");
        assert_eq!(cols[2], ""); // page_count None → empty cell
        assert_eq!(cols[3], ""); // text_sample None → empty cell
    }

    #[test]
    fn drilldown_csv_empty_samples_with_header_emits_header_only() {
        let d = drill(SampleBucket::Fallthrough, vec![]);
        let csv = sample_drilldown_to_csv(&d, &[], true);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("filename,"));
    }

    #[test]
    fn drilldown_csv_preserves_input_order() {
        // Order in the CSV matches the order in `drill.samples` — the
        // UI lists samples newest-first from the run log, and the export
        // should mirror exactly what the user saw on screen.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![
                RuleSample {
                    filename: "c.pdf".into(),
                    ..Default::default()
                },
                RuleSample {
                    filename: "a.pdf".into(),
                    ..Default::default()
                },
                RuleSample {
                    filename: "b.pdf".into(),
                    ..Default::default()
                },
            ],
        );
        let csv = sample_drilldown_to_csv(&d, &[], false);
        let first_col: Vec<&str> = csv
            .trim()
            .lines()
            .map(|l| l.split(',').next().unwrap())
            .collect();
        assert_eq!(first_col, vec!["c.pdf", "a.pdf", "b.pdf"]);
    }

    #[test]
    fn drilldown_csv_unicode_filename_passes_through_when_safe() {
        // Non-ASCII filenames don't need quoting unless they contain
        // CSV special chars — mirrors backfill_report_to_csv behaviour.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "café.pdf".into(),
                ..Default::default()
            }],
        );
        let csv = sample_drilldown_to_csv(&d, &[], false);
        let cols: Vec<&str> = csv.trim().split(',').collect();
        assert_eq!(cols[0], "café.pdf");
    }

    #[test]
    fn drilldown_csv_row_count_matches_samples_not_total() {
        // total_in_bucket reports the FULL bucket size (pre-cap), but
        // the CSV only emits rows for the samples actually carried in
        // the drilldown (cap-trimmed). The truncation footnote belongs
        // on the UI / the toast, not in the CSV.
        let d = SampleDrilldown {
            bucket: SampleBucket::Fallthrough,
            samples: vec![
                RuleSample {
                    filename: "a.pdf".into(),
                    ..Default::default()
                },
                RuleSample {
                    filename: "b.pdf".into(),
                    ..Default::default()
                },
            ],
            total_in_bucket: 47,
            truncated: true,
        };
        let csv = sample_drilldown_to_csv(&d, &[], false);
        assert_eq!(csv.trim().lines().count(), 2);
    }

    // ── Slice 93 — sample_drilldown_to_json envelope ─────────────────

    #[test]
    fn drilldown_json_envelope_carries_schema_and_generated_timestamp() {
        // schema_version pins the v1 contract; generated_at_iso uses
        // the explicit-now form so tests don't race the wall clock.
        // 1_710_000_000 is 2024-03-09T16:00:00Z.
        let d = drill(SampleBucket::Fallthrough, vec![]);
        let env = sample_drilldown_to_json_with_now(&d, &[], 1_710_000_000);
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.generated_at_iso, "2024-03-09T16:00:00Z");
    }

    #[test]
    fn drilldown_json_envelope_carries_bucket_kind_and_name_for_fallthrough() {
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "orphan.pdf".into(),
                ..Default::default()
            }],
        );
        let env = sample_drilldown_to_json_with_now(&d, &[], 0);
        assert_eq!(env.bucket_kind, "fallthrough");
        assert_eq!(env.bucket_name, "Fall-through");
        assert_eq!(env.bucket, SampleBucket::Fallthrough);
    }

    #[test]
    fn drilldown_json_envelope_resolves_rule_bucket_name() {
        let d = drill(
            SampleBucket::Rule { index: 1 },
            vec![RuleSample {
                filename: "tax.pdf".into(),
                ..Default::default()
            }],
        );
        let names = vec!["Invoices".to_string(), "Tax forms".to_string()];
        let env = sample_drilldown_to_json_with_now(&d, &names, 0);
        assert_eq!(env.bucket_kind, "rule");
        assert_eq!(env.bucket_name, "Tax forms");
        assert_eq!(env.bucket, SampleBucket::Rule { index: 1 });
    }

    #[test]
    fn drilldown_json_envelope_rule_bucket_falls_back_to_rule_n_when_name_missing() {
        // Same describeBucket fallback chain the CSV uses (1-based).
        let d = drill(
            SampleBucket::Rule { index: 2 },
            vec![RuleSample {
                filename: "a.pdf".into(),
                ..Default::default()
            }],
        );
        // Empty names array → "Rule #N".
        let env = sample_drilldown_to_json_with_now(&d, &[], 0);
        assert_eq!(env.bucket_name, "Rule #3");
        // Whitespace-only / blank names also fall back.
        for name in ["", "   "] {
            let names = vec!["".to_string(), "".to_string(), name.to_string()];
            let env = sample_drilldown_to_json_with_now(&d, &names, 0);
            assert_eq!(env.bucket_name, "Rule #3", "blank {:?} -> Rule #3", name);
        }
    }

    #[test]
    fn drilldown_json_envelope_carries_sample_count_and_total_in_bucket() {
        // sample_count == samples.len() (post-cap); total_in_bucket
        // is the FULL bucket size pre-cap. Truncated drilldowns
        // surface BOTH so a consumer can detect truncation.
        let d = SampleDrilldown {
            bucket: SampleBucket::Fallthrough,
            samples: vec![
                RuleSample {
                    filename: "a.pdf".into(),
                    ..Default::default()
                },
                RuleSample {
                    filename: "b.pdf".into(),
                    ..Default::default()
                },
            ],
            total_in_bucket: 47,
            truncated: true,
        };
        let env = sample_drilldown_to_json_with_now(&d, &[], 0);
        assert_eq!(env.sample_count, 2);
        assert_eq!(env.total_in_bucket, 47);
        assert!(env.truncated);
        assert_eq!(env.samples.len(), 2);
    }

    #[test]
    fn drilldown_json_envelope_untruncated_when_sample_count_equals_total() {
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "a.pdf".into(),
                ..Default::default()
            }],
        );
        let env = sample_drilldown_to_json_with_now(&d, &[], 0);
        assert_eq!(env.sample_count, 1);
        assert_eq!(env.total_in_bucket, 1);
        assert!(!env.truncated);
    }

    #[test]
    fn drilldown_json_envelope_empty_drilldown_still_renders() {
        // Empty bucket exports are valid — the envelope tells the
        // consumer the bucket was checked and contained zero samples,
        // which is different from "nobody ran the export".
        let d = drill(SampleBucket::Fallthrough, vec![]);
        let env = sample_drilldown_to_json_with_now(&d, &[], 1_710_000_000);
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.sample_count, 0);
        assert_eq!(env.total_in_bucket, 0);
        assert!(!env.truncated);
        assert!(env.samples.is_empty());
        assert_eq!(env.bucket_kind, "fallthrough");
    }

    #[test]
    fn drilldown_json_envelope_preserves_input_sample_order() {
        // Same ordering invariant as the CSV emitter — the envelope's
        // `samples` array mirrors the drilldown's `samples` order
        // verbatim so the JSON file reads in the same sequence the
        // popover rendered.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![
                RuleSample {
                    filename: "c.pdf".into(),
                    ..Default::default()
                },
                RuleSample {
                    filename: "a.pdf".into(),
                    ..Default::default()
                },
                RuleSample {
                    filename: "b.pdf".into(),
                    ..Default::default()
                },
            ],
        );
        let env = sample_drilldown_to_json_with_now(&d, &[], 0);
        let names: Vec<&str> = env.samples.iter().map(|s| s.filename.as_str()).collect();
        assert_eq!(names, vec!["c.pdf", "a.pdf", "b.pdf"]);
    }

    #[test]
    fn drilldown_json_envelope_preserves_full_sample_axes() {
        // RuleSample has four fields; the envelope must carry every
        // one of them so the JSON consumer never has to cross-reference
        // a separate source for size/page/text data.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                filename: "complete.pdf".into(),
                size_bytes: 12_345,
                page_count: Some(7),
                text_sample: Some("hello world".into()),
            }],
        );
        let env = sample_drilldown_to_json_with_now(&d, &[], 0);
        let s = &env.samples[0];
        assert_eq!(s.filename, "complete.pdf");
        assert_eq!(s.size_bytes, 12_345);
        assert_eq!(s.page_count, Some(7));
        assert_eq!(s.text_sample.as_deref(), Some("hello world"));
    }

    #[test]
    fn drilldown_json_envelope_serializes_full_roundtrip() {
        // Round-trip the entire envelope so a future serde-shape
        // change surfaces here rather than at a downstream consumer.
        let d = drill(
            SampleBucket::Rule { index: 0 },
            vec![RuleSample {
                filename: "a.pdf".into(),
                size_bytes: 100,
                page_count: Some(2),
                text_sample: None,
            }],
        );
        let names = vec!["Receipts".to_string()];
        let env = sample_drilldown_to_json_with_now(&d, &names, 1_710_000_000);
        let json = serde_json::to_string(&env).unwrap();
        let back: DrilldownExportEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
        // Spot-check a few field names in the JSON so a careless rename
        // breaks here.
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"generated_at_iso\":\"2024-03-09T16:00:00Z\""));
        assert!(json.contains("\"bucket_kind\":\"rule\""));
        assert!(json.contains("\"bucket_name\":\"Receipts\""));
        assert!(json.contains("\"sample_count\":1"));
    }

    #[test]
    fn drilldown_json_envelope_pretty_print_is_valid_json() {
        // The Tauri command writes pretty-printed JSON to disk for
        // human readability; this pins that the serialiser doesn't
        // emit anything that breaks JSON.parse on the consumer side.
        let d = drill(
            SampleBucket::Fallthrough,
            vec![RuleSample {
                // A filename with a quote tests escaping survives the
                // pretty-printer too.
                filename: "weird \"name\".pdf".into(),
                ..Default::default()
            }],
        );
        let env = sample_drilldown_to_json_with_now(&d, &[], 1_710_000_000);
        let pretty = serde_json::to_string_pretty(&env).unwrap();
        let back: DrilldownExportEnvelope = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back, env);
        // Pretty form should contain at least one newline.
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn drilldown_json_envelope_iso_helper_handles_bad_timestamp() {
        // The wall-clock helper falls back to 0 on a SystemTime
        // failure; the ISO helper should render a representable
        // timestamp for 0 and an empty string for an out-of-range
        // i64. Tests pin both branches so a future change to the
        // chrono call surfaces.
        let ok = drilldown_iso8601_utc(0);
        assert_eq!(ok, "1970-01-01T00:00:00Z");
        let bad = drilldown_iso8601_utc(i64::MAX);
        assert_eq!(bad, "");
    }

    #[test]
    fn drilldown_json_envelope_kind_string_matches_bucket_serde_tag() {
        // The bucket_kind field MUST match what serde emits for the
        // bucket's tag — otherwise a consumer reading bucket_kind and
        // a consumer reading bucket.kind get different answers.
        let cases = [
            (SampleBucket::Fallthrough, "fallthrough"),
            (SampleBucket::Rule { index: 0 }, "rule"),
        ];
        for (bucket, expected_kind) in cases {
            let d = drill(bucket, vec![]);
            let env = sample_drilldown_to_json_with_now(&d, &[], 0);
            assert_eq!(env.bucket_kind, expected_kind);
            // Round-trip through serde and verify bucket.kind matches.
            let json = serde_json::to_value(env.bucket).unwrap();
            let kind_from_serde = json.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(kind_from_serde, expected_kind);
        }
    }
}
