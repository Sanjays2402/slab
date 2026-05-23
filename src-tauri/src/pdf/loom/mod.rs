// src-tauri/src/pdf/loom/mod.rs
//
// Slab Loom — PDF/UA-1 (ISO 14289-1:2014) tagging & validation pipeline.
//
// This is the v3.1.0 "Loom" release entry point. The architecture is laid out
// in docs/adr/2026-05-23-pdf-ua-conformance.md and the slice-by-slice plan in
// docs/plans/2026-05-22-v3.1.0-loom-pdf-ua.md.
//
// Pipeline overview (seven stages):
//
//      bytes → layout → segments → structure → tags → validate → emit
//                                                    ↘
//                                                     human-review queue
//
//   1. `layout`     — extract every text run + image XObject placement with
//                     bbox + font + matrix (Slice 1).
//   2. `segments`   — group runs into lines / columns / blocks (Slice 2).
//   3. `structure`  — classify blocks into Document / Sect / H1..H6 / P / L /
//                     Table / Figure with confidence scores (Slice 3).
//   4. `tags`       — emit the StructTreeRoot, set /MarkInfo, write XMP
//                     pdfuaid:part=1, attach Alt + ActualText + Lang (Slice 4).
//   5. `validate`   — run the auto-decidable Matterhorn 1.1 conditions
//                     (`Verdict::Auto`) against the tagged PDF (Slice 5).
//   6. `review`     — surface remaining `Verdict::Human` conditions in the
//                     Loom Review UI (Slice 6).
//   7. `emit`       — serialise the result + PAC-compatible report bundle
//                     (Slice 7).
//
// The Matterhorn 1.1 failure-condition registry lives in
// `docs/specs/matterhorn-1.1.json` and is the single source of truth. The
// `matterhorn` submodule below is code-generated from it by
// `scripts/loom/codegen-matterhorn.mjs`. CI verifies it stays in sync via
// `pnpm loom:codegen --check`.
//
// Module status (v3.1.0 Slice 0/1 docs-only tick — 2026-05-23):
//
//   * matterhorn   — generated registry + helpers + tests ✅
//   * layout       — TODO (Slice 1 Rust tick after `cargo clean`)
//   * segments     — TODO (Slice 2)
//   * structure    — TODO (Slice 3)
//   * tags         — TODO (Slice 4)
//   * validate     — TODO (Slice 5; uses matterhorn::auto_conditions())
//   * review       — TODO (Slice 6)
//   * emit         — TODO (Slice 7)

pub mod alt_text;
pub mod classify;
pub mod layout;
pub mod matterhorn;
pub mod reading_order;
pub mod structure_tree;

pub use alt_text::{
    alt_text_for_bbox, default_cache_dir as default_alt_text_cache_dir, enrich_with_alt_text,
    AltTextOptions, AltTextStats,
};
pub use classify::{classify, NodeKind, StructNode, StructTree, StructTreePage};
pub use layout::{
    extract_layout, extract_layout_from_doc, Bbox, ImagePlacement, LayoutTree, PageLayout, TextRun,
};
pub use matterhorn::{
    all_conditions, auto_conditions, find_condition, human_conditions, out_of_scope_conditions,
    section_by_id, FailureCondition, Section, Totals, Verdict, APPLIES_TO, CONDITIONS_COUNT,
    PROTOCOL_VERSION, SECTIONS, TOTALS,
};
pub use reading_order::{order_reading, ReadingOrder, ReadingOrderPage};

/// Project-wide coverage snapshot — the numbers that drive the
/// `/accessibility.html` Matterhorn coverage cards and the GitHub release
/// notes. Kept here so a single `cargo test loom::coverage_snapshot` proves
/// the on-disk registry, the generated Rust, and the public-facing copy all
/// agree.
#[derive(Debug, Clone, Copy)]
pub struct CoverageSnapshot {
    pub registry_total: usize,
    pub full_protocol_total: usize,
    pub auto: usize,
    pub human: usize,
    pub out_of_scope: usize,
    pub auto_share_of_full_protocol: f64,
}

impl CoverageSnapshot {
    pub fn from_registry() -> Self {
        let auto = matterhorn::auto_conditions().count();
        let human = matterhorn::human_conditions().count();
        let oos = matterhorn::out_of_scope_conditions().count();
        let full = TOTALS.failure_conditions_in_full_protocol;
        let share = if full == 0 {
            0.0
        } else {
            auto as f64 / full as f64
        };
        Self {
            registry_total: TOTALS.failure_conditions_in_this_registry,
            full_protocol_total: full,
            auto,
            human,
            out_of_scope: oos,
            auto_share_of_full_protocol: share,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_snapshot_matches_registry_totals() {
        let snap = CoverageSnapshot::from_registry();
        assert_eq!(
            snap.registry_total,
            TOTALS.failure_conditions_in_this_registry
        );
        assert_eq!(
            snap.full_protocol_total,
            TOTALS.failure_conditions_in_full_protocol
        );
        assert_eq!(snap.auto, TOTALS.auto);
        assert_eq!(snap.human, TOTALS.human);
        assert_eq!(snap.out_of_scope, TOTALS.out_of_scope);
    }

    #[test]
    fn auto_share_meets_landing_page_claim() {
        // /accessibility.html claims "≈50% of Matterhorn conditions
        // automated" — guard against a regression that would invalidate
        // marketing copy. Today: 48/136 ≈ 35.3%; the claim becomes true
        // once Slice 5 ships the remaining auto rules from the 45
        // not-yet-transcribed conditions. Keep this test conservative:
        // assert the projection ≥35% holds today, ≥50% triggers a copy
        // update reminder.
        let snap = CoverageSnapshot::from_registry();
        assert!(
            snap.auto_share_of_full_protocol >= 0.35,
            "auto coverage regressed below 35% of full protocol: {}",
            snap.auto_share_of_full_protocol,
        );
    }

    #[test]
    fn public_constants_match_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "1.1");
        assert!(APPLIES_TO.contains("14289-1"));
        assert_eq!(CONDITIONS_COUNT, TOTALS.failure_conditions_in_this_registry);
    }

    #[test]
    fn find_condition_handles_unknown_id() {
        assert!(find_condition("99-999").is_none());
        assert!(find_condition("").is_none());
        assert!(find_condition("01-007").is_some());
    }

    #[test]
    fn section_by_id_handles_known_and_unknown() {
        assert!(section_by_id("01").is_some());
        assert!(section_by_id("31").is_some());
        assert!(section_by_id("00").is_none());
        assert!(section_by_id("99").is_none());
    }

    #[test]
    fn every_condition_belongs_to_a_known_section() {
        for c in all_conditions() {
            assert!(
                section_by_id(c.section_id).is_some(),
                "orphan condition {} -> section {}",
                c.id,
                c.section_id,
            );
        }
    }

    #[test]
    fn verdicts_partition_the_registry() {
        let total = all_conditions().count();
        let a = auto_conditions().count();
        let h = human_conditions().count();
        let o = out_of_scope_conditions().count();
        assert_eq!(
            a + h + o,
            total,
            "verdicts do not partition: a={a} h={h} o={o} total={total}",
        );
    }
}
