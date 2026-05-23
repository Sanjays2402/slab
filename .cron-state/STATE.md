# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🎉 v3.0.2 RELEASED + landing-page upgrade live

**TICK 2026-05-23 00:50 PT** — Two things shipped:

### 1. v3.0.2 "Foundry Fonts" RELEASED to GitHub

CI run 26326665482 went green (all 4 platforms), artifacts downloaded
and curated, release published with 5 installers:

- macOS arm64 DMG (Slab_3.0.2_aarch64.dmg)
- macOS x64 DMG   (Slab_3.0.2_x64.dmg)
- Linux AppImage  (Slab_3.0.2_amd64.AppImage)
- Windows MSI     (Slab_3.0.2_x64_en-US.msi)
- Windows NSIS    (Slab_3.0.2_x64-setup.exe)

(.deb not produced by this run — only AppImage on linux pipeline. Follow-up:
re-enable deb in `.github/workflows/build.yml` linux job. Tracked verbally.)

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.2
`RELEASE_PENDING` cleared.

### 2. Landing-page upgrade — 4 commits, ~580 net LOC HTML/CSS/SVG/JSON

Off the back of disk pressure that made another big Rust slice unwise,
this tick pivoted to the marketing funnel which monetizes the v3.0.x
compliance trio (Bedrock + Loupe + Foundry Fonts). All four commits
pass the Buy-Button test on **Notice-it** (returning visitors see new
content) and **Tell-a-friend** (social card now unfurls correctly).

- 6f6a463 feat(landing): animated 5-second hero demo SVG (drop → OCR
  → redact → PDF/A badge). Pure SMIL, ~9 KB, honors reduced-motion.
  This is the in-product equivalent of closed issue #27 (5s demo video)
  but generated rather than recorded — ships without a capture session.
- 23e3a50 feat(landing): "The v3.0 Compliance Suite" section + 6-Q FAQ
  + sticky-nav anchors (Compliance, FAQ added).
- aff26b2 feat(landing): /enterprise.html — live TCO calculator
  (default 500 seats × $239 × 3 yrs = $358,500 vs Slab $0), per-OS
  signed-installer deployment recipes (Jamf/Munki/Mosyle/SCCM/Intune/
  GPO/apt one-liners), and 8-item security checklist.
- ef9edf6 feat(landing): SEO meta, Open Graph, Twitter cards, sitemap,
  robots, JSON-LD SoftwareApplication schema, 1200×630 social-card.svg.

Pushed to main (c86c77a..ef9edf6).

**WOW**: The hero-animated.svg loop — five named beats, purple PDF/A
badge stamps in at 4.5s. It's the tweetable visual we needed on the
homepage. **LAST_WOW_TICK_AT**: 2026-05-23T07:50 UTC.

**RECENTLY_CLOSED_ISSUES**: backlog still empty (re-polled, 0 issues).
The override list (#23-#27) closed previously — pipeline is fully open.

## Next tick plan (MODE C — DEVELOP)

Disk pressure remains the binding constraint (~690 MiB free).
Recommended options:

**Option A — v3.1.0 Loom (PDF/UA accessibility), Slice 0**: 1148-LOC
plan exists. Slice 0 is pure documentation (ADR + Matterhorn enum) —
zero Rust compile, no disk impact. Sets up the next compliance wedge
which is the $400M/yr Section 508 / EAA market with no free competitor.

**Option B — `cargo clean` + v3.1.0 Loom Slice 1**: Frees ~2.5 GB by
nuking target/, then ships LayoutTree extraction (Rust, ~220 LOC) with
a proper compile budget. Higher payoff but a full rebuild burns the
tick's compile budget.

**Option C — landing-page screenshots + product hunt prep**: Capture
real Slab screenshots (requires running the app — can't in headless
cron) and prep launch copy. Defer until Sanjay is at the keyboard.

Recommendation: **A** — Slice 0 of Loom is the cleanest non-compile
move. Documents the spec, lands the Matterhorn checkpoint table,
sets up Slice 1 for a future tick when disk allows.

**Quality gates**: cargo fmt / clippy / cargo test --lib / pnpm check
— gate every Rust tick before push. Landing-only ticks skip (no Rust touched).

**Disk pressure**: 690 MiB free. Next Rust tick should start with
`cargo clean -p slab-app` or full `cargo clean`. ttf-parser + lopdf
rebuild is ~90s warm, ~6min cold.

---

## ARCHIVED: 🎉 v3.0.1 Loupe RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.1
Wedge: Acrobat Pro DC Preflight ($239/yr) parity — Loupe panel +
Mod+Shift+I shortcut + Copy-as-Markdown export.

## ARCHIVED: 🎉 v3.0.0 Bedrock RELEASED

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.0.0
First FREE, OFFLINE, cross-platform PDF/A archival converter.
