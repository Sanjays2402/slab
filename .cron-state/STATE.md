# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.

---

## STATUS: 🎉 v3.18.0 "Bind" SHIPPED — merged + tagged, CI running (2026-05-24 05:38 PT)

**TICK 2026-05-24 05:25 PT (Sunday off-hours)** — MODE C → MODE A → RELEASE_PENDING.

Tasks 7-10 of the v3.18.0 Bind plan all executed in one tick. Feature
branch finalised, merged to main, tag pushed, CI kicked off.

### What shipped this tick (5 commits on feature/v3.18.0-bind → merged)

- `092734f` feat(epub): Tauri slab_bind_to_epub + Atelier ConvertToEpub step
- `32f5af6` feat(epub): BindPanel + nav entry + command palette + detached window
- `edd5f4e` chore(release): bump v3.17.0 -> v3.18.0 'Bind' + release notes
- `d855a02` chore(epub): allow clippy::too_many_arguments on slab_bind_to_epub
- `aad86bf` Merge v3.18.0 'Bind' — offline PDF to EPUB 3  ← on main

End-to-end working capability shipped:

1. Backend (last tick): `pdf::epub::convert_to_epub()` produces valid
   EPUB 3 ZIPs (mimetype-first, Stored compression, OPF, nav.xhtml,
   XHTML5 chapters).
2. Tauri command: `slab_bind_to_epub(input, output, opts)` registered
   in generate_handler — frontend can invoke it now.
3. Atelier batch: `Step::ConvertToEpub { detect_tables, detect_lists,
   split_on_h1, language }` — drop a folder of PDFs in a recipe, get
   a folder of EPUBs out.
4. Frontend: `BindPanel.svelte` (13-language dropdown, title/author
   metadata, dropzone, full result stats), wired into:
   - side nav as `bind` with 📖 icon
   - DETACHABLE_PANELS (pop into native window)
   - +page.svelte active + detached routes
   - Command palette: "Convert PDF to EPUB" with keywords for kindle/
     kobo/calibre/reflowable/ebook
5. Marketing release notes + README competitive-matrix table
   (Slab vs Acrobat / PDF Expert / Foxit / Calibre — only Slab and
   Calibre ship PDF→EPUB at all, and Calibre's UI is a 2008 GUI).

### Quality gates (this tick) — ALL GREEN

- `cargo fmt --all --check` ✅
- `cargo clippy --lib --all-targets -- -D warnings` ✅
- `cargo test --lib` ✅ **1588 passed, 0 failed**
- `pnpm check` ✅ 0 errors, 62 (pre-existing) warnings

### Ship size

- 5 commits on feature branch (4 substantive + 1 clippy fix)
- Across both ticks: 10 commits, ~1,430 net LOC
- This tick alone: +662 LOC (panel 522, recipe/run/lib.rs wires 70+,
  release notes + README 70+)

### RELEASE_PENDING: v3.18.0

- Merge SHA: `aad86bf`
- Tag: `v3.18.0`
- CI runs in_progress at tick end:
  - build (main): `26361458975`
  - Docker slab-server (v3.18.0 tag): `26361459009`
  - pages: `26361458765`
- Next tick: MODE B FINALIZE — poll CI, download artifacts, publish
  GitHub release with the 6 best artifacts (mac arm64/x64 dmg, linux
  deb + AppImage, win msi + nsis), verify Docker on GHCR, attach the
  release notes from `.cron-state/release-notes-v3.18.0.md`.

### Buy-Button verdict — all 4 PASS

- **Pay-for-it ✅** — Adobe doesn't ship EPUB. Foxit doesn't. PDF
  Expert doesn't. Calibre does but the UI is from 2008. A $49 power
  user reading research on Kindle would pay.
- **Notice-it ✅** — New nav entry, new palette command, new wedge
  feature visible immediately on next launch.
- **Pick-us ✅** — Closes the only category where Slab competitors
  literally have nothing.
- **Tell-a-friend ✅** — "Drop a PDF, get a Kindle book, offline, free."
  Screenshot bait.

### LAST_WOW_TICK_AT: 2026-05-24T12:38Z (v3.18.0 Bind merge)

Fresh wow this tick (offline PDF→EPUB UI). Next 24h timer reset.

### Next tick (after MODE B finalize) — v3.19.0 "Marquee"

A sibling cron promoted `docs/plans/2026-05-24-v3.19.0-marquee-try.md`
during this tick (note: was originally numbered v3.1.0, sibling
renamed to v3.19.0 to avoid collision with Bind). MODE C will start
on `feature/v3.19.0-marquee-try` next time, after Bind is finalised.

---

## PRIOR STATUS: 🚀 v3.18.0 "Bind" — backend end-to-end on feature branch (2026-05-24 04:55 PT)

(See git history for prior tick log — Tasks 1-6 of the Bind plan, backend
landed on feature branch with 1588 tests green and the convert_to_epub()
pipeline functional end-to-end.)
