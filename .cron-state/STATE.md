# Slab Cron State

Last updated: 2026-05-24 23:08 PT by Cake (cron)

## Active version

**v3.29.0 "Forms Tour" — RELEASED.** Published on GitHub with 6 artifacts
(macOS arm64/x64 DMG, Linux deb + AppImage, Windows MSI + NSIS). Build run
26385319422 success, Docker tag run 26385319416 success. Issue list empty.

Next arc: **v0.10.0 "Beacon"** (Slab AI — Ollama-backed local PDF chat,
auto-summary, semantic search, PII highlighter, selection actions). Spec at
`.cron-state/proposals/v0.10.0-beacon-ai.md`. First slice next tick:
provider abstraction + Ollama impl + `slab_beacon_chat` Tauri command +
right-rail BeaconChatPanel scaffold (vertical slice 1 of 5).

## This tick (2026-05-24 22:43–23:08 PT)

**MODE B — Finalize release.** Polled CI for ~14 min while build/bundle
jobs finished, then downloaded artifacts and published the release.

- Build run 26385319422: all 4 bundle jobs + 3 cargo test jobs → success
- Docker tag run 26385319416: success (slab-server image live on GHCR)
- Downloaded 6 installers (~390MB), curated to release
- `gh release create v3.29.0` — published (not draft), 6 assets attached
- Release URL: https://github.com/Sanjays2402/slab/releases/tag/v3.29.0
- RELEASE_PENDING cleared

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T05:22:39Z (v3.29.0 Forms Tour — spotlight
+ cubic-bezier coachmark; reaffirmed by public release this tick)

## Recently closed issues

(none — issue list still empty after re-poll)

## Next ticks

- **v0.10.0 Beacon Slice 1**: `src-tauri/src/ai/provider.rs` trait +
  `OllamaProvider` impl + `slab_beacon_chat` streaming Tauri command +
  right-rail `BeaconChatPanel.svelte` skeleton with streaming UI +
  palette entry + keyboard shortcut + STATE.md update.
- Re-poll `gh issue list` at start of every tick (override still active
  if any of #23-#27 reappear).

## Pipeline state

| Branch                        | Status               | Notes                                  |
| ----------------------------- | -------------------- | -------------------------------------- |
| `main`                        | v3.29.0 RELEASED     | Public release live, Docker live       |
| `feature/v3.29.0-forms-tour`  | merged + released    | Safe to delete                         |
| (next) `feature/v0.10.0-beacon` | not started        | Spec ready; first slice = chat panel   |
