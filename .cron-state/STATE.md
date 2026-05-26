# Slab Cron State

Last updated: 2026-05-25 22:45 PT by Cake (cron) — v3.38.0 "Atlas Suggest" shipped end-to-end + merged + tagged + pushed.

## Active version

**v3.38.0 "Atlas Suggest"** — MERGED to main (345f068), tagged v3.38.0, pushed with --follow-tags.

End-to-end working capability: Slab now observes library search history
and proposes personal Smart Folders ("you searched 'invoice' 8 times —
save a folder?") with a sparkle-bordered UI in the Smart Folders Hub.
Heuristic engine only this release; AI variant (slice 4) deferred.

## ⚠️ CI STILL BLOCKED — needs Sanjay (unchanged from prior tick)

GitHub Actions billing failure persists. Tag-trigger Docker workflow
will fail in 4-6s as before. Last 24h `gh run list`: every push
failure is the spending-limit error.

**Action for Sanjay**: https://github.com/settings/billing
→ update payment method OR raise spending limit.

Once unblocked, v3.38.0 + the unreleased v3.33.0 onward will all be
able to produce GitHub Releases. Until then, the tag is pushed but
no DMG/MSI/AppImage artifacts will exist for end users.

## This tick (2026-05-25 22:00-22:45 PT) — MODE C develop tick + MODE A release

**Shipped v3.38.0 "Atlas Suggest" end-to-end on feature branch +
merged to main + tagged + pushed in a single tick.**

Per-slice:

- Slice 1 `96c7206`: schema v6→v7 migration, library_search_log +
  library_suggestion_dismissed tables. +54 / -3 LOC. Test: schema bump green.
- Slice 2 `786dae9`: `pdf/library/search_log.rs` (190 LOC) — record /
  record_conn / recent_queries / count / dismiss / is_dismissed.
  30s dedupe window; 500-row rolling cap. Wired into `search::search()`
  so every library FTS query records a row best-effort. 6 unit tests.
- Slice 3 `fddc1fa`: `pdf/library/folder_suggest.rs` (~280 LOC) —
  deterministic heuristic clusterer. Tokenize + drop stopwords +
  drop pure-numeric tokens. 13-entry emoji domain table.
  FNV-1a 64-bit cluster_hash. Top-3 by support. Honors dismissals.
  7 unit tests cover empty/threshold/cluster/stopwords/dismissed/cap/hash.
- Slice 5 `edeaa87`: 4 Tauri commands (suggestions_list/dismiss/accept,
  search_log_count) + TS client. Accept creates a personal preset via
  LibraryFilter{title_substring=query_template}. 130 LOC.
  Slice 4 (AI provider) deferred — heuristic alone is shippable.
- Slice 6 `845ba28`: `SuggestedFolders.svelte` (~280 LOC) mounted in
  SmartFoldersHubPanel above the search bar. Conic-gradient sparkle
  border with rotating @property --angle animation, pulse on ✨,
  slide-out on accept/dismiss. Honors prefers-reduced-motion.
  Version bumped 3.37.0 → 3.38.0 in package.json, Cargo.toml, tauri.conf.json.
- Slice 7 (release) merge commit `345f068`: merged to main, ran gates
  (all pass), tagged v3.38.0, pushed with --follow-tags.

Quality gates ALL PASS on main:
- `cargo fmt --all --check` → clean
- `cargo clippy --all-targets -- -D warnings` → clean
- `cargo test --lib` → 1822 passed (was 1808; +14 new tests)
- `pnpm check` → 0 errors, 105 warnings (all pre-existing a11y)

## Buy-Button qualification (v3.38.0)

- **Pay-for-it** ✅ Adobe's "Smart Search" / AI suggestions are paid +
  cloud. Slab's run on-device, free.
- **Notice-it** ✅ New "✨ Suggested for you" section materializes at the
  top of the Smart Folders Hub after 10+ searches.
- **Pick-us** ✅ Preview / PDF Expert / Foxit have no concept of
  personalised folder suggestions. Adobe sends your search history to
  their cloud; Slab clusters it locally.
- **Tell-a-friend** ✅ The sparkle-bordered card with rotating conic
  gradient appearing the first time you reopen the hub = pure demo gif.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-26T05:45 UTC (rotating conic-gradient sparkle
border + pulse on the ✨ icon + slide-out animation on the suggestion cards).
Next wow due by ~2026-05-27 05:45 UTC.

## Recently closed issues

- (none this tick — issues #23-#27 already closed in earlier ticks)

## Next ticks

- **Tick 1 (NEXT)**: Branch cleanup — delete merged feature branches
  (v3.34.0, v3.35.0, v3.36.0, v3.37.0, v3.38.0). Mostly housekeeping;
  pair with one buyer-facing item to clear SHIP-SIZE.
- **Tick 2**: v3.38.1 — slice 4 (AI variant) — when the heuristic
  baseline is solid, wire `ai::folder_suggest_ai` so Ollama/OpenAI-compat
  providers can produce nicer suggestion names + icons. Falls back to
  heuristic on any error.
- **Tick 3**: v3.39.0 idea — "Suggested tags": same approach but the
  AI suggests tags to add to a doc based on its title + first page.
- **Tick 4**: revisit landing page demo video (#27) if app can be built
  locally — current pipeline depends on Sanjay unblocking GH Actions.

## Pipeline state

| Branch                                  | Status                              | Notes                            |
| --------------------------------------- | ----------------------------------- | -------------------------------- |
| `main`                                  | v3.38.0 merged + tagged + pushed    | CI billing-blocked               |
| `feature/v3.38.0-atlas-suggest`         | merged → main                       | Safe to delete                   |
| `feature/v3.37.0-smart-folders-hub`     | merged → main                       | Safe to delete                   |
| `feature/v3.36.0-personal-presets`      | merged → main                       | Safe to delete                   |
| `feature/v3.35.0-atlas-presets`         | merged → main                       | Safe to delete                   |
| `feature/v3.34.0-atlas-smart-plus`      | merged → main                       | Safe to delete                   |
