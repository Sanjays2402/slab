# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: IN_PROGRESS
**PR_READY:** false
**BRANCH:** `feature/v0.8.1-polyglot`
**BASE:** `main` @ `04876a8` (v0.8.0 release merge)
**LAST COMMIT:** `708531d` — feat(polyglot): scaffold module + register in pdf.rs

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
- [ ] Task 2: Implement extension allow-list (`supported_extension`)
- [ ] Task 3: Implement `require_markitdown()` preflight
- [ ] Task 4: Wire `polyglot_to_pdf` to subprocess + md2pdf
- [ ] Task 5: Live integration test for .html (gated)
- [ ] Task 6: Live integration test for .csv (gated)
- [ ] Task 7: Wire `slab polyglot` into the CLI
- [ ] Task 8: Wire `slab_polyglot` Tauri command
- [ ] Task 9: Full quality-gate sweep (fmt + clippy + tests)
- [ ] Task 10: Frontend — accept polyglot inputs in open-file flow
- [ ] Task 11: Docs — README section for Polyglot
- [ ] Task 12: Bump version 0.8.0 → 0.8.1
- [ ] Task 13: Write release notes draft (`docs/release-notes/v0.8.1.md`)
- [ ] Task 14: Manual smoke check on Mac mini
- [ ] Task 15: Flip STATE.md to `STATUS: DONE` / `PR_READY: true`

## NEXT UP
**Task 2: Implement extension allow-list `supported_extension(input: &Path) -> Option<&'static str>`.**
- Modify `src-tauri/src/pdf/polyglot.rs` per plan § Task 2.
- Pure function — case-insensitive ext match, returns canonical kind name.
- Supported kinds (locked): docx, xlsx, pptx, html, htm→html, epub, csv, json, xml, txt, md, markdown→md, png, jpg, jpeg→jpg, gif, bmp, tiff, tif→tiff, webp, heic, mp3, wav, m4a, flac, ogg, opus.
- PDF explicitly rejected (returns None) — decision #4 in STATE.
- TDD: write failing tests first, run, implement, re-run, commit.
- Quality gates: fmt + clippy + test --lib must pass before commit.
- Commit message: `feat(polyglot): extension allow-list with case-insensitive dispatch`.

Exact code in `docs/plans/2026-05-16-v0.8.1-polyglot.md` § Task 2 (line 85 onwards).

## BLOCKERS
None.

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises. The patch tool warned about a "sibling subagent" having modified `pdf.rs` — false alarm, that was the earlier bootstrap session's edit; confirmed file state by reading before commit.

## QUICK REFERENCE
- Quality gates (must all pass before commit):
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --lib`
- Push freely to `origin/feature/v0.8.1-polyglot` — CI only triggers on `main`.
- **NEVER** push to or merge into `main`, **NEVER** open PRs unless `PR_READY: true`.
- Skill discipline: clippy prefers `&Path` over `&PathBuf`; rustfmt expands single-statement `if x { y; }` to braces.
- **Push gotcha:** plain `git push` errors with "could not read Username". Use:
  ```
  GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin feature/v0.8.1-polyglot
  ```
- **Commit author:** use `git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit …` so commits are properly attributed and don't leak any other identity.
