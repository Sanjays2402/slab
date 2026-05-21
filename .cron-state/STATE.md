# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 📋 Eleven release plans on disk — pipeline goes through v2.0.12; v2.0.2 SHIPPED to main

**Main HEAD**: `75bf6c0` (v2.0.11 plan promotion). Will advance by 2 commits this tick (v2.0.12 plan + STATE chore).
**v2.0.12 PLAN** (new this tick): `docs/plans/2026-05-21-v2.0.12-sign.md` (~34 KB, 6 slices + pre-flight, ~2400 LOC, 8 commits, 58 new tests). WOW = single-key `s` drop + stamp palette + **320ms rubber-stamp animation (scale-1.5→1.0 + rotate-3° + 12 ink-spatter droplets, cubic-bezier `(0.34, 1.56, 0.64, 1)`)** (Slice 4). Codename "Sign" ✒️ — **signatures + 14 preset stamps**, the single biggest paid-tier gap remaining. Adobe Sign $180/yr, DocuSign Personal $120/yr — v2.0.12 ships the visual half free, with v2.1.0 "Notary" tracked in open-questions for cryptographic PKCS#7 signing. Reuses v2.0.6 sidecar JSON + v2.0.7 ink capture (extracted to shared `inkCapture.ts`) + v2.0.10 edit mode + v2.0.11 rotation/multi-select. **Must ship AFTER v2.0.11 — extends sidecar v2 + rotation field.**
**v2.0.11 PLAN** (new this tick): `docs/plans/2026-05-21-v2.0.11-anvil.md` (~47 KB, 6 slices + pre-flight, ~2140 LOC, 8 commits, 52 new tests). WOW = Figma-style alignment guides + animated distribute (240ms cubic-bezier) + rotate-handle with live degree readout (Slice 4). Codename "Anvil" ⚒️ — **multi-select** (marquee + Shift-click + Mod+A) + **rotate** (drag handle, 15° snap) + **align** (6 align ops + 2 distribute ops, all animated, one Cmd+Z undoes the lot). **Redeems v2.0.10's two open-question deferrals (group/lasso multi-select + rotate handle) in a single release**, plus introduces alignment guides Acrobat/PDF Expert/Foxit/Bluebeam don't ship. Breaking sidecar v1→v2 migration adds `rotation: number` field to every annotation kind. **Must ship AFTER v2.0.10 — extends every file v2.0.10 touches.**
**v2.0.10 PLAN** (new this tick): `docs/plans/2026-05-21-v2.0.10-lathe.md` (~51 KB, 6 slices + pre-flight, ~2200 LOC, 8 commits, 66 new tests). WOW = keyboard editor (j/k cycle + Delete dissolve + Cmd+Z restore animation) + Figma-style snap-guides on drag + cross-document copy-paste (Slice 4). Codename "Lathe" 🔧 — annotation edit mode: click to select, drag to move, drag handles to resize, undo/redo via command-stack history, copy-paste annotations between docs. **Redeems FIVE deferred items stacked across v2.0.6–v2.0.9 open-questions blocks.** After v2.0.10 ships, Slab reaches Acrobat-Comment-workspace parity + wins on snap-guides, keyboard-first, sidecar JSON — the v1.0.0 launch narrative. **Must ship AFTER v2.0.9 — touches every file v2.0.6–v2.0.9 touch.**
**Previous (v2.0.9) PLAN**: `docs/plans/2026-05-21-v2.0.9-typeset.md` (~46 KB, 6 slices + pre-flight, ~1920 LOC, 8 commits). WOW = single-key `t` + smart font auto-fit on overflow + accent-tinted blinking caret + 220ms type-in fade-in (Slice 4). Codename "Typeset" 📝 — FreeText / typewriter annotations, the **fifth and final annotation primitive** completing the Acrobat-tier markup toolkit (highlight + note + ink + 4 shapes + freetext). Redeems v2.0.8's open-questions defer ("free-floating text box"). Reuses v2.0.6 sidecar JSON + v2.0.7 DrawLayer + v2.0.8 tagged-union Rust enum. Rust emits ISO 32000-1 §12.5.6.6 `/FreeText` dicts. IME-safe via compositionstart/end gating. **Must ship AFTER v2.0.8 — touches every file v2.0.8 touches.** After v2.0.9 the annotation primitive family is feature-complete vs every paid competitor; v2.0.10 ("Lathe") can pivot to edit-mode polish (selection handles, drag-to-move, undo/redo).
**Previous (v2.0.8) PLAN**: `docs/plans/2026-05-20-v2.0.8-compose.md` (~41 KB, 6 slices + pre-flight, ~1880 LOC, 8 commits). WOW = 15° rotation-snap dial + 220ms shape-pop animation on commit (Slice 4). Codename "Compose" 🔷 — adds four shape primitives (rect/ellipse/arrow/line). Will advance by 2 commits this tick (v2.0.8 plan + STATE chore).
**v2.0.2 STATUS**: 🚀 **SHIPPED TO MAIN** — commit `168e638`, tag `v2.0.2` already pushed. Sanjay merged the work directly (no MODE A from cron — humans-first wins).
**v2.0.8 PLAN** (new this tick): `docs/plans/2026-05-20-v2.0.8-compose.md` (~41 KB, 6 slices + pre-flight, ~1880 LOC, 8 commits). WOW = 15° rotation-snap dial + 220ms shape-pop animation on commit (Slice 4). Codename "Compose" 🔷 — adds four shape primitives (rect/ellipse/arrow/line) as the fourth annotation family, redeeming v2.0.7's explicit defer. Reuses v2.0.7 DrawLayer + v2.0.6 sidecar JSON + tagged-union Rust enum. Three-tier rotation snap (no-mod free / Shift 15° fine w/ dial / Cmd 45° axis). **Must ship AFTER v2.0.7 — touches every file v2.0.7 touches.**
**Latest tag**: `v2.0.1` (annotated, pushed) — Bundled Hello Workshop 🧩.
**Latest release**: `v2.0.0` — Workshop 🔧.
**Previous tag**: `v2.0.0` (Workshop — plugin platform).
**Active dev branch**: *(none — five pipeline items on disk as plans; next tick cuts `feature/v2.0.2-workshop-marketplace`)*
**RELEASE_UNBLOCKING**: LF hotfix `1a28a5f` on main. v2.0.1 retag-and-re-release decision deferred (roll unblock into v2.0.2 release notes as cleaner customer narrative).
**v2.0.2 PLAN**: `docs/plans/2026-05-20-v2.0.2-workshop-marketplace.md` (46 KB, 1394 lines, 9 slices, ~1140 LOC, ~10 commits). WOW = Fuse.js live-highlight search in Slice 6.
**v2.0.3 PLAN**: `docs/plans/2026-05-20-v2.0.3-beacon-settings.md` (~57 KB, ~1780 lines, 6 slices + pre-flight, ~1030 LOC). WOW = live Ollama-introspection model dropdown in Slice 4. **Must ship AFTER v2.0.2 — both touch SettingsPanel.svelte + i18n/en.json.**
**v2.0.4 PLAN**: `docs/plans/2026-05-20-v2.0.4-memory.md` (~51 KB, 1482 lines, 6 slices + pre-flight, ~580-720 LOC). WOW = SVG progress rings on Recents grid (Slice 4). Codename "Memory" 📖 — closes the "remember where I left off" table-stakes gap vs Adobe / PDF Expert / Foxit.
**v2.0.5 PLAN**: `docs/plans/2026-05-20-v2.0.5-bookmarks.md` (~62 KB, 1932 lines, 6 slices + pre-flight, ~980 LOC). WOW = drag-to-reorder bookmark slots with HTML5 drag API + accent-tinted drop indicators (Slice 4). Codename "Bookmarks" ★ — closes the "save my spot" companion gap to v2.0.4's "remember last page".
**v2.0.7 PLAN** (new this tick): `docs/plans/2026-05-20-v2.0.7-inkwell.md` (~33 KB, 963 lines, 6 slices + pre-flight, ~1080 LOC, 8 commits). WOW = single-key `d` drawing mode + 220ms ink-trail-shimmer SVG sweep on stroke commit (Slice 4). Codename "Inkwell" 🖊️ — adds freehand ink/drawing as the third annotation primitive, reusing v2.0.6's sidecar JSON store via a new `kind: "ink"` discriminator. Rust gets a third `Annotation::Ink` enum variant emitting PDF spec §12.5.6.13 `/Subtype /Ink` w/ `/InkList`. **Must ship AFTER v2.0.6 — touches every file v2.0.6 touches (annotations.ts, AnnotateLayer.svelte, ReaderPanel.svelte, CommandPalette.svelte, OnboardingTour.svelte, SettingsPanel.svelte, i18n/en.json, src-tauri/src/pdf/annotations.rs, src-tauri/src/keymap/action.rs). Disjoint namespaces (`annotations.draw.*` / `palette.draw.*` / `settings.annotations.draw.*` / `onboarding.draw.*`) + appended-at-end keys keep merge clean; strict order is the insurance policy.**
**v2.0.6 PLAN**: `docs/plans/2026-05-20-v2.0.6-margin.md` (~47 KB, 1191 lines, 6 slices + pre-flight, ~990 LOC, 8 commits). WOW = single-key `h` Quick-Highlight mode with 220ms ink-bloom animation on every new highlight (Slice 4). Codename "Margin" 📝 — **closes the highlights-vanish-on-close launch-blocker gap** vs Adobe / PDF Expert / Foxit / macOS Preview. Persistent annotations sidecar JSON keyed by file path, byte-compatible with existing Rust `Annotation` enum (no backend changes needed for the happy path). **Must ship AFTER v2.0.5 — both touch ReaderPanel.svelte + CommandPalette.svelte + OnboardingTour.svelte + SettingsPanel.svelte + i18n/en.json (disjoint namespaces: annotations.\* / palette.annot.\* / settings.annotations.\* / onboarding.annotations.\* vs bookmarks.\* / palette.bookmark.\* vs reader.progress.\* / palette.resume.\* / onboarding.memory.\* vs settings.beacon.\*).**
**LAST_WOW_TICK_AT**: 2026-05-20 00:20 PT — v2.0.1 itself ships the "0 → 1" plugin moment.

---

## TICK 2026-05-21 01:35 PT — MODE C writing-plans skill — v2.0.12 plan promoted ✒️

Tenth consecutive writing-plans tick. This tick: **v2.0.12 "Sign" ✒️** —
digital signatures + 14 preset stamps. Single biggest paid-tier gap remaining
(Adobe Sign $180/yr, DocuSign Personal $120/yr).

**Plan**: `docs/plans/2026-05-21-v2.0.12-sign.md` (~34 KB, 6 slices +
pre-flight, ~2400 LOC, 8 commits, 58 new tests). WOW = trackpad capture modal
w/ pressure ink + **320ms rubber-stamp drop animation (scale-1.5→1.0 +
rotate-3° + 12 ink-spatter droplets, cubic-bezier `(0.34, 1.56, 0.64, 1)`)**
(Slice 4). Reuses v2.0.6 sidecar JSON, v2.0.7 RDP ink (extracted to shared
`inkCapture.ts`), v2.0.10 edit mode, v2.0.11 rotation + multi-select. Zero
new Tauri commands. Zero new Rust deps. One new frontend dep family
(`@fontsource/{caveat,dancing-script,great-vibes}`, OFL, self-hosted, ~75KB).

**Buy-Button 4/4 PASS**: Pick-us (every paid PDF tool ships sigs), Notice-it
(sidebar panel + `s` shortcut + `Cmd+Shift+S` palette + Settings section +
OnboardingTour step #16), Tell-a-friend (rubber-stamp animation + offline +
keyboard-first), Pay-for-it (replaces $120-180/yr of competing SaaS for
internal sign-offs).

**Cryptographic PKCS#7 signing** explicitly deferred to v2.1.0 "Notary" —
the $49 Pro-tier hook. v2.0.12 ships visual sigs + sidecar `signedBy`
metadata; legally-binding `/Sig` field comes later.

**Commits this tick (2 on main)**:
- `<HASH>` docs(plans): v2.0.12 "Sign" implementation plan ✒️
- `<HASH>` chore(cron): STATE.md + session log — v2.0.12 plan promoted

Session log: `.cron-state/sessions/2026-05-21-0135.md`.

**Scheduling**: do not start v2.0.12 Slice 1 until v2.0.11 ships + tags.
Strict order: v2.0.2 → … → v2.0.11 → v2.0.12.

---

## TICK 2026-05-21 01:07 PT — MODE C writing-plans skill — v2.0.11 plan promoted ⚒️

Ninth consecutive writing-plans tick. This tick: **v2.0.11 "Anvil" ⚒️** —
multi-select + rotate + align. Redeems v2.0.10's two open-question deferrals
(group/lasso multi-select + rotate handle) **and** introduces alignment
guides Acrobat/PDF Expert/Foxit/Bluebeam don't ship.

**Plan**: `docs/plans/2026-05-21-v2.0.11-anvil.md` (~47 KB, 6 slices +
pre-flight, ~2140 LOC, 8 commits, 52 new tests). WOW = animated distribute
(240ms cubic-bezier) + Figma-style live alignment guides + rotate-handle
with live degree readout (Slice 4). Breaking sidecar v1→v2 migration adds
`rotation: number` to every annotation kind; existing files auto-upgrade
on first read with `#[serde(default)]` Rust-side safety net.

**Buy-Button 4/4 PASS**: Pick-us (Acrobat/PDF Expert/Foxit/Bluebeam ship
multi-select+rotate+align), Notice-it (marquee + handle + toolbar +
Settings + palette + step #15), Tell-a-friend (live alignment guides
nobody else ships + animated distribute), Pay-for-it (Bluebeam $349/yr
partly justified by batch alignment).

**Commits this tick (2 on main)**:
- `<HASH>` docs(plans): v2.0.11 "Anvil" implementation plan ⚒️
- `<HASH>` chore(cron): STATE.md + session log — v2.0.11 plan promoted

Session log: `.cron-state/sessions/2026-05-21-0107.md` (full breakdown).

**Scheduling**: do not start v2.0.11 Slice 1 until v2.0.10 ships + tags.
Strict order: v2.0.2 → … → v2.0.10 → v2.0.11.

---

## TICK 2026-05-21 00:42 PT — MODE C writing-plans skill — v2.0.10 plan promoted 🔧

User invoked the `writing-plans` skill an **eighth** consecutive tick. This
tick: **v2.0.10 "Lathe" 🔧** — annotation edit mode (selection, drag,
resize, undo/redo, copy-paste). Redeems FIVE deferred items stacked across
v2.0.6–v2.0.9 open-questions blocks in a single release.

**Why v2.0.10 next?**
1. STATE.md line 11 already pre-claimed the codename "Lathe" for the
   release after v2.0.9. Plan redeems that pledge.
2. After v2.0.9 ships, Slab has 5 annotation primitives but ZERO
   editability. That's a launch-blocker for any reviewer who tries the
   markup toolkit for >30 seconds.
3. Buy-Button 4/4 PASS: Acrobat / PDF Expert / Foxit / Drawboard all ship
   select+move+resize+undo as table-stakes.
4. Wow is triple-stacked: Figma-style snap-guides + 220ms dissolve/
   restore animation + cross-document copy-paste. None of the paid
   competitors ship snap-guides on annotation move.

**Commits this tick (2 on main):**
- `<HASH>` docs(plans): v2.0.10 "Lathe" implementation plan 🔧
- `<HASH>` chore(cron): STATE.md + session log — v2.0.10 plan promoted
- Plan file: `docs/plans/2026-05-21-v2.0.10-lathe.md` (~51 KB).

**Plan structure — 6 slices + pre-flight, ~2200 LOC, 8 commits, 66 new tests:**
- **Slice 0** (pre-flight): 9 verification checks — v2.0.9 tag exists,
  all 8 annotation kinds present in TS + Rust, annotations.ts exposes
  update/remove helpers, edit i18n namespace unclaimed, 8 new ActionId
  variants free, no existing history.ts module.
- **Slice 1**: pure-TS `selection.ts` + `history.ts` (~380 LOC + 320 LOC
  tests, 23 vitest cases). Per-document command-stack undo at 200-deep
  cap, branch-rewrite semantics, reentrancy-safe.
- **Slice 2**: `SelectionLayer.svelte` w/ 8-handle marching-ants frame
  (~440 LOC + tests). SVG `stroke-dashoffset` animation, respects
  `prefers-reduced-motion`. Single ring for point annotations (notes),
  2 endpoint handles for lines/arrows.
- **Slice 3**: drag-to-move + drag-to-resize w/ snap guides (~600 LOC +
  12 geometry tests). Live preview via CSS `transform` (zero pdfjs
  re-renders mid-drag). Snap-to-edge at 4px threshold.
- **Slice 4 ⭐ WOW**: keyboard editor (~430 LOC). 8 new ActionId variants
  — j/k cycle, Delete dissolve (220ms scale+fade), Cmd+Z restore,
  Cmd+C/V cross-document copy-paste w/ custom MIME
  `application/x-slab-annot-v1+json`. Mod+D collision with v2.0.5
  bookmark resolved via context guard.
- **Slice 5**: Settings → Annotations → Edit subsection (6 knobs) +
  palette "Annotations · Edit" group (8 entries w/ peek-undo labels) +
  22 new i18n keys all under disjoint namespaces.
- **Slice 6**: OnboardingTour step #14, version bumps (2.0.9 → 2.0.10
  in three files), CHANGELOG entry, customer-facing release notes at
  `docs/release-notes/v2.0.10.md`, MODE A merge + tag + push, queue
  `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level: 4/4 PASS** — Pick-us (table-
stakes editability), Notice-it (handles + dissolve + cycle), Tell-a-
friend (Figma-style snap-guides + keyboard-first + cross-doc paste),
Pay-for-it (Acrobat $239/yr partly justified by editable markup).

**Codebase-discovery decisions baked in:**
- Selection store is in-memory only — matches every editor; not
  persisted across opens.
- History is per-path command-stack (do/undo pairs), not snapshot diff
  — O(1) per mutation regardless of doc size.
- Drag preview via CSS `transform` avoids pdfjs canvas re-rasterization.
- Clipboard custom MIME w/ plain-text fallback handles Linux/Wayland.
- ESC during drag cancels, restores transform identity, no
  `history.apply`.
- Cross-document paste assigns a new ID + drops onto current page.
- Mod+D collision w/ v2.0.5 Bookmark resolved by context guard
  (selection!=null → AnnotDuplicate; else → BookmarksToggle).

**Comparison table baked in** vs Adobe / PDF Expert / Foxit on 12 markup
capabilities. After v2.0.10 lands, Slab wins on snap-guides, keyboard-
first editing, sidecar JSON, and free/offline/no-telemetry — that's
the **v1.0.0 launch narrative**.

**Scheduling note (critical, documented in plan):**
Do not start v2.0.10 Slice 1 until v2.0.9 ships and is tagged. Strict
order: v2.0.2 → v2.0.3 → v2.0.4 → v2.0.5 → v2.0.6 → v2.0.7 → v2.0.8 →
v2.0.9 → v2.0.10.

**Open questions:**
- Group/lasso multi-select → v2.0.11
- Rotate handle → v2.0.11 (needs `rotation` field on ShapeAnnotation —
  breaking sidecar change)
- Cross-page drag → deferred (use copy-paste instead)
- Touch gestures (pinch-resize) → deferred until Tauri mobile lands
- **Nine-plan backlog now on disk** — ~9 weeks of execution ticks
  front-loaded. After v2.0.10 the cron can pivot from "annotation
  primitives" to other v1.0.0 gaps (Library, OCR, visual diff,
  presenter mode, command-palette polish).

**Next ticks (in order):**
1. ~~v2.0.2~~ — **SHIPPED by Sanjay directly** (commit `168e638`).
2. **v2.0.3 Slices 1-6** on `feature/v2.0.3-beacon-settings`.
3. v2.0.3 MODE A merge + tag + release.
4. **v2.0.4 Slices 1-6** on `feature/v2.0.4-memory`.
5. v2.0.4 MODE A.
6. **v2.0.5 Slices 1-6** on `feature/v2.0.5-bookmarks`.
7. v2.0.5 MODE A.
8. **v2.0.6 Slices 1-6** on `feature/v2.0.6-margin`.
9. v2.0.6 MODE A.
10. **v2.0.7 Slices 1-6** on `feature/v2.0.7-inkwell`.
11. v2.0.7 MODE A.
12. **v2.0.8 Slices 1-6** on `feature/v2.0.8-compose`.
13. v2.0.8 MODE A.
14. **v2.0.9 Slices 1-6** on `feature/v2.0.9-typeset`.
15. v2.0.9 MODE A.
16. **v2.0.10 Slices 1-6** on `feature/v2.0.10-lathe`. WOW = keyboard
    editor + snap-guides + cross-doc paste.
17. v2.0.10 MODE A.

---

## TICK 2026-05-20 23:?? PT — MODE C writing-plans skill — v2.0.8 plan promoted 🔷

User invoked the `writing-plans` skill a **seventh** consecutive tick. Pattern
is now reliable: each writing-plans tick promotes the next release plan in the
pipeline. v2.0.2–v2.0.7 all on disk; this tick: **v2.0.8 "Compose" 🔷** —
four shape primitives (rectangle, ellipse, arrow, line), redeeming the exact
item v2.0.7's plan flagged as "defer to v2.0.8."

**Also noticed this tick:** Sanjay personally merged the v2.0.2 marketplace
work to main (commit `168e638`, tag `v2.0.2` already pushed). The cron's
MODE A merge-and-release sequence is therefore no longer needed for v2.0.2.
Updated the STATUS line + Next-ticks list accordingly. The pipeline picks
up cleanly at v2.0.3.

**Why v2.0.8 next?**
1. v2.0.7's open-questions block explicitly named shape primitives as the
   v2.0.8 candidate. Redeeming that pledge keeps the planning thread honest.
2. v2.0.7's `DrawLayer.svelte` + sidecar `annotations.ts` discriminated
   union + Rust `Annotation` tagged enum are all freshly extended — v2.0.8
   adds four sibling variants to each, ~25-30% cheaper now than after they
   cool.
3. Reviewer expectation: Acrobat, PDF Expert, Foxit, Drawboard all ship
   the rect/ellipse/arrow/line quartet as table-stakes markup. Slab today
   ships zero. Closes that gap entirely in one release.
4. Three-tier rotation snap (free / Shift 15° / Cmd 45°) with a real
   on-screen dial is a *differentiator* Acrobat doesn't have — turns a
   table-stakes feature into a tell-a-friend moment.

**Commits this tick (2 on main):**
- `<HASH>` docs(plans): v2.0.8 "Compose" implementation plan 🔷
- `<HASH>` chore(cron): STATE.md + session log — v2.0.8 plan promoted
- Plan file: `docs/plans/2026-05-20-v2.0.8-compose.md` (~41 KB).

**Plan structure — 6 slices + pre-flight, ~1880 LOC, 8 commits:**
- **Slice 0** (pre-flight): 7 verification checks — v2.0.7 tag exists,
  annotations.ts has highlight/note/ink, Rust enum tagged-union with all
  three, `ActionId::AnnotDraw` sibling present, shape namespaces unclaimed
  in i18n, single-letter `r`/`e`/`a`/`l` unclaimed in vim keymap, lopdf
  still 0.32.
- **Slice 1**: pure-TS `annotations.ts` extension (~250 LOC + 180 LOC
  vitest, 12 cases) — `RectAnnotation` / `EllipseAnnotation` /
  `ArrowAnnotation` / `LineAnnotation` variants, `add*Annotation` helpers,
  6-digit hex enforcement, width clamping (0.5..12 PDF-pt), zero-length
  arrow rejection.
- **Slice 2**: Rust `Annotation::{Rect,Ellipse,Arrow,Line}` variants +
  `build_annotation_dict` arms emitting ISO 32000-1 `/Square`, `/Circle`,
  `/Line` annotation dicts w/ correct `/Rect`, `/L`, `/LE`, `/BS`, `/C`,
  `/CA`, `/F` fields. +4 unit tests.
- **Slice 3**: `DrawLayer.svelte` shape sub-modes (~290 LOC) — pointer
  drag w/ rubber-band SVG preview, Shift aspect-ratio constraint for
  rect/ellipse (squares/circles), 45° axis-snap for arrow/line, Esc
  cancels. Persistent overlay paint loop in ReaderPanel via
  `pagerendered`.
- **Slice 4 ⭐ WOW**: Single-key `r`/`e`/`a`/`l` modes + real
  `ActionId::AnnotShape(ShapeKind)` for keymap-remap survivability +
  **15° fine rotation snap with on-screen accent-tinted dial** (24-tick
  ring + angle label chip) when Shift is held during arrow drag + 220ms
  cubic-bezier shape-pop animation on commit. Respects
  `prefers-reduced-motion`.
- **Slice 5**: Settings → Annotations → Shapes subsection (color, width,
  opacity, arrow head style, line dashed default, snap-15° toggle) +
  Command-Palette "Annotations · Shapes" group (5 entries gated on
  `readerCtx`) + 22 new i18n keys.
- **Slice 6**: OnboardingTour step #12, version bumps (2.0.7 → 2.0.8 in
  three files), CHANGELOG entry, customer-facing release notes at
  `docs/release-notes/v2.0.8.md`, MODE A merge + tag + push, queue
  `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Adobe/PDF Expert/Foxit/Drawboard all ship the
  rect/ellipse/arrow/line quartet. v1.0.0 launch-blocker.
- **Notice-it PASS** — sub-toolbar, 4 vim shortcuts, Settings subsection,
  palette group, OnboardingTour step #12.
- **Tell-a-friend PASS** — single-key shape mode + 15° rotation dial in
  a free, offline reader. Architecture-diagram-on-academic-paper
  screenshot bait.
- **Pay-for-it PASS** — Acrobat charges $239/yr partly for the Comment
  toolset (square/circle/line).

**Codebase-discovery decisions baked in:**
- `Annotation` enum tagged-union pattern (`serde(tag = "kind",
  rename_all = "snake_case")`) — adding four variants is a pure
  extension; `slab_append_annotations` Tauri command signature
  unchanged.
- `build_annotation_dict()` arms for Square/Circle/Line follow
  ISO 32000-1 §12.5.6.10 + §12.5.6.7 — Adobe-compatible burn output
  guaranteed.
- `DrawLayer.svelte` mode union extends from
  `"off"|"highlight"|"note"|"ink"` to add four shape modes.
- pdfjs `eventBus.pagerendered` is the right hook for persistent
  shape overlay paint (same as v2.0.6 highlight + v2.0.7 ink overlays).
- Vim-style single-letter `r`/`e`/`a`/`l` mirrors v2.0.6's `h` and
  v2.0.7's `d` pattern. All four free per pre-flight check #6.
- Onboarding step math: …10 → 11 → **12** (v2.0.8).
- i18n namespaces `annotations.shape.*` / `palette.shape.*` /
  `settings.annotations.shape.*` / `onboarding.shape.*` all greenfield.

**Scheduling note (critical, documented in plan):**
**Do not start v2.0.8 Slice 1 until v2.0.7 ships and is tagged.**
v2.0.8 touches every file v2.0.7 touches. Strict order:
v2.0.3 → v2.0.4 → v2.0.5 → v2.0.6 → v2.0.7 → v2.0.8 (v2.0.2 already
shipped this evening directly by Sanjay).

**Next ticks (in order):**
1. ~~v2.0.2~~ — **SHIPPED by Sanjay directly** (commit `168e638`).
2. **v2.0.3 Slices 1-6** on `feature/v2.0.3-beacon-settings`. WOW =
   live Ollama-introspection model dropdown (Slice 4).
3. **v2.0.3 MODE A merge + tag + release**.
4. **v2.0.4 Slices 1-6** on `feature/v2.0.4-memory`. WOW = SVG
   progress rings on Recents grid (Slice 4).
5. **v2.0.4 MODE A merge + tag + release**.
6. **v2.0.5 Slices 1-6** on `feature/v2.0.5-bookmarks`. WOW = drag-to-
   reorder (Slice 4).
7. **v2.0.5 MODE A merge + tag + release**.
8. **v2.0.6 Slices 1-6** on `feature/v2.0.6-margin`. WOW = ink-bloom +
   `h` mode (Slice 4).
9. **v2.0.6 MODE A merge + tag + release**.
10. **v2.0.7 Slices 1-6** on `feature/v2.0.7-inkwell`. WOW = ink-trail-
    shimmer + single-key `d` (Slice 4).
11. **v2.0.7 MODE A merge + tag + release**.
12. **v2.0.8 Slices 1-6** on `feature/v2.0.8-compose`. WOW = 15° dial +
    shape-pop (Slice 4).
13. **v2.0.8 MODE A merge + tag + release**.

**Open questions for Sanjay:**
- Layered drawing groups (multi-shape select/move/resize as a unit)
  for v2.0.9? Plan defers (~400 LOC).
- Per-shape default color override (sticky per-kind), or one shared
  default across all four? Plan ships shared default.
- Free text annotation (free-floating text box, distinct from v2.0.6
  sticky note) for v2.0.9 or v2.0.10's "Lathe" edit-mode pipeline?
  Plan defers to v2.0.10.
- **Seven-plan backlog now on disk** — ~8 weeks of execution ticks
  front-loaded. With Sanjay shipping v2.0.2 directly this evening, is
  the next tick the right moment for cron to flip from "planning" to
  "executing" (subagent against v2.0.3 Slice 0)?

---

## TICK 2026-05-20 22:33 PT — MODE C writing-plans skill — v2.0.7 plan promoted 🖊️

User invoked the `writing-plans` skill a **sixth** consecutive tick.
Pattern is now firm: each writing-plans tick promotes the next release
plan in the pipeline. v2.0.2–v2.0.6 all on disk; this tick:
**v2.0.7 "Inkwell" 🖊️** — freehand ink/drawing as the third annotation
primitive, the exact item v2.0.6's plan flagged as "defer to v2.0.7".

**Why v2.0.7 next?**
1. v2.0.6's open-questions block explicitly named drawings as
   v2.0.7. Redeeming that pledge.
2. v2.0.6's sidecar JSON store (`annotations.ts`) + `AnnotateLayer.svelte`
   mode union + Rust `Annotation` tagged enum are all freshly extended —
   v2.0.7 adds exactly one new variant in each, ~30% cheaper now than
   after they cool.
3. Reviewer expectation: Acrobat/PDF Expert/Foxit/Preview all ship
   freehand ink. Apple-Pencil-on-iPad demo failing on Slab today is a
   v1.0.0 launch-blocker.

**Commits this tick (2 on main):**
- `<HASH>` docs(plans): v2.0.7 "Inkwell" implementation plan 🖊️
- `<HASH>` chore(cron): STATE.md + session log — v2.0.7 plan promoted
- Plan file: `docs/plans/2026-05-20-v2.0.7-inkwell.md` (~33 KB, 963 lines).

**Plan structure — 6 slices + pre-flight, ~1080 LOC, 8 commits:**
- **Slice 0** (pre-flight): 5 checks — v2.0.6 tag exists, annotations.ts
  shape intact, Rust `Annotation` enum still tagged-union, `ActionId::
  AnnotHighlight`+`AnnotNote` siblings present, draw-namespace i18n
  unclaimed.
- **Slice 1**: pure-TS `annotations.ts` extension (~250 LOC + 150 LOC
  vitest, 11 tests) — `InkAnnotation` variant, Ramer-Douglas-Peucker
  simplifier (ε≈0.5px), CSS→PDF batch convert, `addInkAnnotation()`
  with width clamping.
- **Slice 2**: Rust `Annotation::Ink` variant + `build_annotation_dict`
  arm emitting `/Subtype /Ink` with `/InkList`, bounding `/Rect` w/
  width/2+1 pad, `/BS` border style. +2 unit tests.
- **Slice 3**: `DrawLayer.svelte` (~280 LOC) — pointer-event capture
  with Apple-Pencil pressure/tilt, RDP on pointer-up, render-layer
  integration in ReaderPanel paints persisted strokes on
  `pagerendered`. Eraser sub-mode with per-segment hit-test.
- **Slice 4 ⭐ WOW**: Single-key `d` mode (~140 LOC). 220ms
  ink-trail-shimmer via SVG `<linearGradient>` sweep masking a
  re-traced stroke. Respects `prefers-reduced-motion`. Real
  `ActionId::AnnotDraw` (Mod+I) for keymap-remap survivability.
- **Slice 5**: Settings → Annotations → Drawings subsection
  (color/width/eraser/opacity/pencil-pressure/burn-width) +
  Command-Palette "Annotations · Drawing" group (5 entries gated on
  readerCtx) + 18 new i18n keys.
- **Slice 6**: OnboardingTour step #11, version bumps (2.0.6 → 2.0.7
  in three files), CHANGELOG entry, customer-facing release notes at
  `docs/release-notes/v2.0.7.md`, MODE A merge + tag + push, queue
  `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Acrobat/PDF Expert/Foxit/Preview all ship ink.
  Launch-blocker for v1.0.0.
- **Notice-it PASS** — Draw toolbar button + `d` shortcut + Settings →
  Drawings + palette group + OnboardingTour step #11.
- **Tell-a-friend PASS** — single-key `d` + 220ms ink-trail-shimmer +
  Apple-Pencil tilt in a free, offline reader is a true screenshot
  moment.
- **Pay-for-it PASS** — Adobe Comment tier (with Pencil) is $239/yr.

**Codebase-discovery decisions baked in:**
- `Annotation` Rust enum tagged-union confirmed at line 27-29 of
  `src-tauri/src/pdf/annotations.rs`. Adding `Ink` is a pure variant
  extension; `slab_append_annotations` Tauri command signature
  unchanged.
- `build_annotation_dict()` is a single match — new arm slots in next
  to Highlight + Note.
- `AnnotateLayer.svelte` mode union `"off" | "highlight" | "note"`
  extends to `"draw"` (line 19).
- pdfjs `eventBus.pagerendered` again the right hook for persisted-ink
  overlay (same as v2.0.6 highlight overlay, ReaderPanel.svelte:677-688).
- Vim-style single-letter `d` mirrors v2.0.6's `h` and existing
  `src/lib/vim/keymap.ts` pattern.
- Onboarding step math: 6 → 7 → 8 → 9 → 10 → **11** (v2.0.7).
- i18n namespaces `annotations.draw.*` / `palette.draw.*` /
  `settings.annotations.draw.*` / `onboarding.draw.*` all greenfield.

**Scheduling note (critical, documented in plan):**
**Do not start v2.0.7 Slice 1 until v2.0.6 ships and is tagged.**
v2.0.7 touches every file v2.0.6 touches. Strict order:
v2.0.2 → v2.0.3 → v2.0.4 → v2.0.5 → v2.0.6 → v2.0.7, each cut from
`main` after the previous merges.

**Next ticks (in order):**
1. **v2.0.2 Slices 1-8** on `feature/v2.0.2-workshop-marketplace`.
2. **v2.0.2 MODE A merge + tag + release**.
3. **v2.0.3 Slices 1-6** on `feature/v2.0.3-beacon-settings`.
4. **v2.0.3 MODE A merge + tag + release**.
5. **v2.0.4 Slices 1-6** on `feature/v2.0.4-memory`.
6. **v2.0.4 MODE A merge + tag + release**.
7. **v2.0.5 Slices 1-6** on `feature/v2.0.5-bookmarks`.
8. **v2.0.5 MODE A merge + tag + release**.
9. **v2.0.6 Slices 1-6** on `feature/v2.0.6-margin`.
10. **v2.0.6 MODE A merge + tag + release**.
11. **v2.0.7 Slices 1-6** on `feature/v2.0.7-inkwell`. WOW = ink-trail-shimmer + single-key `d` (Slice 4).
12. **v2.0.7 MODE A merge + tag + release**.

**Open questions for Sanjay:**
- Shape primitives (rect/ellipse/arrow) in v2.0.7 or defer to v2.0.8?
  Plan defers (~250 LOC).
- Apple-Pencil pressure default on? Plan says on.
- Ink smoothing (Catmull-Rom)? Plan defers — RDP + SVG line-caps
  already look smooth enough.
- Layered drawing groups (multi-stroke "drawings")? Plan defers to
  v2.0.8.
- **Six-plan backlog now on disk** — ~7 weeks of execution ticks
  front-loaded. At what point does Sanjay want the next tick to
  switch from "planning" to "executing" (subagent against v2.0.2
  Slice 0)?

---

## TICK 2026-05-20 19:16 PT — MODE C writing-plans skill — v2.0.6 plan promoted 📝

User invoked the `writing-plans` skill a **fifth** consecutive tick.
Pattern firmly established: each writing-plans tick promotes the next
release plan in the pipeline. v2.0.2-v2.0.5 already on disk; this tick:
**v2.0.6 "Margin" 📝** — persistent annotations sidecar JSON.

**Why v2.0.6 next? (Real launch-blocker.)**
1. Adobe / PDF Expert / Foxit / Preview ALL persist annotations. Slab
   today loses every highlight the moment the doc closes (unless user
   clicks "Save to new PDF" which *destructively rewrites the file*).
   A reviewer who tries v2.0.5 expecting paid-product parity will close
   the app the moment their first highlight disappears.
2. v2.0.6's `annotations.ts` store mirrors v2.0.4's `readingPosition.ts`
   and v2.0.5's `bookmarks.ts` line-for-line — writing it now while
   those patterns are fresh in context is ~30% cheaper than later.
3. Existing Rust surface is already there: `slab_append_annotations`
   accepts the exact serde shape (`tag = "kind"`, snake_case Highlight |
   Note) the new TS store produces. **No new Rust modules for happy
   path** — only 2 ActionId variants (Mod+H, Mod+Shift+H).
4. Introduces the "persistent sidecar JSON keyed by file path" pattern
   v2.0.7 (drawings), v2.0.8 (comment threading), v2.0.9 (reading speed)
   can all reuse.

**Commits this tick (2 on main):**
- `<HASH>` docs(plans): v2.0.6 "Margin" implementation plan
- `<HASH>` chore(cron): STATE.md + session log — v2.0.6 plan promoted
- Plan file: `docs/plans/2026-05-20-v2.0.6-margin.md` (~47 KB, 1191 lines).

**Plan structure — 6 slices + pre-flight, ~990 LOC, 8 commits:**
- **Slice 0** (pre-flight): 5 verification checks — `Annotation` Rust enum
  still tagged-union w/ exactly Highlight+Note, `PendingAnnotation`
  shape unchanged, `ActionId` unions (TS + Rust) missing the two new
  variants, no existing `annotations.*` i18n keys.
- **Slice 1**: pure-TS `src/lib/annotations.ts` (~330 LOC + ~150 LOC
  vitest, 12 tests enumerated) — per-path localStorage bucket under
  `slab.annotations.v1`, 200/doc cap + 3000 total cap, oldest-first
  eviction. Module mirrors `bookmarks.ts` line-for-line.
- **Slice 2**: Rewire `AnnotateLayer.svelte` to `subscribeAnnotations(path)`;
  drop `pending[]` + Save button. Register `ActionId::AnnotHighlight`
  (Mod+H) + `ActionId::AnnotNote` (Mod+Shift+H) in Rust enum + ACTIONS
  table + TS union end-to-end. New sidebar header pills: "🖨 Burn into
  PDF" + "📋 Markdown".
- **Slice 3**: Persistent overlay render layer in `ReaderPanel.svelte`
  (~220 LOC) — listens to `eventBus.pagerendered`, paints accent-tinted
  highlight quads + yellow `■` note pins. `pointer-events: none` on
  overlay, `auto` on children. Re-renders on store events too.
- **Slice 4 ⭐ WOW**: Single-key `h` Quick-Highlight (~140 LOC).
  Press `h` (no modifier, vim-style) → highlight mode. Pulsing "✎
  Highlight" pill top-right. **220ms ink-bloom** on every new highlight.
  Screenshot moment.
- **Slice 5**: Settings → Annotations section (color, author, clear
  per-doc / clear all) + Command-Palette "Annotations" group (5 entries
  gated on `readerCtx`) + 14 new i18n keys.
- **Slice 6**: OnboardingTour step #10 ("📝 Annotations that stay put"),
  version bumps (2.0.5 → 2.0.6 in three files), CHANGELOG entry,
  customer-facing release notes at `docs/release-notes/v2.0.6.md`, MODE
  A merge + tag + push, queue `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Adobe / PDF Expert / Foxit / Preview all persist
  annotations. Real launch-blocker.
- **Notice-it PASS** — every returning user sees: (a) highlights survive
  across opens, (b) Mod+H + `h` shortcuts, (c) Annotations Settings
  section, (d) Annotations palette group, (e) onboarding step #10,
  (f) ink-bloom on first highlight.
- **Tell-a-friend PASS** — single-key `h` + 220ms ink-bloom is a true
  screenshot moment in a free PDF reader.
- **Pay-for-it PASS** — Adobe Acrobat charges $239/yr partly because it
  persists annotations. v2.0.6 gives it away.

**Codebase-discovery decisions baked into the plan:**
- `Annotation` Rust enum is **already** tagged-union (`serde(tag =
  "kind", rename_all = "snake_case")`) — TS store's on-disk shape is
  byte-compatible with what `slab_append_annotations` accepts. Burn
  works with zero backend changes.
- `AnnotateLayer.svelte` already does the hard quad-math via
  `pv.viewport.convertToPdfPoint()`. Slice 2 only changes the *commit
  step* from `pending = [...pending, ...]` to `addAnnotation(path, ...)`.
- pdfjs `eventBus.pagerendered` is the right hook (fires on initial
  render AND zoom changes). Confirmed at line 677–688 of ReaderPanel.
- Vim-style single-letter `h` mirrors `src/lib/vim/keymap.ts` pattern.
- Onboarding step math: 6 today → 7 (v2.0.3) → 8 (v2.0.4) → 9 (v2.0.5)
  → 10 (v2.0.6). Strict-order requirement.
- i18n namespace `annotations.*` / `settings.annotations.*` /
  `palette.annot.*` / `onboarding.annotations.*` all greenfield. Zero
  collision with v2.0.2-2.0.5.

**Scheduling note (critical, documented in the plan):**
**Do not start v2.0.6 Slice 1 until v2.0.5 ships and is tagged.** v2.0.6
touches `ReaderPanel.svelte`, `CommandPalette.svelte`, `OnboardingTour.svelte`,
`SettingsPanel.svelte`, `i18n/en.json` — every one of those is touched
by an earlier-queued plan. Disjoint surface areas + appended-at-end i18n
keys keep three-way merge clean, but strict ordering is the insurance
policy. Strict order: v2.0.2 → v2.0.3 → v2.0.4 → v2.0.5 → v2.0.6.

**Next ticks (in order):**
1. **v2.0.2 Slices 1-8** on `feature/v2.0.2-workshop-marketplace`. WOW = Fuse.js search w/ live highlighting (Slice 6).
2. **v2.0.2 MODE A merge + tag + release**.
3. **v2.0.3 Slices 1-6** on `feature/v2.0.3-beacon-settings`. WOW = live Ollama model dropdown (Slice 4).
4. **v2.0.3 MODE A merge + tag + release**.
5. **v2.0.4 Slices 1-6** on `feature/v2.0.4-memory`. WOW = SVG progress rings (Slice 4).
6. **v2.0.4 MODE A merge + tag + release**.
7. **v2.0.5 Slices 1-6** on `feature/v2.0.5-bookmarks`. WOW = drag-to-reorder (Slice 4).
8. **v2.0.5 MODE A merge + tag + release**.
9. **v2.0.6 Slices 1-6** on `feature/v2.0.6-margin`. WOW = ink-bloom + `h` mode (Slice 4).
10. **v2.0.6 MODE A merge + tag + release**.

**Open questions for Sanjay (only if he wants to weigh in):**
- Drawing annotations (ink / shapes) in v2.0.6 or defer to v2.0.7? Plan
  defers — adds ~400 LOC and a new Rust enum variant.
- Should "Burn into PDF" delete in-Slab annotations? Plan defaults to
  **keep** (burned PDF is a shareable static copy; editable copy stays
  in Slab).
- Per-doc default color override? Plan defers to v2.0.7.
- **Five-plan backlog now on disk.** At what point does Sanjay want the
  next tick to switch from "planning" to "executing" (start a subagent
  against v2.0.2 Slice 1)? Cron has front-loaded ~6 weeks of execution
  ticks.

---

## TICK 2026-05-20 07:23 PT — MODE C writing-plans skill — v2.0.5 plan promoted ★

User invoked the `writing-plans` skill a **fourth** consecutive tick.
Pattern firmly established: each writing-plans tick promotes the next
release in the pipeline. v2.0.2, v2.0.3, v2.0.4 all already on disk;
this tick: **v2.0.5 "Bookmarks" ★** — companion feature to v2.0.4's
"Memory" — Slab gains real per-document user bookmarks.

**Why v2.0.5 next?**
1. v2.0.4 plan flagged this explicitly: "Bundle per-page bookmarks
   into v2.0.4 or defer to v2.0.5? Plan defaults to deferring
   (separate `bookmarks.v1` store, ~150 LOC, out of scope for
   'Memory')." That is now redeemed as a real plan.
2. Buy-Button verdict **passes 3/4 tests** — Adobe, PDF Expert,
   Foxit all ship per-page user bookmarks. Slab today maps Cmd+D to
   *nothing* in the Reader. That is launch-blocker territory for any
   reviewer who tries v2.0 expecting parity with paid products.
3. Surfaces are designed to be **as DRY as possible against v2.0.4**:
   `bookmarks.ts` deliberately mirrors `readingPosition.ts` line for
   line (same eviction policy, same retry-on-quota, same listener
   pattern). A reader who's read v2.0.4 can read this plan in half
   the time.
4. Disjoint i18n namespaces (`bookmarks.*`, `palette.bookmark.*`)
   ensure no JSON merge conflict with v2.0.4's
   `reader.progress.*` / `palette.resume.*` / `onboarding.memory.*`,
   provided the strict-order pipeline holds (see scheduling note).

**Commit this tick (1 on main):**
- `<HASH>` docs(plans): v2.0.5 "Bookmarks" implementation plan
- Plan file: `docs/plans/2026-05-20-v2.0.5-bookmarks.md` (~62 KB, 1932 lines).

**Plan structure — 6 slices + pre-flight, ~980 LOC, 6 commits:**
- **Slice 0** (pre-flight): 5 verification checks — bookmark namespace
  unclaimed, Mod+D unclaimed, sidebar toggle pattern intact,
  pdfjs thumbnail surface available, no surface collision with
  v2.0.2/v2.0.3/v2.0.4.
- **Slice 1**: pure-TS `src/lib/bookmarks.ts` (~250 LOC incl. header)
  — per-path localStorage store under `slab.bookmarks.v1`, 100
  bookmarks/doc cap, 2_000 total cap with oldest-doc-first eviction
  on quota crash. Idempotent addBookmark, toggle/rename/reorder/
  subscribe round out the public API. Mirrors readingPosition.ts
  shape line-for-line (deliberate DRY against v2.0.4).
- **Slice 2**: Mod+D keymap action (real Rust ActionId variant +
  ACTIONS row, NOT a hardcoded handler — survives Settings remap)
  + toolbar ★/☆ button + `⊕` sidebar toggle in new tb-group between
  page-nav and zoom. Toast confirmation via notify.ts.
- **Slice 3**: Right-rail 280px frosted-glass bookmarks sidebar with
  56x72 pdfjs-rendered thumbnails (lazy + cached per session),
  inline-editable name (click to rename, Enter to save, Esc to
  cancel), one-click jump, hover-reveal delete, accent-tinted
  border on the current page's slot.
- **Slice 4 ⭐ WOW**: Drag-to-reorder via HTML5 drag API. Pointer-Y
  relative drop indicators (accent line above OR below the target
  slot) make insertion unambiguous. 220ms ease-out slot animation.
  `renamingId == null` gates `draggable={}` so rename input clicks
  don't start a drag. Subtle ↕ hint chip appears in the header
  once there are 2+ bookmarks.
- **Slice 5**: Three command-palette entries under a new
  "Bookmarks" group — "Bookmark this page" (toggle, label flips),
  "Jump to: <name>" (one per bookmark, top 12), all gated on
  `readerCtx` (new Props field passing the active Reader tab's
  path/page). Includes a static subscription so palette listings
  stay live as the user toggles via Cmd+D elsewhere.
- **Slice 6**: onboarding tour step #5 (★ Bookmarks), version bumps
  (2.0.4 → 2.0.5 in package.json/Cargo.toml/tauri.conf.json),
  CHANGELOG entry, customer-facing release notes at
  `docs/release-notes/v2.0.5.md`, MODE A merge + tag + push, queue
  `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Adobe / PDF Expert / Foxit all ship per-page
  user bookmarks. Closing this gap is a hard prerequisite for the
  v1.0.0 launch narrative.
- **Notice-it PASS** — every returning user sees: (a) new ★/☆
  toolbar toggle, (b) new ⊕ sidebar toggle, (c) new "Bookmarks"
  group in Cmd+K palette, (d) new ★ onboarding-tour step.
- **Tell-a-friend PASS** — drag-to-reorder bookmark slots with
  thumbnails and inline rename in a free PDF reader is a genuine
  screenshot moment. "Try doing this in Adobe Acrobat (you can't
  even drag in their bookmarks panel — right-click → Move →
  destination → OK)."

**Codebase-discovery decisions baked into the plan:**
- `Mod+D` is registered as a real `ActionId::BookmarksToggle`
  variant (not hardcoded) so it survives Settings remap. Verified
  not already in use across `keymap/action.rs`, `keymap.ts`, or
  `ReaderPanel.svelte`.
- Sidebar markup follows the existing
  `<aside class="outline-sidebar">` / `class="thumbs"` / `class="info-sidebar"`
  pattern, sitting alongside them in the same flex row. New CSS
  block at end of `<style>`.
- pdfjs thumbnail render piggybacks on whatever `getPage().render`
  helper the existing thumbs sidebar already uses (DRY — bookmarks
  must not ship a parallel thumbnail pipeline). Slice 3 documents
  the fallback (inline a small `renderThumb` helper) if no shared
  helper exists.
- `bookmarks.ts` deliberately mirrors `readingPosition.ts` API
  shape (same listener pattern, same eviction strategy, same
  retry-on-quota). A future v2.0.6 could merge them into a single
  `perDocStore<T>()` factory if pattern stabilises.
- Dynamic `import()` for `renameBookmark` + `removeBookmarkById` +
  `reorderBookmarks` in the Reader sidebar — these are rare paths;
  the bundler can defer them. (If tree-shaking is already good,
  these can be folded into the Slice 2 static import block — call
  flagged in the plan as an "if".)

**Scheduling note (critical, documented in the plan):**
**Do not start v2.0.5 Slice 1 until v2.0.4 ships and is tagged.**
Both versions touch `ReaderPanel.svelte`, `CommandPalette.svelte`,
`OnboardingTour.svelte`, and `i18n/en.json` (disjoint namespaces
but git's textual three-way merge doesn't know JSON structure).
Strict order: v2.0.2 → v2.0.3 → v2.0.4 → v2.0.5, each cut from
`main` after the previous merges.

**Next ticks (in order):**
1. **v2.0.2 Slices 1-8** on `feature/v2.0.2-workshop-marketplace`. WOW = Fuse.js search w/ live highlighting (Slice 6).
2. **v2.0.2 MODE A merge + tag + release**.
3. **v2.0.3 Slices 1-6** on `feature/v2.0.3-beacon-settings`.
4. **v2.0.3 MODE A merge + tag + release**.
5. **v2.0.4 Slices 1-6** on `feature/v2.0.4-memory`.
6. **v2.0.4 MODE A merge + tag + release**.
7. **v2.0.5 Slices 1-6** on `feature/v2.0.5-bookmarks`. WOW = drag-to-reorder (Slice 4).
8. **v2.0.5 MODE A merge + tag + release**.

**Open questions for Sanjay (only if he wants to weigh in):**
- Pinboard view (all bookmarks across all docs in one grid) into
  v2.0.5 or defer to v2.0.6? Plan defers — adds another ~250 LOC
  and a new top-level panel.
- Bookmark export to Markdown / JSON in v2.0.5? Plan defers —
  small but adds a new menu/button to the sidebar header.
- Auto-section detection (auto-name from PDF outline crumb)?
  Plan defers — needs a dest-to-page resolver pass on every
  outline node (would more than double Slice 3's scope).
- **Four-plan backlog is now on disk.** At what point does Sanjay
  want the next tick to switch from "planning" to "executing"
  (i.e. start a subagent against v2.0.2 Slice 1)?

---



## TICK 2026-05-20 05:37 PT — MODE C writing-plans skill — v2.0.4 plan promoted 📖

User invoked the `writing-plans` skill a third consecutive tick.
Pattern holds: each writing-plans tick promotes the next release in
the pipeline. Two ticks ago: v2.0.2. Last tick: v2.0.3. This tick:
**v2.0.4 "Memory" 📖** — Slab remembers where you left off.

**Why v2.0.4 next?**
1. v2.0.2 and v2.0.3 plans are already on disk and ready for subagent
   execution. Writing them in advance lets future ticks just run
   plans without re-discovering codebase context.
2. v2.0.4 fills a *table-stakes* Buy-Button gap: Adobe Acrobat,
   PDF Expert, and Foxit all remember last page. Slab today opens
   every doc on page 1, even ones the user just had open. That's a
   launch-blocker for v1.0.0 — no serious PDF-reader review forgives
   it. Cheap fix (no Tauri / Rust changes), huge perceived-quality
   uplift.
3. Surfaces are clean: `src/lib/recent.ts`, a new
   `src/lib/readingPosition.ts`, `ReaderPanel.svelte`,
   `CommandPalette.svelte`, `OnboardingTour.svelte`, and disjoint
   i18n namespaces. **Zero collision with v2.0.2 (marketplace) or
   v2.0.3 (Settings Beacon section).**

**Commit this tick (1 on main):**
- `<HASH>` docs(plans): v2.0.4 "Memory" implementation plan
- Plan file: `docs/plans/2026-05-20-v2.0.4-memory.md` (~51 KB, 1482 lines).

**Plan structure — 6 slices + pre-flight, ~580-720 LOC, 6-7 commits:**
- **Slice 0** (pre-flight): 4 verification checks — `RecentFile` shape,
  pdfjs event names, vitest availability, surface-collision audit
  against v2.0.2/v2.0.3.
- **Slice 1**: pure-TS `src/lib/readingPosition.ts` (~150 LOC) —
  per-path localStorage store under `slab.reading.positions.v1`,
  capped at 200 entries with oldest-first eviction on quota crash.
  Optionally ships with 9 vitest unit tests (~120 LOC) if test
  runner is wired.
- **Slice 2**: Save+restore hooks in `ReaderPanel.svelte` — debounced
  500ms write on `pagechanging` / `scalechanging`; restore page +
  zoom on `pagesinit` with defensive clamp to `1..pageCount`. Flush
  on `tearDownDoc` to avoid losing the last write.
- **Slice 3**: Reader-toolbar pill chip — accent-tinted percent
  badge right of the page-number input. Tooltip = "Page N of M · X%
  read". Two new i18n strings under `reader.progress.*`.
- **Slice 4 ⭐ WOW**: SVG progress rings on every Recents card.
  Frosted-glass backdrop, accent-tinted arc, integer percent label,
  220ms ease-out arc transition. Cards subscribe to
  `subscribeReadingPosition` so the ring updates the moment the user
  finishes reading and closes a tab.
- **Slice 5**: Command-palette "Resume reading…" group — top 3
  in-progress docs (sorted by recency, filtered to 2-99% progress).
  Cmd+K → "r" → Enter resumes the most-recent read. Three new i18n
  strings under `palette.resume.*`.
- **Slice 6**: onboarding tour step #4 (📖 Memory), version bumps
  (2.0.3 → 2.0.4 in package.json/Cargo.toml/tauri.conf.json),
  CHANGELOG entry, customer-facing release notes at
  `docs/release-notes/v2.0.4.md`, MODE A merge + tag + push, queue
  `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Adobe / PDF Expert / Foxit all ship this.
  Closing the gap is a hard prerequisite for the v1.0.0 launch
  narrative.
- **Notice-it PASS** — every returning user sees: (a) new toolbar
  pill chip, (b) progress rings on every recent card, (c) the exact
  page they closed at restored on every reopen.
- **Tell-a-friend PASS** — the progress-ring grid visualizes your
  reading life. /r/macapps + Twitter-grade screenshot. **None of
  Adobe / PDF Expert / Foxit do this.**

**Codebase-discovery decisions baked into the plan:**
- `RecentFile` shape is left alone — the new store is a *separate*
  module so a future Pro-tier sync layer can swap the backend
  cleanly without touching `recent.ts`.
- `currentScaleValue` restore wrapped in `try { } catch {}` — pdfjs
  accepts both named tokens (`"page-width"`, `"auto"`) and numeric
  strings; invalid stored zoom silently falls back to default.
- `pagesinit` is the right hook for restore (not `pagesloaded`,
  which is later). Verified at line ~694 of ReaderPanel.svelte.
- Recents grid currently lives *inside* `ReaderPanel.svelte` (not
  a separate component) — Slice 4's markup edit is local to one
  file.
- `subscribeReadingPosition` is the right pattern (vs polling) — the
  ring needs to refresh on every position write, and the existing
  recents `subscribeRecent` proves the listener pattern is clean
  inside the component.

**Scheduling note (critical, documented in the plan):**
**Do not start v2.0.4 Slice 1 until v2.0.3 ships and is tagged.**
Both versions touch `src/lib/i18n/en.json` (disjoint namespaces, but
git's textual three-way merge doesn't know JSON). Strict order:
v2.0.2 → v2.0.3 → v2.0.4, each cut from `main` after the previous
merges.

**Next ticks (in order):**
1. **v2.0.2 Slices 1-8** on `feature/v2.0.2-workshop-marketplace`. WOW = Fuse.js search w/ live highlighting (Slice 6).
2. **v2.0.2 MODE A merge + tag + release**.
3. **v2.0.3 Slices 1-6** on `feature/v2.0.3-beacon-settings`.
4. **v2.0.3 MODE A merge + tag + release**.
5. **v2.0.4 Slices 1-6** on `feature/v2.0.4-memory`.
6. **v2.0.4 MODE A merge + tag + release**.

**Open questions for Sanjay (only if he wants to weigh in):**
- Bundle per-page bookmarks into v2.0.4 or defer to v2.0.5? Plan
  defaults to deferring (separate `bookmarks.v1` store, ~150 LOC,
  out of scope for "Memory").
- Add reading-time estimates ("3 min left at your reading speed")?
  Plan defers — needs cross-session reading-speed tracking.
- Three-plan backlog ready to execute. Any preference on next tick's
  mode (start *executing* v2.0.2 vs continue planning v2.0.5)?

---



## TICK 2026-05-20 01:05 PT — MODE C writing-plans skill — v2.0.3 plan promoted 📋

User invoked the `writing-plans` skill again. Following the same
pattern as the previous tick (which wrote v2.0.2), this tick wrote the
**next** release plan in the pipeline: **v2.0.3 "Beacon Settings"**.

**Why v2.0.3 next instead of more v2.0.2 slices?**
1. v2.0.2 plan is already on disk and ready for subagent execution —
   future ticks just need to run it. No more planning needed there.
2. v2.0.3 fills an objectively glaring gap: Beacon (Slab's AI layer)
   has shipped 9 features across v1.5–v1.9 with **zero UI to
   configure it**. Today users must hand-edit `~/.slab/config.toml`
   to switch providers or models. That fails the buy-button test
   loudly — Adobe's AI is *cloud-locked + paid* but it has a UI; ours
   is *local + free* and has no UI. Easy fix, huge perceived-quality
   uplift.
3. Plans cluster on the same files (SettingsPanel.svelte + i18n).
   Writing the v2.0.3 plan now while the codebase is fresh in
   context — including its real CmdResult tagged-enum shape, the
   `From<Result, AiError>` idiom, the existing `slab_beacon_config_*`
   command pair — is cheaper than re-discovering all that two ticks
   later.

**Commit this tick (1 on main):**
- `a3fa231` docs(plans): v2.0.3 "Beacon Settings" implementation plan
- Plan file: `docs/plans/2026-05-20-v2.0.3-beacon-settings.md` (~57 KB, ~1780 lines).

**Also landed on main this tick** (sibling subagent prepared on `fix/windows-eol-bundled-scripts`, this tick ff-merged):
- `1a28a5f` fix(ci): pin bundled plugin files to LF on Windows checkouts — `.gitattributes` (48 LOC) marks bundled plugin scripts as binary so git never touches line endings, plus `normalize_lf` belt-and-suspenders helper in seeder + 2 regression tests. Author claims 913/913 lib tests green locally. Unblocks v2.0.1 Windows CI.

**Plan structure — 6 slices + pre-flight, ~1030 LOC, 7 commits:**
- **Slice 0** (pre-flight): 4 verification checks before any code lands. Specifically pins the `CmdResult` tagged-enum shape, the `SettingsPanel` section pattern, the existing `slab_beacon_config_*` cmds.
- **Slice 1**: pure-Rust `ai::introspect` module — `models()` + `ping()` + `IntrospectError` + `is_embedding_model()` heuristic. 8 mockito-backed tests for Ollama + OpenAI happy paths, connection-refused, 5xx, invalid JSON, `/v1` path handling, ping semantics. +280 LOC.
- **Slice 2**: three Tauri commands (`slab_beacon_list_models`, `slab_beacon_provider_test`, `slab_beacon_default_config`) + the matching `From<Result, IntrospectError>` impl. +1 unit test pinning defaults.
- **Slice 3**: `src/lib/beacon-settings.ts` TS client wrapper + 26 i18n strings under `settings.beacon.*`.
- **Slice 4 ⭐ WOW**: `BeaconSettingsSection.svelte` (~340 LOC). Live Ollama-introspection model dropdown with disk-size badges, green/red/amber status dot, debounced refresh on base-URL edits, "No models installed" empty state with copy-pull-command button, auto-save 500ms after changes. Wired into `SettingsPanel.svelte` between Language and Theme sections.
- **Slice 5**: onboarding step #4 + 2 command-palette entries + 9 more i18n strings. Discoverability multiplier on Slice 4.
- **Slice 6**: version bumps (2.0.2 → 2.0.3 in package.json/Cargo.toml/tauri.conf.json), CHANGELOG entry, customer-facing release notes at `docs/release-notes/v2.0.3.md`, MODE A merge + tag + push, queue `RELEASE_PENDING` for next tick.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Adobe ships paid cloud-only AI; PDF Expert ships none. We give away a local-first AI config UI for free.
- **Notice-it PASS** — every returning user sees a new "Beacon AI" Settings section the moment they upgrade.
- **Tell-a-friend PASS** — "look, my PDF reader auto-finds my installed Ollama models." Genuine /r/LocalLLaMA + HN-grade screenshot.

**Codebase-discovery decisions baked into the plan:**
- `CmdResult` is `{ kind: "ok", value: T } | { kind: "err", message: string }` (tagged serde enum, NOT a Result alias). All 5 plan code blocks corrected mid-write to match this exactly.
- `.into()` requires a `From<Result<T, MyError>>` impl. Plan adds the `IntrospectError` variant alongside the existing 6 (AiError, PdfError, StudyError, IndexError, PiiError, LibraryError).
- `mockito` already a dev-dep — no new Cargo deps required.
- `SettingsPanel.svelte` insertion uses surrounding-line anchors (between Language `</section>` and Theme `<h2>`), not absolute line numbers, so the plan survives drift.

**Scheduling note (critical, documented in the plan):**
**Do not start v2.0.3 Slice 1 until v2.0.2 ships and is tagged.** Both
versions modify `SettingsPanel.svelte` + `src/lib/i18n/en.json` — running
in parallel branches = merge conflicts. Strict order: v2.0.2 Slice 0
(gitattributes hotfix on main) → v2.0.2 Slices 1–8 on feature branch →
v2.0.2 merge + tag → THEN cut `feature/v2.0.3-beacon-settings` from main.

**Next ticks (in order):**
1. **v2.0.2 Slice 0**: `.gitattributes` LF pin on `main` to unblock Windows CI. ~2 commits.
2. **v2.0.2 Slices 1–8** on `feature/v2.0.2-workshop-marketplace`. WOW = Fuse.js search w/ live highlighting (Slice 6).
3. **v2.0.2 MODE A merge + tag + release**.
4. **v2.0.3 Slices 1–6** on `feature/v2.0.3-beacon-settings`.

**Open questions for Sanjay (only if he wants to weigh in):**
- Fold auto-download Ollama + model-autopull-from-dropdown into v2.0.3, or keep them parked for v2.0.4? Plan defaults to deferring (both add ~150 LOC each and v2.0.3 is already ~1030 LOC).
- Release-notes framing names Adobe explicitly as the comparison. OK or soften?

---

## TICK 2026-05-20 00:42 PT — MODE C writing-plans skill — v2.0.2 plan promoted 📋

User invoked the `writing-plans` skill. Tick deliverable is a plan, not
running code. Plan target: **v2.0.2 "Workshop Marketplace"** — leads with
a Slice 0 hotfix for the v2.0.1 Windows CI failure, then ships the
Plugin Store revamp on `feature/v2.0.2-workshop-marketplace`.

**Why v2.0.2 and not v0.10.0 Beacon?** Two reasons:
1. v2.0.1's CI is *blocked* on Windows (run `26147660187` failed on a
   sha256 mismatch — CRLF-in-Windows-checkout vs. LF-on-disk-hash). That
   blocker has to land somewhere before any new release, and it folds
   naturally into the front of the next release.
2. STATE.md (previous tick) flagged v2.0.2 candidate = Plugin Store UX
   polish. v0.10.0 Beacon is also queued but per the cron mandate
   ("BUY-BUTTON, BIG features, customer-would-pay") a *real Plugin Store*
   is a more obvious "would-pay" upgrade for v2.0.x than another AI
   slice on top of an already-shipped Beacon stack (v1.7-v1.9 already
   shipped chat, citations, glossary, study, voice, PII, vision).

**Commit this tick (1 on main):**
- `d9239f1` docs(plans): v2.0.2 "Workshop Marketplace" implementation plan
- Plan file: `docs/plans/2026-05-20-v2.0.2-workshop-marketplace.md` (46 KB, 1394 lines).

**Plan structure — 9 slices, ~1140 LOC, ~10 commits:**
- **Slice 0** (hotfix on main): `.gitattributes` pinning *.js / *.toml /
  *.json to `eol=lf`. Unblocks Windows CI. 2 commits.
- **Slice 1**: marketplace::index schema v2 — `IndexEntryV2` wraps
  `IndexEntry` via `#[serde(flatten)]` + new fields `categories`,
  `tags`, `screenshots`, `installs`. Backwards-compatible signing
  payload via `IndexEntryUnsignedV2`. +4 unit tests.
- **Slice 2**: TS mirror in `src/lib/types/marketplace.ts` + store update.
- **Slice 3**: Embedded `seed-index.json` so Browse is useful offline on
  day one. New `BUNDLED` signature sentinel for in-binary entries.
- **Slice 4**: Category chip filters w/ live counts (Liquid Glass styles).
- **Slice 5**: Sort dropdown — Popular / A-Z / Updates First.
- **Slice 6** ⭐ WOW: Fuse.js fuzzy search w/ live match highlighting.
  Typing "hello" instantly narrows results AND highlights matched chars
  in plugin names with accent-tinted `<mark>` spans.
- **Slice 7**: Hero card on empty Browse + install-count social proof
  (only shown when `installs >= 10` to avoid the "1 install" anti-pattern).
- **Slice 8**: Version bump 2.0.1→2.0.2 + CHANGELOG.md entry +
  customer-facing release notes at `docs/release-notes/v2.0.2.md` +
  onboarding-tour copy update + MODE A merge + tag.

**Buy-Button verdict at release level:**
- **Pick-us PASS** — Adobe / PDF Expert / Foxit ship no plugin stores.
- **Tell-a-friend PASS** — categorized, searchable, sortable storefront
  is screenshot-worthy.
- **Notice-it PASS** — v2.0.1 users see the flat grid become a real
  store on upgrade.

**Decisions documented in the plan:**
- Schema v2 as `flatten`-wrapper rather than mutating `IndexEntry`
  preserves v1-signed-index verification.
- `BUNDLED` sentinel signature short-circuits Ed25519 verification only
  when paired with `download_url: "bundled://..."` — defense in depth.
- Fuse.js (6 KB gzipped) is the only new runtime dep; hand-roll fallback
  documented as Step 6.1 alternate.
- YAGNI: drop "Newest" sort until publisher emits `published_at`.

**Next ticks:**
1. **Slice 0** — `.gitattributes` + renormalize + push on main; wait for
   CI green; then v2.0.1 is unblocked (or we just roll into v2.0.2).
2. **Slice 1** — cut `feature/v2.0.2-workshop-marketplace`, schema v2.
3. Slices 2-7 — likely fold 2-3 per tick (each is below cron's
   buy-button bar individually but the WOW slice 6 is its own tick).
4. Slice 8 — MODE A merge + tag + release.

**Open questions for Sanjay (only if he wants to weigh in):**
- Re-finalize v2.0.1 GitHub release after Slice 0, or roll the hotfix
  directly into v2.0.2? Plan defaults to v2.0.2 (no force-pushed tags).
- Fuse.js vs hand-roll? Plan documents both; default = Fuse.js.

---


## TICK 2026-05-20 00:??-01:?? PT — MODE C → MODE A v2.0.1 SHIPPED 🧩 (6 commits + merge, +1700 LOC)

Plan-then-execute mega-tick. The v2.0.1 plan (1024 lines) was already on
disk from the previous tick; this tick executed Slices 2–7 in one
session, ran quality gates on the feature branch AND on main after the
merge, tagged v2.0.1, and pushed everything.

**Headline:** Three example plugins — `com.slab.examples.hello-workshop`,
`com.slab.examples.storage-counter`, `com.slab.examples.url-fetch` — now
ship in the Slab binary itself. On first boot a tiny seeder
(`plugins::bundled`) `include_str!`s manifest+script for all three and
materializes them under `~/.slab/plugins/<id>/` *before* the registry
discovery scan, so a brand-new install has three real, working plugins
in the panel on day one. No marketplace round-trip required.

**Commits this tick (6 on feature branch, 1 merge on main):**
- `91f3d55` feat(plugins): bundle storage-counter + url-fetch (v2.0.1 Slice 2)
- `8853fd1` feat(plugins/ui): "Bundled" pill on first-party plugins (v2.0.1 Slice 3)
- `1741af0` feat(onboarding): "Plugins, included" tour step (v2.0.1 Slice 4)
- `67d2051` test(plugins): pin BUNDLED roster + manifest-id parity (v2.0.1 Slice 5)
- `1c79f51` docs: v2.0.1 CHANGELOG.md + README bundled-plugins blurb (Slice 6)
- `de8ec72` chore(release): bump tauri.conf.json to 2.0.1 (Slice 7)
- `9780273` Merge v2.0.1 'Bundled Hello Workshop' (MODE A, on main)

**What landed:**
- `src-tauri/src/plugins/bundled.rs` — extended `BUNDLED` from 1 → 3 plugins (Slice 2). All three sha256 hashes verified against the actual `script.js` bytes.
- `src/lib/plugins.ts` — `BUNDLED_PLUGIN_IDS` readonly const + `isBundled(id)` helper (Slice 3).
- `src/lib/panels/PluginsPanel.svelte` — accent-tinted "Bundled" pill next to plugin name in Installed tab, with tooltip explaining seed semantics (Slice 3).
- `src/lib/i18n/en.json` — two new strings `plugins.installed.bundled_pill` + `plugins.installed.bundled_tooltip` (Slice 3).
- `src/lib/OnboardingTour.svelte` — new "🧩 Plugins, included" step between Beacon AI and Command Palette steps; tour grew from 5 → 6 steps (Slice 4).
- Two new bundled unit tests: `bundled_roster_contains_all_three_v2_0_1_examples` (pins roster contents — catches removal regressions) and `bundled_manifest_ids_match_roster_entries` (parity between rust roster ids and embedded manifest ids — catches the silent-invisibility bug class) (Slice 5).
- `CHANGELOG.md` — new top-level file, Keep a Changelog 1.1.0 format, [2.0.1] + [2.0.0] entries + compare-link footers (Slice 6).
- `README.md` — "Extensible" highlight expanded to mention 3 example plugins ship in the binary (Slice 6).
- `src-tauri/tauri.conf.json` — version 2.0.0 → 2.0.1 (Slice 7). All three version sources (package.json, Cargo.toml, tauri.conf.json) now in sync.

**Quality gates after merge on `main` HEAD `9780273`:**
- `cargo fmt --all -- --check` → clean
- `cargo clippy --all-targets -- -D warnings` → clean
- `cargo test --lib` → **911 passed / 0 failed** (was 909 before Slice 5; +2 from `bundled_roster_*` + `bundled_manifest_ids_*` tests)
- `pnpm check` → 0 errors, 35 pre-existing warnings (unchanged)

**Buy-Button verdict:**
- **Pick-us (PASS)** — Adobe, PDF Expert, Foxit ship zero bundled plugins for end users. We ship three with source code on disk.
- **Tell-a-friend (PASS)** — "Look, I downloaded a free PDF reader and it came with three example plugins I can read and modify" is genuinely HN-screenshot-worthy for the developer audience.
- **Notice-it (PASS)** — Anyone upgrading from v2.0.0 will see 3 new plugins in Cabinet where they had 0.
- **Pay-for-it (n/a)** — table-stakes onboarding moment, not a power feature, but it makes the entire v2.0 Workshop release land.

**WOW moment:** The Cabinet quick-action bar now populates itself with three real plugin tools ("Say Hi", "Counter +1", "Fetch URL…") before the user has done anything. First-time user fires a real plugin within 5 seconds of opening the app.

**Next ticks:**
1. **MODE B finalize v2.0.1** once CI run `26147660187` goes green — `gh release create v2.0.1` with the customer-facing notes drafted in `CHANGELOG.md`. Curate the 7 best artifacts (linux: AppImage + deb + rpm; macos: arm64 dmg + x86 dmg; windows: msi + setup.exe).
2. **v2.0.2 candidates:** Plugin Store UI polish (categorize bundled vs third-party in the discover tab); Foundry plugin marketplace listings for the 3 bundled plugins so they show up as "first-party" entries; settings panel section for "Manage bundled plugins" (toggle bundle-on-first-boot off if user is a power dev who hates pre-installs).
3. **v0.10.0 "Beacon" AI spec** still queued at `.cron-state/proposals/v0.10.0-beacon-ai.md` — the buyer-magnet release. Could pivot to this after v2.0.1 finalize if the Workshop arc feels feature-complete enough.

---

## TICK 2026-05-19 23:?? PT — MODE B+C v2.0.0 RELEASE FINALIZED 🚀 + v2.0.1 PLAN PUSHED 📋

**Two MODEs in one tick:**

### MODE B finalize: v2.0.0 Workshop release published
- Polled CI run `26145461445` (the fix-forward after `26145019423` was killed by a pnpm.overrides CI lint). All 7 jobs green:
  - `cargo test` x3 OS (macos-arm64, windows-x64, linux-x64)
  - `bundle` x4 (macos-arm64, macos-x64, windows-x64, linux-x64)
- `gh release view v2.0.0` shows the release was auto-published by the build workflow with **7 artifacts** (macOS arm64+x64 dmg, linux deb+AppImage+rpm, windows msi+nsis-exe) plus SHA256SUMS. URL: https://github.com/Sanjays2402/slab/releases/tag/v2.0.0
- `RELEASE_PENDING` cleared.
- Customer-facing release notes drafted at `/tmp/slab-v2.0.0-release-notes.md` (the auto-published notes are already on GH, but the long-form pitch is available for HN/Twitter/Product Hunt if Sanjay wants to use it).

### MODE C: writing-plans skill — v2.0.1 "Bundled Hello Workshop" plan
Sanjay invoked the writing-plans skill against the next release. WIP for v2.0.1 was already on disk (319-line `src-tauri/src/plugins/bundled.rs` untracked + modifications to lib.rs, plugins/mod.rs, and the three SDK example manifests/scripts) but no formal plan existed. Promoted the WIP to a full sliced plan.

**Plan file**: `docs/plans/2026-05-19-v2.0.1-bundled-hello-workshop.md` — 1024 lines, 38 KB.

**Commit this tick**:
- `ff4f52b` docs(plans): v2.0.1 "Bundled Hello Workshop" implementation plan

**The release in one sentence**: every fresh Slab install now ships with three working runtime plugins (`com.slab.examples.{hello-workshop,storage-counter,url-fetch}`) seeded into `~/.slab/plugins/` on first launch — closing the "platform without plugins" gap v2.0.0 left behind.

**Plan covers 7 slices on `feature/v2.0.1-bundled-hello-workshop`:**
1. Commit existing WIP — hello-workshop seeder + lib.rs wiring (~340 LOC, 7 unit tests).
2. Extend BUNDLED roster to three plugins (~30 LOC).
3. "Bundled" pill in PluginsPanel + i18n strings (~90 LOC).
4. New OnboardingTour step ("Try a plugin — three are already installed") (~70 LOC).
5. Integration smoke test guarding TS/Rust/manifest id alignment (~50 LOC).
6. CHANGELOG.md v2.0.1 entry + README bullet (~30 LOC).
7. Version bump 2.0.0 → 2.0.1 (~10 LOC).
8. Then MODE A merge to main + tag v2.0.1.

**Buy-Button verdict (documented in the plan):**
- **Pick-us PASS** — Adobe, PDF Expert, Foxit ship zero plugins to end users.
- **Tell-a-friend PASS** — three working plugins + colocated SDK source is a screenshot-worthy "developer respect" moment.
- **Notice-it PASS** — anyone who updated v2.0.0 → v2.0.1 will see 3 new plugins in the panel.

**WOW for v2.0.1**: Cabinet quick-action bar populates itself with "Say Hi", "Counter +1", and "Fetch URL…" before the user does anything. First-time user fires a plugin within 5 seconds.

**Push**: `feature/v2.0.1-bundled-hello-workshop` pushed to origin with the plan commit.

**Next tick**: dispatch a subagent against Slice 1 of the plan (hello-workshop seeder commit). Existing WIP on disk already satisfies most of the slice — the tick will be cargo gates + git add + commit.

---

## TICK 2026-05-19 21:?? PT — MODE C v2.0.0 Slice 9 SHIPPED 📦 (@slab/plugin-sdk npm package, 5 commits, ~1200 LOC)

**Branch**: `feature/v2.0.0-workshop` — pushed to origin.
**Plan**: `docs/plans/2026-05-19-v2.0.0-workshop-slice-9.md` (20.8 KB).

The plugin authoring SDK is **done**. Third-party authors no longer need
to read Rust source to write a Slab plugin — they `npm i -D @slab/plugin-sdk`,
type `import { definePlugin } from "@slab/plugin-sdk"`, and the entire
`slab.*` surface lights up in IntelliSense. **Buy-Button: Pick-us pass** —
Adobe doesn't ship a plugin SDK; PDF Expert/Foxit don't even have a
plugin model. **WOW**: ambient `declare global` propagates into the
emitted `.d.ts` so a single import wires up the whole surface.

**Commits this tick (5):**
- `2c464c5` feat(sdk): @slab/plugin-sdk package skeleton (Slice 9.1)
- `7970e12` feat(sdk): typed mirrors for slab.{manifest,beacon,ui,document,storage,fetch} (Slice 9.2)
- `e0baad9` feat(sdk): definePlugin + assertSlab + global ambient + smoke test (Slice 9.3)
- `b3b7787` feat(sdk): three example plugins — hello-workshop, storage-counter, url-fetch (Slice 9.4)
- `fef65de` docs(sdk): top-level README + CHANGELOG for @slab/plugin-sdk (Slice 9.5)

**What's at `sdk/slab-plugin-sdk/`:**
- `package.json` — `@slab/plugin-sdk@0.1.0`, MIT license, dual ESM+CJS+`.d.ts` exports.
- `src/types/` — 7 modules (manifest, beacon, ui, document, storage, fetch, global) totaling ~23 KB, every type with `@see` JSDoc pointing at the Rust ground-truth file:line.
- `src/define.ts` — `definePlugin(spec)`, `assertSlab(slab)`, `trySlab(slab)` (~85 LOC, ~0.5 KB gzipped).
- `src/index.ts` — public re-exports + ambient `declare global { var slab: SlabGlobal }` block (tsc propagates into emitted `.d.ts`).
- `tests/typecheck-smoke.ts` — 170 LOC exhaustive consumer-side test with positive + `@ts-expect-error` negative cases.
- `examples/{hello-workshop,storage-counter,url-fetch}/` — three reference plugins, < 100 LOC each, manifest + script + README.
- `scripts/rename-cjs.mjs` — post-build CJS extension fixer (TS #54573 workaround); handles `.js → .cjs`, `.js.map → .cjs.map`, and rewrites `require()` + `sourceMappingURL` refs.
- `README.md` (7.5 KB) — install, quickstart, API tour, capability lattice table, security model, examples links.
- `CHANGELOG.md` (3.3 KB) — Keep-a-Changelog 1.1.0 format, 0.1.0 entry documenting every addition.
- 4 tsconfig files: base, `.build.json` (ESM), `.cjs.json`, `.types.json`, `.tests.json` (with path-mapping so examples typecheck in-tree).

**Build verification:** clean dist = 54 files / 220 KB.
- `dist/esm/` 9 `.js` + 9 `.js.map`
- `dist/cjs/` 9 `.cjs` + 9 `.cjs.map`
- `dist/types/` 9 `.d.ts` + 9 `.d.ts.map`

**Quality gates (all green):**
- `tsc --noEmit -p tsconfig.tests.json` → exit 0
- `cargo fmt --all -- --check` → clean
- `cargo clippy --all-targets -- -D warnings` → clean (no rust touched, but verified)
- `pnpm check` → 0 errors, 35 pre-existing CSS warnings (unrelated)

**Key design decisions:**
- **MIT license for SDK** (vs GPL-3.0 parent): keeps plugin-author friction low, no copyleft contamination of third-party plugin source.
- **`declare global` inline in `src/index.ts`** (not a separate `ambient.d.ts`): tsc emits broken runtime `require("./ambient.cjs")` for side-effect imports. Inline declaration propagates cleanly into `dist/types/index.d.ts`.
- **CJS rename via post-build script**: tsc still doesn't honor `outFileExtension` (TS #54573). Tiny node script handles it.
- **Tests path-mapped via `tsconfig.tests.json`**: `paths: { "@slab/plugin-sdk": ["./src/index.ts"] }` so examples can `import from "@slab/plugin-sdk"` and typecheck without `pnpm install`.

**Out of scope (Slice 9 follow-up):**
- `npm publish @slab/plugin-sdk` — requires `@slab` org ownership.
- Foundry Plugin Store UI panel (planned next as part of v2.0.0 finalize).

**Next ticks:**
1. **Merge `feature/v2.0.0-workshop` → main** as v2.0.0 "Workshop" release. 44 commits ahead of main. Run full quality gates on main first.
2. **Foundry Plugin Store UI** — discover/install/uninstall plugin panel; first-party listing for the 3 example plugins from Slice 9.
3. **v0.10.0 "Beacon"** spec is queued at `.cron-state/proposals/v0.10.0-beacon-ai.md`.

---

## TICK 2026-05-19 02:33 PT — MODE C v2.0.0 Slice 8.2-8.7 SHIPPED 🗄️ (slab.storage.* end-to-end live)

Massive vertical slice — Slice 8 of v2.0.0 Workshop is now COMPLETE on
the feature branch. `slab.storage.{get,set,remove,list,clear,usage}`
is fully wired from plugin JS → rquickjs binding → `HostBindings` →
SQLite (`~/.slab/plugin-storage.sqlite`), with per-plugin scoping
enforced in every SQL WHERE clause and a three-axis quota system.
Three commits this tick.

**Commits this tick:**
- `8b10219` feat(plugins/storage): kv CRUD + per-plugin quotas + 16 unit tests (v2.0.0 Slice 8.2-8.3)
- `26487f3` feat(plugins/runtime): plumb SharedPluginStorage through HostBindings (v2.0.0 Slice 8.4)
- `4534aa5` feat(plugins/runtime): slab.storage.* live in JS (v2.0.0 Slice 8.5+8.7)
- `73d2210` feat(plugins/runtime): slab.storage.* E2E tests + microtask pump (v2.0.0 Slice 8.6)

**Slice 8.2-8.3 (`8b10219`) — CRUD + quotas + 16 unit tests:**
- Six methods on `PluginStorage`: `kv_get` / `kv_set` / `kv_remove` /
  `kv_list` / `kv_clear` / `kv_usage_bytes`. All take `&plugin_id` as
  first arg; scoping is enforced in SQL only.
- `kv_set` does the quota dance: `current_total − prev_size + new_size`
  against `MAX_PLUGIN_BYTES`. Overwrite of an existing key doesn't
  double-count (covered by `overwrite_does_not_double_count`).
- 16 new unit tests cover: scoping (cross-plugin invisibility),
  CRUD round-trip, unicode + embedded NUL bytes, overwrite, sorted
  list output, KeyTooLong/ValueTooLarge/QuotaExceeded boundaries,
  usage byte accounting. 24/24 storage tests green.

**Slice 8.4 (`26487f3`) — HostBindings plumbing:**
- New `pub storage: Option<SharedPluginStorage>` field on `HostBindings`.
- `PluginActor::spawn` now calls `shared_storage().ok()`. Filesystem
  failure → `storage = None` → bindings reject Promise per-call
  instead of crashing the actor (better UX than fatal init).
- New `PluginActor::spawn_inner(..., Some(shared))` test seam — used
  by Slice 8.6 E2E tests with `shared_in_memory()` for determinism.
- Ephemeral `enable_plugin` and `execute_script` paths pass `None` so
  declarative-only plugins still load cleanly.

**Slice 8.5+8.7 (`4534aa5`) — live JS bindings + sentinel flip:**
- Six `make_storage_*` factories in `slab_global.rs`. Each: lock the
  mutex, call the corresponding `kv_*` method, wrap result in a
  Promise. When `b.storage` is `None`, return a rejected Promise with
  "slab.storage unavailable outside the per-plugin runtime".
- Module-level docs and the `enable_plugin_reserved_surfaces_throw…`
  sentinel test in `mod.rs` updated to reflect Slice 8 LIVE status —
  rewrote the test to verify the new contract (rejected Promise on
  ephemeral path, NOT a sync throw).

**Slice 8.6 (`73d2210`) — E2E tests + critical microtask pump fix:**
- 7 new E2E tests using `spawn_inner` + `shared_in_memory()`. All green.
- **Two bugs surfaced and fixed in flight:**
  1. `resolve_now` was using `Promise.resolve(value)` via
     `Promise` global; rquickjs 0.11 tuple-arg marshalling treated
     `(value,)` as "not an object". Replaced with `Promise::new(ctx)` +
     immediate `resolve.call((value,))?` — same pattern fetch uses.
  2. **Actor never pumped microtasks after eval.** Plugin scripts
     using the canonical `(async () => { ... })()` IIFE idiom would
     suspend on the first `await` and never wake. Now: after eval
     succeeds, drain `execute_pending_job` to completion (outside
     `ctx.with` to avoid nested-borrow panic). Fetch path still pumps
     after each `dispatch_fetch`.

**Quality gates on `feature/v2.0.0-workshop` HEAD `73d2210`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **902 passed / 0 failed** (879 prior + 16
  storage CRUD unit tests + 7 storage E2E tests; sentinel test
  rewritten in place — no net delta from that file)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Slice 8 status:** COMPLETE except for 8.8 (final push). Pushing this
tick.

---

## TICK 2026-05-19 01:40 PT — MODE C v2.0.0 Slice 8 plan + Slice 8.1 scaffolding

Plan-first tick (writing-plans skill invoked). Slice 8 (`slab.storage.*`)
ships a per-plugin, sqlite-backed key/value store with no manifest
capability gate (scoping IS the security boundary) — a deliberate
departure from the slab.fs/slab.net/slab.ui/slab.beacon pattern. Worth
documenting before writing 1230 LOC of host code.

**Commits this tick:**
- `457503a` docs(plans): v2.0.0 Slice 8 sub-plan — slab.storage.* per-plugin KV store
- `26d498f` feat(plugins/storage): PluginStorage skeleton + sqlite schema (v2.0.0 Slice 8.1)

**Slice 8 plan (`docs/plans/2026-05-19-v2.0.0-workshop-slice-8.md`, 826 lines):**
- 8 sub-tasks (8.1 module+schema ✅, 8.2 CRUD+quotas, 8.3 12 unit tests,
  8.4 plumb into HostBindings, 8.5 5 JS bindings, 8.6 5 E2E tests, 8.7
  flip reserved-surface sentinel, 8.8 gates+push).
- Architecture: process-wide `OnceLock<Arc<Mutex<PluginStorage>>>` over
  one `~/.slab/plugin-storage.sqlite`. Mirrors `fetch::shared_client()`
  shape. Per-plugin scoping enforced in code — every WHERE clause pins
  `plugin_id`.
- Major divergence from Slice 7: NO actor round-trip for the JS API.
  Sqlite k/v ops are microseconds; we run them synchronously inside
  `Mutex::lock()` and wrap results in `Promise.resolve(...)` /
  `Promise.reject(...)`. Forward-compatible — can switch to actor
  bridging later without breaking the JS API.
- Quota model: 8 MiB total per plugin / 1 MiB per value / 64 KiB per key.
  Compile-time `const_assert!` chain enforces invariants between them
  (e.g. value cap ≤ plugin cap, plugin cap is a multiple of value cap).

**Slice 8.1 (`26d498f`) — module scaffolding (~480 LOC, 2 files):**
- New `src-tauri/src/plugins/storage.rs`:
  - `PluginStorage` owning handle wrapping `rusqlite::Connection`.
  - Schema v1: `kv(plugin_id, key, value, value_size, updated_at)` with
    `PRIMARY KEY (plugin_id, key)` + `idx_kv_plugin`. `value` is BLOB
    (not TEXT) so arbitrary UTF-8 with embedded NULs round-trips.
    `value_size` denormalised for fast `SUM()` quota queries.
  - `open()` / `open_in_memory()` / `shared_storage()` / test-only
    `shared_in_memory()`.
  - `StorageError` enum pre-declares `KeyTooLong` / `ValueTooLarge` /
    `QuotaExceeded` variants now (CRUD lands in 8.2; pre-declaration
    keeps the Display assertions testable today).
  - Public quota constants: `MAX_PLUGIN_BYTES` / `MAX_VALUE_BYTES` /
    `MAX_KEY_BYTES`. Compile-time `const _: () = assert!(...)` chain
    enforces the invariants between them.
- Wired into `plugins/mod.rs`: `pub mod storage;` + re-exports.
- 8 unit tests cover schema init, PRAGMA user_version stamping, index
  existence, compound-PK behaviour (same key allowed across plugins),
  init_schema idempotency, in-memory handle isolation, default path
  lives under `~/.slab/`, and StorageError Display messages include the
  numeric size info the JS binding embeds into rejected-Promise messages.

**Why a plan tick (again) instead of full Slice 8:** Slice 8 has one
non-obvious design decision worth pinning before writing code (no
manifest cap), one structural divergence from Slice 7 (sync-then-wrap
vs actor round-trip), and a security invariant (per-plugin scoping in
SQL only — no other layer enforces it) that needs explicit
documentation so a future maintainer doesn't refactor the WHERE
clauses out. Plan first, ship the obvious scaffolding, dispatch
focused sub-task subagents for 8.2-8.8 in future ticks.

**Quality gates on `feature/v2.0.0-workshop` HEAD `26d498f`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **879 passed / 0 failed** (871 prior + 8 new
  storage scaffolding tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 23:30 PT — MODE C v2.0.0 Slice 7 SHIPPED (host fetch + JS binding + E2E)

Two-commit big vertical slice. `slab.fetch` is now a live, capability-gated,
timeout-bounded HTTP client surface for runtime plugins. Plugins can now
`await slab.fetch(url, init?)` and get back a web-Fetch-shaped Response
object (status, headers, text(), json()).

**Commits this tick:**
- `b5aaead` feat(plugins/runtime): host fetch executor — slab.fetch backend infra (v2.0.0 Slice 7.1-7.3)
- `3929d48` feat(plugins/runtime): live slab.fetch JS binding — host-mediated HTTP from plugins (v2.0.0 Slice 7.4)

**Slice 7.1-7.3 (commit 1) — backend (~750 LOC, 3 files):**
- New module `src-tauri/src/plugins/runtime/fetch.rs`: process-global
  `reqwest::Client` (rustls, 30s default timeout, 10-redirect cap,
  cookies off, 16 MiB body cap on both directions). `do_fetch` async,
  `response_to_js` builds the JS Response-like Object with `text()` /
  `json()` methods. URL parsing uses `reqwest::Url` re-export (no new
  direct dep on `url`).
- `actor.rs`: `RuntimeCmd::Fetch { request_id, request }` variant —
  intentionally carries only `Send` data; the `Persistent<Function>`
  resolve/reject callbacks live in a worker-local `PendingFetches`
  table keyed by request_id. Solves `RuntimeCmd: Send + Clone` while
  still routing settlement back into the actor's `Context`.
- `dispatch_fetch` helper: `tokio::Handle::try_current()` → use Tauri's
  runtime if alive, else build a single-threaded current-thread rt for
  unit tests. Wall-clock interrupt set fresh per dispatch.

**Slice 7.4 (commit 2, `3929d48`) — live JS binding (~500 LOC net):**
- `slab_global.rs::make_fetch`: builds `slab.fetch(url, init?)`. Pre-flight
  host parse + capability gate (sync throw on deny like every other
  `slab.*` surface), then mints a Promise via `rquickjs::Promise::new`,
  persists resolve/reject into the worker-local map, sends
  `RuntimeCmd::Fetch` at the actor's own channel.
- `dispatch_fetch` now drains `execute_pending_job` after settling so
  awaiting `.then` bodies run in the same tick (otherwise plugin code
  would starve until the next actor command).
- `HostBindings.{cmd_tx, pending_fetches}` plumbed through `run_actor`
  signature. Both `Option` to keep the ephemeral `enable_plugin` path
  valid; on that path `slab.fetch` returns an already-rejected Promise.
- Flipped `enable_plugin_reserved_surfaces_throw_with_slice_label` from
  `slab.fetch` (now live) to `slab.storage.get` (still Slice-8 placeholder).
- 3 new E2E tests (all green):
  - `slab_fetch_round_trip_resolves_with_body_via_actor` — one-shot
    `127.0.0.1` HTTP server in-process, plugin does `await slab.fetch(...)`,
    we observe `r.text()` via `slab.ui.notify`. End-to-end through
    reqwest + Promise + microtask drain.
  - `slab_fetch_throws_sync_when_net_not_granted` — capability gate.
  - `slab_fetch_throws_sync_when_host_not_in_allowlist` — allowlist enforcement.

**Quality gates on `feature/v2.0.0-workshop` HEAD `3929d48`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **871 passed / 0 failed** (848 prior + 19 fetch
  unit tests in commit 1 + 4 actor/binding tests in commit 2)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 23:01 PT — MODE C v2.0.0 Slice 6.7 + 6.8 (Slice 6 COMPLETE)

True end-to-end vertical slice: the actor system from 6.1–6.6 is now
wired into Slab's real plugin enable flow AND the PDF viewer's real
load/teardown path. `slab.document.{onOpen,onClose,getActive}` is no
longer "wired but no callers" — it's live every time a user enables a
runtime plugin and opens a PDF.

**Commits this tick:**
- `6bd171d` feat(plugins): Tauri document-event commands + actor lifecycle on enable (v2.0.0 Slice 6.7)
- `65588a5` feat(viewer): wire ReaderPanel into plugin document lifecycle (v2.0.0 Slice 6.8)

**Slice 6.7 (`6bd171d`) — backend + enable wiring (~420 insertions, 2 files):**
- New `slab_plugins_document_opened(path, registry)` and `_closed` Tauri
  commands. Each builds `DocumentEvent::from_path(path)` and calls
  `registry.broadcast(RuntimeCmd::Document{Opened,Closed}(ev))`. Both
  registered in `tauri::generate_handler!`.
- `slab_plugins_set_enabled` now takes `runtime_reg: State<PluginRuntimeRegistry>`.
  On enable for `[runtime]` plugins: spawn `PluginActor` with grants from
  `~/.slab/plugin-grants.toml` (deny-all default), insert into registry.
  On disable: `registry.remove(id)` → `LiveEntry::Drop` → worker
  Shutdown + join. Declarative-only plugins untouched.
- 5 new registry-level integration tests verify broadcast reaches real
  JS handlers across 2+ plugins, removed plugins are skipped,
  re-insertion shuts down the old actor cleanly, and open→close fires
  in order.

**Slice 6.8 (`65588a5`) — frontend hook (~54 LOC, 1 file):**
- `ReaderPanel.svelte` got `notifyPluginsDocumentOpened/Closed` helpers
  — fire-and-forget invokes guarded by `isInTauri()`; failures log to
  `console.debug`. `lastPluginPath` tracks which path has an
  outstanding "opened" so every close pairs with a real prior open.
- `loadBytes()` fires `_opened` after existing audit kickoffs (covers
  open, drag-drop, recents, post-OCR/Polyglot/decrypt — every load
  path funnels here).
- `tearDownDoc()` fires `_closed` BEFORE clearing pdfjs state (mirrors
  the actor's "clear active_doc before dispatch" ordering so plugin
  `onClose` handlers observe `getActive() === null`).
- onDestroy already calls tearDownDoc → tab-close & app-exit fire close.

**Quality gates on `feature/v2.0.0-workshop` HEAD `65588a5`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — **848 passed / 0 failed** (843 prior + 5 new
  registry/broadcast integration tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 22:23 PT — MODE C v2.0.0 Slice 6.5 (real actor runtime) + 6.6 finalize

Two commits, BIG vertical slice: `PluginRuntimeRegistry` finalized + Tauri-managed, AND the real Slice 6.5 actor body that turns `slab.document.{onOpen,onClose,getActive}` from "throws when called outside enable context" into a fully live, event-driven surface. `Persistent<Function>` callbacks now actually fire on `DocumentOpened`/`DocumentClosed` commands.

**Commits this tick:**
- `db3897b` feat(plugins): PluginRuntimeRegistry — process-global live actor handles (v2.0.0 Slice 6.6)
- `7b73329` feat(plugins/runtime): long-lived actor evaluates plugin + dispatches doc events (v2.0.0 Slice 6.5)

**Slice 6.5 (`7b73329`) — the meaty one (732 insertions, 99 deletions in 2 files):**
- Replaced placeholder `run_actor` with full Runtime+Context worker. `slab.document.*` is *live*.
- Init handshake via `sync_channel(1)`: spawn blocks until eval completes; syntax/throw/time/memory errors propagate to the host the same way `Runtime::enable_plugin` already does.
- Event loop dispatches each `Persistent<Function>` callback inside fresh `ctx.with` with a fresh interrupt deadline per batch.
- `active_doc` set BEFORE OnOpen dispatch, cleared BEFORE OnClose dispatch — handlers observe the doc that just opened / "no doc" intuitively.
- **Drop order strictly enforced** on both happy and error paths: `lifecycle.clear()` → `drop(ctx)` → `drop(rt)`. No rquickjs aborts.
- `ActorSharedState` only carries Send-safe state (registrations + logs). `SharedLifecycle`/`SharedActiveDoc` are worker-thread-local because `Persistent` wraps `*mut JSRuntime` (`!Send`).
- Per-callback try/catch in `dispatch_lifecycle` — one buggy handler doesn't poison the batch. Logged via `eprintln!`.
- Snapshot `Vec<Persistent>` under the lock then call — avoids reentrancy deadlock when a callback re-registers via `slab.document.on*`.
- `WorkerHandle::shared_state()` exposes `Arc<ActorSharedState>` for Slice 6.7 commands.
- 9 new contract tests cover: onOpen/onClose dispatch + payload, registration order, error isolation, getActive() inside handlers (both axes), Persistent shutdown safety, syntax/throw propagation, top-level log capture.

**Slice 6.6 (`db3897b`) — registry finalize:**
- `PluginRuntimeRegistry` `Mutex<HashMap<String, LiveEntry>>` with `insert` (replaces and Drop-shuts-down old handle), `remove`, `broadcast` (best-effort fan-out), `live_plugin_ids`, `len`/`is_empty`. 8 tests.
- `.manage(plugins::PluginRuntimeRegistry::default())` wired in `lib.rs::run()`.

**Quality gates on `feature/v2.0.0-workshop` HEAD `7b73329`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean (added one targeted `#[allow]` on `lifecycle::new_shared` with doc explaining intra-thread refcounting)
- `cargo test --lib` — **843 passed / 0 failed** (834 before 6.5; +9 actor contract tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Push:** in progress (see below).

---

## TICK 2026-05-18 21:46 PT — MODE C v2.0.0 Slice 6 plan + Slice 6.1 scaffolding

Slice 6 (`slab.document.{onOpen,onClose,getActive}` event dispatch) is the trickiest piece in the Workshop arc because rquickjs runtimes aren't `Send` across `Context::with`, so dispatch needs a per-plugin actor thread. Spent this tick writing the ship-ready implementation plan (10 sub-tasks, ~1030 LOC est, full drop-order safety notes), and landing the first sub-task as scaffolding so future ticks have a stable starting point.

**Commits this tick:**
- `17d2cc1` docs(plans): v2.0.0 Slice 6 implementation plan — document lifecycle events
- `5941add` feat(plugins/runtime): RuntimeCmd + DocumentEvent actor types (v2.0.0 Slice 6.1)

**Slice 6 plan (`docs/plans/2026-05-18-v2.0.0-workshop-slice-6.md`, 1059 lines):**
- Architecture: per-plugin actor thread owns a long-lived `rquickjs::Runtime` + `Context`. Host sends `RuntimeCmd` over `crossbeam-channel`. `Persistent<Function>` is `Send + 'static` but the runtime is pinned to its spawn thread — that's why actors, not a global runtime.
- 10 sub-tasks: 6.1 types ✅, 6.2 PluginActor skeleton, 6.3 JS-side `onOpen/onClose` (stores `Persistent`), 6.4 `getActive()` reads shared `ActiveDoc`, 6.5 real `run_actor` body (init handshake + event loop + drop-order), 6.6 `PluginRuntimeRegistry` Tauri state, 6.7 Tauri commands, 6.8 frontend integration in `+page.svelte`, 6.9 10+ contract tests, 6.10 quality gates.
- Critical invariant called out explicitly: **`Persistent` handles MUST be cleared before the owning `Runtime` drops** or rquickjs aborts. Actor exit path: `lifecycle.clear()` → `drop(ctx)` → `drop(rt)`.

**Slice 6.1 (`5941add`) — scaffolding:**
- New `src-tauri/src/plugins/runtime/actor.rs` (141 LOC): `RuntimeCmd { DocumentOpened, DocumentClosed, Shutdown }`, `DocumentEvent { path, name }` with `from_path()` deriving name from `file_stem()`.
- 7 unit tests cover the stem-derivation edge cases (no extension, multi-dot like `archive.tar.gz` → `archive.tar`, dotfile `.bashrc` → `.bashrc`, root path → empty), `matches!(Shutdown)`, and a compile-time `Send + Clone + 'static` assertion. All pass.

**Quality gates on HEAD `5941add`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **812 passed / 0 failed** (805 prior + 7 new actor tests)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged)

**Why a plan tick instead of full Slice 6:** Slice 6 touches lifetimes (`Persistent<Function<'static>>`), threading (one actor thread per plugin), and unsafe-adjacent drop ordering (`Persistent` outliving the runtime aborts the process). Writing 600+ LOC of that without a written design first would mean revising it in-flight. Plan-first means future ticks can dispatch focused sub-task subagents with zero ambiguity.

**Push:** in progress (see below).

---

Big vertical slice: typed manifest surface + standalone modal component + parent panel wiring + permission management actions — 3 commits, ~850 LOC net.

**Commits this tick:**
- `407e2c3` feat(plugins/ts): Manifest.runtime + ManifestCapabilities + consent i18n (v2.0.0 Slice 5a)
- `7b09f0f` feat(plugins/ui): PluginConsentModal component (v2.0.0 Slice 5b)
- `ad1d0d0` feat(plugins/ui): wire consent modal into enable flow + permission actions (v2.0.0 Slice 5c)

**Slice 5a (`407e2c3`) — typing + i18n prep:**
- `src/lib/plugins.ts`: added `Manifest.runtime: RuntimeManifest | null` mirror, plus new `RuntimeManifest` (entry/sha256/capabilities) and `ManifestCapabilities` (fs/net/ui/beacon + two allow-lists) interfaces. `ManifestCapabilities` is kept distinct from `PluginGrants` so the modal can enforce "user can dial down, not up" by comparing the two.
- `src/lib/i18n/en.json`: 36 new strings under `plugins.consent.*` + `plugins.permissions.*`. Other locales fall back to en.

**Slice 5b (`7b09f0f`) — modal component (510 lines):**
- New `src/lib/components/PluginConsentModal.svelte`. Pure presentational; no Tauri knowledge.
- Header: 🔐 icon + "Permissions for <name>" + default-deny subtitle.
- Per-cap rows (fs/net/ui/beacon) — render only when declared bound is non-`none`. Segmented radio with values from `none` up to declared max. 11px declared-hint shows what the plugin asked for.
- Allow-lists (fs paths, net hosts) surfaced read-only when the axis is non-`none`. Collapsed when set to `none`. Editing them lands in a follow-up slice.
- Helper `allowedValues<T>(order, max)` computes per-axis lattice prefix. `as const` tuples so the lattice is type-checked.
- On approve: scrubs allow-lists when axis is `none` (keeps grants file tidy).
- Esc + backdrop = Deny. Approve focused on mount.
- `noRuntime` short-circuit branch for declarative-only plugins (defensive — parent skips entirely for those).

**Slice 5c (`ad1d0d0`) — panel wiring (264 LOC added):**
- `toggleEnabled(p, true)` gates on `getPluginGrants(id).has_decision === false` for `[runtime]` plugins. First-enable → consent modal pre-fills with manifest's declared bounds (max-useful default).
- `ConsentModalState` carries plugin + optional `initial` grants + optional `onResolve` callback. Two flows: first-enable (initial=null, onResolve resumes enable) vs re-review (initial=current grants, onResolve=null).
- Approve: `setPluginGrants(id, grants)` + success toast + resume enable if pending.
- Deny: first-enable path persists `emptyPluginGrants()` (so we don't re-prompt) + info toast. Re-review path just closes without writing.
- Contrib drilldown gets a new "Permissions" row (only when `manifest.runtime !== null`) with "Review permissions" + "Reset permissions" link-buttons. Reset → `resetPluginGrants(id)` + toast explaining next-enable will re-prompt.
- `.permissions-row` CSS with dashed top-border for visual separation.

**Quality gates on `feature/v2.0.0-workshop` HEAD `ad1d0d0`:**
- `cargo fmt --all -- --check` — clean
- `cargo clippy --lib --all-targets -- -D warnings` — clean
- `cargo test --lib` — **805 passed / 0 failed** (unchanged from Slice 4 — Slice 5 is frontend-only)
- `pnpm check` — 0 errors, 35 pre-existing warnings (unchanged in count + identity)

**Push:** in progress (see below).

---

## NEXT TICK PLAYBOOK

### Step 1 — MODE C continue v2.0.0 "Workshop" Slice 9 (SDK npm package)

Slice 8 DONE. Slice 9 ships a typed npm SDK so plugin authors get
IntelliSense on the `slab` global without copying types around. The
workshop master plan is at `docs/plans/2026-05-18-v2.0.0-workshop.md`.

- Pick directory shape: `sdk/slab-plugin-sdk/` in repo root (matches
  the marketplace-seed pattern).
- Build the `.d.ts` from the live TS types in `src/lib/plugins.ts` —
  no manual maintenance.
- Author + ship to npm as `@slab/plugin-sdk` (Sanjay needs to own the
  npm org first; flag this if blocked).

### Step 2 — Or skip ahead to Slice 10 (sample plugin) for faster demo

A real sample plugin exercising `slab.storage` + `slab.fetch` + the
document lifecycle is the highest-signal demo for v2.0.0. Use this if
the npm namespace is blocked.

### Step 3 — Watch for sibling subagent activity

Sibling subagents can touch `/tmp/msg.txt`. Always overwrite right
before commit. Sibling subagents also can `git checkout main`
unexpectedly — verify branch at start of tick.

---

## ROADMAP

### v0.8.1 → v1.6.0 — RELEASED (see git history)
### v1.7.0 "Study Mode" 🎓 — **RELEASED 2026-05-18**
### v1.8.0 "Glossary" 📖 — **RELEASED 2026-05-18**
### v1.9.0 "Voice Mode" 🔊 (TTS-first) — **RELEASED 2026-05-18**
### v1.9.1 "Beacon Voice Mode: Listen" 🎙 — **RELEASED 2026-05-18**
### v1.9.2 "Voice Mode: Polish" — **RELEASED 2026-05-18** (6 assets on GH)
### v1.9.3 "Voice Mode: Windows-native" — Windows WASAPI recorder via cpal (T6 from v1.9.2 plan, plus real impl)
### v2.0.0 "Workshop" — TypeScript Plugins (rquickjs). **In flight on `feature/v2.0.0-workshop`. Slices 1-8 done (8/12). Plan at `docs/plans/2026-05-18-v2.0.0-workshop.md`; Slice 8 sub-plan at `docs/plans/2026-05-19-v2.0.0-workshop-slice-8.md`.**

---

## TICK MODE DECISION TREE

```
1. Read STATE.md
2. RELEASE_PENDING in STATE.md + CI run → MODE B (poll CI; if green, gh release create)
3. Any feature/* branch with STATUS: DONE → MODE A (merge to main + tag + push)
4. No pending release, no DONE branch → MODE C (DEVELOP — ship a vertical slice)
```

---

## POST-v1.9 ROADMAP REMINDERS

**v1.9.3** — Windows-native STT (WASAPI via cpal). Real implementation, not the `todo!()` scaffold from v1.9.2 T6. Cargo feature `windows-stt`. ~3-4 commits + integration tests.

**v2.0.0 "Workshop" slice progress:**
- ✅ Slice 2 (manifest schema + hash-pinned loader) — shipped 2026-05-18
- ✅ Slice 1 (rquickjs embedding + sandboxed console) — shipped 2026-05-18
- ✅ Slice 3 (capability grants backend + enforce()) — shipped 2026-05-18
- ✅ Slice 4 (`slab` global + lifecycle + Tauri grant cmds + TS bindings) — shipped 2026-05-18
- ✅ Slice 5 (Cabinet consent modal + enable integration) — shipped 2026-05-18
- ✅ Slice 6 (document lifecycle events) — **COMPLETE 2026-05-18** (6.1-6.6 actor system, 6.7 Tauri commands + enable-flow spawn/teardown, 6.8 ReaderPanel hook)
- ✅ Slice 7 (`slab.fetch` shim — host-mediated HTTP) — **COMPLETE 2026-05-18** (7.1-7.4: process-shared reqwest client + Promise-bridging actor + JS binding + E2E)
- ✅ Slice 8 (`slab.storage.*` — per-plugin KV) — **COMPLETE 2026-05-19** (8.1 module+schema, 8.2-8.3 kv CRUD + quotas + 16 unit tests, 8.4 HostBindings plumbing + spawn_inner test seam, 8.5 6 JS bindings, 8.6 7 E2E tests + actor microtask pump fix, 8.7 sentinel flip)
- ⏭ Slices 9-12 — see plan doc

Slices in target order: 1→rquickjs+console ✅, 2→manifest schema ✅, 3→capability backend ✅, 4→`slab` global + lifecycle ✅, 5→Cabinet consent modal ✅, 6→event dispatch ✅, 7→fetch shim ✅, 8→storage ✅, 9→SDK npm pkg, 10→sample plugin+docs, 11→AI provider registration, 12→release.

**v2.1.0 candidates (post-Workshop):**
- **Forge** — author-signed plugins. Wants 10+ plugins in curated index before considering (Sanjay's flag).
- **Slab CLI** — `slab plugin install <url>`.
- **Plugin author cookbook** — recipes for common plugin patterns.

**Parked items (pre-existing):**
- `docs/screenshots-v1.3.1/` working copy in repo root — harmless, can `rm -rf` someday.
- CommandPalette DETACHABLE_PANELS drift — missing citations/study/glossary entries pre-existed v1.9.0; voice was added but the other three remain. Quick cleanup tick someday.
- Sanjay's external action for v1.4.1: create `Sanjays2402/slab-plugins` GH repo, drop seed files from `docs/marketplace-seed/`, sign the hello-slab plugin, post first real `index.json`.
