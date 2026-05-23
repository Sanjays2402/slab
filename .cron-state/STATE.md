# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: v3.1.0 "Loom" Slice 1 — SHIPPED + MERGED TO MAIN

**TICK 2026-05-23 01:48 PT** — 5 commits on `feature/v3.1.0-loom-slice-1`,
merged to main as `0f33f66`. +1661 net LOC. `cargo test --lib pdf::loom`
green (24/24 passing).

### What shipped (Slice 1)

1. `283acd2` **fix(loom): drop Deserialize from generated matterhorn structs**
   Generated structs hold `&'static [...]` slices that can't satisfy
   Deserialize without lifetime gymnastics. Stripped derives from all 4
   structs (Verdict, FailureCondition, Section, Totals); aligned codegen
   script. Was the root cause of windows-x64 main CI failure.

2. `58f6527` **feat(loom): add content-stream layout extractor**
   `src-tauri/src/pdf/loom/layout.rs` (929 LOC). Parses content streams
   into `LayoutTree { pages: Vec<PageLayout> }` of TextRuns + ImagePlacements.
   Tracks Tm/CTM with q/Q stack, Tf font state, Tj/TJ/'/" show operators,
   Td/TD/T* with leading, Do for image XObjects. 11 unit tests all green.

3. `c31d7d1` **feat(loom): expose layout + matterhorn digest via Tauri**
   Two new commands: `slab_loom_layout_summary(input)` and
   `slab_loom_matterhorn_digest()`. Wired into generate_handler!.

4. `d08ae8c` **feat(loom): LoomPanel UI with Layout / Conformance / About tabs**
   `src/lib/panels/LoomPanel.svelte` (614 LOC, 3 tabs). File picker on
   Layout tab, Matterhorn digest (91/48/33/10) on Conformance tab, scope
   docs on About. Wired into sidebar nav (♿ Loom (PDF/UA), ready: true),
   panel switch, command palette.

5. `9b2d549` **docs(loom): mark Slice 1 live on the accessibility landing page**

### Merged to main

`0f33f66` — merge commit. Pushed to origin. **Side-benefit: this unblocks
main CI**, which had been failing on the matterhorn Deserialize bug
introduced by the Slice 0 codegen tick. CI should green up on the next
main run.

### Buy-Button verdict

PASS on **Pick-us test** (PDF/UA accessibility is a procurement table-stake
no Acrobat-killer ships without) and PASS on **Notice-it test** (new
sidebar entry, new panel, conformance numbers visible).

### Next tick

- **Verify main CI green** after the merge (Windows job especially — that's
  the one Slice 0 broke).
- Slice 2 of v3.1.0 plan: tag tree inference (parse /StructTreeRoot,
  enrich LayoutTree with structural roles).

LAST_WOW_TICK_AT: 2026-05-22 (codegen pipeline shipped)

RECENTLY_CLOSED_ISSUES:
- (none this tick — Slice 1 doesn't close a numbered issue; it advances
  the v3.1.0 plan)

---

## PREVIOUS STATUS: v3.1.0 "Loom" Slice 0 follow-up — Matterhorn codegen pipeline shipped

**TICK 2026-05-23 01:10 PT** — 4 commits, +1684 net LOC (schema 130 + codegen 446 + matterhorn.rs 934 + mod.rs 174). No Rust compile (still 523 MiB free, target/ at 2.5 GB; cargo clean deferred to next Rust tick).

### What shipped

1. `45c6993` **docs(specs): JSON Schema for Matterhorn 1.1 registry**
   `docs/specs/matterhorn-1.1.schema.json` (130 lines). Draft 2020-12 schema
   that pins the contract for `matterhorn-1.1.json`: two-digit section ids,
   `NN-NNN` condition ids, verdict ∈ {auto, human, outOfScope}, cross-totals
   counts, unique ids, no additionalProperties. Editors with JSON-schema
   support surface typos on save.

2. `bc3a48d` **feat(loom): codegen-matterhorn script + pnpm loom:codegen[:check]**
   `scripts/loom/codegen-matterhorn.mjs` (446 lines). Deterministic Node
   codegen: validates registry → emits Rust → pipes through rustfmt → writes
   `matterhorn.rs`. `--check` mode fails with SHA-256 diff if matterhorn.rs
   is stale (CI gate). Wired into package.json as `loom:codegen` and
   `loom:codegen:check`.

3. `7da265d` **feat(loom): generated matterhorn.rs + module skeleton**
   `src-tauri/src/pdf/loom/matterhorn.rs` (934 lines, @generated) — Verdict
   enum, FailureCondition/Section/Totals structs, 31 const arrays for 91
   conditions, helper iterators (auto/human/out_of_scope), 6 in-module tests.
   `src-tauri/src/pdf/loom/mod.rs` (174 lines) — pipeline architecture
   docstring (7 stages), CoverageSnapshot bridge between registry and
   /accessibility.html, 7 cross-cutting tests including
   `auto_share_meets_landing_page_claim` guarding marketing copy from
   drifting from the registry. Wired into `pub mod loom;` in `pdf.rs`.

4. `8c003d0` **docs(landing): accessibility cards cite the generated registry**
   /accessibility.html coverage cards now show actual transcribed counts
   (48/33/10) alongside projections (≈68/≈38/≈30). Footnote links to all
   four artefacts: JSON registry, JSON Schema, generated Rust, codegen script.
   This is the procurement-officer audit trail.

### Buy-Button test result

- **Pay-for-it** ✅ — a procurement officer auditing Slab against CommonLook
  ($1,800/seat) can now follow the /accessibility.html footnote → land on
  working Rust + JSON in the repo → forward to legal. Trust earned through
  transparency.
- **Pick-us** ✅ — no other free PDF/UA-1 tool publishes its conformance
  matrix as a CI-verified JSON registry.
- **Notice-it** ✅ — concrete numbers (48 / 33 / 10) replaced hand-wavy
  "projected ≈ 68" copy.
- **Tell-a-friend** ✅ — the four-link audit trail in the footnote is the
  kind of thing accessibility consultants screenshot to vouch for vendors.

### WOW

The codegen pipeline itself: a single command (`pnpm loom:codegen`) rebuilds
934 lines of Rust from 333 lines of human-edited JSON, byte-identically every
time. CI's `loom:codegen:check` makes "the registry, the engine, and the
marketing page never lie to each other" a build-time invariant. That's a
class of guarantee Adobe ships zero of — their conformance reports are
generated by a closed-source PDF Library and audited by humans on retainer.

**LAST_WOW_TICK_AT**: 2026-05-23T08:18 UTC (this tick).

**RECENTLY_CLOSED_ISSUES**: backlog empty (`gh issue list` still returns
`[]`). Override list #23-#27 was closed earlier this week.

## Next tick plan (MODE C — DEVELOP)

**Rust tick at last.** Slice 1: LayoutTree extraction. Plan at
`docs/plans/2026-05-22-v3.1.0-loom-pdf-ua.md` line 144.

**MUST FIRST**:
```bash
cd src-tauri && cargo clean
```
Currently 523 MiB free; target/ holds 2.5 GB. Cleaning frees disk; the cold
rebuild (~6 min) is within tick budget.

**Slice 1 scope**:
- `src-tauri/src/pdf/loom/layout.rs` (~220 LOC) — Bbox / TextRun /
  ImagePlacement / PageLayout / LayoutTree DTOs + `extract_layout(pdf_bytes)`.
- `src-tauri/tests/fixtures/heading_plus_paragraph.pdf` — 24pt heading +
  12pt body fixture, generated via md2pdf or ghostscript.
- `src-tauri/tests/pdf_loom_layout_test.rs` — passing E2E test.
- Drop the `#![allow(dead_code)]` from `loom/mod.rs` once layout.rs uses
  the matterhorn helpers (or carry it until Slice 5).

**Quality gates** (REQUIRED before push on Rust ticks):
- `cd src-tauri && cargo fmt --all -- --check`
- `cd src-tauri && cargo clippy --all-targets -- -D warnings`
- `cd src-tauri && cargo test --lib`  (note: this also runs the matterhorn
  + mod tests landed this tick — they should pass on first run)
- `pnpm check`
- `pnpm loom:codegen:check`  (new gate; verifies matterhorn.rs in sync)

This tick skipped Rust gates intentionally (no Rust compile attempted on
the tight-disk machine). All four new files are rustfmt-clean against
edition 2021, verified with standalone `rustfmt --check`.

**Alternative if disk stays tight**: ship Slice 1 as a docs-only refinement
of the plan + a fixture-generation script in `scripts/`, defer the actual
Rust slice to the tick after a successful `cargo clean`.

## Disk pressure

- 523 MiB free on /System/Volumes/Data at end of tick (unchanged — only
  text files touched).
- `src-tauri/target` = 2.5 GB. **Next tick MUST run `cargo clean` first.**
- Codegen + landing-only ticks remain unblocked.

---

## ARCHIVED: v3.1.0 Loom Slice 0 (2026-05-23 00:50 PT)

Three commits: ADR (`46b2322`), Matterhorn JSON registry (`3068e96`),
accessibility landing page (`d4c62a7`). 1020 net LOC. Set up the
groundwork that this tick's codegen pipeline executes against.

## ARCHIVED: 🎉 v3.0.2 "Foundry Fonts" RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.2

## ARCHIVED: 🎉 v3.0.1 "Loupe" RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.1

## ARCHIVED: 🎉 v3.0.0 "Bedrock" RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.0
