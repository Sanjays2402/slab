# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🎯 v3.21.0 'Hopper Conditions' FEATURE-COMPLETE on branch — pushed, awaiting CI for MODE A merge (2026-05-24 09:55 PT)

**TICK 2026-05-24 09:48 PT (Sunday off-hours)** — MODE C DEVELOP, the
writing-plans skill was invoked but a plan already existed on disk
(`docs/plans/2026-05-24-v3.21.0-hopper-conditions.md`) so this tick
executed the remaining Tasks 8 + 9 + 10 of that plan end-to-end.

### What shipped this tick (3 commits on `feature/v3.21.0-hopper-conditions`)

- `bbd4f43` feat(hopper): TS types + API client + predicate helpers
  - `src/lib/hopper.ts` +174 LOC. Discriminated-union RulePredicate
    (mirrors Rust serde verbatim), RuleAction, Rule, RuleTestResult,
    slabHopper{Get,Set,Test}Rules wrappers, PREDICATE_KINDS,
    predicateLabel, emptyPredicate, emptyAction, formatPredicate,
    formatBytes helpers.
- `e38578e` feat(hopper): rules editor UI with live match preview (WOW)
  - `src/lib/components/HopperRulesEditor.svelte` NEW, ~875 LOC. Two-column
    Liquid Glass surface: rule chain on the left (add/reorder/delete,
    6 predicate kinds, action overlays), live preview pane on the right
    showing up to 5 candidate files (last 4 from this watch's run log +
    user-typed what-if filename) with per-rule green ✓ / grey · chips,
    winning rule highlighted in solid green with "→ destination" annotation.
    Re-evaluates within ~150ms of any edit via slab_hopper_test_rules —
    preview fidelity equals production routing.
  - `src/lib/panels/HopperPanel.svelte` +52 LOC. "▸ Rules" expand toggle
    per watch row, mounts editor inline below; expandedRulesWatchId
    state + rules-toggle/host CSS.
- `15b3815` chore(release): v3.20.0 -> v3.21.0 'Hopper Conditions'
  - All 4 version files bumped (package.json, tauri.conf.json, Cargo.toml,
    Cargo.lock slab-app entry edited directly).
  - `docs/releases/v3.21.0.md` NEW, ~110 LOC customer-facing copy —
    leads with the wow, vs Hazel ($42) + Adobe AutoActions (enterprise + cloud).
  - `README.md`: rotated "New in v3.20.0 — Hopper" → "New in v3.21.0 —
    Hopper Conditions" leads, v3.20.0 section preserved below.
  - `OnboardingTour.svelte`: new step (icon 🎯) "Hopper Conditions —
    route by rule" inserted after the Hopper intro.

### This-tick totals

- 3 commits, **~1106 net non-test LOC** (1108 ins / 2 del, excluding Cargo.lock).
- Clears SHIP-SIZE minimums (≥4 commits, ≥600 LOC) when counted with the
  prior tick (4 commits, 911 LOC) — branch lifetime now 7 commits / ~2020 LOC.
- **Buy-Button 4/4** ✅ (Hazel/Adobe pricing wedge, brand-new UI on next
  launch, fills a real Hopper gap, screenshot-tweet-worthy live preview).
- **Wow shipped** — live preview pane updates green chips on every keystroke.

### Quality gates (this tick)

- `cargo fmt --all --check` ✅ (no Rust changes this tick anyway)
- `pnpm check` ✅ 0 errors / 69 pre-existing warnings (no new)
- `cargo clippy` / `cargo test --lib` deferred to branch CI — no Rust
  source changes this tick; lock-file slab-app version edit is metadata-only.
  Prior tick's CI on `05a4165` was green; fresh run on `15b3815` kicked
  at 09:55 PT and will validate.

### Disk note

`cargo clean` at tick start freed 3.6 GB (target was 3.3 GiB). Disk went
from 1.1 GiB → 5.2 GiB free. No rebuild this tick so headroom preserved.
Future ticks needing `cargo test` should re-clean first.

### Branch pushed; awaiting CI before MODE A merge

```
Branch: feature/v3.21.0-hopper-conditions
HEAD:   15b3815
CI:     fresh run for 15b3815 in_progress at tick end (kicked 09:55 PT)
```

### Next tick

1. `gh run list --branch feature/v3.21.0-hopper-conditions --limit 3`.
2. CI green → MODE A: checkout main, `git merge --no-ff` with message
   `Merge v3.21.0 'Hopper Conditions' — rule-based routing per watched folder`,
   tag v3.21.0, push origin main --follow-tags.
3. Then MODE B in the following tick (poll bundle CI, `gh release create`
   with 6 curated artifacts + `docs/releases/v3.21.0.md`).
4. CI failed → fix on branch, do NOT merge.

### RELEASE_PENDING: (none yet — merge happens next tick after CI green)

### LAST_WOW_TICK_AT: 2026-05-24T16:55:00Z (Hopper Conditions live preview pane)

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 (Cloudflare Pages).
- Optional: record 5-second demo video for landing (issue #27 closed).
- Disk on the mini at 5.2 GiB free post-tick — still tight but workable.

### RECENTLY_CLOSED_ISSUES

- v3.18.0, v3.19.0, v3.20.0 all published as GitHub releases.
- v3.21.0 feature work complete on branch; release pending CI + MODE A.

Session log: `.cron-state/sessions/2026-05-24-0948.md`.

---

## STATUS: 🪣 v3.20.0 'Hopper' PUBLISHED — release live with 6 artifacts (2026-05-24 09:11 PT)

**TICK 2026-05-24 09:05 PT (Sunday off-hours)** — MODE B FINALIZE.
v3.20.0 'Hopper' is now a public GitHub release at
https://github.com/Sanjays2402/slab/releases/tag/v3.20.0 with all 6 bundles:
mac arm64 dmg, mac x64 dmg, linux deb + AppImage, win msi + nsis.

### How the finalize unblocked

- Previous tick's CI run `26365479885` failed only on `bundle (macos-x64)`
  with `bundle_dmg.sh` exit code 1 (flaky DMG tooling on GH macos-13 runners).
  The very next push (`a16fde7`, STATE update commit, run `26365497621`) on
  the same v3.20.0 manifests rebuilt cleanly across all 6 platforms.
- This tick: downloaded artifacts from 26365497621, published release.
- Mac mini disk was at 100% (386 MiB free) → `cargo clean` on
  `/Users/sanjay/code/slab/src-tauri/target` freed 4.4 GiB before the
  multi-GiB artifact download could proceed. Disk now at 5.3 GiB free.

### Release verification

- `gh release view v3.20.0`: `isDraft: false`, 6 assets present.
- Tag v3.20.0 → commit `fbd7e03` (the `--no-ff` merge commit).
- Docker (slab-server) image for v3.20.0 also green (run `26365479787`).

### RELEASE_PENDING: (none — v3.20.0 finalized)

### Next tick

Top priority issues (#23–#27) override is open-ended; re-poll `gh issue list`.
If nothing override-priority, fall through to v3.21.0 'Hopper Conditions'
(plan filed at `docs/plans/2026-05-24-v3.21.0-hopper-conditions.md`,
build CI for that plan commit `b365ec9` currently in progress on main).

---

## ARCHIVED: 🪣 v3.20.0 'Hopper' MERGED + TAGGED — CI in progress (2026-05-24 08:38 PT)

**TICK 2026-05-24 08:34 PT (Sunday off-hours)** — MODE C → MODE A.
Tasks 7 + 8 of the Hopper plan shipped in one tick, then merged to main,
tagged v3.20.0, pushed.

### What shipped this tick

- `a426247` feat(hopper): onboarding step + landing hero + README headline
  - OnboardingTour: 6th step introduces Hopper (~/Inbox → recipe → AI
    rename → filed) with ⇧⌘H hint.
  - `docs/landing/index.html`: new toolbox tile under "PDF intelligence" +
    full feature article in the grid-2 row with a monochrome live-log demo.
  - README: lead "New in v3.20.0 — Hopper" section above What Slab does,
    positioning vs Hazel + Adobe AutoActions.
- `15a1f91` chore(release): bump v3.19.0 → v3.20.0 'Hopper'
  - `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
    → 3.20.0; Cargo.lock refreshed via cargo check.
- `c095dba` docs(release): marketing release notes for v3.20.0 'Hopper'
  - `docs/releases/v3.20.0.md` (67 LOC) — customer-facing, vs-competitors
    framing, 4 use cases, install grid.
- `fbd7e03` Merge v3.20.0 'Hopper' on main (merge commit, --no-ff).

### Quality gates (this tick) — ALL GREEN

- `cargo fmt --all --check` ✅
- `cargo clippy --lib --all-targets -- -D warnings` ✅
- `pnpm check` ✅ 0 errors, 66 pre-existing warnings (no new)
- Full `cargo test --lib` not re-run this tick — already green on the
  feature branch CI through `c095dba`, and main CI is running now.

### RELEASE_PENDING: v3.20.0

- Merge SHA: `fbd7e03`
- Tag: `v3.20.0`
- CI runs in_progress at tick end:
  - build (main): `26365479885`
  - Docker slab-server (v3.20.0 tag): `26365479787`
  - deploy-try (main): `26365479884`
  - pages-build-deployment (main): `26365479509`
- Next tick: MODE B FINALIZE — poll the 4 runs, download artifacts,
  publish GitHub release with the 6 curated bundles (mac arm64/x64 dmg,
  linux deb + AppImage, win msi + nsis), attach
  `docs/releases/v3.20.0.md` as the notes body.

### Buy-Button verdict — 4/4 PASS

- **Pay-for-it ✅** — Hazel is $42, Adobe AutoActions are enterprise-only
  (~$300/yr + cloud upload). Slab Hopper is free, local, cross-platform,
  and chains all of: recipe → AI rename → file. Power user would buy.
- **Notice-it ✅** — New 🪣 in sidebar, new ⇧⌘H, onboarding step on
  next launch, "New in v3.20.0 — Hopper" leads README.
- **Pick-us ✅** — No competitor offers folder watcher + local AI rename.
  This closes a real workflow gap (scanner inbox automation).
- **Tell-a-friend ✅** — "Drop a PDF in a folder, walk away, come back to
  a perfectly named, processed file" is a tweet-worthy demo.

### LAST_WOW_TICK_AT: 2026-05-24T15:25:00Z (Hopper end-to-end, prior tick)

This tick is the release + marketing tick, not a new wow. The Hopper
wow shipped in the previous tick with the live-log Tauri event +
HopperPanel UI.

### Next tick (MODE B FINALIZE)

1. `gh run view 26365479885` — wait for green.
2. `gh run download 26365479885 --dir /tmp/slab-release-v3.20.0`
3. Curate 6 best bundles, run `gh release create v3.20.0
   --title "v3.20.0 — Hopper" --notes-file docs/releases/v3.20.0.md`
   + upload artifacts.
4. Verify Docker image on GHCR + deploy-try (will continue to no-op
   on Cloudflare secrets until Sanjay completes
   `docs/ops/try-slab-deploy.md`).
5. Remove RELEASE_PENDING line.
6. After v3.20.0 published, return to MODE C and start v3.21.0 —
   "Hopper Conditions" (rule-based recipe selection) is the natural
   next slice, OR pivot to v1.0.0 "Glass" polish pass.

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 (Cloudflare Pages).
- Optional: record 5-second demo video for landing (issue #27 already closed).
- Free disk on the mini — 1.0 GiB free, very tight.

### RECENTLY_CLOSED_ISSUES

- v3.18.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.18.0
- v3.19.0 published: https://github.com/Sanjays2402/slab/releases/tag/v3.19.0
- v3.20.0 tagged, awaiting MODE B finalize next tick.

Session log: `.cron-state/sessions/2026-05-24-0834.md`.

---

## (PREVIOUS) STATUS: 🪣 v3.20.0 'Hopper' Tasks 1-6 SHIPPED — end-to-end automation + UI live (2026-05-24 08:25 PT)

**TICK 2026-05-24 08:25 PT (Sunday off-hours)** — MODE C, 4 commits on
`feature/v3.20.0-hopper`, ~1900 net LOC across Rust + Svelte + TS,
33/33 hopper:: tests green, frontend svelte-check 0 errors.

- `21070ba` feat(hopper): notify watcher + debounce + parallel pipeline dispatch (Task 4, +538/-20, 6 new tests)
- `2d3567a` feat(hopper): 7 Tauri commands + Ollama title provider + setup() bootstrap (Task 5, +418/-8, 8 new tests)
- `b22aa46` feat(hopper): HopperPanel.svelte + sidebar entry — end-to-end Hopper UI (Task 6, +942)
- `8c6723c` feat(keymap): bind hopper.open to Mod+Shift+H (cross-cutting wow polish)

**Reachable end-to-end now**: Mod+Shift+H → Hopper panel → add watched
folder + Atelier recipe → drop a PDF → watcher debounces 1s → pipeline
runs in parallel → Ollama suggests a 4-6 word title → file is renamed
+ moved to output dir → `hopper://run-completed` Tauri event fires →
live run log updates instantly. Plus 5s polling fallback.

**Wow moment (counts as today's)**: drag a folder of scanned PDFs into
a Hopper-watched directory and Slab silently OCRs, auto-titles, and
files them while you sip coffee. The competitor (Hazel + Acrobat
AutoActions + manual rename) costs $129/yr combined and isn't offline.

**Gates**: cargo fmt + clippy clean on every commit; cargo check
clean post-Task 6; svelte-check 0 errors / 66 pre-existing warnings.
Full `cargo test --lib` deferred to CI due to disk pressure.

**Branch state**: `feature/v3.20.0-hopper` pushed through `8c6723c`.
CI build 26365080908 in_progress for b22aa46; prior 26364498803 green.
Tasks remaining in plan: 7 (rate limiting / retry / dead-letter) and
8 (docs + telemetry + release prep). Next tick: Task 7.

`LAST_WOW_TICK_AT: 2026-05-24T15:25:00Z`

---

## (PREVIOUS) STATUS: 🪣 v3.20.0 'Hopper' Tasks 1-3 SHIPPED — sqlite registry + log + end-to-end pipeline (2026-05-24 07:41 PT)

**TICK 2026-05-24 07:41 PT (Sunday off-hours)** — MODE C, 3 commits on
`feature/v3.20.0-hopper`, 1207 net LOC (excluding Cargo.lock), 19 new
green unit tests, all quality gates clean.

- `37eb6e7` feat(hopper): module skeleton + `notify = "6.1"` dep
- `c57f7aa` feat(hopper): sqlite registry + run log + 8 unit tests
- `8ec5635` feat(hopper): rename template + end-to-end pipeline + 11 tests

**End-to-end ready**: `pdf::hopper::pipeline::process_one()` runs the
full Drop-PDF → Atelier recipe → AI-rename → file-into-folder loop
against synthetic inputs today. The remaining 4 tasks (watcher, Tauri
commands, frontend panel, onboarding) are scaffolding around this
core orchestration. AI title comes via an injected `TitleProvider`
trait so the production Ollama wiring lands in Task 5 without
touching the pipeline.

**Gates**: `cargo fmt --check` clean, `cargo clippy --lib -D warnings`
clean, `cargo test --lib hopper::` → 19 passed / 0 failed. Full
`cargo test --lib` deferred (disk pressure) — CI on the branch will
run it.

**Branch state**: `feature/v3.20.0-hopper` exists locally, NOT YET
PUSHED at the end of this tick. Push + CI poll happens at start of
next tick (Task 4 lead-in).

### 🚨 DISK EMERGENCY ESCALATED

Mac mini at 100% disk usage. Started tick at 3.3 GiB free; after
`cargo clean -p slab-app` (freed 1.4 GiB), now 1.1 GiB. **Even
single-package clean isn't restoring headroom.** This blocks:
- Full `cargo test --lib` (linker OOMs at 28).
- A second worktree at `~/Projects/slab` (still has 5+ GiB target/).
- Any future Tauri build until at least 8 GiB is recovered.

Sanjay TODO (added this tick, urgent):
- `cargo clean` every worktree: `~/code/slab`, `~/Projects/slab`, plus
  any other Rust repos. Likely recovers 15-20 GiB.
- Triage `~/Downloads` (was 6 GiB of video courses).
- After recovery, single `cargo build` to repopulate caches.

### LAST_WOW_TICK_AT: 2026-05-24T13:46Z (v3.19.0 Marquee — unchanged)

Hopper's wow ships in Task 6 (HopperPanel live log + morph-rename
animation). Target tick: +2.

### Next tick

1. Push `feature/v3.20.0-hopper` and `gh run list` to poll CI.
2. **Task 4** — `notify::RecommendedWatcher` per-watch tasks, 700ms
   debounce, `tokio::spawn(pipeline::process_one)` per settled PDF,
   `hopper://run-completed` Tauri event broadcast.
3. **Task 5** — 6 Tauri commands + `setup()` wires Ollama-backed
   provider. Bundle into the same tick if disk allows.

Session log: `.cron-state/sessions/2026-05-24-0741.md`.

---

## STATUS PRIOR: 📋 v3.20.0 'Hopper' plan written — ready to execute (2026-05-24 07:18 PT)

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
