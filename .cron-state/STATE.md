# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: ✦ v1.9.2 "Voice Mode: Polish" 🎙 — MERGED + TAGGED + PUSHED, CI in flight

**Main HEAD**: `cc51f72` (merge commit v1.9.2).
**Latest tag**: `v1.9.2` (annotated, pushed).
**RELEASE_PENDING**: **v1.9.2** — merge SHA `cc51f72`, tag `v1.9.2`, CI run `26071609861`. Finalize next tick.

**v2.0.0 spec authored** at `.cron-state/proposals/v2.0.0-workshop.md` (12 slices, ~48 commits, TypeScript Plugins via QuickJS). Pre-flight gate from previous STATE is now resolved.

---

## TICK 2026-05-18 18:55 PT — MODE A merge v1.9.2 + author v2.0.0 spec (BIG ROADMAP)

Cron tick at 18:55 PT (5min into off-hours window). Shipped a release + a roadmap doc.

**Descope decision**: v1.9.2 T6 (Windows-native cpal recorder scaffold) deferred to v1.9.3. Per plan §1327 explicit escape hatch — Tasks 1-5 ship as v1.9.2, T6 lands when WASAPI implementation is real (not a `todo!()` stub). Justification: T6 is perf-only delta on Windows (PowerShell shell-out from v1.9.1 still works fine); holding v1.9.2 for T6 robs macOS+Linux users of the polish slice; "scaffolding only" commit doesn't qualify as "big" per Sanjay's directive.

**Commits this tick:**
- `aad0b61` fix(clippy): drop useless into_iter() in list_whisper_models (uncommitted from prior tick)
- `8f5d1df` chore(release): v1.9.2 — version bumps + release notes
- `cc51f72` Merge v1.9.2 'Voice Mode: Polish' 🎙 (merge commit on main)
- Plus `.cron-state/proposals/v2.0.0-workshop.md` (15 KB spec; not committed to git, lives in the proposals cabinet)

**Quality gates on main post-merge:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **736 passed / 0 failed**
- `pnpm check` — 0 errors, 23 pre-existing warnings (unchanged)

**Push:** `main` → `cc51f72` + tag `v1.9.2` via `--follow-tags`. CI run `26071609861` triggered (in_progress at tick end — first 3/4 jobs through "Cache cargo" step at end of tick window).

---

## NEXT TICK PLAYBOOK

### Step 1 — MODE B finalize v1.9.2

CI run `26071609861` for `main @ cc51f72`. Poll:
```bash
gh run view 26071609861 --json status,conclusion
```

If `conclusion = success`:
```bash
gh run download 26071609861 --dir /tmp/slab-release-1.9.2
gh release create v1.9.2 \
  --title 'v1.9.2 — Voice Mode: Polish 🎙' \
  --notes-file docs/release-notes/v1.9.2.md \
  /tmp/slab-release-1.9.2/macos-arm64/Slab_1.9.2_aarch64.dmg \
  /tmp/slab-release-1.9.2/macos-x64/Slab_1.9.2_x64.dmg \
  /tmp/slab-release-1.9.2/linux-x64/Slab_1.9.2_amd64.deb \
  /tmp/slab-release-1.9.2/linux-x64/Slab_1.9.2_amd64.AppImage \
  /tmp/slab-release-1.9.2/windows-x64/Slab_1.9.2_x64_en-US.msi \
  /tmp/slab-release-1.9.2/windows-x64/Slab_1.9.2_x64-setup.exe
```
(Asset paths from v1.9.1 finalize tick. Adjust per `gh run download` actual layout.)

Then clear `RELEASE_PENDING` line above + proceed to Step 2.

If CI fails: write `RELEASE_FAILED: v1.9.2 CI run 26071609861 — <failing job>` to STATE.md, fix on a follow-up branch.

### Step 2 — MODE C start v2.0.0 "Workshop"

After v1.9.2 finalized:
1. Promote `.cron-state/proposals/v2.0.0-workshop.md` → `docs/plans/2026-05-XX-v2.0.0-workshop.md` (commit as `docs(plan): v2.0.0 Workshop — TypeScript Plugins`).
2. `git checkout -b feature/v2.0.0-workshop main`.
3. Ship **Slice 1 — QuickJS embedding + sandboxed console.log** (4-5 commits, +6 tests). See spec for slice details.
4. Aggressive: pair Slice 1 + Slice 2 (manifest schema bump + script load) in the same tick = 7-8 commits and ~+11 tests. That's BIG per Sanjay's directive.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **MERGED + TAGGED 2026-05-18, awaiting CI green to finalize**
### v1.9.3 "Voice Mode: Windows-native" — Windows WASAPI recorder via cpal (T6 from v1.9.2 plan, plus real impl)
### v2.0.0 "Workshop" — TypeScript Plugins (QuickJS/rquickjs). **Spec at `.cron-state/proposals/v2.0.0-workshop.md`.** 12 slices, ~48 commits.

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## POST-v1.9 ROADMAP REMINDERS

**v1.9.3** — Windows-native STT (WASAPI via cpal). Real implementation, not the `todo!()` scaffold from v1.9.2 T6. Cargo feature `windows-stt`. ~3-4 commits + integration tests.

**v2.0.0 "Workshop"** — spec is now real at `.cron-state/proposals/v2.0.0-workshop.md`. When ready to start:
1. Promote to `docs/plans/`.
2. Branch `feature/v2.0.0-workshop`.
3. Ship slices in order: 1→QuickJS+console, 2→manifest schema, 3→capability prompt, 4→`slab` global, 5→Beacon tool registration, 6→panel registration, 7→fetch shim, 8→storage, 9→SDK npm pkg, 10→sample plugin+docs, 11→AI provider registration (closes parked v1.3.x TODO), 12→release.

**v2.1.0 candidates (post-Workshop):**
- **Forge** — author-signed plugins. Wants 10+ plugins in curated index before considering (Sanjay's flag).
- **Slab CLI** — `slab plugin install <url>`.
- **Plugin author cookbook** — recipes for common plugin patterns.

**Parked items (pre-existing):**
- `docs/screenshots-v1.3.1/` working copy in repo root — harmless, can `rm -rf` someday.
- CommandPalette DETACHABLE_PANELS drift — missing citations/study/glossary entries pre-existed v1.9.0; voice was added but the other three remain. Quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins` GH repo, drop seed files from `docs/marketplace-seed/`, sign the hello-slab plugin, post first real `index.json`.
