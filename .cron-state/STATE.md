# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🚀 v1.2.0 "Glass II" 🪟² RELEASED ✓ — v1.3.0 "Foundry" 🛠 Slice 1 shipped on branch

**Main HEAD**: `bdcba0f` — `docs(README): bring up to v1.2.0 "Glass II"`
**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓
**Active branch**: `feature/v1.3.0-foundry` (4 commits ahead of main)
**Branch HEAD**: `d722ad7` — `feat(plugins): validate() + from_toml + fixture (Slice 1, Tasks 3-4)`

**Quality gates green on branch HEAD:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (478 passed — 10 new plugin tests)
- `pnpm exec svelte-check` ✓ (0 errors / 23 warnings)

**NO RELEASE_PENDING** — Foundry has 11 more slices before merge.

---

## TICK 2026-05-17 17:32 PT — v1.2.0 release finalized + v1.3.0 Foundry Slice 1 (4 commits)

### MODE B — v1.2.0 release
- CI run `26006878376` (ea2939d) → success
- Downloaded artifacts, curated 6 assets into `assets/v1.2.0/`
- `gh release create v1.2.0` (5 assets) + `gh release upload` for the 79MB AppImage
- Release page live with all 6 assets

### MODE C — v1.3.0 Foundry kickoff (plugin API)
- Wrote proposal `.cron-state/proposals/v1.3.0-foundry.md` (12-slice roadmap)
- Wrote writing-plans-format plan `docs/plans/2026-05-17-v1.3.0-foundry.md` (Slice 1 detail)
- Shipped Slice 1 (manifest schema + parser + validation):
  - `0570e3e` — docs(plans): proposal + Slice 1 plan
  - `43045a4` — feat(plugins): scaffold plugins module (Task 1)
  - `7f26f2b` — feat(plugins): manifest schema with serde Deserialize (Task 2)
  - `d722ad7` — feat(plugins): validate() + from_toml + fixture (Tasks 3-4)
- 10 new unit tests, all green
- Branch pushed to origin

### Foundry architecture decisions (locked in)
- **Declarative TOML manifests** — no arbitrary Rust/JS code in plugins
- **5 contribution kinds**: themes, locales, pdf_actions, commands, ai_providers
- **No sandbox in v1**: trust-by-prompt with declared permissions (fs/net/spawn)
- **Reverse-DNS plugin IDs** (`com.example.foo`)
- **AI providers**: only `kind = "openai_compat"` in v1.3.0 (extensible later)
- **PDF actions**: shell out to external CLI with `{in}` / `{out}` arg substitution, configurable timeout (default 30s)

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
### v1.3.0 "Foundry" 🛠 — IN PROGRESS (Slice 1/12 done)

### v1.3.0 Slice ledger
- ✅ Slice 1 — manifest schema + parser + validation (this tick)
- ⏳ Slice 2 — plugin registry + discovery loop
- ⏳ Slice 3 — Tauri commands (list/enable/disable/reload)
- ⏳ Slice 4 — theme contribution kind
- ⏳ Slice 5 — locale contribution kind
- ⏳ Slice 6 — pdf_action contribution kind (CLI runner)
- ⏳ Slice 7 — command contribution kind (palette + keymap)
- ⏳ Slice 8 — ai_provider contribution kind
- ⏳ Slice 9 — plugin panel UI
- ⏳ Slice 10 — permission prompt + grant ledger
- ⏳ Slice 11 — example-plugins repo + PLUGINS.md
- ⏳ Slice 12 — version bump + release notes + merge + tag + push

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## PRIOR TICK STATE (kept for reference)

## STATUS-PRIOR: v1.2.0 Glass II Slices 5-RTL + 7 + MERGE + TAG + PUSH

**Quality gates green on `main` HEAD before tag push:**
- 4 gates passing (468 tests)

**Tag**: `v1.2.0` (pushed) — CI run `26006878376` → success this tick.
