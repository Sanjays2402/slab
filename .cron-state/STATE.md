# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🪑 v1.4.0 "Bench" Slices 1–8 SHIPPED — feature/v1.4.0-bench

**Main HEAD**: `9f0fd6f` — `Merge v1.3.1 'Foundry Patch'`
**Active feature branch**: `feature/v1.4.0-bench` @ `7087c5d` — Slices 1–8 done, pushed to origin (backend + Tauri cmds + TS store + Browse UI + plugin detail drawer + install progress modal + cross-tab update badge)
**Latest release**: v1.3.1 → https://github.com/Sanjays2402/slab/releases/tag/v1.3.1

**Quality gates green on feature/v1.4.0-bench HEAD (`7087c5d`):**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ **581 passed** (no backend changes in Slice 8)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

**Marketplace test breakdown (42):**
- index.rs: 4 (roundtrip, to_unsigned, field order, real-world JSON)
- verify.rs: 9 (correct/tampered/wrong-key/bad-b64/short/long sig, maintainer fixture verify + reject)
- fetch.rs: 14 (parse envelope checks, mockito 200/500, cache fresh/stale/failed/corrupt paths, default_cache_path, default_client, FetchOutcome accessors)
- install.rs: 16 (happy path, sha256 mismatch, oversize, traversal/abs sanitizer, replace existing, uninstall+idempotent, validate_plugin_id battery, zip-bomb cap, uppercase sha256, sha256 vector, sanitize curdir/empty, ct_eq)

---

## TICK 2026-05-17 21:13 PT — v1.3.1 finalized + v1.4.0 Slice 1

(History — first tick on Bench.)

---

## TICK 2026-05-17 22:39 PT — v1.4.0 Slice 8 (drawer + install modal + update badge) in one tick

**MODE C develop v1.4.0 "Bench":** (on `feature/v1.4.0-bench`)

1. `7087c5d` — feat(marketplace): plugin detail drawer + install modal + update badges (Slice 8)
   - New `PluginDetailDrawer.svelte` — right-side 420px slide-in with full description, identity, install state, size, compat, SHA-256, signature, download URL, contextual action button. ESC+backdrop close.
   - New `InstallProgressModal.svelte` — centered modal with phased status (verifying → downloading → extracting → done|error), CSS-only indeterminate progress bar (respects prefers-reduced-motion), success auto-dismiss (1.8s), errors stick with detail + Close.
   - Cross-tab update badge: `availableUpdates` $derived map; amber pill on Installed-tab rows when marketplace has a newer version; one-click jump to Browse + drawer at matching entry.
   - `onMount` opportunistically refreshes the marketplace index when any plugins are installed, so badges populate without requiring Browse-tab visit.
   - Wiring: cards become role=button + tabindex=0 + Enter/Space handlers; .market-card-actions stops click propagation to prevent double-fire; drawer & modal mount outside <section> so backdrops cover whole panel.
   - i18n: 16 new keys × 4 locales (en, es, fr, ar) under `plugins.detail.*`, `plugins.install.*`, `plugins.installed.*`.
   - Plan: `docs/plans/2026-05-17-v1.4.0-bench-slice-8.md`.

**Pushed `7087c5d` to origin/feature/v1.4.0-bench. All gates green.** 581 tests still pass (Slice 8 is 100% frontend).

---

## TICK 2026-05-17 22:21 PT — v1.4.0 Slices 5 + 6 + 7 in one tick (3 commits)

**MODE C develop v1.4.0 "Bench":** (on `feature/v1.4.0-bench`)

1. `80f08e5` — feat(marketplace): Tauri commands — fetch/install/uninstall (Slice 5)
   - 3 commands in `src-tauri/src/lib.rs`: `slab_marketplace_index`, `slab_marketplace_install`, `slab_marketplace_uninstall`.
   - `MarketplaceFetchResult` flat struct (is_fresh/is_stale/index/error) — never throws, errors flow through `error` field.
   - Install verifies signature pre-network (defence in depth), then atomic install, then triggers `PluginRegistry::discover` for auto-refresh.
   - Uninstall is sync + idempotent (returns false if dir absent).

2. `9f104fc` — feat(marketplace): TypeScript store + types (Slice 6)
   - New 253-line `src/lib/marketplace.ts`: types, `marketplaceStore` writable, `refreshMarketplace()`, `installPlugin(entry)`, `uninstallPluginById(id)`, `compareSemver` (semver + pre-release aware), `formatBytes`.
   - Browser-mode short-circuits everywhere via `isInTauri()` so `pnpm dev` outside Tauri shell still works.
   - Per-id busy flags in store so each card spins independently.

3. `c826248` — feat(marketplace): Browse tab + 4-locale i18n (Slice 7)
   - PluginsPanel.svelte: tab strip (Installed | Browse) using existing `.seg` aesthetic.
   - Browse states: loading, error+retry, empty, stale-banner, responsive grid (1→2 cols).
   - Per-card statuses: install / installed (accent border) / update (amber glow), driven by `compareSemver`.
   - i18n: 23 new keys × 4 locales (en, es, fr, ar).
   - +170 lines of scoped styles, line-clamp polyfilled.
   - Plan: `docs/plans/2026-05-17-v1.4.0-bench-slices-5-7.md`.

**Pushed `c826248` to origin/feature/v1.4.0-bench. All gates green.**

---

## TICK 2026-05-17 21:43 PT — v1.4.0 Slices 2 + 3 + 4 in one tick (5 commits)

**MODE C develop v1.4.0 "Bench":** (on `feature/v1.4.0-bench`)

1. `c14fd44` — feat(marketplace): bake real Ed25519 maintainer public key into verifier
   - Replaced all-zero placeholder with real key (hex `17f38d92db3af964…7b27`).
   - Private key generated locally + saved to `~/.slab-maintainer-key` (chmod 600, NOT committed).
   - Regression test pins a known-good fixture signature so any future key drift breaks loudly.

2. `ad0c23c` — feat(marketplace): slab-sign-plugin CLI for maintainer signing (Slice 2)
   - New `[[bin]]` in `src-tauri/Cargo.toml` → `cargo run --bin slab-sign-plugin`.
   - Reads private key file (base64 32-byte seed, `#` comments stripped), computes tarball sha256, signs canonical IndexEntryUnsigned, prints pretty JSON.
   - `--print-public-key` / `--print-fixture-signature` utility modes.
   - 14 unit tests + verified end-to-end against a real test tarball.

3. `337c0d5` — feat(marketplace): HTTP fetch + offline cache (Slice 3)
   - `fetch_index` (pure HTTP+parse), `fetch_index_with_cache` (network-first, stale fallback).
   - Envelope validation: schema_version ≤ CURRENT, signing_key_id == MAINTAINER_KEY_ID.
   - `default_client` mirrors Ollama config (3s connect, 30s total).
   - 14 unit tests via mockito + tempfile — fully offline.

4. `017bc1a` — feat(marketplace): atomic install pipeline (Slice 4)
   - `install_from_bytes` / `install_from_entry` / `uninstall_plugin`.
   - Staging-then-rename atomicity; replace existing via `.trash/<id>-<ts>`.
   - Hardening: plugin-id validation, path sanitizer (no `..`/abs/empty), symlink target containment, type allowlist, MAX_TARBALL_BYTES (5 MiB) + MAX_UNCOMPRESSED_BYTES (50 MiB) caps.
   - New deps: `flate2 = "1"` (rust_backend), `tar = "0.4"`.
   - 16 unit tests including a real zip-bomb gzip → uncompressed-cap defense.

5. `2afc228` — chore(marketplace): polish — rustfmt, clippy clean, README test count 539→581.

**Pushed `2afc228` to origin/feature/v1.4.0-bench. All gates green.**

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
### v1.3.1 "Foundry Patch" 🩹 — RELEASED 2026-05-17 ✓
### v1.4.0 "Bench" 🪑 — **IN PROGRESS** (Slices 1–8/11 shipped & pushed; backend + Tauri cmds + TS store + Browse UI + drawer + install modal + update badges done; uninstall polish + docs + release next)

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — MODE C continue v1.4.0 Bench (Slice 9 → 11: uninstall polish + docs + release)

Slices 1–8 are done. Remaining work:

1. **Slice 9 — Uninstall flow polish** (smaller — good single-tick or combined-with-10 target):
   - Confirmation dialog ("Uninstall <name>? This removes its plugin files.").
   - Move-to-trash semantics already exist in backend; surface a "Trashed (restore?)" toast for 5 seconds.
   - Auto-deselect if active plugin (active AI provider, current theme, etc.) is uninstalled — refresh selectors gracefully.

2. **Slice 10 — Docs + seed `slab-plugins` repo**:
   - Public repo with 3 example plugins signed by maintainer key.
   - Generate `index.json` via `slab-sign-plugin` + commit to repo.
   - Host via GitHub Pages or raw.githubusercontent.
   - Add `MARKETPLACE.md` to slab repo documenting authorship/signing/publishing.

3. **Slice 11 — Release ceremony**:
   - Bump `package.json` + `Cargo.toml` + `tauri.conf.json` to 1.4.0.
   - Release notes from Slice 1–8 tick history.
   - Merge feature/v1.4.0-bench → main (--no-ff).
   - Tag v1.4.0, push, wait for CI, GH release with artifacts.

Suggested batching:
- **Next tick:** Slice 9 + 10 together (uninstall polish + docs + maybe seed repo). Both are smaller pieces.
- **Tick after:** Slice 11 ship ceremony.

---

## (Previous playbook — Slice 8 — superseded above; kept for history)

(Slice 8 completed in TICK 2026-05-17 22:39 PT — see history section above.)

---

## v1.4.0 "Bench" Slice plan summary (full spec in `.cron-state/proposals/v1.4.0-bench.md`)

1. ✅ Marketplace index schema + Ed25519 verifier (Tick 1, 12 tests)
2. ✅ Maintainer signing tool (Tick 2, 14 tests + real key bake-in)
3. ✅ `marketplace/fetch.rs` — HTTP + offline cache (Tick 2, 14 tests)
4. ✅ `marketplace/install.rs` — atomic extract with hardening (Tick 2, 16 tests)
5. ✅ Tauri commands `slab_marketplace_*` (Tick 3, `80f08e5`)
6. ✅ Frontend `src/lib/marketplace.ts` store (Tick 3, `9f104fc`)
7. ✅ Frontend Browse tab + plugin cards + 4-locale i18n (Tick 3, `c826248`)
8. ✅ Frontend install modal + update-available badges + plugin detail drawer (Tick 4, `7087c5d`)
9. Uninstall flow polish (NEXT TICK)
10. Docs + seed `slab-plugins` repo with 3 example plugins
11. Release — version bump 1.3.1 → 1.4.0 + notes + merge + tag + push

---

## POST-v1.4 ROADMAP REMINDERS

- v1.5.0 "TypeScript Plugins" — V8/QuickJS sandbox for `script.js` contribs (bigger lift; security-heavy)
- v1.5.x — AI provider hook-up of plugin-contributed providers through Beacon's runtime
- v1.5.x — Slab CLI `slab plugin install <url>` command
- Beacon Bonus Slices (`.cron-state/proposals/v0.10.0-beacon-bonus-slices.md`) — Smart Outline, Citations, Study Mode, Glossary, Voice Mode
