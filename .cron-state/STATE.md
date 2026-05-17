# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: IN_PROGRESS
**PR_READY:** false
**BRANCH:** `feature/v0.8.1-polyglot`
**BASE:** `main` @ `04876a8` (v0.8.0 release merge)
**LAST COMMIT:** `dff9ca0` — test(polyglot): live HTML round-trip (gated on markitdown)

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
**Task 6: Live integration test for `.csv` input (gated on `markitdown_available`).**
- Modify `src-tauri/src/pdf/polyglot.rs` per plan § Task 6 (~line 450).
- Add `csv_round_trip_produces_pdf` test inside `mod tests`:
  1. Skips with `eprintln!("skip: markitdown not on PATH")` when the binary
     is absent (mirrors Task 5's gate).
  2. Writes a tiny CSV (`name,score\nalice,99\nbob,87\n`) to `tempfile::tempdir`.
  3. Calls `polyglot_to_pdf(&csv, &out, PolyglotOpts::default())`.
  4. Asserts `report.source_kind == "csv"` and `out.exists()`.
- Quality gates: fmt + clippy + test --lib must pass.
- Expected suite size: 89 → 90 (test skips here since markitdown isn't installed, but still counts as ok).
- Commit message: `test(polyglot): live CSV round-trip`.
- Exact code in `docs/plans/2026-05-16-v0.8.1-polyglot.md` § Task 6.

## BLOCKERS
None.

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises. The patch tool warned about a "sibling subagent" having modified `pdf.rs` — false alarm, that was the earlier bootstrap session's edit; confirmed file state by reading before commit.
- 2026-05-16 17:00 (Cake/cron): Task 2 done. Pure-fn allow-list + 3 tests. Plan code worked verbatim. fmt/clippy clean, full suite 84 pass (81→84). Pushed `c66167c`. Dependabot surfaced 5 vulns on default branch (4 mod, 1 low) at push time — unrelated to this branch, note for future cleanup.
- 2026-05-16 17:18 (Cake/cron): Task 3 done. `require_markitdown()` + `markitdown_available()` test gate. Suite 84→86 (+2). Pushed `5b7d9b9`. **Plan deviation**: the plan's suggested test (`require_markitdown_error_message_mentions_install`) just constructs a `PdfError::Other` literal and asserts its formatting — it doesn't exercise `require_markitdown()` at all. Replaced with two real tests gated on `markitdown_available()`, mirroring how `ocr.rs` gates `tesseract_available()`. **Gotcha**: `require_markitdown` is dead code until Task 4 wires it in, so clippy `-D dead-code` fails the build. Added `#[allow(dead_code)]` with a TODO referencing Task 4 to remove it. Cleanup is part of Task 4's commit.
- 2026-05-16 17:37 (Cake/cron): Task 4 done. Real pipeline + 2 cheap unit tests (`missing_input_errors`, `unsupported_extension_errors`). Dropped `#[allow(dead_code)]` and the underscore prefixes as planned. Plan code worked verbatim with one rustfmt nit (rustfmt prefers `polyglot_to_pdf(&a, &b, c).unwrap_err()` on one line, not multi-line). Suite 86→88 (+2). Pushed `37b9356`. The two `_available()`-gated tests (Task 3 + Task 4 setup) silently skip on this Mac mini — `markitdown` isn't installed. Live HTML/CSV tests (Tasks 5–6) will also skip until `pipx install 'markitdown[all]'` runs locally; that's fine for CI-less branch work.
- 2026-05-16 17:55 (Cake/cron): Task 5 done. `html_round_trip_produces_pdf` added per plan verbatim. Suite 88→89 (+1, skipped here since no markitdown). fmt/clippy/tests all clean. Pushed `dff9ca0`. No deviations.

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
