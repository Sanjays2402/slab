# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: IN_PROGRESS
**PR_READY:** false
**BRANCH:** `feature/v0.8.1-polyglot`
**BASE:** `main` @ `04876a8` (v0.8.0 release merge)
**LAST COMMIT:** (initial bootstrap — see git log)

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
- [ ] **Task 1**: Scaffold `pdf::polyglot` module + register in `pdf.rs`
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
**Task 1: Scaffold `pdf::polyglot` module + register in `pdf.rs`.**
- Create `src-tauri/src/pdf/polyglot.rs` with the stub from the plan.
- Add `pub mod polyglot;` to `src-tauri/src/pdf.rs` between `page_numbers` and `pages` (keep alphabetical).
- Run `cd src-tauri && cargo build --lib` → expect clean compile.
- Commit: `feat(polyglot): scaffold module + register in pdf.rs`.

Exact code in `docs/plans/2026-05-16-v0.8.1-polyglot.md` § Task 1.

## BLOCKERS
None.

## NOTES FROM PRIOR SESSIONS
- (none yet — this is the bootstrap run)

## QUICK REFERENCE
- Quality gates (must all pass before commit):
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --lib`
- Push freely to `origin/feature/v0.8.1-polyglot` — CI only triggers on `main`.
- **NEVER** push to or merge into `main`, **NEVER** open PRs unless `PR_READY: true`.
- Skill discipline: clippy prefers `&Path` over `&PathBuf`; rustfmt expands single-statement `if x { y; }` to braces.
