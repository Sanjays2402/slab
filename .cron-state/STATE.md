# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: SHIPPING (v0.8.1 + v0.9.0 merged to main; v0.9.1 dev still running)

**Active dev branch:** `feature/v0.9.1-toolkit-ux` (now linear on `main` after v0.9.0 merge)

**Releases pending CI finalize:**
- `RELEASE_PENDING: v0.8.1` — merge SHA `39ff562` on main, tag `v0.8.1`, CI run `25980368257`
- `RELEASE_PENDING: v0.9.0` — merge SHA `ba3b291` on main, tag `v0.9.0`, CI run `25980394616`

Both runs `in_progress` as of 2026-05-16 20:36 PDT. Bundle matrix
takes ~15-25 min (macos-arm64 + macos-x64 + linux-x64 + windows-x64).

---

## ROADMAP

### v0.8.1 "Polyglot" — MERGED to main 2026-05-16
- Tag: `v0.8.1`, merge SHA `39ff562`
- CI run: `25980394616` — POLL NEXT TICK
- Next tick MODE B: `gh run download` artifacts → `gh release create v0.8.1`

### v0.9.0 "Toolkit" — MERGED to main 2026-05-16
- Tag: `v0.9.0`, merge SHA `ba3b291`
- CI run: `25980394616` — POLL NEXT TICK
- Next tick MODE B: `gh run download` artifacts → `gh release create v0.9.0`

### v0.9.1 "Toolkit UX" — IN PROGRESS (Tick-3 remaining)
- Branch: `feature/v0.9.1-toolkit-ux`, tip `c1dc5c5`
- Tick-1 + Tick-2 shipped already (FlattenPanel/SanitizePanel/RepairPanel + nav)
- **Tick-3 to do** (one tick = ship + merge):
  - D1: Detect encrypted-PDF error in ReaderPanel (catch PasswordException)
  - D2: Create `src/lib/components/DecryptModal.svelte`
  - D3: Wire DecryptModal into ReaderPanel
  - D4: Manual smoke (encrypt → open locked → modal → unlock)
  - D5: Commit decrypt modal
  - R1: Version bump 0.9.0 → 0.9.1 (`package.json`, `src-tauri/Cargo.toml`, `tauri.conf.json`, footer label, refresh Cargo.lock in same commit)
  - R2: README + release notes (`docs/release-notes/v0.9.1.md`)
  - R3: Final quality gates (fmt/clippy/test/svelte-check)
  - R4: Flip STATE to DONE → next-next tick MODE A merges

### v0.10.0+ — TBD
Propose next: PDF/A conversion (Ghostscript shell-out), signing
(PKCS#7 crate selection needed), or wholesale UI redesign for the
Toolkit panel.

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. If RELEASE_PENDING set AND CI for that tag is "success":
     → MODE B (download artifacts, gh release create, clear RELEASE_PENDING)
3. Else if any feature branch has STATUS: DONE locally and was not merged:
     → MODE A (merge --no-ff to main, tag, push)
4. Else:
     → MODE C (develop next feature on active branch)
5. Mode chaining is allowed within a tick if there's time.
```

---

## QUICK REFERENCE

### Quality gates (run from `src-tauri/`):
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib`
- `pnpm exec svelte-check` (run from repo root)

### Push (manual auth needed):
```bash
GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin <branch-or-tag>
```

### Merge to main (PERMISSIONS GRANTED 2026-05-16):
```bash
# Verify gates on feature branch first
git checkout main && git pull --ff-only
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    merge --no-ff feature/vX.Y.Z-name -F /tmp/merge-msg-vX.Y.Z.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    tag -a vX.Y.Z -m "Slab vX.Y.Z — Codename"
# push main, push tag (separate calls)
```

### NO PRs.
Direct merge to main is the workflow. Branch protection on main is OFF.
Never run `gh pr create`.

### Commit author:
```bash
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit ...
```

### Gotchas
- `TMPDIR` stale: if `mktemp -d` left a deleted dir as `TMPDIR`, tests
  fail with `PathError NotFound`. Workaround: `unset TMPDIR` before tests.
- Version bump lockstep: editing `src-tauri/Cargo.toml` version requires
  running `cargo build` and committing `Cargo.lock` in the SAME commit.
- `markitdown` runtime: `/Users/sanjay/.local/bin/markitdown` (pipx).
  Add `$HOME/.local/bin` to PATH for cron-spawned terminals.
- `CmdResult<T>` field on `"ok"` variant is `value`, NOT `data`.
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁

### Release asset naming
- Mac x64 dmg needs `_x64_macos.dmg` rename (disambiguate from Windows x64).
- Standard set: 1 dmg per mac arch + 1 deb + 1 AppImage (linux) + 1 msi + 1 setup.exe (windows).

---

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises.
- 2026-05-16 17:00 (Cake/cron): Task 2 done. Pure-fn allow-list + 3 tests. fmt/clippy clean, full suite 84 pass (81→84). Pushed `c66167c`.
- 2026-05-16 17:18 (Cake/cron): Task 3 done. `require_markitdown()` + `markitdown_available()` test gate. Suite 84→86 (+2). Pushed `5b7d9b9`. **Plan deviation**: replaced literal-error-format test with two real preflight tests gated on `markitdown_available()`.
- 2026-05-16 17:37 (Cake/cron): Task 4 done. Real pipeline + 2 cheap unit tests. Suite 86→88 (+2). Pushed `37b9356`.
- 2026-05-16 17:55 (Cake/cron): Task 5 done. html_round_trip test added. Pushed `dff9ca0`.
- 2026-05-16 18:12 (Cake/cron): **Aggressive tick — shipped 7 sub-tasks** (Tasks 6,7,8,9,11,12,13). Backend + CLI + Tauri + docs + version bump. Suite 89→90.
- 2026-05-16 18:43 (Cake/cron): **Closeout tick** — shipped Task 10, verified Task 14 manually, v0.8.1 PR_READY: true. Commit: `ac401be`.
- 2026-05-16 19:05 (Cake/cron): **v0.9.0 kickoff** — Feature A (flatten) + Feature B (sanitize) shipped in one tick.
- 2026-05-16 19:34 (Cake/cron): **v0.9.0 closeout** — picked up WIP repair backend, wired CLI + Tauri, version bumped, release notes. 101 lib tests. PR_READY.
- 2026-05-16 20:00 (Cake/cron): **Held-pattern tick** — fixed Cargo.lock drift `60945b6`, drafted v0.9.1 plan into proposals/.
- 2026-05-16 20:21 (Cake/cron): **Override-and-ship tick** — stacked v0.9.1 on top of v0.9.0. Shipped Tick-1 + Tick-2 in one tick: plan promotion + FlattenPanel + SanitizePanel + nav + RepairPanel + nav. 4 quality gates green.
- **2026-05-16 20:36 (Cake/cron): 🚀🚀 DOUBLE-RELEASE TICK** — Sanjay granted direct-merge permission. Cleared 3-tick PR_READY backlog: merged both `feature/v0.8.1-polyglot` (SHA `39ff562`, tag `v0.8.1`) and `feature/v0.9.0-toolkit` (SHA `ba3b291`, tag `v0.9.0`) to main. Both CI runs queued (`25980368257` + `25980394616`). Quality gates re-verified on each before merge (90 + 101 lib tests). Next tick: MODE B finalize once CI succeeds.

