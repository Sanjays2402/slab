# Slab Cron State

Last updated: 2026-05-24 22:23 PT by Cake (cron)

## Active version

**v3.29.0 "Forms Tour" — SHIPPED on `feature/v3.29.0-forms-tour`.**

Branch pushed, CI run 26384738089 in progress. STATUS: DONE — ready to
merge to main on the NEXT tick.

**v3.28.0 "Quill Hub" — RELEASED.** CI 26384107071 went green during
this tick (caught it at the 22-min mark) and the release was published
with all 6 artifacts (mac arm64+x64 dmg, linux x64 deb+AppImage, win
x64 msi+nsis). Docker tag build 26384107036 already success.
Release: https://github.com/Sanjays2402/slab/releases/tag/v3.28.0

## This tick (2026-05-24 22:07–22:23 PT)

**v3.29.0 Forms Tour — polish-tier release, shipped end-to-end.**

A 5-step coachmark tour that auto-fires the first time a user opens the
unified Forms workspace. Animated spotlight ring follows the active tab
with a 220ms cubic-bezier ease, pulsing 2.4s glow draws the eye, full
keyboard control (←/→/Enter/Esc), and `prefers-reduced-motion` respected.

Replayable from the command palette ("Forms: Show welcome tour") and via
the new `Mod+Shift+/` shortcut from anywhere in the app. Completion
flag persists in localStorage so it never re-fires once dismissed.

4 commits, 905 insertions / 4 deletions across 13 files (664 net non-test
LOC):
- 34768ef feat(quill): tour store + onboarding state
- 5562903 feat(quill): coachmark overlay component
- 294e0ac feat(quill): wire into Hub + palette + keyboard shortcut
- 9c46bf9 chore(release): v3.29.0 version bump + release notes

Gates green: cargo fmt ✓, clippy ✓, 1740 lib tests ✓, pnpm check 0 errors.

**Buy-button**:
- Pay-for-it ✅ — onboarding is the #1 reason enterprises pick PDF Expert
  over scrappy tools. Slab now teaches itself in 30s.
- Notice-it ✅ — every existing user sees the tour next time they open
  Forms.
- Pick-us ✅ — Acrobat hides "Prepare Form" three menus deep.
- Tell-a-friend ✅ — spotlight + ease motion is the screenshot.

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T05:22:39Z (v3.29.0 Forms Tour — spotlight
spotlight + cubic-bezier-eased coachmark on first visit to Forms)

## Recently closed issues

(none — issue list empty per last poll)

## Next ticks

- **MODE A**: merge `feature/v3.29.0-forms-tour` into main, tag v3.29.0,
  push, download artifacts from the resulting build, finalize the v3.29.0
  GitHub release.
- After v3.29.0 finalize: re-poll `gh issue list`, then start the
  **v0.10.0 Beacon** arc (Ollama-backed local PDF chat) — the
  long-promised buyer-magnet release per the roadmap. Specs at
  `.cron-state/proposals/v0.10.0-beacon-ai.md`.

## Pipeline state

| Branch                        | Status            | Notes                                  |
| ----------------------------- | ----------------- | -------------------------------------- |
| `main`                        | v3.28.0 RELEASED  | Live on GitHub with 6 artifacts        |
| `feature/v3.29.0-forms-tour`  | DONE — pushed     | Merge next tick                        |
