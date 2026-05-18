# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🪑 v1.4.0 "Bench" Slice 1 IN PROGRESS — feature/v1.4.0-bench

**Main HEAD**: `9f0fd6f` — `Merge v1.3.1 'Foundry Patch' — cross-platform test fixes (Linux + Windows CI)`
**Active feature branch**: `feature/v1.4.0-bench` @ `6ad9fb0` — Slice 1: marketplace index + Ed25519 verifier (4 commits)
**Latest release**: v1.3.1 → https://github.com/Sanjays2402/slab/releases/tag/v1.3.1 — all 6 assets uploaded ✓

**Quality gates green on feature/v1.4.0-bench HEAD:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (**551 passed**, +12 new marketplace tests over v1.3.1 baseline of 539)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

---

## TICK 2026-05-17 21:13 PT — v1.3.1 finalized + v1.4.0 Slice 1 in one tick (4 commits)

**MODE B finalize v1.3.1:**
- CI run 26012655931 went green at 04:11 UTC (Windows bundle finished while I was investigating).
- `gh release create v1.3.1` with `docs/release-notes/v1.3.1.md` + all 6 assets:
  - macos-arm64 dmg, macos-x64 dmg, linux deb + AppImage, windows msi + nsis.
- v1.3.1 RELEASED.

**MODE C develop v1.4.0 "Bench" Slice 1:** (on `feature/v1.4.0-bench`)
1. `daa10d9` — chore(deps): add ed25519-dalek (pure-Rust, std features only)
2. `6f2a7fe` — feat(marketplace): scaffold module + define Index/IndexEntry schema (4 tests)
3. `f5552f4` — feat(marketplace): Ed25519 signature verifier (8 tests, covers tampering / wrong-key / bad-base64 / placeholder fail-closed)
4. `6ad9fb0` — docs(plans): v1.4.0 proposal (11 slices) + Slice 1 implementation plan

**Slice 1 done locally. 12/12 marketplace tests green; all quality gates green.**

Will push the branch next tick after one more sanity polish + plan Slice 2.

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
### v1.3.1 "Foundry Patch" 🩹 — **RELEASED 2026-05-17** ✓
### v1.4.0 "Bench" 🪑 — **IN PROGRESS** (Slice 1/11 local-done; 12 new tests; not yet pushed)

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — MODE C continue v1.4.0 Bench

1. **Push the v1.4.0-bench branch** (Slice 1 commits) with the gh-auth credential helper:
   ```
   TOK=$(gh auth token)
   git -c credential.helper="!f() { printf 'username=x-access-token\npassword=%s\n' '$TOK'; }; f" \
       push -u origin feature/v1.4.0-bench
   ```
2. **Ship Slice 2** — maintainer signing tool `tools/sign-plugin/src/main.rs`:
   - CLI that takes a tarball + private key file
   - Emits JSON entry ready to paste into `index.json`
   - Generates a real Ed25519 key pair (the maintainer's), bakes the
     public key into `marketplace::verify::MAINTAINER_PUBLIC_KEY`
   - Private key goes into `~/.slab-maintainer-key` (out of tree, NOT committed)
3. **Ship Slice 3** — `marketplace/fetch.rs` HTTP GET + offline cache:
   - reqwest GET of the curated `index.json`
   - Cache to `~/.slab/marketplace-cache.json`
   - Stale-cache fallback on network failure
4. End the tick by pushing all of Slices 2+3 together (≥6 commits in one push).
5. Update STATE.md with what shipped.

---

## v1.4.0 "Bench" Slice plan summary (full spec in `.cron-state/proposals/v1.4.0-bench.md`)

1. ✅ Marketplace index schema + Ed25519 verifier (this tick)
2. Maintainer signing tool (`tools/sign-plugin/`)
3. `marketplace/fetch.rs` — HTTP + offline cache
4. `marketplace/install.rs` — download, sha256 verify, atomic extract
5. Tauri commands `slab_marketplace_*`
6. Frontend `src/lib/marketplace.ts` store
7. Frontend Browse tab + plugin cards
8. Frontend install modal + update-available badges
9. Uninstall flow
10. Docs + seed `slab-plugins` repo with 3 example plugins
11. Release — version bump 1.3.1 → 1.4.0 + notes + merge + tag + push

---

## POST-v1.4 ROADMAP REMINDERS

- v1.5.0 "TypeScript Plugins" — V8/QuickJS sandbox for `script.js` contribs (bigger lift; security-heavy)
- v1.5.x — AI provider hook-up of plugin-contributed providers through Beacon's runtime
- v1.5.x — Slab CLI `slab plugin install <url>` command
- Beacon Bonus Slices (`.cron-state/proposals/v0.10.0-beacon-bonus-slices.md`) — Smart Outline, Citations, Study Mode, Glossary, Voice Mode
