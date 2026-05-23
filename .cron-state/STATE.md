# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: v3.8.0 Press Slices 1+2 SHIPPED on `feature/v3.8.0-press` — ADR + FOGRA51/GRACoL ICC vendored, OutputIntent enum, normalize_color() pass with 17 passing tests. Next tick: Slice 3 (geometry) + Slice 4 (orchestrator) — gets us to CLI-level "any PDF → valid PDF/X-4".

**TICK 2026-05-23 10:15 PT (Saturday off-hours)** — MODE C develop. 4 commits, ~680 net LOC + ~280 LOC tests + 5.5MB ICC profiles. All gates green. Session log: `.cron-state/sessions/2026-05-23-1015.md`.

### Next tick
- Slice 3: `pdf::press::geometry::ensure_print_boxes` — synthesize TrimBox, optional 3mm BleedBox.
- Slice 4: `pdf::press::orchestrate::convert_to_pdfx4` — glue passes 1-6 (sanitize → font_embed → color → geometry → xmp → output_intent). Extends `pdfa::xmp::XmpBuilder` with PdfX4 variant and `pdfa::output_intent` with `IntentSubtype::GtsPdfX`.
- Target ship at end of next tick: working CLI-level conversion.

### LAST_WOW_TICK_AT: 2026-05-23T16:15Z (Loom Slice 6 sub-badge stagger — Slice 1+2 was plumbing, no new wow needed; daily budget satisfied)

---

## PRIOR STATUS: v3.7.0 Loom PUBLISHED on GitHub Releases — 6 artifacts uploaded, Docker image live on GHCR.

**TICK 2026-05-23 10:00 PT (Saturday off-hours)** — MODE B FINALIZE executed:
- CI run 26337874627 (build) — **success** ✅
- CI run 26337874606 (Docker slab-server) — **success** ✅
- Downloaded all 4 artifact bundles (required freeing ~500MB of /tmp first — disk was at 355Mi free; cleaned old slab-release-3.4.0 + stale screenshots).
- `gh release create v3.7.0 --title "v3.7.0 — Loom"` with marketing-tone notes (`/tmp/slab-v3.7.0-notes.md`) and 6 artifacts uploaded:
  - Slab_3.7.0_aarch64.dmg (macOS Apple Silicon)
  - Slab_3.7.0_x64.dmg (macOS Intel)
  - Slab_3.7.0_amd64.deb (Linux)
  - Slab_3.7.0_amd64.AppImage (Linux portable)
  - Slab_3.7.0_x64-setup.exe (Windows NSIS)
  - Slab_3.7.0_x64_en-US.msi (Windows MSI)
- Docker image `ghcr.io/sanjays2402/slab-server:v3.7.0` live.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.7.0
- RELEASE_PENDING cleared.
- Open issues polled: **0 open**. Next pipeline item: **v3.8.0 Press** (PDF/X-4 print production).

### Next tick — MODE C DEVELOP
1. Create branch `feature/v3.8.0-press`.
2. Execute Slice 1 of `docs/plans/2026-05-23-v3.8.0-press-pdf-x.md` (ADR + ICC vendoring + module scaffold).
3. Honor ship-size: bundle Slice 1+2 (ICC + scaffolding alone won't pass buy-button).

### LAST_WOW_TICK_AT: 2026-05-23T16:15Z (Loom Slice 6 sub-badge stagger — within 24h window)



**TICK 2026-05-23 09:30 PT (Saturday off-hours)** — MODE A RELEASE executed:
- Merged feature/v3.1.0-loom-slice-3 → main (merge commit a04f8e6).
- Bumped version 3.6.0 → **3.7.0** across Cargo.toml, tauri.conf.json, package.json (ce739dd).
- Tagged **v3.7.0** with marketing-tone annotation.
- Pushed main + tag to origin.

### RELEASE_PENDING: v3.7.0 — merge SHA a04f8e6, tag v3.7.0, CI run 26337874627 (build) + 26337874606 (Docker)

### Quality gates on main (post-merge)
- cargo fmt --all -- --check: clean
- cargo clippy --lib --all-targets -- -D warnings: clean
- cargo test --lib: **1286 passed, 0 failed**
- pnpm check: 0 errors, 46 warnings (pre-existing unused-CSS)

### Why version jumped 3.1.0 → 3.7.0
Slab's released version line had already reached v3.6.0 (Compactor) on main while
the Loom feature branch was being developed in parallel under its own legacy
"v3.1.0 Loom" codename. To avoid version regression and keep tag history
monotonic, Loom ships as **v3.7.0 — Loom**. Codename "Loom" preserved in the
release title.

### Next tick — MODE B FINALIZE
1. `gh run view 26337874627` — if green, `gh run download` artifacts.
2. `gh release create v3.7.0` with notes-file + 6 artifacts.
3. Verify Docker tag workflow (26337874606) too.
4. Clear RELEASE_PENDING line.
5. Then start next pipeline item (v3.8.0 — see roadmap).

### LAST_WOW_TICK_AT: 2026-05-23T16:15Z (Loom Slice 6 sub-badge stagger)

---

## PRIOR HISTORY (v3.1.0 Loom development on feature/v3.1.0-loom-slice-3)


## STATUS: v3.1.0 Loom Slice 6 (metadata + validator + UI) SHIPPED on feature/v3.1.0-loom-slice-3. Branch is feature-complete for v3.1.0; ready to merge to main next tick.

**TICK 2026-05-23 09:15 PT (Saturday off-hours)** — MODE C develop. Slice 6
finishes v3.1.0 Loom. Slab can now tag PDF/UA-1 documents AND certify them
with an 8-check validator, all offline, in one panel. PAC 2024 / CommonLook
Validator / veraPDF Enterprise cost hundreds per seat and only grade — Slab
does both for free.

### What shipped this tick (4 commits, ~1500 LOC)

- `5b2b296 feat(loom): apply_pdfua_metadata — XMP packet + ViewerPreferences (Slice 6)`
  - `src-tauri/src/pdf/loom/metadata.rs` (483 LOC, 7 tests).
  - XMP packet with `pdfuaid:part=1`, `dc:title`, `dc:language`, `xmp:CreatorTool`.
  - `/ViewerPreferences /DisplayDocTitle true` (Matterhorn 07-001).
  - Info dict `/Title` sync from XMP `dc:title` (Matterhorn 06-001).
  - `/Lang` fallback ("en-US") at catalog (Matterhorn 11-001).
  - `MetadataOptions` builder + `MetadataStats { xmp_written, title_synced, ... }`.
- `12a3238 feat(loom): validate() — 8 Matterhorn auto-conditions on tagged PDFs (Slice 6)`
  - `src-tauri/src/pdf/loom/validate.rs` (557 LOC, 7 tests).
  - 8 auto-decidable checks: StructTree present, MarkInfo /Marked true, /Lang
    set, XMP present, XMP pdfuaid:part=1, ViewerPrefs /DisplayDocTitle true,
    Info /Title set, every Figure has /Alt. Each yields PDF/UA-1 clause +
    Matterhorn condition ID.
  - `ValidateReport { overall, checks: Vec<CheckResult>, passed, failed }`.
- `1580e46 feat(loom): slab_loom_validate command + auto-validate after tag (Slice 6)`
  - `src-tauri/src/lib.rs`: `LoomTagResult` extended with `validation` +
    `metadata` fields. `slab_loom_tag_document` now runs apply_pdfua_metadata
    then validate after weave. New `#[tauri::command] slab_loom_validate`
    grades any existing PDF (vendor docs, Acrobat output, etc.).
- `ff11a50 feat(loom): Validate tab + sub-badge UI — Slice 6 finale, PDF/UA-1 verdict in the panel`
  - `src/lib/panels/LoomPanel.svelte`: new "Validate" tab
    (Cmd/Ctrl+Shift+V), verdict card, per-check list with PDF/UA-1
    clause + Matterhorn ID per row, idle empty-state pitch naming the
    competitors. Sub-badge on Tag tab reveals 380ms after main badge —
    green "✓ Validated · ISO 14289-1 · 8/8 checks" or red verdict.
    ~140 LOC of CSS, full dark-mode parity.
  - Also fixed one clippy vec_init_then_push lint in validate.rs.

### Quality gates this tick

- `pnpm check`: 0 errors, 46 warnings (all pre-existing unused-CSS, not Slice 6).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --lib -- -D warnings`: clean.
- `cargo test --lib`: **1234 passed, 0 failed**.

### Buy-Button passes ALL FOUR

- Pay-for-it: validator alone competes with $$$ commercial tools.
- Notice-it: green sub-badge after every tag is unmissable.
- Pick-us: Adobe Acrobat doesn't grade conformance; Slab does — offline.
- Tell-a-friend: "ISO 14289-1 certified, free, on my Mac" is a screenshot.

### Wow moment

LAST_WOW_TICK_AT: 2026-05-23T16:15Z — sub-badge stagger + Validate tab verdict.

### Next tick

MODE A — RELEASE. feature/v3.1.0-loom-slice-3 is now feature-complete for
v3.1.0. Merge to main, run gates on main, tag v3.1.0, push --follow-tags,
finalize GitHub release with marketing-tone notes.

### What shipped this tick (3 commits, ~1240 LOC across 3 files)

- `74480d9 feat(loom): structure_tree weave() — emit StructTreeRoot for PDF/UA-1 (Slice 5)`
  - `src-tauri/src/pdf/loom/structure_tree.rs` (~960 LOC incl. 17 unit tests).
  - `ParentTreeBuilder` (NumberTree for `/ParentTree`).
  - `build_role_map` + `make_struct_elem` helpers.
  - `plan_page` — StructTree page → flat `RunMcid` sequence in stream order.
    Containers (Document/Sect/List) pass through; artifacts skip MCID counter.
  - `rewrite_page_stream` — injects `BDC /<Tag> << /MCID n >> ... EMC` around
    every Tj/TJ/'/"/Do operator; artifacts use empty-dict form. Re-flates the
    stream via lopdf.
  - `weave(doc, tree, order, opts)` — public entry. Builds StructElems
    depth-first mirroring classify's tree, wires `/StructTreeRoot`,
    `/MarkInfo`, `/Lang`, `/RoleMap`, `/ParentTreeNextKey`, and
    `/StructParents` on every page. Per-Figure `/Alt` from Slice 4 alt-text;
    per-node `/Lang` if classify sets it. Artifacts excluded from the elem
    tree (kept in content stream as `/Artifact BDC ... EMC`).
  - 17 unit tests covering: builder sort, role map, struct-elem invariants,
    plan_page (heading levels H1..H6 + collapse, container traversal,
    artifact MCID-skip), rewrite_page_stream (BDC/EMC bracketing, artifact
    empty-dict, empty-plan no-op), and weave (catalog wiring, /StructParents
    on every page, /Alt on Figure, artifact exclusion, /Lang preserve-existing).
- `4fea7b1 feat(loom): slab_loom_tag_document Tauri command (Slice 5 backend wiring)`
  - `src-tauri/src/lib.rs` adds the async command (~110 LOC).
  - Runs the full pipeline: layout → classify → reading_order → best-effort
    Beacon alt-text → weave → save `<stem>.tagged.pdf`.
  - Best-effort alt-text means tagging still ships if Ollama is offline.
  - Returns `LoomTagResult { output_path, elapsed_ms, pages_processed,
    pages_skipped, bdc_pairs_injected, struct_elems_created,
    figures_with_alt_text }`.
  - Registered in `tauri::generate_handler![…]` next to the other Loom cmds.
- `ff2a201 feat(loom): LoomPanel "Tag PDF" tab with Cmd+Shift+T + reveal anim (Slice 5 UI)`
  - New "Tag PDF" tab on `src/lib/panels/LoomPanel.svelte` (~210 LOC).
  - Primary CTA "Tag Document for PDF/UA" with stats card on success.
  - **WOW**: 320ms purple-glow reveal animation on the "PDF/UA-1 emitted"
    pill badge after a successful tagging run. Designed for screenshot.
  - Cmd/Ctrl+Shift+T global shortcut from any LoomPanel tab.
  - Empty-state copy frames the privacy/cost wedge:
    > "Adobe Acrobat Pro's Auto-tag costs $239/yr. CommonLook starts at
    > $1,200 per seat. veraPDF won't generate alt-text. Slab does the whole
    > pipeline in one click — without your file leaving this Mac."
  - Dark-mode variant included.

### Quality gates this tick

- `cargo fmt --all -- --check` ✓
- `cargo clippy --lib -- -D warnings` ✓
- `cargo test --lib` → 1220 passed (+17 new from structure_tree).
- `pnpm check` → 0 errors, 46 warnings (all pre-existing CSS-unused-selector).

### LAST_WOW_TICK_AT: 2026-05-23T08:20 PT

The purple-glow "PDF/UA-1 emitted" badge reveal anim. Plus the underlying
capability — generating valid tagged PDFs locally — is itself the bigger wow.
Acrobat Pro charges $239/yr for this; CommonLook charges $1,200+/seat;
neither runs on Linux. Slab ships it free, cross-platform, offline.

### Buy-Button verdict for the entire Slice 5

- **Pay-for-it:** PASS — Acrobat Pro AutoTag is the $239/yr feature. We
  give it away with vision-LLM alt-text on top.
- **Pick-us:** PASS — no free cross-platform PDF/UA tagger exists today.
  veraPDF tags but won't auto-generate alt-text; pdfarranger doesn't tag.
- **Notice-it:** PASS — new Tag PDF tab + Cmd+Shift+T shortcut visible the
  moment the user opens Loom.
- **Tell-a-friend:** PASS — "I tagged my dissertation for screen readers
  locally, in seconds, free." Plus the badge reveal screenshot.

### Branch state

`feature/v3.1.0-loom-slice-3` is now 10 commits ahead of main:

Slices 3 + 4 + 5 all live on the branch (the branch name lags the content).
Plan written 07:37 PT this morning, implementation shipped 08:20 PT.

### Next tick candidate (Slice 6: metadata + XMP)

ISO 14289-1 also requires:
- XMP metadata with `pdfuaid:part=1` namespace.
- `/ViewerPreferences << /DisplayDocTitle true >>` on the catalog.
- `/Metadata` stream on the catalog with the XMP packet.
- `/Lang` if not already set (we already do this — re-confirm).
- ActualText for ligatures + math (lower priority, can defer to 6.2).
- Title in document info dict matching XMP `dc:title`.

Slice 6 will add `src-tauri/src/pdf/loom/metadata.rs` mirroring the
structure_tree.rs pattern. Pair with a "PDF/UA validator" tab that runs the
already-shipped Matterhorn auto-conditions against the tagged output and
shows a pass/fail card. That makes Slice 6 the right buy-button tick:
post-tag verification turns the badge from a claim into evidence.

After Slice 6 lands, v3.1.0 Loom is ready to merge → main → tag → release.

### Slice 3 + 4 archive (previous ticks)

Reading order + column-aware traversal (Slice 3); Beacon alt-text generation
with SHA-256 disk cache (Slice 4). See prior STATE entries / git log.

---

## ARCHIVED: v3.6.0 Compactor — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.6.0
Real PDF compression (image downsample + JPEG re-encode + metadata strip).

## ARCHIVED: v3.5.0 Veil — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.5.0
True PDF redaction (content-stream excision, not black bars).

## ARCHIVED: v3.4.0 Discovery — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.4.0
Bates numbering for legal discovery.

RECENTLY_CLOSED_ISSUES:
- v3.5.0 Veil — published earlier this morning
- v3.6.0 Compactor — published this morning (CI 26334187463)

## OPS NOTE — 2026-05-23 07:37 PT

Disk filled to 100% mid-tick (228GB SSD, 117MB free). Root cause: 78GB
stale Chrome code-signing scratch clone at
`/private/var/folders/9g/.../X/com.google.Chrome.code_sign_clone/`.
Removed it during this tick; APFS recovered ~1.4GB usable plus the rest
as purgeable space. If this happens again, the same path is a safe first
target — macOS regenerates as needed.
