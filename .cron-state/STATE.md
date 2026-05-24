# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 📋 v3.20.0 'Hopper' plan written — ready to execute (2026-05-24 07:18 PT)

**TICK 2026-05-24 07:15 PT (Sunday off-hours, writing-plans skill invoked).**

Sanjay (via the writing-plans skill) asked for an implementation plan
this tick instead of a normal ship tick. Output:

- **`docs/plans/2026-05-24-v3.20.0-hopper.md`** — full 8-task plan for
  v3.20.0 "Hopper", the watched-folder PDF automation feature
  (Hazel + Adobe AutoAction + AI paralegal in one box).
- Passes all 4 buy-button tests, has a designed wow moment (live log
  with morph-rename animation), bundles into 4 cron ticks.
- Reuses existing `pdf::atelier::run_recipe` + `ai::ollama` —
  only one new crate dep (`notify = "6.1"`).

### Why Hopper, not Beacon

STATE.md previously pointed to "v3.20.0 Beacon" as the next big
feature, but Beacon is already heavily shipped throughout the v3.x
line (chat / search / PII / citations / glossary / study / voice /
vision panels all live). The proposal at
`.cron-state/proposals/v0.10.0-beacon-ai.md` is essentially complete.

Looking at what would actually move the buy-button needle next:
**no competitor has drop-folder PDF automation with local AI rename.**
Adobe gates it behind enterprise. Hazel doesn't speak PDF. PDF Expert
doesn't have automation at all. That's the wedge.

### Next tick (off-hours)

Execute Task 1+2 of the Hopper plan (branch + notify dep + module
skeleton + sqlite registry with unit tests). ~600 LOC, 2-3 commits.

### LAST_WOW_TICK_AT: 2026-05-24T13:46Z (v3.19.0 Marquee — browser playground live)

(Plan tick doesn't bump this — Hopper's wow ships in tick 3 of the
plan with the live-log animation.)

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 to wire up Cloudflare
  Pages and make try.slab.app actually serve from the workflow.
- Optional: record 5-second demo video for landing (issue #27, all
  override issues closed).
- Free disk on the mini — only 5.4 GiB free even after `cargo clean`.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.18.0
- v3.19.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.19.0

---

## PRIOR STATUS: v3.19.0 'Marquee' PUBLISHED — GitHub release live (2026-05-24 07:?? PT)

**TICK 2026-05-24 06:57 PT (Sunday off-hours)** — MODE B FINALIZE complete.

### What shipped this tick

- Polled CI: build run `26362972057` finished GREEN (mac arm64+x64, linux,
  windows bundles all built; cargo test all three OSes green).
- Docker `slab-server` run `26362971977` — success.
- deploy-try run `26362972083` — success in graceful-degradation mode
  (artifact-only, Cloudflare secrets not yet configured — expected).
- Downloaded artifacts to `/tmp/slab-release-v3.19.0` (after `cargo clean`
  to free 5+ GiB on the volume — disk was at 100%, blocking the download).
- Published `gh release create v3.19.0` with 6 curated assets:
  - `Slab_3.19.0_aarch64.dmg`
  - `Slab_3.19.0_x64.dmg`
  - `Slab_3.19.0_amd64.deb`
  - `Slab_3.19.0_amd64.AppImage`
  - `Slab_3.19.0_x64_en-US.msi`
  - `Slab_3.19.0_x64-setup.exe`
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.19.0
- isDraft: false ✅
- Removed RELEASE_PENDING.

### Disk-space note

`/System/Volumes/Data` was at 100% (404 MiB free) when this tick started,
which caused the first `gh run download` to fail mid-zip. Ran
`cargo clean` in `src-tauri/` (freed 5.2 GiB) and retry succeeded. Worth
flagging: the Mac mini hosting the cron is running tight on disk
(228 GiB volume, 171 GiB used after cleanup, only ~5.4 GiB free). Sanjay
may want to prune old caches / Xcode derived data / old node_modules.

### LAST_WOW_TICK_AT: 2026-05-24T13:46Z (v3.19.0 Marquee — browser playground live)

### Next tick (MODE C develop)

Roadmap drift: STATE.md historically pointed to "v0.10.0 Beacon next"
but the real version pipeline has run far past that into the v3.x line.
Re-grounding: with no open issues, no RELEASE_PENDING, no failing CI,
and a fresh release just published, the next BIG feature to ship is
**Beacon (local-first AI / chat-with-PDF)** — still the buyer-magnet
per SOUL/SLAB doctrine. Spec lives at
`.cron-state/proposals/v0.10.0-beacon-ai.md` and needs a version-number
refresh (probably v3.20.0 "Beacon" given the real numbering line).

Plan for the next off-hours tick:
1. Re-read the Beacon proposal, retag it as v3.20.0 'Beacon'.
2. Create branch `feature/v3.20.0-beacon`.
3. Ship Slice 1 end-to-end: Ollama detection + Beacon settings panel
   + model picker UI + "Ask anything" floating input scaffolded.
4. Honor SHIP-SIZE: ≥4 commits, ≥600 LOC, e2e capability.

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 to wire up Cloudflare
  Pages and make try.slab.app actually serve from the workflow.
- Optional: record 5-second demo video for landing (issue #27, all
  override issues closed).
- Free disk on the mini — only 5.4 GiB free even after `cargo clean`.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.18.0
- v3.19.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.19.0

---

## PRIOR STATUS: v3.19.0 'Marquee' MERGED + TAGGED — CI running (2026-05-24 06:46 PT)

**TICK 2026-05-24 06:39 PT (Sunday off-hours)** — MODE C → MODE A → RELEASE_PENDING.

Slice 9 of Marquee shipped in one tick: deploy pipeline + version bump
+ release notes + merge + tag.

### What shipped this tick (3 commits on feature, then merged to main)

- `c48277f` chore(release): bump v3.18.0 -> v3.19.0 'Marquee'
- `5561ca4` ci(deploy): try.slab.app — Cloudflare Pages workflow + ops doc
  - `.github/workflows/deploy-try.yml` (143 LOC) — builds SvelteKit SPA,
    writes `_redirects` + `_headers`, deploys to Cloudflare Pages
    project `slab-try`. Falls back to artifact-only build if CF secrets
    absent (graceful degradation).
  - `docs/ops/try-slab-deploy.md` (102 LOC) — 8-step Sanjay setup
    checklist for CF Pages project, API token, account ID, GitHub
    secrets, custom domain, verify, rollback, local preview.
- `649ed20` docs(release): marketing release notes for v3.19.0 'Marquee'
  - 78 LOC customer-facing copy framing try.slab.app as funnel companion
    to the desktop app.
- `9617676` Merge v3.19.0 'Marquee' — try.slab.app browser playground +
  markdown editor (merge commit on main)

### Tick totals (feature branch lifetime: 6 commits, ~1800 LOC)

This release's full diff vs v3.18.0:
- ADR-0007 + `/try` shell + 3 samples + DownloadWall + page ops + metadata
  editor + privacy banner + landing CTA + mdToPdf module + /try/markdown
  live editor + landing grid + wallCopy md-extras
- Deploy pipeline + ops doc
- Release notes + version bump

### Quality gates (this tick) — ALL GREEN

- `pnpm check` ✅ 0 errors, 62 pre-existing warnings
- `cargo fmt --all --check` ✅
- `cargo clippy --lib --all-targets -- -D warnings` ✅
- `cargo test --lib` ✅ **1588 passed, 0 failed**

### RELEASE_PENDING: v3.19.0

- Merge SHA: `9617676`
- Tag: `v3.19.0`
- CI runs in_progress at tick end:
  - build (main): `26362972057`
  - Docker slab-server (v3.19.0 tag): `26362971977`
  - deploy-try (main): `26362972083`  ← NEW workflow's first real run
- Next tick: MODE B FINALIZE — poll CI, download artifacts, publish
  GitHub release with the 6 best bundle artifacts (mac arm64/x64 dmg,
  linux deb + AppImage, win msi + nsis), attach
  `.cron-state/release-notes-v3.19.0.md`.

### Buy-Button verdict — 4/4 PASS

- **Pay-for-it ✅** — Smallpdf/iLovePDF charge $7-$12/mo for in-browser
  page ops + md→PDF + metadata editing AND upload your file. Slab does
  the same, free, zero upload.
- **Notice-it ✅** — try.slab.app is a brand-new surface that didn't
  exist yesterday — anyone visiting the GitHub repo or release notes
  sees the new playground link.
- **Pick-us ✅** — Funnel addition: users who refuse to install a desktop
  app now have an entry point. Competitors require account + upload.
- **Tell-a-friend ✅** — "Drop PDF in browser, edit, download, never
  leaves the tab" is a screenshot-tweet moment.

### LAST_WOW_TICK_AT: 2026-05-24T13:46Z (v3.19.0 Marquee merge — browser playground live)

### Next tick (MODE B finalize, then v0.10.0 Beacon)

1. Poll the three CI runs above.
2. Download artifacts from build run, curate top 6, publish GitHub
   release v3.19.0 with `.cron-state/release-notes-v3.19.0.md`.
3. Verify Docker image on GHCR.
4. Verify deploy-try ran (will likely report "Cloudflare credentials
   not configured" notice — that's expected until Sanjay completes
   `docs/ops/try-slab-deploy.md` checklist).
5. Once finalized, kick off v0.10.0 Beacon (Slab AI) — the buyer-magnet
   feature. Spec at `.cron-state/proposals/v0.10.0-beacon-ai.md`.

### Sanjay TODO (when he gets back)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 to enable the
  Cloudflare deploy. Without it, the workflow runs but stops at the
  artifact-upload step.
- Optional: record the 5-second demo video for the landing page
  (issue #27 from the override list, all higher-priority issues now
  closed via prior ticks).

### RECENTLY_CLOSED_ISSUES

- v3.18.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.18.0
- v3.19.0 tagged, awaiting MODE B finalize next tick.

---

## PRIOR STATUS: v3.19.0 Marquee Slice 6 (Markdown→PDF) shipped (2026-05-24 06:25 PT)

(Branch `feature/v3.19.0-marquee-try`, 3 commits, ~1243 LOC for Slice 6.
See prior session logs for the full Marquee build-out from Slice 0
through Slice 8. This tick closed Slice 9 = deploy pipeline + release.)
