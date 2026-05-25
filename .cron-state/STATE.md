# Slab Cron State

Last updated: 2026-05-25 02:28 PT by Cake (cron)

## Active version

**v3.30.0 "Quill Smart Fill" — SHIPPED. Public GitHub release live.**

https://github.com/Sanjays2402/slab/releases/tag/v3.30.0
6 artifacts: macos-arm64.dmg, macos-x64.dmg, linux .deb + .AppImage,
windows .msi + nsis. Release notes from docs/release-notes/v3.30.0.md.

## This tick (2026-05-25 02:26–02:30 PT)

**MODE B — Finalize.** v3.30.0 GitHub release published.

- CI run 26390862691 confirmed green (build/main, 22m37s)
- Docker (slab-server) tag v3.30.0 green (9m37s)
- Downloaded 6 artifacts from run 26390862691
- `gh release create v3.30.0 --title 'v3.30.0 — Quill Smart Fill' --notes-file docs/release-notes/v3.30.0.md` + 6 artifacts
- Cleared RELEASE_PENDING

Note: an earlier `site: pivot landing` run (26391133497) had a bundle
(windows-x64) failure — looks like a runner flake, since the very next push
(26391483500, "docs(plans): v3.31.0 Atlas Lite") went fully green on the
same bundle. Not blocking. No action needed.

## Release pending

(none)

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T07:35:00Z — v3.30.0 Smart Fill (drag-drop AI
form mapping, 100% local). Now live for download. >24h gate: still within
window, next BIG wow due by ~07:35 PT tomorrow.

## Recently closed issues

(none — issue list empty; #23-#27 override no longer applies)

## Queued plans

- `docs/plans/2026-05-25-v3.31.0-atlas-lite.md` — Recent Files panel with
  thumbnails + last-position resume + tag/collection sidebar. 8 tasks,
  designed to fold into 2 BIG cron ticks (1-5, then 6-8).

## Next ticks

- **Tick 1 (NEXT, MODE C)**: start v3.31.0 "Atlas Lite" — execute tasks 1-5
  of the plan as a single BIG slice (backend Recent Files store + thumbnail
  generator + Tauri commands + frontend panel + nav entry + keyboard
  shortcut + palette entry). Branch: `feature/v3.31.0-atlas-lite`.
- **Tick 2**: tasks 6-8 (last-position resume + tag/collection sidebar),
  then merge + tag + release v3.31.0.

## Pipeline state

| Branch                              | Status                | Notes                                  |
| ----------------------------------- | --------------------- | -------------------------------------- |
| `main`                              | **v3.30.0 RELEASED**  | Public release live with 6 artifacts   |
| `feature/v3.30.0-quill-smart-fill`  | merged + released     | Safe to delete                         |
| `feature/v3.29.0-forms-tour`        | merged + released     | Safe to delete                         |
