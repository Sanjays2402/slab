# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: DONE
**PR_READY:** true
**BRANCH:** `feature/v0.8.1-polyglot`
**BASE:** `main` @ `04876a8` (v0.8.0 release merge)
**LAST COMMIT (v0.8.1):** `ac401be` — feat(ui): accept polyglot inputs in open-file flow

## NEXT VERSION: v0.9.0 "Toolkit"

v0.8.1 is feature-complete and verified end-to-end (HTML/CSV/JSON all
round-tripped through the CLI on the Mac mini with markitdown 0.0.2).
Next: open PR for v0.8.1 (requires human approval — `PR_READY` flag set
but cron does not open PRs autonomously per the hard rules).

After the PR merges, kick off **v0.9.0 "Toolkit"** — see proposal block
below. Until merge, additional cron ticks should NOT keep adding to
`feature/v0.8.1-polyglot` (it's done). Cut a new branch
`feature/v0.9.0-toolkit` and start the planning doc.

---

## v0.8.1 "Polyglot" — DONE

### Goal (shipped)
markitdown bridge that lets Slab accept .docx / .xlsx / .pptx / .html /
.epub / .csv / .json / .xml / images / audio as input and convert to
PDF via the existing `md2pdf` engine.

### Decisions (locked, kept for reference)
1. Subprocess, not embed — same model as `pdf::ocr`.
2. No bundling — users `pipx install 'markitdown[all]'`.
3. Stdout → in-memory String → md2pdf. No temp markdown file.
4. Extension allow-list; PDF is rejected on purpose.
5. Error mapping → `PdfError::Other` with actionable messages.
6. Live tests gated behind `markitdown_available()`.
7. Out of scope: Azure DocIntel, YouTube, ZIP recursion, PDF→MD→PDF.

### Sub-tasks (all complete)
- [x] Bootstrap: branch, research note, STATE.md, plan, session log
- [x] Task 1: Scaffold `pdf::polyglot` module + register in `pdf.rs` (`708531d`)
- [x] Task 2: Extension allow-list `supported_extension` (`c66167c`)
- [x] Task 3: `require_markitdown()` preflight + test gate (`5b7d9b9`)
- [x] Task 4: Wire `polyglot_to_pdf` to subprocess + md2pdf (`37b9356`)
- [x] Task 5: Live integration test for .html (gated) (`dff9ca0`)
- [x] Task 6: Live integration test for .csv (gated) (`63bcb02`)
- [x] Task 7: Wire `slab polyglot` into the CLI (`d190a55`)
- [x] Task 8: Wire `slab_polyglot` Tauri command (`5d29d87`)
- [x] Task 9: Full quality-gate sweep (verified only)
- [x] Task 10: Frontend — accept polyglot inputs in open-file flow (`ac401be`)
- [x] Task 11: Docs — README section for Polyglot (`c0e92f7`)
- [x] Task 12: Bump version 0.8.0 → 0.8.1 (`d4ba16e`)
- [x] Task 13: Release notes draft `docs/release-notes/v0.8.1.md` (`656248a`)
- [x] Task 14: Manual smoke check on Mac mini — DONE (see notes below)
- [x] Task 15: Flip STATE to DONE / PR_READY: true (this commit)

### Final state
- Test suite: **90 lib tests pass**, **9 polyglot tests pass** (incl. live
  HTML + CSV round-trips against real markitdown 0.0.2 on the Mac mini)
- Quality gates: `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings`, `cargo test --lib`, `pnpm exec svelte-check` all green
- Manual CLI smoke: HTML/CSV/JSON → PDF all confirmed on the Mac mini
  via `slab polyglot <input> -o <output>` (release build, 18:43 PDT
  2026-05-16)
- `markitdown` installed at `/Users/sanjay/.local/bin/markitdown`
  (pipx, version 0.0.2)

---

## v0.9.0 "Toolkit" — PROPOSAL (queued for next branch)

**Goal:** ship a high-leverage set of PDF utilities that pdftk/qpdf
users rely on, native to Slab. These are independent vertical features
(each one shippable on its own), so they parallelize cleanly across
cron ticks.

### Proposed feature set (rank-ordered by user impact / shipping ease)

1. **`pdf::flatten`** — flatten form fields and annotations into the
   page content stream (no more editable layers). One-shot CLI:
   `slab flatten input.pdf -o out.pdf`. Lopdf-only, no external bin.
   *Smallest ship; do this first to validate the v0.9.0 cadence.*

2. **`pdf::sanitize`** — strip JavaScript, embedded files, launch
   actions, external links. Reuses the `pdf::metadata::strip_*`
   pattern. CLI + Tauri + a new "Sanitize" panel.

3. **`pdf::repair`** — rebuild xref table / fix dangling objects via
   lopdf's compress/clean pipeline. Useful for partially-corrupt PDFs
   that Reader currently fails to load.

4. **`pdf::decrypt`** — given a password, write an unencrypted copy.
   pdf-lib in Rust binding or lopdf with the password. CLI flag
   `--password`. Tauri command prompts via a secure modal.

5. **`pdf::sign`** — visible signature image + invisible cryptographic
   sig. This is the big one — needs a real PKCS#7 signing crate.
   Defer to v0.9.1 if it bloats v0.9.0 scope.

6. **`pdf::pdfa`** — convert to PDF/A-2b (archival). Most ambitious;
   may need Ghostscript shell-out à la markitdown for color-profile
   embedding. Investigate first.

### v0.9.0 launch plan

- Cut `feature/v0.9.0-toolkit` from `main` once v0.8.1 PR merges
- Ship features 1–3 as the v0.9.0 release (flatten / sanitize / repair)
- Defer features 4–6 to v0.9.1 (decrypt) and v0.9.2 (sign + pdfa)
- Each feature gets its own commit chain: backend → CLI → Tauri →
  frontend → docs → release-notes bump. Same pattern as v0.8.1.

### When the next cron tick runs (post-merge)

1. Verify `feature/v0.8.1-polyglot` is merged (check `main` HEAD).
2. `git checkout main && git pull --ff-only`
3. `git checkout -b feature/v0.9.0-toolkit`
4. Write `docs/plans/2026-05-XX-v0.9.0-toolkit.md` (3 features × 5
   tasks each = 15 tasks, matches v0.8.1 cadence).
5. Set STATUS to IN_PROGRESS, BRANCH to the new one.
6. Ship Task 1 (flatten module scaffold + types) in the same tick.

### If v0.8.1 is NOT yet merged when next tick runs

Do not start v0.9.0 work yet — that would mean concurrent feature
branches against `main`. Instead: deliver `[cron] v0.8.1 awaiting
review` and exit. (Cron does not auto-open PRs per the hard rules,
and cron does not merge to `main` ever.)

---

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises.
- 2026-05-16 17:00 (Cake/cron): Task 2 done. Pure-fn allow-list + 3 tests. fmt/clippy clean, full suite 84 pass (81→84). Pushed `c66167c`.
- 2026-05-16 17:18 (Cake/cron): Task 3 done. `require_markitdown()` + `markitdown_available()` test gate. Suite 84→86 (+2). Pushed `5b7d9b9`. **Plan deviation**: replaced literal-error-format test with two real preflight tests gated on `markitdown_available()`.
- 2026-05-16 17:37 (Cake/cron): Task 4 done. Real pipeline + 2 cheap unit tests. Suite 86→88 (+2). Pushed `37b9356`.
- 2026-05-16 17:55 (Cake/cron): Task 5 done. html_round_trip test added. Pushed `dff9ca0`.
- 2026-05-16 18:12 (Cake/cron): **Aggressive tick — shipped 7 sub-tasks** (Tasks 6,7,8,9,11,12,13). Backend + CLI + Tauri + docs + version bump. Suite 89→90. Commits: `63bcb02 d190a55 5d29d87 c0e92f7 656248a d4ba16e`.
- 2026-05-16 18:43 (Cake/cron): **Closeout tick** — shipped Task 10 (frontend polyglot wiring incl. drag-and-drop), installed markitdown via pipx on the Mac mini, **verified Task 14 manually** (HTML/CSV/JSON → PDF via release CLI), live tests now actually run (`html_round_trip_produces_pdf` + `csv_round_trip_produces_pdf` both PASS, not skip), flipped STATE to `STATUS: DONE / PR_READY: true`. v0.8.1 is shippable. Commit: `ac401be`. Next: human opens PR; cron drafts v0.9.0 plan when v0.8.1 merges.

## QUICK REFERENCE
- Quality gates (run from `src-tauri/`):
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --lib`
  - `pnpm exec svelte-check` (run from repo root)
- Push freely to `origin/feature/*` — CI only triggers on `main`.
- **NEVER** push to or merge into `main`, **NEVER** open PRs unless `PR_READY: true`.
- **Push gotcha:** plain `git push` errors with "could not read Username". Use:
  ```
  GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin <branch>
  ```
- **Commit author:** use `git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit …`
- Skill discipline: clippy prefers `&Path` over `&PathBuf`; rustfmt expands single-statement `if x { y; }` to braces.
- `markitdown` runtime: `/Users/sanjay/.local/bin/markitdown` (pipx). Add `$HOME/.local/bin` to PATH when invoking from cron-spawned terminals.
