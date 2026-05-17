# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: DEVELOP — `feature/v0.13.0-lens`, Slice 2 of 9 DONE (auto-OCR queue end-to-end)

**v0.13.0 Lens slices progress (2 / 9):**
- ✅ **Slice 1: Scan-audit + Reader banner** — `pdf::scan_audit` classifier (text/image/mixed), `slab_scan_audit` Tauri cmd, Reader `lens.ts` auto-banner on open (commits d4905e2, b53e0e5, dfb2061)
- ✅ **Slice 2: Library Auto-OCR Queue** — schema v2 + scanner derives `ocr_state` from scan_audit; new `pdf::library::ocr_queue` module (synchronous run_one/run_all/list_pending, `<stem>.ocr.<ext>` output naming); 3 Tauri commands `slab_library_ocr_queue_*` + TS bindings; LibraryPanel UI with color-coded badges, per-card Run-OCR / Open-OCR'd buttons, "OCR N pending" toolbar action, context-menu items (commits 8ca2ba0, 220d025, 31bc79a, e72a60c). 302 lib tests / svelte-check 0 errors / clippy `-D warnings` clean.
- ⏳ **Slice 3+: tesseract DPI tuning, table → CSV, math → LaTeX, vision Q&A in Beacon, auto-tag, CMap rewriting, font subsetting** — see `.cron-state/proposals/v0.13.0-lens.md`

**Active branch:** `feature/v0.13.0-lens` (4 new commits this tick: 8ca2ba0 → e72a60c)
**Last shipped tick:** Slice 2 closeout — vertical slice top-to-bottom (registry → scanner → queue → IPC → UI)

**Quality gates green on feature branch:**
- `cargo fmt --all -- --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --lib` — 302 passed (273 → 302, +29 between Slice 1 & 2 additions: schema v2 + ocr_queue module + scanner ocr_state derivation)
- `pnpm check` — 0 errors, 28 (pre-existing) warnings
- `pnpm build` — clean

---

## PRIOR (kept for reference)

**v0.12.0 RELEASE_PENDING** 2026-05-17 — merge SHA `1ca71a2`, tag `v0.12.0`, CI run `25989140987`. Release notes staged at `.cron-state/release-notes/v0.12.0.md`. Next tick (MODE B): poll CI, if green → `gh release create v0.12.0 --notes-file .cron-state/release-notes/v0.12.0.md` and upload 6 artifacts.

**v0.11.0 RELEASED** 2026-05-17 10:21 UTC — all 6 installers on GH Releases, marked latest. Release URL: https://github.com/Sanjays2402/slab/releases/tag/v0.11.0

**Atlas (v0.12.0) slices (2 / 5 done — merged to main):**
- ✅ Slice 1: Library Backend Foundation
- ✅ Slice 3: LibraryPanel UI + sidebar nav + Reader handoff
- ⏳ Slice 2: `library::watch` daemon — deferred to v0.12.1
- ⏳ Slice 4: Cross-doc Beacon chat — deferred
- ⏳ Slice 5: Saved searches — deferred

**Lathe slices (8 / 8 done — for reference):**
- ✅ Slice 1: `duplicate_pages` kernel + Tauri (commit `e95c0ef`)
- ✅ Slice 2: `split_by_pattern` chapter splitter backend (commit `d0635a7`)
- ✅ Slice 3 backend: `pages_build` composite kernel — handles permutations + duplicates + blank inserts + per-cell rotation in one Tauri round-trip; 14 unit tests (commit `3e93038`)
- ✅ Slice 3 UI: `PagesVisualPanel.svelte` drag-reorder grid (commit `5807fe4`)
- ✅ Slice 4: `SplitPatternPanel.svelte` regex+outline split UI with 5 presets + live preview (commit `3393697`)
- ✅ Slice 5: Multi-PDF tabs in main shell (commits `5af4a92` + `a3e0f49`)
- ✅ **Slice 6: `pdf::edit_text` backend** — `find_text_spans` + `replace_text_span` for ASCII Tj/TJ rewrite. 15 unit tests cover happy path + CID Type0 read-only + non-ASCII read-only + multi-segment TJ kerning read-only + multi-page id distinctness + replace round-trip with extract_text proof + control-char rejection + span-id parser. 2 Tauri commands. 220→235 lib tests (commit `46b0f3c`)
- ✅ **Slice 7: `EditTextPanel.svelte`** — page-tab strip with editable counts, editable-only/all filter toggle, span rows with id pill + font/size + inline input + "edited" pill + revert link, read-only chips with friendly reason, iterative save (chain replace calls, reload on success), collapsible caveats section listing the ASCII-only / Type1-only / no-kerning limitations honestly. Sidebar nav `✎ Edit Text`. (commit `4e5257a`)
- ✅ **Slice 8: Release prep** — version bumped 0.10.0 → 0.11.0 in Cargo.toml + tauri.conf.json + package.json. Cargo.lock refreshed. Sidebar version pill updated. Release notes written to `docs/release-notes/v0.11.0.md` covering all 5 user-visible features. (commit `79f820a`)

---

## ROADMAP

### v0.8.1 "Polyglot" — RELEASED 2026-05-16
- Tag `v0.8.1`, merge SHA `39ff562`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.8.1)

### v0.9.0 "Toolkit" — RELEASED 2026-05-16
- Tag `v0.9.0`, merge SHA `ba3b291`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.0)

### v0.9.1 "Toolkit UX" — RELEASED 2026-05-16
- Tag `v0.9.1`, merge SHA `7226574`, CI run `25980874364`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.1)

### v0.10.0 "Beacon" — RELEASED 2026-05-17
- Tag `v0.10.0`, merge SHA `f91b374`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.10.0)

### v0.11.0 "Lathe" — RELEASED 2026-05-17
- Tag `v0.11.0`, merge SHA `76cd7ed`, CI run `25987817724`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.11.0)
- Headline: in-place PDF text editing (ASCII / Type1+TrueType). Backend `pdf::edit_text` + frontend `EditTextPanel`.

### v0.12.0 "Atlas" — Library Mode (IN PROGRESS)
Cross-doc Beacon chat across indexed library, tags, collections, watch folders.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.12.0. 7 slices.

### v0.13.0 "Lens" — OCR + Vision (PLANNED)
Local OCR (surya/tesseract), table → CSV, math → LaTeX, vision Q&A in Beacon, auto-tag. **Also where CID/Unicode text-edit work lands** (CMap rewriting, font subsetting).
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.13.0. 9 slices.

### v0.14.0 "Stack" — Diff & Compare (PLANNED)
Visual + text diff, track changes, patch/merge, Beacon diff summary.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.14.0. 6 slices.

### v0.15.0 "Theater" — Presenter Mode (PLANNED)
Slides view, presenter window, live drawing, auto-advance, Stream Deck profile.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.15.0. 5 slices.

### v1.0.0 "Glass" — Stable Release (PLANNED)
Floating panels, multi-window, command palette (⌘K), Vim bindings, a11y, i18n, frozen API.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v1.0.0. 10 slices.

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

### Release finalize (MODE B):
```bash
# 1. Download artifacts from CI run
mkdir -p /tmp/slab-vX.Y.Z-release
gh run download <RUN_ID> -R Sanjays2402/slab -D /tmp/slab-vX.Y.Z-release/
# 2. Stage in assets/ (gitignored) — rename mac x64 dmg
mkdir -p assets/vX.Y.Z
cp .../Slab_X.Y.Z_aarch64.dmg assets/vX.Y.Z/
cp .../Slab_X.Y.Z_x64.dmg assets/vX.Y.Z/Slab_X.Y.Z_x64_macos.dmg
cp .../*.{deb,AppImage,msi,exe} assets/vX.Y.Z/
# 3. Build release body from docs/release-notes/vX.Y.Z.md
# 4. Create release. Big assets (76MB AppImage) sometimes time out the
#    single `gh release create` call — use a background process,
#    then `gh release edit vX.Y.Z --draft=false --latest` once all 6 assets are up.
gh release create vX.Y.Z --title 'vX.Y.Z — Codename emoji' --notes-file body.md assets/vX.Y.Z/*
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
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁 ✦ ⌕ 🔒 ✂ ≣ ✎
- `gh release create` with 6 assets including the 76MB AppImage often
  times out at 60s in foreground. Run it in `background=true` or upload
  the AppImage with a follow-up `gh release upload` and then
  `gh release edit --draft=false --latest`.

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
- 2026-05-16 20:36 (Cake/cron): **🚀🚀 DOUBLE-RELEASE TICK** — Sanjay granted direct-merge permission. Cleared 3-tick PR_READY backlog: merged both `feature/v0.8.1-polyglot` (SHA `39ff562`, tag `v0.8.1`) and `feature/v0.9.0-toolkit` (SHA `ba3b291`, tag `v0.9.0`) to main. Both CI runs queued. Quality gates re-verified on each before merge (90 + 101 lib tests).
- 2026-05-16 21:25 (Cake/cron): 🚀 RELEASE + KICKOFF TICK: v0.9.1 published; v0.10.0 Beacon Slice 1 shipped.
- 2026-05-16 21:46 (Cake/cron): Beacon Slice 2 (provider abstraction) shipped in 4 commits.
- 2026-05-16 22:45 (Cake/cron): 🚀 TRIPLE-SLICE TICK — Beacon Slices 3+4+5 in one tick. 121→133 lib tests. v0.10.0 50% shipped.
- 2026-05-16 23:50 (Cake/cron): 🚀 Beacon Slice 9 — Selection Actions vertical slice in 2 commits. 173→186 lib tests. v0.10.0 90% shipped.
- 2026-05-17 00:09 (Cake/cron): 🚀 v0.10.0 "Beacon" MERGED TO MAIN.
- 2026-05-17 01:20 (Cake/cron): 🚀 BIG LATHE TICK — Slice 3 (backend+UI) + Slice 4 in 3 commits. v0.11.0 50% shipped.
- 2026-05-17 02:30 (Cake/cron): 🚀 BIG LATHE TICK — Slice 5 multi-PDF tabs (5af4a92 + a3e0f49). v0.11.0 62% shipped.
- **2026-05-17 02:55 (Cake/cron): 🚀🚀 TRIPLE-SLICE LATHE CLOSEOUT — Slices 6 + 7 + 8 in one tick.** Took v0.11.0 from 62% → 100% DONE. (a) Slice 6 `pdf::edit_text` backend: 818 LOC including 15 unit tests. `find_text_spans` walks every page's content streams, tracks Tf/Td/Tm/T*/BT/ET state, emits one TextSpan per Tj/TJ/'/' op with stable `p<page>:s<seq>` ids. `replace_text_span` re-decodes the target stream, finds the op by sequence number, swaps its literal-string operand, re-encodes. Handles single Tj happy path, single-segment TJ arrays (editable), multi-segment TJ (read-only, kerning hint), CID Type0 fonts (read-only, font_is_safe check), non-ASCII glyphs in source (read-only), and rejects non-ASCII replacements + control chars + malformed span-ids. 220→235 lib tests (+15). 2 new Tauri commands (commit `46b0f3c`). (b) Slice 7 `EditTextPanel.svelte` 530 LOC: file picker → scans on open → groups spans by page in a tab strip with editable counts + "•" dirty indicator → editable-only/all filter toggle → span rows with id pill + font/size badge + inline `<input>` bound to current value + "edited" pill + revert link → read-only rows with friendly reason chips → Apply chains replace calls writing to a save-dialog destination, then re-loads the saved file so user can keep editing → collapsible caveats section that's honest about ASCII/Type1/no-kerning limits. Sidebar nav `✎ Edit Text`. svelte-check 0 errors (commit `4e5257a`). (c) Slice 8 release prep: version bumped 0.10.0→0.11.0 in Cargo.toml + tauri.conf.json + package.json, Cargo.lock refreshed, sidebar version pill bumped, comprehensive release notes at `docs/release-notes/v0.11.0.md` covering all 5 user-visible Lathe features + stats + upgrade notes (commit `79f820a`). All 4 quality gates green: fmt + clippy `-D warnings` + 235 lib tests + svelte-check 0 errors. Branch pushed. STATE marked DONE — next tick is MODE A merge to main + tag v0.11.0 + push.
- **2026-05-17 03:05 (Cake/cron): 🚀🚀🚀 v0.11.0 "Lathe" MERGED TO MAIN.** Same-tick MODE C → MODE A chain after the triple-slice closeout. Merged `feature/v0.11.0-lathe` to `main` with `--no-ff` (resolved STATE.md conflict with `--theirs` since feature branch had the up-to-date DONE flags). Re-ran all 4 quality gates ON MAIN: fmt clean, clippy `-D warnings` clean, 235 lib tests pass, svelte-check 0 errors. Tagged `v0.11.0` with message "Slab v0.11.0 — Lathe 🪚". Pushed `main` + tag. Merge SHA `76cd7ed`. CI run `25987817724` building 6 installers. STATE flipped to RELEASE_PENDING. Next tick: MODE B finalize when CI green, then pivot to v0.12.0 Atlas.
- **2026-05-17 03:32 (Cake/cron): 🚀 ATLAS SLICE 1 SHIPPED — Library Backend Foundation.** Full backend stack for library mode in one tick: (a) sqlite registry at `pdf::library::registry` with 4 tables (`library_folders`, `library_documents`, `library_tags`, `library_doc_tags`), schema-versioned via `PRAGMA user_version`, `LibraryDb::open`/`open_in_memory`, folder CRUD, document upsert keyed by path, tag CRUD, `set_doc_tags` (replace semantics), cascade delete on folder removal. 15 unit tests. `walkdir = "2"` added to Cargo.toml. (commit `1027569`) (b) walking scanner at `pdf::library::scanner` with `walkdir`-based recursive walk (max_depth=12, no follow_links), quick-key skip on `(size, mtime_ns)`, SHA-256 only on first sight or quick-key mismatch, lopdf page count, corrupt-PDF silent skip (one bad file mustn't fail a 500-doc scan), `ScanReport` with added/updated/unchanged/skipped counts. 10 unit tests covering empty folder + root pdfs + subdir pdfs + non-pdf skip + quick-key unchanged skips hash + quick-key changed re-hashes + corrupt pdf skipped + report counts accurate. (commit `37fc505`) (c) filter+sort query layer at `pdf::library::query` with `LibraryFilter { folder_id, tag_ids: Vec<i64> AND-match, title_substring case-insensitive, limit, sort: AddedDesc|TitleAsc|LastSeenDesc }`, dynamic SQL builder, eager tag-load via second query + HashMap to avoid O(rows×tags) round-trips. 9 unit tests. (d) 8 Tauri commands in `lib.rs`: `slab_library_add_folder`, `slab_library_remove_folder`, `slab_library_list_folders`, `slab_library_scan`, `slab_library_list_docs`, `slab_library_list_tags`, `slab_library_add_tag`, `slab_library_set_doc_tags`. Each opens `~/.slab/library.sqlite` on demand. `LibraryError → CmdResult<T>` From-impl following the `IndexError` pattern. (e) TS client bindings at `src/lib/library.ts` (3927 bytes) — typed wrappers with DTO mirrors (FolderRecord, DocumentRecord, TagRecord, ScanReport, LibraryFilter, LibrarySortBy), each returns `Promise<T>` or rejects `Error(message)`. No UI logic — IPC seam only. (commit `08f5bdd`) Quality gates: fmt clean, clippy `-D warnings` clean (needed one `#[allow(clippy::too_many_arguments)]` on `upsert_document` since 8 cols is structural), 269 lib tests passing (235→269, +34), svelte-check 0 errors. Branch pushed. NOT merging to main yet — Slice 3 LibraryPanel UI must ship first to make the backend visible to users.
- **2026-05-17 05:00 (Cake/cron): 🚀 LENS SLICE 2 SHIPPED — Library Auto-OCR Queue end-to-end in 4 commits.** Vertical slice top-to-bottom: (a) `feat(lens): library schema v2 + scanner writes ocr_state` (8ca2ba0) — bumped `SCHEMA_VERSION` 1→2 in `pdf::library::registry`, added 7 `OCR_STATE_*` consts (`unknown`, `text_native`, `scanned`, `mixed`, `ocr_pending`, `ocr_done`, `ocr_failed`), added `ocr_state` (NOT NULL DEFAULT 'unknown') + `ocr_output_path` (nullable) columns via `ALTER TABLE` migration, extended `DocumentRecord` with both fields, added `set_doc_ocr_state`/`set_doc_ocr_output_path` setters; `upsert_document` gained 8th arg `initial_ocr_state` that only upgrades from `unknown` (so OCR'd files don't get reset on rescan); scanner.`heavy_inspect` now calls `scan_audit::audit` and derives ocr_state from `Recommendation::{None→text_native, OcrAll→scanned, OcrSome→mixed}` (audit failures fall back to `text_native` so registry never breaks); 13 new tests. (b) `feat(lens): add ocr_queue module` (220d025) — new `pdf::library::ocr_queue` with `list_pending(&LibraryDb)` (scanned/mixed only, added_at ASC), `run_one(&mut LibraryDb, doc_id, &OcrOpts)` (flips state to ocr_pending → calls `pdf::ocr::ocr` → flips to ocr_done+output_path on success / ocr_failed+error on failure; always returns `OcrQueueResult`), `run_all(&mut LibraryDb, &OcrOpts)` (drains list_pending, continues past per-doc failures), `ocr_output_path_for(input)` helper (canonical `<stem>.ocr.<ext>` naming). 9 unit tests cover path naming (extension casing, no-extension), list filtering (excludes done/failed/text_native), ordering (added_at ASC), error paths (missing doc id, missing input file). 290→299 lib tests. (c) `feat(lens): expose OCR queue via Tauri + TS bindings` (31bc79a) — 3 `slab_library_ocr_queue_*` commands (`list_pending`, `run_one(doc_id, opts?)`, `run_all(opts?)`) registered in `invoke_handler`; `OcrState` string union + `OcrQueueResult` + `OcrOpts` types in `src/lib/library.ts`; 3 typed wrappers `ocrQueueListPending`/`ocrQueueRunOne`/`ocrQueueRunAll` following the established `unwrap` pattern. DocumentRecord type extended with `ocr_state` + `ocr_output_path`. (d) `feat(lens): LibraryPanel OCR queue UI` (e72a60c) — color-coded badges in card-meta (amber Scanned, purple Mixed, blue OCR'ing…, green OCR'd, red OCR failed); per-card action buttons `🔍 Run OCR` (when state ∈ {scanned, mixed, ocr_failed}) and `📄 Open OCR'd` (when state == ocr_done); toolbar `🔍 OCR N pending` button shown only when `pendingOcrCount > 0`; context-menu entries for both actions; optimistic local update via `applyResult(r)` so UI repaints instantly without full re-fetch; `ocringDocIds` set tracks per-doc spinner state; per-doc OCR failures land in `result.error` (not thrown) so `runAll` keeps draining. Also fixed clippy `redundant_closure_call` in `ocr_queue::run_one` (changed `(|| { ... })()` to plain block). 302 lib tests / svelte-check 0 errors / clippy `-D warnings` clean / pnpm build clean. Branch pushed pending.
