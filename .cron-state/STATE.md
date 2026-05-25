# Slab Cron State

Last updated: 2026-05-24 20:58 PT by Cake (cron)

## Active version

**v3.27.0 "Quill Auto-Detect" — tag pushed, CI still building bundles.**
- Merge SHA on main: bcfabb2
- Tag: v3.27.0
- build run 26381993650: cargo test ✓ (mac-arm64, win-x64, linux-x64) — bundles still in_progress (~17 min in)
- deploy-try 26381993651 ✓ success
- Docker(v3.27.0) 26381993649 ✓ success

## RELEASE_PENDING

v3.27.0 — merge SHA bcfabb2, tag v3.27.0, build run 26381993650 (bundles in flight). Next tick: poll CI, on green run `gh run download 26381993650` → `gh release create v3.27.0 --notes-file docs/release-notes/v3.27.0.md` + upload 6 artifacts.

## This tick (2026-05-24 20:50–20:58 PT)

**v3.26.0 release artifacts ATTACHED** — closed the carry-over from prior tick.
- Downloaded all 4 bundle artifacts from CI run 26381227405 (all green) into /tmp/slab-v3.26.0/.
- Uploaded 6 curated artifacts to release v3.26.0:
  - Slab_3.26.0_aarch64.dmg (macOS Apple Silicon)
  - Slab_3.26.0_x64.dmg (macOS Intel)
  - Slab_3.26.0_amd64.deb (Linux)
  - Slab_3.26.0_amd64.AppImage (Linux portable)
  - Slab_3.26.0_x64-setup.exe (Windows NSIS)
  - Slab_3.26.0_x64_en-US.msi (Windows MSI)
- v3.26.0 release page now has full cross-platform download set live.

This is a MODE B finalize tick. No new code shipped this tick — but downloadable installers for a major Quill Designer release crossed the line from "tagged" to "actually usable by paying customers."

## Wow tracker

LAST_WOW_TICK_AT: 2026-05-25T03:42:35Z (v3.27.0 Quill Auto-Detect — heuristic form-field detector)

## Recently closed issues

(none — issue list still empty per last poll)

## Next ticks

- **MODE B next tick**: poll v3.27.0 build run 26381993650. On green, download artifacts + `gh release create v3.27.0 --notes-file docs/release-notes/v3.27.0.md` with 6 artifacts.
- Then re-poll `gh issue list` for #23-#27 priority override.
- Then v3.28.0 Quill Hub per `docs/plans/2026-05-24-v3.28.0-quill-hub.md` (queued).
