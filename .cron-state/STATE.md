# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🛠 v1.3.0 "Foundry" — Slice 9 COMPLETE (frontend wired end-to-end, 9/12 slices done)

**Main HEAD**: `bdcba0f` — `docs(README): bring up to v1.2.0 "Glass II"`
**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓
**Active branch**: `feature/v1.3.0-foundry` (21 commits ahead of main)
**Branch HEAD**: `7ee6ba4` — `feat(plugins-frontend): Reader toolbar dropdown for plugin PDF actions`
**Slice 9 plan**: `docs/plans/2026-05-17-v1.3.0-foundry-slice-9.md` — all 8 tasks shipped this tick

**Quality gates green on branch HEAD:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (538 passed — unchanged from previous tick, no Rust touched)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

**NO RELEASE_PENDING** — Foundry has 3 more slices before merge.

---

## TICK 2026-05-17 19:22 PT — Foundry Slice 9 MEGA-TICK: 8 commits, ~600 LOC TS/Svelte

Frontend now consumes the entire v1.3.0 plugin backend end-to-end.
Every contribution kind shipped in Slices 1-8 has at least one
visible UI surface — themes & commands & AI providers in the palette,
locales merged into i18n at boot, PDF actions in the Reader toolbar.

### MODE C — v1.3.0 Foundry sprint (frontend pass)

**Task 1 — `src/lib/plugins.ts`** (commit `352054a`)
- 254-line TypeScript adapter: type mirrors of every Serde shape,
  Svelte writable `pluginsStore`, `refreshPlugins()` /
  `setPluginEnabled()` / `reloadPlugins()` / `pluginsDir()` /
  `readPluginAsset()` / `loadPluginLocaleBundle()` /
  `validatePluginAiProvider()` / `runPluginCommand()` /
  `runPluginPdfAction()`, plus `currentPlugins()` sync accessor
  and `logActiveAiProviders()` discoverability helper.

**Task 2 — boot in root layout** (commit `60c143e`)
- `+layout.svelte` calls `void refreshPlugins().then(() =>
  logActiveAiProviders())` inside `onMount`, after `bootKeymap()`.

**Task 3 — i18n merge** (commit `07698ff`)
- New `mergePluginBundle(id, bundle, pluginId)` mutates BUNDLES in
  place + re-emits the `locale` store to repaint `$tStore`-bound UI.
- `bootI18n()` now subscribes to `pluginsStore` (lazy bundle fetch
  with a `lastSeen` Set short-circuit) AND to `locale` (re-merge on
  language switch).

**Task 4 — `BUILT_IN_THEMES` extracted** (commit `cdb384d`)
- Moved the three hard-coded built-in themes from CommandPalette's
  inline array to a typed `BUILT_IN_THEMES` export in `theme.ts`.
- Zero behaviour change; just prep work so plugin themes can append
  to the same loop.

**Task 5 — plugin themes + CSS injection** (commit `b742e92`)
- New `$lib/pluginThemes` owns a runtime `<style id="slab-plugin-theme">`
  tag (singleton). `applyPluginTheme()` reads CSS via
  `slab_plugins_read_asset` and swaps it in via `textContent` (NOT
  innerHTML — defence in depth).
- `setUiConfig()` calls `clearPluginTheme()` whenever the user picks
  a built-in theme, so "back to default" actually goes back.
- CommandPalette renders one entry per active plugin theme under the
  Appearance group.

**Task 6 — plugin commands in palette** (commit `8d7e892`)
- Each `ActiveCommand` becomes a palette entry under "Plugin
  commands" group, alphabetised by label.
- `dispatchPluginCommand()` handles both outcome kinds: URL outcomes
  open via `@tauri-apps/plugin-opener`'s `openUrl`, shell outcomes
  show status-keyed toasts (success / warning / error / timeout)
  with truncated stdout/stderr.

**Task 7 — plugin AI providers** (commit `08a4f68`)
- Informational surface only (full hook-up = v1.3.x): each active
  AI provider appears in "Plugin AI providers" palette group; running
  copies the base_url to clipboard with an info toast.
- `logActiveAiProviders()` (defined in plugins.ts since Task 1) is
  now actually called from layout boot for console discoverability.

**Task 8 — Reader toolbar dropdown** (commit `7ee6ba4`)
- "✦ Plugin" button next to Find/Info — hidden entirely when zero
  active PDF actions (no empty dropdown).
- Click opens menu listing each action by label + plugin id; click
  prompts for output path via Tauri save dialog, runs
  `slab_plugins_run_pdf_action`, and toasts the ActionReport status.
- Click-outside captured handler dismisses the menu.
- Styles reuse theme vars (--bg-2/--bg-3/--border/--text) so it
  follows light/dark/accent.

### Plan doc
`docs/plans/2026-05-17-v1.3.0-foundry-slice-9.md` (written previous tick,
fully executed this tick).

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
### v1.3.0 "Foundry" 🛠 — IN PROGRESS (9/12 slices done, all backend + frontend wiring complete ✓)

### v1.3.0 Slice ledger
- ✅ Slice 1 — manifest schema + parser + validation
- ✅ Slice 2 — plugin registry + discovery loop
- ✅ Slice 3 — Tauri commands (list/enable/disable/reload)
- ✅ Slice 4 — theme contribution + asset reader
- ✅ Slice 5 — locale contribution + bundle loader
- ✅ Slice 6 — pdf_action contribution + CLI runner
- ✅ Slice 7 — command contribution + shell/url runner
- ✅ Slice 8 — ai_provider contribution + materialiser
- ✅ Slice 9 — frontend wiring (this tick — 8 commits)
- ⏳ Slice 10 — Settings → Plugins panel UI (enable/disable, list
  contributions, show errors, "open plugins dir" button)
- ⏳ Slice 11 — example plugin demonstrating all five kinds +
  PLUGINS.md
- ⏳ Slice 12 — version bump + release notes + merge + tag + push

**Note**: After Slice 10 + 11, Foundry will be feature-complete and
ready to release.

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
3. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, download + create GH release)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## NEXT TICK PLAYBOOK — Slice 10 Settings → Plugins panel

Goal: dedicated control surface for plugins. Currently they're
discoverable via the palette but you can't see/enable/disable them
without dropping files into `~/.slab/plugins` and reloading.

1. **`src/lib/panels/PluginsPanel.svelte`** — new Settings sub-panel
   (or add as a tab to existing SettingsPanel.svelte; check existing
   layout first). Lists every `Plugin` from `pluginsStore.plugins`
   with:
   - name + version + author + description (from manifest)
   - enabled toggle (calls `setPluginEnabled`)
   - error chip when `plugin.error != null` (red, expandable to show
     the parse/validation message)
   - contribution counts ("3 themes · 1 locale · 2 commands")

2. **"Open plugins directory" button** — `tauri_plugin_opener`'s
   `revealItemInDir(path)` so users can drop plugins in without
   knowing the path. Wrap with `pluginsDir()` to get the actual
   directory.

3. **"Reload plugins" button** — calls `reloadPlugins()` and shows
   an info toast with the new count. Also calls `refreshPlugins()`
   under the hood so all UI surfaces refresh.

4. **Per-plugin contribution drilldown** — clicking a row expands to
   show all themes/locales/etc the plugin contributes. Useful for
   debugging "why isn't my theme showing up".

5. **Empty state** — if 0 plugins, big card explaining what plugins
   are + the plugins dir path + a link to PLUGINS.md (which lands in
   Slice 11).

6. **Register the panel** in `+page.svelte`'s `PANELS` list with a
   sensible icon (✦? or 🧩) and `ready: true`.

7. **Add a palette entry** — "Open Settings → Plugins" so the
   panel is keyboard-reachable.

8. **Tests** — same shape as Slice 9: pure helpers if any, skip e2e.
   Visual smoke check via `pnpm dev` if possible (not in cron).

9. Quality gates as usual; commit per touchpoint; push branch.

Slice 10 estimate: 3-5 commits in TS/Svelte. Then Slice 11 (example
plugin + PLUGINS.md) + Slice 12 (release). Foundry should ship in
the next 2-3 ticks.
