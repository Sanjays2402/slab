# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: IN_PROGRESS
**PR_READY:** false
**BRANCH:** `feature/v0.8.1-polyglot`
**BASE:** `main` @ `04876a8` (v0.8.0 release merge)
**LAST COMMIT:** `d4ba16e` — chore: bump version 0.8.0 → 0.8.1 (Polyglot)

## GOAL
Ship **v0.8.1 "Polyglot"** — a `markitdown` bridge that lets Slab accept .docx / .xlsx / .pptx / .html / .epub / .csv / .json / .xml / images / audio as input and convert to PDF via the existing `md2pdf` engine.

Full plan: `docs/plans/2026-05-16-v0.8.1-polyglot.md` (15 bite-sized tasks).
Research note: `.cron-state/research-markitdown.md`.

## DECISIONS (locked, do not relitigate)
1. **Subprocess, not embed.** Shell out to `markitdown` exactly like `pdf::ocr` shells out to `tesseract`/`pdftoppm`. No Python embedding, no FFI.
2. **No bundling.** Users install `markitdown` themselves (`pipx install 'markitdown[all]'`). Friendly preflight error with install hint if missing.
3. **Stdout capture → md2pdf in-memory.** Capture markitdown stdout as a UTF-8 `String`, feed into `crate::pdf::md2pdf::render`. No temp markdown file.
4. **Extension allow-list.** Dispatch by file extension. PDF is **rejected** on purpose (round-trip would silently degrade content).
5. **Error mapping → `PdfError`.**
   - Binary missing → `PdfError::Other("markitdown not found on PATH … pipx install …")`.
   - Subprocess nonzero → `PdfError::Other("markitdown failed (<code>): <stderr>")`.
   - Unsupported ext → `PdfError::Other("unsupported polyglot input: …")`.
   - Empty stdout → `PdfError::Other("markitdown returned empty document …")`.
   - Non-UTF-8 stdout → `PdfError::Other("markitdown produced non-UTF-8 …")`.
6. **Live tests are gated** behind `markitdown_available()` so dev machines without the binary don't break the build (same pattern as `ocr::tesseract_available`).
7. **Out of scope for v0.8.1:** Azure Document Intelligence, YouTube URLs, ZIP recursion, PDF→MD→PDF.

## SUB-TASKS (from plan)
- [x] Bootstrap: branch, research note, STATE.md, plan, session log
- [x] **Task 1**: Scaffold `pdf::polyglot` module + register in `pdf.rs` (commit `708531d`)
- [x] **Task 2**: Extension allow-list `supported_extension` (commit `c66167c`)
- [x] **Task 3**: `require_markitdown()` preflight + `markitdown_available()` test gate (commit `5b7d9b9`)
- [x] **Task 4**: Wire `polyglot_to_pdf` to subprocess + md2pdf (commit `37b9356`)
- [x] **Task 5**: Live integration test for .html (gated) (commit `dff9ca0`)
- [x] **Task 6**: Live integration test for .csv (gated) (commit `63bcb02`)
- [x] **Task 7**: Wire `slab polyglot` into the CLI (commit `d190a55`)
- [x] **Task 8**: Wire `slab_polyglot` Tauri command (commit `5d29d87`)
- [x] **Task 9**: Full quality-gate sweep (verification only — no commit needed)
- [ ] Task 10: Frontend — accept polyglot inputs in open-file flow
- [x] **Task 11**: Docs — README section for Polyglot (commit `c0e92f7`)
- [x] **Task 12**: Bump version 0.8.0 → 0.8.1 (commit `d4ba16e`)
- [x] **Task 13**: Write release notes draft `docs/release-notes/v0.8.1.md` (commit `656248a`)
- [ ] Task 14: Manual smoke check on Mac mini (BLOCKED until `markitdown` is installed locally)
- [ ] Task 15: Flip STATE.md to `STATUS: DONE` / `PR_READY: true`

## NEXT UP
**Task 10: Frontend — wire polyglot into the open-file flow.**

Whole frontend slice for the next tick:
1. **Inspect Svelte structure** — find the open-file handler (likely
   `src/routes/+page.svelte` or `src/lib/components/Open*.svelte`). Search:
   `slab_md2pdf|openFile|@tauri-apps/plugin-dialog`.
2. **Extend the file-picker `accept`** to the polyglot extension list:
   `.docx,.xlsx,.pptx,.xls,.html,.htm,.epub,.csv,.json,.xml,.rtf,.odt,.png,.jpg,.jpeg,.gif,.bmp,.tif,.tiff,.webp,.wav,.mp3,.m4a,.flac,.ogg`.
3. **Add an `openAny(path)` branching helper** — if `.pdf`, open directly; else
   call `invoke('slab_polyglot', { input, output: tmpOut, opts: { page_size: 'A4' } })`,
   then open the produced PDF.
4. **Compute a tmp output path** — use Tauri's `tempDir()` /
   `@tauri-apps/plugin-fs` or a deterministic name in `os.tmpdir()` keyed on
   input basename + sha-ish suffix to avoid clobbering.
5. **Friendly error UI** — if the invoke throws with `markitdown not found`,
   show a toast with the install hint instead of the raw stack trace. Mirror
   how `slab_ocr`'s missing-binary case is handled (search:
   `tesseract not found` for the precedent).
6. **`pnpm exec svelte-check`** must stay green.
7. **Commit:** `feat(ui): accept polyglot inputs in open-file flow`.

After Task 10 ships, the only remaining work is:
- Task 14 (manual smoke) — requires `pipx install 'markitdown[all]'` on the
  Mac mini first. If installed at run-time, do the smoke; otherwise document
  the manual-step blocker and proceed.
- Task 15 (flip STATE to `DONE` / `PR_READY: true`).

## BLOCKERS
- **Task 14 manual smoke** can't run until `markitdown` is installed on the
  Mac mini. The two live-pipeline integration tests (`html_round_trip`,
  `csv_round_trip`) currently skip with `eprintln!("skip: markitdown not on
  PATH")`. The next time Sanjay (or cron) is interactive, run:
  `pipx install 'markitdown[all]'` and the live tests will execute end-to-
  end. Not a code blocker — only the human-smoke step is gated.

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises.
- 2026-05-16 17:00 (Cake/cron): Task 2 done. Pure-fn allow-list + 3 tests. Plan code worked verbatim. fmt/clippy clean, full suite 84 pass (81→84). Pushed `c66167c`. Dependabot surfaced 5 vulns on default branch (4 mod, 1 low) at push time — unrelated to this branch, note for future cleanup.
- 2026-05-16 17:18 (Cake/cron): Task 3 done. `require_markitdown()` + `markitdown_available()` test gate. Suite 84→86 (+2). Pushed `5b7d9b9`. **Plan deviation**: replaced the plan's literal-error-format test with two real preflight tests gated on `markitdown_available()`. **Gotcha**: `require_markitdown` was dead code until Task 4 wired it in; added `#[allow(dead_code)]` with a Task-4 TODO that was removed in Task 4.
- 2026-05-16 17:37 (Cake/cron): Task 4 done. Real pipeline + 2 cheap unit tests (`missing_input_errors`, `unsupported_extension_errors`). Dropped `#[allow(dead_code)]` and the underscore prefixes as planned. Suite 86→88 (+2). Pushed `37b9356`.
- 2026-05-16 17:55 (Cake/cron): Task 5 done. `html_round_trip_produces_pdf` added per plan verbatim. Suite 88→89 (+1, skipped here since no markitdown). Pushed `dff9ca0`.
- 2026-05-16 18:12 (Cake/cron): **Aggressive tick — shipped 7 sub-tasks** (Tasks 6, 7, 8, 9, 11, 12, 13). Backend, CLI, Tauri, docs and version bump are all complete. Suite 89→90 (+1 CSV test). Quality gates all green (fmt check / clippy / 90 tests / cargo build). Commits: `63bcb02` `d190a55` `5d29d87` `c0e92f7` `656248a` `d4ba16e`. **Notable**: Task 9 (quality-gate sweep) needed zero changes — the per-task discipline kept fmt/clippy clean throughout. **rustfmt nit caught early**: `slab_polyglot` Tauri fn must be one-line signature, not multi-line (caught by the patch-tool lint). **Plan deviation on Task 12**: Cargo.lock has multiple 0.8.0 hits but only `slab-app` and `slab-lib` package entries are ours; cargo auto-bumped them on the `cargo build` after the Cargo.toml edit, no manual fixup needed. Branch is feature-complete for the backend & release-prep slice; remaining work is Task 10 (frontend wiring) and Task 14/15 (smoke + flip STATE).

## QUICK REFERENCE
- Quality gates (must all pass before tick ends):
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --lib`
- Push freely to `origin/feature/v0.8.1-polyglot` — CI only triggers on `main`.
- **NEVER** push to or merge into `main`, **NEVER** open PRs unless `PR_READY: true`.
- Skill discipline: clippy prefers `&Path` over `&PathBuf`; rustfmt expands single-statement `if x { y; }` to braces; rustfmt collapses 3-param Tauri command signatures onto one line.
- **Push gotcha:** plain `git push` errors with "could not read Username". Use:
  ```
  GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin feature/v0.8.1-polyglot
  ```
- **Commit author:** use `git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit …` so commits are properly attributed and don't leak any other identity.
