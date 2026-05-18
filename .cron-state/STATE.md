# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🛠 v1.3.0 "Foundry" — Slice 9 plan written (planning tick, no code)

**Main HEAD**: `bdcba0f` — `docs(README): bring up to v1.2.0 "Glass II"`
**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓
**Active branch**: `feature/v1.3.0-foundry` (13 commits ahead of main)
**Branch HEAD**: see latest `docs(plans): …slice-9` commit
**Slice 9 plan**: `docs/plans/2026-05-17-v1.3.0-foundry-slice-9.md` (8 tasks, ~600 LOC TS/Svelte expected)

**Quality gates green on branch HEAD:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (538 passed — +22 new tests this tick: 10 locale + 9 command runner + 8 header injection + 5 materialiser, minus dedup)
- `pnpm check` ✓ (0 errors / 23 warnings)

**NO RELEASE_PENDING** — Foundry has 4 more slices before merge.

---

## TICK 2026-05-17 18:33 PT — Foundry MEGA-TICK: Slices 5+7+8 (3 commits, 22 new tests, ~30KB backend)

Backend is now COMPLETE for the plugin system. All five contribution kinds
(themes, locales, pdf_actions, commands, ai_providers) have Tauri commands,
runners/loaders, and full test coverage. Only Svelte UI work remains.

### MODE C — v1.3.0 Foundry sprint

**Slice 5 — Locale bundle loader** (commit `d23cd27`)
- `plugins/locale_loader.rs` with `load_locale_bundle(plugin_dir, bundle_path)`
- Validates flat JSON `Record<string,string>` shape: rejects arrays,
  nested objects, numbers, non-strings; passes empty objects
- Inherits `read_asset` path-traversal guard (canonicalize + starts_with)
- Tauri cmd `slab_plugins_load_locale_bundle(plugin_id, locale)` returns
  `HashMap<String,String>` ready for the frontend i18n merge
- 10 tests covering all the rejection paths + happy path + traversal

**Slice 7 — Command runner** (commit `e9fe2c0`)
- `plugins/command_runner.rs` with `run_command(active_command)`
- Two outcome variants: `Shell(ShellReport)` (status, stdout, stderr,
  duration) and `Url { url }` (frontend dispatches via opener plugin)
- `/bin/sh -c` on Unix, `cmd /C` on Windows
- 30s wall-clock timeout via try_wait polling (SIGKILL before reading
  pipes — avoids blocking on still-alive child)
- `CommandStatus`: Ok, NonZeroExit, Timeout, SpawnFailed
- Tauri cmd `slab_plugins_run_command(plugin_id, command_id)`
- 9 tests (shell echo, stderr capture, non-zero exit, spawn fail,
  timeout, url variant, validation of shell/url exclusivity)

**Slice 8 — Plugin-contributed AI providers** (commit `3247d75`)
- `OpenAiCompatibleProvider::with_headers(HashMap<String,String>)` builder
- `resolve_header_value()` — bare values pass through, `$VAR_NAME` reads
  from env at request time (errors on missing/empty)
- `apply_extra_headers()` helper called by both `chat()` and `embed()`
- `chat()`/`embed()` now skip `bearer_auth` when api_key is empty
  (plugin providers default to header-only auth)
- New module `plugins/ai_materialize.rs` with
  `materialize_active(active)` and `materialize_contribution(c)`
- Tauri cmd `slab_plugins_validate_ai_provider(plugin_id, provider_id)`
  — Settings UI uses this to mark misconfigured providers
- 13 new tests: header injection happy path (chat + embed), env var
  expansion at request time, missing/empty env var errors, materialiser
  happy path, unknown kind rejection, end-to-end mocked HTTP

### Plan doc
`docs/plans/2026-05-17-v1.3.0-foundry-slices-5-7-8.md` (committed in `d23cd27`)

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
### v1.3.0 "Foundry" 🛠 — IN PROGRESS (8/12 slices done, all backend complete ✓)

### v1.3.0 Slice ledger
- ✅ Slice 1 — manifest schema + parser + validation
- ✅ Slice 2 — plugin registry + discovery loop
- ✅ Slice 3 — Tauri commands (list/enable/disable/reload)
- ✅ Slice 4 — theme contribution + asset reader
- ✅ Slice 5 — locale contribution + bundle loader (this tick)
- ✅ Slice 6 — pdf_action contribution + CLI runner
- ✅ Slice 7 — command contribution + shell/url runner (this tick)
- ✅ Slice 8 — ai_provider contribution + materialiser (this tick)
- ⏳ Slice 9 — frontend wiring (theme picker reads active_themes,
  i18n merges plugin bundles, palette inserts active_commands,
  Settings AI tab reads active_ai_providers)
- ⏳ Slice 10 — Settings → Plugins panel UI (enable/disable, list
  contributions, show errors, "open plugins dir" button)
- ⏳ Slice 11 — example plugin demonstrating all five kinds + PLUGINS.md
- ⏳ Slice 12 — version bump + release notes + merge + tag + push

**Note**: All backend is complete. Slices 9-12 are pure frontend/docs.

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — Slice 9 frontend wiring

Goal: wire the plugin backend into the running UI so plugins actually
DO something visible (no Settings panel yet — that's Slice 10).

1. **Create `src/lib/plugins.ts`** — thin TypeScript wrapper over the
   Tauri commands. Type definitions matching the Serde shapes:
   `Plugin`, `ActiveTheme`, `ActiveLocale`, `ActiveCommand`,
   `ActiveAiProvider`, `ActivePdfAction`, `CommandOutcome`.
   Exported `pluginsStore` (Svelte writable) populated on boot.

2. **i18n merge** — extend `src/lib/i18n.ts`:
   - On boot, after `bootI18n()`, call `slab_plugins_active_locales`
     and for each one that matches the current locale call
     `slab_plugins_load_locale_bundle(plugin_id, locale)`.
   - Merge into the in-memory `BUNDLES[locale]` map (plugin overrides
     win over built-in, plugin loaded later wins over earlier — but
     warn in console on collision).
   - Re-run on locale change.

3. **Theme picker integration** — wherever `ThemeContribution`s are
   meant to show up. Read CSS via `slab_plugins_read_asset` and inject
   into a `<style>` tag whose id is `plugin-theme-${plugin_id}-${id}`.
   Activation removes other plugin themes, restores built-in on
   "default". Built-in themes still ship in `src/lib/themes.ts`.

4. **Command palette merge** — append `active_commands` entries to the
   palette source so `Cmd-K` shows plugin commands alphabetically.
   On selection, call `slab_plugins_run_command`; on `CommandOutcome::Url`,
   open via `tauri_plugin_opener`; on `CommandOutcome::Shell`, show a
   toast with exit status + (truncated) stdout.

5. **AI provider list in Settings → Beacon** — when v0.10.0 Beacon's
   "provider" dropdown is open, prepend "from plugins" group sourced
   from `slab_plugins_active_ai_providers`. Selecting one would call
   `slab_plugins_validate_ai_provider` first and surface any error.
   (Actual hook-up of materialised provider through `make_provider` is
   a v1.3.x follow-up — Slice 9 is just UI surface.)

6. **PDF action menu** — add a "Plugin actions" submenu in the PDF view's
   context menu, populated from `slab_plugins_active_pdf_actions`. On
   click, call `slab_plugins_run_pdf_action` with a chosen output path.

7. Tests for `plugins.ts` are challenging without a Tauri runtime —
   ship unit tests for any pure helpers (e.g. theme-CSS injection),
   skip e2e for now.

8. Quality gates as usual; commit per touchpoint (i18n / themes /
   palette / AI / pdf-actions); push branch.

Slice 9 estimate: 5-6 commits, similar scope to this tick's backend
mega-tick but in TypeScript/Svelte. Then Slices 10-12 are smaller and
should fit in 1-2 more ticks before merge.
