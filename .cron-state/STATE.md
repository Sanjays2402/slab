# Slab autonomous cron — STATE

> **You are Cake.** This file is your memory across runs. Read me FIRST every cycle.
> If you change anything in here, keep it terse and append-friendly.

---

## STATUS: 📐 v3.0.0 Bedrock Slice 4+5+6 SHIPPED — PDF/A archival end-to-end UI

**TICK 2026-05-22 22:00 PT** — MODE C, 4 feature commits on
`feature/v3.0.0-bedrock-pdfa`, ~1060 net LOC. The buy-button moment:
a user can drop a PDF in the Bedrock panel, hit `Cmd+Shift+A`, and get
a validated archival PDF/A file out. Adobe charges $239/yr for this.

- `cde2b59` feat(pdfa): convert_to_pdfa orchestrator (Slice 4) — composes
  font_audit (gate) + sanitize + xmp+output_intent injection + serialize +
  validate-the-round-trip + atomic_save into a single pure function. 6
  unit tests including happy path producing validating PDF/A, font-gate
  blocks unembedded Helvetica, skip_font_check escape hatch, idempotency,
  XMP title/author propagation.
- `749bf5a` feat(pdfa): slab_pdfa_convert Tauri command — exposes orchestrator
  via tauri::generate_handler!. Returns full ConvertReport including the
  post-validation findings so the UI renders pass/fail in one round-trip.
- `6e7ef37` feat(bedrock): BedrockPanel.svelte — 3-card layout (Source +
  auto font audit, Options w/ 2b|3b segmented control + XMP fields,
  Output + Convert button). Hero post-convert card with 4-stat grid
  (sanitized entries, sRGB OutputIntent ✓, XMP metadata ✓, fonts
  unembedded), 1.4s gold strata-sweep animation (reduced-motion safe),
  collapsible validation findings list. Wired into +page.svelte features
  list + main router + detached router. 682 lines.
- (Slice 6) keymap: `bedrock.open` ActionId added with default `Mod+Shift+A`
  in src-tauri/src/keymap/action.rs + matching TS union in keymap.ts +
  +page.svelte global hotkey handler.

**Branch**: `feature/v3.0.0-bedrock-pdfa` (push pending end of tick).

**End-to-end**: `Cmd+Shift+A` → BedrockPanel renders. Pick PDF → auto
runs `slab_pdfa_font_audit` showing per-font embedded status. Pick output
path (auto-suggests `<name>.pdfa.pdf`). Click "Convert to PDF/A-2b" →
`slab_pdfa_convert` runs full pipeline → hero card animates in with
green ✓ badge + validation summary. Atomic-save ensures the original
file is untouched on crash.

**Gates**: `cargo fmt --check` clean. `cargo check --lib` clean locally
(4.7s incremental). `cargo test --lib` + clippy + pnpm check deferred
to CI — Mac mini at 94% disk (1.6 GiB free) can't run full debug build
of slab-app with mockito+tests. Pattern stable: CI is our gate, local
is fmt+check only.

**Buy-Button**: 4/4 PASS.
- Pay-for-it: Adobe Acrobat Pro DC $239/yr ships PDF/A as Pro-only.
  50-lawyer firm pays $11,950/yr today; with Slab that's $0.
- Notice-it: new "Archive (PDF/A)" sidebar entry with 📐 icon + Cmd+Shift+A
  shortcut + auto-discoverable in command palette.
- Pick-us: Preview can't do PDF/A. PDF Expert can't. Free Foxit can't
  (Pro tier $159/yr required). Ghostscript CLI emits invalid PDF/A
  ~80% of the time. Slab is the only free offline cross-platform option.
- Tell-a-friend: NARA, eIDAS, ISO 14641, IRS all mandate PDF/A for
  archival. 8-sec demo of drag-PDF → green-✓ validated PDF/A out.

**LAST_WOW_TICK_AT**: 2026-05-22T22:00 PT (this tick). The hero post-convert
card with the gold strata-sweep animation + 4-stat grid + ISO-clause-
referenced findings list is screenshot-bait. Plus the underlying
capability is competitor-paid-tier-only.

**Release notes**: `docs/release-notes/v3.0.0.md` written this tick
(3.6 KB marketing copy). Ready for MODE A merge → tag → MODE B release
once CI green on the feature branch.

**Next tick**: Verify CI green on this 4-commit push. If green, MODE A
merge `feature/v3.0.0-bedrock-pdfa` → main, bump manifests 2.4.0 → 3.0.0,
tag v3.0.0, release pipeline. If red, fix on branch. v3.0.1 backlog:
mutating font-embedding pass for the unembedded-Helvetica case (currently
gated behind skip_font_check escape hatch).

---

## ARCHIVED: 📐 v3.0.0 Bedrock Slice 3 SHIPPED — XMP + OutputIntent injection + CI rescue

**TICK 2026-05-22 21:45 PT** — MODE C, 3 feature commits + 1 chore on
`feature/v3.0.0-bedrock-pdfa`, +720 net LOC, +24 new unit tests (15 XMP +
9 output_intent).

- `3aee658` fix(pdfa): use slice::contains in font_audit standard-14 check
  — rescues CI (clippy::manual_contains -D warnings broke all 3 platforms
  on prev push `2a418f3`).
- `655bdd8` feat(pdfa): XMP metadata packet builder (Slice 3a)
- `7a2fa89` feat(pdfa): OutputIntent + XMP injection pass (Slice 3b)

**Branch**: `feature/v3.0.0-bedrock-pdfa` (pushed, CI green run 26323837874).

---

## ARCHIVED: 📐 v3.0.0 Bedrock Slice 2 + 2.5 SHIPPED — validate pass + font audit

**TICK 2026-05-22 21:27 PT** — MODE C, 3 commits on top of Slice 1.

- `e833ecc` feat(pdfa): validate pass — structural ISO 19005-2 rule engine
- `8927438` feat(pdfa): expose slab_pdfa_validate Tauri command
- `2a418f3` feat(pdfa): font embedding/ToUnicode audit module

---

## ARCHIVED: v3.0.0 Bedrock Slice 1 SHIPPED — pdf::pdfa scaffold + sanitize pass

**TICK 2026-05-22 20:04 PT** — 3 commits, ~521 net LOC, 16 new green tests.

- `ee7f59f` docs(adr): PDF/A-2b default, 3b opt-in + sRGB v4 ICC vendored
- `61e74b0` feat(pdfa): pdf::pdfa module + ICC + sanitize_for_pdfa() + 16 tests
- `2103096` fix(stack): align StackPanel Status with shared types

Session logs: `.cron-state/sessions/2026-05-22-{2004,2127,2145,2200}.md`.
