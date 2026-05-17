# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: SHIPPING (v0.10.0 Beacon Slices 1-9 landed — chat + summary + search + PII + selection actions live)

**Active dev branch:** `feature/v0.10.0-beacon`

**No RELEASE_PENDING.** v0.8.1, v0.9.0, v0.9.1 all live on GitHub Releases with 6 installers each.

---

## ROADMAP

### v0.8.1 "Polyglot" — RELEASED 2026-05-16
- Tag `v0.8.1`, merge SHA `39ff562`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.8.1)

### v0.9.0 "Toolkit" — RELEASED 2026-05-16
- Tag `v0.9.0`, merge SHA `ba3b291`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.0)

### v0.9.1 "Toolkit UX" — RELEASED 2026-05-16
- Tag `v0.9.1`, merge SHA `7226574`, CI run `25980874364`, [GH release](https://github.com/Sanjays2402/slab/releases/tag/v0.9.1)

### v0.10.0 "Beacon" — IN PROGRESS (Slices 1-9 of 10 shipped)
- Branch: `feature/v0.10.0-beacon`, tip pushed this tick.
- Plan promoted to `docs/plans/2026-05-16-v0.10.0-beacon-ai.md` 2026-05-16 21:46 PDT.
- **Slice 1 DONE 2026-05-16 21:25 PDT**: `AiProvider` trait + `OllamaProvider` impl + 5 mockito unit tests. Commit `154c008`.
- **Slice 2 DONE 2026-05-16 22:00 PDT** (4 commits): `OpenAiCompatibleProvider` + `BeaconConfig` (TOML) + `make_provider` factory + 4 Tauri config commands. 106→121 lib tests.
- **Slice 3 DONE 2026-05-16 22:45 PDT** (commit `cc5a6ea`): Chat backend `ai/chat.rs`. `build_context` (page-aware truncation), `extract_citations` (handles `[pN]` / `[page N]` / `[pages 2,5,9]`, rejects footnote `[1]` and dates), `beacon_chat()` + `_from_path()` + 7 tests via in-memory MockProvider. Tauri command `slab_beacon_chat`. 121→128 lib tests.
- **Slice 4 DONE 2026-05-16 22:45 PDT** (commit `9abc425`): `BeaconChatPanel.svelte` + sidebar nav (✦ Beacon AI between Reader and Merge). Conversation view, citation chips that dispatch `slab:beacon-goto-page`, sample-prompt grid empty state, friendly error mapping (Ollama down → "start ollama or switch provider" hint), Enter-to-send composer. svelte-check clean.
- **Slice 5 DONE 2026-05-16 22:45 PDT** (commit `21960ce`): Auto-summary. `ai/summary.rs` with `SummaryLength` enum (Tldr/Short/Long), `BeaconSummary` DTO, low-temperature (0.1) prompt, per-length token budgets. Tauri command `slab_beacon_summary`. UI: 3 quick-action chips (✦ TL;DR / ✦ Summarize / ✦ Detailed) in BeaconChatPanel — result lands as an assistant turn so follow-up questions work conversationally. 5 new tests. 128→133 lib tests.
- **Slice 6 DONE 2026-05-16 22:48 PDT** (commit `92a58b2`): Semantic search backend. `ai::chunker` (page-aware, paragraph-first, UTF-8-safe) + `ai::embedding_index` (rusqlite at `~/.slab/beacon-index.sqlite`, brute-force cosine top-K, idempotent re-index by content SHA-256). 22 new tests via MockEmbedProvider. 133→156 lib tests.
- **Slice 7 DONE 2026-05-16 23:11 PDT** (commit `1a8db1f`): Semantic search UI. `BeaconSearchPanel.svelte` (506 LOC) — index card with Browse/Index/Re-index, search bar with All/This-PDF scope toggle, hit cards (page chip + filename + similarity %), footer stats, friendly-error mapping. 4 Tauri commands: `slab_beacon_index_pdf`, `slab_beacon_search`, `slab_beacon_index_stats`, `slab_beacon_index_forget`. Sidebar nav `⌕ Beacon Search`.
- **Slice 8 DONE 2026-05-16 23:25 PDT** (commits `fd303d1` + `e4a43a0`): PII Highlighter. `ai::pii` module — regex pass (email/SSN/phone/CC reusing `auto_redact` presets, now pub-exported) + optional LLM pass (names + addresses via configured provider, liberal JSON parsing, per-page best-effort errors). `BeaconPiiPanel.svelte` (~620 LOC) — kind checkboxes + AI toggle + custom regex patterns + colored kind pills + click-to-jump hits + one-click "Redact selected → save as new PDF" reusing `pdf::auto_redact`. 2 Tauri commands: `slab_beacon_pii_find` (returns hits + summary), `slab_beacon_pii_redact` (thin wrapper over auto_redact). 17 new tests via in-memory MockProvider. 156→173 lib tests. Sidebar nav `🔒 PII Redact`.
- **Slice 9 DONE 2026-05-16 23:50 PDT** (commits `14b6e7d` + `4825c93`): Selection Actions — floating LLM bubble on text highlight. `ai::selection_action` module with 5 actions (Translate/Explain/Define/Rewrite/Summarize), per-action prompts, low temperature (0.2), per-action max_tokens budget (80-500), hard cap at 8K chars. Tauri command `slab_beacon_selection_action(text, action, target_lang?)`. `BeaconSelectionBubble.svelte` (~620 LOC) — captures `mouseup` selections inside the PDF.js text layer, positions above the selection bbox, 5-button action grid, inline target-language picker for Translate (15 languages), result view with Copy button, Esc/click-outside dismiss. Mounted as a sibling of `pdfjs-container` in ReaderPanel. 13 new tests via in-memory MockProvider. 173→186 lib tests. **No new sidebar nav** — this lives inline in the reader.
- **Slice 10 next**: Release prep — version bump 0.9.x → 0.10.0, write release notes, smoke test, MARK STATUS: DONE for next tick to merge to main.
- Remaining: Slice 10 only.

### v0.11.0 "Lathe" — Edit Mode (PLANNED)
In-place PDF text editing, page reorder/insert/delete, multi-PDF tabs, image insert.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.11.0. 8 slices.

### v0.12.0 "Atlas" — Library Mode (PLANNED)
Cross-doc Beacon chat across indexed library, tags, collections, watch folders.
Spec: `.cron-state/proposals/roadmap-to-v1.0.md` § v0.12.0. 7 slices.

### v0.13.0 "Lens" — OCR + Vision (PLANNED)
Local OCR (surya/tesseract), table → CSV, math → LaTeX, vision Q&A in Beacon, auto-tag.
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
- Sidebar nav icons in use: ▥ ⧉ ⎯ ▦ ▼ ❡ ▣ ○ ↔ ⓘ № ✍ ⊟ ＋ ≡ ▮ ⊘ ▦ Ⓜ ◐ ⅰ ▤ ⊗ ✚ 👁
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
- **2026-05-16 21:25 (Cake/cron): 🚀 RELEASE + KICKOFF TICK**: (1) v0.9.1 "Toolkit UX" published on GitHub Releases with all 6 installers, latest release. (2) v0.10.0 **Beacon Slice 1** shipped on `feature/v0.10.0-beacon`: `AiProvider` trait + `OllamaProvider` impl (chat + embeddings) + 5 mockito unit tests (no real Ollama in CI). New deps reqwest/async-trait/futures-util/bytes (runtime), mockito (dev). 101→106 lib tests. All quality gates green. Commit `154c008`. Also added `assets/` to `.gitignore` (cron-staged release binaries live on GitHub Releases, not in git).
- 2026-05-16 21:46 (Cake/cron): Beacon Slice 2 (provider abstraction) shipped in 4 commits: plan promoted to `docs/plans/`, `OpenAiCompatibleProvider` + 6 mockito tests, `BeaconConfig` TOML + `make_provider` + 7 tests, 4 Tauri commands wired. 106→121 lib tests. New runtime dep: `toml` 0.8.
- **2026-05-16 22:45 (Cake/cron): 🚀 TRIPLE-SLICE TICK — Beacon Slices 3+4+5 in one tick.** (a) Slice 3 backend `ai/chat.rs` — page-aware context builder + citation extractor + `beacon_chat()` + 7 tests via in-memory MockProvider + Tauri `slab_beacon_chat` command (commit `cc5a6ea`). (b) Slice 4 frontend `BeaconChatPanel.svelte` — conversation view, citation chips that dispatch goto-page events, sample-prompt grid, friendly-error mapping, Enter-to-send composer; nav entry "✦ Beacon AI" between Reader and Merge (commit `9abc425`). (c) Slice 5 summary — `ai/summary.rs` with Tldr/Short/Long enum + low-temp prompts + 5 tests; Tauri `slab_beacon_summary`; 3 quick-action chips in chat panel that push results as assistant turns (commit `21960ce`). Total: 121→133 lib tests (+12), 1 new module + 1 new component + 2 new Tauri commands. All quality gates green (fmt, clippy `-D warnings`, lib tests, svelte-check). Pushed to `feature/v0.10.0-beacon`. v0.10.0 is now 50% shipped (5 of 10 slices done).
- **2026-05-16 23:50 (Cake/cron): 🚀 Beacon Slice 9 — Selection Actions vertical slice in 2 commits.** Backend `ai::selection_action` module (5 actions: Translate/Explain/Define/Rewrite/Summarize, per-action prompts pinned in tests, low temp 0.2, per-action max_tokens 80-500, 8K-char selection cap with user-grade error) + Tauri command `slab_beacon_selection_action` (commit `14b6e7d`, 13 new tests via in-memory MockProvider). Frontend `BeaconSelectionBubble.svelte` (~620 LOC) — floats above the PDF.js text-layer selection on `mouseup`, 5-button action grid + emoji + tooltips, inline Translate target-language picker (15 languages), busy/error/result states with Copy button, friendly-error mapping for Ollama-down + 429, Esc dismiss, mounted inside ReaderPanel as a sibling of pdfjs-container (commit `4825c93`). 173→186 lib tests (+13). All quality gates green (fmt, clippy `-D warnings`, lib tests, svelte-check 0 errors). v0.10.0 is now 90% shipped (9 of 10 slices). Slice 10 (release prep + version bump + DONE flag for next-tick merge) is the only remaining work.
