# Slab Cron State

Last updated: 2026-06-18 00:45 PT by Cake — v3.39.0 "Atlas Tag-Suggest" committed + pushed to feature branch (NOT merged; Sanjay reviews in AM).

## Active branch & version

**Branch: `feature/v3.39.0-atlas-tag-suggest`** (this is THE active feature
branch — keep shipping onto it unless Sanjay says otherwise).
**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `f997a33` — "feat(library): v3.39.0 Atlas Tag-Suggest".
Verified on origin (git rev-parse HEAD == origin/feature/v3.39.0-atlas-tag-suggest).

### What v3.39.0 already shipped (DONE — do not redo)
Local, deterministic tag suggestions for library documents:
- `registry.rs`: schema v7→v8 + `library_tag_suggestion_dismissed` table (4 tests)
- `tag_suggest.rs`: suggester core + bulk + accept/dismiss, ranks by vocabulary
  match + tag co-occurrence + domain hints. No network, no model calls. (18 tests)
- `lib.rs`: 5 Tauri commands (suggest / bulk / accept / dismiss / list-dismissed)
- `library.ts`: typed client for the 5 commands
- `SuggestedTagsRow.svelte`: per-doc suggestion chips, accept/dismiss
- `LibraryPanel.svelte`: mounts the row + optimistic accept handler
Gates passed: `cargo test --lib` (tag_suggest 18 + schema 4 green), `pnpm check`
0 errors. NOTE: full Tauri binary build was NOT run (wedges on slow disk; see below).

## BUILD ENVIRONMENT — CRITICAL, read before any cargo command

Internal disk is FULL (~2.9 GiB free of 228). Cargo target is redirected to an
APFS sparse image at **/Volumes/SlabBuild** via `src-tauri/.cargo/config.toml`
(gitignored). Verify mounted each tick: `df -h /Volumes/SlabBuild | tail -1`.
If missing: `hdiutil attach "/Volumes/Sanjay SSD/SlabBuild.sparseimage"`.

**The image has very slow fsync.** Proven tonight across many attempts:
- `cargo test --lib`, `cargo check --lib`, `pnpm check` → WORK (slow but finish).
- A FULL `cargo build` / `cargo tauri build` → WEDGES on the `tauri` crate's
  final codegen (rustc goes to sleep state, no CPU, target size flat for min).
**RULE: never run a full binary build in a tick.** It's release work, blocked by
CI billing anyway. Gate with `cargo test --lib` + `cargo clippy --lib` + `pnpm
check`. If cargo wedges >5 min with no rustc CPU: `pkill -f 'cargo'`, retry once.

## CI STILL BLOCKED — needs Sanjay

GitHub Actions billing failure persists → no release artifacts (DMG/MSI/AppImage)
until fixed. Action: https://github.com/settings/billing → update payment / raise
limit. Does NOT affect local dev or branch pushes.

## Roadmap — next ticks (pick the top undone item each tick)

These extend the tag system the v3.39.0 work introduced. Ship ONE complete
vertical slice per tick (Rust + tests + Tauri command + TS client + Svelte UI).

1. **Untagged filter** — a one-click "Show untagged documents" toggle in the
   library panel (backend: `LibraryFilter` already exists; add an `untagged`
   predicate + Tauri command + a chip in LibraryPanel). High user value, small.
2. **Bulk tag-apply** — multi-select documents in the library and apply/remove a
   tag across all of them in one action. (Backend bulk op + selection UI.)
3. **Tag colors** — let a tag carry a color; store on the tag row (schema bump),
   render colored chips. Visual "wow" candidate.
4. **Tag rename** — rename a tag everywhere it's used (single backend op +
   inline-edit UI on the tag chip). Co-occurrence data updates automatically.
5. **Recently-used tags** — surface the N most recently applied tags as quick
   chips when tagging a new doc.

## House style (match existing code)

- Rust: mirror `tag_suggest.rs` / `folder_suggest.rs`. Tauri commands in `lib.rs`
  via `open_library_db()` + `CmdResult<T>` + `.into()`. Tests use
  `LibraryDb::open_in_memory()`.
- TS: flat in `src/lib/library.ts`, `invoke<CmdResult<T>>(...)` then `unwrap()`,
  camelCase args.
- Svelte 5 runes only (`$props`, `$state`, `onMount`). Dark-first design,
  monochrome glyphs in app chrome, no emoji.

## Tick log

- 2026-06-18 00:45 PT (Cake, interactive): committed + pushed v3.39.0 Atlas
  Tag-Suggest to feature branch (f997a33). Diagnosed slow-disk full-build wedge;
  set gates to lib-only. Seeded roadmap above. Overnight loop armed (30m, →08:00).
