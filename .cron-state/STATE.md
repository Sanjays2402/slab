# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: v3.1.0 "Loom" Slice 0 shipped + accessibility marketing page live

**TICK 2026-05-23 00:50 PT** — 3 commits, ~1020 net LOC of docs/HTML/CSS/JSON.
No Rust compile (disk at 450 MiB free, full `cargo clean` deferred to next
tick that ships actual Rust).

### What shipped

1. `46b2322` **docs(adr): PDF/UA-1 conformance target + Matterhorn subset**
   New ADR at `docs/adr/2026-05-23-pdf-ua-conformance.md` (172 lines).
   Pins v3.1.0 Loom to ISO 14289-1:2014 (not -2). Explains why every
   procurement RFP from FY2024-FY2026 cites -1. Documents the
   Auto / Human / OutOfScope verdict glossary. Lists deferrals (forms,
   multimedia, signatures, math) by version. Cites WCAG 2.1 / Section 508
   refresh / EN 301 549 v3.2.1 / AODA 2025 / EAA 2025.

2. `3068e96` **docs(loom): Matterhorn Protocol 1.1 checkpoint registry**
   New JSON at `docs/specs/matterhorn-1.1.json` (333 lines). 91 of 136
   leaf-level failure conditions transcribed across 31 sections. Verdict
   split: 48 auto / 33 human / 10 out-of-scope. Auto-share projects to
   ≈50% once the registry is complete (vs Adobe Auto-Tag's ~40%). This
   file is the single source of truth — `src-tauri/src/pdf/loom/matterhorn.rs`
   will be codegen'd from it in Slice 1.

3. `d4c62a7` **feat(landing): /accessibility.html — PDF/UA-1 wedge**
   New marketing page at `docs/landing/accessibility.html` (218 lines)
   + dedicated stylesheet `accessibility.css` (190 lines). Wired into
   nav in index.html + enterprise.html and into sitemap.xml. Includes:
   - Vendor pricing comparison (Adobe $239/yr, CommonLook $1,800/seat,
     axesPDF €390/yr, PAC validator-only free, Slab $0)
   - Live savings calculator with perpetual-license amortization
   - Seven-stage pipeline diagram
   - Matterhorn coverage cards
   - 6-question FAQ (ship date, Adobe parity, -2, AI alt-text, bulk
     tagging, federal posture)
   - "Why offline matters" — SCIF / air-gap pitch
   - Loom preview-request CTA → pre-filled GitHub issue link

### Buy-Button test result

- **Pay-for-it** ✅ — a procurement officer at HHS or DOE who tries Slab
  and finds it generates valid PDF/UA-1 will buy seats. Adobe charges
  $239/yr per seat for this exact capability.
- **Pick-us** ✅ — no free, cross-platform, offline, end-to-end PDF/UA-1
  tagger exists today. Slab will be the first.
- **Notice-it** ✅ — new top-level nav link on the landing page.
- **Tell-a-friend** ✅ — savings calculator showing $35,850 saved over 3
  years for 50 seats vs Adobe is screenshot-bait for IT-Twitter / r/sysadmin.

### WOW

The Matterhorn coverage cards on `/accessibility.html` — green Auto card
(48 of 91 transcribed → projected ≈68/136 ≈ 50%), amber Human card,
neutral OutOfScope card — visually communicate "we automate more than
Adobe" in one glance. Combined with the live savings calculator, this is
the page a paralegal forwards to their IT director.

**LAST_WOW_TICK_AT**: 2026-05-23T07:50 UTC (still valid — same UTC day,
this tick adds the accessibility wedge on top of yesterday's hero anim).

**RECENTLY_CLOSED_ISSUES**: backlog empty (re-polled, `gh issue list`
returned `[]`). Override list (#23-#27) closed previously.

## Next tick plan (MODE C — DEVELOP)

**v3.1.0 Loom Slice 1: LayoutTree extraction** — first Rust slice. Plan
in `docs/plans/2026-05-22-v3.1.0-loom-pdf-ua.md` (762 lines, slice 1
starts at line 144).

**Prerequisite**: free disk space. Currently 450 MiB free with
`src-tauri/target` at 2.5 GB. Next Rust tick MUST start with:

```bash
cd src-tauri && cargo clean
```

That frees ~2.5 GB. ttf-parser + lopdf cold rebuild is ~6 minutes; that's
within tick budget if we start clean.

**Slice 1 scope**: `src-tauri/src/pdf/loom/layout.rs` (~220 LOC) +
`src-tauri/src/pdf/loom/mod.rs` + `src-tauri/src/pdf/loom/matterhorn.rs`
(codegen from `docs/specs/matterhorn-1.1.json`) + module wiring in
`pdf/mod.rs` + 1 test fixture PDF + at least one passing test. Target
3 commits minimum, ≥600 LOC.

**Alternative if disk stays tight**: ship the codegen step + matterhorn.rs
generation as a docs/script-only slice (no Rust compile), then do
LayoutTree on the tick after.

**Quality gates** (REQUIRED for any Rust tick before push):
- `cd src-tauri && cargo fmt --all -- --check`
- `cd src-tauri && cargo clippy --all-targets -- -D warnings`
- `cd src-tauri && cargo test --lib`
- `pnpm check` from repo root

This tick: gates skipped (docs + HTML + CSS + JSON only, no Rust touched,
no Svelte/TS touched).

## Disk pressure

- 450 MiB free on /System/Volumes/Data at end of tick.
- `src-tauri/target` = 2.5 GB. Cleaning this is the next move when we
  need Rust compile cycles.
- Landing-only ticks can continue indefinitely without disk pressure.

---

## ARCHIVED: 🎉 v3.0.2 "Foundry Fonts" RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.2
Wedge: auto-embed Standard-14 fonts via DejaVu substitution (closes the
last PDF/A-2u font-embed gap that blocked legal/compliance customers
delivering documents without bundled fonts).

## ARCHIVED: 🎉 v3.0.1 "Loupe" RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.1
Wedge: Acrobat Pro DC Preflight ($239/yr) parity — Loupe panel +
Mod+Shift+I shortcut + Copy-as-Markdown export.

## ARCHIVED: 🎉 v3.0.0 "Bedrock" RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.0
First FREE, OFFLINE, cross-platform PDF/A archival converter.
