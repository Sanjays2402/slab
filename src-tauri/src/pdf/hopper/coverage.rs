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

use super::rules::{Rule, RuleContext, RulePredicate};

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

// ─── Slice 123 — rule coverage CSV export primitive ──────────────────
//
// The drilldown popover has shipped CSV + JSON export since round 19
// (slices 88-94), but the parent coverage panel — the surface that
// holds the per-rule first_match / would_match counts plus the
// fall-through count — has no export of its own. A paralegal building
// a 6-rule chain who wants to email "here's the coverage report for
// last 100 runs" to a partner still has to screenshot the panel; the
// drilldown CSV only carries the files in ONE bucket, not the per-
// rule routing decision summary.
//
// This primitive fills the gap with a pure-data RFC-4180 serialiser
// that turns a [`RuleCoverageReport`] into a per-row CSV table. One
// row per rule + one trailing "fall-through" row so the export is a
// complete picture of where every sample went. Same RFC-4180 escape
// policy as [`sample_drilldown_to_csv`] (slice 88) and
// [`crate::pdf::hopper::backfill::backfill_report_to_csv`] (slice 13)
// — paralegals see one consistent CSV idiom across the audit surfaces.
//
// Columns:
//   index          — 0-based rule index, blank on the fall-through row
//   name           — rule display name, "Fall-through" on the synth row
//   first_match    — actual routing volume at this rule's position
//   would_match    — predicate matches in isolation (blank on fall-through)
//   first_match_pct — first_match / total_samples * 100, two decimals
//   diagnostic     — "" / "dead" / "shadowed" / "zero" / "fallthrough"
//
// `first_match_pct` is denormalised onto every row so the consumer
// reads "this rule routed 23.45% of runs" without re-computing the
// ratio against the report's `total_samples`. We render to two
// decimal places so a chain of small percentages (5.12%, 3.84%)
// stays legible without losing precision the panel's rounded
// summary line throws away.
//
// `diagnostic` mirrors the TS [`ruleCoverageDiagnostic`] helper plus
// a synthetic `"fallthrough"` value for the trailing row. A consumer
// loading the CSV into a notebook gets the same chain-health story
// the in-app panel renders, without having to re-derive it from
// raw counts.

/// Header row for [`rule_coverage_to_csv`]. Pub so tests +
/// downstream consumers (a future bulk-export composer) can lock
/// onto a stable column order rather than re-parse the first line.
pub const RULE_COVERAGE_CSV_HEADER: &str =
    "index,name,first_match,would_match,first_match_pct,diagnostic";

/// Render a [`RuleCoverageReport`] as RFC-4180-compliant CSV.
///
/// Emits one row per rule in the report's input order, plus one
/// trailing synthetic row for the fall-through bucket so the export
/// accounts for every sample. Total rows == `report.rules.len() + 1`
/// (the fall-through row is emitted even when its count is zero —
/// "no fall-through" is a real audit signal worth recording).
///
/// `include_header` opt-in matches the sibling exporters
/// ([`sample_drilldown_to_csv`], [`super::backfill::backfill_report_to_csv`])
/// — append-to-existing-audit-log workflows suppress the header so
/// the second export doesn't insert a stray header mid-file.
///
/// Pure function — never touches the filesystem; the Tauri command
/// layer owns disk I/O.
pub fn rule_coverage_to_csv(report: &RuleCoverageReport, include_header: bool) -> String {
    let mut out = String::new();
    if include_header {
        out.push_str(RULE_COVERAGE_CSV_HEADER);
        out.push('\n');
    }
    let total = report.total_samples;
    for r in &report.rules {
        let pct = pct_two_decimal(r.first_match, total);
        let diag = coverage_diagnostic_str(r);
        let row = [
            r.index.to_string(),
            csv_escape(&r.name),
            r.first_match.to_string(),
            r.would_match.to_string(),
            pct,
            diag.to_string(),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    // Synthetic fall-through row. The index column is blank (the
    // fall-through is not a numbered rule), `would_match` is blank
    // (there's no predicate to evaluate in isolation), and the
    // diagnostic is the literal "fallthrough" so a consumer can
    // grep for the bucket without parsing the empty index column.
    let ft_pct = pct_two_decimal(report.fallthrough, total);
    let ft_row = [
        String::new(),
        csv_escape("Fall-through"),
        report.fallthrough.to_string(),
        String::new(),
        ft_pct,
        "fallthrough".to_string(),
    ];
    out.push_str(&ft_row.join(","));
    out.push('\n');
    out
}

/// Render a percentage as a two-decimal string (`"23.45"`).
/// Denominator zero returns `"0.00"` so a report with no samples
/// emits a self-consistent zero column rather than `"NaN"`.
fn pct_two_decimal(numerator: u64, denominator: u64) -> String {
    if denominator == 0 {
        return "0.00".to_string();
    }
    let pct = (numerator as f64) * 100.0 / (denominator as f64);
    format!("{pct:.2}")
}

/// Diagnostic discriminator for one rule's coverage row. Returns
/// `""` when the rule is "healthy" (some samples routed via this
/// rule, no shadowing). Mirrors the TS [`ruleCoverageDiagnostic`]
/// helper's priority chain:
///
/// 1. `dead`     — never wins at this position but would win earlier
/// 2. `zero`     — predicate matches no sample at all
/// 3. `shadowed` — partially shadowed (would_match > first_match)
/// 4. `""`       — healthy
fn coverage_diagnostic_str(rule: &RuleCoverage) -> &'static str {
    if rule.dead_at_position {
        return "dead";
    }
    if rule.would_match == 0 {
        return "zero";
    }
    if rule.would_match > rule.first_match {
        return "shadowed";
    }
    ""
}

// ─── Slice 124 — rule coverage JSON export envelope ──────────────────
//
// Pure-data envelope wrapping a [`RuleCoverageReport`] for the JSON
// export path. CSV (slice 123) carries the per-rule rows in
// spreadsheet-friendly form; JSON carries them in a self-describing
// envelope a downstream pipeline / archived audit record can read
// without consulting the export filename.
//
// Mirrors the canonical envelope shape from the install-log family
// (slices 60, 99, 105, 111, 116, 121): `schema_version` +
// `generated_at_iso` + corpus-scoped invariant totals on the
// envelope + body Vec carrying the per-row data verbatim. The
// envelope-level totals answer the chain-health questions in ONE
// read without re-walking the rows:
//
//   total_samples           — sample-count denominator
//   fallthrough_count       — bucket size for the fall-through
//   fallthrough_pct         — pre-divided percentage (two decimals
//                             rounded, matches the CSV serialiser)
//   rule_count              — N (== `rules.len()`)
//   dead_rule_count         — rules with dead_at_position == true
//   shadowed_rule_count     — rules with `would_match > first_match`
//                             and NOT dead (partial shadow only)
//   zero_coverage_rule_count — rules whose predicate matched zero
//                             samples (would_match == 0); not dead.
//
// The four count fields are envelope-level (one per export) NOT
// per-row properties — they're corpus-scoped invariants of the
// chain run, matching the histogram envelope's `grand_total` and
// the auto-prune-runs envelope's `total_rows_removed` shape
// philosophy.
//
// `rules` carries the per-rule [`RuleCoverage`] rows verbatim in
// input order. A consumer can re-render the panel from the envelope
// alone without a second source.
//
// `fallthrough_pct` is included because the panel header surfaces
// it (round-19 summarizeCoverage) — denormalising it onto the
// envelope keeps the JSON consumer from re-computing a number the
// CSV already pre-computed.

/// Schema version of the rule-coverage JSON export envelope. Starts
/// at v1; bumped independently of the six sibling envelopes
/// (install-log + histogram + activity-timeline + bucket-drilldown +
/// plugin-retention + auto-prune-runs) because the bodies are
/// unrelated. PARALLEL-versioned: a future shape change in one
/// envelope bumps that one only.
pub const RULE_COVERAGE_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Wire shape for the JSON rule-coverage export. Self-describing
/// envelope mirroring the install-log family's shape: schema-version
/// + generated-at + corpus-scoped invariant totals + per-row Vec.
///
/// The four `*_rule_count` totals are derived in ONE pass over the
/// input rows during envelope construction so a consumer reading
/// "this chain has 2 dead rules, 1 shadowed, 0 zero-coverage" doesn't
/// have to re-walk and classify the rows itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleCoverageExportEnvelope {
    /// Schema version of the envelope itself (NOT of the
    /// [`RuleCoverageReport`] body). Bumped on a non-additive shape
    /// change.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of when the export was produced
    /// (`"2026-06-21T22:14:07Z"`).
    pub generated_at_iso: String,
    /// Total samples scanned (matches
    /// [`RuleCoverageReport::total_samples`]).
    pub total_samples: u64,
    /// Samples that matched no rule (matches
    /// [`RuleCoverageReport::fallthrough`]).
    pub fallthrough_count: u64,
    /// `fallthrough_count / total_samples * 100`, two decimals.
    /// Pre-divided so the consumer doesn't re-compute the ratio.
    /// Zero when `total_samples == 0` (divide-by-zero guard matches
    /// the CSV serialiser).
    pub fallthrough_pct: f64,
    /// Number of rules in the chain (matches `rules.len()`).
    pub rule_count: usize,
    /// Number of rules with `dead_at_position == true`. A high count
    /// is the chain-health smoke signal: reorder the dead rows or
    /// tighten the rules shadowing them.
    pub dead_rule_count: usize,
    /// Number of rules partially shadowed: `would_match > first_match`
    /// but NOT dead (some samples still route to this rule, but
    /// strictly fewer than the predicate would match in isolation).
    pub shadowed_rule_count: usize,
    /// Number of rules whose predicate matched zero samples
    /// (`would_match == 0`) and that are NOT dead. The predicate is
    /// too narrow — refine it or drop the rule.
    pub zero_coverage_rule_count: usize,
    /// Per-rule rows verbatim in input order. Length ==
    /// `rule_count`.
    pub rules: Vec<RuleCoverage>,
}

/// Build the envelope from a [`RuleCoverageReport`]. Wall-clock
/// `generated_at_iso` stamp; tests pass a fixed timestamp via
/// [`rule_coverage_to_json_with_now`].
pub fn rule_coverage_to_json(report: &RuleCoverageReport) -> RuleCoverageExportEnvelope {
    rule_coverage_to_json_with_now(report, drilldown_unix_now())
}

/// Same as [`rule_coverage_to_json`] but takes an explicit unix-
/// seconds "now" so unit tests don't race the wall clock.
pub fn rule_coverage_to_json_with_now(
    report: &RuleCoverageReport,
    now_unix: i64,
) -> RuleCoverageExportEnvelope {
    let mut dead = 0usize;
    let mut shadowed = 0usize;
    let mut zero = 0usize;
    for r in &report.rules {
        if r.dead_at_position {
            dead += 1;
        } else if r.would_match == 0 {
            // Zero-coverage and dead-at-position are MUTUALLY EXCLUSIVE
            // here because dead_at_position implies would_match > 0
            // (the predicate matched something — just not at this
            // position). Classification matches the CSV diagnostic
            // priority chain.
            zero += 1;
        } else if r.would_match > r.first_match {
            shadowed += 1;
        }
    }
    let fallthrough_pct = if report.total_samples == 0 {
        0.0
    } else {
        // Round to two decimals to match the CSV serialiser exactly —
        // a consumer cross-referencing CSV + JSON exports of the same
        // report sees identical percentages, not (e.g.) 42.86 vs
        // 42.857142857142854.
        let raw = (report.fallthrough as f64) * 100.0 / (report.total_samples as f64);
        (raw * 100.0).round() / 100.0
    };
    RuleCoverageExportEnvelope {
        schema_version: RULE_COVERAGE_EXPORT_SCHEMA_VERSION,
        generated_at_iso: drilldown_iso8601_utc(now_unix),
        total_samples: report.total_samples,
        fallthrough_count: report.fallthrough,
        fallthrough_pct,
        rule_count: report.rules.len(),
        dead_rule_count: dead,
        shadowed_rule_count: shadowed,
        zero_coverage_rule_count: zero,
        rules: report.rules.clone(),
    }
}

// ─── Slice 128 — rule coverage diagnostic filter primitive ───────────
//
// Round 26 surfaced a chain-health chip ("2 dead rules — reorder or
// tighten the shadowing rules"). The natural follow-up question — "OK,
// which 2 rules?" — has no answer without the user manually scanning
// the per-row list for matching chips. In a 20-rule chain that's
// tedious; in a chain with mixed diagnostics (1 dead + 3 shadowed) the
// chain-level chip's count doesn't even tell you where to start
// looking.
//
// This module gives the UI a pure-data primitive for narrowing the
// coverage report to one diagnostic kind. The return type is a NEW
// [`RuleCoverageReport`] (not a `Vec<&RuleCoverage>`) so the existing
// export / render path treats the filtered view exactly like the
// unfiltered one. Totals are preserved verbatim from the source —
// `fallthrough` + `total_samples` are corpus-scoped invariants of the
// underlying chain RUN, not properties of the filtered slice; we
// surface a separate [`rule_count_in`] / [`rule_count_out`] split via
// the wire envelope (slice 130) when the caller wants the
// "Showing 2 of 6 rules" copy.
//
// ## Why preserve totals
//
// A filtered export's CSV envelope's `fallthrough_count` still names
// the SAME number of samples that fell through to the watch
// defaults — the filter narrows which RULES the export carries, not
// which SAMPLES the chain saw. A consumer reading a filtered
// `hopper-coverage_watch-7_dead_2026-06-23.csv` and unfiltered
// `hopper-coverage_watch-7_2026-06-23.csv` of the same run sees
// identical fall-through accounting; only the per-rule rows differ.
//
// ## Filter semantics
//
// Mutually-exclusive diagnostic kinds matching the established
// priority chain ([`coverage_diagnostic_str`] and the TS
// `ruleCoverageDiagnostic`): `dead > zero > shadowed > healthy`.
// `All` is the identity filter and exists so the UI can wire one
// helper for both filtered and unfiltered code paths.
//
// A rule classified as `dead` does NOT also pass the `shadowed`
// filter — the priority chain is preserved end-to-end so the
// envelope's `*_rule_count` totals and the filter both agree on
// "what kind of trouble is this rule in". A user clicking the
// chain-health chip's "1 dead rule" and seeing exactly 1 rule, then
// clicking through to "2 shadowed rules" and seeing 2 disjoint
// rules, can sum the diagnostic counts and never double-count.

/// Discriminator for the coverage filter. Matches the TS-side
/// `CoverageDiagnosticFilter` 1:1 so the wire payload round-trips
/// without re-mapping.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CoverageFilter {
    /// Identity filter — preserves every rule. Exists so the UI
    /// pipeline can switch between filtered and unfiltered code paths
    /// with one helper.
    All,
    /// Keep only rules with `dead_at_position == true`.
    Dead,
    /// Keep only rules with `would_match == 0` (and NOT dead — the
    /// priority chain dead > zero is preserved).
    Zero,
    /// Keep only rules with `would_match > first_match` AND
    /// `would_match > 0` AND NOT dead (partial shadow only).
    Shadowed,
    /// Keep only rules where `coverage_diagnostic_str` returns the
    /// empty string — predicate fires AND is not shadowed.
    Healthy,
}

impl CoverageFilter {
    /// Discriminator slug used by [`super::super::super::pdf::hopper::cmds`]'s
    /// filename-helper extension and the JSON envelope tag. Kept
    /// here so a single source of truth maps between the enum
    /// variant and the wire string.
    pub fn slug(self) -> &'static str {
        match self {
            CoverageFilter::All => "all",
            CoverageFilter::Dead => "dead",
            CoverageFilter::Zero => "zero",
            CoverageFilter::Shadowed => "shadowed",
            CoverageFilter::Healthy => "healthy",
        }
    }
}

/// True iff `rule` matches `filter`. Internal predicate composed from
/// [`coverage_diagnostic_str`] so the filter and the CSV diagnostic
/// column never drift apart. Keeping this private lets the public API
/// stay `RuleCoverageReport -> RuleCoverageReport`; callers don't need
/// to know the per-row matching rule, only that the result is
/// self-consistent.
fn rule_matches_filter(rule: &RuleCoverage, filter: CoverageFilter) -> bool {
    match filter {
        CoverageFilter::All => true,
        CoverageFilter::Dead => coverage_diagnostic_str(rule) == "dead",
        CoverageFilter::Zero => coverage_diagnostic_str(rule) == "zero",
        CoverageFilter::Shadowed => coverage_diagnostic_str(rule) == "shadowed",
        CoverageFilter::Healthy => coverage_diagnostic_str(rule).is_empty(),
    }
}

/// Apply a [`CoverageFilter`] to a [`RuleCoverageReport`], returning a
/// new report with `rules` narrowed to those passing the filter.
///
/// `fallthrough` + `total_samples` are PRESERVED verbatim — the
/// filter narrows the rule rows the consumer renders / exports, not
/// the chain-run sample accounting. See module docs above for the
/// rationale.
///
/// `CoverageFilter::All` is the identity transform (clones the
/// input). Filtering an empty rule list returns an empty rule list.
/// Filtering preserves the input order of the rules — no resort.
///
/// Pure function — no I/O, no Tauri.
pub fn filter_coverage_by_diagnostic(
    report: &RuleCoverageReport,
    filter: CoverageFilter,
) -> RuleCoverageReport {
    let rules: Vec<RuleCoverage> = report
        .rules
        .iter()
        .filter(|r| rule_matches_filter(r, filter))
        .cloned()
        .collect();
    RuleCoverageReport {
        rules,
        fallthrough: report.fallthrough,
        total_samples: report.total_samples,
    }
}

// ─── Slice 133 — dead-rule reorder planner ───────────────────────────
//
// Round 27 (slices 128-132) shipped the "diagnose + drill in" half of
// the dead-rule story: the chain-health chip surfaces "2 dead rules",
// the filter chip narrows the panel to those 2 rules, the drilldown
// shows the empty bucket. The natural follow-up — "OK, FIX it for
// me" — has no answer; the user has to read the rule chain, identify
// which earlier rule is shadowing the dead one, and manually drag
// the dead row earlier. In a 20-rule chain with three dead rules
// that's tedious; in a 6-rule chain with one shadowing `Always`
// catch-all the fix is mechanical and a one-click action would close
// the loop end-to-end.
//
// This slice ships the pure-data primitive. Given a chain + its
// coverage report, it produces a list of [`ReorderProposal`]s — one
// per dead rule, each carrying the current index, the proposed
// target index, the name of the shadowing rule (if any), and the
// sample count the move would recover.
//
// ## Why a planner, not a single "fix everything" call
//
// Multiple dead rules can interact — fixing one rearranges the
// chain and may re-classify a previously-dead rule (or, more rarely,
// create a NEW dead rule). The planner produces independent
// proposals against the ORIGINAL chain; the UI applies them one at
// a time, refreshes coverage, and the next planner run reflects
// the new chain state. This keeps each fix-it action atomic and
// revertible (the user can apply one proposal, see the result, and
// undo before applying the next).
//
// ## Heuristic for target_index
//
// Without sample data, the planner can't know WHICH earlier rule
// catches the dead rule's samples (the coverage report carries
// counts, not per-sample winners). Two cases:
//
// 1. Some rule in `[0..rule_index)` has predicate [`RulePredicate::Always`].
//    `Always` is the only predicate that PROVABLY shadows ANY other
//    rule's matches — by definition it catches every sample,
//    including everything the dead rule would catch. Target_index =
//    index of the EARLIEST `Always` in `[0..rule_index)`. Moving the
//    dead rule there preserves any specific-predicate rules above
//    the `Always` while letting the dead rule fire just before the
//    catch-all swallows everything.
//
// 2. No `Always` predicate in `[0..rule_index)`. The shadower is a
//    non-`Always` rule whose predicate overlaps the dead rule's by
//    sample; we can't identify which one without samples. The
//    conservative fix is `target_index = 0` — move the dead rule
//    to the chain's front, guaranteeing it fires before any
//    potential shadower. This is more aggressive than necessary in
//    some cases but is always correct (the dead rule will fire on
//    every sample it would catch, period).
//
// `target_index < rule_index` is an INVARIANT — the planner never
// proposes moving a dead rule LATER, because that strictly cannot
// help (the rule was already dead at its current position).
//
// ## Pure data
//
// No I/O, no DB, no Tauri. The Tauri command surface in
// [`crate::pdf::hopper::cmds`] wraps this primitive 1:1.

/// One reorder suggestion produced by [`plan_dead_rule_reorder`] —
/// "move rule X from index `rule_index` to index `target_index` to
/// recover `samples_recovered` matches it currently loses".
///
/// All counts are taken verbatim from the coverage report; the
/// planner does not re-derive them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReorderProposal {
    /// 0-based index of the dead rule in the input chain
    /// (== [`RuleCoverage::index`] for the row).
    pub rule_index: usize,
    /// User-visible name of the dead rule. Echoed so the UI can
    /// render the proposal without re-resolving the rule by index.
    pub rule_name: String,
    /// 0-based index to move the rule TO. Always strictly less than
    /// `rule_index` — the planner never suggests a move that can't
    /// help. See module docs above for the heuristic.
    pub target_index: usize,
    /// User-visible name of the rule currently at `target_index` —
    /// the rule the dead one will leapfrog. Empty string when
    /// `target_index == 0` AND the front rule has no name OR when
    /// the planner picked target_index = 0 as the conservative
    /// fallback (no `Always` shadower identified); the UI uses an
    /// empty value to fall back to a generic copy ("Move to the
    /// front of the chain") rather than naming a specific rule.
    pub shadowing_rule_name: String,
    /// Number of samples the dead rule would route AFTER the move.
    /// Equal to [`RuleCoverage::would_match`] — the predicate is
    /// unchanged, so once the rule fires earlier than any shadower
    /// it routes every sample its predicate matches. The UI uses
    /// this for the "Recovers 3 matches" suffix on the fix-it chip.
    pub samples_recovered: u64,
}

/// Reason a proposal was skipped by [`apply_reorder_proposals_batch`].
///
/// Discriminated as snake_case `kind` for the TS mirror.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchReorderSkipReason {
    /// The rule named in `rule_name` was not present in the chain at
    /// the time the proposal was about to be applied. Either an
    /// earlier proposal in the batch removed/renamed it, or the
    /// caller's proposal list is stale relative to the chain.
    RuleNotFound,
    /// The rule still exists, but the proposal's `target_index` is
    /// no longer earlier than the rule's CURRENT position (an earlier
    /// proposal in the batch already moved the rule earlier than this
    /// proposal wanted to). Applying it would be a no-op or, worse,
    /// a backwards move.
    AlreadyEarlier,
}

/// One proposal that was not applied by [`apply_reorder_proposals_batch`],
/// carried alongside the new chain so the UI can render an audit
/// breakdown ("2 applied, 1 skipped — Tax already earlier").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedProposal {
    /// 0-based index into the INPUT proposal list (NOT the chain).
    /// Lets the UI render the skipped proposals in their original
    /// order alongside the applied ones.
    pub input_index: usize,
    /// Echo of the proposal that was skipped — the UI doesn't have
    /// to round-trip to the source list.
    pub proposal: ReorderProposal,
    /// Why the proposal was skipped.
    pub reason: BatchReorderSkipReason,
}

/// Outcome of a batch reorder application. Carries the new chain
/// plus per-proposal applied/skipped accounting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchReorderOutcome {
    /// The chain AFTER every applied proposal landed. Each rule
    /// object is a clone of the corresponding source rule (the
    /// planner's input ownership semantics are preserved). When
    /// every proposal is skipped this equals the source chain
    /// verbatim.
    pub rules: Vec<Rule>,
    /// Input-order indices of proposals that landed successfully.
    /// `applied.len() + skipped.len() == input proposal count`
    /// (pinned by invariant test).
    pub applied: Vec<usize>,
    /// Proposals that did not land, with reason. Carried in input
    /// order — `skipped[i].input_index` is strictly monotonic.
    pub skipped: Vec<SkippedProposal>,
    /// Total recovered samples across applied proposals. Pre-summed
    /// so the UI doesn't have to re-walk the slice for the toast.
    pub total_recovered: u64,
}

/// Apply every proposal in `proposals` to `rules` in input order,
/// resolving the source rule by NAME at each step so the index
/// drift from earlier moves doesn't make later proposals point at
/// the wrong rule.
///
/// Algorithm: for each proposal in INPUT order
///   1. Find the current index of `proposal.rule_name` in the
///      running chain. If absent -> skip (`RuleNotFound`).
///   2. Find the current index of `proposal.shadowing_rule_name`
///      in the running chain — if non-empty AND present, use THAT
///      as the target index (so the proposal lands "before the
///      shadower" even if the shadower has itself moved). When the
///      shadower name is empty (the planner's fallback) OR the
///      shadower is no longer present, fall back to `target_index = 0`
///      (move to front).
///   3. If the resolved target index is `>=` the current source
///      index -> skip (`AlreadyEarlier`). Applying would be a no-op
///      or a backwards move.
///   4. Otherwise splice the rule from its current index to the
///      target and continue.
///
/// Pure function — no I/O, no Tauri. Mirrored 1:1 in TS as
/// `applyReorderProposalsBatch` (slice 139).
pub fn apply_reorder_proposals_batch(
    rules: &[Rule],
    proposals: &[ReorderProposal],
) -> BatchReorderOutcome {
    let mut chain: Vec<Rule> = rules.to_vec();
    let mut applied: Vec<usize> = Vec::new();
    let mut skipped: Vec<SkippedProposal> = Vec::new();
    let mut total_recovered: u64 = 0;
    for (i, p) in proposals.iter().enumerate() {
        // Resolve source by name in the CURRENT chain.
        let src = chain.iter().position(|r| r.name == p.rule_name);
        let Some(src_idx) = src else {
            skipped.push(SkippedProposal {
                input_index: i,
                proposal: p.clone(),
                reason: BatchReorderSkipReason::RuleNotFound,
            });
            continue;
        };
        // Resolve target by shadower name when possible; fall back
        // to the proposal's recorded target_index otherwise.
        let target = if !p.shadowing_rule_name.is_empty() {
            chain
                .iter()
                .position(|r| r.name == p.shadowing_rule_name)
                .unwrap_or(0)
        } else {
            0
        };
        if target >= src_idx {
            skipped.push(SkippedProposal {
                input_index: i,
                proposal: p.clone(),
                reason: BatchReorderSkipReason::AlreadyEarlier,
            });
            continue;
        }
        let moved = chain.remove(src_idx);
        chain.insert(target, moved);
        applied.push(i);
        total_recovered = total_recovered.saturating_add(p.samples_recovered);
    }
    BatchReorderOutcome {
        rules: chain,
        applied,
        skipped,
        total_recovered,
    }
}

// ─── Slice 143 — reorder-effect summary primitive (round-30) ──────────
//
// Round 29 closed the "fix one / fix all" loop with the per-row
// fix-it pill and the batch fix-all button. Round 30's payoff is the
// natural completion: undo. A user who clicked "Fix all · 5" and
// regrets it (or who clicked "Fix it" on the wrong row) needs ONE
// button to revert — manually re-dragging 5 rules back to their
// prior order is exactly the kind of friction the round 29 batch
// path was supposed to eliminate, and it would be cruel to leave
// the undo path missing.
//
// This slice is the load-bearing pure-data primitive: given a
// chain BEFORE the reorder and a chain AFTER, produce a structural
// summary of what changed. The undo UI uses this to (a) generate
// the human-facing "Undo · move 3 rules back" copy, (b) detect
// when the current chain has DRIFTED away from the post-reorder
// state (the user manually edited rules between fix-it and undo —
// the undo target is now ambiguous), and (c) feed an audit-friendly
// breakdown to scripted-consumers via the slice-145 Tauri command.
//
// The primitive answers three questions:
//   1. Which rules moved (by name, with from/to positions)?
//   2. Were any rules added or removed (a fix-it / fix-all path
//      doesn't add or remove, but a future caller composing this
//      against a manual edit might)?
//   3. Is the AFTER chain a PERMUTATION of BEFORE (same rule names,
//      same multiset), or has the rule set itself shifted?
//
// The permutation flag is the load-bearing signal for undo's
// staleness check: undo can only safely revert when the AFTER
// state is still a permutation of BEFORE (no rules were added /
// removed / renamed in between). If a rule was added since the
// reorder, the snapshot's chain has stale length and reverting
// would silently drop the new rule.

/// One rule that moved positions between two chains. Both indices
/// are 0-based into their respective chains. `from == to` is NOT
/// represented — only genuinely-moved rules appear.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReorderMove {
    /// Display name of the rule. By-name resolution is the
    /// canonical identity throughout the reorder pipeline
    /// (apply_reorder_proposals_batch resolves the same way).
    pub rule_name: String,
    /// Position in the BEFORE chain.
    pub from_index: usize,
    /// Position in the AFTER chain.
    pub to_index: usize,
}

/// Structural summary of how an AFTER chain differs from a BEFORE
/// chain. The fields together let the UI render an undo affordance
/// without re-walking either chain itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReorderEffect {
    /// Rules whose index changed between BEFORE and AFTER. In
    /// AFTER-chain order (ascending `to_index`) so the consumer
    /// can iterate top-to-bottom of the visible chain. Empty when
    /// no rule moved (the chains are pointwise-equal modulo
    /// add/remove). A rule that's purely added or removed does NOT
    /// appear here — it's accounted for in `added` / `removed`.
    pub moved: Vec<ReorderMove>,
    /// Rules present in AFTER but absent from BEFORE — by name.
    /// In AFTER-chain order.
    pub added: Vec<String>,
    /// Rules present in BEFORE but absent from AFTER — by name.
    /// In BEFORE-chain order.
    pub removed: Vec<String>,
    /// True iff AFTER is a permutation of BEFORE: the two chains
    /// have the same length AND the same multiset of rule names
    /// (`added.is_empty() && removed.is_empty()`). This is the
    /// load-bearing signal for undo's staleness check — undo can
    /// only safely revert when this is true.
    pub is_permutation: bool,
}

/// Summarise the structural difference between two chains by NAME.
///
/// Algorithm:
///   1. Build a map of name -> index for each chain (length-aware).
///   2. Rules in BOTH whose indices differ -> `moved` entries (in
///      AFTER-chain order).
///   3. Rules in AFTER but not BEFORE -> `added` (AFTER order).
///   4. Rules in BEFORE but not AFTER -> `removed` (BEFORE order).
///   5. `is_permutation` = `added.is_empty() && removed.is_empty()`.
///
/// Duplicate rule names are handled gracefully: the FIRST occurrence
/// in each chain is the canonical position (matches the by-name
/// resolution in `apply_reorder_proposals_batch`), and a name that
/// appears with different multiplicity is treated as added/removed
/// rather than as a partial move. In practice, the Hopper UI
/// enforces unique rule names so the duplicate path is defensive
/// only.
///
/// Pure function — no I/O, no Tauri. Mirrored 1:1 in TS as
/// `summarizeReorderEffect` (slice 144).
pub fn summarize_reorder_effect(before: &[Rule], after: &[Rule]) -> ReorderEffect {
    // First-occurrence index map for each chain. By-name resolution
    // matches the rest of the reorder pipeline.
    let mut before_idx: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, r) in before.iter().enumerate() {
        before_idx.entry(r.name.as_str()).or_insert(i);
    }
    let mut after_idx: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, r) in after.iter().enumerate() {
        after_idx.entry(r.name.as_str()).or_insert(i);
    }

    // Moved: rules present in BOTH whose first-occurrence indices
    // differ. Walk the AFTER chain so the output is in AFTER order.
    let mut moved: Vec<ReorderMove> = Vec::new();
    for (to_index, r) in after.iter().enumerate() {
        // Only consider the FIRST occurrence in AFTER — a second
        // occurrence of the same name is not a "move" of the same
        // rule (the by-name model can't distinguish duplicates).
        if after_idx.get(r.name.as_str()) != Some(&to_index) {
            continue;
        }
        if let Some(&from_index) = before_idx.get(r.name.as_str()) {
            if from_index != to_index {
                moved.push(ReorderMove {
                    rule_name: r.name.clone(),
                    from_index,
                    to_index,
                });
            }
        }
    }

    // Added: in AFTER (first occurrence) but not in BEFORE.
    let mut added: Vec<String> = Vec::new();
    for (i, r) in after.iter().enumerate() {
        if after_idx.get(r.name.as_str()) != Some(&i) {
            continue;
        }
        if !before_idx.contains_key(r.name.as_str()) {
            added.push(r.name.clone());
        }
    }

    // Removed: in BEFORE (first occurrence) but not in AFTER.
    let mut removed: Vec<String> = Vec::new();
    for (i, r) in before.iter().enumerate() {
        if before_idx.get(r.name.as_str()) != Some(&i) {
            continue;
        }
        if !after_idx.contains_key(r.name.as_str()) {
            removed.push(r.name.clone());
        }
    }

    // Permutation iff both buckets empty AND lengths match
    // (defensive: a chain with one duplicate rule could have empty
    // added/removed but a different length; treat that as not-a-
    // permutation so undo's staleness check stays conservative).
    let is_permutation = added.is_empty() && removed.is_empty() && before.len() == after.len();

    ReorderEffect {
        moved,
        added,
        removed,
        is_permutation,
    }
}

/// One entry in the Hopper UI's undo ring — a compact summary the
/// audit / script layer can read without round-tripping the full
/// snapshot `Vec<Rule>` that the UI keeps in memory.
///
/// This is the SUMMARY view (no snapshot, no full-fidelity diff)
/// because the ring's audit consumers (CLI "what did I undo
/// recently" subcommand, cron health-check) only need the label /
/// timestamp / applied-effect breadcrumb. The UI keeps the
/// `Vec<Rule>` snapshot in TS state alongside this summary; the
/// wire shape stays small enough to log without bloat.
///
/// Mirrors `UndoEntrySummary` in `src/lib/hopper.ts` (slice 149).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UndoEntrySummary {
    /// Human-facing label sourced from the apply call site —
    /// `"fix-all"` / `"fix-it: Tax"` / etc. Used for the ring
    /// summary copy (`"3 undo steps (oldest: fix-it: Tax)"`).
    pub label: String,
    /// Unix-ms timestamp of when the entry was captured. Signed
    /// because `chrono`'s JS-style Date.now() can predate the epoch
    /// in unit-test injectables; the UI never sends a value < 0
    /// in practice.
    pub captured_at_ms: i64,
    /// Structural breadcrumb describing what the reorder DID.
    /// Pre-computed at capture time by the bridge layer; carried
    /// through here so a scripted-audit consumer can render
    /// `describeReorderEffect`-style copy without re-running the
    /// diff against a snapshot it doesn't have access to.
    pub applied_effect: ReorderEffect,
}

/// Structural summary of an undo ring — the entries (oldest-trimmed
/// to capacity) plus capacity / full metadata for the UI's counter
/// chip and the audit log's "at capacity" warning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UndoRingSummary {
    /// Entries OLDEST-FIRST in the order they were captured (after
    /// trimming). `entries[0]` is the next entry to be evicted when
    /// the ring fills; `entries.last()` is the most-recent capture
    /// (the one the UI's button would target by default).
    pub entries: Vec<UndoEntrySummary>,
    /// Configured capacity — the maximum number of entries the
    /// ring retains. The UI seeds this with `UNDO_RING_CAPACITY`
    /// (slice 152, currently 5); audit consumers receive it so
    /// they can render `"at capacity"` warnings without hard-coding
    /// the constant.
    pub capacity: usize,
    /// True iff `entries.len() == capacity` — the ring is full
    /// and the next push will evict the oldest. The UI surfaces
    /// this as a subtle visual cue (the counter chip darkens).
    pub full: bool,
}

/// Summarise a list of undo entries against a ring capacity.
///
/// Algorithm:
///   1. If `capacity == 0` -> empty summary with `full = true`
///      (a zero-capacity ring is structurally always full;
///      defensive against a UI bug that passes 0).
///   2. If `entries.len() > capacity` -> trim the OLDEST entries
///      (keep the most-recent `capacity` entries). The trim is
///      defensive — the UI's `pushUndoEntry` (slice 151) trims at
///      push time, but a stale caller (e.g. an audit replay that
///      hands in raw historical entries) should still get a sane
///      summary.
///   3. Otherwise pass the entries through unchanged.
///   4. `full = entries_after_trim.len() == capacity`.
///
/// Pure function — no I/O, no Tauri. Mirrored 1:1 in TS as
/// `summarizeUndoRing` (slice 149).
pub fn summarize_undo_ring(entries: &[UndoEntrySummary], capacity: usize) -> UndoRingSummary {
    if capacity == 0 {
        return UndoRingSummary {
            entries: Vec::new(),
            capacity: 0,
            full: true,
        };
    }
    let kept: Vec<UndoEntrySummary> = if entries.len() > capacity {
        entries[entries.len() - capacity..].to_vec()
    } else {
        entries.to_vec()
    };
    let full = kept.len() == capacity;
    UndoRingSummary {
        entries: kept,
        capacity,
        full,
    }
}

/// Plan minimal-reorder fixes for every dead rule in `report`.
///
/// Returns one [`ReorderProposal`] per `dead_at_position == true`
/// rule, in INPUT ORDER (the order of `report.rules`). Empty result
/// when no dead rules exist.
///
/// Per-proposal heuristic for `target_index`:
/// - If any rule in `rules[0..rule_index]` has predicate
///   [`RulePredicate::Always`], `target_index` = index of the
///   EARLIEST such rule (insert the dead rule just before the
///   catch-all). `shadowing_rule_name` is that rule's name.
/// - Otherwise `target_index = 0` and `shadowing_rule_name` is empty
///   (the UI renders a generic "Move to the front of the chain"
///   copy when this happens).
///
/// `samples_recovered` is `RuleCoverage::would_match` verbatim.
///
/// Defensive: rules whose `rule_index >= rules.len()` are SKIPPED
/// (a stale `report` against a shrunken chain shouldn't crash the
/// planner). A `rules.len() == 0` input always returns an empty
/// proposal list (there are no rules to be dead).
///
/// Pure function — no I/O, no Tauri.
pub fn plan_dead_rule_reorder(rules: &[Rule], report: &RuleCoverageReport) -> Vec<ReorderProposal> {
    if rules.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<ReorderProposal> = Vec::new();
    for cov in &report.rules {
        if !cov.dead_at_position {
            continue;
        }
        if cov.index >= rules.len() {
            // Stale report shape; skip silently rather than panic.
            continue;
        }
        // Find the EARLIEST Always in [0..rule_index). That's the
        // shadower we can name with confidence.
        let earliest_always = rules[..cov.index]
            .iter()
            .position(|r| matches!(r.predicate, RulePredicate::Always));
        let (target_index, shadowing_rule_name) = match earliest_always {
            Some(j) => (j, rules[j].name.clone()),
            None => (0, String::new()),
        };
        out.push(ReorderProposal {
            rule_index: cov.index,
            rule_name: cov.name.clone(),
            target_index,
            shadowing_rule_name,
            samples_recovered: cov.would_match,
        });
    }
    out
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

    // ── Slice 123 — rule_coverage_to_csv ──────────────────────────────

    fn coverage(
        rules: Vec<RuleCoverage>,
        fallthrough: u64,
        total_samples: u64,
    ) -> RuleCoverageReport {
        RuleCoverageReport {
            rules,
            fallthrough,
            total_samples,
        }
    }

    fn cov_row(
        index: usize,
        name: &str,
        first_match: u64,
        would_match: u64,
        dead_at_position: bool,
    ) -> RuleCoverage {
        RuleCoverage {
            index,
            name: name.into(),
            first_match,
            would_match,
            dead_at_position,
        }
    }

    #[test]
    fn coverage_csv_header_opt_in_round_trips() {
        let report = coverage(vec![cov_row(0, "Tax", 3, 5, false)], 2, 10);
        let with_header = rule_coverage_to_csv(&report, true);
        let bare = rule_coverage_to_csv(&report, false);
        assert!(with_header.starts_with(RULE_COVERAGE_CSV_HEADER));
        assert!(!bare.starts_with(RULE_COVERAGE_CSV_HEADER));
        // Same body when header is suppressed.
        let header_with_newline = format!("{}\n", RULE_COVERAGE_CSV_HEADER);
        let suffix = with_header.strip_prefix(&header_with_newline).unwrap();
        assert_eq!(suffix, bare);
    }

    #[test]
    fn coverage_csv_emits_one_row_per_rule_plus_fallthrough() {
        let report = coverage(
            vec![
                cov_row(0, "Tax", 3, 3, false),
                cov_row(1, "Invoice", 4, 4, false),
            ],
            3,
            10,
        );
        let csv = rule_coverage_to_csv(&report, false);
        let lines: Vec<&str> = csv.lines().collect();
        // 2 rules + 1 fall-through row.
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("0,Tax,"));
        assert!(lines[1].starts_with("1,Invoice,"));
        // Fall-through row has empty index + 'Fall-through' label.
        assert!(lines[2].starts_with(",Fall-through,3,"));
    }

    #[test]
    fn coverage_csv_fallthrough_row_emitted_even_when_zero() {
        // An export with no fall-through is STILL more useful than
        // an export that silently drops the row — the explicit zero
        // row tells a downstream reader "no fall-through" rather than
        // "the export forgot the fall-through bucket".
        let report = coverage(vec![cov_row(0, "Always", 10, 10, false)], 0, 10);
        let csv = rule_coverage_to_csv(&report, false);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].starts_with(",Fall-through,0,"));
        assert!(lines[1].ends_with(",fallthrough"));
    }

    #[test]
    fn coverage_csv_first_match_pct_is_two_decimals() {
        // 3/7 = 42.857142...; expect "42.86" (rounded, two decimals).
        let report = coverage(vec![cov_row(0, "Tax", 3, 3, false)], 4, 7);
        let csv = rule_coverage_to_csv(&report, false);
        let lines: Vec<&str> = csv.lines().collect();
        // index,name,first_match,would_match,first_match_pct,diagnostic
        let tax_cols: Vec<&str> = lines[0].split(',').collect();
        assert_eq!(tax_cols[4], "42.86");
        // Fall-through pct: 4/7 = 57.14...
        let ft_cols: Vec<&str> = lines[1].split(',').collect();
        assert_eq!(ft_cols[4], "57.14");
    }

    #[test]
    fn coverage_csv_zero_total_samples_emits_zero_zero_pct() {
        // Pinning the divide-by-zero guard: an empty report's pct
        // column reads "0.00" (NOT "NaN").
        let report = coverage(vec![cov_row(0, "Tax", 0, 0, false)], 0, 0);
        let csv = rule_coverage_to_csv(&report, false);
        let lines: Vec<&str> = csv.lines().collect();
        let tax_cols: Vec<&str> = lines[0].split(',').collect();
        assert_eq!(tax_cols[4], "0.00");
        let ft_cols: Vec<&str> = lines[1].split(',').collect();
        assert_eq!(ft_cols[4], "0.00");
    }

    #[test]
    fn coverage_csv_diagnostic_classifies_priority_chain() {
        let report = coverage(
            vec![
                // healthy: would_match == first_match > 0
                cov_row(0, "Healthy", 5, 5, false),
                // shadowed: would_match > first_match, NOT dead
                cov_row(1, "Shadowed", 2, 4, false),
                // zero: would_match == 0
                cov_row(2, "Zero", 0, 0, false),
                // dead: dead_at_position takes priority
                cov_row(3, "Dead", 0, 3, true),
            ],
            1,
            10,
        );
        let csv = rule_coverage_to_csv(&report, false);
        let diagnostics: Vec<&str> = csv
            .lines()
            .map(|l| l.split(',').next_back().unwrap())
            .collect();
        assert_eq!(
            diagnostics,
            vec!["", "shadowed", "zero", "dead", "fallthrough"]
        );
    }

    #[test]
    fn coverage_csv_rfc4180_escapes_name_with_comma_and_quote() {
        let report = coverage(vec![cov_row(0, "Tax, 2026 \"draft\"", 1, 1, false)], 0, 1);
        let csv = rule_coverage_to_csv(&report, false);
        // Quoted with internal quotes doubled.
        assert!(csv.contains("\"Tax, 2026 \"\"draft\"\"\""));
    }

    #[test]
    fn coverage_csv_empty_report_still_emits_fallthrough_row() {
        // An empty rule chain (no rules) should still produce a one-
        // line CSV with the fall-through bucket so the export is
        // shaped correctly for a downstream reader.
        let report = coverage(vec![], 0, 0);
        let csv = rule_coverage_to_csv(&report, false);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with(",Fall-through,0,"));
    }

    #[test]
    fn coverage_csv_column_count_matches_header_length() {
        // Lock onto a stable column count so a future header bump
        // surfaces a missing column on every row in one test.
        let report = coverage(vec![cov_row(0, "Tax", 3, 5, false)], 2, 10);
        let csv = rule_coverage_to_csv(&report, true);
        let header_cols = RULE_COVERAGE_CSV_HEADER.split(',').count();
        for line in csv.lines() {
            // Naive split is fine here — no escaped commas in this fixture.
            let cols = line.split(',').count();
            assert_eq!(cols, header_cols, "wrong col count on line: {line}");
        }
    }

    #[test]
    fn coverage_csv_diagnostic_dead_wins_over_zero_when_both_would_apply() {
        // Edge case: dead_at_position=true with would_match=0 (the
        // dead flag should NOT be set this way in practice, but the
        // helper's priority chain says dead wins). Pin the contract.
        let report = coverage(vec![cov_row(0, "DeadAndZero", 0, 0, true)], 0, 1);
        let csv = rule_coverage_to_csv(&report, false);
        let diagnostics: Vec<&str> = csv
            .lines()
            .map(|l| l.split(',').next_back().unwrap())
            .collect();
        assert_eq!(diagnostics[0], "dead");
    }

    // ── Slice 124 — rule_coverage_to_json ─────────────────────────────

    #[test]
    fn coverage_json_envelope_schema_version_pinned() {
        let report = coverage(vec![], 0, 0);
        let env = rule_coverage_to_json_with_now(&report, 1_710_000_000);
        assert_eq!(env.schema_version, RULE_COVERAGE_EXPORT_SCHEMA_VERSION);
        assert_eq!(env.schema_version, 1);
    }

    #[test]
    fn coverage_json_envelope_generated_at_iso_is_canonical_utc() {
        let report = coverage(vec![], 0, 0);
        let env = rule_coverage_to_json_with_now(&report, 1_710_000_000);
        assert_eq!(env.generated_at_iso, "2024-03-09T16:00:00Z");
    }

    #[test]
    fn coverage_json_envelope_carries_rows_verbatim_in_order() {
        let rows = vec![
            cov_row(0, "Tax", 3, 3, false),
            cov_row(1, "Invoice", 4, 4, false),
            cov_row(2, "Misc", 1, 2, false),
        ];
        let report = coverage(rows.clone(), 2, 10);
        let env = rule_coverage_to_json_with_now(&report, 1_710_000_000);
        assert_eq!(env.rules, rows);
        assert_eq!(env.rule_count, 3);
    }

    #[test]
    fn coverage_json_envelope_totals_match_report() {
        let report = coverage(vec![cov_row(0, "Tax", 3, 5, false)], 7, 10);
        let env = rule_coverage_to_json_with_now(&report, 0);
        assert_eq!(env.total_samples, 10);
        assert_eq!(env.fallthrough_count, 7);
    }

    #[test]
    fn coverage_json_envelope_diagnostic_counts_classify_chain() {
        // Mirror the CSV diagnostic priority chain so the JSON
        // envelope's chain-health totals tell the same story.
        let report = coverage(
            vec![
                cov_row(0, "Healthy", 5, 5, false),
                cov_row(1, "Shadowed", 2, 4, false),
                cov_row(2, "Zero", 0, 0, false),
                cov_row(3, "Dead", 0, 3, true),
                cov_row(4, "Healthy2", 1, 1, false),
            ],
            2,
            15,
        );
        let env = rule_coverage_to_json_with_now(&report, 0);
        assert_eq!(env.dead_rule_count, 1);
        assert_eq!(env.shadowed_rule_count, 1);
        assert_eq!(env.zero_coverage_rule_count, 1);
        // 5 rules total: 2 healthy + 1 shadowed + 1 zero + 1 dead.
        assert_eq!(env.rule_count, 5);
    }

    #[test]
    fn coverage_json_envelope_fallthrough_pct_two_decimals_matches_csv() {
        // 4/7 = 57.14285714...; envelope and CSV must agree exactly.
        let report = coverage(vec![cov_row(0, "Tax", 3, 3, false)], 4, 7);
        let env = rule_coverage_to_json_with_now(&report, 0);
        assert_eq!(env.fallthrough_pct, 57.14);
        // Cross-check against the CSV fall-through row.
        let csv = rule_coverage_to_csv(&report, false);
        let ft_line = csv.lines().nth(1).unwrap();
        let ft_cols: Vec<&str> = ft_line.split(',').collect();
        assert_eq!(ft_cols[4], "57.14");
    }

    #[test]
    fn coverage_json_envelope_zero_total_samples_no_nan() {
        // Empty report — fallthrough_pct should be exactly 0.0 (NOT
        // NaN, NOT inf), matching the CSV serialiser.
        let report = coverage(vec![], 0, 0);
        let env = rule_coverage_to_json_with_now(&report, 0);
        assert_eq!(env.fallthrough_pct, 0.0);
        assert!(env.fallthrough_pct.is_finite());
    }

    #[test]
    fn coverage_json_envelope_round_trips_through_serde() {
        let report = coverage(
            vec![
                cov_row(0, "Tax", 3, 3, false),
                cov_row(1, "Shadowed", 2, 4, false),
            ],
            1,
            6,
        );
        let env = rule_coverage_to_json_with_now(&report, 1_710_000_000);
        let json = serde_json::to_string(&env).unwrap();
        let back: RuleCoverageExportEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
        // Spot-check a few field names so a careless rename breaks here.
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"generated_at_iso\":\"2024-03-09T16:00:00Z\""));
        assert!(json.contains("\"shadowed_rule_count\":1"));
        assert!(json.contains("\"dead_rule_count\":0"));
    }

    #[test]
    fn coverage_json_envelope_pretty_print_is_valid_json() {
        // The Tauri command writes pretty-printed JSON to disk for
        // human readability; pin that the serialiser doesn't emit
        // anything that breaks JSON.parse on the consumer side.
        let report = coverage(vec![cov_row(0, "weird \"name\"", 1, 1, false)], 0, 1);
        let env = rule_coverage_to_json_with_now(&report, 0);
        let pretty = serde_json::to_string_pretty(&env).unwrap();
        let back: RuleCoverageExportEnvelope = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back, env);
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn coverage_json_envelope_parallel_versioned_with_drilldown() {
        // Both envelopes are pinned at v1 today but they are
        // PARALLEL-versioned (a future shape change in one bumps
        // that one only). This test pins the current equality so a
        // careless cross-bump surfaces here.
        assert_eq!(
            RULE_COVERAGE_EXPORT_SCHEMA_VERSION, DRILLDOWN_EXPORT_SCHEMA_VERSION,
            "rule-coverage and drilldown envelopes both start at v1; \
             a future shape change in ONE bumps that one only (not both)."
        );
    }

    #[test]
    fn coverage_json_envelope_dead_rule_excluded_from_shadowed_count() {
        // A dead rule satisfies the shadowed condition
        // (would_match > first_match) too; the envelope must NOT
        // double-count it. Pin the precedence.
        let report = coverage(vec![cov_row(0, "Dead", 0, 3, true)], 0, 10);
        let env = rule_coverage_to_json_with_now(&report, 0);
        assert_eq!(env.dead_rule_count, 1);
        assert_eq!(env.shadowed_rule_count, 0);
        assert_eq!(env.zero_coverage_rule_count, 0);
    }

    #[test]
    fn coverage_json_envelope_rule_count_matches_rows_len() {
        // rule_count must mirror rows.len() in every shape — a
        // future change to the envelope's derivation can't silently
        // break this invariant.
        let report = coverage(
            vec![
                cov_row(0, "A", 1, 1, false),
                cov_row(1, "B", 2, 2, false),
                cov_row(2, "C", 3, 3, false),
            ],
            0,
            6,
        );
        let env = rule_coverage_to_json_with_now(&report, 0);
        assert_eq!(env.rule_count, env.rules.len());
        assert_eq!(env.rule_count, 3);
    }

    // ── Slice 128 — filter_coverage_by_diagnostic ──────────────────────

    /// Build a mixed report with all four diagnostic kinds represented.
    /// Used by every filter test so the priority chain is exercised in
    /// each case without re-writing the inputs.
    fn mixed_diagnostic_report() -> RuleCoverageReport {
        coverage(
            vec![
                // Healthy: predicate matches some samples + no shadow.
                cov_row(0, "Healthy A", 4, 4, false),
                // Dead at position: never wins here but would win
                // earlier. Priority dead > shadowed > zero applies.
                cov_row(1, "Dead B", 0, 3, true),
                // Partially shadowed: would_match > first_match,
                // first_match > 0 (so not dead).
                cov_row(2, "Shadowed C", 1, 5, false),
                // Zero coverage: predicate matches nothing at all.
                cov_row(3, "Zero D", 0, 0, false),
                // Another healthy row so the All / Healthy filter
                // counts are non-trivial.
                cov_row(4, "Healthy E", 2, 2, false),
            ],
            7,
            17,
        )
    }

    #[test]
    fn filter_all_preserves_every_rule_and_totals() {
        let src = mixed_diagnostic_report();
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::All);
        assert_eq!(got, src, "All filter must be the identity transform");
    }

    #[test]
    fn filter_dead_keeps_only_dead_rule() {
        let src = mixed_diagnostic_report();
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::Dead);
        assert_eq!(got.rules.len(), 1);
        assert_eq!(got.rules[0].name, "Dead B");
        assert!(got.rules[0].dead_at_position);
        // Totals preserved verbatim — filter narrows rules, not
        // the chain-run sample accounting.
        assert_eq!(got.fallthrough, 7);
        assert_eq!(got.total_samples, 17);
    }

    #[test]
    fn filter_shadowed_excludes_dead_even_though_dead_is_also_shadowed() {
        // A dead rule has would_match > first_match, which is the
        // shadowed predicate. The priority chain dead > shadowed
        // forces it OUT of the shadowed bucket. Pin that here so a
        // refactor can't silently double-count.
        let src = mixed_diagnostic_report();
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::Shadowed);
        assert_eq!(got.rules.len(), 1);
        assert_eq!(got.rules[0].name, "Shadowed C");
        for r in &got.rules {
            assert!(
                !r.dead_at_position,
                "Shadowed filter must NOT include dead rules"
            );
        }
    }

    #[test]
    fn filter_zero_keeps_only_zero_coverage_rule() {
        let src = mixed_diagnostic_report();
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::Zero);
        assert_eq!(got.rules.len(), 1);
        assert_eq!(got.rules[0].name, "Zero D");
        assert_eq!(got.rules[0].first_match, 0);
        assert_eq!(got.rules[0].would_match, 0);
    }

    #[test]
    fn filter_healthy_keeps_only_healthy_rules() {
        let src = mixed_diagnostic_report();
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::Healthy);
        assert_eq!(got.rules.len(), 2);
        assert_eq!(got.rules[0].name, "Healthy A");
        assert_eq!(got.rules[1].name, "Healthy E");
    }

    #[test]
    fn filter_preserves_input_order() {
        // The healthy rule at index 4 comes AFTER the healthy rule at
        // index 0 in the source, and the filter must preserve that.
        // A future refactor to (e.g.) re-sort by name would break the
        // user's mental model — pin it here.
        let src = mixed_diagnostic_report();
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::Healthy);
        assert_eq!(got.rules[0].index, 0);
        assert_eq!(got.rules[1].index, 4);
    }

    #[test]
    fn filter_empty_rules_returns_empty_rules() {
        // Empty input -> empty output. Totals still preserved.
        let src = coverage(vec![], 5, 5);
        for filter in [
            CoverageFilter::All,
            CoverageFilter::Dead,
            CoverageFilter::Zero,
            CoverageFilter::Shadowed,
            CoverageFilter::Healthy,
        ] {
            let got = filter_coverage_by_diagnostic(&src, filter);
            assert!(got.rules.is_empty());
            assert_eq!(got.fallthrough, 5);
            assert_eq!(got.total_samples, 5);
        }
    }

    #[test]
    fn filter_no_matching_rules_returns_empty_rules_with_totals() {
        // A report with only healthy rules filtered by `Dead` yields
        // zero rule rows but keeps the corpus totals.
        let src = coverage(
            vec![cov_row(0, "A", 5, 5, false), cov_row(1, "B", 3, 3, false)],
            0,
            8,
        );
        let got = filter_coverage_by_diagnostic(&src, CoverageFilter::Dead);
        assert!(got.rules.is_empty());
        assert_eq!(got.fallthrough, 0);
        assert_eq!(got.total_samples, 8);
    }

    #[test]
    fn filter_envelope_counts_agree_with_filter_results() {
        // Conservation invariant: summing the filtered rule counts
        // for the three trouble kinds (Dead + Zero + Shadowed) +
        // Healthy must equal the total rule count. A regression in
        // either the envelope's classification or the filter's
        // matching rule would surface here.
        let src = mixed_diagnostic_report();
        let env = rule_coverage_to_json_with_now(&src, 0);
        let dead = filter_coverage_by_diagnostic(&src, CoverageFilter::Dead)
            .rules
            .len();
        let zero = filter_coverage_by_diagnostic(&src, CoverageFilter::Zero)
            .rules
            .len();
        let shadowed = filter_coverage_by_diagnostic(&src, CoverageFilter::Shadowed)
            .rules
            .len();
        let healthy = filter_coverage_by_diagnostic(&src, CoverageFilter::Healthy)
            .rules
            .len();
        assert_eq!(dead, env.dead_rule_count);
        assert_eq!(zero, env.zero_coverage_rule_count);
        assert_eq!(shadowed, env.shadowed_rule_count);
        assert_eq!(dead + zero + shadowed + healthy, env.rule_count);
    }

    #[test]
    fn filter_slug_round_trips_for_all_variants() {
        // The slug is the wire-level discriminator the filename
        // helper (TS) and the JSON tag rely on; pin every variant so
        // a careless rename breaks the test, not paralegal exports.
        assert_eq!(CoverageFilter::All.slug(), "all");
        assert_eq!(CoverageFilter::Dead.slug(), "dead");
        assert_eq!(CoverageFilter::Zero.slug(), "zero");
        assert_eq!(CoverageFilter::Shadowed.slug(), "shadowed");
        assert_eq!(CoverageFilter::Healthy.slug(), "healthy");
    }

    #[test]
    fn filter_does_not_mutate_input_report() {
        // Pure function — the filter must not edit the source report
        // even though both share the same `Vec` backing in many call
        // sites. A clone-on-filter regression that mutated in place
        // would break the UI's "switch back to All" path.
        let src = mixed_diagnostic_report();
        let snapshot = src.clone();
        let _ = filter_coverage_by_diagnostic(&src, CoverageFilter::Dead);
        assert_eq!(src, snapshot);
    }

    // ── Slice 133 — plan_dead_rule_reorder ─────────────────────────────

    /// Helper: build a Rule with the given name + predicate, default
    /// action. The planner cares about the predicate (Always vs
    /// not) and the name (echoed onto the proposal); the action is
    /// irrelevant to reorder planning.
    fn rule_with_predicate(name: &str, predicate: RulePredicate) -> Rule {
        Rule {
            name: name.into(),
            predicate,
            action: RuleAction::default(),
        }
    }

    #[test]
    fn planner_empty_rules_returns_empty_proposals() {
        // No rules -> no dead rules -> no proposals. Pin the early
        // exit so a refactor doesn't try to index into rules[..0].
        let report = coverage(vec![], 0, 0);
        let proposals = plan_dead_rule_reorder(&[], &report);
        assert!(proposals.is_empty());
    }

    #[test]
    fn planner_no_dead_rules_returns_empty_proposals() {
        // Healthy chain — every rule fires. No proposals.
        let rules = vec![
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
            rule_with_predicate("All", RulePredicate::Always),
        ];
        let report = coverage(
            vec![
                cov_row(0, "Tax", 3, 3, false),
                cov_row(1, "All", 7, 7, false),
            ],
            0,
            10,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert!(proposals.is_empty());
    }

    #[test]
    fn planner_one_dead_rule_after_always_targets_the_always_index() {
        // The classic case: catch-all `Always` at index 0 shadows a
        // specific predicate at index 1. The planner moves the
        // specific rule to index 0 (where the Always sits) so it
        // fires before the catch-all swallows everything.
        let rules = vec![
            rule_with_predicate("Catch-all", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "Catch-all", 10, 10, false),
                cov_row(1, "Tax", 0, 3, true),
            ],
            0,
            10,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert_eq!(proposals.len(), 1);
        let p = &proposals[0];
        assert_eq!(p.rule_index, 1);
        assert_eq!(p.rule_name, "Tax");
        assert_eq!(p.target_index, 0);
        assert_eq!(p.shadowing_rule_name, "Catch-all");
        assert_eq!(p.samples_recovered, 3);
    }

    #[test]
    fn planner_picks_earliest_always_when_multiple_alwayss_exist() {
        // Two `Always` rules; the planner must pick the EARLIEST
        // one. Inserting just before the later one would still be
        // shadowed by the earlier one.
        let rules = vec![
            rule_with_predicate("Early all", RulePredicate::Always),
            rule_with_predicate(
                "Specific",
                RulePredicate::FilenameGlob {
                    pattern: "spec_*.pdf".into(),
                },
            ),
            rule_with_predicate("Late all", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "Early all", 10, 10, false),
                cov_row(1, "Specific", 0, 2, true),
                cov_row(2, "Late all", 0, 0, false),
                cov_row(3, "Tax", 0, 3, true),
            ],
            0,
            10,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert_eq!(proposals.len(), 2);
        // Both dead rules target the EARLIEST Always (index 0).
        for p in &proposals {
            assert_eq!(p.target_index, 0);
            assert_eq!(p.shadowing_rule_name, "Early all");
        }
    }

    #[test]
    fn planner_falls_back_to_index_zero_with_empty_shadower_when_no_always() {
        // No `Always` predicate in `[0..rule_index)` — the planner
        // can't identify the shadower by name (some overlapping
        // specific predicate is winning) so it falls back to
        // target_index = 0 with an EMPTY shadowing_rule_name. The
        // UI gates on the empty name to render a generic copy
        // ("Move to the front of the chain") rather than naming
        // a wrong rule.
        let rules = vec![
            rule_with_predicate(
                "Wide glob",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
        ];
        // Wide glob is winning every sample even though it isn't
        // `Always` — `*.pdf` matches every PDF in the run log.
        let report = coverage(
            vec![
                cov_row(0, "Wide glob", 5, 5, false),
                cov_row(1, "Tax", 0, 2, true),
            ],
            0,
            5,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert_eq!(proposals.len(), 1);
        let p = &proposals[0];
        assert_eq!(p.rule_index, 1);
        assert_eq!(p.target_index, 0);
        assert!(
            p.shadowing_rule_name.is_empty(),
            "No `Always` shadower => empty shadowing_rule_name (UI fallback path)"
        );
        assert_eq!(p.samples_recovered, 2);
    }

    #[test]
    fn planner_returns_proposals_in_input_order_not_severity_order() {
        // Two dead rules at indices 1 and 3. Proposals come out
        // in the report's row order (1 then 3), NOT sorted by
        // samples_recovered or any other heuristic. The UI may
        // re-sort if it wants; the planner stays predictable.
        let rules = vec![
            rule_with_predicate("Catch-all", RulePredicate::Always),
            rule_with_predicate(
                "Low value",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
            rule_with_predicate("Healthy", RulePredicate::Always),
            rule_with_predicate(
                "High value",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "Catch-all", 5, 5, false),
                cov_row(1, "Low value", 0, 1, true),
                cov_row(2, "Healthy", 0, 0, false),
                cov_row(3, "High value", 0, 99, true),
            ],
            0,
            5,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].rule_index, 1);
        assert_eq!(proposals[0].samples_recovered, 1);
        assert_eq!(proposals[1].rule_index, 3);
        assert_eq!(proposals[1].samples_recovered, 99);
    }

    #[test]
    fn planner_target_index_strictly_less_than_rule_index_invariant() {
        // Pin the invariant: the planner never proposes a move
        // that can't help (target_index < rule_index always). A
        // regression that proposed an equal or later target would
        // produce a no-op or actively-worse chain.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "A",
                RulePredicate::FilenameGlob {
                    pattern: "a_*".into(),
                },
            ),
            rule_with_predicate(
                "B",
                RulePredicate::FilenameGlob {
                    pattern: "b_*".into(),
                },
            ),
            rule_with_predicate(
                "C",
                RulePredicate::FilenameGlob {
                    pattern: "c_*".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "All", 5, 5, false),
                cov_row(1, "A", 0, 1, true),
                cov_row(2, "B", 0, 2, true),
                cov_row(3, "C", 0, 3, true),
            ],
            0,
            5,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        for p in &proposals {
            assert!(
                p.target_index < p.rule_index,
                "target_index must be strictly less than rule_index (got target {} >= rule_index {})",
                p.target_index,
                p.rule_index
            );
        }
    }

    #[test]
    fn planner_skips_dead_rows_whose_index_overflows_rules_len() {
        // A stale `report` against a shrunken chain should not
        // crash the planner. Dead row at index 5 against a 2-rule
        // chain: skipped silently.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "A",
                RulePredicate::FilenameGlob {
                    pattern: "a_*".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "All", 5, 5, false),
                cov_row(1, "A", 0, 1, true),
                cov_row(5, "Stale Z", 0, 99, true),
            ],
            0,
            5,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        // Only one proposal (the in-range dead row); the stale one
        // is silently skipped.
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].rule_index, 1);
    }

    #[test]
    fn planner_skips_non_dead_diagnostics_zero_and_shadowed() {
        // Zero-coverage + partially-shadowed rules are NOT dead and
        // must not appear in the proposals list. The planner's
        // contract is dead rules only.
        let rules = vec![
            rule_with_predicate(
                "Healthy",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
            rule_with_predicate(
                "Shadowed",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
            rule_with_predicate(
                "Zero",
                RulePredicate::FilenameGlob {
                    pattern: "never_*.pdf".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "Healthy", 3, 3, false),
                cov_row(1, "Shadowed", 1, 5, false),
                cov_row(2, "Zero", 0, 0, false),
            ],
            0,
            4,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert!(proposals.is_empty());
    }

    #[test]
    fn planner_samples_recovered_equals_would_match() {
        // samples_recovered is would_match VERBATIM. Pin this so
        // a future "recovered estimate = would_match - some_loss"
        // refactor surfaces here.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Dead",
                RulePredicate::FilenameGlob {
                    pattern: "x_*".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "All", 10, 10, false),
                cov_row(1, "Dead", 0, 42, true),
            ],
            0,
            10,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].samples_recovered, 42);
    }

    #[test]
    fn planner_serde_round_trips_proposal_field_names() {
        // The wire shape is what the TS mirror reads. Pin every
        // field name (snake_case via serde default) so a careless
        // rename surfaces here, not in the renderer.
        let p = ReorderProposal {
            rule_index: 3,
            rule_name: "Tax".into(),
            target_index: 0,
            shadowing_rule_name: "Catch-all".into(),
            samples_recovered: 7,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"rule_index\":3"));
        assert!(json.contains("\"rule_name\":\"Tax\""));
        assert!(json.contains("\"target_index\":0"));
        assert!(json.contains("\"shadowing_rule_name\":\"Catch-all\""));
        assert!(json.contains("\"samples_recovered\":7"));
        let back: ReorderProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn planner_proposal_echoes_dead_rule_name_exactly() {
        // rule_name is echoed from the coverage report's row, NOT
        // re-resolved from the rules array. Pin that.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "real-name",
                RulePredicate::FilenameGlob {
                    pattern: "a_*".into(),
                },
            ),
        ];
        let report = coverage(
            vec![
                cov_row(0, "All", 5, 5, false),
                // Coverage report's name field is the authoritative
                // copy (it was snapshotted at compute time).
                cov_row(1, "Tax — renamed since", 0, 2, true),
            ],
            0,
            5,
        );
        let proposals = plan_dead_rule_reorder(&rules, &report);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].rule_name, "Tax — renamed since");
    }

    // ── Slice 138 — apply_reorder_proposals_batch ─────────────────────

    /// Helper: build a one-shot ReorderProposal directly. The batch
    /// applier doesn't care which planner produced the proposal; a
    /// caller could in principle hand-roll one. These tests pin
    /// what happens under both well-formed planner output AND
    /// hand-rolled / drifted input.
    fn proposal(
        rule_index: usize,
        rule_name: &str,
        target_index: usize,
        shadowing_rule_name: &str,
        samples_recovered: u64,
    ) -> ReorderProposal {
        ReorderProposal {
            rule_index,
            rule_name: rule_name.into(),
            target_index,
            shadowing_rule_name: shadowing_rule_name.into(),
            samples_recovered,
        }
    }

    #[test]
    fn batch_empty_proposals_returns_source_chain_verbatim() {
        let rules = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate(
                "B",
                RulePredicate::FilenameGlob {
                    pattern: "*.pdf".into(),
                },
            ),
        ];
        let outcome = apply_reorder_proposals_batch(&rules, &[]);
        assert_eq!(outcome.rules, rules);
        assert!(outcome.applied.is_empty());
        assert!(outcome.skipped.is_empty());
        assert_eq!(outcome.total_recovered, 0);
    }

    #[test]
    fn batch_empty_rules_skips_every_proposal_as_not_found() {
        let proposals = vec![proposal(1, "Tax", 0, "Catch-all", 3)];
        let outcome = apply_reorder_proposals_batch(&[], &proposals);
        assert!(outcome.rules.is_empty());
        assert!(outcome.applied.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert!(matches!(
            outcome.skipped[0].reason,
            BatchReorderSkipReason::RuleNotFound
        ));
        assert_eq!(outcome.skipped[0].input_index, 0);
        assert_eq!(outcome.total_recovered, 0);
    }

    #[test]
    fn batch_single_proposal_moves_rule_before_named_shadower() {
        // Classic case: Catch-all at index 0 shadows Tax at index 1.
        // The proposal asks to move Tax to index 0 (where Catch-all
        // sits) so Tax fires first.
        let rules = vec![
            rule_with_predicate("Catch-all", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*.pdf".into(),
                },
            ),
        ];
        let proposals = vec![proposal(1, "Tax", 0, "Catch-all", 3)];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0]);
        assert!(outcome.skipped.is_empty());
        assert_eq!(outcome.rules[0].name, "Tax");
        assert_eq!(outcome.rules[1].name, "Catch-all");
        assert_eq!(outcome.total_recovered, 3);
    }

    #[test]
    fn batch_resolves_source_by_name_after_prior_move() {
        // The KEY invariant: after move 1, index 2 in the original
        // chain is no longer index 2 in the running chain. We must
        // resolve "Receipts" by NAME to land it correctly.
        //
        // Original: [Catch-all, Tax, Receipts]
        //   Move Tax before Catch-all -> [Tax, Catch-all, Receipts]
        //   Move Receipts before Catch-all (now at idx 1) -> [Tax, Receipts, Catch-all]
        let rules = vec![
            rule_with_predicate("Catch-all", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*".into(),
                },
            ),
            rule_with_predicate(
                "Receipts",
                RulePredicate::FilenameGlob {
                    pattern: "r_*".into(),
                },
            ),
        ];
        // Both proposals were produced against the original chain
        // (rule_index = 1 and 2 respectively, target_index = 0
        // both — the planner's earliest-Always heuristic).
        let proposals = vec![
            proposal(1, "Tax", 0, "Catch-all", 3),
            proposal(2, "Receipts", 0, "Catch-all", 5),
        ];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0, 1]);
        assert!(outcome.skipped.is_empty());
        let names: Vec<&str> = outcome.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Tax", "Receipts", "Catch-all"]);
        assert_eq!(outcome.total_recovered, 8);
    }

    #[test]
    fn batch_skips_proposal_when_rule_was_renamed() {
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Renamed",
                RulePredicate::FilenameGlob {
                    pattern: "a_*".into(),
                },
            ),
        ];
        // Proposal still names the OLD label.
        let proposals = vec![proposal(1, "Tax", 0, "All", 2)];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.rules, rules);
        assert!(outcome.applied.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert!(matches!(
            outcome.skipped[0].reason,
            BatchReorderSkipReason::RuleNotFound
        ));
        assert_eq!(outcome.skipped[0].input_index, 0);
    }

    #[test]
    fn batch_skips_proposal_when_rule_already_earlier_than_target() {
        // Chain: [A, B, C]. First proposal moves C before A -> [C, A, B].
        // Second proposal (from the ORIGINAL plan) asks to move B
        // before A, but A is now at index 1 and B is at index 2 —
        // target (1) < src (2), still movable. Try a tougher case:
        // first move B to front -> [B, A, C]. Second proposal asks
        // to move C before A — A is at index 1, C is at index 2.
        // Target (1) < src (2) is still a valid move. We need a
        // scenario where prior moves push a rule ALREADY earlier.
        //
        // Use: chain [A, B, C]; proposals [move B before A, move B
        // before A again]. Second is a self-redundant proposal that
        // should skip as AlreadyEarlier.
        let rules = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate(
                "B",
                RulePredicate::FilenameGlob {
                    pattern: "b_*".into(),
                },
            ),
            rule_with_predicate(
                "C",
                RulePredicate::FilenameGlob {
                    pattern: "c_*".into(),
                },
            ),
        ];
        let proposals = vec![proposal(1, "B", 0, "A", 2), proposal(1, "B", 0, "A", 2)];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0]);
        assert_eq!(outcome.skipped.len(), 1);
        assert!(matches!(
            outcome.skipped[0].reason,
            BatchReorderSkipReason::AlreadyEarlier
        ));
        assert_eq!(outcome.skipped[0].input_index, 1);
        let names: Vec<&str> = outcome.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["B", "A", "C"]);
        assert_eq!(outcome.total_recovered, 2);
    }

    #[test]
    fn batch_falls_back_to_index_zero_when_shadower_name_empty() {
        // No-Always chain: planner produces a proposal with
        // shadowing_rule_name = "" and target_index = 0. The batch
        // applier honours the fallback.
        let rules = vec![
            rule_with_predicate(
                "First",
                RulePredicate::FilenameGlob {
                    pattern: "f_*".into(),
                },
            ),
            rule_with_predicate(
                "Second",
                RulePredicate::FilenameGlob {
                    pattern: "s_*".into(),
                },
            ),
            rule_with_predicate(
                "Dead",
                RulePredicate::FilenameGlob {
                    // Shadowed but no Always to name.
                    pattern: "f_*".into(),
                },
            ),
        ];
        let proposals = vec![proposal(2, "Dead", 0, "", 0)];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0]);
        assert!(outcome.skipped.is_empty());
        assert_eq!(outcome.rules[0].name, "Dead");
        assert_eq!(outcome.rules[1].name, "First");
        assert_eq!(outcome.rules[2].name, "Second");
    }

    #[test]
    fn batch_shadower_drifted_falls_back_to_index_zero() {
        // The proposal names "Catch-all" as shadower but that rule
        // has been removed (a prior proposal in the batch wouldn't
        // do that, but a hand-rolled proposal list might). The
        // applier falls back to target = 0 rather than refusing.
        let rules = vec![
            rule_with_predicate(
                "First",
                RulePredicate::FilenameGlob {
                    pattern: "f_*".into(),
                },
            ),
            rule_with_predicate(
                "Dead",
                RulePredicate::FilenameGlob {
                    pattern: "f_*".into(),
                },
            ),
        ];
        let proposals = vec![proposal(1, "Dead", 0, "Catch-all-gone", 0)];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0]);
        assert!(outcome.skipped.is_empty());
        assert_eq!(outcome.rules[0].name, "Dead");
        assert_eq!(outcome.rules[1].name, "First");
    }

    #[test]
    fn batch_mixed_applied_and_skipped_preserves_input_order() {
        // Three proposals: 0 and 2 applicable, 1 references a
        // missing rule. Outcome.applied should be [0, 2] and
        // outcome.skipped should be [{input_index: 1, ...}].
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "t_*".into(),
                },
            ),
            rule_with_predicate(
                "Receipts",
                RulePredicate::FilenameGlob {
                    pattern: "r_*".into(),
                },
            ),
        ];
        let proposals = vec![
            proposal(1, "Tax", 0, "All", 2),
            proposal(99, "ghost", 0, "All", 7),
            proposal(2, "Receipts", 0, "All", 4),
        ];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0, 2]);
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].input_index, 1);
        assert!(matches!(
            outcome.skipped[0].reason,
            BatchReorderSkipReason::RuleNotFound
        ));
        let names: Vec<&str> = outcome.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Tax", "Receipts", "All"]);
        // Recovered = 2 (Tax) + 4 (Receipts) — the skipped 7
        // should NOT contribute.
        assert_eq!(outcome.total_recovered, 6);
    }

    #[test]
    fn batch_applied_plus_skipped_equals_input_count_invariant() {
        // Conservation invariant — every input proposal lands in
        // exactly one of `applied` or `skipped`. Pinned across a
        // moderately busy mixed batch.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "t_*".into(),
                },
            ),
            rule_with_predicate(
                "Receipts",
                RulePredicate::FilenameGlob {
                    pattern: "r_*".into(),
                },
            ),
            rule_with_predicate(
                "Invoices",
                RulePredicate::FilenameGlob {
                    pattern: "i_*".into(),
                },
            ),
        ];
        let proposals = vec![
            proposal(1, "Tax", 0, "All", 2),
            proposal(2, "Receipts", 0, "All", 4),
            // Stale rule -> skipped
            proposal(99, "ghost", 0, "All", 7),
            proposal(3, "Invoices", 0, "All", 1),
        ];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(
            outcome.applied.len() + outcome.skipped.len(),
            proposals.len(),
            "every input proposal must land in exactly one bucket"
        );
        // Applied indices are a STRICT SUBSET of [0..proposals.len())
        // — no duplicates, no out-of-range entries.
        for &a in &outcome.applied {
            assert!(a < proposals.len());
        }
        // Skipped input_indices are strictly monotonic (carries
        // input order).
        let mut prev: Option<usize> = None;
        for s in &outcome.skipped {
            if let Some(p) = prev {
                assert!(s.input_index > p);
            }
            prev = Some(s.input_index);
        }
    }

    #[test]
    fn batch_does_not_mutate_input_rules() {
        // The source slice is borrowed; the running chain is a
        // clone. Pin that a partial-success batch doesn't leave the
        // source visibly mutated to any external observer.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "t_*".into(),
                },
            ),
        ];
        let snapshot = rules.clone();
        let proposals = vec![proposal(1, "Tax", 0, "All", 3)];
        let _ = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(rules, snapshot, "source slice must not be mutated");
    }

    #[test]
    fn batch_serde_round_trips_outcome_field_names() {
        // The wire shape is what the TS mirror reads. Pin every
        // top-level field name + the discriminator on
        // BatchReorderSkipReason.
        let outcome = BatchReorderOutcome {
            rules: vec![rule_with_predicate("Tax", RulePredicate::Always)],
            applied: vec![0, 2],
            skipped: vec![SkippedProposal {
                input_index: 1,
                proposal: proposal(99, "ghost", 0, "All", 7),
                reason: BatchReorderSkipReason::RuleNotFound,
            }],
            total_recovered: 9,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"rules\":"));
        assert!(json.contains("\"applied\":[0,2]"));
        assert!(json.contains("\"skipped\":"));
        assert!(json.contains("\"input_index\":1"));
        assert!(json.contains("\"reason\":{\"kind\":\"rule_not_found\"}"));
        assert!(json.contains("\"total_recovered\":9"));
        // AlreadyEarlier branch.
        let already = BatchReorderSkipReason::AlreadyEarlier;
        let already_json = serde_json::to_string(&already).unwrap();
        assert_eq!(already_json, "{\"kind\":\"already_earlier\"}");
        // Round-trip wholeness.
        let back: BatchReorderOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(back, outcome);
    }

    #[test]
    fn batch_total_recovered_uses_saturating_add() {
        // Defensive: a planner that produced enormous would_match
        // counts won't wrap. We use saturating_add on u64.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "t_*".into(),
                },
            ),
            rule_with_predicate(
                "Receipts",
                RulePredicate::FilenameGlob {
                    pattern: "r_*".into(),
                },
            ),
        ];
        let proposals = vec![
            proposal(1, "Tax", 0, "All", u64::MAX),
            proposal(2, "Receipts", 0, "All", 5),
        ];
        let outcome = apply_reorder_proposals_batch(&rules, &proposals);
        assert_eq!(outcome.applied, vec![0, 1]);
        // saturating_add(MAX, 5) == MAX.
        assert_eq!(outcome.total_recovered, u64::MAX);
    }

    #[test]
    fn batch_each_rule_is_a_clone_of_source_not_a_reference_alias() {
        // The outcome chain is built from `rules.to_vec()` plus
        // splice ops — each Rule is a clone of the source. Mutating
        // a rule in the outcome must NOT visibly mutate the source.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "tax_*".into(),
                },
            ),
        ];
        let proposals = vec![proposal(1, "Tax", 0, "All", 3)];
        let mut outcome = apply_reorder_proposals_batch(&rules, &proposals);
        outcome.rules[0].name = "Tax-touched".into();
        assert_eq!(rules[1].name, "Tax", "source rule must be unchanged");
    }

    // ── Slice 143 — summarize_reorder_effect (round-30) ──────────────

    #[test]
    fn effect_two_empty_chains_is_empty_permutation() {
        // Edge case: both chains empty. is_permutation = true (a
        // 0-length chain is trivially a permutation of itself).
        let effect = summarize_reorder_effect(&[], &[]);
        assert!(effect.moved.is_empty());
        assert!(effect.added.is_empty());
        assert!(effect.removed.is_empty());
        assert!(effect.is_permutation);
    }

    #[test]
    fn effect_identical_chains_has_no_moves_but_is_permutation() {
        // Identity case: BEFORE == AFTER. No moves, no add/remove,
        // is_permutation = true. The UI's undo gate uses this to
        // recognise "nothing actually changed" and hide the undo
        // affordance gracefully.
        let rules = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate("Tax", RulePredicate::Always),
            rule_with_predicate("Receipts", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&rules, &rules);
        assert!(effect.moved.is_empty(), "identical chains have no moves");
        assert!(effect.added.is_empty());
        assert!(effect.removed.is_empty());
        assert!(effect.is_permutation);
    }

    #[test]
    fn effect_single_swap_records_two_moves_in_after_order() {
        // [A, B] -> [B, A]: both rules moved. Output is in
        // AFTER-chain order so a top-down UI walker reads
        // [B (1->0), A (0->1)].
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("A", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        assert_eq!(effect.moved.len(), 2);
        assert_eq!(effect.moved[0].rule_name, "B");
        assert_eq!(effect.moved[0].from_index, 1);
        assert_eq!(effect.moved[0].to_index, 0);
        assert_eq!(effect.moved[1].rule_name, "A");
        assert_eq!(effect.moved[1].from_index, 0);
        assert_eq!(effect.moved[1].to_index, 1);
        assert!(effect.is_permutation);
    }

    #[test]
    fn effect_lift_one_rule_records_only_displaced_rules() {
        // [A, B, C, Dead] -> [Dead, A, B, C]: every rule moved.
        // Pin that ALL displaced rules appear (not just Dead).
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
            rule_with_predicate("Dead", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("Dead", RulePredicate::Always),
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        // All four rules moved.
        assert_eq!(effect.moved.len(), 4);
        let names: Vec<&str> = effect.moved.iter().map(|m| m.rule_name.as_str()).collect();
        assert_eq!(names, vec!["Dead", "A", "B", "C"]);
        // After-order: ascending to_index.
        for (i, m) in effect.moved.iter().enumerate() {
            assert_eq!(m.to_index, i);
        }
        assert!(effect.is_permutation);
    }

    #[test]
    fn effect_added_rule_recorded_and_not_a_permutation() {
        // [A, B] -> [A, B, C]: C was added. Not a permutation; undo
        // gate uses this to refuse a stale revert.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        assert!(effect.moved.is_empty());
        assert_eq!(effect.added, vec!["C".to_string()]);
        assert!(effect.removed.is_empty());
        assert!(
            !effect.is_permutation,
            "added rule means not-a-permutation; undo must refuse"
        );
    }

    #[test]
    fn effect_removed_rule_recorded_and_not_a_permutation() {
        // [A, B, C] -> [A, B]: C was removed. Not a permutation.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        assert!(effect.moved.is_empty());
        assert!(effect.added.is_empty());
        assert_eq!(effect.removed, vec!["C".to_string()]);
        assert!(!effect.is_permutation);
    }

    #[test]
    fn effect_renamed_rule_appears_as_added_plus_removed() {
        // [A, B] -> [A, B-renamed]: the rename looks like B
        // removed + B-renamed added (the by-name resolution can't
        // know it's a rename). Not a permutation; undo must refuse.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B-renamed", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        assert!(effect.moved.is_empty());
        assert_eq!(effect.added, vec!["B-renamed".to_string()]);
        assert_eq!(effect.removed, vec!["B".to_string()]);
        assert!(!effect.is_permutation);
    }

    #[test]
    fn effect_added_and_removed_at_once_carries_both() {
        // [A, B, C] -> [A, D, C]: B removed, D added, C moved (or
        // not — both indices match). Pin that the buckets carry
        // both, in their respective chain orders.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("D", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        assert_eq!(effect.added, vec!["D".to_string()]);
        assert_eq!(effect.removed, vec!["B".to_string()]);
        // A and C both at the same index in both chains => no moves.
        assert!(effect.moved.is_empty());
        assert!(!effect.is_permutation);
    }

    #[test]
    fn effect_unmoved_rules_are_omitted_from_moved() {
        // [A, B, C, D] -> [A, C, B, D]: only B and C moved; A and D
        // unchanged. Pin that the unchanged rules do NOT appear in
        // `moved`.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
            rule_with_predicate("D", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("D", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        let names: Vec<&str> = effect.moved.iter().map(|m| m.rule_name.as_str()).collect();
        assert_eq!(names, vec!["C", "B"]);
        assert!(effect.is_permutation);
    }

    #[test]
    fn effect_serde_round_trips_field_names_for_ts_mirror() {
        // The wire shape is what the TS mirror reads. Pin every
        // top-level field name + the ReorderMove field names.
        let effect = ReorderEffect {
            moved: vec![ReorderMove {
                rule_name: "Tax".into(),
                from_index: 3,
                to_index: 0,
            }],
            added: vec!["NewRule".into()],
            removed: vec!["OldRule".into()],
            is_permutation: false,
        };
        let json = serde_json::to_string(&effect).unwrap();
        assert!(json.contains("\"moved\":"));
        assert!(json.contains("\"rule_name\":\"Tax\""));
        assert!(json.contains("\"from_index\":3"));
        assert!(json.contains("\"to_index\":0"));
        assert!(json.contains("\"added\":[\"NewRule\"]"));
        assert!(json.contains("\"removed\":[\"OldRule\"]"));
        assert!(json.contains("\"is_permutation\":false"));
        let back: ReorderEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(back, effect);
    }

    #[test]
    fn effect_does_not_mutate_either_input_chain() {
        // Both slices are borrowed; the helper builds a fresh
        // ReorderEffect. Pin that nothing leaks back to the caller.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("A", RulePredicate::Always),
        ];
        let before_snap = before.clone();
        let after_snap = after.clone();
        let _ = summarize_reorder_effect(&before, &after);
        assert_eq!(before, before_snap);
        assert_eq!(after, after_snap);
    }

    #[test]
    fn effect_composes_with_apply_reorder_proposals_batch() {
        // End-to-end: a batch reorder's outcome.rules summarised
        // against the original chain should match the batch's own
        // move semantics. This pins that slice 143 is the right
        // language for describing slice 138's output.
        let before = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "t_*".into(),
                },
            ),
            rule_with_predicate(
                "Receipts",
                RulePredicate::FilenameGlob {
                    pattern: "r_*".into(),
                },
            ),
        ];
        let proposals = vec![
            proposal(1, "Tax", 0, "All", 2),
            proposal(2, "Receipts", 0, "All", 4),
        ];
        let outcome = apply_reorder_proposals_batch(&before, &proposals);
        let effect = summarize_reorder_effect(&before, &outcome.rules);
        // Result is [Tax, Receipts, All] — Tax landed at 0, then
        // Receipts landed before the displaced All (which is now at
        // index 1). Every rule shifted.
        assert_eq!(effect.moved.len(), 3);
        let to_names: Vec<&str> = effect.moved.iter().map(|m| m.rule_name.as_str()).collect();
        assert_eq!(to_names, vec!["Tax", "Receipts", "All"]);
        assert!(effect.is_permutation);
        assert!(effect.added.is_empty());
        assert!(effect.removed.is_empty());
    }

    #[test]
    fn effect_after_order_strictly_ascending_to_index() {
        // Pin the after-order invariant in a busy case: the
        // moved entries must be sorted by to_index ascending.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
            rule_with_predicate("D", RulePredicate::Always),
            rule_with_predicate("E", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("E", RulePredicate::Always),
            rule_with_predicate("D", RulePredicate::Always),
            rule_with_predicate("C", RulePredicate::Always),
            rule_with_predicate("B", RulePredicate::Always),
            rule_with_predicate("A", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        let mut prev: Option<usize> = None;
        for m in &effect.moved {
            if let Some(p) = prev {
                assert!(m.to_index > p, "to_index must be strictly ascending");
            }
            prev = Some(m.to_index);
        }
    }

    #[test]
    fn effect_duplicate_name_uses_first_occurrence_as_canonical() {
        // Defensive — Hopper UI enforces unique names but a hand-
        // rolled chain could carry duplicates. The first occurrence
        // in each chain is canonical; a second occurrence neither
        // counts as a move nor as added.
        let before = vec![
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("Dup", RulePredicate::Always),
            rule_with_predicate("Dup", RulePredicate::Always),
        ];
        let after = vec![
            rule_with_predicate("Dup", RulePredicate::Always),
            rule_with_predicate("A", RulePredicate::Always),
            rule_with_predicate("Dup", RulePredicate::Always),
        ];
        let effect = summarize_reorder_effect(&before, &after);
        // A moved (0 -> 1). First Dup moved (1 -> 0). Second Dup
        // is ignored on both sides.
        let names: Vec<&str> = effect.moved.iter().map(|m| m.rule_name.as_str()).collect();
        assert_eq!(names, vec!["Dup", "A"]);
    }

    #[test]
    fn effect_undo_round_trip_recovers_original_chain_shape() {
        // The motivating use case: a batch reorder followed by
        // summarising the effect, then "undoing" by handing the
        // BEFORE chain back through set-rules — summarizing the
        // post-undo chain against the post-reorder chain should
        // produce the INVERSE permutation.
        let original = vec![
            rule_with_predicate("All", RulePredicate::Always),
            rule_with_predicate(
                "Tax",
                RulePredicate::FilenameGlob {
                    pattern: "t_*".into(),
                },
            ),
            rule_with_predicate(
                "Receipts",
                RulePredicate::FilenameGlob {
                    pattern: "r_*".into(),
                },
            ),
        ];
        let proposals = vec![
            proposal(1, "Tax", 0, "All", 2),
            proposal(2, "Receipts", 0, "All", 4),
        ];
        let reordered = apply_reorder_proposals_batch(&original, &proposals).rules;
        // "Undo" = put the original chain back. Summarise the
        // round-trip against the reordered chain.
        let undo_effect = summarize_reorder_effect(&reordered, &original);
        assert!(undo_effect.is_permutation);
        // Every moved entry should now point from its reordered
        // position back to its original position.
        for m in &undo_effect.moved {
            let orig_pos = original.iter().position(|r| r.name == m.rule_name).unwrap();
            let reord_pos = reordered
                .iter()
                .position(|r| r.name == m.rule_name)
                .unwrap();
            assert_eq!(m.from_index, reord_pos);
            assert_eq!(m.to_index, orig_pos);
        }
    }

    // ── Slice 148 — summarize_undo_ring (round-31) ────────────────────

    fn summary(label: &str, captured_at_ms: i64) -> UndoEntrySummary {
        // Use a trivial single-move effect so the entries are
        // distinguishable when we test trim ordering / round-trip
        // fidelity. Content doesn't matter to the summariser — it
        // never inspects entries beyond passing them through.
        UndoEntrySummary {
            label: label.into(),
            captured_at_ms,
            applied_effect: ReorderEffect {
                moved: vec![ReorderMove {
                    rule_name: label.into(),
                    from_index: 1,
                    to_index: 0,
                }],
                added: Vec::new(),
                removed: Vec::new(),
                is_permutation: true,
            },
        }
    }

    #[test]
    fn ring_empty_summary_not_full() {
        // Edge case: zero entries against a non-zero capacity.
        // is_full = false; the UI's chip stays hidden when there's
        // nothing to undo.
        let summary = summarize_undo_ring(&[], 5);
        assert!(summary.entries.is_empty());
        assert_eq!(summary.capacity, 5);
        assert!(!summary.full);
    }

    #[test]
    fn ring_single_entry_under_capacity() {
        // One entry against capacity 5 — pass-through, not full.
        let e = summary("fix-it: Tax", 1000);
        let summary = summarize_undo_ring(&[e.clone()], 5);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0], e);
        assert_eq!(summary.capacity, 5);
        assert!(!summary.full);
    }

    #[test]
    fn ring_at_capacity_marks_full() {
        // Exactly capacity entries -> full = true. The UI's chip
        // darkens to warn the user the next undo capture will evict
        // the oldest.
        let entries = vec![summary("a", 100), summary("b", 200), summary("c", 300)];
        let summary = summarize_undo_ring(&entries, 3);
        assert_eq!(summary.entries.len(), 3);
        assert!(summary.full);
        // Order preserved (oldest first).
        assert_eq!(summary.entries[0].label, "a");
        assert_eq!(summary.entries[2].label, "c");
    }

    #[test]
    fn ring_over_capacity_trims_oldest() {
        // 7 entries against capacity 5 -> trim 2 oldest (a, b);
        // keep c, d, e, f, g in that order. full = true.
        let entries = vec![
            summary("a", 100),
            summary("b", 200),
            summary("c", 300),
            summary("d", 400),
            summary("e", 500),
            summary("f", 600),
            summary("g", 700),
        ];
        let summary = summarize_undo_ring(&entries, 5);
        assert_eq!(summary.entries.len(), 5);
        assert!(summary.full);
        assert_eq!(summary.entries[0].label, "c");
        assert_eq!(summary.entries[1].label, "d");
        assert_eq!(summary.entries[4].label, "g");
        // Oldest (a, b) dropped.
        assert!(!summary.entries.iter().any(|e| e.label == "a"));
        assert!(!summary.entries.iter().any(|e| e.label == "b"));
    }

    #[test]
    fn ring_zero_capacity_is_always_full() {
        // Defensive: capacity == 0 -> empty entries, full = true.
        // A zero-capacity ring is structurally always full.
        let entries = vec![summary("a", 100), summary("b", 200)];
        let summary = summarize_undo_ring(&entries, 0);
        assert!(summary.entries.is_empty());
        assert_eq!(summary.capacity, 0);
        assert!(summary.full);
    }

    #[test]
    fn ring_one_capacity_keeps_only_newest() {
        // Capacity 1, two entries -> keep only the most-recent.
        let entries = vec![summary("old", 100), summary("new", 200)];
        let summary = summarize_undo_ring(&entries, 1);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].label, "new");
        assert!(summary.full);
    }

    #[test]
    fn ring_summary_pass_through_preserves_field_identity() {
        // Pinned: the summariser does NOT mutate / re-serialise
        // entries; the carried effect / timestamp round-trip with
        // exact equality. Audit consumers depend on this.
        let entry = summary("fix-all", 1700000000000);
        let summary = summarize_undo_ring(std::slice::from_ref(&entry), 5);
        assert_eq!(summary.entries[0], entry);
        assert_eq!(summary.entries[0].captured_at_ms, 1700000000000);
        assert_eq!(summary.entries[0].label, "fix-all");
        assert_eq!(summary.entries[0].applied_effect.moved.len(), 1);
    }

    #[test]
    fn ring_summary_serde_round_trip_snake_case() {
        // Pin snake_case field names so the TS mirror reads the
        // wire shape correctly: captured_at_ms / applied_effect
        // (camelCase would silently drop both on the TS side).
        let entries = vec![summary("fix-it: Tax", 1700000000000)];
        let summary = summarize_undo_ring(&entries, 5);
        let json = serde_json::to_string(&summary).unwrap();
        assert!(
            json.contains("\"captured_at_ms\":1700000000000"),
            "expected snake_case captured_at_ms in {json}"
        );
        assert!(
            json.contains("\"applied_effect\":"),
            "expected snake_case applied_effect in {json}"
        );
        assert!(json.contains("\"capacity\":5"));
        assert!(json.contains("\"full\":false"));
        let back: UndoRingSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back, summary);
    }

    #[test]
    fn ring_no_input_mutation() {
        // Pinned: the input slice is never mutated (the summariser
        // builds a fresh Vec). Defensive guard against future
        // refactor breaking the contract.
        let entries = vec![summary("a", 100), summary("b", 200), summary("c", 300)];
        let snapshot = entries.clone();
        let _ = summarize_undo_ring(&entries, 2);
        assert_eq!(entries, snapshot);
    }

    #[test]
    fn ring_summary_capacity_field_echoes_input() {
        // The capacity field round-trips the input verbatim so the
        // UI's "X of Y undo steps" chip reads the right denominator.
        for cap in [1usize, 3, 5, 10, 100] {
            let s = summarize_undo_ring(&[], cap);
            assert_eq!(s.capacity, cap);
        }
    }
}
