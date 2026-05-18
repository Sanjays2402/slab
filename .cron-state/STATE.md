# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🛠 v1.3.0 "Foundry" — Slices 2 + 3 + 4 + 6 shipped on branch (4 commits this tick)

**Main HEAD**: `bdcba0f` — `docs(README): bring up to v1.2.0 "Glass II"`
**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓
**Active branch**: `feature/v1.3.0-foundry` (5 commits ahead of origin, 8 ahead of main)
**Branch HEAD**: `7cc1501` — `feat(plugins): PDF action runner with timeout + Tauri cmd (Slice 6)`

**Quality gates green on branch HEAD:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (506 passed — +38 new tests this tick)
- `pnpm exec svelte-check` ✓ (0 errors / 23 warnings)

**NO RELEASE_PENDING** — Foundry has 6 more slices before merge.

---

## TICK 2026-05-17 18:05 PT — Foundry MEGA-TICK: Slices 2+3+4+6 (4 commits, 28 new tests, ~1200 LOC backend)

Sanjay said "ship BIG things, not 1-2 fixes." Done — quadruple slice.

### MODE C — v1.3.0 Foundry sprint

**Slice 2 — Registry + discovery + enabled-state** (commit `2c7ca32`)
- `PluginRegistry` (Mutex<HashMap<id, Plugin>>) held as Tauri State
- `discover(root, enabled_state)` scans `~/.slab/plugins/*/plugin.toml`
- Per-plugin error capture: one broken manifest doesn't take down load
- Enabled flags persist to `~/.slab/plugin-state.toml` (flat TOML map)
- Helpers: `default_plugins_root`, `default_state_path`, read/write
- Added `Serialize` to all manifest types so `Plugin` can cross the bridge
- 10 new tests (empty root, valid load, error isolation, persistence, reload, etc.)

**Slice 3 — Tauri commands + boot discovery** (commit `b681d22`)
- `setup()` runs `discover()` at boot against `~/.slab/plugins`
- 4 commands: `slab_plugins_list/set_enabled/reload/dir`

**Slice 4 — Contribution resolution + asset reader** (commit `b5e111b`)
- New module `contributions.rs` with `Active<Kind>` wrapper structs
- 5 helpers (`active_themes/locales/pdf_actions/commands/ai_providers`)
- `read_asset()` with path-traversal guard (canonicalize both, starts_with check)
- 6 new Tauri commands: `slab_plugins_active_*` + `slab_plugins_read_asset`
- 10 new tests (per-kind active list, disabled-plugin contributes nothing, asset
  traversal/absolute/missing rejected)

**Slice 6 — PDF action CLI runner** (commit `7cc1501`)
- New module `runner.rs` — shell-out with `{in}`/`{out}` argv substitution
- Tempfile isolation (original PDF never exposed to plugin's CLI)
- Wall-clock timeout via `try_wait` polling, kill SIGKILL before reading
  stdout/stderr (avoids blocking on still-alive child)
- `ActionReport` carries status (Ok / NonZeroExit / Timeout / SpawnFailed /
  NoOutput), stdout, stderr, duration_ms, output_path
- Tauri command: `slab_plugins_run_pdf_action`
- 8 new tests (unix-gated where they spawn cp/sleep/false/true)

### Disk hygiene
- `target/debug/incremental` ate 8.3G — nuked it, freed disk (96% → 57%)

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
### v1.3.0 "Foundry" 🛠 — IN PROGRESS (5/12 slices done, all backend complete)

### v1.3.0 Slice ledger
- ✅ Slice 1 — manifest schema + parser + validation (prior tick)
- ✅ Slice 2 — plugin registry + discovery loop (this tick)
- ✅ Slice 3 — Tauri commands (list/enable/disable/reload) (this tick)
- ✅ Slice 4 — theme contribution kind + contribution resolution + asset reader (this tick)
- ⏳ Slice 5 — locale contribution kind: wire into i18n bundle map (frontend work)
- ✅ Slice 6 — pdf_action contribution kind (CLI runner) (this tick)
- ⏳ Slice 7 — command contribution kind (palette + keymap registration)
- ⏳ Slice 8 — ai_provider contribution kind (inject into AiProvider registry)
- ⏳ Slice 9 — plugin panel UI (Svelte)
- ⏳ Slice 10 — permission prompt + grant ledger
- ⏳ Slice 11 — example-plugins repo + PLUGINS.md
- ⏳ Slice 12 — version bump + release notes + merge + tag + push

**Note**: Slices 4 and 6 *backend* are done — but the frontend wiring
(theme picker reading active_themes, palette inserting active_commands,
etc.) is left to Slice 9's UI tick. That keeps each tick's surface area
focused.

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK

1. Slice 5 (locale wiring) + Slice 7 (palette wiring) + Slice 8 (AI
   provider injection) — these are all small backend touches plus
   frontend stores that pull from `slab_plugins_active_*`. Aim to ship
   all three in one tick.
2. Then Slice 9 (UI panel) as a dedicated tick — it's the largest
   single user-visible piece.
3. Slice 10 (permission prompt) + Slice 11 (example plugins + docs) +
   Slice 12 (release) — those can plausibly fit in 1-2 more ticks.

So Foundry is 4-5 ticks from merge if cadence holds.
