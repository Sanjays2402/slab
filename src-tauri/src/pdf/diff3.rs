//! Three-way PDF compare (base / mine / theirs).
//!
//! v3.24.0 "Stack Pro". Given a common ancestor PDF and two divergent
//! revisions, classify every base line as `Unchanged | MineOnly | TheirsOnly
//! | BothAgree | Conflict`. This is the canonical legal/dev-team feature
//! Litera Compare charges $400/seat/yr for; Adobe Acrobat doesn't ship it.
//!
//! Strategy: run two existing 2-way diffs (base→mine, base→theirs) via
//! `pdf::diff::diff_pdfs`, then merge them per base line. The classification
//! table is:
//!
//! | mine    | theirs  | kind        |
//! |---------|---------|-------------|
//! | equal   | equal   | Unchanged   |
//! | change  | equal   | MineOnly    |
//! | equal   | change  | TheirsOnly  |
//! | same change on both | BothAgree |
//! | different changes    | Conflict |

use crate::pdf::diff::{diff_pdfs, DiffOp, DocDiff, LineDiff};
use crate::pdf::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreeWayKind {
    /// Both sides left this base line untouched.
    Unchanged,
    /// Only "mine" edited this line; theirs == base.
    MineOnly,
    /// Only "theirs" edited this line; mine == base.
    TheirsOnly,
    /// Both sides applied the *same* edit — clean merge.
    BothAgree,
    /// Both sides edited the line to *different* text — conflict.
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWayLine {
    pub kind: ThreeWayKind,
    pub base_line: Option<u32>,
    pub mine_line: Option<u32>,
    pub theirs_line: Option<u32>,
    pub base_text: String,
    pub mine_text: Option<String>,
    pub theirs_text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWaySummary {
    pub unchanged: u32,
    pub mine_only: u32,
    pub theirs_only: u32,
    pub both_agree: u32,
    pub conflicts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWayPage {
    pub page: u32,
    pub lines: Vec<ThreeWayLine>,
    pub summary: ThreeWaySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeWayDiff {
    pub base_path: PathBuf,
    pub mine_path: PathBuf,
    pub theirs_path: PathBuf,
    pub pages: Vec<ThreeWayPage>,
    pub total: ThreeWaySummary,
}

/// Three-way diff entry point. Reads three PDFs from disk, runs two 2-way
/// diffs against the common ancestor, and merges.
pub fn three_way_diff(base: &Path, mine: &Path, theirs: &Path) -> Result<ThreeWayDiff, PdfError> {
    let bm = diff_pdfs(base, mine)?;
    let bt = diff_pdfs(base, theirs)?;
    Ok(three_way_from_diffs(
        &bm,
        &bt,
        base.to_path_buf(),
        mine.to_path_buf(),
        theirs.to_path_buf(),
    ))
}

/// Pure merge: take two 2-way diffs that share a common `base` and fold them
/// into a `ThreeWayDiff`. Separated out so unit tests can drive it with
/// fabricated `DocDiff` values rather than minting real PDFs.
pub fn three_way_from_diffs(
    bm: &DocDiff,
    bt: &DocDiff,
    base_path: PathBuf,
    mine_path: PathBuf,
    theirs_path: PathBuf,
) -> ThreeWayDiff {
    let page_count = bm.old_page_count.min(bt.old_page_count);
    let mut pages = Vec::with_capacity(page_count as usize);
    let mut total = ThreeWaySummary::default();

    for p in 0..page_count {
        let mine_page = bm.pages.get(p as usize);
        let theirs_page = bt.pages.get(p as usize);
        let (mp, tp) = match (mine_page, theirs_page) {
            (Some(m), Some(t)) => (m, t),
            _ => continue,
        };
        let mine_by_base = index_by_base_line(&mp.lines);
        let theirs_by_base = index_by_base_line(&tp.lines);

        let mut lines = Vec::new();
        let mut summary = ThreeWaySummary::default();
        let max_base = mine_by_base
            .keys()
            .chain(theirs_by_base.keys())
            .max()
            .copied()
            .unwrap_or(0);

        for base_lineno in 1..=max_base {
            let m_entry = mine_by_base.get(&base_lineno);
            let t_entry = theirs_by_base.get(&base_lineno);
            // Skip base line numbers neither side observed (e.g. blank lines
            // dropped by the diff engine).
            if m_entry.is_none() && t_entry.is_none() {
                continue;
            }
            let base_text = m_entry
                .or(t_entry)
                .map(|e| e.base_text.clone())
                .unwrap_or_default();
            let mine_changed = m_entry.is_some_and(|e| e.changed);
            let theirs_changed = t_entry.is_some_and(|e| e.changed);
            let mine_text = m_entry.and_then(|e| e.new_text.clone());
            let theirs_text = t_entry.and_then(|e| e.new_text.clone());

            let kind = match (mine_changed, theirs_changed) {
                (false, false) => {
                    summary.unchanged = summary.unchanged.saturating_add(1);
                    ThreeWayKind::Unchanged
                }
                (true, false) => {
                    summary.mine_only = summary.mine_only.saturating_add(1);
                    ThreeWayKind::MineOnly
                }
                (false, true) => {
                    summary.theirs_only = summary.theirs_only.saturating_add(1);
                    ThreeWayKind::TheirsOnly
                }
                (true, true) => {
                    if mine_text == theirs_text {
                        summary.both_agree = summary.both_agree.saturating_add(1);
                        ThreeWayKind::BothAgree
                    } else {
                        summary.conflicts = summary.conflicts.saturating_add(1);
                        ThreeWayKind::Conflict
                    }
                }
            };

            lines.push(ThreeWayLine {
                kind,
                base_line: Some(base_lineno),
                mine_line: m_entry.and_then(|e| e.new_line),
                theirs_line: t_entry.and_then(|e| e.new_line),
                base_text,
                mine_text,
                theirs_text,
            });
        }

        total.unchanged = total.unchanged.saturating_add(summary.unchanged);
        total.mine_only = total.mine_only.saturating_add(summary.mine_only);
        total.theirs_only = total.theirs_only.saturating_add(summary.theirs_only);
        total.both_agree = total.both_agree.saturating_add(summary.both_agree);
        total.conflicts = total.conflicts.saturating_add(summary.conflicts);
        pages.push(ThreeWayPage {
            page: p + 1,
            lines,
            summary,
        });
    }

    ThreeWayDiff {
        base_path,
        mine_path,
        theirs_path,
        pages,
        total,
    }
}

/// Per-base-line view of one 2-way diff page.
#[derive(Debug, Clone)]
struct BaseEntry {
    changed: bool,
    new_line: Option<u32>,
    base_text: String,
    new_text: Option<String>,
}

fn index_by_base_line(lines: &[LineDiff]) -> std::collections::BTreeMap<u32, BaseEntry> {
    use std::collections::BTreeMap;
    let mut out: BTreeMap<u32, BaseEntry> = BTreeMap::new();

    // Walk lines, pairing Delete+Insert as a "changed" base line.
    let n = lines.len();
    let mut i = 0;
    while i < n {
        let l = &lines[i];
        match l.op {
            DiffOp::Equal => {
                if let Some(bl) = l.old_line {
                    out.insert(
                        bl,
                        BaseEntry {
                            changed: false,
                            new_line: l.new_line,
                            base_text: l.text.clone(),
                            new_text: Some(l.text.clone()),
                        },
                    );
                }
                i += 1;
            }
            DiffOp::Delete => {
                // Look ahead for paired Insert (a "Replace").
                if i + 1 < n && lines[i + 1].op == DiffOp::Insert {
                    if let Some(bl) = l.old_line {
                        out.insert(
                            bl,
                            BaseEntry {
                                changed: true,
                                new_line: lines[i + 1].new_line,
                                base_text: l.text.clone(),
                                new_text: Some(lines[i + 1].text.clone()),
                            },
                        );
                    }
                    i += 2;
                } else {
                    // Pure delete.
                    if let Some(bl) = l.old_line {
                        out.insert(
                            bl,
                            BaseEntry {
                                changed: true,
                                new_line: None,
                                base_text: l.text.clone(),
                                new_text: None,
                            },
                        );
                    }
                    i += 1;
                }
            }
            DiffOp::Insert => {
                // Pure insert has no base line to key on; skipped here.
                // (The 3-way UI surfaces these via the per-side raw diffs.)
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::diff::{DiffSummary, PageDiff};

    fn line(op: DiffOp, old: Option<u32>, new: Option<u32>, text: &str) -> LineDiff {
        LineDiff {
            op,
            old_line: old,
            new_line: new,
            text: text.into(),
            words: None,
        }
    }

    fn one_page_diff(lines: Vec<LineDiff>) -> DocDiff {
        DocDiff {
            old_path: PathBuf::from("/tmp/base.pdf"),
            new_path: PathBuf::from("/tmp/other.pdf"),
            old_page_count: 1,
            new_page_count: 1,
            pages: vec![PageDiff {
                old_page: Some(1),
                new_page: Some(1),
                lines,
                summary: DiffSummary::default(),
            }],
            total: DiffSummary::default(),
        }
    }

    #[test]
    fn three_way_line_serializes_kind_lowercase() {
        let l = ThreeWayLine {
            kind: ThreeWayKind::Conflict,
            base_line: Some(3),
            mine_line: Some(3),
            theirs_line: Some(3),
            base_text: "x".into(),
            mine_text: Some("y".into()),
            theirs_text: Some("z".into()),
        };
        let j = serde_json::to_string(&l).unwrap();
        assert!(j.contains(r#""kind":"conflict""#));
    }

    #[test]
    fn unchanged_when_both_sides_match_base() {
        // base = mine = theirs = three identical lines.
        let lines = vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Equal, Some(2), Some(2), "beta"),
            line(DiffOp::Equal, Some(3), Some(3), "gamma"),
        ];
        let bm = one_page_diff(lines.clone());
        let bt = one_page_diff(lines);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        assert_eq!(t.total.conflicts, 0);
        assert_eq!(t.total.mine_only, 0);
        assert_eq!(t.total.theirs_only, 0);
        assert_eq!(t.total.unchanged, 3);
    }

    #[test]
    fn mine_only_when_only_mine_diverges() {
        let bm = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA"),
            line(DiffOp::Equal, Some(3), Some(3), "gamma"),
        ]);
        let bt = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Equal, Some(2), Some(2), "beta"),
            line(DiffOp::Equal, Some(3), Some(3), "gamma"),
        ]);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        assert_eq!(t.total.mine_only, 1, "got {:?}", t.total);
        assert_eq!(t.total.theirs_only, 0);
        assert_eq!(t.total.conflicts, 0);
        assert_eq!(t.total.unchanged, 2);
    }

    #[test]
    fn theirs_only_when_only_theirs_diverges() {
        let bm = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Equal, Some(2), Some(2), "beta"),
        ]);
        let bt = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BRAVO"),
        ]);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        assert_eq!(t.total.theirs_only, 1, "got {:?}", t.total);
        assert_eq!(t.total.mine_only, 0);
        assert_eq!(t.total.conflicts, 0);
    }

    #[test]
    fn conflict_when_both_diverge_differently() {
        let bm = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA-MINE"),
        ]);
        let bt = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA-THEIRS"),
        ]);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        assert_eq!(t.total.conflicts, 1, "got {:?}", t.total);
        let l = &t.pages[0]
            .lines
            .iter()
            .find(|l| l.kind == ThreeWayKind::Conflict)
            .unwrap();
        assert_eq!(l.mine_text.as_deref(), Some("BETA-MINE"));
        assert_eq!(l.theirs_text.as_deref(), Some("BETA-THEIRS"));
        assert_eq!(l.base_text, "beta");
    }

    #[test]
    fn both_agree_when_same_change_on_both_sides() {
        let bm = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "DELTA"),
        ]);
        let bt = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "DELTA"),
        ]);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        assert_eq!(t.total.both_agree, 1, "got {:?}", t.total);
        assert_eq!(t.total.conflicts, 0);
    }

    #[test]
    fn pure_delete_on_one_side_classified_as_one_sided() {
        // Mine deletes line 2, theirs leaves it.
        let bm = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Equal, Some(3), Some(2), "gamma"),
        ]);
        let bt = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Equal, Some(2), Some(2), "beta"),
            line(DiffOp::Equal, Some(3), Some(3), "gamma"),
        ]);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        assert_eq!(t.total.mine_only, 1);
        assert_eq!(t.total.unchanged, 2);
        let conflict_line = &t.pages[0]
            .lines
            .iter()
            .find(|l| l.kind == ThreeWayKind::MineOnly)
            .unwrap();
        assert_eq!(conflict_line.base_text, "beta");
        assert_eq!(conflict_line.mine_text, None);
    }

    #[test]
    fn round_trips_through_serde() {
        let bm = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA-MINE"),
        ]);
        let bt = one_page_diff(vec![
            line(DiffOp::Equal, Some(1), Some(1), "alpha"),
            line(DiffOp::Delete, Some(2), None, "beta"),
            line(DiffOp::Insert, None, Some(2), "BETA-THEIRS"),
        ]);
        let t = three_way_from_diffs(&bm, &bt, "/b".into(), "/m".into(), "/h".into());
        let j = serde_json::to_string(&t).unwrap();
        let back: ThreeWayDiff = serde_json::from_str(&j).unwrap();
        assert_eq!(t, back);
    }
}
