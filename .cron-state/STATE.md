# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 🛠 v1.3.0 "Foundry" — Slices 10+11 COMPLETE, Slice 12 (release) NEXT TICK (11/12 done)

**Main HEAD**: `bdcba0f` — `docs(README): bring up to v1.2.0 "Glass II"`
**v1.2.0 release**: https://github.com/Sanjays2402/slab/releases/tag/v1.2.0 — all 6 assets uploaded ✓
**Active branch**: `feature/v1.3.0-foundry` (29 commits ahead of main)
**Branch HEAD**: `0b505b7` — `docs(README): mention plugin system + link to PLUGINS.md`
**Slice 10+11 plan**: `docs/plans/2026-05-17-v1.3.0-foundry-slices-10-11.md`

**Quality gates green on branch HEAD:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` ✓ (**539 passed** — +1 over previous, new `example_hello_slab_manifest_parses`)
- `pnpm check` ✓ (0 errors / 23 warnings — baseline preserved)

**NO RELEASE_PENDING** — Foundry needs Slice 12 (release prep) before merge.

---

## TICK 2026-05-17 20:05 PT — Foundry Slices 10+11 MEGA-TICK (8 commits)

Two slices in one tick. Foundry is now **feature-complete** —
Slice 12 (version bump + release notes + merge + tag + push) lands
cleanly next tick.

### MODE C — v1.3.0 Foundry sprint (control surface + docs)

**Slice 10 — Settings → Plugins control surface**

Task 1 (commit `e651c62`) — 22 new i18n keys × 4 locales (en/es/fr/ar)
covering feature label, title, subtitle, empty state, contribution
count labels, enable/disable toggle text, error chip, expand/collapse,
toolbar buttons, palette command label. Appended-only diff for
reviewability.

Task 2 (commit `bb2d6e8`) — `src/lib/panels/PluginsPanel.svelte` (506
lines). Every discovered plugin renders as a row with:
- name + version + author + plugin id + description from manifest
- segmented enable/disable toggle (calls `setPluginEnabled`,
  optimistically refreshed via the `pluginsStore` subscription)
- red "Manifest error" chip + collapsible `<details>` with raw parse
  error + the plugin's on-disk dir when the manifest is malformed
- contribution count chips ("3 themes · 1 locale · 2 commands")
- per-plugin expandable drilldown listing every theme/locale/
  command/ai-provider/pdf-action by ID + label (debugging aid)
Toolbar: "📁 Open plugins directory" (revealItemInDir) + "↻ Reload"
(`reloadPlugins` + toast). Empty state: dashed-border card with the
absolute plugins-dir path + open-dir CTA. Footer-row: dir path when
list is non-empty.

Task 3 (commit `d6b37a2`) — Registered in `+page.svelte` features
array (`{ id: "plugins", label: "Plugins", icon: "🧩", ready: true }`,
slotted next to Settings), wired the conditional render, added
`"plugins"` to `DETACHABLE_PANELS`.

Task 4 (commit `2f19ab2`) — Palette entry "Open Settings → Plugins"
in the Settings group with wide keyword coverage (plugins, extensions,
themes, locales, commands, ai, install, enable, etc.).

**Slice 11 — example plugin + author docs**

Task 1 (commit `16e94db`) — `examples/plugins/hello-slab/` (4 files):
- `plugin.toml` — manifest exercising **all five contribution kinds**:
  Midnight theme, partial Japanese locale, URL command (open github),
  shell command (echo hello), Ollama AI provider (openai_compat),
  qpdf-linearize PDF action with `{in}`/`{out}` placeholders
- `themes/midnight.css` — deep-blue dark theme CSS variable overrides
- `locales/jp.json` — 40-key Japanese bundle covering most-visible UI
- `README.md` — install + contribution table + try-it-out walkthrough
- Plus a new unit test `example_hello_slab_manifest_parses` that loads
  the shipped manifest via `Manifest::from_toml` and asserts every
  contribution count. **If we ever break the manifest schema, this
  test fires and forces an example update — keeping docs honest.**

Task 2 (commit `17bdc9c`) — `docs/PLUGINS.md` (253 lines) — the
canonical author guide. Sections: TL;DR, directory layout, full
manifest reference, every contribution kind with worked TOML
examples, permissions semantics (declarative not sandboxed —
honest framing), validation cheat-sheet, enabled-state persistence,
security model, distribution, troubleshooting table, schema
stability policy. Linked from PluginsPanel's empty state and from
hello-slab's README.

Task 3 (commit `0b505b7`) — Added "## Extending Slab (plugins)"
section to README.md between Tests and Under the hood, pointing
to PLUGINS.md + the hello-slab example.

### Plan doc
`docs/plans/2026-05-17-v1.3.0-foundry-slices-10-11.md` (written + fully
executed this tick — `fb87bbc`).

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
### v1.3.0 "Foundry" 🛠 — IN PROGRESS (**11/12** slices done — FEATURE-COMPLETE, Slice 12 = release)

### v1.3.0 Slice ledger
- ✅ Slice 1 — manifest schema + parser + validation
- ✅ Slice 2 — plugin registry + discovery loop
- ✅ Slice 3 — Tauri commands (list/enable/disable/reload)
- ✅ Slice 4 — theme contribution + asset reader
- ✅ Slice 5 — locale contribution + bundle loader
- ✅ Slice 6 — pdf_action contribution + CLI runner
- ✅ Slice 7 — command contribution + shell/url runner
- ✅ Slice 8 — ai_provider contribution + materialiser
- ✅ Slice 9 — frontend wiring (8 commits previous tick)
- ✅ Slice 10 — Settings → Plugins panel UI (this tick — 4 commits)
- ✅ Slice 11 — example plugin + PLUGINS.md (this tick — 3 commits)
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

## NEXT TICK PLAYBOOK — Slice 12 = ship v1.3.0 Foundry

This is mechanical. Order matters.

1. **Bump version everywhere it appears:**
   - `package.json`: `"version": "1.3.0"`
   - `src-tauri/Cargo.toml`: `version = "1.3.0"` (currently `1.2.0`)
   - `src-tauri/tauri.conf.json`: `"version": "1.3.0"`
   - Verify with `grep -rE '\"version\": ?\"1\.' package.json src-tauri/tauri.conf.json && grep '^version' src-tauri/Cargo.toml`.
   - Run `cd src-tauri && cargo check` so `Cargo.lock` updates.

2. **Write release notes** at `docs/release-notes/v1.3.0.md` covering:
   - What Foundry is (declarative plugin system — themes, locales,
     commands, AI providers, PDF actions via TOML manifest).
   - Headline UX: Settings → Plugins panel, Cmd-K integration,
     Reader toolbar dropdown, hello-slab example.
   - Backend: 5 contribution kinds, 13 Tauri commands, 539 tests.
   - Permissions / security framing (declarative, not sandboxed).
   - Pointer to docs/PLUGINS.md.
   - "Bonus changes" if any (none expected).

3. **Update README** if it pins a version anywhere.

4. **Commit** version bump + release notes:
   ```
   chore(release): v1.3.0 "Foundry" — declarative plugin system
   ```

5. **Quality gates** on feature branch one last time
   (fmt/clippy/test --lib/pnpm check).

6. **STATUS: DONE marker** — write it into STATE.md so the *next-next*
   tick (which will be MODE A) merges + tags. OR (preferred) flip
   into MODE A inline this tick after the version bump:
   - `git checkout main && git pull`
   - `git merge --no-ff feature/v1.3.0-foundry -m "Merge v1.3.0 'Foundry' — declarative plugin system"`
   - Run quality gates on main.
   - `git tag v1.3.0`
   - `git push origin main --follow-tags` (with the auth dance).
   - Record `RELEASE_PENDING: v1.3.0 — merge SHA <hash>, tag v1.3.0, CI run <id>` in STATE.md.
   - Find the CI run with `gh run list --branch main --limit 3`.
   - Next tick lands in MODE B to download + create the GH release.

Slice 12 should land in ONE tick. Don't fragment it.

---

## POST-v1.3 ROADMAP REMINDERS

After Foundry ships, the proposals on disk for the next versions:
- `.cron-state/proposals/v0.10.0-beacon-bonus-slices.md` — Slices 11-15
  (Smart Outline, Citations, Study Mode, Glossary, Voice Mode)
- Plugin marketplace UI + signed-manifest install flow (post-v1.3 idea)
- AI provider hook-up of plugin-contributed providers through Beacon's
  runtime (planned v1.3.x patch)
