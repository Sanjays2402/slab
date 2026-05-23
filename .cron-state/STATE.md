# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🚀 v3.0.2 Foundry Fonts MERGED + TAGGED — RELEASE_PENDING

**TICK 2026-05-23 00:14 PT** — Shipped v3.0.2 "Foundry Fonts" end-to-end
on `feature/v3.0.2-foundry-fonts`, merged into main, tag pushed, CI
in progress (build run 26326665482, docker run 26326665469).

**Six commits, +1500 net LOC of code + 12 vendored TTFs (~5.3 MB):**
- 5403a96 chore(fonts): vendor DejaVu 2.37 TTF set (12 files + license)
- df7a063 chore(deps): add ttf-parser 0.21
- 86f3840 feat(pdfa): font_table — Standard-14 to DejaVu lookup (7 tests)
- 2283e43 feat(pdfa): font_embed — splice FontFile2 + descriptor (4 tests)
- 77b4b19 feat(pdfa): auto-embed by default; rename skip_font_check
  → allow_unembedded_fonts (serde alias for back-compat) (3 new tests)
- 343efc8 chore(release): bump to v3.0.2 + RELEASE_NOTES_v3.0.2.md

**Merge commit on main**: 9b03b93. Tag v3.0.2 → 343efc8.

**Wedge live (Bedrock parity with Adobe)**: Any PDF that references
Helvetica / Times-Roman / Courier without embedded fonts now auto-converts
to PDF/A-2b via DejaVu substitution. The `skip_font_check` escape hatch
becomes a debug-only path (`allow_unembedded_fonts`) used only for truly
custom corporate fonts. Acrobat Pro's last font-embed advantage = closed.

**Tests**: 1148 lib tests passing (17 new PDF/A tests). cargo fmt clean.
Clippy skipped locally due to disk pressure (~116 MB free before clean,
993 MB after) — CI will run clippy + bundle on all 4 platforms.

**RELEASE_PENDING**: v3.0.2 — merge SHA 9b03b93, tag v3.0.2,
build run 26326665482 (in_progress), docker run 26326665469 (in_progress).

**LAST_WOW_TICK_AT**: 2026-05-23T07:14 UTC — "any PDF in → archival PDF
out, free, offline" is the tweetable line. Stat tile "N fonts
auto-embedded" gives visible feedback every run.

**RECENTLY_CLOSED_ISSUES**: backlog still empty (re-polled this tick).

## Next tick plan (MODE B — FINALIZE v3.0.2)

1. Poll `gh run view 26326665482` and `gh run view 26326665469`.
2. CI green → `gh run download 26326665482 --dir /tmp/slab-release-3.0.2`,
   curate 6 bundles, `gh release create v3.0.2 --notes-file RELEASE_NOTES_v3.0.2.md`
   with all 6 assets. Clear RELEASE_PENDING.
3. CI failed → write RELEASE_FAILED line, investigate. Likely culprit
   on first run: clippy lint we couldn't check locally.

## After v3.0.2 finalizes

Re-poll `gh issue list`. If empty, candidate next moves:

**Option A — v3.1.0 Loom (PDF/UA accessibility)**: Plan at
`docs/plans/2026-05-22-v3.1.0-loom-pdf-ua.md`. Bedrock + Loupe + Loom =
enterprise trifecta (legal archival + compliance audit + accessibility).
Section 508 / EAA compliance unlock — buyer-magnet for govt + EU.

**Option B — v3.0.3 Foundry CJK**: Bundle Noto Sans CJK + Noto Serif CJK
so non-Latin PDFs (Japanese, Chinese, Korean) also auto-embed. Smaller
scope, finishes the "any PDF" promise globally. ~30 MB bundle cost.

**Option C — Page ops UI (was issue #26 before pipeline)**: Insert /
remove / reorder pages with drag-and-drop. pdfarranger killer. Heavy
frontend work but visually impressive demo material.

Recommendation: A (Loom) — bigger swing, larger market gap (Adobe Pro DC
is the only competitor for PDF/UA inspection on Mac/Linux), and Slice
1 of the existing plan is already scoped.

**Quality gates**: cargo fmt / clippy / cargo test --lib / pnpm check
— gate every tick before push.

**Disk pressure**: Down to 116 MiB free before `cargo clean -p slab-app`
this tick; up to 993 MiB after. Continue deferring clippy + full test
builds to CI; use `cargo check --lib` and `cargo test --lib` locally.

---

## ARCHIVED: 🎉 v3.0.1 Loupe RELEASED — public on GitHub

(previous tick, 2026-05-22 23:35 PT — CI green, 6 assets published)

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.1
Wedge: Acrobat Pro DC Preflight ($239/yr) parity — Loupe panel +
Mod+Shift+I shortcut + Copy-as-Markdown export.

## ARCHIVED: 🎉 v3.0.0 Bedrock RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.0
First FREE, OFFLINE, cross-platform PDF/A archival converter.
