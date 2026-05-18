# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🚢 v1.3.0 "Foundry" MERGED + TAGGED — release pending CI

**Main HEAD**: `e10fcbd` — `Merge v1.3.0 'Foundry' — declarative plugin system`
**Pre-merge HEAD**: `bdcba0f` — `docs(README): bring up to v1.2.0 "Glass II"`
**Tag**: `v1.3.0` → `e10fcbd` (pushed to origin)
**Feature branch HEAD**: `8703bf3` — `chore(release): v1.3.0 "Foundry" — declarative plugin system`

**Quality gates green on main HEAD (e10fcbd):**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (**539 passed**)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

**RELEASE_PENDING: v1.3.0 — merge SHA `e10fcbd`, tag `v1.3.0`, CI run `26011887503`**
- Run URL: https://github.com/Sanjays2402/slab/actions/runs/26011887503
- Status as of 20:28 PT: `in_progress`
- Next tick: MODE B — poll CI; if green, download artifacts (`gh run download 26011887503 --dir /tmp/slab-release-v1.3.0`), curate 6 best (macos arm64 + x64 dmgs, linux x64 deb + AppImage, windows msi + nsis), create GH release with `docs/release-notes/v1.3.0.md` as notes.

**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓

---

## TICK 2026-05-17 20:28 PT — Foundry Slice 12 = ship v1.3.0 (1 commit + merge + tag + push)

**Slice 12 / 12 — version bump + release notes + merge + tag + push.**
Foundry is now feature-complete AND tagged. CI is the only gate left
before the release is live.

### What shipped this tick
1. **Version bump** across `package.json`, `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json` → 1.3.0. `cargo check` refreshed
   `Cargo.lock`.
2. **README**: heading "What's in v1.3.0 Foundry" + 8th pillar
   ("Extensible — Foundry — declarative plugin system…") + test
   badge 468 → 539.
3. **`docs/release-notes/v1.3.0.md`** — full Foundry shipping notes:
   the 5 contribution kinds with worked semantics, control surface
   (Settings → Plugins), hello-slab example, the honest security
   framing (declarative, NOT sandboxed), numbers table
   (468→539 tests, +13 Tauri commands, +88 i18n strings), and a
   "what's NOT in here for v1.3.x" section (marketplace, AI
   hot-swap, JS plugins).
4. **Quality gates** green on feature branch THEN on main after
   merge: fmt + clippy + 539 lib tests + pnpm-check (0 errors).
5. **Merged** `feature/v1.3.0-foundry` → `main` with `--no-ff` →
   merge commit `e10fcbd`.
6. **Tagged** `v1.3.0` on `e10fcbd`.
7. **Pushed** main + tag + feature branch (auth-helper dance).

### Commits
- `8703bf3` — chore(release): v1.3.0 "Foundry" — declarative plugin system
- `e10fcbd` — Merge v1.3.0 'Foundry' — declarative plugin system

---

## ROADMAP

### v0.8.1 "Polyglot" — RELEASED 2026-05-16
### v0.9.0 "Toolkit" — RELEASED 2026-05-16
### v0.9.1 "Toolkit UX" — RELEASED 2026-05-16
### v0.10.0 "Beacon" — RELEASED 2026-05-17
### v0.11.0 "Lathe" — RELEASED 2026-05-17
### v0.12.0 "Atlas" — TAGGED, NOT RELEASED (CI artifacts skipped)
### v0.13.0 "Lens" — TAGGED, NOT RELEASED (Windows pdftotext bug)
### v0.13.1 "Lens Patch" — RELEASED 2026-05-17
### v0.14.0 "Stack" — RELEASED 2026-05-17 (diff & compare)
### v0.15.0 "Theater" — RELEASED 2026-05-17 (presenter mode)
### v1.0.0 "Glass" — RELEASED 2026-05-17 🎉🪟
### v1.1.0 "Cabinet" — RELEASED 2026-05-17 🗄
### v1.2.0 "Glass II" — RELEASED 2026-05-17 🪟²
### v1.3.0 "Foundry" 🛠 — **MERGED + TAGGED 2026-05-17, AWAITING CI** (12/12 slices done)

### v1.3.0 Slice ledger — ALL ✅
- ✅ Slice 1 — manifest schema + parser + validation
- ✅ Slice 2 — plugin registry + discovery loop
- ✅ Slice 3 — Tauri commands (list/enable/disable/reload)
- ✅ Slice 4 — theme contribution + asset reader
- ✅ Slice 5 — locale contribution + bundle loader
- ✅ Slice 6 — pdf_action contribution + CLI runner
- ✅ Slice 7 — command contribution + shell/url runner
- ✅ Slice 8 — ai_provider contribution + materialiser
- ✅ Slice 9 — frontend wiring (8 commits)
- ✅ Slice 10 — Settings → Plugins panel UI (4 commits)
- ✅ Slice 11 — example plugin + PLUGINS.md (3 commits)
- ✅ Slice 12 — version bump + release notes + merge + tag + push (this tick)

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — MODE B finalize v1.3.0

1. **Poll CI**: `gh run view 26011887503` — if still in_progress, write a one-line status and exit silently (or just say "CI still running"). If failed, write `RELEASE_FAILED:` + run_id + failing job, and consider revert vs forward-fix.
2. **If CI green**:
   - `gh run download 26011887503 --dir /tmp/slab-release-v1.3.0`
   - Inspect the artifact tree. We expect:
     - macos-arm64 dmg
     - macos-x64 dmg
     - linux x64 deb + AppImage
     - windows msi + nsis
   - `gh release create v1.3.0 --title 'v1.3.0 — Foundry 🛠' --notes-file docs/release-notes/v1.3.0.md /tmp/slab-release-v1.3.0/<asset1> /tmp/slab-release-v1.3.0/<asset2> ...`
   - Verify on the GH release page (6 assets, notes rendered).
3. **Remove `RELEASE_PENDING:` from STATE.md** + flip v1.3.0 in the roadmap to "RELEASED 2026-05-17".
4. Then start the next version. See "POST-v1.3 ROADMAP" below.

---

## POST-v1.3 ROADMAP REMINDERS

After v1.3.0 ships, candidate next versions:

**Option A — v1.4.0 "Bench" (plugin marketplace + signed manifests)**
- A read-only marketplace UI inside Settings → Plugins showing a
  curated GitHub-hosted index (JSON list of plugins).
- Signed-manifest install flow: each plugin in the index has a
  `manifest.sig` we verify against a hardcoded public key.
- One-click install: download tarball into `~/.slab/plugins/<id>/`.
- Persists in `~/.slab/plugins-index.json` for offline.
- Probably 8–10 slices. Backend: HTTP client + sigverify. Frontend:
  Marketplace tab in PluginsPanel + install confirmation modal.

**Option B — Beacon Bonus Slices (`.cron-state/proposals/v0.10.0-beacon-bonus-slices.md`)**
- Smart Outline, Citations, Study Mode, Glossary, Voice Mode — five
  AI features riding the existing Beacon infra. Quick wins.

**Option C — v1.4.0 "TypeScript Plugins"** — a `script.js`
contribution kind running in an embedded V8/QuickJS sandbox. Bigger
project; would let plugins do real frontend work (custom panels).
Risk: sandbox security is hard.

My recommendation: **Option A (Bench)** next. It completes the
Foundry story (you can now distribute plugins, not just write them)
and re-uses everything we just shipped.

Other parked items:
- AI provider hook-up of plugin-contributed providers through Beacon's
  runtime (planned v1.3.x patch — currently they appear in the
  palette + boot log but aren't yet selectable in chat)
- Slab CLI `slab plugin install <url>` command (post-Bench)
