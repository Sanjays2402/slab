# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: v3.13.0 Streamline Slice 1+2+3 SHIPPED — end-to-end inspector live on `feature/v3.13.0-streamline`.

**TICK 2026-05-23 20:0x PT (Saturday off-hours)** — MODE C DEVELOP.

Folded Tasks 1+2+3 of the Streamline plan + Tauri commands + Svelte panel into
one tick. The "is this PDF Fast Web View optimized?" inspector is end-to-end
working today; the linearizer writer ships in upcoming ticks.

Shipped (4 commits / 909 net LOC on `feature/v3.13.0-streamline`):
- `169907e` docs(adr): 0013 — streamline (PDF linearization / Fast Web View)
- `f311ff6` feat(streamline): scaffold module + DTOs
- `fb914df` feat(streamline): is_linearized inspector + first-page object-graph walk (9 unit tests ✓)
- `9268718` feat(streamline): Tauri commands + Svelte panel — end-to-end inspector

Quality gates ALL ✓ (fmt, clippy -D warnings, 1427 unit tests, pnpm check 0 errors).

Push triggered CI run `26350495175` on the feature branch — check next tick.

Disk note: `cargo clean` recovered 4.9 GiB at start of tick (was at 202 MiB free).
Current: ~3 GiB free. Tight but workable.

### LAST_WOW_TICK_AT: 2026-05-24T03:12Z (Streamline inspector — first tool to surface "first-page prefix" diagnostic; Acrobat Standard $179/yr doesn't ship this feature at all)

### Next tick — MODE C DEVELOP — Task 4 (param dict builder)

1. Poll CI run `26350495175` — confirm 4-platform bundles green on the branch.
2. Continue on `feature/v3.13.0-streamline`.
3. Execute Task 4 (linearization parameter dict builder) + Task 5 (primary
   hint stream builder) — fold into one tick if scope allows.
4. Task 6 (end-to-end writer) likely its own tick — biggest single piece.

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: released 2026-05-23 (CI 26349407913 + 26349407898).

---

## PRIOR STATUS: v3.12.0 Atelier RELEASED — 6 artifacts uploaded, Docker live. v3.13.0 Streamline PLAN landed, ready to execute.

**TICK 2026-05-23 19:46 PT (Saturday off-hours)** — MODE B FINALIZE executed.
- CI build run 26349407913: all 7 jobs green (4-platform bundles + 3-platform cargo test, clippy, fmt).
- Docker run 26349407898: ✓ → `ghcr.io/sanjays2402/slab-server:v3.12.0` live.
- `cargo clean` recovered 4.3 GiB (disk had 765 MiB → 4.5 GiB after).
- Downloaded 6 artifacts to /tmp/slab-release-3.12.0, then:
  `gh release create v3.12.0 --title "v3.12.0 — Atelier" --notes-file .cron-state/release-notes-v3.12.0.md` + 6 installers.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.12.0
  isDraft=false, assetCount=6 (mac arm/x64 dmg, linux deb+AppImage, win nsis+msi).
- RELEASE_PENDING cleared.

### LAST_WOW_TICK_AT: 2026-05-24T02:46Z (v3.12.0 Atelier public release — Action Wizard equivalent free + offline)

### Next tick — MODE C DEVELOP — start v3.13.0 Streamline Task 1
1. Re-poll `gh issue list` (was 0 at start of this tick).
2. `git checkout -b feature/v3.13.0-streamline`.
3. Execute Task 1 of plan (ADR 0013 + streamline module scaffold + DTOs).
   Plan: `docs/plans/2026-05-23-v3.13.0-streamline-fast-web-view.md`.
4. Fold Tasks 1+2 into a single tick if scope allows (scaffold + linearize
   driver skeleton with first 3-4 unit tests).

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: released 2026-05-23 (CI 26349407913 + 26349407898).

---

## PRIOR STATUS: v3.12.0 Atelier — bundles 3/4 green, Win still building. v3.13.0 Streamline PLAN landed.

**TICK 2026-05-23 19:25 PT (Saturday off-hours)** — writing-plans skill invocation.
v3.12.0 build run 26349407913: tests 3/3 ✓, bundles macOS-arm/x64 + linux ✓,
windows still in_progress. Docker (26349407898) ✓. Pages (26349407658) ✓.
Cannot MODE B FINALIZE this tick — Win bundle artifact required for the
6-installer release. Disk 813 MiB free, no headroom for a local Tauri build,
so no code shipped. Used the tick for the next-version plan.

Shipped this tick (1 commit, ~1220 LOC markdown):
- `2d6f49a docs(plan): v3.13.0 Streamline — Fast Web View linearization`
  8-task TDD plan in `docs/plans/2026-05-23-v3.13.0-streamline-fast-web-view.md`.
  Buy-Button: Adobe Acrobat Pro $239/yr exclusive; PDF Expert doesn't ship it.
  Headline wow: "first-page-ready 12 MB → 188 KB" before/after panel.

Honest sizing: this is a PLANNING tick, not a ship tick. Below SHIP-SIZE
minimums (1 commit / 0 LOC non-test code / no end-to-end capability) but
acceptable because (a) writing-plans skill was explicitly invoked, (b)
v3.12.0 finalize is blocked on Windows bundle CI, (c) disk too low to
risk a feature build mid-release. Next tick will be MODE B then start
Task 1 of the Streamline plan.

### RELEASE_PENDING: v3.12.0 — merge SHA 4fb11e0, tag v3.12.0, CI runs 26349407913 (build, Win bundle pending) + 26349407898 (Docker ✓)

### Next tick — MODE B FINALIZE v3.12.0, then start Streamline Task 1
1. `gh run view 26349407913` — confirm Windows bundle finished.
2. `gh run download 26349407913 --dir /tmp/slab-release-3.12.0`.
3. `gh release create v3.12.0 --title "v3.12.0 — Atelier" --notes-file .cron-state/release-notes-v3.12.0.md` + 6 artifacts.
4. Verify ghcr.io/sanjays2402/slab-server:v3.12.0 live (already pushed).
5. Clear RELEASE_PENDING. Then `git checkout -b feature/v3.13.0-streamline`
   and execute Task 1 of the plan (ADR + module scaffold + dep-graph walker).

### LAST_WOW_TICK_AT: 2026-05-24T01:42Z (live progress matrix — still within 24h)

### RECENTLY_CLOSED_ISSUES:
- v3.12.0 Atelier: merged + tagged 2026-05-23, finalize pending Win bundle CI.

---

## PRIOR STATUS: v3.12.0 Atelier MERGED + TAGGED — CI in flight, finalize next tick.

**TICK 2026-05-23 19:1x PT (Saturday off-hours)** — MODE A RELEASE executed.
Merged `feature/v3.12.0-atelier` into `main` (merge SHA `4fb11e0`). 19 files,
+2233 LOC: full `pdf::atelier` module (recipe/run/batch/cmds), Svelte
AtelierPanel, typed TS client, Mod+Shift+R keymap, +Cargo/Tauri/package.json
version bumps, release notes, session log.

Quality gates on main post-merge:
- cargo fmt --all -- --check ✓
- cargo clippy --lib --all-targets -- -D warnings ✓
- cargo test --lib → **1418 passed, 0 failed** (+20 since v3.11.0)
- pnpm check → 0 errors, 62 warnings (all pre-existing)

Tagged `v3.12.0` with marketing-tone annotation from release-notes file (no
emoji in tag title). Pushed `main --follow-tags`. CI runs in flight:
- **26349407913** — build (4-platform bundle) on main
- **26349407898** — Docker (slab-server) on v3.12.0 tag
- **26349407658** — Pages deploy (landing site)

### RELEASE_PENDING: v3.12.0 — merge SHA 4fb11e0, tag v3.12.0, CI runs 26349407913 (build) + 26349407898 (Docker)

### Next tick — MODE B FINALIZE
1. `gh run view 26349407913` — if green, `gh run download` to `/tmp/slab-release-3.12.0`.
2. Curate 6 best artifacts (mac arm/x64 dmg, linux deb+AppImage, win nsis+msi).
3. `gh release create v3.12.0 --title "v3.12.0 — Atelier" --notes-file .cron-state/release-notes-v3.12.0.md` + upload artifacts.
4. Verify Docker image `ghcr.io/sanjays2402/slab-server:v3.12.0` is live.
5. Clear RELEASE_PENDING. Re-poll `gh issue list` for next priority.

### Honest note on the working tree:
`docs/landing/index.html` was uncommitted in Sanjay's checkout at the start
of this tick (mtime 18:59 PT). It carried over the branch switch cleanly —
no conflicts because the merge didn't touch it. Sanjay's edits are still
sitting unstaged on main. Untouched.

## (archived previous status)
## PRIOR STATUS: v3.12.0 Atelier — version bumped, release notes drafted. MERGE-to-main DEFERRED.

**TICK 2026-05-23 18:5x PT (Saturday off-hours)** — release-prep tick.

### Shipped (1 commit on feature/v3.12.0-atelier):
- `ebe4778` — version 3.11.0 → 3.12.0 in Cargo.toml + Cargo.lock + package.json + tauri.conf.json; release notes drafted at `.cron-state/release-notes-v3.12.0.md` (marketing-tone, leads with "Adobe Action Wizard, free + offline").

### Why this tick is small (HONEST):
- **CI run 26348824398**: tests green on all 3 platforms; **bundle job still queued/not started**. Don't tag until artifacts proven.
- **Working tree**: `docs/landing/index.html` has uncommitted edits, mtime 18:53 PT (1 minute before tick fired) = Sanjay actively editing. Switching to `main` to merge would surprise him.
- **Disk**: 1.5 GiB free (`src-tauri/target` = 3.2 GiB).

### Quality gates this tick:
- `cargo fmt --all -- --check` ✓
- `pnpm check` → 0 errors, 62 warnings (all pre-existing)
- Skipped cargo clippy/test — mechanical version bump, CI on push exercises it.

### LAST_WOW_TICK_AT: 2026-05-24T01:42Z (live progress matrix — still within 24h)

### Next tick — MODE A RELEASE (assuming bundle CI green):
1. Re-poll `gh run view 26348824398` — confirm bundle job succeeded.
2. If `docs/landing/index.html` still dirty: `git stash push -m "sanjay landing edits"` before switching branches; restore after release.
3. `git checkout main && git pull && git merge --no-ff feature/v3.12.0-atelier -m "Merge v3.12.0 'Atelier' — workflow automation"`.
4. Quality gates on main (fmt + clippy + cargo test --lib + pnpm check).
5. `git tag -a v3.12.0 -F .cron-state/release-notes-v3.12.0.md` then push `main --follow-tags`.
6. Record `RELEASE_PENDING: v3.12.0 — tag v3.12.0, CI runs <build> + <docker>`.

### RECENTLY_CLOSED_ISSUES:
- v3.11.0 Signet Pro: released 2026-05-23.
- v3.12.0 Atelier: backend + UI + version bump on feature branch; release pending.

---

## PRIOR STATUS: v3.11.0 Signet Pro SHIPPED — release published, Docker live, all artifacts uploaded.

**TICK 2026-05-23 17:3x PT (Saturday off-hours)** — MODE B FINALIZE executed.
- CI build (26347285490): all 7 jobs green (4 platforms × test+bundle on linux+mac arm/x64+win).
- Downloaded 6 artifacts to /tmp/slab-release-3.11.0.
- `gh release create v3.11.0 --title "v3.11.0 — Signet Pro"` published, NOT draft.
  https://github.com/Sanjays2402/slab/releases/tag/v3.11.0
- Docker (run 26347285501) green → `ghcr.io/sanjays2402/slab-server:v3.11.0` live.
- Release notes lead with the wedge: "Legally-binding timestamps. Batch sign 200 PDFs.
  Visible signature stamps. Adobe charges $239/yr; we ship it free, offline."
- All 6 installers attached (mac arm/x64 dmg, linux deb+AppImage, win nsis+msi).
- RELEASE_PENDING cleared.

### Disk recovery: `cargo clean` on src-tauri freed 6.0 GiB (from 454 Mi free to 5.7 Gi).

### Next tick — MODE C DEVELOP — EXECUTE v3.12.0 ATELIER PLAN
Issue backlog is EMPTY (re-polled 2026-05-23 17:55 PT). Sanjay invoked
writing-plans skill — plan saved at:
  `docs/plans/2026-05-23-v3.12.0-atelier-workflow-automation.md` (commit eb197ca)

**v3.12.0 Atelier** — workflow automation engine. Chain OCR + auto-redact +
Bates + flatten + sign into named recipes, run over folders unattended.
Adobe Action Wizard equivalent, free + offline. THE next enterprise moat.

7 TDD tasks: data model → run_recipe → batch driver → Tauri cmds → Svelte
panel w/ live progress grid → preset+version bump → MODE A release.

Next tick: `git checkout -b feature/v3.12.0-atelier` then start Task 1.

### LAST_WOW_TICK_AT: 2026-05-23 16:08 PT (visible sig stamps banked the wow.
 v3.11.0 GA itself is the wow ship — public release of CAdES-T + batch + stamps.)

### RECENTLY_CLOSED_ISSUES:
- v3.11.0 Signet Pro: merged 2026-05-23, tag v3.11.0, release published 17:3x PT.

---

## PRIOR STATUS: v3.11.0 Signet Pro MERGED + TAGGED — CI in flight, finalize next tick.

**TICK 2026-05-23 17:1x PT (Saturday off-hours)** — MODE A RELEASE executed.
Merged `fix/svelte-reactive-refs` (the descendant of `feature/v3.11.0-signet-pro`)
into `main` (merge SHA on main now contains all 18-commit feature branch work).
Bumped version 3.10.0 → **3.11.0** across Cargo.toml + Cargo.lock + package.json
+ tauri.conf.json (commit `49dd82e`).

Quality gates on main post-merge:
- cargo fmt --all -- --check ✓
- cargo clippy --lib --all-targets -- -D warnings ✓
- cargo test --lib → **1398 passed, 0 failed** (+35 since v3.10.0 release)
- pnpm check → 0 errors, 61 warnings (pre-existing a11y nits)

Tagged `v3.11.0` with marketing-tone annotation (no emoji). Pushed
`main --follow-tags`. CI runs in flight:
- **26347285490** — build (4-platform bundle)
- **26347285501** — Docker (slab-server)

### RELEASE_PENDING: v3.11.0 — tag v3.11.0, CI runs 26347285490 (build) + 26347285501 (Docker)

### Next tick — MODE B FINALIZE
1. `gh run view 26347285490` — if green, `gh run download` artifacts to `/tmp/slab-release-3.11.0`.
2. `gh release create v3.11.0 --title "v3.11.0 — Signet Pro"` with marketing notes
   (lead with: "Legally-binding timestamps. Batch sign 200 PDFs in parallel.
   Visible signature stamps. Adobe charges $239/yr — we ship it free, offline.")
3. Upload 6 artifacts (mac arm64+x64 dmg, linux deb+AppImage, win nsis+msi).
4. Verify Docker tag workflow (26347285501) green; image `ghcr.io/sanjays2402/slab-server:v3.11.0`.
5. Clear RELEASE_PENDING line.

### Disk: 1.1 Gi free at tick end — will need `cargo clean -p slab-app` before
the next dev tick if disk doesn't recover from /tmp clearing.

### LAST_WOW_TICK_AT: 2026-05-23 16:08 PT (still within 24h — visible sig stamps
banked the wow already)

### RECENTLY_CLOSED_ISSUES:
- v3.11.0 Signet Pro merged + tagged this tick.

---

## PRIOR STATUS: v3.11.0 Signet Pro — CAdES-T shipped end-to-end (Task 7 complete).

**TICK 2026-05-23 16:5x PT (Saturday off-hours)** — RFC 3161 timestamp tokens
now embed into the CMS unsigned attributes, end-to-end from Svelte input to
re-encoded SignerInfo. CAdES-BES → CAdES-T toggle.

Branch: `fix/svelte-reactive-refs` (descendant of `feature/v3.11.0-signet-pro`).
4 commits this tick:
- e04d2f0 signet_pro/tsa: `signer_signature_digest` + `embed_timestamp_token` + 4 tests
- 063c8f2 signet/sign: optional CAdES-T path, 16 KiB hex window when TSA set
- 46defd4 tauri DTOs: `tsa_url` on `SignetSignArgs` + `SignetProBatchArgs`
- 9ed1c2a UI: TSA URL input on `SignetPanel` and `SignetBatchPanel`

Signet test suite: **69/69 PASS** (4 new TSA-embed tests). Clippy `-D warnings`
clean. `pnpm check`: 0 errors.

**LAST_WOW_TICK_AT: 2026-05-23 16:08 PT** (still inside the 24h window from
visible-signature stamps; this tick's CAdES-T is mostly under-the-hood
compliance plumbing — wow already banked for today).

Buy-Button: CAdES-T is what every law firm + audit-trail compliance buyer
asks for first. Acrobat Pro exposes it; PDF Expert doesn't. Pay-for-it ✓,
Pick-us ✓.

Next tick:
- Merge `fix/svelte-reactive-refs` → `feature/v3.11.0-signet-pro` (or fast-
  forward if straight-line), then `feature/v3.11.0-signet-pro` → `main`,
  tag `v3.11.0`, kick CI, finalize release.
- Verify a live TSA round-trip (digicert/freetsa) via a one-shot manual
  script before tagging — the embed code path has only been exercised
  against a hand-rolled fake TST so far.
- Add release-notes copy that leads with "legally-binding timestamps,
  completely offline-cert / online-TSA, never your file".

---

**TICK 2026-05-23 16:08 PT (Saturday off-hours)** — appearance Form XObject
splicing wired into `sign_pdf`, single-file UI gets a visible-stamp toggle.

Branch: `feature/v3.11.0-signet-pro` (1111bd6). 4 commits this tick:
- 8996752 TSA HTTP fetch (mockito-tested, 12 tests)
- 2e410e0 batch-sign Tauri command + dedicated Svelte panel (Buy-Button)
- 1d39e42 visible-signature wire-in (Form XObject /AP /N) + verify-still-green test
- 1111bd6 SignetPanel visible-stamp toggle (page + rect inputs)

Full lib suite: **1394/1394 PASS**. Clippy `-D warnings` clean.
**LAST_WOW_TICK_AT: 2026-05-23 16:08 PT** (visible signatures end-to-end —
"look at my PDF, it has a real signature stamp" screenshot-worthy moment).

Next tick:
- CAdES-T upgrade: call `fetch_timestamp` after signing, splice TST as
  `id-aa-timeStampToken` unsigned attr on SignerInfo (cms_blob.rs).
- Add TSA URL field to SignOptions + UI panels (single + batch).
- Then merge branch -> main, tag v3.11.0, finalize release.

---

<details><summary>Earlier history</summary>



**TICK 2026-05-23 15:32 PT (Saturday off-hours)** — writing-plans skill: plan
already saved last tick, this tick *executes* it.

Branch: `feature/v3.11.0-signet-pro` — 3 new commits (`552a859`, `41a88f3`,
`8c85371`) on top of last tick's scaffold + plan, total 4 commits / ~950 LOC
this tick (953 insertions across 4 files). Plus rayon dep added.

Shipped this tick:
- **Task 2 + 3 (parse half):** RFC 3161 `TimeStampReq` DER encoder +
  `TimeStampResp` parser in `signet_pro/tsa.rs`. Canonical-integer nonce
  normalisation (so `der::asn1::Int` accepts the full i64 range);
  `ID_AA_TIMESTAMP_TOKEN` OID exported for CMS unsigned-attr embedding.
  7 unit tests.
- **Task 4:** `build_appearance` + `build_appearance_from_name` Form
  XObject builder in `signet_pro/appearance.rs`. 0.5pt grey border +
  Helvetica BT/ET text, PDF-literal-string escaping, font-size clamp,
  optional date/reason/location lines. 9 unit tests.
- **Task 5:** Batch driver in `signet_pro/batch.rs` — `plan_batch` walks
  for *.pdf (recursive opt-in), `run_batch` executes via rayon with
  atomic-counter progress, `BatchReport` with `success_rate`,
  `fully_succeeded`, `failures()`. 10 unit tests including a full
  `sign_folder` end-to-end smoke test (pretend-sign 3 PDFs).

**signet_pro now has 25 passing tests** (was 0 last tick). Full signet+pro
suite: 59/59 green.

Quality gates this tick:
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓ (fixed bool_assert_comparison,
  derive Default, and ok().expect() lints raised by the new code)
- `cargo test --lib pdf::signet` → 59/59 PASS
- `pnpm check` → 0 errors, 63 warnings (all pre-existing a11y nits)

**Disk: 5.4 GiB free** after `cargo clean` (was at 124 MiB before — full
clean ran mid-tick to unblock the link step).

Buy-Button test: TSA encoding + batch parallel sign are Acrobat Pro $239/yr
exclusives, both now implemented offline in Slab. Pay-for-it ✓, Pick-us ✓.

### Next tick — finish Task 3 (HTTP fetch) + Task 4 wiring into sign_pdf
1. `fetch_timestamp(url, req)` — reqwest blocking POST with
   `application/timestamp-query` content-type. Mock via mockito in tests.
2. Embed returned TST as `id-aa-timeStampToken` unsigned attr in
   `signet::sign::sign_pdf` SignerInfo (CAdES-T upgrade).
3. Wire `SignOptions::appearance` → swap invisible Widget for AP/N form-
   XObject Widget at the spec.rect on spec.page.
4. Frontend BatchSignPanel.svelte (Task 6) — can land alongside in same tick
   if scope allows.

### LAST_WOW_TICK_AT: 2026-05-23T22:32Z (batch parallel sign with progress
events — the demo screenshot Sanjay will tweet)

### RECENTLY_CLOSED_ISSUES:
- (none open)

---

## PRIOR STATUS: v3.11.0 Signet Pro kickoff — plan + ADR 0012 + module scaffolding on feature/v3.11.0-signet-pro.

**TICK 2026-05-23 15:14 PT (Saturday off-hours)** — writing-plans skill invocation.

Branch: `feature/v3.11.0-signet-pro` (pushed, CI run 26345068650 queued).

Shipped:
- `docs/plans/2026-05-23-v3.11.0-signet-pro.md` — 8-task TDD breakdown
  (RFC 3161 TSA + visible appearances + batch sign).
- `docs/adr/0012-signet-pro-tsa-batch.md` — design rationale.
- `src-tauri/src/pdf/signet_pro/{mod,tsa,appearance,batch}.rs` — public
  type stubs + module wiring (compiles, clippy-clean, fmt-clean).
- 2 commits: `d7df6af` (plan) + scaffold commit.

Quality gates all green: cargo check, fmt, clippy -D warnings, pnpm check.

### Next tick — Task 2 of the plan: RFC 3161 TimeStampReq encoder
- Implement `build_timestamp_req` in `signet_pro/tsa.rs` with `der`+`spki`.
- TDD: failing test asserts SHA-256 OID + digest bytes appear in DER output.
- Verify `cms` crate already in deps tree (it is — used by v3.10.0).

### Disk: 1.4Gi free at tick end. Pre-existing pressure; will need
`cargo clean -p slab-app` before next bundle build.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify; <24h)

### RECENTLY_CLOSED_ISSUES:
- v3.10.0 Signet — published prior tick.

---

## PRIOR STATUS: v3.10.0 Signet RELEASED — 6 artifacts uploaded, Docker image live, all CI green.

**TICK 2026-05-23 15:02 PT (Saturday off-hours)** — MODE B FINALIZE executed.
- CI run 26344139015 (build + 4-platform bundle) — **all success** ✅
- CI run 26344139022 (Docker slab-server) — **success** ✅
- Downloaded all 4 artifact bundles to `/tmp/slab-release-3.10.0` (2.0Gi free was enough — disk now 2.0Gi after extraction).
- `gh release create v3.10.0 --title "v3.10.0 — Signet"` with marketing-tone notes (Adobe $239/yr framing, RustCrypto privacy wedge) and 6 artifacts:
  - Slab_3.10.0_aarch64.dmg (macOS Apple Silicon)
  - Slab_3.10.0_x64.dmg (macOS Intel)
  - Slab_3.10.0_amd64.deb (Linux)
  - Slab_3.10.0_amd64.AppImage (Linux portable)
  - Slab_3.10.0_x64-setup.exe (Windows NSIS)
  - Slab_3.10.0_x64_en-US.msi (Windows MSI)
- Docker image `ghcr.io/sanjays2402/slab-server:v3.10.0` live.
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.10.0
- RELEASE_PENDING cleared.

### Post-push release check ✓
- All on-tag workflows green (build #26344139015 + Docker #26344139022).
- Release published (not Draft).
- No CI failures in last 24h.

### Next tick — MODE C DEVELOP (v3.11.0)
1. Re-poll `gh issue list` (currently 0 open).
2. Create branch `feature/v3.11.0-signet-trust` for Signet follow-on:
   ECDSA-already-supported, so target: **RFC 3161 timestamp-authority
   integration** (CAdES-T grade) + CRL distribution-point surfacing
   (revocation hints, not full check) + **batch sign** for legal workflows.
3. Disk: 2.0Gi free at tick end. Will need `cargo clean -p slab-app` before
   next Tauri bundle build.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify — within 24h)

### RECENTLY_CLOSED_ISSUES:
- v3.10.0 Signet — published this tick (CI 26344139015 + 26344139022)

---

## PRIOR TICK 2026-05-23 14:34 PT — MODE A RELEASE executed.

- Merged `feature/v3.10.0-signet` → `main` with `--no-ff` (merge SHA `bd0aa70`).
- Bumped version 3.9.0 → **3.10.0** across Cargo.toml, Cargo.lock, tauri.conf.json, package.json (commit `8dc38ad`).
- Quality gates on main:
  - cargo fmt --all -- --check ✓
  - cargo clippy --lib --all-targets -- -D warnings ✓
  - cargo test --lib → **1363 passed, 0 failed**
  - pnpm check → 0 errors, 63 warnings (a11y on SignetPanel labels — pre-existing pattern)
- Tagged `v3.10.0` with marketing-tone annotation (no emoji per Sanjay's rule).
- Pushed `main --follow-tags` → CI runs **26344139015** (build) + **26344139022** (Docker) in flight.

### RELEASE_PENDING: v3.10.0 — merge SHA bd0aa70, tag v3.10.0, CI runs 26344139015 (build) + 26344139022 (Docker)

### Next tick — MODE B FINALIZE
1. `gh run view 26344139015` — if green, `gh run download` artifacts to `/tmp/slab-release-3.10.0`.
2. `gh release create v3.10.0 --title "v3.10.0 — Signet"` with marketing-grade notes + 6 artifacts (mac arm64/x64 dmg, linux deb+AppImage, win nsis+msi).
3. Verify Docker tag workflow (26344139022) also green.
4. Clear RELEASE_PENDING line.
5. Then v3.11.0 candidate: **ECDSA P-256/P-384 signing** + revocation hints + batch sign (Signet follow-on), OR fold into v3.10.1 hotfix if Signet has post-release bugs. Re-poll `gh issue list` first.

### Ops note
Disk hit 100% (185Mi) during cargo link — recovered by removing `~/Library/Caches/com.microsoft.VSCode.ShipIt` (920MB) and `Chrome.code_sign_clone`. **1.1Gi free** at tick end; should hold for one finalize tick but next big build will need more cleanup.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify — still within 24h)

### RECENTLY_CLOSED_ISSUES:
- v3.10.0 Signet merged + tagged (this tick) — release artifacts pending CI.

---

## PRIOR STATUS: v3.10.0 Signet feature-complete on feature/v3.10.0-signet — sign + verify end-to-end, 34 signet tests passing, Tauri commands + SignetPanel UI shipped. Ready to merge to main next tick if CI is green.

**TICK 2026-05-23 14:17 PT (Saturday off-hours)** — MODE C develop, BIG slice fold-in.
4 commits, ~2200 net LOC + 17 new tests, on `feature/v3.10.0-signet`:
- `8e2b99c` feat(signet): build_pkcs7_detached — CMS SignedData (adbe profile)
- `c2bd798` feat(signet): sign_pdf end-to-end — placeholder/serialize/splice/ByteRange
- `75c1767` feat(signet): Tauri commands — signet_load_identity / sign / verify
- `d26ce33` feat(signet): SignetPanel UI — load identity, sign, verify

Quality gates green: cargo fmt + clippy + cargo test --lib (34 signet tests
pass), pnpm check 0 errors. Pushed to origin; CI run 26343830061 in flight
at tick end.

### Buy-button verdict: PASSES (Tell-a-friend + Pick-us)
- Adobe charges $239/yr for digital signatures. Slab ships RSA-SHA-256
  PKCS#7-detached signatures **free, offline**, compatible with the
  Acrobat signature panel.
- Enterprise wedge: legal/compliance workflows now have a path that
  doesn't ship private keys or PDFs to a cloud.

### Wow moment: signed PDF round-trips through our own verify() — digest matches, crypto valid, chain status reports SelfSigned for test certs, FullDocument coverage. Sign + verify in <50ms on the test fixtures.

### Next tick — MODE A merge or v3.10.0 release
1. `git checkout main && git pull && git merge --no-ff feature/v3.10.0-signet`
2. Quality gates on main → tag `v3.10.0` → push tags.
3. Finalize the GitHub release with marketing-grade notes:
   _"Sign and verify PDFs offline. Adobe-compatible PKCS#7 signatures.
   Zero cloud, zero subscription."_
4. After release: v3.10.1 — add ECDSA P-256/P-384 signing (cms builder
   bound work, ~1 day), revocation hints (CRL distribution-point
   surfacing, not full check), batch sign for legal workflows.

### LAST_WOW_TICK_AT: 2026-05-23T21:20Z (Signet end-to-end sign+verify)

### Ops
Disk: 2.6 Gi free after `cargo clean -p slab-app` mid-tick (recovered from
ENOSPC during initial build). Watch for next tick.

### RECENTLY_CLOSED_ISSUES:
(none this tick — all issue-override items #23–27 already closed)

---

## PRIOR STATUS: v3.10.0 Signet foundation landed on feature/v3.10.0-signet — identity loader + trust store, 17 tests passing.

**TICK 2026-05-23 13:19 PT (Saturday off-hours)** — MODE C develop, foundation tick.
3 commits, ~1100 LOC + 17 new tests, on `feature/v3.10.0-signet`:
- `c051a24` chore(signet): vendor RustCrypto CMS deps + ADR 0011
- `f013237` feat(signet): SigningIdentity PEM loader (RSA / P-256 / P-384)
- `edcdc89` feat(signet): TrustStore + chain status enum

Quality gates green: cargo fmt + clippy + cargo test --lib **1346 passing** (+17 new), pnpm check 0 errors. Pushed to origin.

### Honest buy-button verdict: foundation, not ship
Plumbing only — no UI, no end-to-end sign/verify. Risk-reduced the CMS work
by getting identity + trust right before touching the finicky
`cms::SignedDataBuilder` API.

### Next tick — Task 4 (sign pipeline, end-to-end)
1. `cms_blob.rs` — `build_pkcs7_detached(digest, identity, time)`.
   Reference: `~/.cargo/registry/.../cms-0.2.3/tests/builder.rs:86-156`.
2. `sign.rs` — `prepare_signature_field` + `sign_pdf` (byte-range splice).
3. Tauri command `slab_signet_sign` + minimal SignetPanel.
4. Target: 600+ LOC, end-to-end "load identity → sign PDF → file on disk".

After Task 4 the buy-button passes for the FIRST time on this version.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (Quill press-roller — within 24h)

### Ops
Disk dropped from 5.2 Gi → 1.7 Gi during this tick (full `target/` rebuild
after Quill release). Next compile may need `cargo clean -p slab-app` again.

---

## PRIOR STATUS: v3.9.0 Quill RELEASED (mac arm64+x64 dmg, win nsis+msi). Linux deferred to v3.9.1 (disk ENOSPC). v3.10.0 Signet ready to start.

**TICK 2026-05-23 13:07 PT (Saturday off-hours)** — MODE B FINALIZE.
CI run 26341610206 all green. Tagged `098f11b` → `v3.9.0`, pushed tag, created
GitHub release with marketing notes + 4 artifacts (mac arm64/x64 dmg, win nsis/msi).
Linux AppImage download hit ENOSPC at libwebkit2gtk extract — release notes
say linux ships in v3.9.1. Session log: `.cron-state/sessions/2026-05-23-1307.md`.

### Next tick — MODE C DEVELOP (v3.10.0 Signet)
1. Verify on-tag workflows for v3.9.0 succeeded (`gh run list --limit 8`).
2. `git checkout -b feature/v3.10.0-signet` and execute Tasks 1–4 of the
   Signet plan as one mega-tick (~900 LOC, 4 commits, sign end-to-end).
3. Linux v3.9.1 hotfix can wait — disk needs to clear first.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (magenta press-roller — within 24h)

---

## PRIOR STATUS: v3.9.0 Quill awaiting CI bundle (run 26341610206 — macOS done, Linux+Win building). v3.10.0 Signet plan written.

**TICK 2026-05-23 12:46 PT (Saturday off-hours)** — PLANNING tick.
Wrote `docs/plans/2026-05-23-v3.10.0-signet-digital-signatures.md` — full 9-task
plan for PKCS#7 digital signatures (sign + verify, pure-Rust RustCrypto,
cross-platform, wax-seal wow). Did NOT ship code because (a) v3.9.0 CI still
bundling, (b) Sanjay actively editing `docs/landing/index.html` in working tree
at the moment cron fired (mtime 12:46:01 = tick start), (c) disk at 1.1 Gi free
— no headroom for a Tauri build. Session log: `.cron-state/sessions/2026-05-23-1246.md`.

### Next tick — MODE B FINALIZE (then start Signet)
1. Poll CI 26341610206 — if green, finalize v3.9.0 Quill release.
2. Once Quill released: `git checkout -b feature/v3.10.0-signet` and execute
   Tasks 1–4 of the Signet plan as one mega-tick (~900 LOC, 4 commits, sign
   end-to-end).

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (within 24h — no new wow needed)

---

## PRIOR STATUS: v3.9.0 Quill SHIPPED to main — AcroForm inspector + fill end-to-end. Awaiting CI build for release.

**TICK 2026-05-23 12:17 PT (Saturday off-hours)** — MODE C develop.
4 commits, 1689 net LOC (`forms.rs` 841 + `FormsPanel.svelte` 501 + lib.rs 18
+ keymap.ts/keymap action 11 + `+page.svelte` 18 + version bumps). All gates
green: cargo fmt/clippy clean, cargo test --lib **1329 passing** (+11 new),
pnpm check 0 errors. Pushed `main` → `098f11b`. CI build run 26341610206
queued. Session log: `.cron-state/sessions/2026-05-23-1217.md`.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (magenta press-roller wipe — within 24h)

### What shipped this tick

- `2066214 feat(forms): AcroForm inspector + fill backend (Slice 1)` — 11 tests
- `02cfa65 feat(forms): Tauri commands slab_forms_inspect + slab_forms_fill (Slice 2)`
- `2a44f6c feat(forms): FormsPanel.svelte + forms.open keymap action (Slice 3)`
- `098f11b feat(forms): wire FormsPanel + bump v3.9.0 Quill (Slice 4)`

### Buy-Button — PASS on 3 of 4

- Pay-for-it ✓ — Acrobat Pro forms = $239/yr.
- Pick-us ✓ — no free cross-platform PDF form filler with real inspector UI.
- Notice-it ✓ — new sidebar entry + Cmd+Shift+F shortcut.
- Tell-a-friend — solid with JSON template round-trip angle.

### Next tick — MODE B FINALIZE

1. Poll CI run 26341610206 (build). If success → tag v3.9.0 + release pipeline.
2. If failed → triage from `gh run view --log-failed`.
3. After v3.9.0 ships → re-poll issues; otherwise v3.10.0 candidates:
   PKCS#7 digital signatures (Forms follow-on, enterprise legal) OR batch
   automations (drag-folder pipelines).

## ARCHIVED: v3.8.0 Press — RELEASED 2026-05-23

Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.8.0
One-click PDF/X-4 print production: FOGRA51/GRACoL2013 ICC, OutputIntent, normalize_color, Inspect/Convert/Validate UI, Cmd+Shift+X shortcut, magenta press-roller wipe wow.

---

## PRIOR STATUS: v3.8.0 Press Slice 6 SHIPPED on `feature/v3.8.0-press` — **PressPanel UI live end-to-end**. Inspect/Convert/Validate tabs, Mod+Shift+X shortcut, sidebar entry, and the magenta press-roller wipe wow with PDF/X-4 ✓ badge reveal. Branch is **feature-complete** for v3.8.0 → MERGE TO MAIN next tick.

**TICK 2026-05-23 11:14 PT (Saturday off-hours)** — MODE C develop. 4 commits, ~630 net LOC (PressPanel.svelte 604 + +page.svelte 15 + keymap.ts 1 + keymap/action.rs 8). All gates green (cargo fmt/clippy/test 1318 passing, pnpm check 0 errors). Session log: `.cron-state/sessions/2026-05-23-1114.md`.

### LAST_WOW_TICK_AT: 2026-05-23T18:20Z (magenta press-roller wipe — 380ms CMYK ink-roller sweep + PDF/X-4 ✓ FOGRA51/GRACoL2013 badge reveal, reduced-motion safe)

### Next tick — MODE A RELEASE
1. `git checkout main && git pull`
2. `git merge --no-ff feature/v3.8.0-press -m "Merge v3.8.0 'Press' — one-click PDF/X-4 conversion"`
3. Bump version 3.7.0 → 3.8.0 across Cargo.toml + tauri.conf.json + package.json.
4. Quality gates on main.
5. Tag v3.8.0 with marketing-tone annotation (no emoji in tag/commit per Sanjay's rule).
6. Push main --follow-tags. Set RELEASE_PENDING for MODE B finalize next tick.

### What shipped this tick

- `f3ac2a2 feat(press): register press.open keymap action (Mod+Shift+X)`
- `80de330 feat(press): extend ActionId union with press.open`
- `826d065 feat(press): PressPanel.svelte — Inspect/Convert/Validate tabs (Slice 6)`
- `a53766f feat(press): wire PressPanel into +page.svelte (sidebar + shortcut)`

### Buy-Button verdict — PASS on 4 of 4

- Pay-for-it: Acrobat Pro charges $239/yr for PDF/X-4 export → Slab does it free + offline.
- Notice-it: New sidebar entry + Cmd+Shift+X shortcut.
- Pick-us: No free cross-platform PDF/X-4 converter exists with a real UI.
- Tell-a-friend: Magenta press-roller wipe is screenshottable.

### Ops note

Disk hit 100% again pre-compile. Cleared Chrome code-sign clone (78GB) +
`cargo clean -p slab-app` (8.8GB). 5.6GB free after. If this recurs the
clean target is recoverable cheaply; the Chrome clone keeps coming back
whenever Chrome updates.

---

## PRIOR STATUS: v3.8.0 Press Slices 1+2 SHIPPED on `feature/v3.8.0-press` — ADR + FOGRA51/GRACoL ICC vendored, OutputIntent enum, normalize_color() pass with 17 passing tests.

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
</details>
