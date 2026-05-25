# Slab Cron State

Last updated: 2026-05-24 22:01 PT by Cake (cron)

## Active version

**v3.28.0 "Quill Hub" — MERGED + TAGGED + PUSHED.** CI in progress.

- Merge SHA: f550863 (main)
- Tag: v3.28.0
- CI runs (kicked off 2026-05-25 05:00 UTC):
  - `build` on main → run 26384107071
  - `Docker (slab-server)` on tag v3.28.0 → run 26384107036
  - `deploy-try` on main → run 26384107034

**RELEASE_PENDING: v3.28.0** — finalize next tick (MODE B):
1. Poll run 26384107071 (build) until green.
2. `gh run download 26384107071 --dir /tmp/slab-v3.28.0/`
3. `gh release create v3.28.0 --title 'v3.28.0 — Quill Hub' --notes-file docs/release-notes/v3.28.0.md`
   with the 6 curated artifacts (mac dmg arm64 + x64, linux deb + AppImage, win msi + nsis).
4. Verify Docker tag workflow on v3.28.0 succeeded (run 26384107036).

## This tick (2026-05-24 21:46–22:01 PT)

**v3.28.0 Quill Hub — shipped end-to-end in one MODE C tick.**

The four scattered Acrobat-killer features (Forms Fill, Quill Batch, Quill
Designer, Quill Auto-Detect) are now one unified "Forms" workspace with a
4-tab subnav (Detect · Design · Fill · Batch), a shared state store, a
live status chip, and a smart "Next: …" footer CTA. The three Quill
shortcuts (`Mod+Shift+B/D/Y`) now open the Hub on the matching tab.
Command palette surfaces all four sub-tabs as discoverable actions.

4 commits, 702 insertions, 9 deletions across 14 files:
- 18d8a4f feat(quill): shared hub store
- 809be98 feat(quill): hub shell + tab nav + cross-panel state sync
- c070ad5 feat(quill): palette + shortcut-sheet discoverability
- 36d2d96 chore(release): v3.28.0 version bump + release notes

Gates green: cargo fmt ✓, clippy ✓, 1740 lib tests ✓, pnpm check 0 errors.

Buy-button: passes Pay-for-it (paralegal $49) + Notice-it (new sidebar
behaviour visible day one) + Pick-us (Acrobat's $239/yr "Prepare Form"
flow unified offline + free) + Tell-a-friend (one tab, four steps, CSV
merge at the end is the demo gif).

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T05:00:39Z (v3.28.0 Quill Hub — one tab, four sub-tabs, no more re-picking files)

## Recently closed issues

(none — issue list empty per last poll)

## Next ticks

- **MODE B**: finalize v3.28.0 (poll CI, download artifacts, `gh release create`).
- After finalize: re-poll `gh issue list`, then start the **v0.10.0 Beacon**
  arc (local LLM chat with PDFs via Ollama) — the long-promised buyer-magnet
  release per the roadmap. Specs at `.cron-state/proposals/v0.10.0-beacon-ai.md`.
- Optional polish before Beacon: a single-pane Forms onboarding overlay
  (3-screen tour) that fires on first visit to the Hub — would be a wow tick.
