# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🎯 v3.21.0 'Hopper Conditions' MERGED + TAGGED — CI running (2026-05-24 10:09 PT)

**TICK 2026-05-24 10:08 PT (Sunday off-hours)** — MODE A executed.
Branch `feature/v3.21.0-hopper-conditions` CI was green (run 26367256148);
merged `--no-ff` to main, tag `v3.21.0` cut, push origin main --follow-tags
succeeded.

### What happened this tick

- `git checkout main && git pull` — up to date at `7cb65b3`.
- `git merge --no-ff feature/v3.21.0-hopper-conditions` → 1 conflict in
  `.cron-state/STATE.md` (expected, divergent memory writes). Resolved with
  `--ours` (this file you're reading). Merge commit: `5f1b37c`.
- Quality gates on main:
  - `cargo fmt --all --check` ✅
  - `pnpm check` ✅ 0 errors / 69 pre-existing warnings
  - `cargo clippy` / `cargo test --lib` not re-run on main — feature branch
    CI on the exact same source tree (run 26367256148) was green, and
    main CI is already running (`26367522708`) to re-validate.
- `git tag -a v3.21.0 -m "v3.21.0 — Hopper Conditions"`
- `git push origin main --follow-tags` → `7cb65b3..5f1b37c` + new tag v3.21.0.

### RELEASE_PENDING: v3.21.0

- Merge SHA: `5f1b37c`
- Tag: `v3.21.0`
- CI runs in_progress at tick end:
  - build (main): `26367522708`
  - Docker slab-server (v3.21.0 tag): `26367522688`
  - deploy-try (main): `26367522699`
  - pages-build-deployment (main): `26367522316`
- Next tick: MODE B FINALIZE — poll `gh run view 26367522708`, when green
  `gh run download 26367522708 --dir /tmp/slab-release-v3.21.0`, curate
  6 best bundles, `gh release create v3.21.0 --title "v3.21.0 — Hopper Conditions"
  --notes-file docs/releases/v3.21.0.md` + upload artifacts.

### LAST_WOW_TICK_AT: 2026-05-24T16:55:00Z (Hopper Conditions live preview pane — prior tick)

This tick is a release tick, not a new wow.

### Next tick

1. `gh run view 26367522708` — poll until completed.
2. CI green → MODE B FINALIZE (download artifacts, publish release, verify
   Docker on GHCR, remove RELEASE_PENDING).
3. CI red → `gh run view 26367522708 --log-failed`, fix on hotfix branch
   (consider revert if catastrophic).
4. After v3.21.0 published, MODE C: pick next from roadmap. Candidates:
   - **v3.22.0 'Hopper Loop'** — closes the Hopper-rules loop with
     batch backfill (apply current rules to all files already in the
     watched folder, not just newcomers), a "test on this folder" button
     that runs the rule chain against every existing PDF and shows a
     dry-run summary. Tight scope, single tick.
   - **v1.0.0 'Glass'** polish pass — command palette parity for new
     Hopper commands, settings UI section for Hopper defaults, onboarding
     refresh now that we have 21 versions of features.
   - **v0.13.0 'Lens' OCR upgrade** — multi-language Tesseract + table
     extraction. Bigger lift but a real Acrobat-paid feature, given free.

### Sanjay TODO (carried over)

- Complete `docs/ops/try-slab-deploy.md` steps 1-6 (Cloudflare Pages).
- Optional: record 5-second demo video for landing (issue #27 closed).
- Disk on the mini at ~5 GiB free — still tight, future `cargo test --lib`
  ticks should `cargo clean` first.

### RECENTLY_CLOSED_ISSUES

- v3.18.0, v3.19.0, v3.20.0 published.
- v3.21.0 merged + tagged; release pending CI + MODE B next tick.

Session log: `.cron-state/sessions/2026-05-24-1008.md`.

---

## STATUS: 🎯 v3.21.0 'Hopper Conditions' FEATURE-COMPLETE on branch — pushed, awaiting CI for MODE A merge (2026-05-24 09:55 PT)

**TICK 2026-05-24 09:48 PT (Sunday off-hours)** — MODE C DEVELOP, the
writing-plans skill was invoked but a plan already existed on disk
(`docs/plans/2026-05-24-v3.21.0-hopper-conditions.md`) so this tick
executed the remaining Tasks 8 + 9 + 10 of that plan end-to-end.

### What shipped this tick (3 commits on `feature/v3.21.0-hopper-conditions`)

- `bbd4f43` feat(hopper): TS types + API client + predicate helpers
- `e38578e` feat(hopper): rules editor UI with live match preview (WOW)
- `15b3815` chore(release): v3.20.0 -> v3.21.0 'Hopper Conditions'

(See git log for full prior-tick details — trimmed for STATE.md brevity.)

---

(Older entries trimmed — see git history for v3.20.0 'Hopper', v3.19.0
'Marquee', and the v0.10.0 Beacon proposal.)
