# Slab Cron State

Last updated: 2026-06-18 06:35 PT by Cake (cron) — fresh roadmap #8 "Empty/unused tag cleanup" shipped (cd4219a backend + ba7a83d UI), pushed + verified on feature branch.

## Active branch & version

**Branch: `feature/v3.39.0-atlas-tag-suggest`** (this is THE active feature
branch — keep shipping onto it unless Sanjay says otherwise).
**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `ba7a83d` — "feat(library): one-click clean up unused tags in the rail head".
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

1. ~~**Untagged filter**~~ — DONE (v3.40.0 slice, 2026-06-18 01:05 PT, a0836e3 +
   95ed028). Added first-class `Untagged`/`Tagged` clauses to the filter
   language (query.rs), fixed the `untagged` preset TODO, and added a one-click
   "Untagged" toggle chip to the LibraryPanel toolbar.
2. ~~**Bulk tag-apply**~~ — DONE (2026-06-18 01:55 PT, d6a46fb backend +
   c4c9848 UI). New `bulk_tag.rs` (apply_tag_to_docs find-or-creates + unions,
   remove_tag_from_docs detaches links only; both transactional, report
   affected/total; 12 tests). `registry::find_tag_by_id` added; `pastel_for`
   promoted to pub(crate). Two Tauri commands (bulk_apply / bulk_remove).
   TS clients + a multi-select grid: "Select" toolbar toggle, per-card
   checkboxes, floating action bar with All/None/Clear + a tag picker
   (apply existing/new, remove existing) and an "N of M" toast. Selection is
   pruned to the visible set each refresh; live multi-selection drags as a set.
   ALSO fixed a stale collections.rs test (schema_version 7→8) that the
   v3.39.0 migration had silently broken — earlier ticks ran scoped tests so
   the full `cargo test --lib` never surfaced it.
3. ~~**Tag colors**~~ — DONE (2026-06-18 02:40 PT, 6a7ff10 backend +
   155fe06 UI). The `color` column already existed, so this shipped the
   EDIT path: `registry::set_tag_color(tag_id, Option<&str>)` updates/clears
   a tag's color + returns the row, guarded by `valid_tag_color()` which only
   persists `#hex` / `hsl()/hsla()/rgb()/rgba()` shapes (functional body
   restricted to digits/dots/%/comma/space — no CSS injection). Unknown id and
   bad color both error without touching the row (11 tests). One Tauri command
   (set_tag_color). TS `setTagColor` client + a tag-rail color-edit affordance:
   a filled-dot button per row opens a "Tag color" modal (live preview swatch +
   the existing palette + a "Default" clear-to-deterministic option); saving
   swaps the updated row into the rail and every doc card in place (no refetch).
4. ~~**Tag rename**~~ — DONE (2026-06-18 03:25 PT, 44444f8 backend +
   161dfbb UI). `registry::rename_tag(tag_id, new_name)` is a single UPDATE
   on library_tags; because library_doc_tags links by tag_id (never name),
   the rename propagates to every doc + live co-occurrence with no migration
   and no orphans. Name is trimmed; same-name is a no-op; a pure case change
   (research->Research) is a valid distinct rename under BINARY collation;
   renaming onto a *different* tag's existing name is REJECTED (UNIQUE name
   col) rather than silently merging — the rejected update leaves both rows
   untouched; empty name and unknown id also error (8 tests). One Tauri
   command (slab_library_rename_tag) returns the updated row. TS `renameTag`
   client + an inline rail edit: a pencil glyph (beside the color dot +
   delete x) swaps the row label for an auto-selected text input; Enter
   commits, Escape/blur cancels, unchanged/empty cancels with no round-trip;
   on success the row swaps into the rail + every doc card in place (no
   refetch); a rejected rename keeps the row in edit mode and shows the
   backend reason inline so the user can fix + retry.
5. ~~**Recently-used tags**~~ — DONE (2026-06-18 04:20 PT, cf62147 backend +
   3fc663a UI). Schema v8->v9: nullable `applied_at` on library_doc_tags +
   `(tag_id, applied_at)` index. `set_doc_tags` rewritten from
   wipe-and-reinsert into a true DIFF so surviving links keep their original
   stamp and only new links are stamped now() — re-saving an unchanged set
   must not restamp (would shuffle a stable tag to the top). `bulk_tag` apply
   stamps too. `registry::recently_used_tags(limit)` returns each used tag
   once by MAX(applied_at) desc, link-rowid tie-break, NULL stamps last,
   never-applied excluded. One Tauri command (slab_library_recently_used_tags,
   limit default 8). TS `recentlyUsedTags` client + a "Recently used"
   quick-chip row at the top of the per-doc tag context menu (lazy-loaded on
   open, re-ranked after each apply/remove, hides tags already on the doc via
   a $derived list). ALSO relaxed two schema-version-pinning tests
   (registry + collections) from `== 8` to `>=` + added a dedicated v9 column
   test, so the next migration won't trip an unrelated equality assert (the
   exact trap that bit the v3.39.0->bulk tick). Gates: cargo fmt clean,
   cargo test --lib pdf::library 206 passed/0 failed (9 new), clippy --lib
   -D warnings clean (9.1s warm), pnpm check 0 errors. Pushed + verified.
6. ~~**Tag merge**~~ — DONE (2026-06-18 04:55 PT, 2083c1f backend +
   e2fe7b7 UI). `registry::merge_tags(source_id, target_id)` folds the
   source tag into the target in one transaction: step 1 lifts the target
   link's applied_at to the NULL-aware max of both stamps for docs carrying
   BOTH tags (max(coalesce(a,b), coalesce(b,a)) so a real timestamp always
   beats a legacy NULL, NULL only when both are), step 2 re-points
   source-only links via UPDATE OR IGNORE (keeping their own stamp), step 3
   deletes leftover source links + the orphaned source tag row. Both ends
   validated up front so a rejected merge (unknown id, or merge-into-self)
   leaves every row untouched; returns the surviving target. One Tauri
   command (slab_library_merge_tags). 12 new tests (source-only re-point,
   both-tag coalesce-to-one-link, newest-stamp each side, real-beats-NULL
   either side, re-pointed stamp carry-over, recently-used order survives,
   self/unknown rejection intact, multi-doc, no-doc). UI: TS mergeTags +
   a merge glyph in the rail row menu (beside rename/color/delete) opening
   a "Merge tag" modal that names the source and lists every other tag as a
   target ($derived candidates exclude source); on success the rail drops
   the source row + swaps the target in place, an active filter on the
   source re-points to the target, doc cards re-point + de-dupe their
   source chip in place (no refetch), recently-used reloads; a rejected
   merge keeps the modal open with the reason inline. Gates: cargo fmt
   clean, cargo test --lib pdf::library:: 218 passed/0 failed (12 new),
   clippy --lib -D warnings clean (6.2s warm), pnpm check 0 errors (no new
   LibraryPanel warnings; the 2 there are pre-existing autofocus + webkit
   CSS). Build cache from the 04:20 tick still warm — test 1.72s.

   This completes the tag-management surface the v3.39.0 work introduced
   (suggest, untagged filter, bulk apply, color, rename, recently-used,
   merge). Next ticks pick from the fresh roadmap below.

## Roadmap — fresh items (tag system is feature-complete; these are new)

7. ~~**Tag usage counts in the rail**~~ — DONE (2026-06-18 05:50 PT, 966db5e).
   `registry::tag_usage_counts() -> Vec<(tag_id, count)>` single LEFT JOIN +
   GROUP BY (one round-trip, never N); every tag appears once, a tag on zero
   docs reports 0 (LEFT JOIN keeps the merge/remove residue an INNER JOIN
   would drop), id-ordered. One Tauri command (slab_library_tag_usage_counts);
   6 tests (per-doc counts, zero-for-unused, one-row-per-tag-id-ordered,
   empty, reflects bulk apply/remove, reflects merge as a distinct union with
   no double-count + gone source unreported). TS `tagUsageCounts()` returns a
   Map<tagId,count>. LibraryPanel loads counts alongside listFolders/listTags
   in refreshAll so the rail count self-heals on every library-changed poke
   (no bespoke optimistic plumbing — same resync path tags/docs already use);
   a muted `rail-meta` count renders beside each tag (mirrors the folder rail)
   and a rail-head A-Z / Most-used sort toggle (count desc, name tie-break for
   a stable order; shown only when >1 tag) makes the count meaningful.
   Gates: cargo fmt clean, cargo test --lib pdf::library:: 224 passed/0 failed
   (6 new), clippy --lib -D warnings clean (6.61s warm), pnpm check 0 errors
   (no new LibraryPanel warnings; still the 2 pre-existing autofocus + webkit).
8. ~~**Empty/unused tag cleanup**~~ — DONE (2026-06-18 06:35 PT, cd4219a
   backend + ba7a83d UI). `registry::delete_unused_tags() -> usize`: a single
   DELETE over library_tags guarded by `NOT EXISTS` against library_doc_tags
   (tag_id is NOT NULL so NOT EXISTS is the clean form), removes every tag on
   zero docs and returns the count; a tag with even one link is untouched, an
   empty library is a no-op returning 0. One Tauri command
   (slab_library_delete_unused_tags) emits library-changed only on a non-empty
   cleanup. 4 tests (removes-only-unused, no-op when all used, empty-is-zero,
   and the motivating bulk-remove-leaves-residue-at-0 reclaim). UI: TS
   deleteUnusedTags + a $derived unusedTagCount off the existing tagCounts map
   (count 0 == unused, self-heals on every refresh, no bespoke plumbing); a
   muted "Clean up N" rail-head affordance shown only when >0, danger-tinted
   hover, disabled while pruning; click confirms with the exact count,
   snapshots doomed ids to prune the active filter, toasts "Removed N", then
   refreshAll reconciles off the backend. Gates: cargo fmt clean, cargo test
   --lib pdf::library:: 228 passed/0 failed (4 new), clippy --lib -D warnings
   clean (6.48s warm), pnpm check 0 errors (no new LibraryPanel warnings).
9. **Tag filter combinator (AND/OR)** — the rail's tag toggles currently
   union (OR). Add an AND/OR switch on the Tags rail head so selecting two
   tags can mean "docs with BOTH" not just "either". Wire through the
   existing query.rs tag clause + the LibraryPanel filter builder. Pure
   filter-language work (query.rs already has the AND/OR ClauseGroup
   machinery the Untagged slice touched) + a small UI toggle. Tests in
   query.rs for the AND vs OR tag-clause expansion.

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
- 2026-06-18 01:05 PT (Cake, cron): roadmap #1 "Untagged filter" shipped.
  Backend a0836e3 (Untagged/Tagged filter clauses + preset TODO fixed, 32 query/
  preset tests green), UI 95ed028 (toolbar toggle chip + TS union + ClauseGroup
  narrowing fix). Gates: cargo fmt clean, clippy --lib -D warnings clean (13s warm),
  cargo test query 22 + presets 10 green, pnpm check 0 errors. Pushed + verified.
  NOTE for next tick: v3.39.0's first `cargo test`/`clippy` were COLD (~12-14 min
  each on the image) because the test/clippy profiles recompiled tauri+mockito
  from scratch; once warm, incremental test+clippy is ~10-20s. Budget the first
  build of a session generously. Also: the interactive session committed v3.39.0
  mid-build under author "Sanjay Santhanam" (its default git identity) — that's
  expected for the interactive session, not a cron mis-attribution.
- 2026-06-18 01:55 PT (Cake, cron): roadmap #2 "Bulk tag-apply" shipped.
  Backend d6a46fb (bulk_tag.rs apply/remove + find_tag_by_id + pastel_for
  pub(crate) + 2 Tauri commands; 12 new tests), UI c4c9848 (TS clients +
  multi-select grid + floating action bar + tag picker). Gates: cargo fmt clean,
  clippy --lib -D warnings clean (10.9s warm), cargo test --lib pdf::library::
  182 passed/0 failed, pnpm check 0 errors. Pushed + verified (local==origin).
  Incidentally fixed a PRE-EXISTING red test: collections.rs asserted
  schema_version==7 but the v3.39.0 migration moved it to 8; prior ticks only ran
  scoped tests (query/presets) so the full --lib suite never caught it. This
  session's first `cargo test` was warm (~24s compile) — build cache from the
  01:05 tick was still fresh, no cold recompile this time.
- 2026-06-18 02:40 PT (Cake, cron): roadmap #3 "Tag colors" shipped.
  Backend 6a7ff10 (registry::set_tag_color + valid_tag_color guard + 1 Tauri
  command; 11 new tests), UI 155fe06 (TS setTagColor + tag-rail color-edit
  affordance: per-row dot button -> "Tag color" modal with preview swatch +
  palette + Default clear, in-place row swap on save). No schema bump — the
  color column already existed; this was the edit path. Gates: cargo fmt clean,
  cargo test --lib pdf::library:: 190 passed/0 failed (8 new tag_color tests
  green), clippy --lib -D warnings clean (7.2s warm), pnpm check 0 errors.
  Pushed + verified (local==origin). Build cache from the 01:55 tick was still
  warm — test compile ~under a sec incremental, clippy 7s.
- 2026-06-18 03:25 PT (Cake, cron): roadmap #4 "Tag rename" shipped.
  Backend 44444f8 (registry::rename_tag — single UPDATE on library_tags, rename
  propagates via tag_id links so docs + co-occurrence follow with no migration;
  trims, same-name no-op, case-only rename valid, UNIQUE-collision rejected
  (no silent merge), empty/unknown error; 8 new tests + 1 Tauri command
  slab_library_rename_tag). UI 161dfbb (TS renameTag + inline rail edit: pencil
  glyph -> auto-selected text input, Enter commits / Escape+blur cancels,
  unchanged/empty short-circuits, in-place row+doc-card swap on success, inline
  error keeps row in edit mode on a rejected rename + focusSelect action).
  Gates: cargo fmt clean, cargo test --lib pdf::library:: 198 passed/0 failed
  (8 new rename_tag tests green), clippy --lib -D warnings clean (7.06s warm),
  pnpm check 0 errors (new input has aria-label, no new a11y warnings). Pushed
  + verified (local==origin 161dfbb). Build cache from the 02:40 tick still
  warm — test compile 1.46s, clippy 7s. No manifest bump (kept 3.39.0, per the
  established convention that v3.4x.0 labels are logical feature versions).
- 2026-06-18 04:20 PT (Cake, cron): roadmap #5 "Recently-used tags" shipped.
  Backend cf62147 (schema v8->v9: nullable applied_at on library_doc_tags +
  (tag_id, applied_at) index; set_doc_tags rewritten wipe-and-reinsert ->
  true diff so surviving links keep their stamp, only new links stamped now();
  bulk_tag apply stamps too; recently_used_tags(limit) ranks each used tag
  once by MAX(applied_at) desc, rowid tie-break, NULL-last, never-applied
  excluded; 1 Tauri command slab_library_recently_used_tags; 9 new tests).
  UI 3fc663a (TS recentlyUsedTags + "Recently used" quick-chip row at top of
  the per-doc tag context menu, lazy-load on open, re-rank after each toggle,
  $derived filter hides already-attached tags; dark-first pill styling).
  Relaxed two schema-version-pinning tests (registry + collections) from
  == 8 to >= + added a v9 column test, pre-empting the equality-assert trap.
  Gates: cargo fmt clean, cargo test --lib pdf::library:: 206 passed/0 failed,
  clippy --lib -D warnings clean (9.1s warm), pnpm check 0 errors (no new
  LibraryPanel warnings; the 2 there are pre-existing autofocus + webkit CSS).
  First cargo test of the session hit a borrow-lifetime slip in the new
  set_doc_tags (query_map temporary outliving stmt at block end) — fixed by
  draining rows with a while-let loop instead. Pushed + verified (local==origin
  3fc663a). Build cache from the 03:25 tick still warm — test 1.72s, clippy 9s.
- 2026-06-18 04:55 PT (Cake, cron): roadmap #6 "Tag merge" shipped — and it's
  the LAST tag-system item; the surface is now feature-complete (suggest /
  untagged filter / bulk apply / color / rename / recently-used / merge).
  Backend 2083c1f (registry::merge_tags — transactional fold: NULL-aware-max
  lift of applied_at for both-tag docs via max(coalesce(a,b),coalesce(b,a)),
  UPDATE OR IGNORE re-point of source-only links keeping their stamp, delete
  leftover source links + orphaned source row; both ends validated up front so
  a rejected merge/self-merge leaves rows untouched; 1 Tauri command
  slab_library_merge_tags; 12 new tests). UI e2fe7b7 (TS mergeTags + a merge
  glyph in the rail row menu opening a "Merge tag" target-picker modal;
  $derived candidates exclude the source; on success rail drops source + swaps
  target in place, active filter re-points, doc cards re-point + de-dupe their
  chip in place no refetch, recently-used reloads; rejected merge keeps modal
  open w/ inline reason; dark-first, monochrome glyph). Gates: cargo fmt clean,
  cargo test --lib pdf::library:: 218 passed/0 failed (12 new merge tests all
  green), clippy --lib -D warnings clean (6.2s warm), pnpm check 0 errors (no
  new LibraryPanel warnings — still the 2 pre-existing autofocus + webkit CSS).
  No schema bump (no new columns; pure re-point + delete over existing tables).
  Build cache from the 04:20 tick still warm — test 1.72s. Pushed + verified
  (local==origin e2fe7b7). Seeded a fresh roadmap (#7 usage counts, #8 unused-
  tag cleanup, #9 AND/OR tag combinator) since the tag roadmap is exhausted.
- 2026-06-18 05:50 PT (Cake, cron): fresh roadmap #7 "Tag usage counts in the
  rail" shipped (966db5e, single commit). Backend registry::tag_usage_counts()
  -> Vec<(tag_id, count)>: one LEFT JOIN + GROUP BY round-trip (never N), every
  tag once, zero-doc tags report 0 (LEFT JOIN keeps the residue an INNER JOIN
  would drop), id-ordered; 1 Tauri command slab_library_tag_usage_counts; 6
  new tests (per-doc counts, zero-for-unused, one-row-per-tag-id-ordered, empty,
  reflects bulk apply/remove, reflects merge as distinct union no-double-count
  + gone source unreported). Frontend: TS tagUsageCounts() -> Map<tagId,count>;
  LibraryPanel loads counts in refreshAll alongside listFolders/listTags so the
  rail count self-heals on every library-changed poke (reused the existing
  resync path, no bespoke optimistic plumbing); muted rail-meta count beside
  each tag (mirrors folder rail) + a rail-head A-Z/Most-used sort toggle (count
  desc, name tie-break, shown only when >1 tag). Gates: cargo fmt clean, cargo
  test --lib pdf::library:: 224 passed/0 failed (6 new), clippy --lib -D
  warnings clean (6.61s warm), pnpm check 0 errors (no new LibraryPanel
  warnings; still the 2 pre-existing autofocus + webkit line-clamp). No schema
  bump (pure read over existing tables). Build cache from the 04:55 tick still
  warm — first session test compile 16s (test profile cold-ish), full suite
  1.72s warm. Pushed + verified (local==origin 966db5e). Next undone: #8
  empty/unused tag cleanup.
- 2026-06-18 06:35 PT (Cake, cron): fresh roadmap #8 "Empty/unused tag
  cleanup" shipped (cd4219a backend + ba7a83d UI, two commits). Backend
  registry::delete_unused_tags() -> usize: one DELETE over library_tags
  guarded by NOT EXISTS against library_doc_tags (tag_id is NOT NULL so
  NOT EXISTS is the clean idiomatic form vs NOT IN), removes every zero-doc
  tag and returns the count; a tag with even one link untouched, empty
  library a no-op returning 0. 1 Tauri command slab_library_delete_unused_tags
  emits library-changed only when removed>0. 4 new tests (removes-only-unused
  keeps in-use drops orphans, no-op when all used, empty-is-zero, and the
  motivating case: a bulk-remove that strips a tag off its last doc leaves it
  in tag_usage_counts at 0 and the cleanup reclaims it). UI: TS deleteUnusedTags
  + a $derived unusedTagCount computed straight off the existing tagCounts map
  (count 0 == unused) so it self-heals on every refreshAll with zero bespoke
  plumbing; a muted "Clean up N" affordance in the Tags rail head (shown only
  when >0, danger-tinted hover marking it destructive, disabled while pruning).
  Click confirms with the exact count, snapshots the doomed ids to prune any
  now-stale tag out of the active filter, calls backend, toasts "Removed N
  unused tags" via the existing bulkSummary channel, then refreshAll reconciles
  rail+counts off the source of truth. Gates: cargo fmt clean, cargo test --lib
  pdf::library:: 228 passed/0 failed (4 new), clippy --lib -D warnings clean
  (6.48s warm), pnpm check 0 errors (105 warnings, all pre-existing in other
  panels — none in LibraryPanel from this change). No schema bump (pure delete
  over existing tables). Build cache from the 05:50 tick still warm — full
  library suite 1.74s, clippy 6.48s. Pushed + verified (local==origin ba7a83d).
  Next undone: #9 tag filter combinator (AND/OR).

