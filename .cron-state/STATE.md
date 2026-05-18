# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🩹 v1.3.1 "Foundry Patch" MERGED + TAGGED — release pending CI

**Main HEAD**: `9f0fd6f` — `Merge v1.3.1 'Foundry Patch' — cross-platform test fixes (Linux + Windows CI)`
**Pre-merge HEAD**: `e10fcbd` — `Merge v1.3.0 'Foundry'` (CI: failed)
**Tag**: `v1.3.1` → `9f0fd6f` (pushed to origin)
**Feature branch HEAD**: `df13d68` — `chore(release): v1.3.1 "Foundry Patch"`

**Quality gates green on main HEAD (9f0fd6f):**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (**539 passed**, was 538/539 on Linux pre-fix)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

**RELEASE_PENDING: v1.3.1 — merge SHA `9f0fd6f`, tag `v1.3.1`, CI run `26012655931`**
- Run URL: https://github.com/Sanjays2402/slab/actions/runs/26012655931
- Status as of 20:56 PT: `in_progress`
- Next tick: MODE B — poll CI; if green, `gh run download 26012655931 --dir /tmp/slab-release-v1.3.1`, curate 6 best (macos arm64 + x64 dmgs, linux x64 deb + AppImage, windows msi + nsis), create GH release with `docs/release-notes/v1.3.1.md` as notes.

**v1.3.0**: tagged but CI failed — superseded by v1.3.1. No release artifacts for v1.3.0 (we will NOT publish v1.3.0 — it has been re-tagged to v1.3.1 which is functionally identical aside from three test fixes).
**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓

---

## TICK 2026-05-17 20:50 PT — v1.3.1 hotfix tick (4 commits, merged + tagged + pushed)

**v1.3.0 CI (run 26011887503) failed on Linux + Windows:**
- Linux: `shell_timeout_kills_long_running` took 10s (timeout fired, but stale grandchild held pipes open, blocking `read_to_string`)
- Windows: `read_asset_rejects_absolute_path` + `rejects_absolute_path` — `Path::is_absolute("/etc/passwd")` is `false` on Windows
- macOS: green throughout

**Fixes (4 commits on `feature/v1.3.1-foundry-patch`):**
1. `13e1bf0` — fix(plugins): don't block on stale pipes after shell-command timeout — drop pipe handles on timeout path. Local: 10.0s → 0.32s.
2. `ab722c3` — test(plugins): use platform-appropriate absolute paths — `#[cfg(windows)]` picks `C:\Windows\System32\drivers\etc\hosts`.
3. `df13d68` — chore(release): v1.3.1 "Foundry Patch" — version bump (1.3.0 → 1.3.1) across `package.json`, `Cargo.toml`, `tauri.conf.json`, `Cargo.lock` + `docs/release-notes/v1.3.1.md` + plan doc.
4. `9f0fd6f` — Merge v1.3.1 'Foundry Patch' (merge commit on `main`)

**Tagged + pushed `v1.3.1` → `9f0fd6f`. CI run `26012655931` in flight.**

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
### v1.3.0 "Foundry" 🛠 — TAGGED but CI failed, superseded by v1.3.1
### v1.3.1 "Foundry Patch" 🩹 — **MERGED + TAGGED 2026-05-17, AWAITING CI 26012655931**

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — MODE B finalize v1.3.1

1. **Poll CI**: `gh run view 26012655931` — if still in_progress, write a one-line status and exit silently. If failed (would be a surprise — we ran the gates locally and fixed the only platform-specific tests that broke v1.3.0), write `RELEASE_FAILED:` + run_id + failing job, and forward-fix on a v1.3.2 patch branch.
2. **If CI green**:
   - `gh run download 26012655931 --dir /tmp/slab-release-v1.3.1`
   - Inspect the artifact tree. We expect:
     - macos-arm64 dmg
     - macos-x64 dmg
     - linux x64 deb + AppImage
     - windows msi + nsis
   - `gh release create v1.3.1 --title 'v1.3.1 — Foundry Patch 🩹' --notes-file docs/release-notes/v1.3.1.md /tmp/slab-release-v1.3.1/<asset1> /tmp/slab-release-v1.3.1/<asset2> ...`
   - Verify on the GH release page (6 assets, notes rendered).
3. **Remove `RELEASE_PENDING:` from STATE.md** + flip v1.3.1 in the roadmap to "RELEASED 2026-05-17".
4. Then start the next version. See "POST-v1.3 ROADMAP" below.

---

## POST-v1.3 ROADMAP REMINDERS

After v1.3.1 ships, candidate next versions:

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
