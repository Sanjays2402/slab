# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: DONE
**PR_READY:** true
**BRANCH:** `feature/v0.9.0-toolkit`
**BASE:** `main` @ `04876a8` (v0.8.0 release merge)
**LAST COMMIT (v0.9.0):** `99dd3e3` — docs: README + release notes for v0.9.0 Toolkit

> v0.8.1 also still PR_READY (separate branch `feature/v0.8.1-polyglot`,
> tip `e49899e`). Open both PRs when ready. They don't conflict — v0.9.0
> was cut from `main` not from the v0.8.1 branch, so each is independent.

## NEXT VERSION: v0.9.1 "Toolkit UX"

Surface what's already wired (flatten / sanitize / repair / decrypt) in
the Svelte UI, and add the decrypt-with-password modal. See proposal
block at the bottom.

---

## v0.9.0 "Toolkit" — DONE

### Goal (shipped)
Three pdftk/qpdf-grade utilities native to Slab, pure Rust, zero new
deps: `slab flatten`, `slab sanitize`, `slab repair`. CLI + Tauri
surfaces ready. Frontend panel deferred to v0.9.1 (CLI users have
everything today).

### Sub-tasks (all complete)
- [x] Plan doc `docs/plans/2026-05-16-v0.9.0-toolkit.md`
- [x] Feature A — `pdf::flatten` module + 3 unit tests (`e7983b8`)
- [x] Feature A — CLI `slab flatten` + Tauri `slab_flatten` (`c776ded`)
- [x] Feature B — `pdf::sanitize` module + tests + CLI + Tauri (`a52f707`)
- [x] Feature C — `pdf::repair` module + 3 unit tests (`c1bcfb0`)
- [x] Feature C — CLI `slab repair` + Tauri `slab_repair` (`b632270`)
- [x] Version bump 0.8.1 → 0.9.0 + clippy clean-up on SanitizeOpts (`cb1b00f`)
- [x] README + release-notes docs (`99dd3e3`)
- [x] Quality gates: fmt / clippy / 101 lib tests / svelte-check all green
- [x] Smoke test: `slab repair tiny.pdf -o out.pdf` round-trips on Mac mini
- [x] Flip STATE → DONE / PR_READY: true (this commit)

### Final state
- **101 lib tests pass** (90 v0.8.1 → 101 v0.9.0, +11: flatten 3,
  sanitize 5, repair 3)
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib`, `pnpm exec svelte-check` — all green
- Manual smoke: `slab repair` round-trips a 1-page synthetic PDF on
  the Mac mini (19:39 PDT 2026-05-16)
- Three CLI subcommands live: `slab flatten`, `slab sanitize`,
  `slab repair`. Three Tauri commands registered.
- README has a 'Toolkit: flatten, sanitize, repair — new in v0.9.0'
  section with copy-paste examples; release notes describe deliberate
  scope cuts (decrypt UX → v0.9.1, sign → v0.9.2, pdfa → v0.9.3).

---

## v0.9.1 "Toolkit UX" — PROPOSAL (next branch)

**Goal:** ship the Svelte UI surface for the v0.9.0 Toolkit backends,
plus a decrypt-with-password modal that uses the already-wired
`pdf::encrypt::decrypt` lib.

### Proposed feature set

1. **Toolkit panel** (`src/lib/panels/ToolkitPanel.svelte`) — sidebar
   entry with three actions: Flatten / Sanitize / Repair. Each opens
   a save-dialog, calls the matching `slab_*` Tauri command, shows the
   report fields inline. Reuses the existing panel scaffolding from
   `RedactPanel` / `OcrPanel`.

2. **Decrypt modal** — when the reader detects an encrypted PDF, surface
   a password prompt. On submit, call `slab_decrypt` (already exists)
   and reload the document. Same UX as Preview's password dialog.

3. **Sanitize-when-saving** option — checkbox on the existing Save As
   dialog: "Strip JS / embeds / launches before saving". One-line
   wiring on top of the new Tauri command.

### v0.9.1 launch plan

- Cut `feature/v0.9.1-toolkit-ux` from `main` once **both** v0.8.1 and
  v0.9.0 PRs merge (cron will see two new commits on `main` and one
  fresh `dev` branch).
- 4-5 frontend tasks per tick. Same cadence as v0.8.1 Task 10
  (`ac401be`) — small Svelte + store wiring + smoke test.
- Defer `pdf::sign` (PKCS#7 crate selection) until human input on the
  dep choice. Defer `pdf::pdfa` until Ghostscript-shellout vs
  pure-Rust research.

### When the next cron tick runs (post-merge)

1. Verify both v0.8.1 and v0.9.0 are merged (`git log main --oneline`).
2. `git checkout main && git pull --ff-only`
3. `git checkout -b feature/v0.9.1-toolkit-ux`
4. Write `docs/plans/2026-05-XX-v0.9.1-toolkit-ux.md`.
5. Ship Task 1 (Toolkit panel skeleton + Flatten action) in the same
   tick.

### If v0.9.0 is NOT yet merged when next tick runs

Do not start v0.9.1 work — don't stack feature branches.
Deliver: `[cron] v0.9.0 awaiting review` and exit.

If v0.8.1 is merged but v0.9.0 isn't (or vice versa), same rule:
wait until both land, then cut a single fresh branch.

---

## NOTES FROM PRIOR SESSIONS
- 2026-05-16 16:43 (Cake/cron): Task 1 done. Scaffold compiled clean, clippy clean, 81 tests pass. Pushed `708531d`. No surprises.
- 2026-05-16 17:00 (Cake/cron): Task 2 done. Pure-fn allow-list + 3 tests. fmt/clippy clean, full suite 84 pass (81→84). Pushed `c66167c`.
- 2026-05-16 17:18 (Cake/cron): Task 3 done. `require_markitdown()` + `markitdown_available()` test gate. Suite 84→86 (+2). Pushed `5b7d9b9`. **Plan deviation**: replaced literal-error-format test with two real preflight tests gated on `markitdown_available()`.
- 2026-05-16 17:37 (Cake/cron): Task 4 done. Real pipeline + 2 cheap unit tests. Suite 86→88 (+2). Pushed `37b9356`.
- 2026-05-16 17:55 (Cake/cron): Task 5 done. html_round_trip test added. Pushed `dff9ca0`.
- 2026-05-16 18:12 (Cake/cron): **Aggressive tick — shipped 7 sub-tasks** (Tasks 6,7,8,9,11,12,13). Backend + CLI + Tauri + docs + version bump. Suite 89→90. Commits: `63bcb02 d190a55 5d29d87 c0e92f7 656248a d4ba16e`.
- 2026-05-16 18:43 (Cake/cron): **Closeout tick** — shipped Task 10 (frontend polyglot wiring incl. drag-and-drop), installed markitdown via pipx on the Mac mini, **verified Task 14 manually** (HTML/CSV/JSON → PDF via release CLI), live tests now actually run (`html_round_trip_produces_pdf` + `csv_round_trip_produces_pdf` both PASS, not skip), flipped STATE to `STATUS: DONE / PR_READY: true`. v0.8.1 is shippable. Commit: `ac401be`. Next: human opens PR; cron drafts v0.9.0 plan when v0.8.1 merges.
- 2026-05-16 19:05 (Cake/cron): **v0.9.0 kickoff** — cut `feature/v0.9.0-toolkit`, wrote plan doc, shipped Feature A (flatten backend + CLI + Tauri) and Feature B (sanitize backend + CLI + Tauri) in one tick. Commits `e7983b8 c776ded a52f707`.
- 2026-05-16 19:34 (Cake/cron): **v0.9.0 closeout** — picked up WIP `pdf::repair` backend (already passing 3 tests), committed it as `c1bcfb0`, then wired CLI + Tauri (`b632270`), fixed a clippy::derivable_impls nit on SanitizeOpts, bumped version 0.8.1→0.9.0 (`cb1b00f`), wrote README + release-notes (`99dd3e3`). 101 lib tests pass; fmt/clippy/svelte-check all clean. v0.9.0 is **PR_READY**. Stacked two ready releases (v0.8.1 + v0.9.0); cron does not auto-open PRs, human decides order. Next: v0.9.1 Toolkit UX once both merge.

## QUICK REFERENCE
- Quality gates (run from `src-tauri/`):
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --lib`
  - `pnpm exec svelte-check` (run from repo root)
- **`TMPDIR` gotcha:** if a prior shell `mktemp -d` session left a stale
  `TMPDIR` env var pointing at a deleted dir, ~all lib tests will
  appear to "fail" with `PathError NotFound` from `tempfile::tempdir()`.
  Workaround: `unset TMPDIR` before running tests. Not a real failure.
- Push freely to `origin/feature/*` — CI only triggers on `main`.
- **NEVER** push to or merge into `main`, **NEVER** open PRs unless `PR_READY: true`.
- **Push gotcha:** plain `git push` errors with "could not read Username". Use:
  ```
  GH_TOKEN=$(gh auth token) git -c credential.helper='!f() { test "$1" = get && echo "username=x-access-token" && echo "password=$GH_TOKEN"; }; f' push origin <branch>
  ```
- **Commit author:** use `git -c user.email='51058514+Sanjays2402@users.noreply.github.com' -c user.name='Cake (cron)' commit …`
- Skill discipline: clippy prefers `&Path` over `&PathBuf`; rustfmt expands single-statement `if x { y; }` to braces; prefer `#[derive(Default)]` to manual impls when the body is `Self::default()`-equivalent.
- `markitdown` runtime: `/Users/sanjay/.local/bin/markitdown` (pipx). Add `$HOME/.local/bin` to PATH when invoking from cron-spawned terminals.
