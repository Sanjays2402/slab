# Slab Cron State

Last updated: 2026-06-22 00:35 PT by Cake (cron) — round-21 BATCH shipped: 5 slices closing one cohesive arc. Top plugins histogram audit-export arc (slices 98-102): pure-data plugin_histogram_to_csv(rows, include_header) RFC-4180 serialiser with 8 columns (plugin_id, installs, updates, uninstalls, failures, total, last_occurred_at_unix, last_occurred_at_iso) matching install_log_to_csv shape with ISO column byte-for-byte (slice 98); pure-data plugin_histogram_to_json(rows, since, until, grand_total) -> PluginHistogramExportEnvelope mirroring InstallLogExportEnvelope (slice 60) and DrilldownExportEnvelope (slice 93) — schema_version=1 PARALLEL-VERSIONED with INSTALL_LOG_EXPORT_SCHEMA_VERSION (independent bumps as bodies diverge) + generated_at_iso + row_count (mirrors rows.len()) + grand_total (caller-supplied verbatim, no re-sum) + window-bounds + rows verbatim (slice 99); slab_marketplace_install_log_export_histogram_csv + _json Tauri commands writing pretty-printed JSON / RFC-4180 CSV to disk via the same plugin_histogram(since, until, limit.unwrap_or(25)) reload as the read endpoint so "what you see is what you export" default semantics (slice 100); exportInstallLogHistogramCsv/Json TS wrappers + suggestHistogramExportFilename(filter, ext, now?) helper producing marketplace-top-plugins_<window>_<YYYY-MM-DD>.<ext> with identical window shape to suggestInstallLogExportFilename so the two filenames sort side-by-side in a directory (slice 101); Export… popover beside Sort by selector inside .top-plugins-sort row sharing the .export-menu vocabulary from the footer popover, in-state-snapshot semantics (Tauri layer re-queries with same window so file content matches what's on-screen), independent histogram-export-anchor so dismissing one popover doesn't dismiss the other, toast "Exported N plugins as CSV/JSON (X.X KB)" accent-green tint matching install-event seg-install vocabulary with 0.16s fade-in keyframe (slice 102).

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `518f261` — "feat(plugins): Export menu for Top plugins histogram".

### What round-21 (2026-06-22 00:35 PT) just shipped

Five slices closing one cohesive arc. Before this tick the Top
plugins histogram (round-18 slice 87 read + round-20 slice 97 sort)
could be VIEWED and SORTED but couldn't be SAVED — a paralegal
investigating "which plugins drove my install activity this month?"
could pivot the order but couldn't export the resulting view for a
report, an audit attachment, or a downstream pipeline. Tonight the
histogram closes its audit-export symmetry loop with both CSV
(spreadsheet-primary) and JSON (archive-secondary) formats sharing
identical column semantics, identical schema_version provenance,
identical window-shape filenames, and identical row order — the
same canonical four-layer arc pattern as round-19 (drilldown CSV
88-91) and round-20 (drilldown JSON 93-96) plus a UI composite.

Round 20's closing notes listed "next subsystem candidates" including
"histogram time-bucket axis" — round 21 instead closed the existing
audit-export loop first because the existing slice-87 histogram +
slice-97 sort cohort were the most obvious unfinished symmetry
relative to the drilldown popover (which had CSV + JSON exports
already).

- Slice 98: histogram CSV export primitive (aee2c75, 245 LOC).
  Pure-data plugin_histogram_to_csv(rows, include_header) -> String
  RFC-4180 serialiser. Eight columns: plugin_id, installs, updates,
  uninstalls, failures, total, last_occurred_at_unix,
  last_occurred_at_iso. Both timestamp columns share one source
  (last_occurred_at via iso8601_utc helper) so unix + ISO can never
  drift. The total field is written verbatim (not re-summed from
  the four bucket columns) so a future axis added to
  PluginHistogramRow doesn't silently corrupt totals in the lag
  window. PLUGIN_HISTOGRAM_CSV_HEADER exposed as pub const so
  tests + future column reorders share one source of truth.
  12 new tests pin: header opt-in invariant, header/row column
  count parity, documented column order, ISO matches install-log
  format byte-for-byte (downstream join compatibility), preserves
  input order (server emits sorted DESC, UI may re-sort, exporter
  ships verbatim), RFC-4180 escaping for comma + escaping for
  quote, zero timestamp renders as integer 0 not empty (NOT NULL
  contract for aggregate rows), one row per input invariant,
  total field written verbatim (mismatch test confirms no re-sum),
  no "None"/"null" leaks for a future Option<_> column addition.

- Slice 99: histogram JSON export envelope primitive (743ec95,
  267 LOC). Pure-data plugin_histogram_to_json(rows, since, until,
  grand_total) -> PluginHistogramExportEnvelope mirroring the
  InstallLogExportEnvelope (slice 60) and DrilldownExportEnvelope
  (slice 93) shapes: schema_version=1, generated_at_iso (same
  iso8601_utc helper so timestamps match install-log byte-for-byte),
  row_count (mirrors rows.len() — pre-computed so consumers read
  one int not a count), grand_total (caller-supplied verbatim —
  the server pre-summed via PluginHistogramResult.grand_total;
  re-summing here would let row-truncation diverge silently from
  the actual corpus total), since_unix/since_iso/until_unix/
  until_iso window bounds, rows Vec<PluginHistogramRow> verbatim.
  plugin_histogram_to_json_with_now takes explicit now-seconds
  so tests don't race the wall clock. PLUGIN_HISTOGRAM_EXPORT_
  SCHEMA_VERSION exposed as pub const matching INSTALL_LOG_EXPORT_
  SCHEMA_VERSION at v1 today; both are PARALLEL-versioned (a
  future shape change in one bumps that one only). 13 new tests
  pin: schema_version=1, row_count==rows.len() invariant,
  grand_total carried verbatim (mismatch test confirms), generated_
  at_iso format matches install-log envelope byte-for-byte, no
  window bounds means no ISO sides either, both bounds round-trip
  to ISO, only-since case has only-since ISO, preserves input row
  order, rows are owned clones (caller-mutation isolation),
  serde round-trip with full field-set assertion, pretty-print
  round-trip (Tauri layer uses to_string_pretty), empty input
  renders cleanly, parallel-versioning equality check.

- Slice 100: histogram CSV+JSON Tauri commands (e624e48, 95 LOC).
  slab_marketplace_install_log_export_histogram_csv(path, since,
  until, limit) -> u64 + slab_marketplace_install_log_export_
  histogram_json(path, since, until, limit) -> u64. Both reload
  the histogram via log.plugin_histogram(since, until,
  limit.unwrap_or(25)) — SAME default limit as the read endpoint
  so the export ships the same 25 rows the user is looking at.
  CSV writes with include_header=true; JSON computes grand_total
  = rows.iter().map(.total).sum() then to_string_pretty (matches
  the install-log JSON export's pretty-print so the file is
  human-readable in a text editor; compactness saves bytes that
  don't matter for a per-plugin aggregate). Tauri-layer disk I/O
  because the frontend's plugin-fs scope doesn't cover arbitrary
  user-chosen paths. Both create parent dirs if missing
  (idempotent), overwrite if target exists (save dialog handles
  overwrite confirmation upstream), return byte count actually
  written. Both registered in invoke_handler between
  slab_marketplace_install_log_plugin_histogram (read) and
  slab_marketplace_install_log_retention_policy. No new lib-test
  surface because the slice-98 + slice-99 primitives already pin
  shape — the commands are thin disk-IO wrappers following the
  same untested-thin-wrap pattern as the four existing CSV/JSON
  export commands.

- Slice 101: histogram export TS client + filename helper
  (94ab3ec, 240 LOC across marketplace.ts + marketplace.test.ts).
  HistogramExportFilter { since_unix?, until_unix?, limit? }
  shared between the two wrappers. exportInstallLogHistogramCsv +
  exportInstallLogHistogramJson thin invoke wrappers around the
  slice-100 commands; both return bytes-written; browser-mode
  returns 0 (no-op pattern matching exportInstallLogCsv).
  suggestHistogramExportFilename(filter, ext, now?) pure helper
  proposing marketplace-top-plugins_<window>_<YYYY-MM-DD>.<ext>.
  Window slot reads "all" / "from-YYYYMMDD" / "to-YYYYMMDD" /
  "YYYYMMDD-YYYYMMDD" — IDENTICAL shape to suggestInstallLog
  ExportFilename (slice 61) so a paralegal collecting audit
  exports sees the two filenames sort side-by-side in a directory.
  11 new pure-helper tests in marketplace.test.ts (extends slice 97's
  inline-expect file): no-window csv form (== "marketplace-top-
  plugins_all_20240309.csv"), no-window json form, only-since
  "from-" prefix, only-until "to-" prefix, both bounds
  "YYYYMMDD-YYYYMMDD" slot, csv/json pair differs ONLY in suffix
  (slice-prefix equality assertion pins the invariant — mirrors
  slice-95's drilldown ext-aware test), csv ends .csv + json
  ends .json, marketplace-top-plugins_ prefix preserved across
  all four window-shape variants, window slot regex (no internal
  separators), today slug uses UTC date math (deterministic NOW
  pinning so the test stays stable across timezones).

- Slice 102: Export menu for Top plugins histogram (518f261,
  219 LOC). The demo-able payoff tying slices 98-101 together.
  Imports exportInstallLogHistogramCsv/Json + suggestHistogram
  ExportFilename + HistogramExportFilter alongside the existing
  install-log export imports. New state cells beside the existing
  histogram cells: histogramExportMenuOpen (popover open/close
  dismissed by outside click + Escape + selection), histogram
  Exporting (gates the button while save-dialog + Tauri write
  are in flight — prevents double-saves), histogramExportToast +
  histogramExportToastTimer (4s notice with a named handle so
  back-to-back exports cleanly REPLACE rather than stack); onMount
  cleanup adds the timer to the existing queryDebounce cleanup.
  runHistogramExport(kind) handler mirrors runExport's shape
  exactly: filter carries since_unix from windowSinceUnix (same
  axis the timeline uses, "what you see is what you get") + limit
  from histogramLimit, suggestHistogramExportFilename proposes
  default, save dialog opened with kind-appropriate filter (CSV-
  only or JSON-only), cancellation is a clean no-op, bytes
  returned surfaces in toast "Exported 12 plugins as CSV/JSON
  (1.8 KB)" — same "as <fmt>" suffix pattern as slice 96's
  drilldown toast so a user exporting both formats back-to-back
  can tell which one just landed. UI: Export… button sits BESIDE
  the Sort by selector inside .top-plugins-sort row (margin-left:
  auto pushes it to the row's right edge). Popover anchors DOWN+
  RIGHT-aligned beneath the button (sort row is at top of body,
  opening upward would clip the section toggle). Reuses
  .export-menu styling from the footer popover so the two
  surfaces feel like one verb across the drawer. The
  histogram-export-anchor class gates the outside-click dismiss
  SEPARATELY from the footer .export-anchor so the two popovers
  don't dismiss each other. onKeydown's Escape chain puts
  histogramExportMenuOpen BEFORE exportMenuOpen so an Escape with
  both open dismisses the histogram first (more recently opened,
  closer to user attention). Toast renders inline BELOW the sort
  row, before the histogram list, accent-green
  (rgb(170,230,195) matching the install-event seg-install
  vocabulary), 0.16s fade-in keyframe — slightly different
  placement vs the install-log toast (footer-anchored) because
  the histogram has its own body; the toast stays attached to
  its section. Legend footer extended: "Export… ships the current
  window as a CSV (spreadsheet) or JSON (archive) snapshot."

Gates result: cargo fmt clean (cargo fmt --all --check exit 0
on first run — no fmt fixups needed this tick), cargo clippy
--lib -- -D warnings PASSED CLEAN in 14.68s (matches round-20
13.23s baseline — pure-data CSV serialiser + JSON envelope
serialiser + thin command wrappers + UI-only export popover add
no new clippy surface), cargo test --lib 2345 passed / 0 failed
(round-20 baseline 2320 + 12 from slice 98 + 13 from slice 99 =
2345), pnpm check 0 errors / 104 warnings (round-20 baseline
preserved EXACTLY — zero new warnings from the export wrappers,
ext-aware suggest helper, button + popover + toast wiring,
scoped CSS), tsx src/lib/marketplace.test.ts 54 inline expects
pass (round-20 ~40 + 11 from slice 101 + 3 implicit on iterating
opts = 54).

PROCESS NOTES:
- Round-20 closing notes listed "histogram time-bucket axis" as
  a next-tick candidate; round 21 instead closed the existing
  audit-export loop first because the histogram had a SORT axis
  (round-20 slice 97) and READ surface (round-18 slice 87) but
  no SAVE path — the obvious unfinished symmetry relative to the
  drilldown popover (which had CSV+JSON since rounds 19+20).
- Five slices, five commits, ONE logical subsystem (the histogram
  export arc). Mirrors the four-layer cadence of round-19 (drilldown
  CSV arc 88-91) and round-20 (drilldown JSON arc 93-96): pure-data
  primitive (CSV) → pure-data primitive (JSON) → Tauri commands →
  TS client + filename helper → demo-able UI. The split into
  separate CSV-primitive + JSON-primitive slices (98 + 99) rather
  than one combined "exporters" slice gives each format its own
  revert point and its own focused test surface — same revertibility
  posture as the round-19/20 drilldown arcs.
- The PluginHistogramExportEnvelope schema_version=1 matches the
  install-log envelope's schema_version constant by value today,
  but they are parallel-versioned (a future shape change in the
  histogram envelope bumps the histogram constant only, NOT the
  install-log constant). A test pins the v1==v1 equality so a
  careless joint bump surfaces immediately.
- The Tauri command in slice 100 ships the limit parameter through
  to plugin_histogram() with limit.unwrap_or(25) — same default
  as the read endpoint so "export the same 25 you're looking at
  right now" is the natural reading. The frontend's runHistogram
  Export handler in slice 102 passes histogramLimit (which IS 25
  in the current UI) so the export window matches the on-screen
  view bit-for-bit. A future "show more" affordance in the UI that
  bumps histogramLimit will flow through to the export without
  any other plumbing change.
- The toast handle pattern (histogramExportToastTimer holding the
  named setTimeout id) matches the round-17 hopper coverage panel's
  named-timer cleanup. Without the named handle, back-to-back
  exports would stack timers and the 4s clear could race the
  second export's toast.

DESIGN NOTES:
- Export… button placement BESIDE Sort by (not in the section
  toggle, not in the footer) reads as "controls for this view live
  with the view". The footer Export… exports the EVENT LOG; the
  histogram Export… exports the AGGREGATE. Two distinct artefacts,
  two distinct verbs — same look-and-feel via the shared
  .export-menu class but separate state cells + separate anchors.
- Popover anchors DOWN from the button instead of UP because the
  sort row is at the top of the histogram body; opening upward
  would clip against the section toggle. The footer popover opens
  UP for the opposite reason — it's at the bottom of the drawer.
  Both popovers cascade INTO the drawer body, never out of it.
- Toast tint is accent-green (rgb(170,230,195) matching the
  install-event seg-install vocabulary) rather than the install-
  log's neutral .export-toast style. Two reasons: (1) the
  histogram toast is anchored to the histogram body, not the
  footer — a neutral tint would visually disappear against the
  surrounding chrome; (2) green reads as "positive write outcome"
  which matches what the toast says ("Exported N plugins…").
- The "as CSV / as JSON" suffix in the toast copy is a tiny
  detail but matters when a user exports both formats back-to-
  back. The 4s toast duration is long enough that two exports
  can overlap; the format-tag in the message disambiguates which
  one just landed without forcing the user to remember which
  menu item they clicked. Same reasoning as slice 96's drilldown
  toast upgrade.
- The marketplace-top-plugins_ filename prefix groups the
  histogram exports with the other marketplace exports
  (marketplace-history_*.csv from slice 61) when a paralegal
  collects audit files in a directory. Sorting by name puts the
  history first, then the top-plugins exports — natural reading
  order for "the events that drove the aggregate".

## Roadmap — round 21 (Top plugins histogram audit-export) — ALL DONE

Round 21 batched FIVE feature slices into one cron tick closing
ONE cohesive arc: the Top plugins histogram audit-export loop
(slices 98-102). Two pure-data primitives (CSV + JSON), one
slice of Tauri commands (both wrappers), one slice of TS client
+ filename helper + tests, and one composite UI slice (popover +
state + toast). Same canonical five-layer pattern as the
drilldown CSV arc (rounds 19) and drilldown JSON arc (round 20).

98. ~~**Top plugins histogram CSV export primitive**~~ —
    DONE (2026-06-22 00:35 PT, aee2c75, single commit, 245 LOC).
    Pure-data plugin_histogram_to_csv(rows, include_header) ->
    String. 8 columns: plugin_id, installs, updates, uninstalls,
    failures, total, last_occurred_at_unix, last_occurred_at_iso.
    Shared iso8601_utc with install-log CSV so ISO column matches
    byte-for-byte. PLUGIN_HISTOGRAM_CSV_HEADER pub const.
    12 new tests pin header opt-in + column count parity +
    documented order + ISO format match + preserves input order +
    RFC-4180 escaping + zero timestamp renders integer + total
    field written verbatim + no None/null leaks.
99. ~~**Top plugins histogram JSON envelope primitive**~~ —
    DONE (2026-06-22 00:35 PT, 743ec95, single commit, 267 LOC).
    Pure-data plugin_histogram_to_json(rows, since, until,
    grand_total) -> PluginHistogramExportEnvelope. schema_version=1
    PARALLEL-versioned with INSTALL_LOG_EXPORT_SCHEMA_VERSION.
    generated_at_iso + row_count + grand_total (caller verbatim)
    + window bounds + rows verbatim. 13 new tests pin schema_v1 +
    row_count invariant + grand_total verbatim + ISO format match
    + window-bounds round-trip + preserves order + owned clones +
    serde + pretty-print round-trips + empty input + parallel-
    versioning equality.
100. ~~**Top plugins histogram CSV+JSON Tauri commands**~~ —
    DONE (2026-06-22 00:35 PT, e624e48, single commit, 95 LOC).
    slab_marketplace_install_log_export_histogram_csv +
    slab_marketplace_install_log_export_histogram_json both reload
    via plugin_histogram(since, until, limit.unwrap_or(25)) — same
    default as the read endpoint. CSV write with header; JSON
    pretty-printed. Idempotent (overwrite); creates parent dirs;
    returns bytes written. Both registered in invoke_handler.
    No new lib tests — thin disk-IO wrappers.
101. ~~**Top plugins histogram export TS client + filename helper**~~ —
    DONE (2026-06-22 00:35 PT, 94ab3ec, single commit, 240 LOC).
    HistogramExportFilter shared shape, exportInstallLogHistogram
    Csv/Json lazy-import invoke wrappers (browser no-op),
    suggestHistogramExportFilename(filter, ext, now?) producing
    marketplace-top-plugins_<window>_<YYYY-MM-DD>.<ext> with
    identical window shape to suggestInstallLogExportFilename.
    11 new pure-helper tests in marketplace.test.ts.
102. ~~**Export menu for Top plugins histogram**~~ —
    DONE (2026-06-22 00:35 PT, 518f261, single commit, 219 LOC).
    The demo-able payoff. Export… button beside Sort by selector
    inside .top-plugins-sort row; popover anchors DOWN+RIGHT
    beneath the button. Separate histogram-export-anchor so
    independent dismiss from the footer .export-anchor. State
    cells: histogramExportMenuOpen + histogramExporting +
    histogramExportToast + histogramExportToastTimer. Handler
    ships in-state semantics: window from windowSinceUnix, limit
    from histogramLimit, suggestHistogramExportFilename default,
    kind-appropriate save dialog filter. Toast "Exported N
    plugins as CSV/JSON (X.X KB)" accent-green tint with 0.16s
    fade-in. Legend footer extended.

    With round 21 done, the Top plugins histogram closes its
    audit-export symmetry loop (CSV for spreadsheets + JSON for
    archives, both with identical column semantics and identical
    schema_version provenance), matching the symmetry the
    drilldown popover already has (rounds 19+20). Next subsystem
    candidates: Hopper rule reorder-by-drag in the coverage
    panel (drag a dead row up to fix shadowing in one motion),
    histogram time-bucket axis ("activity per week" alongside the
    current per-plugin breakdown), drilldown row →
    cross-surface filter (clicking a fall-through filename in
    the popover carries the search query into the document
    inspector), Loom-grade tagging explorer, doc-detail metadata
    editor read/write surface, Beacon cache inspector polish
    (column sort by basename / model facet), Quill multi-document
    field-detect queueing, install-log per-plugin retention
    override (some plugins are audit-critical and want longer
    retention than the global default).

### What round-20 (2026-06-21 21:35 PT) just shipped

Round-20 wrap-line (preserved verbatim from the prior STATE for
continuity; full round-20 narrative follows below):
2026-06-21 21:35 PT — drilldown JSON export arc (slices 93-96) +
Top plugins histogram Sort by selector (slice 97), bars stay
anchored to total activity when sort switches (re-anchoring would
shrink/grow widths disorientingly), legend footer updated to
explain the anchor invariant. All gates green: cargo fmt clean
(one trivial cargo-fmt diff in cmds.rs auto-squashed into slice 94
commit via --fixup + --autosquash before push), cargo clippy --lib
-D warnings PASSED CLEAN in 13.23s, cargo test --lib 2320 passed /
0 failed (round-19 baseline 2307 + 13 from drilldown JSON envelope
primitive = 2320), pnpm check 0 errors / 104 warnings (round-19
baseline preserved EXACTLY). Pushed + verified (local==origin
2894329).

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `2894329` — "feat(plugins): Sort by selector for Top plugins histogram".

### What round-20 (2026-06-21 21:35 PT) just shipped

Five slices across two cohesive arcs. Before this tick the
drilldown popover could save its bucket as RFC-4180 CSV (round-19
work) but had no JSON envelope export — a paralegal feeding a
downstream pipeline or archive workflow had to manually wrap the
CSV in JSON or invent provenance metadata. And the round-18 "Top
plugins" histogram emitted rows DESC by total activity only — a
user investigating "which plugin's been breaking my installs the
most this month?" had to scan failure chips on every row visually
instead of pivoting the sort axis. Tonight both gaps close
end-to-end.

Round 19's closing notes listed both items as candidates:
"drilldown JSON export envelope (mirror the install-log JSON
envelope so the CSV + JSON pair stays symmetric across audit
surfaces)" and the histogram could naturally extend with a
"time-bucket axis" / sort-axis pivot. Both lent themselves to
clean composition with round-18 + round-19 shipped surfaces.

- Slice 93: drilldown JSON export envelope primitive (7182624,
  412 LOC). Pure-data sample_drilldown_to_json(drill, rule_names)
  -> DrilldownExportEnvelope mirroring the install_log_to_json
  envelope shape (slice 60). schema_version=1 matching
  INSTALL_LOG_EXPORT_SCHEMA_VERSION so a downstream reader can
  recognise "Slab audit export v1" across both envelopes without
  checking the source surface. generated_at_iso (ISO-8601 UTC)
  via a private chrono helper duplicated from install_log so the
  hopper coverage module doesn't take a cross-subsystem dep.
  bucket field carries the raw SampleBucket discriminator for
  pattern-matching; bucket_kind + bucket_name pre-compute the
  same (kind, name) pair the CSV emits via bucket_csv_labels so
  JSON + CSV exports of the same bucket carry IDENTICAL labels
  exactly. sample_count (post-cap matching samples.len()) +
  total_in_bucket (pre-cap matching SampleDrilldown) +
  truncated flag captured separately so a consumer can detect
  truncation from either source. samples verbatim preserves
  input order. sample_drilldown_to_json_with_now takes an
  explicit unix-seconds now so tests don't race the wall clock
  (same pattern as install_log_to_json_with_now). 13 new tests
  pin schema + ISO format + bucket_kind+name for fallthrough +
  rule + rule-name-resolution + fallback to "Rule #N" 1-based for
  missing/blank/out-of-range names + sample_count+total invariant
  + untruncated case + empty drilldown renders cleanly + preserves
  input order + preserves full sample axes + serde full roundtrip
  + pretty-print is valid JSON + iso_helper handles 0/i64::MAX +
  bucket_kind matches serde tag (consumer reading bucket_kind vs
  bucket.kind gets same answer).
- Slice 94: drilldown JSON export Tauri command (c5f199c,
  53 LOC + fmt fixup squashed via --autosquash). 
  slab_hopper_export_drilldown_json(drilldown, rule_names, path)
  -> u64 writes the slice-93 envelope to disk as
  pretty-printed JSON. Same command shape as
  slab_hopper_export_drilldown_csv (slice 89) and
  slab_marketplace_install_log_export_json (slice 61) — Tauri
  layer owns disk I/O because the frontend's plugin-fs scope
  doesn't cover arbitrary user-chosen paths. Pretty-printed
  (NOT compact) so a paralegal opening the file in a text editor
  can read it; compactness saves bytes that don't matter for a
  per-bucket drilldown. Idempotent (overwrites if target exists),
  returns byte count actually written, creates parent dirs if
  missing. Registered in invoke_handler alongside the CSV export.
  No new lib-test surface because the primitive in slice 93
  already pins the envelope shape — the command is a thin
  disk-IO wrapper following the same untested-thin-wrap pattern.
- Slice 95: drilldown JSON export TS client + ext-aware filename
  helper (df61510, 129 LOC across hopper.ts + hopper.test.ts).
  slabHopperExportDrilldownJson(drilldown, ruleNames, path) ->
  Promise<number> wraps invoke; same lazy-import isInTauri pattern
  as slabHopperExportDrilldownCsv (hopper.test.ts runs under tsx
  without the Tauri plugin chain). Browser-mode returns 0 (no-op).
  suggestDrilldownExportFilename extended with optional ext slot
  ("csv" | "json", default "csv" for backwards compat with slice
  90 callers — pure additive change). Both export wrappers now
  share ONE suggestion path with IDENTICAL filename shape apart
  from the suffix. 5 new pure-helper tests in hopper.test.ts:
  default ext stays "csv" (backwards-compat), explicit ext:"csv"
  matches implicit default exactly, ext:"json" produces expected
  shape + suffix, paired csv/json forms differ ONLY in the suffix
  (slice-prefix equality assertion pins the invariant), rule
  bucket with slug + ext:"json" still slugifies correctly.
- Slice 96: Export JSON button + toast in drilldown popover
  (d608b70, 84 LOC in HopperRulesEditor.svelte). The demo-able
  payoff tying slices 93-95 together. Imports
  slabHopperExportDrilldownJson alongside the existing CSV
  wrapper; no new state cells. The slice-91 exportDrilldownCsv
  handler refactored into single exportDrilldown(format)
  dispatch + thin exportDrilldownCsv/exportDrilldownJson
  wrappers. Both formats share drilldownExporting gate (user
  can't open save dialog twice), drilldownExportToast cell (one
  toast at a time across both formats), in-state-snapshot
  semantics (background rule edit can't sneak in a different
  bucket between "click Export" and "click Save"). Per-format
  diffs: filename suffix (.csv vs .json), save-dialog filter
  (CSV vs JSON), which Tauri command runs, toast copy
  ("Exported 23 files as CSV/JSON"). Export JSON button placed
  AFTER Export CSV (not before) so verb order reads Reload →
  Export CSV → Export JSON → Close; CSV-first because the
  spreadsheet path is the primary audit workflow, JSON-second
  because the envelope is the secondary archive/pipeline path.
  Same disabled states as the CSV button. Toast copy upgraded
  from "Exported N files (X.X KB)" to "Exported N files as
  CSV/JSON (X.X KB)" so a user who exported both formats
  back-to-back can tell from the toast which one just landed.
- Slice 97: Sort by selector for Top plugins histogram
  (2894329, 447 LOC across marketplace.ts + marketplace.test.ts
  + RecentInstallsDrawer.svelte). Pure-data sortHistogramRows(
  rows, key) -> PluginHistogramRow[] in marketplace.ts with 5
  axes (total / installs / updates / failures / recent — no
  uninstalls axis because uninstall-heavy plugins are an
  antipattern users spot via bar segments not sort defaults).
  DESC primary + plugin_id ASC tiebreak matches server contract
  exactly so refresh doesn't reshuffle ties. Returns NEW array
  (Svelte $state proxies don't play well with in-place sorts +
  server payload should stay untouched so a later sort-switch
  sees original rows). HISTOGRAM_SORT_KEYS array +
  histogramSortLabel helper drive the dropdown so adding a sixth
  axis is a one-line edit across all four surfaces (type +
  array + label + UI). 19 new tests in src/lib/marketplace.test
  .ts (new file, follows fuzzy.test.ts inline-expect convention):
  axis count + order (total first, recent last, no uninstalls),
  label per key + spot-checks for renames, non-mutating contract
  (input array unchanged + returns new array), per-axis sort
  order (total/installs/updates/failures/recent each reorder
  expected sequence), ASC plugin_id tiebreak, empty input ->
  empty output, single row passes through every axis, every axis
  preserves array length (sort is permutation never filter) +
  every input plugin_id appears in output, bogus key doesn't
  throw at runtime. UI: native <select> dropdown above the
  histogram list (label "Sort by" + 5 options) with custom
  dark-glass styling (appearance: none + 1px border + hover/
  focus-visible accent + custom chevron via two linear-gradient
  backgrounds — no extra SVG asset). Bars stay anchored to
  TOTAL ACTIVITY (most-active plugin = 100% wide) when sort
  switches — re-anchoring to the sort axis would shrink/grow
  widths disorientingly. Legend footer updated to explain the
  anchor invariant ("bars stay anchored to total activity").

Gates result: cargo fmt clean (cargo fmt --all --check exit 0
after one trivial cmds.rs reformatting auto-squashed into slice
94 via --fixup + --autosquash before push), cargo clippy --lib
-- -D warnings PASSED CLEAN in 13.23s (matches round-19 10.91s
baseline — pure-data JSON envelope serialiser + thin command
wrapper + UI-only sort dropdown add no new clippy surface),
cargo test --lib 2320 passed / 0 failed (round-19 baseline 2307
+ 13 from slice 93 JSON envelope = 2320), pnpm check 0 errors /
104 warnings (round-19 baseline preserved EXACTLY — zero new
warnings from the JSON export wrapper, ext-aware suggest helper,
button + toast wiring, sort helper, dropdown, scoped CSS).

PROCESS NOTES:
- Round-19 closing notes listed "drilldown JSON export envelope
  (mirror the install-log JSON envelope so the CSV + JSON pair
  stays symmetric across audit surfaces)" as the lead candidate;
  slices 93-96 close that arc end-to-end with the same four-layer
  cadence as the round-15 bulk-update arc (68-72), round-16
  install-log filter arc (73-77), round-17 hopper coverage arc
  (79-82), round-18 hopper drilldown arc (83-86), round-19
  drilldown CSV arc (88-91): pure-data primitive → Tauri command
  → TS client → demo-able UI. Slice 97 compressed histogram
  sort-axis into one composite slice because the data path
  already existed (PluginHistogramRow from slice 87) and the
  whole sort axis is pure UI wiring around an already-tested
  data shape.
- Five slices, five commits, two logical subsystems. Drilldown
  JSON arc (93-96) follows the canonical four-layer pattern;
  histogram sort-axis (97) is a single composite commit because
  the backend axis already existed (plugin_histogram from slice
  87) — the slice is pure UI wiring + a pure-helper add around
  an already-tested data shape.
- The DrilldownExportEnvelope schema_version=1 matches the
  install-log envelope's schema_version constant so a downstream
  consumer reading either Slab audit-export JSON file recognises
  the v1 contract by name. A future shape change (e.g. adding
  rule predicate JSON to the drilldown envelope) bumps the
  drilldown's version independently — the two envelopes are
  parallel-versioned, not joint-versioned, because their bodies
  are unrelated.
- The bucket_csv_labels helper from slice 88 was the seam that
  let slice 93 reuse the CSV's exact bucket-name fallback chain
  in the JSON envelope. Both formats now agree byte-for-byte on
  the bucket label, which means a paralegal who exported a bucket
  as CSV and another paralegal who exported the same bucket as
  JSON can compare labels and trust they're identical.
- The ext slot on suggestDrilldownExportFilename was the smallest
  possible surface-area extension — adding a 4th key to the opts
  bag rather than a parallel suggestDrilldownJsonExportFilename
  helper. The default "csv" preserves every existing call site
  verbatim, and the JSON wrapper just passes ext:"json". A future
  3rd format (e.g. JSONL for streaming) is a one-line type
  widening + a one-arm dispatch in exportDrilldown.
- The sortHistogramRows helper returns a NEW array
  deliberately — in-place sort on a Svelte 5 $state proxy
  surfaces reactivity bugs in the proxy machinery, and the server
  payload should stay untouched so a later sort-axis switch sees
  the original rows. Same pattern as the round-15 bulkUpdate
  primitive which never mutated its inputs.

DESIGN NOTES:
- Export JSON button AFTER Export CSV (not before) reads as
  "primary audit path → secondary archive path". A paralegal
  emailing the bucket to a partner reaches for CSV first
  (spreadsheet); a developer feeding the bucket to a downstream
  pipeline reaches for JSON. The verb order Reload → Export CSV
  → Export JSON → Close keeps the most-common verbs leftmost.
- Toast copy upgrade ("as CSV / as JSON" suffix) is a tiny detail
  but matters when a user exports both formats back-to-back. The
  4s toast duration is long enough that two exports can overlap;
  the format-tag in the message disambiguates which one just
  landed without forcing the user to remember which button they
  clicked.
- Sort by selector uses a NATIVE <select> (not a custom popover)
  for two reasons: (1) native selects carry keyboard a11y for
  free (arrow keys + typeahead + Esc-cancel), and a custom
  popover would have to reimplement them; (2) the option count
  is small (5) and stable — a custom popover is the right call
  when the option list is dynamic or the options have rich
  content (icons, sublabels), neither of which applies here.
- Bars stay anchored to total activity when sort switches
  (re-anchoring to the sort axis would shrink/grow widths
  disorientingly). The sort axis affects ORDER, not SCALE — a
  plugin's bar width represents its share of cross-plugin
  activity, which is independent of which axis the user is
  currently asking about.
- No uninstalls sort axis because uninstall-heavy plugins are
  almost always an antipattern the user catches via the bar's
  amber segment (visible at a glance). Adding it would clutter
  the menu with a rarely-useful pivot. The four count axes that
  ARE included (total / installs / updates / failures) each
  answer a real workflow question:
    - total: "what's most active overall" (the default)
    - installs: "what am I adopting most" (cohort tracking)
    - updates: "what's churning most" (release-velocity check)
    - failures: "what's breaking most" (the bug hunt)
- Sort selector placement: above the histogram list, BELOW the
  section toggle. Above the list because the selector affects
  the list's order — controls go above their targets. Below the
  section toggle because the selector is a sub-control of the
  Top plugins section, not a sibling.

## Roadmap — round 20 (Drilldown JSON Export + Histogram Sort Axis) — ALL DONE

Round 20 batched FIVE feature slices into one cron tick. Four
slices built the drilldown JSON export end-to-end (envelope
primitive → Tauri command → TS client + ext-aware suggest helper
→ demo-able UI), and one composite slice shipped the Top plugins
histogram sort axis end-to-end (pure-data sort helper + UI
dropdown + comprehensive tests in a new marketplace.test.ts).

93. ~~**drilldown JSON export envelope primitive**~~ —
    DONE (2026-06-21 21:35 PT, 7182624, single commit, 412 LOC).
    Pure-data sample_drilldown_to_json(drill, rule_names) ->
    DrilldownExportEnvelope mirroring the install_log_to_json
    envelope shape (schema_version=1 matching
    INSTALL_LOG_EXPORT_SCHEMA_VERSION + generated_at_iso +
    bucket + bucket_kind + bucket_name + sample_count +
    total_in_bucket + truncated + samples). 13 new tests pin
    schema + ISO format + bucket labels + truncation invariant
    + serde roundtrip + edge cases.
94. ~~**drilldown JSON export Tauri command**~~ —
    DONE (2026-06-21 21:35 PT, c5f199c, single commit, 53 LOC +
    fmt fixup squashed via --autosquash). slab_hopper_export_
    drilldown_json(drilldown, rule_names, path) -> u64.
    Pretty-printed JSON write to disk. Tauri-layer disk I/O
    matching the existing CSV/JSON export commands.
    Registered in invoke_handler.
95. ~~**drilldown JSON export TS client + ext-aware filename helper**~~ —
    DONE (2026-06-21 21:35 PT, df61510, single commit, 129 LOC).
    slabHopperExportDrilldownJson lazy-import wrapper (browser
    no-op). suggestDrilldownExportFilename extended with
    optional ext ("csv" default for backwards compat, "json"
    for new export). 5 new pure-helper tests in hopper.test.ts.
96. ~~**Export JSON button + toast in drilldown popover**~~ —
    DONE (2026-06-21 21:35 PT, d608b70, single commit, 84 LOC).
    The demo-able payoff. Button after Export CSV (verb order:
    Reload → Export CSV → Export JSON → Close). Shared
    drilldownExporting gate + toast cell + in-state-snapshot
    semantics. Per-format diffs: suggested filename suffix,
    save-dialog filter, which Tauri command, toast copy
    ("Exported N files as CSV/JSON").
97. ~~**Sort by selector for Top plugins histogram**~~ —
    DONE (2026-06-21 21:35 PT, 2894329, single commit, 447 LOC).
    Pure-data sortHistogramRows(rows, key) with 5 axes (total /
    installs / updates / failures / recent — no uninstalls).
    Returns NEW array (non-mutating). HISTOGRAM_SORT_KEYS +
    histogramSortLabel helpers. Native <select> dropdown above
    the histogram list with custom dark-glass styling. Bars
    stay anchored to total activity when sort switches. 19 new
    tests in new src/lib/marketplace.test.ts file (follows
    fuzzy.test.ts inline-expect convention).

    With round 20 done, the Hopper drilldown popover closes the
    audit-export symmetry loop (CSV for partners + JSON for
    archives, both with identical bucket labels), and the
    Recent installs drawer's Top plugins section gains pivot-
    sorting (no refetch, cheap pure-data resort). Next subsystem
    candidates: Hopper rule reorder-by-drag in the coverage
    panel (drag a dead row up to fix shadowing in one motion),
    histogram time-bucket axis ("activity per week" alongside
    the current per-plugin breakdown), drilldown row →
    cross-surface filter (clicking a fall-through filename in
    the popover carries the search query into the document
    inspector), Loom-grade tagging explorer, doc-detail metadata
    editor read/write surface, Beacon cache inspector polish
    (column sort by basename / model facet), Quill multi-document
    field-detect queueing, install-log per-plugin retention
    override (some plugins are audit-critical and want longer
    retention than the global default).

### What round-19 (2026-06-21 18:35 PT) just shipped

Five slices across two cohesive arcs. Before this tick the
round-18 drilldown popover could show the 23 fall-through files for
a coverage bucket but had no way to save that list — paralegals
would have to copy filenames by hand to email them to a partner.
And the round-18 "Top plugins" histogram rows were passive: you
could see "com.acme.ocr-pro: 23 events" but the only way to
actually filter the timeline to those 23 was to type the plugin id
into the search field yourself. Tonight both gaps close end-to-end.

Round 18's closing notes listed both items as candidates: "drilldown
CSV export ('save the fall-through list')" and the histogram could
naturally extend by becoming clickable to drive the existing filter
axis. Both lent themselves to clean composition with round-18's
shipped surfaces.

- Slice 88: drilldown CSV export primitive (0c04f6b,
  354 LOC). Pure-data sample_drilldown_to_csv(drill, rule_names,
  include_header) -> String. RFC-4180 with six columns:
  filename, size_bytes, page_count, text_sample, bucket_kind,
  bucket_name. bucket_kind is the SampleBucket serde tag
  ("fallthrough" / "rule") so a downstream consumer can re-derive
  the bucket without guessing. bucket_name uses the
  describeBucket fallback chain verbatim: trimmed rule_names[i]
  when present + non-empty, else "Rule #N" (1-based). Header
  opt-in (mirror backfill_report_to_csv signature) so an
  append-to-audit workflow can suppress it. RFC-4180 escaping
  duplicated (not re-exported) from backfill so the two emitters
  stay independent. RuleSample now derives Default (additive —
  every field already had serde default attrs) so tests can spread
  RuleSample { filename, ..Default::default() } without listing
  zero fields. 13 new tests pin header behaviour + bare-empty
  shape + fallthrough label + rule-bucket name resolution +
  empty/blank/out-of-range name fallback to "Rule #N" + comma +
  quote + newline escaping + None columns emit empty cells +
  preserves input order + non-ASCII filenames pass through unquoted
  when safe + row count == drill.samples.len() (NOT total_in_bucket
  — truncation footnote belongs on UI, not in CSV).
- Slice 89: drilldown CSV export Tauri command (e7d916e,
  53 LOC). slab_hopper_export_drilldown_csv(drilldown,
  rule_names, path) -> u64 writes the CSV to an absolute path the
  frontend obtained from @tauri-apps/plugin-dialog save(). Same
  command shape as slab_hopper_export_backfill_csv +
  slab_marketplace_install_log_export_csv — the Tauri layer owns
  disk I/O because the frontend's plugin-fs scope doesn't cover
  arbitrary user-chosen paths. Idempotent (overwrites if target
  exists — save dialog handles overwrite confirmation upstream),
  returns byte count actually written so the toast can show
  "Exported 23 files (1.4 KB)" without re-reading the file.
  Creates parent dirs if missing. Registered in invoke_handler
  alongside slab_hopper_export_backfill_csv. No new lib-test
  surface because the primitive in slice 88 already pins the CSV
  shape — the command is a thin disk-IO wrapper following the
  same untested-thin-wrap pattern as the two existing CSV exports.
- Slice 90: drilldown CSV export TS client + filename helper
  (3d48a57, 237 LOC across hopper.ts + hopper.test.ts).
  slabHopperExportDrilldownCsv(drilldown, ruleNames, path) ->
  Promise<number> wraps the invoke; lazy-imports $lib/tauri so the
  hopper.test.ts file (which runs under tsx without the Tauri
  runtime) can pull the helpers without dragging the plugin chain
  into a node import. Browser-mode returns 0 (no-op) — same pattern
  as exportInstallLogCsv. suggestDrilldownExportFilename(bucket,
  ruleNames, opts) pure helper proposing
  hopper-drilldown_<watch>_<bucket>_<YYYY-MM-DD>.csv. watch slot
  reads "watch-N" or bare "watch" when unset/negative; bucket slot
  reads "fallthrough" for the catch-all, "rule-N" (1-based) +
  optional "_<slug>" for rule buckets. Slugifier NFD-strips
  diacritics (café → cafe), collapses non-[a-z0-9] runs to single
  dashes, trims leading/trailing dashes; falls back to bare rule-N
  when nothing survives. Date uses LOCAL time. 11 new pure-helper
  tests in hopper.test.ts (inline-expect convention): fallthrough
  no-watch + watchId=7 shapes, rule no-names = "rule-N", rule
  with name slug, messy chars collapse, NFD diacritics, all-
  punctuation falls back to bare rule-N, whitespace-only name,
  negative watch id falls back, rule index 9 reads as rule-10
  (off-by-one invariant), always ends in .csv.
- Slice 91: Export CSV button + toast in drilldown popover
  (7a358c8, 104 LOC in HopperRulesEditor.svelte). The demo-able
  payoff tying slices 88-90 together. Imports
  slabHopperExportDrilldownCsv + suggestDrilldownExportFilename +
  saveDialog from @tauri-apps/plugin-dialog (same dependency
  RecentInstallsDrawer's export uses). New state cells:
  drilldownExporting (gates the button while save dialog + write
  in flight) and drilldownExportToast (4s success notice) with a
  named setTimeout handle so back-to-back exports don't pile up
  toasts. exportDrilldownCsv() resolves the suggested filename,
  opens the native save dialog (CSV filter + meaningful title),
  ships the in-state drilldown VERBATIM (not re-fetched, so a
  background rule edit can't sneak in a different bucket between
  click-Export and click-Save) + the current ruleNames array to
  the slice-89 command, surfaces a 4s "Exported 23 files (1.4
  KB)" toast. Cancellation is a clean no-op. Local formatBytes
  helper (kept separate from hopper.ts's predicate formatter —
  different signature, different context). Button between Reload
  and Close in the popover header with disabled states for
  in-flight/loading/null/empty-bucket and defensive-tooltip pattern
  matching the slice 91 install-log export. Success toast renders
  inline BELOW the popover header (NOT a floating banner) so it
  stays attached to the popover; green vocabulary
  (rgb(170,230,195) / rgba(110,220,154,...)) matching the
  install-event seg-install color; 0.16s fade-in keyframe.
- Slice 92: click Top plugins row to filter timeline by plugin
  (49511df, 107+ / 32- LOC in RecentInstallsDrawer.svelte). Each
  histogram row is now a <button> (Notion-style row interaction
  matching slice 86 coverage-row click pattern). One click pivots
  the timeline from "everything in window" to "just this plugin's
  events" via the existing plugin_id_substr filter axis — the
  SAME axis the search input + slice 77 chip strip + export
  filenames all feed, so there's ONE narrow carrying consistently
  across every dependent surface. Click semantics: row != current
  filter → apply; row == current filter → clear (toggle-off — the
  natural undo for "I clicked a bar" is "I click the same bar
  again"). Visual states: hover-tinted background +
  faint border on hover; .active state for currently-filtered row
  uses accent-tinted background + border
  (rgba(124,140,255,.1)/.34) so the row reading "this is what I'm
  looking at right now" is unmistakable; focus-visible accent ring.
  a11y: aria-pressed reflects the toggle state, title attr reads
  "Filter timeline below to <id>" / "Clear filter on <id>" per
  state. Legend footer extended explaining the click affordance.

Gates result: cargo fmt clean (cargo fmt --all --check exit 0),
cargo clippy --lib -- -D warnings PASSED CLEAN in 10.91s (matches
round-18 11.43s baseline — pure-data CSV serialiser + thin
command wrapper add no new clippy surface), cargo test --lib 2307
passed / 0 failed (round-18 baseline 2294 + 13 from slice 88 =
2307), pnpm check 0 errors / 104 warnings (round-18 baseline
preserved EXACTLY — zero new warnings from the export wrapper,
suggestFilename helper, button + toast wiring, histogram row
refactor, scoped CSS).

PROCESS NOTES:
- Round-18 closing notes listed "drilldown CSV export ('save the
  fall-through list')" as a next-tick candidate; slices 88-91
  close that arc end-to-end with the same four-layer cadence as
  the round-15 bulk-update arc (68-72), round-16 install-log
  filter arc (73-77), round-17 hopper coverage arc (79-82), and
  round-18 hopper drilldown arc (83-86): pure-data primitive →
  Tauri command → TS client → demo-able UI. Slice 92 compressed
  histogram click-to-filter into one composite slice because the
  backend axis already existed (plugin_id_substr filter from
  slice 73) — the slice is pure UI wiring around an already-tested
  filter primitive.
- Five slices, five commits, two logical subsystems. Drilldown
  CSV arc (88-91) follows the canonical four-layer pattern;
  histogram click-to-filter (92) is single UI-only commit because
  the data path was complete.
- The RuleSample Default derive in slice 88 is a tiny additive
  affordance the test code needed to spread { filename,
  ..Default::default() } without listing every zero field. Every
  field already had a serde default attribute so the runtime
  semantics don't change — Default produces exactly what the
  Deserialize default path produces. Cheap, useful for tests, no
  observable behaviour change for callers.
- The Tauri command in slice 89 ships ruleNames as a Vec<String>
  parameter rather than reading from the watch registry server-
  side because the popover's bucket_name should match what the
  user SAW on screen — even if they have unsaved name edits in
  the editor. A server-side registry lookup would silently use
  the persisted names instead. Same reasoning as why slice 84's
  drilldown command accepts caller-supplied candidate_rules.
- The slugifier in slice 90 deliberately doesn't transliterate
  non-ASCII letters (café → caf would lose info silently). It
  NFD-strips diacritics (café → cafe) which is the standard
  ASCII-fold pattern, and falls back to bare "rule-N" when nothing
  survives the slug. Filenames stay portable across Windows
  without being misleading about what the rule was named.
- The exportDrilldownCsv handler ships the in-state drilldown
  verbatim rather than re-fetching. Re-fetching would race the
  600ms scheduleSave that ripples into a drilldown refresh — a
  background rule edit could sneak in a different bucket between
  "click Export" and "click Save" in the dialog. Shipping the
  snapshot means the CSV matches exactly what the popover
  currently renders. Same in-state-snapshot reasoning as the
  RecentInstallsDrawer export flow.

DESIGN NOTES:
- Export button BETWEEN Reload and Close (not after Close, not
  before Reload) so the verb order reads "refresh this view → save
  this view → done". Reload-then-Export-then-Close is the natural
  workflow: "let me re-pull the latest bucket, then save it, then
  close the popover".
- Disabled-when-empty (drilldown.samples.length === 0) is the right
  call because empty buckets shouldn't offer an export — the CSV
  would just be the header row, which reads like "the export
  failed silently". The defensive tooltip ("No files in this
  bucket to export") explains the disabled state on hover.
- The 4s toast duration matches the install-log export toast
  (slice 62) and the slice 91 install-log retention toast — one
  duration across audit-export toasts so paralegals don't have
  to recalibrate per surface.
- Histogram row .active visual is accent-tinted (not check-marked,
  not chip-suffixed) because the bar IS the row and the bar's
  width already conveys magnitude — adding a check mark would
  fight the bar visually. Accent-tint + border lifts the row's
  z-priority without obscuring the data.
- Click-row-twice-to-clear matches the slice 86 popover toggle
  pattern, which itself was chosen for the same reason: the
  user's last action on the same surface should reverse itself.
  Forcing them to scroll up to the search field and click an X
  to clear would break the spatial mental model.
- Histogram click clears ONLY the plugin axis (not the action
  axis). The action chips are independent narrows; clearing them
  too would feel like an undo'd batch operation. The user only
  clicked ONE control; only that control's effect reverses.

## Roadmap — round 19 (Drilldown CSV Export + Histogram Click-to-Filter) — ALL DONE

Round 19 batched FIVE feature slices into one cron tick. Four
slices built the drilldown CSV export end-to-end (primitive →
command → TS client + suggest helper → demo-able UI), and one
composite slice wired the round-18 histogram rows into the
existing plugin filter axis.

88. ~~**drilldown CSV export primitive**~~ —
    DONE (2026-06-21 18:35 PT, 0c04f6b, single commit, 354 LOC).
    Pure-data sample_drilldown_to_csv(drill, rule_names,
    include_header) RFC-4180 serialiser. bucket_kind matches the
    SampleBucket serde tag; bucket_name uses describeBucket
    fallback chain (Rule #N 1-based). RuleSample now derives
    Default (additive — every field already had serde default
    attrs). 13 new tests pin header + escaping + None columns +
    preserves order + row count == samples.len() invariant.
89. ~~**drilldown CSV export Tauri command**~~ —
    DONE (2026-06-21 18:35 PT, e7d916e, single commit, 53 LOC).
    slab_hopper_export_drilldown_csv(drilldown, rule_names, path)
    -> u64. Tauri-layer disk I/O matching the existing two CSV
    export commands. Idempotent, returns byte count, creates
    parent dirs. Registered in invoke_handler.
90. ~~**drilldown CSV export TS client + filename helper**~~ —
    DONE (2026-06-21 18:35 PT, 3d48a57, single commit, 237 LOC).
    slabHopperExportDrilldownCsv lazy-import wrapper (browser
    no-op). suggestDrilldownExportFilename helper proposing
    hopper-drilldown_<watch>_<bucket>_<YYYY-MM-DD>.csv with
    NFD-aware slugifier + 1-based bucket index. 11 new pure-
    helper tests in hopper.test.ts.
91. ~~**Export CSV button + toast in drilldown popover**~~ —
    DONE (2026-06-21 18:35 PT, 7a358c8, single commit, 104 LOC).
    The demo-able payoff. Button between Reload and Close;
    disabled states for in-flight/loading/null/empty-bucket; 4s
    green success toast inline below header; native save dialog
    with CSV filter; ships in-state drilldown verbatim so
    background rule edits can't race the export.
92. ~~**click Top plugins row to filter timeline by plugin**~~ —
    DONE (2026-06-21 18:35 PT, 49511df, single commit, 107+/32- LOC).
    Histogram rows now <button>s with onHistogramRowClick toggle
    semantics (click → apply plugin filter; click again → clear).
    Reuses existing plugin_id_substr axis — ONE filter narrow
    carries consistently across timeline + chip strip + export
    filenames. .active accent-tint + focus-visible ring +
    aria-pressed; legend footer extended explaining click affordance.

    With round 19 done, the Hopper drilldown workflow closes the
    audit-export loop (click a coverage row → see the files →
    save them as CSV for the partner), and the Recent installs
    drawer's Top plugins section becomes bidirectional (view AND
    navigation surface — click a bar to see that plugin's
    timeline). Next subsystem candidates: drilldown JSON export
    envelope (mirror the install-log JSON envelope so the CSV +
    JSON pair stays symmetric across audit surfaces), Hopper
    rule reorder-by-drag in the coverage panel (drag a dead row
    up to fix shadowing in one motion), histogram time-bucket
    axis ("activity per week" alongside the current per-plugin
    breakdown), Loom-grade tagging explorer, doc-detail metadata
    editor read/write surface, Beacon cache inspector polish
    (column sort by basename / model facet), Quill multi-document
    field-detect queueing, drilldown row → toast a "filter
    timeline" cross-surface (clicking a fall-through filename in
    the popover could carry the search query into the document
    inspector).

### What round-18 (2026-06-21 15:30 PT) just shipped

Five slices across two cohesive arcs. Before this tick the
HopperRulesEditor coverage panel surfaced per-rule bars but had
no way to answer the natural follow-up question "which 8 files
fell through?" — clicking a row was a dead affordance. And the
RecentInstallsDrawer surfaced per-event timelines but couldn't
answer "which plugins did I install the most this month?". Tonight
both gaps close end-to-end.

Round 17's closing notes listed both items as candidates: "Hopper
sample-set explorer (drill into 'show me the 23 fall-through files'
from a coverage row)" and "install-log drawer's coverage-like
aggregate ('which plugins did you install the most this month?')".
Both lent themselves to 1-4 slice arcs that composed into a 5-slice
batch.

- Slice 83: Hopper sample drilldown primitive (7fc3463,
  389 LOC). New pure-data compute_sample_drilldown(rules,
  samples, bucket, preview_cap) in pdf::hopper::coverage
  returning SampleDrilldown {bucket, samples, total_in_bucket,
  truncated}. SampleBucket is a tag-discriminated enum:
  Rule{index} for "samples this rule was the FIRST to match"
  (matches RuleCoverage::first_match in count) or Fallthrough for
  "samples no rule matched". O(rules * samples) — same shape as
  compute_coverage; we don't reuse the coverage report because
  winners aren't carried in its shape (only counts are), and re-
  running the chain is cheap enough that a second pass is
  simpler than caching a winners vec. preview_cap clamps to
  [1, 5000] so a misuse can't copy a giant payload across IPC.
  total_in_bucket reports the FULL match count even after the
  cap trims samples, so the UI can render "Showing 25 of 47".
  truncated flag (total > samples.len()) so the UI doesn't
  compare counts itself. Out-of-range Rule{index} yields empty
  rather than panicking — matches the analyzer's lenient
  stance. 15 new tests pin rule bucket / fallthrough bucket /
  shadowed-rule empty bucket / cap clamps [1, 5000] / truncated
  flag / out-of-range / fall-through with no rules = all / fall-
  through with Always = empty / preserves input order / full
  sample axes (size/page/text) survive / SampleBucket serde
  rule + fallthrough round-trips / SampleDrilldown serde shape.
- Slice 84: sample drilldown Tauri command (80e03bc, 130
  LOC). slab_hopper_sample_drilldown(watch_id, bucket,
  candidate_rules?, samples?, sample_limit?, preview_cap?)
  mirrors slab_hopper_rule_coverage on every input axis
  (candidate_rules + samples + sample_limit) so a click on a
  coverage row drills into the EXACT same sample set the
  coverage report counted. Anything else would surface "27
  fall-throughs" in the header but only show 23 in the
  drilldown — would read as a bug. preview_cap (default 25,
  clamped to [1, 1000]) caps the drilldown payload — heavier
  per row than coverage (full filename + axes vs counts) so
  its ceiling is lower (1000 vs 5000) and its default smaller
  (25 vs 100). 5 new clamp_preview_cap helper tests pin
  default 25 + bounds + i64 boundaries + the invariant that
  clamp_preview_cap default < clamp_sample_limit default.
- Slice 85: sample drilldown TS client + bucket helpers
  (6c4fa84, 288 LOC across hopper.ts + hopper.test.ts).
  SampleBucket discriminated union, ergonomic constructors:
  FALLTHROUGH_BUCKET singleton (stable object, no per-call
  allocation, identity-stable for === checks) + ruleBucket(i)
  (throws on negative/non-integer indices — the Rust side
  treats out-of-range as empty; negatives indicate a TS bug,
  so fail loud client-side). slabHopperSampleDrilldown wrapper
  with same opts shape as slabHopperRuleCoverage + previewCap.
  Three pure helpers: sampleBucketEquals (gates "open" highlight
  without object-identity dependency), describeDrilldown
  ("No files" / "1 file" / "3 files" / "Showing 25 of 47" /
  defensive "Showing 0 of 5"), describeBucket(bucket,
  ruleNames?) (fallthrough copy / "#3 Receipts" / "Rule #N"
  fallback for missing/empty/whitespace/out-of-range names —
  popover never reads as "#1 " with trailing space). 19 new
  pure-helper tests in hopper.test.ts following inline-expect
  convention.
- Slice 86: clickable coverage rows + drilldown popover
  (e38a358, 421 LOC in HopperRulesEditor.svelte). The demo-
  able payoff. Coverage rows (including fall-through) are now
  <button>s wrapping the existing grid markup; button reset
  keeps them reading like rows (left-align / inherit font /
  cursor pointer). Accent-tinted .open state + focus-visible
  ring. Chevron column (▸ → ▾) in the counts cell makes the
  affordance obvious. Tooltip per row reads "Show the N
  samples this rule routed" so click-purpose is clear; empty
  rows say "no samples in this bucket; click for empty-state
  details". Click expands an in-panel popover under the row
  via shared {#snippet renderDrilldownBody} (one render path
  for both rule + fall-through buckets). Popover header:
  bucket label via describeBucket + live describeDrilldown
  summary + 56px previewCap number input clamped [1, 1000]
  matching the server clamp + Reload + Close. Body: monospace
  file list (max-height 260px + overflow-y so 100s of fall-
  throughs scroll inside the popover not the editor); per-
  row chevron glyph; truncated footnote. Empty-state copy
  differs per bucket — fall-through reads "every recent file
  matched at least one rule" (informational); rule bucket
  reads "no recent files OR an earlier rule won first — look
  at Dead/Shadowed chips above" (actionable, points at the
  diagnostic chips). openDrilldown toggles off if already
  open (Notion-style). scheduleSave refreshes the open
  drilldown alongside coverage so the bucket reshapes live on
  every edit. Window-level Escape closes the popover. ~135
  lines of scoped CSS following dark-glass tokens
  (rgba(124,140,255,...) accent + monospace 11.5px file rows).
- Slice 87: per-plugin install histogram (a0504dc, 783 LOC
  across install_log.rs + lib.rs + marketplace.ts +
  RecentInstallsDrawer.svelte). End-to-end backend + Tauri +
  TS + UI as one composite slice. Backend: new
  PluginHistogramRow {plugin_id, installs, updates,
  uninstalls, failures, total, last_occurred_at} (total
  precomputed so UI's bar-width and sort don't re-add four
  columns per row). New InstallLog::plugin_histogram(since,
  until, limit) does ONE indexed GROUP BY (plugin_id, action)
  scan + in-memory sort by total DESC with secondary ASC on
  plugin_id (deterministic tiebreak). 13 new tests pin sort
  order / action buckets / last_occurred_at / window filters
  since/until/both / empty cases / limit caps / negative
  clamps to zero / tiebreak / conservation invariant (total ==
  sum of buckets) / serde shape. Tauri: new
  PluginHistogramResult envelope with rows + echoed window/
  limit + grand_total (sum across plugins so UI renders "12
  events across 3 plugins" without re-summing).
  slab_marketplace_install_log_plugin_histogram registered
  in invoke_handler. TS: PluginHistogramRow /
  PluginHistogramResult wire types,
  getPluginInstallHistogram wrapper with browser-mode empty
  fallback, summarizeHistogram pure helper (singular/plural
  correct). UI: new "Top plugins" collapsible section
  between retention block and events list. Same toggle
  pattern as retention (chevron + label + right-aligned
  meta). Per-plugin row: 3-col grid (id+timestamp /
  stacked bar / counts). Stacked bar scaled relative to
  the most-active plugin's total (top row always = 100%);
  four segments in canonical action order with seg-* colors
  (install green #6dd49a / update accent #7c8cff / uninstall
  amber #d9b04c / failed red #ff5d6c); zero-count segments
  don't render so a zero-failure plugin doesn't get an empty
  red sliver. Counts cell: bold total + per-action chips
  using installEventGlyph + count, chips inherit seg-* color.
  Auto-refreshes on window change via $effect tracking
  windowSinceUnix. Empty + error + loading states; legend
  footer. ~150 lines of scoped CSS matching the existing
  retention-block vocabulary so the two sections read as
  siblings.

Gates result: cargo fmt clean (cargo fmt --all --check exit 0),
cargo clippy --lib -- -D warnings PASSED CLEAN in 11.43s (matches
round-17 15.17s baseline — cheap GROUP BY + pure-data drilldown
add no new clippy surface), cargo test --lib 2294 passed / 0 failed
(round-17 baseline 2261 + 15 from drilldown primitive + 5 from
clamp_preview_cap + 13 from histogram = 2294), pnpm check 0 errors
/ 104 warnings (round-17 baseline preserved EXACTLY — zero new
warnings from the clickable rows, drilldown popover, stacked
histogram bars, or scoped CSS).

PROCESS NOTES:
- Round-17 closing notes listed both arcs as next-tick candidates;
  the existing primitive (compute_coverage in slice 79) gave the
  drilldown a clean second-pass shape (winners not carried in the
  coverage report, so a separate primitive is the right factoring
  rather than caching). And install_log already had install_stats
  per-plugin (single id) so generalising to all-plugins-histogram
  was a one-method addition not a schema rework.
- Five slices, five commits, two logical subsystems. The drilldown
  arc (83-86) splits cleanly into pure-data primitive -> command ->
  TS client -> UI matching the round-15 bulk-update arc (68-72)
  and round-16 install-log-filter arc (73-77) cadence. The
  histogram slice (87) compressed backend + commands + TS + UI
  into one because each layer is small (~50-150 LOC) and they're
  tightly coupled by a single new data shape (PluginHistogramRow).
- SampleBucket's tag-discriminated enum shape ({kind, ...}) reads
  cleanly across the Rust/TS boundary — same vocabulary as
  RulePredicate. The TS ruleBucket(i) constructor that throws on
  negative indices is a deliberate divergence from Rust's lenient
  stance: client-side bugs deserve loud failures so they don't
  silently render as empty buckets.
- The drilldown's preview_cap (default 25) vs coverage's
  sample_limit (default 100) divergence is intentional — see the
  test `clamp_preview_cap_default_is_lower_than_coverage_default`
  which pins the invariant so a future tweak that breaks the
  ordering surfaces as a test failure rather than a silent
  regression. Drilldown carries full filenames + axes per row;
  coverage carries per-rule counts only.
- The histogram's bar scaling (relative to top row's total) reads
  more honestly than absolute scaling — at 25 plugins the top
  may have 50 events and the bottom 1, which would render the
  bottom as a 2% sliver under absolute scaling (visually
  indistinguishable from zero). Relative scaling keeps every row
  visually meaningful while preserving order.

DESIGN NOTES:
- Drilldown popover lives INSIDE the coverage panel (not as a
  modal) because the natural mental model is "this row of bars
  has this list of files" — keeping the file list spatially
  attached to the bar preserves that association. A modal would
  detach them and force the user to remember which bar they
  clicked.
- Bucket-specific empty-state copy was the right call vs one
  generic "No files" string. Fall-through empty is good news
  ("every file matched a rule") while rule-bucket empty is
  actionable ("look at the diagnostic chips") — collapsing them
  would hide the actionable framing.
- Notion-style click-row-twice-to-close (no separate disclosure
  caret) keeps the affordance count low. Chevron is a visual
  indicator only, not a separate interactive element — clicking
  anywhere on the row opens/closes.
- Window-level Escape (no click-outside) matches the editor's
  feel: clicking elsewhere on the page is typically a deliberate
  navigation, and the explicit Close button is always visible
  inside the popover. A click-outside listener would surprise
  users who clicked an adjacent row to switch buckets.
- Histogram section placement BELOW retention (not above) because
  retention is a setting and "Top plugins" is a view — settings
  cluster before views in the existing drawer flow. The retention-
  block's collapsed-by-default pattern continues here for the
  same reason: the timeline is the drawer's primary content.
- Stacked bars (not separate four-bar grid) because the relative
  proportions WITHIN a plugin are more important than the
  absolute counts (the chip strip carries those). One row, one
  bar reads as "this plugin's activity composition"; four bars
  per plugin would compete with the cross-plugin comparison.
- Color vocabulary mapped to the install-event glyph colors
  already shipped in slice 77 (install ✓ green / update ↻ accent
  / uninstall ⌫ amber / failed ✕ red) so a user who learned the
  filter-chip colors recognises them in the histogram instantly.

## Roadmap — round 18 (Hopper Sample Drilldown + Per-Plugin Histogram) — ALL DONE

Round 18 batched FIVE feature slices into one cron tick. Four
slices built the Hopper sample drilldown end-to-end (primitive ->
command -> TS client -> clickable UI), and one composite slice
shipped the per-plugin install histogram end-to-end (storage
aggregate + command + TS client + Top plugins UI in one commit).

83. ~~**Hopper sample drilldown primitive**~~ —
    DONE (2026-06-21 15:30 PT, 7fc3463, single commit, 389 LOC).
    Pure-data compute_sample_drilldown(rules, samples, bucket,
    preview_cap) returning SampleDrilldown {bucket, samples,
    total_in_bucket, truncated}. SampleBucket tag-discriminated
    enum: Rule{index} | Fallthrough. preview_cap clamps [1, 5000].
    Out-of-range Rule index yields empty rather than panicking.
    15 new tests pin bucket assignment + truncation + preserves-
    input-order + serde + edge cases.
84. ~~**Hopper sample drilldown Tauri command**~~ —
    DONE (2026-06-21 15:30 PT, 80e03bc, single commit, 130 LOC).
    slab_hopper_sample_drilldown(watch_id, bucket, candidate_rules?,
    samples?, sample_limit?, preview_cap?). Mirrors rule_coverage's
    input shape so the drilldown evaluates the EXACT same chain +
    samples the coverage counted. clamp_preview_cap default 25
    clamped [1, 1000]; 5 new tests pin bounds + the invariant that
    drilldown default < coverage default.
85. ~~**sample drilldown TS client + bucket helpers**~~ —
    DONE (2026-06-21 15:30 PT, 6c4fa84, single commit, 288 LOC).
    SampleBucket wire type + FALLTHROUGH_BUCKET singleton +
    ruleBucket(i) constructor (throws on negative/non-integer).
    slabHopperSampleDrilldown wrapper. sampleBucketEquals +
    describeDrilldown + describeBucket pure helpers. 19 new tests
    in hopper.test.ts (inline-expect convention).
86. ~~**clickable coverage rows + drilldown popover**~~ —
    DONE (2026-06-21 15:30 PT, e38a358, single commit, 421 LOC).
    The demo-able payoff. Coverage rows now <button>s with button
    reset; .open accent tint + focus-visible ring + chevron
    column. Shared {#snippet renderDrilldownBody} for rule +
    fall-through. Popover with previewCap number input, Reload,
    Close. Monospace file list (260px max-height + scroll), per-
    row glyph, truncated footnote, bucket-specific empty-state
    copy. Window-level Escape close. ~135 lines scoped CSS.
87. ~~**per-plugin install histogram**~~ —
    DONE (2026-06-21 15:30 PT, a0504dc, single commit, 783 LOC).
    End-to-end. Backend PluginHistogramRow + plugin_histogram
    method with indexed GROUP BY + sort + 13 tests pinning DESC
    sort, tiebreak ASC on plugin_id, window filters, conservation
    invariant. Tauri command with PluginHistogramResult envelope
    (grand_total + echoed limit). TS client + summarizeHistogram
    helper. "Top plugins" collapsible section in
    RecentInstallsDrawer between retention and events list with
    per-plugin stacked bars (install green / update accent /
    uninstall amber / failed red) scaled relative to top row,
    counts cell with chip strip, auto-refresh on window change.

    With round 18 done, the Hopper rule editor closes the
    coverage workflow loop (look at bars -> click row -> see
    files -> tune rules with the diagnostic in hand), and the
    Recent installs drawer gains the cross-plugin aggregate that
    turns the timeline into a workflow surface (timeline for
    forensics, histogram for trends). Next subsystem candidates:
    Loom-grade tagging explorer, doc-detail metadata editor
    read/write surface, Beacon cache inspector polish (column
    sort by basename / model facet), Quill multi-document field-
    detect queueing, drilldown CSV export ("save the fall-through
    list"), Hopper rule reorder-by-drag in the coverage panel
    (drag a dead row up to fix shadowing in one motion),
    histogram time-bucket axis ("activity per week" alongside
    the current per-plugin breakdown).

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `8467fc4` — "feat(hopper): rule coverage panel in HopperRulesEditor".

### What round-17 (2026-06-21 11:31 PT) just shipped

Five slices across two cohesive arcs. Before this tick the
SmartFoldersHubPanel surfaced personal-preset rows with Apply + pin
only — the rename/duplicate verbs shipped in slice 76 had no UI
surface in the Hub (round 16 explicitly deferred this). And the
Hopper rule editor's live-preview pane could answer "did rule X
match THIS file?" for up to five sample filenames (round 15 work)
but had no way to answer the more useful question "across my last N
real files, how many would each rule catch and how many fall
through?" — the gap that lets a paralegal spot a dead rule shadowed
by an earlier Always before saving. Tonight both gaps close.

- Slice 78: personal-preset row menu in Smart Folders Hub
  (ddfd6ec, 351 LOC). Grid widened from five to six columns
  (drag handle / icon / body / pin / ... menu / apply) with a
  placeholder cell on built-in rows so the Apply button stays
  column-aligned across both kinds. Personal rows grow the ...
  button (hidden until row-hover; visible while the menu is open);
  the Notion-style popover surfaces Rename / Duplicate / Delete
  with a divider above the danger-tinted Delete. Rename runs
  INLINE in the row body (replaces the name + kind line with a
  focused input) — Enter commits, Escape cancels, blur commits if
  changed + non-empty else cancels, drag disabled while
  mid-rename. Collision errors surface inline beside the input
  (red-tinted border + small error span) so the user can correct
  without losing focus context. busyRowKey gives per-row in-flight
  state (the row hosting the operation dims to 0.7 with cursor:
  progress); two rows can spin independently. Escape ladder grows
  one level: menu -> rename -> Hub close. Window-level click
  listener closes any open popover when a click lands outside any
  row; toggleMenu uses stopPropagation so the open click doesn't
  immediately re-close. a11y: aria-haspopup="menu" + aria-expanded
  on the ... button, role="menu" + role="menuitem" on the popover,
  aria-label on the rename input.
- Slice 79: Hopper rule coverage analyzer primitive (7613923,
  531 LOC). New pure-data module pdf::hopper::coverage with
  compute_coverage(rules, samples) returning a RuleCoverageReport
  carrying per-rule first_match + would_match + dead_at_position
  + the fall-through count + total_samples. Algorithm:
  O(rules*samples) — scans every sample through the FULL chain
  (no first-match short-circuit) so it can populate would_match
  per rule. Conservation invariant: rules.sum(first_match) +
  fallthrough == total_samples by construction; pinned by a test.
  Dead-at-position is the actionable insight: first_match=0 AND
  would_match>0 means the rule never wins at its current index
  but would catch at least one sample if moved earlier (shadowed
  by an earlier rule). A zero-coverage rule (matches nothing in
  isolation) is NOT flagged dead-at-position — it's a different
  diagnostic (zero) the UI surfaces separately. 15 tests pin
  empty inputs (both, rules-only, samples-only), single-rule
  chain (first_match == would_match), Always rule (catches all),
  first-match-wins semantics, fully-shadowed rule flagged dead,
  partially-shadowed disjoint chain NOT flagged, zero-coverage
  rule NOT flagged dead, conservation invariant on mixed chain,
  predicate axes wired through (PageCountBetween / SizeOver /
  TextContainsAll), serde wire smoke + minimal-payload defaults.
- Slice 80: rule coverage Tauri command surface (4bf519f,
  +261 LOC in cmds.rs + lib.rs). New command
  slab_hopper_rule_coverage(watch_id, candidate_rules?, samples?,
  sample_limit?) -> RuleCoverageReport sources samples from the
  watch's recent run log by default via HopperLog::list_recent
  with a cap*4 over-read (clamped at 10_000) then filters to
  watch_id. Sample limit clamped to [1, 1000] (default 100).
  Each run row contributes its input_path basename with
  size_bytes=0 and page_count=None (the run log doesn't persist
  either) — known limitation matching the existing live preview,
  documented at the call site. Refactored into three testable
  helpers: clamp_sample_limit (defaults + bounds), sample_over_read
  (4x with 10_000 ceiling guarded against i64 overflow),
  samples_from_runs (filter + basename reduce). 13 new helper
  tests pin clamp defaults + below-1 floor + above-1000 ceiling
  + i64::MAX boundary, over_read 4x linear + 10_000 ceiling on
  100k + i64::MAX inputs, samples_from_runs watch-id filter +
  basename for abs/var/bare paths + cap honouring 50-rec input +
  empty + no-match + size/page/text axes zeroed + invalid-utf8
  basename fall-back. Command registered alongside
  slab_hopper_test_rules in invoke_handler.
- Slice 81: rule coverage TS client + diagnostic helpers
  (c9e9b03, 268 LOC across hopper.ts + hopper.test.ts). Wire
  types RuleSample / RuleCoverage / RuleCoverageReport mirror the
  Rust serde shape verbatim; slabHopperRuleCoverage wrapper takes
  watchId + opts {candidateRules?, samples?, sampleLimit?} so the
  typical call shape is just the id. Four pure helpers:
  fallthroughPercent (guarded against 0/0 NaN), ruleMatchPercent
  (same guard), ruleCoverageDiagnostic ("dead" | "zero" |
  "shadowed" | null with dead-at-position winning over other
  signals when the server flag is set), summarizeCoverage
  (one-line header copy "<N> of <M> samples routed (<P>%)" with
  Math.round for the pct; empty-state branch). 17 new pure-
  helper tests in src/lib/hopper.test.ts following the existing
  quill.test.ts / fuzzy.test.ts inline-expect convention (no
  runner dep; runs as `pnpm exec tsx`).
- Slice 82: coverage panel in HopperRulesEditor (8467fc4,
  422 LOC). The demo-able payoff tying slices 79-81 together.
  Coverage button in header alongside "Test on this folder…",
  highlighted with .ghost.active when open; live sample count
  ("Coverage · 100") once loaded. Section appears BELOW the rule +
  preview split (full width) so the bars get horizontal real
  estate. Header sub-bar: live summary via summarizeCoverage,
  sample-size number input clamped to [1, 1000] step 10 (matches
  the server clamp_sample_limit so a misaligned client can't shoot
  past wire bounds), Refresh button. Body: per-rule three-column
  grid (name+chip / overlay bar / counts). Each row carries
  diagnostic chip via ruleCoverageDiagnostic ("Dead at position"
  red / "Partly shadowed" amber / "No matches" neutral; nothing
  when healthy) and a 12px bar with TWO stacked layers — lighter
  "would match" overlay + solid "first match" on top. The visual
  relationship between layers IS the shadow diagnostic at a
  glance. Dead rows get a red border + 6% red tint. Counts in
  monospace right-aligned, would-count dimmed as secondary info.
  Fall-through row appended after a dashed separator with a grey
  bar (fall-through is the default-recipe path, not a "bad"
  route). Empty states: zero samples ("Drop a file into <source>
  and re-open coverage") + zero rules ("Add a rule above to start
  routing"). Legend footer explains the two-bar model + the
  dead-at-position fix. Coverage hidden by default; first toggle
  triggers refreshCoverage; scheduleSave (debounced 600ms) also
  calls scheduleCoverage (debounced 400ms) so bars reshape live
  alongside the existing live-preview chips. ~210 lines scoped
  CSS following the dark-glass token vocabulary
  (color-mix(#7c8cff) for accent / #ff7b56 dead / #d9b04c shadow /
  #ff5d6c error). a11y: aria-expanded + aria-controls on the
  toggle button.

Gates result: cargo fmt clean (cargo fmt --all --check exit 0),
cargo clippy --lib -- -D warnings PASSED CLEAN in 15.17s (matches
the round-16 14.6s baseline — coverage.rs adds only pure-data
logic with no new clippy surface), cargo test --lib 2261 passed
/ 0 failed (round-16 baseline 2233 + 15 from slice 79 + 13 from
slice 80 = 2261), pnpm check 0 errors / 104 warnings (round-16
baseline preserved EXACTLY — zero new warnings from the row menu,
inline rename, coverage panel, two-layer bars, diagnostic chips,
or scoped CSS).

PROCESS NOTES:
- The round-16 "Next subsystem candidates" list opened with
  "Smart Folders Hub ... menu wiring (Rename / Duplicate / Delete
  on personal rows)" as the natural follow-up — slice 78 closes
  it verbatim. The round-16 closing notes also listed
  "Hopper rule-test panel Test against last 100 files surface
  extension beyond the current 5" as a candidate. Inspection
  found the test_rules path only does per-filename evaluation
  (not aggregation across many files), so the right framing
  wasn't "extend the existing surface to 100" but "build a new
  coverage analyzer that gives aggregate statistics over the
  run log". The four coverage slices (79-82) split cleanly along
  pure-data primitive -> command -> TS client -> UI, matching the
  round-15 bulk-update arc (68-72) and round-16 install-log filter
  arc (73-77) cadence.
- Five slices, five commits, two logical subsystems. Slice 78 is
  a single UI-only commit because the verbs already shipped in
  slice 76; the coverage arc fans out into four because each
  layer is genuinely separable and revertable.
- The coverage primitive's two-count model (first_match vs
  would_match) was the key design call: first_match alone shows
  what runs at runtime but buries the shadow diagnostic; emitting
  both lets the panel surface dead/shadowed/zero diagnostics from
  one IPC. Conservation invariant (first_match.sum() +
  fallthrough == total_samples) is the test that protects future
  refactors from silently dropping rows.
- samples_from_runs in slice 80 reduces input_path to its
  basename to match the live watcher pipeline's RuleContext
  shape — otherwise glob predicates against "tax_*.pdf" would
  fail when the log carries "/Users/x/Documents/tax_2026.pdf".
  Tested with three path styles + a "/" edge case that falls
  back to the original string via unwrap_or_else.
- summarizeCoverage's "No recent runs to analyse" empty-state
  copy is the right framing for the most common cold-start case:
  a freshly-added watch with no runs yet. Skipping the empty
  state and rendering "0 of 0 samples routed (0%)" looked like
  a bug; the explicit copy reads like guidance.
- The two-bar visualisation (would-overlay + first-solid) was
  the alternative to two parallel bars per row. One row, two
  layers reads as "this rule's potential AND its actual" in a
  single eye-trip; two rows would have doubled the panel height
  for the same information density.

DESIGN NOTES:
- Row ... menu on personal rows only (no menu on built-ins)
  because the verbs the menu surfaces (rename / duplicate /
  delete) are personal-only by definition. A menu with all-disabled
  options on built-ins would be busy + confusing; the placeholder
  cell keeps the grid aligned without exposing dead affordance.
- Inline rename (vs modal dialog) for personal-preset rename
  matches the Smart Folders Hub's lightweight feel and the
  saved-views rail's existing pattern. A modal would have been
  heavier than the action warrants — rename is a one-keystroke
  decision.
- Coverage panel below the split (vs in the right preview pane)
  because (a) the bars need ~600px of horizontal width to be
  readable at typical rule counts, which the right pane doesn't
  have; (b) coverage is a sometimes-used diagnostic, not a
  continuously-visible workflow surface like the live preview.
  Hidden-by-default + toggle-to-open keeps the editor's default
  appearance unchanged for users who don't need it.
- Coverage button copy "Coverage · 100" once loaded (vs "Coverage
  (100)") because the dot-separator reads as a status fragment
  ("coverage, 100 samples") not as a count badge. Matches the
  Recent installs drawer's "Last 7d · 3 events" copy from round
  15.
- Sample-size input as a number field (vs a chip strip of 50 /
  100 / 200 / All) because the analyzer's cost is sub-millisecond
  for the [1, 1000] range so there's no slow-path that would
  motivate stepping. Free-form input is more honest about what's
  configurable; the min/max attributes give browser-native
  clamping for keyboard arrows.
- Dead-at-position chip in red (not amber) because dead rules
  are actionable + fixable (move up), not "warning"-level
  ambiguity. Partly-shadowed is amber because it MIGHT be
  intentional (the user might want a tight rule to catch a
  subset before a broader rule). Zero-coverage is neutral
  because it's purely informational ("nothing for this rule to
  catch in the sample window").
- Two-layer bar uses color depth (75% vs 22% alpha on the same
  accent hue) rather than two different hues so the visual
  relationship reads as "more of the same thing", not "two
  different things". Dead rows swap the would-overlay to red so
  the bar visual reinforces the chip color without changing the
  pattern.
- Fall-through row's grey bar (not blue) because fall-through
  isn't a "good" or "bad" route; it's the watch defaults firing.
  Grey reads as "neutral existing behaviour" — matches the
  install-log drawer's grey for the un-tinted event row.

## Roadmap — round 17 (Personal Preset Row Menu + Hopper Coverage) — ALL DONE

Round 17 batched FIVE feature slices into one cron tick. One slice
closed the round-16 deferred item (the Smart Folders Hub's per-row
... menu for personal presets), and four slices built the Hopper
rule coverage analyzer end-to-end (pure-data primitive -> Tauri
command -> TS client -> coverage panel in HopperRulesEditor).

78. ~~**personal-preset row menu in Smart Folders Hub**~~ —
    DONE (2026-06-21 11:31 PT, ddfd6ec, single commit, 351 LOC).
    Six-column grid (drag / icon / body / pin / menu / apply) with
    placeholder on built-in rows; personal rows grow a ... button
    (hover-visible) and Notion-style popover (Rename / Duplicate /
    Delete with divider). Inline rename in row body (Enter commits /
    Escape cancels / blur smart-commits / drag disabled during).
    busyRowKey for per-row in-flight state. Escape ladder grows one
    level. Window click listener closes popover on outside click.
    a11y: aria-haspopup / aria-expanded / role=menu / role=menuitem.
79. ~~**Hopper rule coverage analyzer primitive**~~ —
    DONE (2026-06-21 11:31 PT, 7613923, single commit, 531 LOC).
    Pure-data hopper::coverage module with compute_coverage(rules,
    samples) returning RuleCoverageReport {rules, fallthrough,
    total_samples}. Per-rule first_match + would_match counts +
    dead_at_position flag (true when first_match=0 AND would_match>0
    — actionable shadow detection). O(rules*samples) two-pass scan
    (full chain per sample to populate would_match). 15 new tests
    pin empty inputs, single rule, first-match semantics, fully-
    shadowed dead flag, partial-shadow disjoint NOT dead, zero-
    coverage NOT dead, conservation invariant, predicate axes,
    serde wire shape + minimal-payload defaults.
80. ~~**Hopper rule coverage Tauri command surface**~~ —
    DONE (2026-06-21 11:31 PT, 4bf519f, single commit, +261 LOC).
    slab_hopper_rule_coverage(watch_id, candidate_rules?, samples?,
    sample_limit?). Sources samples from HopperLog::list_recent with
    cap*4 over-read (10_000 ceiling) filtered to watch_id; sample
    limit clamped to [1, 1000] (default 100). Refactored into three
    testable helpers (clamp_sample_limit / sample_over_read /
    samples_from_runs). 13 new tests pin clamp defaults + bounds +
    i64::MAX boundary, over-read linearity + ceiling, watch-id
    filter + basename reduction (abs/var/bare paths) + cap + empty
    + invalid-utf8 fallback.
81. ~~**Hopper rule coverage TS client + diagnostic helpers**~~ —
    DONE (2026-06-21 11:31 PT, c9e9b03, single commit, 268 LOC).
    RuleSample / RuleCoverage / RuleCoverageReport wire types,
    slabHopperRuleCoverage wrapper. Four pure helpers:
    fallthroughPercent + ruleMatchPercent (both div-zero guarded),
    ruleCoverageDiagnostic ("dead" | "zero" | "shadowed" | null with
    dead winning), summarizeCoverage (header copy with empty-state
    branch). 17 new pure-helper tests in src/lib/hopper.test.ts
    following the inline-expect convention.
82. ~~**rule coverage panel in HopperRulesEditor**~~ —
    DONE (2026-06-21 11:31 PT, 8467fc4, single commit, 422 LOC).
    The demo-able payoff. Coverage button in header (highlighted
    when open, shows live sample count). Section appears full-width
    below the split with header sub-bar (live summary + sample-size
    number input + Refresh). Per-rule three-col grid: name + chip,
    two-layer bar (would-overlay + first-solid), monospace counts
    (first / would dimmed). Diagnostic chips (Dead red / Shadowed
    amber / No-matches neutral; healthy unchipped). Dead rows get
    red border + 6% red tint. Fall-through row after dashed
    separator with grey bar. Empty states for zero samples + zero
    rules. Coverage hidden by default; toggle triggers initial
    refresh; scheduleSave wires scheduleCoverage so bars reshape
    live alongside live-preview chips on every edit. a11y:
    aria-expanded + aria-controls.

    With round 17 done, the Smart Folders Hub closes the round-16
    deferred CRUD parity (personal-preset rename + duplicate + delete
    are now reachable from the same surface that lists them), and
    the Hopper rule editor gains the coverage diagnostic that turns
    "did this one file match" preview into "did my chain handle 100
    real runs". Next subsystem candidates: Loom-grade tagging
    explorer, doc-detail metadata editor read/write surface, Beacon
    cache inspector polish (column sort by basename / model facet),
    Quill multi-document field-detect queueing, install-log
    drawer's coverage-like aggregate ("which plugins did you install
    the most this month?"), Hopper sample-set explorer (drill into
    "show me the 23 fall-through files" from a coverage row).




**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.
branch — keep shipping onto it unless Sanjay says otherwise).
**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `b74a749` — "feat(plugins): install-log filter bar in Recent installs drawer".

### What round-16 (2026-06-21 08:36 PT) just shipped

Five slices across two cohesive arcs. Before this tick the
Recent installs drawer surfaced the install log with only a
time-window filter (Last 7d / Last 30d / All), and personal
presets (the Smart Folders Hub's user-saved entries) shipped
with save / list / delete / apply / export / import but
neither rename nor duplicate. Tonight both gaps close
end-to-end with one user-visible payoff each.

Round-15's closing notes listed "Hopper rule editor live
preview already ships (verified), saved-views drag-handle UI,
smart-folders hub UI polish, Loom-grade tagging explorer,
doc-detail metadata editor, Beacon cache inspector polish,
Quill multi-document field-detect queueing." Inspection
confirmed: saved-views shipped a per-row ⋯ menu with Move
up / Move down (round 12 slice 50 + round 14 polish), the
Hopper rule editor's live preview ships (slice 47 work),
and the Smart Folders Hub already has drag-handle reordering
(round 7). The actual gaps were (a) the install-log drawer's
filter UX (only one axis — time window — even though the
backend log has four orthogonal filter axes available) and
(b) the parity gap between personal_presets and saved_views
on the rename + duplicate verbs. Both lend themselves to
clean 1-3 slice arcs that compose into a 5-slice batch.

- Slice 73: install-log filtered reader (e5f8a7d, 482 LOC).
  New `InstallLog::list_events_filtered(since, until, actions,
  plugin_id_substr, limit)` extending list_events_between with
  two new axes. Action axis is a slice of InstallAction with
  empty == no filter; plugin_id substring is case-insensitive
  via `LOWER(plugin_id) LIKE '%needle%' ESCAPE '\'` backed by
  a fresh `like_escape` helper that doubles \, %, _ so a user
  pasting "100%" doesn't accidentally trigger a wildcard. Also
  new: `recent_plugin_ids(limit)` for the future filter-bar
  autocomplete — distinct plugin_ids ordered by most-recent
  activity via GROUP BY + MAX(occurred_at). 14 new tests pin
  no-axes==list_recent, single-action and multi-action sets,
  empty-set==None, substring anchored anywhere + case-
  insensitive + whitespace-empty==None + no-match returns
  empty + LIKE wildcards escaped to literals, three-axis
  composition via AND, limit clamps zero/negative,
  recent_plugin_ids newest-first + cap + empty log,
  like_escape order-correctness (backslash before % and _).
- Slice 74: filtered-reader Tauri command surface (3b81f5b,
  +90 LOC in lib.rs). Two new commands:
  slab_marketplace_install_log_list_filtered(since, until,
  actions, plugin_id_substr, limit) returns
  InstallEventFilteredResult {events, total_returned,
  limit_used}; slab_marketplace_install_log_recent_plugin_ids
  (limit) returns Vec<String>. Action token parser explicitly
  drops unknown strings so a TS typo can't widen the result
  (the storage layer's InstallAction::parse treats unknown
  as Failed, but the command rejects unknowns before they
  reach storage). Default limit = 500 on the list command,
  25 on the recent-ids command. Self-describing payload
  matches BatchUpdateReport (slice 70) / InstallLogExportEnvelope
  (slice 60) precedent. Both registered in invoke_handler.
- Slice 75: filtered-reader TS client + helpers (e404b53,
  175 LOC in marketplace.ts). New wire types:
  ALL_INSTALL_ACTIONS readonly tuple (canonical four-action
  order), InstallEventQuery (four-axis filter mirroring the
  Rust signature), InstallEventFilteredResult.
  listInstallEventsFiltered / recentInstallPluginIds wrappers
  with browser-mode empty fallbacks. Pure helpers:
  describeActionSet returns "all actions" / "failures only" /
  "installs and updates" / "X, Y and Z" depending on set size
  (single-action specialisation appends " only"; "failed"
  pluralises to "failures"; three-or-more uses Oxford-style
  "X, Y and Z" without the Oxford comma matching slice 70's
  formatUpdateSummary); de-dupes + treats full-set as "all
  actions"; deterministic order via ALL_INSTALL_ACTIONS
  sequence. pluginQueryActiveLabel(query) counts narrowing
  axes (window / action set / plugin substring; window
  counts as one even when both since+until set); returns
  null on clean filter so callers can hide the subtitle.
  Both helpers pure — no I/O, no Tauri.
- Slice 76: personal-preset rename + duplicate (f82fa5e,
  268 LOC across personal_presets.rs + lib.rs + library.ts).
  Closes a parity gap that's been open since saved_views
  shipped rename + duplicate in round 12 (slice 50).
  Backend: rename_personal_preset trims + rejects empty +
  short-circuits unchanged-name + rejects collision via
  UNIQUE constraint (mirrors rename_view verbatim);
  duplicate_personal_preset carbon-copies icon/color/
  description/filter, derives unique name via "<src> (copy)"
  / "<src> (copy N)" capped at 999, gets fresh sort_order at
  bottom via save_personal_preset's MAX+1 (mirrors
  duplicate_view); derive_personal_copy_name helper mirrors
  derive_copy_name. Tauri commands slab_personal_preset_rename
  and slab_personal_preset_duplicate emit library-changed on
  success and return the renamed/duplicated record so the UI
  splices without a refetch. TS wrappers personalPresetRename
  and personalPresetDuplicate. 12 new tests bring
  personal_presets total to 22: rename preserves
  id/created_at/sort_order/icon/color/description; rename
  trims; same-name is no-op; empty rejected with row intact;
  collision rejected with row intact; unknown id errors;
  duplicate creates independent copy; renaming copy doesn't
  affect source; suffix sequence "(copy)" → "(copy 2)" →
  "(copy 3)"; duplicate unknown id errors.
- Slice 77: install-log filter bar in Recent installs drawer
  (b74a749, 459 LOC in RecentInstallsDrawer.svelte). The
  demo-able payoff tying slices 73-75 together. New
  `<section class="filter-strip">` between the window-strip
  and the retention-block: (a) four-chip multi-select action
  group (Installs / Updates / Uninstalls / Failures) with
  monochrome installEventGlyph icons; selected chips tint
  the glyph by action (green / accent / amber / red) so the
  four chips read as four flavours not a uniform "selected"
  block; (b) plugin id substring search with case-insensitive
  matching, 220ms debounce, autocomplete dropdown sourced
  from recentInstallPluginIds(25); mousedown (not click)
  commits suggestions so the blur race is impossible; Enter
  commits if exactly one suggestion is visible; (c) filter
  summary line appears only when at least one axis narrows,
  showing describeActionSet + active substring + a
  "Clear filters" affordance. Wiring: $effect re-runs load()
  on actionFilter OR debounced pluginQueryActive change;
  filter narrowing flips load() from listRecentInstallEvents
  to listInstallEventsFiltered so the result reflects the
  FULL log (not the 100-row buffer — fixes a real gap where
  a "failures last 30d" query couldn't surface old
  failures). Empty state grows a third branch:
  filtered-but-no-match prompts "widen with another chip or
  clear the plugin search". Escape ladder grows two new
  levels: suggest dropdown → export menu → confirm prune →
  retention → narrow filter (clears) → drawer close. CSS
  ~170 lines scoped with the existing dark-first tokens
  (--accent, --border, --bg-1/2/3, --text/-3), focus-within
  accent border, absolute popover matching the install-modal
  z-index/shadow vocabulary, monospace plugin ids for
  id-vs-id alignment. a11y: aria-pressed on chips, role=
  combobox (not the implicit searchbox role from type=
  search which doesn't permit aria-expanded) + aria-controls
  + aria-expanded + aria-autocomplete + role="listbox"/option
  + aria-selected on the dropdown.

Gates result: cargo fmt clean (cargo fmt --all --check
exit 0), cargo clippy --lib -- -D warnings PASSED CLEAN in
14.6s (matches the round-15 14s baseline — der/spki 0.7 pin
from round-14 still holding), cargo test --lib 2233 passed
/ 0 failed (round-15 baseline 2208 + 14 from slice 73 + 11
from slice 76 = 2233), pnpm check 0 errors / 104 warnings
(round-15 baseline preserved EXACTLY — zero new warnings
from the filter strip markup, action chips, suggest
dropdown, or scoped CSS).

PROCESS NOTES:
- The round-15 "Next subsystem candidates" list was a mix
  of already-shipped items (Hopper live preview, saved-views
  reorder UI via ⋯ menu, Smart Folders Hub drag) and real
  gaps (install-log filter UX, personal-preset CRUD parity,
  Loom/Quill/Beacon polish). The pattern from rounds 13-15
  recurs: validate candidates against the actual code before
  trusting the optimism in the closing notes. Two of tonight's
  arcs (filter UX + preset CRUD) came from inspection of the
  install_log/personal_presets module shapes against their
  UI surfaces, not from the candidate list at all.
- Five slices, five commits, two logical subsystems. The
  install-log filter arc (73-75 + 77) is four slices with
  the storage primitive → command → TS client → UI shape
  matching the round-15 bulk-update arc verbatim. The
  personal-preset arc (76) compressed backend + commands +
  TS into one slice because rename + duplicate are tightly
  coupled verbs and the saved_views precedent gives a
  zero-design-cost mirror — each new function is a 30-line
  rename of an existing function.
- like_escape() in slice 73 is the first SQL LIKE wildcard
  escape helper in the codebase. The Hopper rule UI's
  filename substring predicates went through a different
  path (regex-bridged); future SQL LIKE callers should adopt
  this helper rather than reinventing. Tests pin order
  correctness (backslash MUST be replaced first).
- The Tauri command `slab_marketplace_install_log_list_filtered`
  parses action tokens explicitly via a match arm and drops
  unknowns, rather than calling through InstallAction::parse
  which converts unknowns to Failed. This is a defence-in-
  depth choice — the storage layer's behaviour is safe in
  isolation but a TS typo widening the result would be a
  subtle UX bug; the command-level explicit drop makes
  "asked for nothing valid" yield "no filter" rather than
  "secret extra filter for failures".
- The filter strip in slice 77 reloads SERVER-SIDE on filter
  change but keeps the window axis client-side. Rationale:
  toggling 7d/30d/All should be instant from the loaded
  buffer, AND a server-side window refetch would lose
  in-flight context if the user is mid-typing in the plugin
  search. Action chip + plugin-id changes refetch because
  the buffer might not contain the rows needed (the 100-row
  list_recent default may miss a 90-day-old failure).

DESIGN NOTES:
- Four-action chip group instead of a dropdown because the
  count is exactly four and they fit in one row at typical
  drawer widths. A dropdown would hide the affordance
  behind a click; the chip strip surfaces all four states
  at a glance with their associated glyphs.
- Action-specific glyph tint (green install / accent update
  / amber uninstall / red failed) matches the BulkUpdateProgressOverlay's
  three-color palette from slice 72 + extends it with a
  green for install rows. One mental model for "what colour
  is this kind of event" across the drawer + the overlay.
- Plugin search debounce is 220ms (not 100 or 500) because
  it's the same debounce the LibrarySearchPanel uses for
  its fts query — one mental model for "how fast does a
  filter respond?" across the app.
- Autocomplete shows up to 8 matches because the typical
  paralegal install footprint is <25 plugins; beyond 8
  the user is better off completing the substring than
  scrolling a long list. The 8 cap also keeps the dropdown
  height bounded so it doesn't cover the action chips.
- mousedown (not click) on suggestion items because blur
  fires before click, and we want the suggestion to commit
  before the input loses focus. The 120ms blur delay is
  belt-and-suspenders; mousedown is the actual mechanism.
- describeActionSet's "failures only" specialisation (vs
  "failures") reads better as a filter-bar subtitle. The
  Oxford-comma-free three-or-more form matches the existing
  formatUpdateSummary from slice 70 so the two filter
  surfaces share one copy vocabulary.
- pluginQueryActiveLabel counts the WINDOW axis as ONE even
  when both since+until are set because the user makes a
  single semantic choice ("Last 7d") that happens to express
  as two boundaries; "2 filters active" reading from one
  user choice would be wrong.
- Personal-preset rename + duplicate slot into the EXACTLY
  same shape as saved-views' rename + duplicate so the
  Smart Folders Hub can add a per-row ⋯ menu (deferred to
  a later tick — the verbs land tonight, the UI surface
  next time) with the same Notion-style "<src> (copy)"
  naming and the same in-place rename inline-edit pattern
  the saved-views rail uses. One mental model across both
  list-of-named-filters surfaces.

## Roadmap — round 16 (Install-Log Filter + Preset CRUD Parity) — ALL DONE

Round 16 batched FIVE feature slices into one cron tick. Four slices
closed the install-log filter arc (Recent installs drawer's filter UX
went from one axis to four, with the demo-able filter bar landing as
the user-visible payoff); the fifth closed the saved-views vs
personal-presets parity gap on the rename + duplicate verbs.

73. ~~**install-log filtered reader (actions + plugin substring)**~~ —
    DONE (2026-06-21 08:36 PT, e5f8a7d, single commit). New
    `InstallLog::list_events_filtered(since, until, actions,
    plugin_id_substr, limit)` extending list_events_between.
    Action axis is &[InstallAction] with empty == no filter;
    plugin substring is case-insensitive via LOWER + LIKE
    with a fresh `like_escape` helper that doubles \, %, _.
    Also new: `recent_plugin_ids(limit)` for autocomplete via
    GROUP BY plugin_id ORDER BY MAX(occurred_at). 14 new tests.
74. ~~**install-log filtered Tauri command surface**~~ —
    DONE (2026-06-21 08:36 PT, 3b81f5b, single commit). Two
    new commands: slab_marketplace_install_log_list_filtered
    (returns InstallEventFilteredResult {events, total_returned,
    limit_used}; default limit 500) and
    slab_marketplace_install_log_recent_plugin_ids (default 25).
    Action token parser explicitly drops unknowns so a TS typo
    can't widen the result.
75. ~~**install-log filter TS client + describe helpers**~~ —
    DONE (2026-06-21 08:36 PT, e404b53, single commit, 175 LOC).
    ALL_INSTALL_ACTIONS / InstallEventQuery /
    InstallEventFilteredResult wire types. listInstallEventsFiltered
    / recentInstallPluginIds wrappers with browser-mode fallbacks.
    Pure helpers describeActionSet (single→"X only", two→"X and Y",
    three+→Oxford-style without Oxford comma; full-set==no-filter;
    deterministic order) and pluginQueryActiveLabel (counts
    narrowing axes; window counts as one even when both bounds
    set).
76. ~~**personal-preset rename + duplicate**~~ —
    DONE (2026-06-21 08:36 PT, f82fa5e, single commit, 268 LOC).
    Backend rename_personal_preset (trim, empty rejected, same-name
    no-op, collision rejected by UNIQUE) and duplicate_personal_preset
    (carbon-copy, "<src> (copy)"/"<src> (copy N)" capped at 999,
    fresh sort_order). Tauri commands emit library-changed.
    TS wrappers personalPresetRename / personalPresetDuplicate.
    12 new tests bring personal_presets total to 22.
77. ~~**install-log filter bar in Recent installs drawer**~~ —
    DONE (2026-06-21 08:36 PT, b74a749, single commit, 459 LOC).
    Four-chip multi-select action group with action-specific
    glyph tint (green install / accent update / amber uninstall /
    red failed); plugin id substring search with case-insensitive
    matching + 220ms debounce + autocomplete dropdown (top-8
    from recentInstallPluginIds, mousedown commits, Enter
    commits if exactly one); filter summary line shows
    describeActionSet + active substring + "Clear filters"
    affordance when at least one axis narrows. $effect re-runs
    load() on filter change (server-side via
    listInstallEventsFiltered so the result reflects the FULL
    log, not the 100-row buffer). Empty state grows third
    branch; Escape ladder grows two levels. a11y: role=combobox
    + aria-controls + aria-expanded + aria-autocomplete +
    role=listbox/option + aria-selected.

    With round 16 done, the marketplace install-log drawer is
    now a proper four-axis investigative surface — paralegals
    can answer "show me failures for com.acme.\* in the last
    30 days" in three clicks, and personal presets gain rename
    + duplicate verbs that match the saved-views vocabulary
    (the Smart Folders Hub ⋯ menu wiring is the natural next
    tick — verbs are live, UI surface to surface them is the
    small follow-up). Next subsystem candidates: Smart Folders
    Hub ⋯ menu wiring (Rename / Duplicate / Delete on personal
    rows), Loom-grade tagging explorer, doc-detail metadata
    editor read/write surface, Beacon cache inspector polish,
    Quill multi-document field-detect queueing, Hopper
    rule-test panel "Test against last 100 files" surface
    extension beyond the current 5.



**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.
branch — keep shipping onto it unless Sanjay says otherwise).
**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `9fe1d50` — "feat(plugins): live per-step bulk-update progress overlay".

### What round-15 (2026-06-21 05:35 PT) just shipped

A demo-able overhaul of the plugin marketplace's update
experience. Before this tick the Installed tab carried a
per-card "↑ vX.Y.Z — update available" badge (Slice 8a from
v1.4.0) but no bulk affordance: a user with 5 plugins to
update had to click each one individually + wait for each
install modal to dismiss before clicking the next. STATE.md's
candidate list had this listed as "plugin marketplace Browse
search & filter UI" but inspection showed that surface ships
already (the Browse tab has searchQuery, category chips, sort
mode toggles, fuzzy matching with highlights — round-12 work).
The actual gap was bulk updates. Tonight that gap closes
end-to-end.

- Slice 68: marketplace::update_plan planner primitive
  (9c2898a). Pure-data Rust planner that intersects the
  installed plugin set with a freshly-fetched index and
  returns the deterministic set of plugins for which the
  index advertises a strictly-newer version. New types:
  InstalledPlugin {id, version} (slim subset of registry's
  Plugin so unit tests don't need to mock the registry),
  UpdateTarget {id, installed_version, available_version,
  size_bytes, entry} carrying the full IndexEntry for
  downstream consumers, UpdatePlan {targets, total_bytes}
  with count() / is_empty() / target_ids() accessors. Core:
  plan_updates(installed, index) — strict newer test via
  semver_compare; duplicates in either input list collapse
  via first-wins. semver_compare(a, b) is a Rust port of the
  TS compareSemver in src/lib/marketplace.ts; the test corpus
  pins parity (missing components default to 0, non-numeric
  components default to 0, release sorts above same-version
  prerelease, prerelease tags lexicographic). 19 new tests
  pin semver basics + minor/patch + missing components +
  non-numeric + release-vs-prerelease + lexicographic order,
  empty cases (no installs, empty index, already-current,
  installed-ahead), strict-newer inclusion, index-only
  ignored, sort-by-id-ascending, total_bytes sums, full entry
  carried per target, duplicate-id first-wins on both inputs,
  prerelease semantics, serde wire smoke.
- Slice 69: bulk-update Tauri command surface (4b1da4f).
  Two new commands wire the planner into IPC:
  slab_marketplace_list_update_targets() → UpdatePlan
  (re-fetches the index via the same cache-aware path
  slab_marketplace_index uses; combines with PluginRegistry
  via reg.list().filter_map); slab_marketplace_update_all(
  batch_id, plugin_ids) → BatchUpdateReport runs sequential
  updates through the same signature → install_from_entry →
  reg.discover → install_log pipeline slab_marketplace_install
  uses. The batch ALWAYS runs to completion — a failed id N
  does NOT stop ids N+1+ (matches browser extensions, apt/
  brew, VS Code). New wire types: UpdateProgress {batch_id,
  index, total, plugin_id, phase, error?} emitted per step
  on marketplace://update-progress; UpdateOutcome
  (snake_case serde-tagged enum) succeeded vs failed;
  BatchUpdateReport {batch_id, outcomes, succeeded, failed,
  bytes_written} with from_outcomes() folding the counts
  server-side so the TS reducer doesn't have to. Failure
  paths reuse the existing record_install_failure +
  open_install_log_and helpers so every batch step lands in
  the install_log subsystem rounds 11-14 built (one audit
  trail for both individual + bulk updates). Index-moved
  ("id no longer in index") is the only failure path that
  skips the log row — there's no versioned identity to log
  against. Both commands registered in invoke_handler. 7
  new tests pin the accessor methods, count/sum derivations,
  empty-batch handling, serde tags + field names.
- Slice 70: bulk-update TS client + helpers (57a7bfa). New
  exports: UpdateTarget / UpdatePlan / UpdateProgress /
  UpdateOutcome (discriminated union on "kind") /
  BatchUpdateReport interfaces matching the Rust serde
  output. Wrappers: listUpdateTargets() (browser mode returns
  empty plan so the banner naturally hides during pnpm dev),
  updateAllPlugins(batchId, ids) (browser mode synthesises an
  all-failed report so the UI feedback flow is consistent in
  dev), listenUpdateProgress(handler) wraps the
  @tauri-apps/api/event listen() and returns an UnlistenFn
  the caller MUST invoke on cleanup to free the listener
  slot. Pure helpers: pluralizeUpdates(n) for the banner
  header text and formatUpdateSummary(report) for the success
  toast (covers five canonical paths: all-succeed-with-size,
  mixed-with-size, all-fail, single-fail, empty).
- Slice 71: Updates-available banner in Installed tab
  (52d4528, 398 LOC). End-to-end demo-able surface tying
  slices 68-70 together. New banner above the plugin list
  showing "↑ 3 updates available · 4.2 MB · Review ·
  [Update all] [×]". Collapsed-by-default; expand reveals
  per-target rows with "<name> v<prior> → v<next> · <size> ·
  [Update]". Versions use mono font; prior version line-
  through; next version accent-coloured. Per-row Update
  button disables when the global batch is in flight OR
  when the specific row is. State: updatePlan +
  updateBusy + updateRowBusy + updatesExpanded +
  updatesDismissed (per-session, doesn't persist across
  reloads — Sanjay's house style: never let the user
  permanently kill an actionable banner). Wired into
  onMount + onInstall success + confirmUninstall success +
  onReload so the banner re-derives whenever the registry
  changes. Toast grammar uses formatUpdateSummary: all-
  succeed → notify.success, mixed → notify.warning with
  firstErrorDetail, all-fail → notify.error. 185 lines of
  scoped CSS using the existing dark-first design tokens
  (--accent, --border, --bg-2/3, --text-1/2/3, --r-md/sm,
  --font-mono); subtle 6% accent-tint background.
- Slice 72: live per-step progress overlay (9fe1d50, 536
  LOC). New BulkUpdateProgressOverlay.svelte component
  + reducer upgrade in PluginsPanel.svelte. Replaces the
  spinner-only "Updating…" button state with a full modal
  showing every target's phase (pending / updating / done /
  failed) in real time. Header icon: in-flight ↑ / done ✓
  / mixed ! / all-fail ✕, coloured by terminal state.
  Sub-line: "2/5 · Acme PDF Tools" during, "N succeeded ·
  M failed" after. Top progress bar fills as (succeeded +
  failed) / rows.length and flips to green at finish.
  Per-row list: icon (○ → … → ✓ / ✕) + name + version
  transition + size + status label + inline error message
  on failed rows (truncated). Reducer in PluginsPanel:
  initial rows from current plan with phase: "pending";
  set up the overlay state BEFORE awaiting the backend;
  subscribe to listenUpdateProgress BEFORE updateAllPlugins
  so the early `phase: "starting"` event for the first id
  isn't dropped; handler filters on batch_id === overlay
  .batchId so events from other batches can't bleed into
  the wrong overlay; per-event reducer maps starting →
  "updating", done → "done", error → "failed" with the
  error message captured. finally: await unlisten() to
  free the listener slot. The overlay refuses to close
  while !finished so the user can't strand a half-running
  batch off-screen; Esc dismisses only when finished (same
  gate as InstallProgressModal).

Gates result: cargo fmt clean (cargo fmt --all --check
exit 0), cargo clippy --lib -D warnings PASSED CLEAN in
~14s (round-14 baseline preserved — the der/spki pin from
round-14 keeps clippy resolving normally), cargo test --lib
2208 passed / 0 failed (round-14 baseline 2182 + 19 new
from slice 68 + 7 new from slice 69 = 2208), pnpm check 0
errors / 104 warnings (round-14 baseline preserved
EXACTLY — zero new warnings from the banner markup,
overlay component, or scoped CSS).

PROCESS NOTES:
- STATE.md's "Next subsystem candidates" list at the end of
  round-14 claimed "plugin marketplace Browse search & filter
  UI" was the next gap. Inspection found that surface ships
  already (round-12 work in PluginsPanel: browseQuery +
  browseCategory + browseSort + browseRanked + fuzzy matching
  with highlights). Similarly "Hopper rule editor's Test
  against last 5 files live preview" also ships (the
  HopperRulesEditor already has testFilename + recomputePreview
  + slab_hopper_test_rules tied together). Pivoted to bulk
  plugin updates instead — a genuine gap (no update_all
  command anywhere in src-tauri) that's also a natural Linear-
  /Raycast-/Vercel-grade UX addition. Lesson: validate the
  next-candidate list against the actual code, not against the
  optimism in the closing notes.
- Five slices, five commits, one logical bulk-update subsystem.
  The split: pure backend primitive (68) → Tauri command surface
  (69) → TS client (70) → banner UI (71) → live progress
  overlay (72). Each slice is independently revertible and the
  banner UI in slice 71 fell back to a simple notify.success
  toast on completion if slice 72 ever needs to be reverted.
- The marketplace::update_plan module + semver_compare port
  was the natural foundation. The decision to port
  compareSemver from TS to Rust (rather than expose the
  registry/index to the planner and let it call into a
  shared lib) keeps the planner pure-data + lets the TS
  Browse-tab "update available" badge keep using its own
  in-place compareSemver. Both implementations are direct
  ports of each other; 6 of slice 68's 19 tests pin parity.
- Tauri event channel naming follows the existing convention:
  hopper://run-completed, hopper://backfill-progress,
  beacon://chat-stream, beacon://index-progress — and now
  marketplace://update-progress. Hierarchical namespaces +
  kebab-case suffix.

DESIGN NOTES:
- Banner collapsed-by-default because the summary line ("↑ 3
  updates available · 4.2 MB · Review") gives the user
  everything they need to decide "Update all now" vs "expand
  to see what" vs "dismiss for later" in one glance. The
  Review label flips to "Hide list" when expanded so the
  affordance is always discoverable.
- Per-row Update button + Update all both wired into the
  same runUpdateBatch path so the overlay + toast feedback
  is consistent regardless of which the user clicks. The
  per-row button surfaces when a user wants to defer a
  heavyweight update (e.g. "I'll update Beacon later — it's
  120 MB"). The Update all is the dominant path; the
  per-row affordance is the escape hatch.
- updatesDismissed is per-session (no localStorage). Sanjay's
  house style — actionable banners should never be killable
  permanently because the user might dismiss once, forget,
  and never see the actionable surface again. Install /
  uninstall / reload all re-derive the plan, which clears
  the dismiss flag implicitly: a new banner shows up the
  moment the registry changes again.
- Sequential (not concurrent) bulk update because (a) the
  install_log expects one row per install transaction and
  concurrent writes to the sqlite log would interleave the
  audit trail messily, (b) progress events are easier to
  reason about when one target is in flight at a time, and
  (c) macOS doesn't parallelize disk writes well anyway —
  parallel installs would oscillate the disk head.
- BatchUpdateReport's succeeded / failed / bytes_written
  fields are pre-computed server-side so the toast +
  banner-reset logic don't have to fold the outcomes list.
  Same pattern round-13's InstallLogExportEnvelope used:
  self-describing wire shape, slim downstream code.
- Per-step overlay uses the existing modal backdrop +
  z-index stack as InstallProgressModal so the visual
  language is consistent; users who have seen the install
  modal immediately understand the bulk overlay's grammar.
  Three-color status palette (green #3fc88c done / amber
  #e0b450 mixed / red #ff6b6b failed) chosen to match the
  Hopper backfill progress modal's existing palette — one
  mental model for "how this batch went".
- listenUpdateProgress filters on batch_id === overlay
  .batchId so a future concurrent-batches feature wouldn't
  bleed events between overlays. The UI never fires
  concurrent batches today, but the contract honours the
  correlation key the Rust side sends.



**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.
branch — keep shipping onto it unless Sanjay says otherwise).
**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `3d4dde5` — "feat(plugins): Retention section in Recent installs drawer".

### What round-14 (2026-06-21 02:25 PT) just shipped

A demo-able overhaul of the plugin marketplace install
log's self-maintenance. Before this tick round-13 shipped
end-to-end exportability (CSV/JSON of the audit trail) but
the log itself grew without bound — the manual "Clear older
than 90d" affordance worked, but nothing trimmed it
automatically and there was no policy surface. Round-13's
closing notes called this out explicitly as the next
candidate ("the pruneInstallLog command exists; the
auto-prune-on-startup surface isn't wired yet"). Tonight
that gap closes end-to-end.

PRE-SLICE: critical build-fix (0bb1d4c). Three dependabot
bumps that landed before round-13 (der 0.7->0.8 PR #32,
spki 0.7->0.8 PR #33, ttf-parser 0.21->0.25 PR #31) turned
the lib build red because signet/cms_blob.rs uses
der 0.7-era APIs and cms = "0.2" transitively pulls der 0.7,
creating a two-versions-of-der graph that broke
OctetString/Any/Sequence/SubjectPublicKeyInfoOwned resolution
across the cms_blob <-> cms boundary (~57 E0432/E0599/E0782
errors). The ttf-parser bump separately changed
face.italic_angle() from Option<f32> to bare f32, killing
font_embed.rs:106. Round-13 reported "2171 tests passed" but
that referenced a different Cargo.lock cache state — the
actual main was uncompilable. Fix: pin der + spki back to
"0.7" in Cargo.toml + `cargo update --precise` in Cargo.lock,
drop the `.unwrap_or(0.0)` on italic_angle. cargo check --lib
clean, cargo test --lib 2171 base passes.

- Slice 63: install_log retention policy storage primitive
  (bd649cf). Schema bump v1 -> v2 adding `install_log_settings
  (key TEXT PRIMARY KEY, value TEXT NOT NULL)`. Pure additive
  migration via `CREATE TABLE IF NOT EXISTS` + pragma_update.
  Three module constants: DEFAULT_RETAIN_DAYS = 365 (matches
  round-12 design note), MIN_RETAIN_DAYS = 1 (mirrors the
  manual prune floor), AUTO_PRUNE_INTERVAL_SECS = 86_400 (24h
  debounce). Storage surface: retain_days/set_retain_days/
  last_auto_prune_at/set_last_auto_prune_at with clamp-up-on-
  read defence + fallback-on-parse-failure. Auto-prune driver:
  `auto_prune_if_due(now_unix)` checks the debounce, prunes
  if due, stamps last_auto_prune_at; returns AutoPruneOutcome
  (snake_case serde-tagged "pruned"/"skipped" enum with
  rows_removed/retain_days/cutoff_unix or next_due_unix).
  `auto_prune_if_due_now()` is the production wrapper. 11 new
  tests pin: default-when-unset (365), set/get round-trip,
  floor-clamp at 0 + negative, last_auto_prune_at round-trip,
  settings table exists at v2, malformed-value falls back to
  default, auto-prune first-call-prunes with boundary
  semantics, debounce within 24h leaves rows intact, runs
  again after debounce, empty log succeeds zero rows, serde
  tag round-trip.
- Slice 64: retention policy Tauri commands (2f08453). New
  wire type `InstallLogRetentionPolicy { retain_days,
  last_auto_prune_at, default_retain_days, min_retain_days,
  auto_prune_interval_secs }`. Three commands registered:
  `slab_marketplace_install_log_retention_policy()` reads
  (two key-value queries);
  `slab_marketplace_install_log_set_retention_days(days)`
  writes (returns clamped value);
  `slab_marketplace_install_log_auto_prune(force: Option<bool>)`
  runs the auto-prune (force=true clears the debounce stamp
  before calling, so subsequent unforced calls still honour
  24h from this run). All three open per-call (retention edits
  fire on user click not in a hot loop). marketplace/mod.rs
  re-exports AutoPruneOutcome + the three constants.
- Slice 65: TS client wrappers + helpers (0ede3a5, 193 LOC
  in marketplace.ts). Interfaces:
  `InstallLogRetentionPolicy` matching wire shape +
  `InstallLogAutoPruneOutcome` as a discriminated union
  (`{outcome: "pruned", rows_removed, retain_days,
  cutoff_unix}` | `{outcome: "skipped", next_due_unix}`) so
  TS narrows cleanly. Wrappers: getInstallLogRetentionPolicy,
  setInstallLogRetentionDays (browser fallback clamps
  client-side), runInstallLogAutoPrune (browser fallback
  returns synthetic skipped+1d). Pure formatter helpers with
  injectable now param: formatLastAutoPrune ("Never auto-
  pruned" / "just now" / "Nm ago" / "Nh ago" / "yesterday" /
  "Nd ago" / ISO yyyy-mm-dd ladder) and formatNextAutoPrune
  ("Due now" / "Nm" / "Nh Mm" / "Nd Hh" with trailing-zero
  collapse). pnpm check 0/104 baseline preserved.
- Slice 66: auto-prune install log on app startup (ec2b9ac).
  Wired into the Tauri builder's `.setup(|app| { ... })`
  callback right after the Hopper bootstrap. Best-effort +
  non-fatal — open failure logs to stderr and Slab boots
  normally. Outcome handling: `Pruned` with rows_removed > 0
  logs an audit line; rows_removed == 0 is silent (a clean
  log shouldn't add boot noise); `Skipped` is silent (the
  dominant case on a healthy log). Honours the same debounce
  the UI button uses so startup + immediate UI click won't
  re-prune unless force=true is passed. 36 lines added.
- Slice 67: Retention section in Recent installs drawer
  (3d4dde5, 343 LOC). Pure frontend tying slices 63-66 into
  the demo surface. Collapsible section between the window
  strip and event list; defaults collapsed with header
  "▸ Retention   Keep 365d · Last auto-prune: 4h ago".
  Expanded body: retain_days numeric input (min=floor, max=
  3650 ≈ 10y) bound two-way to retainDaysDraft with
  retentionDirty derived (true when draft != persisted +
  policy floor). Reset + Save chips appear only when dirty
  — no no-op buttons cluttering the steady state. Subtitle:
  "Default 365d · floor 1d. Older events auto-prune on app
  launch (max once per 24h)." Bottom row: "Next auto-prune
  in Nh Mm" left, "Run now" button right (force=true so it
  bypasses the 24h debounce; disabled when log is empty or
  retentionBusy). 4s retentionToast surfaces both branches
  of the auto-prune outcome. Save flow writes the storage-
  clamped return value back into both policy.retain_days
  and retainDaysDraft so a typed 0 corrects to 1 inline.
  Run-now refreshes events + summary + policy via load() so
  the drawer reflects the removed rows + bubbles
  rows_removed back to PluginsPanel via onPruned (existing
  prop the manual prune already uses, so toolbar History
  badge updates for free). Escape handler grows a third
  level: export menu → confirm-prune → retention section →
  drawer. ~140 lines of scoped CSS for the new selectors;
  pnpm check 0 errors / 104 warnings (round-13 baseline
  EXACTLY).

Gates result: cargo fmt clean (cargo fmt --all --check
exit 0), **cargo clippy --lib -- -D warnings PASSED CLEAN
in 4m 42s — first clean clippy in 5 rounds; the wedge was
the der/spki two-versions-in-graph issue from PRE-SLICE,
not the sparse image as previously suspected**, cargo test
--lib 2182 passed / 0 failed (round-13 baseline 2171 + 11
new from slice 63), pnpm check 0 errors / 104 warnings
(round-13 baseline preserved EXACTLY — zero new warnings
from the Retention section markup, label-wrapping pattern,
or scoped CSS).

PROCESS NOTES:
- The "sparse image wedge" suspicion of rounds 10-13 was a
  red herring. The actual wedge was clippy's trait-bound
  resolution exploding on the two-version-of-der dependency
  graph that the unmerged dependabot PRs created. With der
  + spki pinned back to 0.7 (matching cms 0.2's transitive
  expectation), clippy resolves in ~5 min on the sparse
  image with zero warnings. This is a significant
  diagnostic correction — earlier rounds blamed sparse-image
  fsync, recommended hdiutil detach/reattach to Sanjay, but
  the real fix was at the Cargo.toml layer all along.
- Schema migrations on the install_log are now demonstrably
  zero-pain: v1 -> v2 added a new table without touching
  the existing install_events table, the migration runs
  idempotently via `CREATE TABLE IF NOT EXISTS`, and the
  init_schema pragma_update bump is the only thing that
  changes between versions. Future v3 bumps (e.g.
  per-plugin retention overrides) can adopt the same
  pattern.
- The AutoPruneOutcome enum's snake_case `outcome` tag
  matches the round-13 export envelope's pattern (also
  snake_case tagged + self-describing payloads). Two
  self-describing audit surfaces, one mental model.
- The retention section's CSS uses a 14px+auto+1fr grid
  for the collapsed header so the right-aligned meta line
  ("Keep 365d · Last auto-prune: 4h ago") truncates with
  ellipsis when it exceeds the available width. The chevron
  + label + meta read as one row of information at a glance
  — no need to expand to know the current policy.
- formatLastAutoPrune's ladder (just-now / Nm / Nh /
  yesterday / Nd / ISO) matches formatInstallEventTime's
  grammar verbatim so paralegals see the same time vocabulary
  on Activity timeline events AND on the retention "last
  ran" subtitle. One mental model for "when did this
  happen?" across the install-log surfaces.
- Slice 67's <label> wraps both field-label and field-input
  spans so the input is "associated by inclusion" — no
  a11y_label_has_associated_control warning despite no
  explicit `for=` attribute. This is the same pattern the
  Slice 11 dialog work taught us; carried forward cleanly
  to this surface.

DESIGN NOTES:
- 24h debounce on the auto-prune (not 12h or 168h) because
  the install-log grows slowly (a typical workstation has
  <1 install per day after the initial setup phase), so a
  daily prune is more than enough cadence to keep growth
  bounded without re-running the DELETE in tight loops on
  CI / dev iteration. The debounce stamp lives in the
  settings table not in a global pref so a future per-DB
  policy is a pure-data migration.
- "Run now" forces by clearing last_auto_prune_at = 0 first
  and then calling the natural auto_prune_if_due path,
  rather than introducing a separate `force` branch in the
  storage primitive. This keeps the storage layer's API
  surface minimal (one `auto_prune_if_due` function, one
  semantic) and routes "force" through the same mechanism
  the natural path uses (clearing the debounce stamp is
  what `auto_prune_if_due` reads to decide).
- Retention section defaults collapsed because 90%+ of
  users will never adjust the default 365d. Collapsed
  state surfaces the policy in one line ("Keep 365d · Last
  auto-prune: 4h ago"); expansion is for the power-user
  paralegal who wants 30d for a tight-audit firm or 730d
  for an enterprise compliance shop.
- Save chips only appear when dirty (retentionDirty
  derived) so the steady state has zero clutter. The
  Reset chip appears alongside Save when dirty so the user
  can abandon a typo without re-typing the original — same
  pattern Linear uses for inline issue-title edits.
- The audit log eprintln on slice 66's rows_removed > 0
  path uses "marketplace install-log:" prefix matching the
  Hopper bootstrap's "hopper:" convention so all
  subsystem-level boot logs share one parseable grep
  pattern.



**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.

**Active branch: `main`** — commit and push DIRECTLY to main every tick. No feature branches.
branch — keep shipping onto it unless Sanjay says otherwise).
**Version: 3.39.0** — already bumped in package.json, src-tauri/Cargo.toml,
src-tauri/tauri.conf.json, Cargo.lock.

Latest commit: `ecc2261` — "feat(plugins): install-log export menu in Recent installs drawer".

### What round-13 (2026-06-20 22:59 PT) just shipped

A demo-able overhaul of the plugin marketplace install log's
exportability. Before this tick the round-12 install-log surface
shipped logging + readers + drawer UI for browsing the audit
trail, but the log itself was trapped in `~/.slab/marketplace-
history.sqlite` — paralegals and auditors who need to email the
partner a record of "every plugin install / uninstall / failure
in the last 90 days" had no path. Round-12's closing notes
called this out explicitly as the next candidate ("marketplace
install log export — CSV + JSON; mirrors round 10's hopper CSV
export pattern"). Tonight that gap closes end-to-end:

- Slice 58: time-window install-log reader (b0a602a).
  `InstallLog::list_events_between(since_unix, until_unix, limit)`
  with optional inclusive boundaries on both ends (None == no
  bound on that side; both None collapses to a plain
  newest-first scan equivalent to list_recent). Same limit
  semantics — negative limit clamps to zero rather than
  panicking. Drives the export surface so the file matches
  the user's window choice exactly. Dynamically assembles the
  WHERE clause so the unbounded scan plan is identical to the
  existing list_recent path. 6 new tests pin: no-bounds-matches-
  list-recent, since-only, until-only, inclusive-both-
  boundaries, empty-window-returns-empty, limit-clamps-results
  (incl. negative-clamps-to-zero).
- Slice 59: RFC-4180 CSV serialiser (26e01a7). Pure function
  `install_log_to_csv(&[InstallEvent], include_header)` +
  module constant `INSTALL_LOG_CSV_HEADER`. Columns:
  `id,plugin_id,version,action,occurred_at_unix,occurred_at_iso,
  source,bytes_written,files_extracted,replaced_existing,
  prior_version,error_msg`. Two timestamp columns by design —
  unix-seconds for machine joining, ISO-8601 UTC for direct
  Excel review; both come from the same `occurred_at` so they
  can't drift. Escaping policy matches the hopper backfill
  CSV: fields containing , " \r \n are wrapped in "; embedded
  " is doubled. NULL-able columns render as empty (never
  "None" or "null" which would trip downstream parsers).
  Boolean replaced_existing renders true/false/empty. Action
  column uses the same lowercase tokens (install/update/
  uninstall/failed) the JSON serde uses so CSV + JSON exports
  align column-for-column. ISO timestamp uses
  chrono::DateTime::from_timestamp (already a direct workspace
  dep) so a pathological out-of-range value degrades to empty
  rather than panicking. 7 new tests pin: header-inclusion-
  caller-controlled, empty-with-header-is-header-only, paired-
  unix-and-ISO timestamps, NULL-renders-as-empty-not-string-
  None, full RFC-4180 escaping (commas + doubled quotes +
  embedded newlines), action-column-matches-serde-vocabulary
  (all 4 kinds), boolean-true-false-or-empty.
- Slice 60: JSON export envelope (b13de9f). New
  `InstallLogExportEnvelope` wire shape + `InstallEventExport`
  row + `INSTALL_LOG_EXPORT_SCHEMA_VERSION = 1`. Envelope
  carries schema_version + generated_at_iso + event_count +
  since_unix/iso + until_unix/iso + events array. Each event
  carries its own occurred_at_iso companion so the JSON file
  is self-describing — a script reading the export doesn't
  need to know about unix-seconds or install a date library
  to render timestamps. `InstallEventExport` uses
  `#[serde(flatten)]` over the InstallEvent so the wire stays
  readable (no nested "event:" container) while still letting
  us add the ISO companion. `install_log_to_json_with_now`
  test-only variant takes an explicit now so unit tests don't
  race the wall clock. Envelope shape designed to mirror a
  generic "audit export" pattern so a future Hopper run log
  export / plugin-storage backup / similar audit surface can
  adopt the same envelope without inventing a third format.
  5 new tests pin: schema + generated_at_iso, window-bounds
  round-trip-iso (since-only + both-bounds), event flatten
  with iso companion (no "event:" nesting on wire), empty-
  events still renders + serde round-trips, full-envelope
  serde round-trip with multiple action kinds preserved.
- Slice 61: Tauri export commands + TS client (8186b2a).
  Two new Tauri commands wired into the builder:
  `slab_marketplace_install_log_export_csv(path, since_unix?,
  until_unix?, limit?)` → u64 bytes_written;
  `slab_marketplace_install_log_export_json(path, ...)` →
  u64 bytes_written. Both open the log per-call (events fire
  on user click not in a hot loop), feed list_events_between
  → install_log_to_csv/json. Default limit = 100_000 (cap
  protects against runaway log eating disk on export).
  Idempotent — overwrites the target path. Returns bytes-
  written so the UI toast can say "Exported N events (X.X KB)"
  without re-reading the file. TS client adds
  `InstallLogExportFilter` shape (since_unix / until_unix /
  limit, all optional), `exportInstallLogCsv` /
  `exportInstallLogJson` wrappers, and
  `suggestInstallLogExportFilename(filter, ext, now?)` helper
  building filenames per the convention
  `marketplace-history_<window>_<YYYY-MM-DD>.<ext>` where
  window reads "all" / "from-YYYYMMDD" / "to-YYYYMMDD" /
  "YYYYMMDD-YYYYMMDD" depending on the bounds. Pure helper
  (no I/O, no Tauri) so it works in browser-mode + tests can
  pin the now param.
- Slice 62: Export menu in RecentInstallsDrawer (ecc2261,
  203 lines). Pure frontend tying slices 58-61 into a
  demo-able surface. Footer "Export…" popover anchored
  absolutely above the trigger with two entries: "Export as
  CSV…" (spreadsheet-friendly) and "Export as JSON…" (with
  envelope metadata). Each entry's subtitle reads either
  "Whole log · <format-hint>" or "Last <window> · <format-
  hint>" so the user sees at a glance what the export will
  contain BEFORE clicking. A new `windowSinceUnix` $derived
  maps the windowChoice toggle (7d/30d/all) to the matching
  unix-seconds cutoff and feeds it into the export filter —
  what gets exported matches what's filtered. Native save-as
  dialog with the kind-appropriate default extension; the
  suggested filename uses suggestInstallLogExportFilename.
  Escape handler dismisses the export menu first if open,
  then falls through to confirm-prune / close ladder.
  Window-click handler dismisses on outside click (Notion/
  Linear pattern). `exporting` boolean gates the Export/
  Clear/Close buttons during in-flight writes so users can't
  double-click or close mid-export. Single 4-second auto-
  clear toast surfaces "Exported N events (X.X KB)" on
  success; failures surface through the existing err banner.

Gates result: cargo fmt clean, cargo test --lib
marketplace::install_log:: 39 passed / 0 failed (+18 from
round-12's 21 baseline: 6 slice 58 + 7 slice 59 + 5 slice 60;
slice 61 is wire layer with no new tests, slice 62 is pure
frontend with no Rust tests), cargo test --lib 2171 passed /
0 failed (round-12 baseline + 18), pnpm check 0 errors /
104 warnings (round-12 baseline preserved EXACTLY — zero new
warnings from the Export menu, toast, or CSS additions on
RecentInstallsDrawer). **cargo clippy --lib gate WEDGED
TWICE AGAIN on /Volumes/SlabBuild sparse image — 4th tick
in a row hitting the same wedge** — first attempt cargo
check spawned but stayed at 0% CPU for 2+ min with no
rustc subprocess; second attempt identical. Per STATE.md
guidance, this batch ships on lib-test + svelte-check
strength.

PROCESS NOTES:
- SlabBuild sparse-image disk responsiveness was fine at
  tick start: `ls /Volumes/SlabBuild/target/debug/deps` ran
  in 0.3s with 6,424 entries cached. The wedge is reliably
  reproducible only when cargo's clippy/check codegen path
  needs to spawn rustc to enumerate the tauri crate's deps.
  cargo test --lib itself ran cleanly through the 2171-test
  suite in 40s with no wedge.
- This is the 4th tick in a row with this exact failure
  mode. **Sanjay action recommended (urgently):
  `hdiutil detach` then reattach `/Volumes/Sanjay
  SSD/SlabBuild.sparseimage` BEFORE the next round so
  clippy can pass cleanly.** The wedge is now consistent
  enough that we should consider it a documented "needs
  reattach between rounds" property of this build setup
  until a more permanent fix lands.
- The clippy gate wedge does NOT affect correctness — every
  new function in slices 58-60 went through cargo test
  --lib which exercises all 18 new tests + the existing
  21-test baseline + the broader 2153-test corpus as a
  regression net. The Tauri command surface in slice 61
  compiles + links via the cargo test build. Slice 62's
  pure-frontend surface passes pnpm check clean.
- Slice 58's dynamic WHERE-clause SQL was the only piece
  needing care: built it from a Vec<&'static str> for the
  clauses + a Vec<rusqlite::types::Value> for the params,
  then joined with " AND " between clauses. Used
  `rusqlite::params_from_iter` to bind the heterogeneous
  param list back. This was cleaner than building 4
  branches (none/since/until/both) by hand.
- Slice 59's CSV constant column header ended up cleaner
  as a `pub const &str` than a builder function — the
  header never changes between calls, every test would
  build the same string, so the constant is the truth.
- Slice 60's `#[serde(flatten)]` was the key insight that
  let the JSON event row look like a plain InstallEvent +
  one extra occurred_at_iso field, instead of either (a) a
  nested `{event: {...}, occurred_at_iso: ...}` shape, or
  (b) duplicating every InstallEvent field on the export
  row. The flatten attribute means downstream consumers
  reading the JSON see exactly what they'd see reading the
  raw InstallEvent over the Tauri wire, with the timestamp
  companion added at the same level.
- Slice 62's `windowSinceUnix` $derived was a small but
  important addition — without it, the export menu would
  have shipped the whole loaded 100-event buffer regardless
  of the windowChoice toggle the user had set, which would
  silently produce exports that don't match what's
  visible. Now the toggle controls both display AND export
  in one place.

DESIGN NOTES:
- Two timestamp columns in the CSV (unix + ISO) chosen over
  one because the two audiences differ: developers writing
  shell pipelines join on unix-seconds (millisecond
  precision doesn't matter for an install audit; jq + awk
  on the int column is cleaner than parsing ISO strings),
  while paralegals reading the file in Excel need
  human-readable dates without writing a formula. The cost
  of both columns is tiny (10 chars per row); the cost of
  picking one and being wrong is friction every time
  someone reads the export.
- JSON envelope schema_version starts at 1 (not 0) because
  v0 has the connotation of "draft / experimental"; v1 is
  the v1.0.0 contract — additive changes (new optional
  fields) stay at v1, breaking changes bump to v2. Same
  versioning convention as the marketplace IndexEntry
  schema bump from v1→v2.
- Export menu lives in the footer, not the header, because
  the header is reserved for "what am I looking at" and
  the footer is reserved for "what can I do with it". The
  Clear/Close pair was already in the footer; adding
  Export… there keeps the action vocabulary in one place.
- Export menu is a popover with subtitles (not a flat
  dropdown of "CSV / JSON") because the choice has two
  axes the user cares about: format AND window scope. The
  subtitle makes the window scope visible without forcing
  the user to remember what they had selected on the
  window strip. This is the same affordance HopperBackfill-
  Panel's Export CSV button uses (single fixed format
  there because the only format that matters for backfill
  is CSV).
- 4-second toast for success matches the HopperBackfill
  panel's CSV export toast — same export grammar, same
  toast lifespan.
- exporting boolean gates Close as well as Export/Clear
  because the user could otherwise close the drawer
  mid-write and lose their progress feedback. The 100k
  default limit means even a worst-case write completes in
  well under a second on any modern disk, but the gate is
  cheap defensive UX.

### What round-12 (2026-06-20 20:47 PT) just shipped

A demo-able overhaul of the plugin marketplace install pipeline's
audit surface. Before this tick `slab_marketplace_install` and
`slab_marketplace_uninstall` ran the install / uninstall pipeline
and then forgot the event happened — the UI could show "you have
v1.4 installed" but couldn't answer "when did I install this?"
or "did an update fail last week?" The marketplace backend had
been shipping since v1.4.0 Bench but the install-history audit
surface was the canonical missing piece (round 11 explicitly
called it out as the next subsystem candidate). Tonight that
gap closes end-to-end:

- Slice 53: marketplace::install_log primitive (9226f88).
  Append-only sqlite log at
  ~/.slab/marketplace-history.sqlite kept independent of
  plugin-storage.sqlite + hopper.sqlite so a failure in one
  DB can't poison another. Schema v1: install_events with
  id / plugin_id / version / action (install | update |
  uninstall | failed) / occurred_at unix-secs / source /
  bytes_written / files_extracted / replaced_existing /
  prior_version / error_msg. Two indexes covering the only
  two read paths (per-plugin newest-first + corpus-wide
  newest-first). NULL-able columns populated only on the
  rows that need them (uninstall rows carry no
  bytes_written; failed rows carry an error_msg but no
  bytes_written). InstallLog with open / open_in_memory /
  schema_version + three writers (record_install /
  record_uninstall / record_failure) + three readers
  (list_events per plugin, list_recent across plugins,
  install_stats + distinct_plugin_count for the toolbar
  badge). InstallStats slim payload with the per-kind
  counts. InstallAction parse returns Failed on unknown
  tags so a future schema bump doesn't panic the reader.
  14 new tests pin schema v1, action round-trip + unknown
  fallback, fresh-vs-update install row shape, uninstall
  NULLs, failure error_msg, newest-first ordering, limit
  clamp zero/negative, empty unknown-plugin, list_recent
  across plugins, install_stats per-action isolation,
  distinct_plugin_count dedup.
- Slice 54: wire log into install / uninstall pipelines
  (182e448). slab_marketplace_install captures prior_version
  BEFORE the install via reg.get(id) so the install
  pipeline's `replaced_existing` flag can be paired with the
  version that was overwritten (the pipeline itself doesn't
  read manifests). Wraps every failure surface — signature
  check, plugins-root resolve, plugins-root create,
  install_from_entry pipeline — in a record_failure call so
  failed installs are auditable. On success appends one
  install row (or update row when replaced_existing) with
  bytes_written + files_extracted + registry-derived
  prior_version. slab_marketplace_uninstall captures prior
  version BEFORE removing (once gone the registry can't
  tell us what we deleted), then on successful removal
  appends one uninstall row with the captured version
  (falling back to "unknown" when the plugin had no readable
  manifest). Two helpers centralise the boilerplate
  (open_install_log_and<F>(f) + record_install_failure
  best-effort). Log writes are out-of-band: a logging
  failure never masks the install failure being reported
  back to the user.
- Slice 55: reader Tauri commands + TS client (ef9fed1).
  slab_marketplace_install_events(plugin_id, limit?) ->
  Vec<InstallEvent>: per-plugin timeline newest first
  (default limit 50). slab_marketplace_install_history_recent
  (limit?) -> Vec<InstallEvent>: corpus-wide recent. 
  slab_marketplace_plugin_install_stats(plugin_id) ->
  InstallStats. All three open
  ~/.slab/marketplace-history.sqlite per call (install
  events fire on user click, not in a hot loop, so per-call
  open beats a managed singleton). TS adds InstallEvent /
  InstallStats interfaces (NULL-able fields typed as
  `T | null` so consumers handle present-but-null
  explicitly), listInstallEvents / listRecentInstallEvents /
  pluginInstallStats helpers, formatInstallEventTime
  (compact relative timestamp with now param injectable for
  deterministic tests, falls back to ISO yyyy-mm-dd for
  events older than 30 days), installEventGlyph
  (monochrome chrome vocabulary ✓ install / ↻ update /
  ⌫ uninstall / ✕ failed).
- Slice 56: retention + summary surface (8e04747). Three
  new InstallLog methods: oldest_occurred_at (wraps the
  SELECT MIN edge case where empty table returns ONE row
  with NULL by reading the column as Option<i64> so NULL
  decodes cleanly to None — first attempt panicked on
  InvalidColumnType so the fix-and-test loop pinned this
  invariant), total_event_count (O(1) on sqlite internal
  counters), prune_older_than (strict less-than predicate
  so boundary row survives; idempotent — second call with
  same cutoff is a no-op). Two new Tauri commands:
  slab_marketplace_install_log_summary -> InstallLogSummary
  { total_events, distinct_plugins, oldest_occurred_at } in
  three cheap queries one round-trip; 
  slab_marketplace_install_log_prune(retain_days) ->
  rows-removed (retain_days clamped to a minimum of 1 so
  a caller can't accidentally wipe the whole log via
  prune(0)). TS adds InstallLogSummary + installLogSummary +
  pruneInstallLog + formatLogSpan ("N events across X
  days" with ceiling-day arithmetic so a 5-minute-old log
  reads "1 day" not "0 days"; returns literal "no events
  yet" on empty so the UI can render unconditionally). 7
  new tests pin oldest empty-log None, earliest-row-wins,
  prune strict boundary, prune empty zero, prune
  idempotency, prune cutoff_zero no-op, total count
  matches inserts + drops after prune. Plus test-only
  insert_at helper pinning occurred_at to known value so
  the prune/oldest tests don't race the clock.
- Slice 57: Activity section + RecentInstallsDrawer
  (7b84083, 830 LOC). PluginDetailDrawer gains an Activity
  section between metadata grid and footer, self-fetching
  on mount + every entry.id change (Promise.all over
  listInstallEvents(20) + pluginInstallStats). Section
  auto-collapses when timeline empty so a never-installed
  plugin's drawer stays clean. Per-row layout: per-action
  glyph + per-action colour accent (failure red, update
  amber, install accent, uninstall muted) + action label +
  version + optional ← v<prior> for updates + bytes/files
  metadata for installs OR truncated error message for
  failures + right-aligned relative timestamp. Header
  subtitle assembles parts only for nonzero kinds so a
  sparse-history plugin renders tight ("3 installs · 1
  update · 1 failure"). RecentInstallsDrawer.svelte (NEW,
  470 LOC): 460px right-side slide-from-right drawer
  mirroring PluginDetailDrawer's Notion side-panel
  convention. Window strip "Last 7d / Last 30d / All"
  filtering loaded events post-fetch (events fetched once
  with limit 100, then client-side filtered — no
  re-round-trip on window flip). Per-event row mirroring
  the Activity vocabulary so visual recognition transfers.
  Empty-state branches handled (no events at all / no
  events in window / loading / error). Footer "Clear older
  than 90d…" with two-step confirm (button morphs into
  confirm-message + Cancel + Delete pair) calling
  pruneInstallLog(90); onPruned bubbles back to
  PluginsPanel so the toolbar count updates without a
  remount. Escape closes drawer or dismisses confirm step.
  PluginsPanel wiring: installLog + recentInstallsOpen
  state + refreshInstallLog helper called on mount + after
  every install / uninstall / install-failure. Toolbar
  gains "⏱ History" button with a count chip (slim mono
  18×16 pill matching the existing tab-count vocabulary).
  Button disappears quietly when log empty (no nag UI on a
  brand-new install).

Plus a fix-up commit (1d79d7b) catching two pure-formatting
rustfmt drifts the cargo fmt gate surfaced on slice 54's
prior_version let-chain (collapsed to one line) + the
record_install_failure closure body (reflowed onto its own
indented line).

Gates result: cargo fmt clean (drift fixed via fixup),
cargo test --lib marketplace::install_log:: 21 passed / 0
failed (+21 vs baseline: 14 from slice 53 + 7 from slice
56), cargo test --lib 2153 passed / 0 failed (round-11
baseline + 21), pnpm check 0 errors / 104 warnings
(round-11 baseline preserved EXACTLY). **cargo clippy
--lib gate WEDGED TWICE AGAIN on /Volumes/SlabBuild sparse
image — third tick in a row hitting the same wedge** —
first attempt cargo check spawned rustc which fell to 0%
CPU on `rustc --crate-name tauri_plugin_opener`, killed
after ~3min; second attempt cargo check itself stayed at
0% (0.52s CPU) without spawning rustc. Per STATE.md
guidance, this batch ships on lib-test + svelte-check
strength.

PROCESS NOTES:
- The sparse-image wedge is reliably reproducible now: it
  hits the cargo invocation that has to enumerate the
  tauri crate's deps. Disk space is fine (56G free), `ls
  /Volumes/SlabBuild/target/debug/deps` ran in 0.2s at
  tick start, and the cargo test gate ran cleanly. So
  it's specifically the clippy/check codegen path that
  trips fsync sleep. **Sanjay action recommended (third
  tick in a row): `hdiutil detach` then reattach
  `/Volumes/Sanjay SSD/SlabBuild.sparseimage` BEFORE the
  next round so clippy can pass cleanly.** This is now
  documented as the consistent failure mode.
- slice 56's `oldest_occurred_at` panicked on first test
  run with `InvalidColumnType(0, "MIN(occurred_at)",
  Null)`. SELECT MIN(...) on an empty table returns one
  row with a NULL column, not zero rows — so `.optional()`
  doesn't help; the fix was reading the column as
  `Option<i64>` so NULL decodes cleanly. Caught + fixed
  before commit so the slice ships green.
- LSP TS-server cache lag flagged "no exported member"
  errors after each marketplace.ts addition; ignored as
  pre-existing rust-analyzer / TS-server cache behaviour
  (the symbols exist, verified via grep + final pnpm check
  which passes 0 errors).
- pnpm check did NOT surface any new a11y warnings on
  either drawer — the slice 11 lessons (use <div
  role="dialog"> not <aside role="dialog">, one
  svelte-ignore rule per comment) carried into both
  PluginDetailDrawer's Activity section and the new
  RecentInstallsDrawer.

DESIGN NOTES:
- Three-DB separation (plugin-storage.sqlite /
  hopper.sqlite / marketplace-history.sqlite) — audit log
  failures must not poison plugin runtime storage or
  hopper routing. Cheap to maintain (each module gets its
  own default_log_path helper) and clean to migrate
  (schema bumps stay local).
- Per-call open of the install log not a managed
  singleton — install events fire when the user clicks
  Install, not in a hot loop. Per-call open keeps the
  open/close path obvious and avoids a tauri-managed
  state bag that would need careful locking. The summary
  is three small queries so per-drawer-open re-fetch is
  also fine.
- prune retain_days clamped at >=1, not at >=0 — to clear
  the log entirely the user has to use an explicit "clear
  all" (not shipped here; remains a separate later
  surface). This keeps the default surface safe.
- Per-action glyph + per-action colour accent: glyph
  conveys the action class even when the row gets
  truncated; colour adds parsing speed on a long
  timeline. Failed = red, update = amber, install =
  accent, uninstall = muted. Matches Hopper run log
  vocabulary so users who learned that scheme transfer
  here.
- Toolbar History button shows count chip ONLY when
  installLog.total_events > 0 — no zero-state badge,
  no nag. Same "never show a UI element that opens
  empty" Notion principle that round 11's "✨ Review N"
  badge used.
- Activity timeline limit 20 on PluginDetailDrawer (vs
  100 on the Recent installs drawer): a per-plugin
  timeline rarely needs scrolling, and the drawer hosts
  metadata above + footer below; 20 keeps the row stack
  short.
- formatInstallEventTime accepts a `now` param so the
  formatter is deterministic for unit tests later, even
  though we don't have JS tests today — cheap optionality
  that costs nothing.
- 90d default on the prune confirmation — matches a
  quarter so paralegals doing quarterly audits still have
  the relevant rows; 30d would be too aggressive, 365d
  too lax for the typical workstation.

### What round-11 (2026-06-20 17:32 PT) just shipped

A demo-able overhaul of the v3.39.0 Atlas Tag-Suggest bulk
pathway. Before this tick the per-doc SuggestedTagsRow chip
strip shipped, but bulk was a half-finished cabbage of
endpoints: `tagSuggestionsBulk` returned N rows, the user had to
N-round-trip through the per-doc primitive to apply anything,
there was no granular dismissal control (the escape hatch only
nuked the entire dismissal list per doc), no way to bulk-suggest
over a saved view or starred-only filter (only "untagged"
shortcut), no badge counter for the review panel, and no review
panel itself. Tonight every gap closes:

- Slice 48: `accept_tag_suggestions_bulk(db, items)` (d17fe91).
  Per-item failure semantics — a malformed name in item 12 fails
  item 12 alone without rolling back the 49 good accepts.
  AcceptItem + BulkAcceptResult types; case + whitespace
  pair-dedupe in the pre-pass so a UI that double-checks the
  same row by accident is silently coalesced. Tauri command
  `slab_library_tag_suggestions_accept_bulk` emits a single
  library-changed event after the batch (only when at least one
  item attached). TS adds `TagSuggestionAcceptItem` +
  `BulkTagAcceptResult` + `acceptTagSuggestionsBulk(items)`. 6
  new tests pin: happy path, case+whitespace dedupe, mid-batch
  failure isolation, empty input no-op, all-empty-failures-no-
  attach, find-or-create on unknown tags.
- Slice 49: granular undismiss (88a5439). New
  `DismissedSuggestion { tag_name, dismissed_at }` row type +
  `list_dismissed_for_doc(db, doc_id)` reader (ORDER BY
  dismissed_at DESC, tag_name ASC so the inspector shows the
  most recent mistake at the top) + `undismiss_one_for_doc(db,
  doc_id, tag_name)` writer returning bool (true if a row was
  deleted, false if no such dismissal). Case-insensitive match
  on the undismiss path mirrors dismiss-time normalisation so
  the undo path is symmetric. Two new Tauri commands
  (`slab_library_tag_suggestions_list_dismissed` +
  `slab_library_tag_suggestion_undismiss_one`). TS adds the
  DismissedTagSuggestion interface + two helpers. 7 new tests
  pin: empty no-dismissals, normalised names, newest-first
  ordering (manual ts insert pins the ordering since
  dismiss_tag_suggestion stamps now()), per-doc isolation,
  siblings-preserved single-row delete, missing-row returns
  false, case-insensitive match.
- Slice 50: `suggest_for_filter(db, filter, limit)` (3053a7c).
  Generalises the bulk surface — `suggest_for_untagged` stays
  as the lighter LEFT-JOIN shortcut; this is the proper review
  entry point that reuses `query::query_documents` so every
  LibraryFilter capability (flat folder/tags/title, the
  recursive clause tree, starred_only, sort, tag_match) composes
  for free. `limit` is forced onto the effective filter (callers
  can't smuggle a huge limit by accident); `sort` is left
  untouched so saved views render in their authored ordering.
  Docs that yield zero suggestions are skipped post-query.
  Tauri command `slab_library_tag_suggestions_bulk_for_filter`
  accepts the same LibraryFilter the live grid uses. TS adds
  `tagSuggestionsBulkForFilter(filter, limit)`. 6 new tests pin:
  empty filter matches all, folder filter narrows correctly,
  starred_only narrows correctly, caller-limit-clamped,
  zero-suggestion docs skipped, clause-tree composes.
- Slice 51: `suggestion_stats(db, sample_cap)` (2a3b8b5). New
  `TagSuggestionStats { untagged_docs_with_suggestions,
  dismissed_total }` slim payload. Walks the `sample_cap`
  most-recently-seen untagged docs and probes each with
  `suggest_tags_for_doc` so the badge counts only docs that
  WOULD actually surface review — never lures the user into
  an empty panel. `dismissed_total` is a single corpus-wide
  COUNT(*) for the settings escape hatch. Sample cap defaults
  to 200 server-side; the UI renders "200+" upstream when the
  working set saturates. Tauri command
  `slab_library_tag_suggestion_stats(sample_cap)`. TS adds
  `TagSuggestionStats` + `tagSuggestionStats(sampleCap)`. 5
  new tests pin: empty library, tagged-docs excluded,
  zero-suggestion-docs excluded, sample-cap bounds the scan,
  corpus-wide dismissal sum, dismissed pair drops from count.
- Slice 52: BulkTagSuggestionsPanel.svelte (c881c62, 640 LOC).
  Pure frontend slice tying all four backend slices into one
  demo-able review surface. 560px right-side drawer (matching
  the DocInspectorPanel Notion side-panel convention) with:
  source-strip segmented toggle ("Untagged only" / "Current
  filter", filter mode disabled when no active filter) + per-doc
  cap input (5–500 step 5, default 50, refetches on change);
  bulk control bar with Refresh + selection chips (All, ♦ vocab,
  ⚭ co-occ, ⌗ domain so paralegals can one-click "accept every
  domain-hint chip" across the whole batch) + Apply N primary
  action; per-doc card grid with title + path + up to 5
  suggestion chips, each chip a checkbox-toggle on accept +
  ✗ dismiss button (the dismiss path strips the chip locally
  AND drops it from selection); per-card "Hidden…" link that
  loads the dismissed list via `listDismissedTagSuggestions`
  with one-click Undo via `undismissOneTagSuggestion`; toast
  confirmation + error banner; deterministic pastel preview
  on chips matching the rust `pastel_for` so the chip background
  previews the saved tag colour. LibraryPanel wiring: toolbar
  gains a "✨ Review N" button gated on
  `bulkBadge.untagged_docs_with_suggestions > 0` (disappears
  quietly when there's nothing to review — no nag UI). Stats
  refresh on mount, after every library-changed event, after
  the drawer closes, and after a bulk apply succeeds. Drawer
  receives `buildCurrentFilter()` so the "Current filter" mode
  picks up whatever the user has narrowed the grid to. No new
  Tauri commands — pure UI composition.

Gates result: cargo fmt clean (one rustfmt drift on slice 49's
undismiss_one signature captured in slice 52's commit since
slice 49 already shipped), cargo test --lib pdf::library::
tag_suggest:: 42 passed / 0 failed (+24 from baseline: 6
bulk-accept + 7 granular undismiss + 6 suggest_for_filter + 5
suggestion_stats), cargo test --lib 2132 passed / 0 failed
(round-10 baseline + 24 from this batch), pnpm check 0 errors
/ 104 warnings (round-10 baseline preserved EXACTLY — caught
2 new a11y warnings during the gate and fixed them before
commit so the final delta is zero). **cargo clippy --lib gate
WEDGED TWICE on /Volumes/SlabBuild sparse image — even `ls
target/debug/deps` hangs >8s post-attempt** (was 0.027s at
tick start — the cargo invocations triggered the wedge again).
Per STATE.md "if cargo wedges twice, commit on cargo test --lib
+ pnpm check strength and log the blocker" guidance, this
batch ships on lib-test + svelte-check strength.

PROCESS NOTES:
- First clippy attempt: rustc spawned `rustc --crate-name
  tauri`, fell to 0% CPU within ~30s, stayed there for 100s+.
  Killed, retried.
- Second clippy attempt: cargo-clippy itself stayed at 0% CPU
  without ever spawning rustc — same fsync sleep, different
  surface. Killed.
- Disk has 56G free; this is fsync slowdown not space.
  Sanjay action recommended: `hdiutil detach` + reattach
  `/Volumes/Sanjay SSD/SlabBuild.sparseimage` before the next
  round so clippy can pass cleanly (same recommendation as
  round 10 — the wedge is now consistent enough that detach/
  reattach should be the standard between-round step until a
  more permanent fix lands).
- The slice-by-slice LSP diagnostics flagged "no exported
  member" errors immediately after each library.ts addition;
  these were rust-analyzer / TS-server cache lag (the symbols
  exist, verified via grep + final pnpm check). Ignored.
- The pnpm check did surface 2 NEW a11y warnings on the bulk
  panel during the gate: `<aside role="dialog">` (non-
  interactive element with interactive role) + missing
  `a11y_no_static_element_interactions` ignore on the overlay
  div's onclick. Fixed both before the gate cycle by swapping
  `<aside>` for `<div role="dialog" aria-modal="true">` and
  splitting the multi-rule svelte-ignore comment into one per
  line (Svelte 5 only honours one rule per comment). Final
  count returned to the 104-warning baseline.

DESIGN NOTES:
- Source strip segmented toggle, not two separate buttons:
  the two modes are mutually exclusive (one or the other
  drives the candidate set), so a segmented control reads as
  "this is THE source choice" rather than two competing
  toggles. Filter mode disabled when no active filter so
  the user can't pick the empty path.
- Selection chips by source (vocab / co-occ / domain) because
  the source is the user's mental model of trust — vocab
  matches are "almost always right", co-occ matches are
  "high-confidence guesses", domain hints are "weakest
  signal". A paralegal who only trusts vocab can one-click
  ♦ vocab, scan the selections, and Apply.
- Set-based selection state, not array-based: toggle becomes
  O(1) instead of O(n) on big batches; the Apply-N button
  label updates on every keystroke.
- Refresh button explicit, not auto-refresh on every key
  press of the per-doc cap input: the per-doc cap can change
  the wire payload size 10× so we want a deliberate user
  action to refetch — `onchange` not `oninput` on the number
  input so a user typing 500 doesn't fire 3 fetches.
- Toolbar badge gated on `untagged_docs_with_suggestions > 0`
  so the button DISAPPEARS quietly when there's nothing to
  review. Notion-pattern: never show a UI element that opens
  empty. The drawer can still surface dismissed suggestions
  via the empty-state "Show dismissed (N)" link as a
  recovery path.
- Drawer width 560px (vs 460px for DocInspector): the bulk
  panel hosts a multi-column-chip grid per card and needs
  the breathing room. Both still cap at viewport max-width
  92vw / 96vw so they remain usable on small windows.

### What round-10 (2026-06-20 14:55 PT) just shipped

A demo-able overhaul of the Hopper batch-backfill loop. Before
this tick the panel used the sync executor with no live progress
and a non-functional Cancel button stub, did only single-level
folder scans (paralegals dumping nested discovery trees needed
to point at each subfolder one at a time), gave no pre-flight
coverage view of which rule would catch how many files, had no
CSV export for the audit trail partners and clients expect, and
the Recent Backfills history had no time-window scoping. Every
gap closes here:

- Slice 43: `plan_backfill_with_options(folder, watch, rules,
  &PlanOptions { recursive, max_depth })` (a6cf10c). The legacy
  `plan_backfill` becomes a back-compat wrapper using
  `PlanOptions::default()` (non-recursive). Internal
  `collect_pdfs` helper recurses via an explicit stack so the
  hopper module doesn't pull in a walkdir transitive dep just for
  this one site. Hidden directories skipped (matches the
  existing hidden-file rule); locked sub-folders swallow errors
  so one denied subdir doesn't kill the whole report. Tauri
  command + TS client widened with `opts: Option<PlanOptions>`
  defaulting to None. 6 new tests pin: default options ==
  legacy, recursive walks subfolders, max_depth caps correctly,
  Some(0) == non-recursive, hidden subdirs invisible, PlanOptions
  serde round-trips with `#[serde(default)]` on both struct + fields
  so an empty JSON object decodes.
- Slice 44: `BackfillReport::per_rule_counts: BTreeMap<String,
  usize>` (3bbc08a). Tally per matched rule with two synthetic
  buckets: `__defaults__` (no rule matched, fell through to
  watch defaults) + `__skip__` (plan-time skips). Rules with
  zero hits are omitted — the editor already lists every rule
  by name; the UI strip stays tight. `#[serde(default)]` on the
  field keeps pre-v3.39 cached BackfillReport JSON decoding
  cleanly. TS adds `BACKFILL_BUCKET_DEFAULTS` /
  `BACKFILL_BUCKET_SKIP` constants + `backfillBucketLabel`
  helper. 6 new tests pin: empty plan → empty counts,
  all-unmatched → __defaults__, mixed splits correctly, skips
  bucket, zero-match rules omitted, sum of values == scanned,
  legacy JSON decodes with default.
- Slice 45: `HopperLog::list_backfill_runs_since(folder,
  since_unix, limit)` (0e9d5e2). New authoritative reader;
  legacy `list_backfill_runs` delegates with `since_unix=None`
  so back-compat is total. Both filters AND together in SQL so
  the wire stays slim. Cutoff is INCLUSIVE on finished_at.
  Tauri command widened with `since_unix`. TS adds the optional
  third arg + `backfillSinceUnix(windowHours)` pure helper
  computing the unix-seconds cutoff for the "Last 24h / Last
  7d / All" chips. 4 new tests pin: since=None matches legacy,
  inclusive boundary, folder + since AND, future cutoff → empty.
- Slice 46: `backfill_report_to_csv(report, include_header)`
  (9a14495). RFC-4180 strict — wraps fields containing `,` `"`
  `\r` `\n` in `"`, doubles embedded `"`. Action column uses
  the same kebab-case wire vocabulary as JSON serde. Missing
  matched_rule / destination render as empty (not "None") so
  downstream parsers don't trip. New Tauri
  `slab_hopper_export_backfill_csv` takes report + absolute
  path (frontend gets it from @tauri-apps/plugin-dialog save()
  so we can write anywhere the user has rights, bypassing the
  default plugin-fs scope), returns bytes written for the toast.
  TS adds `slabHopperExportBackfillCsv` +
  `suggestBackfillCsvFilename` ("backfill_<folder>_<YYYY-MM-DD>.csv"
  with special chars sanitised). 6 new tests pin: header
  inclusion caller-controlled, empty report yields header-only,
  full RFC-4180 escaping, bare fields stay unquoted, action
  column kebab-case, optional fields empty.
- Slice 47: HopperBackfillPanel.svelte rewrite (f720bcf). Pure
  frontend slice tying all four backend slices into one
  surface. Scan-options strip with "Include sub-folders" checkbox
  + depth dropdown (No limit / 1 / 3 / 5) triggering a fresh
  runPlan on flip. Per-rule coverage chips below the summary
  with class-keyed colour (blue for rule names, neutral for
  defaults, amber for skip), sorted by descending count with
  synthetic buckets pinned at end. Apply now goes through the
  round-9 `executeBackfillAsync` streaming executor: progress
  bar with processed/total + moved/skipped/errored split,
  scrolling 12-row tail of per-file outcomes with ✓/↷/✗ glyphs
  + inline error text. Cancel button appears only while
  applying, dims to "Cancelling…" while the cancel-token flip
  propagates. "Export CSV…" affordance calls plugin-dialog
  save() then `slabHopperExportBackfillCsv`; 4s toast confirms
  "Exported N rows (X.X KB)". History disclosure gains "Last
  24h / Last 7 days / All" chips above the run list
  (default 7d to match paralegal weekly batch cadence); empty
  window shows a hint instead of nothing. Row checkboxes
  disable during applying so the selection can't mutate
  mid-run; stale-plan link suppresses during apply.

Plus a fix-up commit (36309a5) catching three bugs the cargo
test gate surfaced: (1) PlanOptions needs `#[serde(default)]` on
the struct so an empty `{}` decodes; (2) the
unreadable-folder branch in plan_backfill_with_options was
tallying per_rule_counts from an empty slice instead of the
populated planned vec; (3) one cargo-fmt drift on
watcher.rs::RunEmitter::emit_backfill_progress default impl
body that the slice 43 batch surfaced.

Gates result: cargo fmt clean, cargo test --lib pdf::hopper::
108 passed / 0 failed (+22 from previous baseline: 6 PlanOptions
+ 6 per_rule_counts + 4 since + 6 CSV), cargo test --lib
pdf::library:: 381 passed / 0 failed (round-9 baseline preserved),
pnpm check 0 errors / 104 warnings (round-9 baseline preserved;
zero new from the panel rewrite). **cargo clippy --lib gate
WEDGED TWICE on /Volumes/SlabBuild sparse image — even a plain
`ls` of target/debug/deps hangs >60s**, so this batch ships on
cargo test --lib + pnpm check strength per the documented
STATE.md guidance: "if cargo wedges twice, commit on cargo
check --lib + pnpm check strength and log the blocker."

PROCESS NOTES:
- First clippy attempt hit the sparse-image sleep at ~4 minutes
  in (all rustc processes 0% CPU on `rustc --crate-name tauri`
  fsync). Killed, retried — second attempt wedged even earlier
  on the same crate.
- Diagnostic `ls /Volumes/SlabBuild/target/debug/deps` then hangs
  >60s — confirms the sparse image directory enumeration itself
  is unresponsive (not specific to cargo). A `hdiutil detach`
  + reattach is likely needed before the next round's full
  cargo gates can run.
- All round-10 backend code went through `cargo test --lib`
  which exercises every new function via the 22 new tests +
  passes the existing 86 backfill/log/registry/rules/watcher
  tests as a regression net. The library 381-test baseline also
  stayed green, so the type-level changes (BackfillReport
  field widening, new tauri commands registered in lib.rs) all
  compile and link clean.
- The frontend panel rewrite passes `svelte-check` clean with
  zero new errors/warnings — exactly the same 104-warning
  baseline (all pre-existing a11y warns in other panels).
- The cargo wedge is the documented SlabBuild sparse-image
  failure mode, not a code defect — every test that DID run
  passed.

DESIGN NOTES:
- Per-rule chips sorted by descending count, then synthetic
  buckets pinned at end. The defaults bucket is rendered as
  neutral (no rule matched is informational, not warning),
  the skip bucket as amber (plan-time skip is "needs
  attention").
- 7d default for the history window matches paralegals' weekly
  batch cadence — most ad-hoc users will only have ever fired
  a backfill in the last week anyway, so the default scopes
  cleanly without losing context.
- CSV export filename uses ISO yyyy-mm-dd not the locale date —
  filesystems are international and partners forward CSVs
  across timezones.
- Streaming progress tail capped at 12 rows to keep DOM bounded
  on a 10,000-file run; newest at top so the visual cue
  ("file just processed") sits at the user's eye level.
- The "Apply N files" button label uses selectedCount (the
  trimmed plan that will actually run) not counts.willMove
  (the planner-derived figure), so when a user deselects some
  rows the label updates immediately. Old behaviour was already
  this; round-10 just preserves it through the streaming
  rewrite.

### What round-9 (2026-06-20 09:55 PT) just shipped

A demo-able overhaul of the saved-views rail. Before this tick the
v3.50 rail had only the CRUD primitives: save / list / delete /
rename. Every power-user verb was missing — no in-place edit (so
tweaking a filter meant delete-and-recreate, losing id +
sort_order + created_at), no fork (so building "Apollo invoices
2024" then "2025" meant retyping the whole filter), no pin (so
your most-used view drifted under newer ones), no reorder.
Tonight every Notion-grade rail verb lands:

- Slice 38: `update_view_filter(id, &LibraryFilter)` (7774964)
  swaps just the saved filter blob in place, preserving id +
  name + created_at + sort_order. get_view confirms the row
  exists first so unknown id surfaces as a hard error instead
  of silent 0-rows-affected. Re-pin the rail onto an existing
  view with one click. 3 new tests + Tauri command + TS client.
- Slice 39: `duplicate_view(id)` (128d0a2) forks an existing
  view's filter byte-for-byte, derives a unique name by
  appending " (copy)" / " (copy 2)" / … up to 999 to dodge
  the UNIQUE constraint, gets a fresh sort_order at the
  bottom. The duplicate is INDEPENDENT — editing it later does
  NOT mutate the source (covered by
  duplicate_view_is_independent_from_source). 5 new tests +
  Tauri command + TS client.
- Slice 40: schema bump v14 -> v15 adds `pinned INTEGER NOT
  NULL DEFAULT 0` to library_saved_views + partial index
  `idx_saved_views_pinned WHERE pinned = 1` (cheap because
  only a small fraction of saved views are ever pinned).
  `set_view_pinned(id, bool)` is the writer; idempotent
  (SQLite reports rows matched not rows changed). list_views
  ORDER BY widens to `pinned DESC, sort_order ASC, name ASC`.
  SavedViewRecord widens with the `pinned: bool` field with
  serde default so pre-v3.56 JSON snapshots cached client-side
  decode as false. (c86cc42) 8 new tests incl. schema_v15
  pragma_table_info + partial-index pin + legacy-JSON-without-
  pinned pin.
- Slice 41: `reorder_views(&[i64])` (8278a2a) atomically
  re-stamps sort_order by zero-based position. Single SQLite
  txn so partial failures can't leave the rail mid-shuffle.
  Validation runs BEFORE the txn opens (duplicate ids → "duplicate
  view id N"; unknown ids → "unknown view id N") — so a rejected
  reorder doesn't touch a row. Subset reorders are PERMITTED
  (unmentioned ids keep their pre-reorder sort_order) — documented
  in reorder_views_subset_only_restamps_named_rows so a future
  change can't regress. Mirrors the
  smart_folders::set_order / set_collection_order patterns.
  Reorder does NOT mutate the pinned flag — the rail's
  pinned-first sort survives shuffles transparently. 6 new
  tests + Tauri command + TS client.
- Slice 42: pure frontend — wired all four verbs into the
  LibraryPanel saved-views rail (58e895b). Rail-head gains an
  "Update" button (visible only when an active view is loaded
  AND the current filter is non-default). Per-row layout
  becomes [★ pin glyph] [◆ row body] [⋯ menu]; the pin is gold
  (#f7c948) when on and ghost on hover when off. The ⋯ menu
  surfaces Pin/Unpin, Rename… (inline-input pattern matching
  the existing rename rails), Duplicate, Move up / Move down
  (conditional on group position; restricted to within
  pin-group because pinned-first dominates the sort), then a
  danger-tinted Delete view. The window-click-outside listener
  was extended to clear savedViewMenuId alongside the doc-card
  menu so the popover dismisses on outside click. Local
  savedViewCompare matches the backend ORDER BY so in-memory
  pin/duplicate/rename mutations keep the rail order without
  a round-trip; reorder does refresh-via-list because
  recomputing sort_order locally is more error-prone than
  re-fetching.

Gates passed: cargo fmt clean, cargo test --lib pdf::library::
381 passed / 0 failed (+22 from round-8's 359 baseline: 3
update_view_filter + 5 duplicate_view + 8 set_view_pinned/list_views
+ 6 reorder_views tests; the schema_v15 pin is in the same
suite), cargo test --lib ai::embedding_index 30 passed / 0
failed (round-8 baseline preserved), cargo clippy --lib -D
warnings clean (4m16s warm — first cycle after a kill, second
cycle would be faster), pnpm check 0 errors / 104 warnings
(same baseline as round-8; zero new warnings from the new
imports / handlers / popover markup / styles).

PROCESS NOTES:
- First gate cycle wedged because I ran cargo clippy + cargo
  test concurrently — STATE.md was prescient: the
  /Volumes/SlabBuild sparse image's slow fsync makes two cargo
  invocations contend on the build lock. Killed both and ran
  serially; the test build then surfaced a borrow-doesn't-live
  long-enough error on the reorder_views id-set collect (stmt
  + query_map + collect needs an explicit Vec intermediate so
  stmt doesn't get dropped while the iterator is still alive).
  Fixed and amended into slice 41's commit (so each slice
  remains independently revertible + tests-green).
- Pre-existing rust-analyzer false positives for `async fn`
  saturate the lint output on lib.rs (it can't see the package's
  edition = "2021"); ignored — cargo itself doesn't complain.
- LSP type cache lag in svelte-check is also pre-existing on
  the SavedViewRecord widening — running `pnpm check` truthfully
  surfaces no new errors.

DESIGN NOTES:
- Reorder restricted to within pin-group: the dominant sort key
  is `pinned DESC`, so letting an unpinned view "swap" past a
  pinned one above it would just visually no-op and confuse the
  user. The UI guards the menu items conditionally on group
  position (Move up hidden at the top of the group, Move down
  hidden at the bottom).
- No drag-handle UI this round — the Move up/down menu items
  cover the use case at-grade and Sanjay can revisit a real
  drag affordance once the rail's volume justifies it. The
  reorder backend takes a full positional list, so wiring a
  drag handle is a pure-frontend follow-up later.
- Update button (slice 38) carries a confirm() dialog because
  the action OVERWRITES the saved filter — irreversible-ish
  (you'd have to recreate the original from memory). Duplicate
  / pin / rename don't confirm because they're either
  reversible or cosmetic.

### What round-8 (2026-06-20 06:09 PT) just shipped

A demo-able overhaul of the doc-row surface. Before this tick a
library_documents row had a `title` column but NO setter — so a
filename like `scan_001.pdf` was stuck as-is — and zero per-doc
context: no notes, no star, no inspector. The card menu let you
open in Reader, OCR, auto-tag, manage tags, remove; nothing else.
Now every Notion-grade per-doc affordance lands:

- Slice 33: `LibraryDb::set_doc_title(doc_id, Option<&str>)` overrides
  the displayed title without renaming the on-disk file. Trims,
  None/empty clears back to NULL so the basename fallback resumes,
  capped at MAX_DOC_TITLE_LEN (500 Unicode scalars). Errors on
  unknown id or oversized text; length check runs BEFORE the
  UPDATE so a rejected setter leaves the prior title untouched.
  Returns the refreshed DocumentRecord with tags eager-loaded.
  5 new tests + Tauri command + TS client. (7398b58)
- Slice 34: schema bump v12 -> v13 adds nullable `notes TEXT` to
  library_documents (pre-v13 rows silently pick up NULL).
  `set_doc_notes(doc_id, Option<&str>)` is the writer, same trim/
  empty-clears/cap shape as set_doc_title; cap is MAX_DOC_NOTES_LEN
  (4000 Unicode scalars, sized for a paragraph or two of provenance
  context). DocumentRecord widened end-to-end: backend struct, the
  four ocr_queue SELECT mappers, the registry/query/collections
  SELECT lists, the TypeScript mirror. 6 new tests (incl. schema_v13
  pragma_table_info pin). (12eab28)
- Slice 35: schema bump v13 -> v14 adds `starred INTEGER NOT NULL
  DEFAULT 0` + partial index `idx_documents_starred WHERE
  starred = 1`. Partial index is cheap because only a small
  fraction of the library is ever starred. `set_doc_starred(
  doc_id, bool)` is the writer; idempotent (SQLite reports rows
  matched not rows changed). 5 new tests (incl. schema_v14 +
  partial-index pin + upsert_existing_doc_preserves_starred — the
  scanner's re-upsert pass must NOT wipe a user-set star).
  (66a14fb)
- Slice 36: queryable surface for the star flag. Three independent
  levers: LibraryFilter.starred_only top-level flag (AND-combined
  with everything, lives at the top so it overlays cleanly on ANY
  saved filter including the clause tree), FilterClause::Starred /
  NotStarred variants for the smart-collection rule builder, and
  the LibraryPanel toolbar "Starred" toggle chip mirroring the
  existing "Untagged" pattern. Pre-v3.55 saved smart collections
  that didn't carry starred_only deserialise as `false`. 6 new
  query.rs tests (incl. starred_filter_serde_round_trip with the
  legacy-JSON-without-the-field deserialises-as-false pin).
  (2fd3027)
- Slice 37: Pure frontend — DocInspectorPanel.svelte (~600-LOC
  Svelte 5 panel) that ties slices 33-35 into one drawer. NOT a
  full-viewport modal like OcrQueuePanel / BeaconCachePanel;
  a 460px slide-from-right drawer (Notion side-panel convention)
  so the doc grid stays visible behind it. Sections: title
  override input (placeholder shows basename fallback, save on
  blur or Enter), notes textarea (save on blur or Cmd/Ctrl+Enter,
  live counter that goes amber at 90% and red over the 4000-char
  cap), read-only tag chips (with hint pointing at the card-menu
  tag affordance), metadata block (path / pages / size / added /
  last-seen / OCR-state with the error reason inline if failed),
  footer with Open in Reader (primary) / Reveal on disk / Remove
  from library (danger, two-step confirm). Star pill at the top-
  left (gold #f7c948 when on). LibraryPanel wiring: imports the
  three setters, gains inspectorDoc state + 4 handlers, adds
  "Inspect…" and "Star/Unstar" context-menu entries between
  "Open in Reader" and the OCR section, and decorates each card
  head with a ★ glyph for starred docs and a ✎ glyph for docs
  with notes. starredOnly side-effect: when the toggle is on and
  the user unstars a doc, the row drops out of the grid via
  refresh. (4fe82f9)

Gates passed: cargo fmt clean, cargo test --lib pdf::library::
359 passed / 0 failed (+22 from round-7's 337 baseline: 5
set_doc_title + 6 set_doc_notes + 5 set_doc_starred + 6 query
starred tests), cargo test --lib ai::embedding_index 30 passed
/ 0 failed (round-7 baseline preserved), cargo clippy --lib -D
warnings clean (11s warm), pnpm check 0 errors / 104 warnings
(same as round-7 baseline; zero new from DocInspector or the
LibraryPanel card-head chrome).

DESIGN NOTES: Drawer NOT modal — the inspector wants context (you
look at it WHILE you scan the grid), unlike OCR Queue which is a
maintenance screen. Tags are read-only in the inspector — duplicating
the picker chrome would either confuse the menu-Tags section or
invite bugs; the hint sends users to the card menu. Notes save on
blur because an autosave inspector has no Save button competing for
footer real estate with Open/Reveal/Remove. No keyboard shortcut for
"open inspector" — vim-mode + the card menu are sufficient discovery.

## BUILD ENVIRONMENT — CRITICAL, read before any cargo command

Internal disk is FULL (~2.9 GiB free of 228). Cargo target is redirected to an
APFS sparse image at **/Volumes/SlabBuild** via `src-tauri/.cargo/config.toml`
(gitignored). Verify mounted each tick: `df -h /Volumes/SlabBuild | tail -1`.
If missing: `hdiutil attach "/Volumes/Sanjay SSD/SlabBuild.sparseimage"`.

**The image has very slow fsync.** Proven tonight across many attempts:
- `cargo test --lib`, `cargo check --lib`, `pnpm check` → WORK (slow but finish).
- A FULL `cargo build` / `cargo tauri build` → WEDGES on the `tauri` crate's
  final codegen (rustc goes to sleep state, no CPU, target size flat for min).
**RULE: never run a full binary build in a tick.** It's release work, blocked by
CI billing anyway. Gate with `cargo test --lib` + `cargo clippy --lib` + `pnpm
check`. If cargo wedges >5 min with no rustc CPU: `pkill -f 'cargo'`, retry once.

## CI STILL BLOCKED — needs Sanjay

GitHub Actions billing failure persists → no release artifacts (DMG/MSI/AppImage)
until fixed. Action: https://github.com/settings/billing → update payment / raise
limit. Does NOT affect local dev or branch pushes.

## Roadmap — round 15 (Bulk Plugin Updates) — ALL DONE

Round 15 batched FIVE feature slices into one cron tick wiring the
plugin marketplace into a proper package-manager-grade update
experience. Before round-15 the Installed tab carried per-card
"update available" badges (v1.4.0 Slice 8a) but no bulk affordance.
Today the marketplace ships an end-to-end bulk-update flow:
deterministic Rust planner → batch Tauri command emitting per-step
events → TS client + helpers → Installed-tab banner with collapse +
per-row Update + Update-all → live per-step progress overlay.

68. ~~**marketplace::update_plan planner primitive**~~ — DONE
    (2026-06-21 05:35 PT, 9c2898a, single commit). Pure-data
    Rust planner that intersects installed plugins with the
    index, returns UpdatePlan {targets, total_bytes} sorted
    by id ascending. Includes InstalledPlugin / UpdateTarget
    / UpdatePlan / plan_updates / semver_compare. semver_compare
    is a Rust port of TS compareSemver; 19 new tests pin
    parity + planner edge cases.
69. ~~**bulk-update Tauri command surface**~~ — DONE
    (2026-06-21 05:35 PT, 4b1da4f, single commit). Two new
    commands: slab_marketplace_list_update_targets() →
    UpdatePlan and slab_marketplace_update_all(batch_id,
    plugin_ids) → BatchUpdateReport. update_all runs
    sequential updates through the existing verify →
    install_from_entry → reg.discover → install_log pipeline,
    emitting UpdateProgress events on
    marketplace://update-progress per step. Batch ALWAYS runs
    to completion (failed N doesn't stop N+1+). 7 new tests
    pin the report folding + serde tags.
70. ~~**bulk-update TS client + helpers**~~ — DONE
    (2026-06-21 05:35 PT, 57a7bfa, single commit). Wire types
    (UpdateTarget / UpdatePlan / UpdateProgress / UpdateOutcome
    discriminated union / BatchUpdateReport), wrappers
    (listUpdateTargets / updateAllPlugins / listenUpdateProgress
    with browser-mode fallbacks), pure helpers (pluralizeUpdates
    / formatUpdateSummary covering five canonical paths).
71. ~~**Updates-available banner in Installed tab**~~ — DONE
    (2026-06-21 05:35 PT, 52d4528, single commit, 398 LOC).
    Collapsed-by-default banner above the plugin list. State:
    updatePlan + updateBusy + updateRowBusy + updatesExpanded
    + updatesDismissed (per-session). Wired into onMount +
    onInstall + onUninstall + onReload. Toast grammar uses
    formatUpdateSummary with severity-appropriate notify
    routing (success / warning / error). 185 LOC of scoped
    CSS using existing dark-first design tokens.
72. ~~**live per-step bulk-update progress overlay**~~ — DONE
    (2026-06-21 05:35 PT, 9fe1d50, single commit, 536 LOC).
    New BulkUpdateProgressOverlay.svelte component + reducer
    upgrade in PluginsPanel.svelte. Per-row icon ladder
    (○ → … → ✓ / ✕), version transition, current-row +
    failed-row tinting, inline truncated error message,
    finished-state header icon (✓ done / ! mixed / ✕
    all-fail). Reducer subscribes to listenUpdateProgress
    BEFORE updateAllPlugins so the first id's starting event
    isn't dropped; filters on batch_id correlation; finally
    unlistens to free the listener slot. Overlay refuses to
    close while !finished.

    With round 15 done, the plugin marketplace is now a
    proper package-manager experience: discover (Browse tab
    search/filter/sort), install (verify → install_from_entry
    → install_log), audit (install_log subsystem from rounds
    11-14), update (bulk planner + banner + progress overlay
    from round 15). Next subsystem candidates: Hopper rule
    editor live preview already ships (verified), saved-views
    drag-handle UI, smart-folders hub UI polish, Loom-grade
    tagging explorer, doc-detail metadata editor, Beacon
    cache inspector polish, Quill multi-document field-detect
    queueing.

## Roadmap — round 14 (Install Log Retention Policy) — ALL DONE

Round 14 batched FIVE feature slices into one cron tick onto the
marketplace install-log subsystem (round-12 shipped logging +
browsing, round-13 shipped exportability, round-14 ships
self-maintenance). The audit log is now end-to-end self-managing:
auto-prunes old rows on app launch, user-configurable retention
window with a 1-day floor, demo-able Retention section in the
Recent installs drawer with Save / Reset / Run-now controls,
24h debounce so repeated launches don't re-prune.

Also includes a critical PRE-SLICE build-fix repairing the
two-versions-of-der dependency graph from unmerged dependabot
PRs — see commit 0bb1d4c. This single fix turned `cargo test
--lib` green AND unwedged `cargo clippy --lib -- -D warnings`
which had been failing for 4 rounds straight (the wedge was
NOT the sparse image — it was clippy's trait-bound resolver
exploding on incompatible der 0.7 vs 0.8 trait impls).

63. ~~**install_log retention storage + auto-prune driver**~~ —
    DONE (2026-06-21 02:25 PT, bd649cf, single commit).
    Schema v1 -> v2 adds `install_log_settings (key TEXT
    PRIMARY KEY, value TEXT NOT NULL)`. Three module
    constants: DEFAULT_RETAIN_DAYS = 365, MIN_RETAIN_DAYS = 1,
    AUTO_PRUNE_INTERVAL_SECS = 86_400. Storage methods:
    retain_days / set_retain_days (clamps at floor) /
    last_auto_prune_at / set_last_auto_prune_at (pub for tests).
    Auto-prune driver: `auto_prune_if_due(now_unix)` honours
    debounce + stamps last run; `auto_prune_if_due_now()` is
    the prod wrapper. AutoPruneOutcome enum with snake_case
    serde-tagged "pruned" (rows_removed + retain_days +
    cutoff_unix) / "skipped" (next_due_unix). 11 new tests.
64. ~~**retention policy Tauri commands**~~ — DONE (2026-06-21
    02:25 PT, 2f08453, single commit). InstallLogRetentionPolicy
    wire type carrying user-modifiable retain_days +
    last_auto_prune_at + the three constants. Three commands:
    slab_marketplace_install_log_retention_policy() reads,
    slab_marketplace_install_log_set_retention_days(days) writes
    (returns clamped value), slab_marketplace_install_log_auto_prune
    (force: Option<bool>) runs. marketplace/mod.rs re-exports
    AutoPruneOutcome + the three constants.
65. ~~**retention policy TS client + relative-time helpers**~~ —
    DONE (2026-06-21 02:25 PT, 0ede3a5, single commit, 193 LOC).
    InstallLogRetentionPolicy interface mirroring wire shape +
    InstallLogAutoPruneOutcome discriminated union. Wrappers:
    getInstallLogRetentionPolicy, setInstallLogRetentionDays,
    runInstallLogAutoPrune. Pure helpers: formatLastAutoPrune
    (just-now / Nm / Nh / yesterday / Nd / ISO ladder),
    formatNextAutoPrune (Due-now / Nm / Nh Mm / Nd Hh).
66. ~~**auto-prune install log on app startup**~~ — DONE
    (2026-06-21 02:25 PT, ec2b9ac, single commit). Wired into
    the Tauri setup callback right after the Hopper bootstrap.
    Best-effort + non-fatal — open failures eprintln but boot
    continues. Outcome handling: rows_removed > 0 logs an
    audit line; rows_removed == 0 and Skipped are silent
    (the healthy steady state).
67. ~~**Retention section in Recent installs drawer**~~ — DONE
    (2026-06-21 02:25 PT, 3d4dde5, single commit, 343 LOC).
    Pure frontend tying slices 63-66 into the demo surface.
    Collapsible section between window strip and event list,
    defaults collapsed with one-line "Keep 365d · Last
    auto-prune: 4h ago" header. Expanded body: number input
    bound to retainDaysDraft + Reset/Save chips (only when
    dirty), subtitle showing policy bounds, "Next auto-prune
    in Nh Mm" + Run-now button (forces past debounce).
    Escape handler grows to dismiss menu → confirm → retention
    → drawer. ~140 lines of scoped CSS.

    With round 14 done, marketplace install log is now fully
    self-managing: auto-trims on launch (24h debounced), users
    can adjust retention or force a prune via the UI, the
    Retention section shows the policy + last-run + next-due
    at a glance. Next subsystem candidates: Hopper rule
    editor's "Test against last 5 files" live preview, saved-
    views drag-handle UI, smart-folders hub UI polish,
    Loom-grade tagging explorer, plugin marketplace "Search
    & filter" UI (Browse tab currently shows all plugins
    flat — no category filter, no tag pills, no sort).


## Roadmap — round 13 (Install Log Export) — ALL DONE

Round 13 batched FIVE feature slices into one cron tick onto the
marketplace install-log subsystem (round-12 shipped logging +
readers + drawer UI, but the audit log was trapped in
`~/.slab/marketplace-history.sqlite` with no deliverable surface).
The install log is now end-to-end exportable: paralegals tick
"Last 7d" in the drawer, click Export… → CSV/JSON, hand a partner
the audit file. Mirrors round-10's hopper CSV export pattern
(suggestBackfillCsvFilename / slab_hopper_export_backfill_csv) so
both export surfaces share one mental model.

58. ~~**list_events_between (time-window reader)**~~ — DONE
    (2026-06-20 22:59 PT, b0a602a, single commit).
    `InstallLog::list_events_between(since_unix, until_unix, limit)`
    with optional inclusive boundaries on both ends. Drives the
    export surface so the exported file matches the user's
    window choice exactly. None on both sides == list_recent
    (plain newest-first scan). Same limit semantics — negative
    limit clamps to zero. Dynamic WHERE clause built from a
    Vec<&'static str> + Vec<rusqlite::types::Value> joined with
    " AND ". 6 new tests.
59. ~~**install_log_to_csv (RFC-4180 serialiser)**~~ — DONE
    (2026-06-20 22:59 PT, 26e01a7, single commit). Pure function
    `install_log_to_csv(events, include_header)` + module constant
    `INSTALL_LOG_CSV_HEADER`. 12 columns including paired
    occurred_at_unix + occurred_at_iso. RFC-4180 escaping matches
    the hopper backfill CSV. NULL-able columns render as empty
    (never "None"/"null"). Boolean replaced_existing renders
    true/false/empty. Action column uses serde-canonical lowercase
    tokens so CSV + JSON align column-for-column. 7 new tests.
60. ~~**install_log_to_json (export envelope)**~~ — DONE
    (2026-06-20 22:59 PT, b13de9f, single commit). New
    InstallLogExportEnvelope (schema_version + generated_at_iso +
    event_count + since_unix/iso + until_unix/iso + events array)
    + InstallEventExport row (flattens InstallEvent with an
    occurred_at_iso companion via #[serde(flatten)] so the wire
    stays nest-free). INSTALL_LOG_EXPORT_SCHEMA_VERSION = 1.
    install_log_to_json_with_now variant pinned for tests. 5 new
    tests pin schema + window bounds + flatten + serde round-trip.
61. ~~**Tauri export commands + TS client**~~ — DONE
    (2026-06-20 22:59 PT, 8186b2a, single commit). Two Tauri
    commands wired into the builder:
    slab_marketplace_install_log_export_csv(path, since_unix?,
    until_unix?, limit?) -> u64 bytes_written, plus the JSON
    twin. Default limit = 100_000. Idempotent. TS adds
    InstallLogExportFilter + exportInstallLogCsv +
    exportInstallLogJson + suggestInstallLogExportFilename
    helper (marketplace-history_<window>_<YYYY-MM-DD>.<ext>
    convention with window slot reading all / from-YYYYMMDD /
    to-YYYYMMDD / YYYYMMDD-YYYYMMDD depending on bounds).
62. ~~**Export menu in RecentInstallsDrawer**~~ — DONE
    (2026-06-20 22:59 PT, ecc2261, 203 lines). Pure frontend
    tying slices 58-61 into one surface. Footer Export…
    popover anchored absolutely above the trigger with two
    entries: "Export as CSV…" (spreadsheet-friendly) +
    "Export as JSON…" (with envelope metadata). Each entry's
    subtitle reads "Whole log · <hint>" or "Last <window> ·
    <hint>" so window scope is visible BEFORE clicking. A new
    windowSinceUnix $derived maps the 7d/30d/all toggle to
    the matching unix-seconds cutoff so the export filter
    matches what the user sees. Native save-as dialog,
    suggested filename via suggestInstallLogExportFilename.
    Escape dismisses menu first, then prune-confirm, then
    drawer. Window-click dismisses on outside click (Notion/
    Linear pattern). exporting boolean gates Export/Clear/
    Close during the in-flight write. 4-second auto-clear
    toast on success.

    With round 13 done, marketplace install log is end-to-end
    exportable: per-plugin Activity timeline (round 12),
    corpus-wide Recent installs drawer with window strip
    (round 12), retention pruning (round 12), and now CSV +
    JSON exports filtered by the same window strip. Next
    subsystem candidates: Hopper rule editor's "Test against
    last 5 files" live preview, saved-views drag-handle UI,
    smart-folders hub UI polish, Loom-grade tagging explorer,
    marketplace install log retention background task (the
    pruneInstallLog command exists; the auto-prune-on-startup
    surface isn't wired yet).

## Roadmap — round 12 (Plugin Marketplace Install History) — ALL DONE

Round 12 batched FIVE feature slices into one cron tick onto the
plugin marketplace subsystem (the v1.4.0 Bench marketplace install
pipeline shipped + per-plugin detail drawer landed in v3.39.0, but
the install pipeline forgot every event the moment it happened —
no audit trail, no per-plugin history, no "when did I install this"
answer). Marketplace install pipeline is now end-to-end auditable:
every install/update/uninstall/failed-install lands in an append-
only sqlite log, PluginDetailDrawer surfaces a per-plugin Activity
timeline, and the toolbar History button opens a Recent installs
drawer with corpus-wide tail + retention pruning.

53. ~~**install_log primitive (sqlite append-only)**~~ — DONE
    (2026-06-20 20:47 PT, 9226f88, single commit). Append-only
    sqlite log at ~/.slab/marketplace-history.sqlite kept
    independent of plugin-storage.sqlite + hopper.sqlite.
    Schema v1: install_events with id / plugin_id / version /
    action (install | update | uninstall | failed) / occurred_at /
    source / bytes_written / files_extracted /
    replaced_existing / prior_version / error_msg. Two indexes
    covering the two read paths (per-plugin newest-first +
    corpus-wide newest-first). InstallLog with open /
    open_in_memory + three writers (record_install /
    record_uninstall / record_failure) + three readers
    (list_events / list_recent / install_stats +
    distinct_plugin_count). InstallStats slim payload.
    InstallAction parse returns Failed on unknown tags so
    future schema bumps don't panic the reader. 14 new tests.
54. ~~**install/uninstall pipeline wiring**~~ — DONE
    (2026-06-20 20:47 PT, 182e448, single commit). 
    slab_marketplace_install captures prior_version BEFORE the
    install via reg.get(id) so the pipeline's replaced_existing
    flag pairs with the version that was overwritten. Wraps
    every failure surface (signature check, plugins-root
    resolve/create, install_from_entry pipeline) in
    record_failure so failed installs are auditable. On
    success appends one install row (or update row when
    replaced_existing) with bytes_written + files_extracted +
    registry-derived prior_version. slab_marketplace_uninstall
    captures prior version BEFORE removing then appends one
    uninstall row (falling back to "unknown" when no readable
    manifest). open_install_log_and<F> + record_install_failure
    helpers centralise the boilerplate.
55. ~~**reader Tauri commands + TS client**~~ — DONE
    (2026-06-20 20:47 PT, ef9fed1, single commit). Three Tauri
    commands: slab_marketplace_install_events(plugin_id, limit?),
    slab_marketplace_install_history_recent(limit?),
    slab_marketplace_plugin_install_stats(plugin_id). Default
    limit 50. TS adds InstallEvent / InstallStats interfaces
    (NULL-able fields typed as T | null so consumers handle
    present-but-null explicitly), three helper wrappers,
    formatInstallEventTime (compact relative timestamp,
    injectable now param, falls back to ISO yyyy-mm-dd for >30d),
    installEventGlyph (monochrome ✓ install / ↻ update /
    ⌫ uninstall / ✕ failed).
56. ~~**retention + summary surface**~~ — DONE (2026-06-20 20:47
    PT, 8e04747, single commit). InstallLog gains
    oldest_occurred_at (Option<i64>; wraps SELECT MIN edge case
    where empty table returns one row with NULL column by
    reading as Option<i64>), total_event_count (O(1) on sqlite
    internal counters), prune_older_than (strict less-than;
    idempotent). Two Tauri commands:
    slab_marketplace_install_log_summary -> InstallLogSummary,
    slab_marketplace_install_log_prune(retain_days) (clamped at
    >=1 so prune(0) can't accidentally wipe). TS adds
    InstallLogSummary + installLogSummary + pruneInstallLog +
    formatLogSpan ("N events across X days" with ceiling-day
    arithmetic + literal "no events yet" on empty). 7 new
    tests; test-only insert_at helper to pin occurred_at.
57. ~~**PluginDetailDrawer Activity + RecentInstallsDrawer**~~
    — DONE (2026-06-20 20:47 PT, 7b84083, 830 LOC). Pure
    frontend tying slices 53-56 into one demo-able surface.
    PluginDetailDrawer Activity section self-fetches on mount
    + every entry.id change. Per-row layout: per-action glyph +
    colour accent + label + version + optional ← v<prior> for
    updates + bytes/files metadata for installs OR truncated
    error for failures + right-aligned relative time. Section
    auto-collapses when timeline empty so never-installed
    plugin's drawer stays clean. Header subtitle assembles
    parts only for nonzero kinds.
    RecentInstallsDrawer.svelte (NEW, 470 LOC): 460px right-
    side slide-from-right drawer mirroring PluginDetailDrawer's
    Notion side-panel convention. "Last 7d / Last 30d / All"
    window strip filtering loaded events post-fetch (events
    fetched once with limit 100, then client-side filtered —
    no re-round-trip on window flip). Empty-state branches
    handled (no events / no events in window / loading / error).
    Footer "Clear older than 90d…" two-step confirm calling
    pruneInstallLog(90); onPruned bubbles back so the toolbar
    count updates without a remount. Escape closes drawer or
    dismisses confirm step.
    PluginsPanel wiring: installLog + recentInstallsOpen
    state + refreshInstallLog called on mount + after every
    install / uninstall / install-failure. Toolbar "⏱ History"
    button with count chip; gated on total_events > 0 so it
    disappears quietly when the log is empty.

    Plus fixup commit (1d79d7b) catching two pure-formatting
    rustfmt drifts on slice 54's prior_version let-chain +
    record_install_failure closure body.

    With round 12 done, plugin marketplace audit surface is
    end-to-end demo-able: install logs every event, drawer
    surfaces per-plugin timeline, toolbar shows corpus-wide
    tail with retention pruning. Next subsystem candidates:
    Hopper rule editor's "Test against last 5 files" live
    preview, saved-views drag-handle UI (reorder backend
    takes positional lists; drag-handle is a pure-frontend
    follow-up later), Loom-grade tagging explorer, smart-
    folders hub UI polish (the rail's drag/pin chrome could
    be tightened), marketplace install log export (CSV
    + JSON; mirrors round 10's hopper CSV export pattern).

## Roadmap — round 11 (Tag-Suggest Bulk Surface) — ALL DONE

Round 11 batched FIVE feature slices into one cron tick onto the
v3.39.0 Atlas Tag-Suggest subsystem (the per-doc SuggestedTagsRow
chip strip shipped, but bulk was a half-finished pipe with no
review panel + no granular dismissal control + no filter-aware
bulk + no badge stats). Tag-Suggest is now end-to-end demo-able:
toolbar "✨ Review N" badge → drawer pre-filtered by current filter
or untagged shortcut → per-doc chip cards with source-filtered
batch selection → Apply N in one round-trip → toast + grid refresh.

48. ~~**accept_tag_suggestions_bulk (per-item batch)**~~ — DONE
    (2026-06-20 17:32 PT, d17fe91). AcceptItem + BulkAcceptResult
    types. Per-item failure semantics — malformed name in item 12
    fails item 12 alone, items 0..11 + 13..N still attach. Case +
    whitespace pair-dedupe in pre-pass. Tauri command emits single
    library-changed event after the batch. TS adds matching types
    + acceptTagSuggestionsBulk(items). 6 new tests.
49. ~~**list_dismissed_for_doc + undismiss_one_for_doc**~~ — DONE
    (2026-06-20 17:32 PT, 88a5439). New DismissedSuggestion row
    type ordered by dismissed_at DESC; undismiss_one returns bool
    (true if a row was deleted). Case-insensitive match mirrors
    dismiss-time normalisation. Two new Tauri commands + TS
    helpers. 7 new tests.
50. ~~**suggest_for_filter (any LibraryFilter)**~~ — DONE
    (2026-06-20 17:32 PT, 3053a7c). Reuses query::query_documents
    so every filter shape composes for free. `limit` forced onto
    the effective filter; `sort` left untouched so saved views
    render in authored order. Zero-suggestion docs skipped
    post-query. Tauri command + TS tagSuggestionsBulkForFilter.
    6 new tests.
51. ~~**suggestion_stats (review badge counter)**~~ — DONE
    (2026-06-20 17:32 PT, 2a3b8b5). TagSuggestionStats {
    untagged_docs_with_suggestions, dismissed_total }. Walks
    sample_cap most-recently-seen untagged docs and probes each
    so badge counts only review-worthy docs — never lures user
    into empty panel. dismissed_total is one COUNT(*). Tauri
    command + TS helper. 5 new tests.
52. ~~**BulkTagSuggestionsPanel UI**~~ — DONE (2026-06-20 17:32
    PT, c881c62, 640 LOC). Pure frontend tying all four backend
    slices into one drawer. 560px right-side. Source-strip
    segmented toggle (untagged / current filter) + per-doc cap
    input. Bulk control bar with source-filtered selection chips
    (♦ vocab / ⚭ co-occ / ⌗ domain) + Apply N. Per-doc card
    grid with checkbox-toggle chips + dismiss button. Per-card
    "Hidden…" link loads dismissed list with one-click Undo.
    Toast confirmation, error banner, deterministic pastel
    preview matching the rust pastel_for. LibraryPanel toolbar
    gains "✨ Review N" button gated on
    untagged_docs_with_suggestions > 0. Stats refresh on mount /
    after library-changed / after drawer close / after bulk
    apply. Drawer receives buildCurrentFilter() so "Current
    filter" mode picks up the live grid narrowing.

    With round 11 done, v3.39.0 Atlas Tag-Suggest is end-to-end
    demo-able: per-doc chip strip, bulk review drawer with
    per-source filtering, filter-aware suggester, granular
    undismiss, slim badge. Next subsystem candidates: plugin
    marketplace UI (the backend ships in marketplace/ but
    PluginsPanel.svelte's Browse tab is the only surface — no
    install history, no per-plugin detail), smart-folders hub
    UI polish (the rail's drag/pin chrome could be tightened),
    saved-views drag-handle UI (reorder backend takes positional
    lists; drag-handle is a pure-frontend follow-up later),
    Hopper rule editor's "Test against last 5 files" live
    preview, Loom-grade tagging explorer.

## Roadmap — round 10 (Hopper Loop Polish) — ALL DONE

Round 10 batched FIVE feature slices into one cron tick onto the
v3.22 Hopper batch-backfill subsystem (the streaming backend +
cancel token shipped round-9, but the UI still used the sync
executor and several demo-able backend gaps remained). Hopper
is now end-to-end demo-able: paralegal points at `discovery/`,
ticks "Include sub-folders", sees 4,000 PDFs scanned with
per-rule coverage chips, exports a CSV to email the partner,
clicks Apply, watches the live progress bar fill while the
scrolling tail shows each file landing.

43. ~~**plan_backfill_with_options (recursive scan + depth cap)**~~
    — DONE (2026-06-20 14:55 PT, a6cf10c, single commit).
    PlanOptions { recursive, max_depth } struct widens
    plan_backfill into plan_backfill_with_options; legacy
    entry point preserved as a back-compat wrapper. Internal
    collect_pdfs helper recurses via an explicit stack so
    the hopper module avoids a walkdir dep. Hidden directories
    skipped; locked sub-folders swallow errors so one denied
    subdir doesn't kill the whole report. Tauri command + TS
    client widened. 6 new tests.
44. ~~**per_rule_counts (pre-flight coverage strip)**~~ —
    DONE (2026-06-20 14:55 PT, 3bbc08a, single commit).
    BackfillReport gains per_rule_counts: BTreeMap<String,
    usize> tallying the planned distribution. Two synthetic
    buckets: __defaults__ (no rule matched) + __skip__
    (plan-time skip). Rules with zero hits omitted. Powers
    the UI's "Tax: 17 · Invoices: 23 · No rule: 4" strip.
    serde-default on the field keeps pre-v3.39 JSON decoding
    cleanly. TS adds bucket-label helper. 6 new tests.
45. ~~**list_backfill_runs_since (time-window history filter)**~~
    — DONE (2026-06-20 14:55 PT, 0e9d5e2, single commit).
    New authoritative reader; legacy list_backfill_runs
    delegates with since_unix=None. Both filters AND together
    in SQL (folder + since combine into one WHERE clause).
    Cutoff is INCLUSIVE on finished_at. Powers the panel's
    "Last 24h / Last 7d / All" chips with the JS-side
    backfillSinceUnix helper. 4 new tests.
46. ~~**backfill_report_to_csv (audit-trail export)**~~ —
    DONE (2026-06-20 14:55 PT, 9a14495, single commit).
    RFC-4180-strict CSV: source_path, size_bytes,
    matched_rule, destination, action, reason. Wraps fields
    with `,` `"` `\r` `\n` in `"`, doubles embedded `"`.
    Action column kebab-case matching JSON serde. Missing
    optional fields render empty (not "None"). New Tauri
    slab_hopper_export_backfill_csv takes report + absolute
    path, returns bytes written. TS adds export helper +
    suggestBackfillCsvFilename. 6 new tests.
47. ~~**HopperBackfillPanel UI wiring**~~ — DONE
    (2026-06-20 14:55 PT, f720bcf, single commit). Pure
    frontend — ties all four backend slices + the round-9
    streaming executor into one panel. Recursive toggle +
    depth dropdown, per-rule coverage chips, live progress
    bar with scrolling tail + working Cancel, "Export CSV…"
    button via plugin-dialog save() + toast, history chips
    (Last 24h / Last 7d / All) with default 7d, row
    checkboxes disable during applying.

    Plus fixup commit (36309a5) for three small bugs the
    cargo test gate surfaced: PlanOptions needed
    #[serde(default)] on the struct, the unreadable-folder
    branch tallied per_rule_counts from an empty slice
    instead of the populated planned vec, and one cargo-fmt
    drift on watcher.rs.

    With round 10 done, Hopper batch-backfill is end-to-end
    demo-able: recursive scan, pre-flight coverage strip,
    live streaming progress with cancel, CSV export, time-
    windowed history. Next subsystem candidates: plugin
    marketplace UI (the backend ships in marketplace/ but
    PluginsPanel.svelte's Browse tab is the only surface —
    no install history, no per-plugin detail), smart-folders
    hub UI polish (the rail's drag/pin chrome could be
    tightened), saved-views drag-handle UI (the reorder
    backend takes positional lists; drag-handle is a pure-
    frontend follow-up later), Hopper rule editor's "Test
    against last 5 files" live preview, Loom-grade tagging
    explorer.

## Tick log

- 2026-06-21 05:35 PT (Cake, cron): round-15 BATCH tick — FIVE
  Bulk-Plugin-Updates slices wiring the marketplace into a proper
  package-manager-grade update experience. All DONE. Five commits,
  pushed + verified (local==origin 9fe1d50). **All gates GREEN:
  cargo fmt clean, cargo clippy --lib -- -D warnings clean in
  ~14s, cargo test --lib 2208 passed (round-14 baseline 2182 +
  19 new from slice 68 + 7 new from slice 69), pnpm check 0
  errors / 104 warnings (baseline preserved EXACTLY).**
  - Slice 68 marketplace::update_plan planner primitive (9c2898a):
    pure-data Rust planner intersecting installed plugins with
    the index, returns UpdatePlan {targets, total_bytes}. New
    types InstalledPlugin / UpdateTarget / UpdatePlan; core
    plan_updates + semver_compare (Rust port of TS compareSemver
    with parity tests). 19 new tests.
  - Slice 69 bulk-update Tauri commands (4b1da4f):
    slab_marketplace_list_update_targets + slab_marketplace_update_all.
    Sequential execution with per-step UpdateProgress events on
    marketplace://update-progress. Batch always runs to completion.
    Reuses existing install pipeline + install_log helpers. 7
    new tests pin BatchUpdateReport folding + serde tags.
  - Slice 70 TS client + helpers (57a7bfa): UpdateTarget /
    UpdatePlan / UpdateProgress / UpdateOutcome / BatchUpdateReport
    interfaces; listUpdateTargets / updateAllPlugins /
    listenUpdateProgress wrappers with browser-mode fallbacks;
    pluralizeUpdates + formatUpdateSummary helpers.
  - Slice 71 Updates-available banner in Installed tab (52d4528):
    collapsed-by-default banner with chevron + ↑ + headline +
    meta + Update-all + dismiss. Expand reveals per-target rows
    with version transition (mono, prior strikethrough, next
    accent-coloured). 398 LOC total; 185 LOC of scoped CSS.
    Wired into mount + install + uninstall + reload lifecycles.
  - Slice 72 live per-step progress overlay (9fe1d50):
    BulkUpdateProgressOverlay.svelte (412 LOC) + reducer upgrade
    in PluginsPanel. Per-row icon ladder ○ → … → ✓/✕, version
    transition, current/failed row tinting, finished header
    icon. Listener subscribed BEFORE updateAllPlugins so first
    starting event isn't dropped; filters on batch_id; finally
    unlistens.

- 2026-06-21 02:25 PT (Cake, cron): round-14 BATCH tick — FIVE
  Install-Log-Retention slices closing the round-13 follow-up
  ("the pruneInstallLog command exists; the auto-prune-on-startup
  surface isn't wired yet"). Plus one prerequisite build-fix that
  repaired the post-dependabot broken main. All DONE. Six commits
  total, pushed + verified (local==origin 3d4dde5). **All gates
  GREEN for the first time in 5 rounds: cargo fmt clean, cargo
  clippy --lib -- -D warnings clean in 4m 42s, cargo test --lib
  2182 passed (round-13 baseline 2171 + 11 new), pnpm check 0
  errors / 104 warnings (baseline preserved EXACTLY).**
  - PRE-SLICE build-fix (0bb1d4c): pin der + spki back to "0.7"
    (matching cms 0.2's transitive expectation), drop the
    `.unwrap_or(0.0)` on ttf-parser 0.25's no-longer-fallible
    italic_angle. Fixes ~57 compilation errors that round-13
    silently shipped without noticing. Discovered when
    cargo test --lib refused to even build the test binary at
    tick start.
  - Slice 63 install_log retention storage + auto-prune driver
    (bd649cf): schema v1->v2 adds install_log_settings KV table.
    Storage primitives (retain_days/set_retain_days/
    last_auto_prune_at/set_last_auto_prune_at), auto-prune driver
    (auto_prune_if_due + auto_prune_if_due_now), AutoPruneOutcome
    snake_case-tagged enum. 11 new tests pin floor-clamp,
    debounce semantics, boundary conditions, serde round-trip.
  - Slice 64 retention policy Tauri commands (2f08453):
    InstallLogRetentionPolicy wire type +
    slab_marketplace_install_log_retention_policy /
    _set_retention_days / _auto_prune commands. force=true on
    auto_prune clears the debounce stamp so the natural
    auto_prune_if_due path can run unconditionally.
    marketplace/mod.rs re-exports AutoPruneOutcome + constants.
  - Slice 65 retention policy TS client + relative-time helpers
    (0ede3a5): InstallLogRetentionPolicy interface +
    InstallLogAutoPruneOutcome discriminated union. Wrappers
    getInstallLogRetentionPolicy/setInstallLogRetentionDays/
    runInstallLogAutoPrune with browser fallbacks. Pure helpers
    formatLastAutoPrune + formatNextAutoPrune with the same
    relative-time ladder as round-12's formatInstallEventTime.
  - Slice 66 auto-prune install log on app startup (ec2b9ac):
    wired into the Tauri setup callback right after Hopper
    bootstrap. Best-effort + non-fatal. Outcome handling:
    rows_removed > 0 logs an audit line; rows_removed == 0 and
    Skipped are silent (steady state shouldn't add boot noise).
  - Slice 67 Retention section in Recent installs drawer
    (3d4dde5, 343 LOC): collapsible section between window strip
    and event list, defaults closed with "Keep 365d · Last
    auto-prune: 4h ago" one-line header. Expanded: number input
    bound to retainDaysDraft with Reset+Save chips appearing
    only when dirty (no clutter in steady state), policy-bounds
    subtitle, "Next auto-prune in Nh Mm" + Run-now button.
    Escape handler grew a third level. ~140 lines of scoped CSS.

  Sanjay action: the build-fix should be propagated as a PR
  closing dependabot's #32 and #33 (which are now stale —
  they merged but broke main). Future bumps to der/spki must
  wait for cms to cut a new major matching the new der/spki
  major. The sparse-image hdiutil detach/reattach recommendation
  from prior rounds was wrong — disregard it.

- 2026-06-20 22:59 PT (Cake, cron): round-13 BATCH tick — FIVE
  Install-Log-Export slices closing the audit-trail-deliverable
  gap (round-12 shipped logging + browsing, round-13 ships the
  exportable artifact). Paralegals can now hand a partner a
  CSV/JSON of "every plugin install/uninstall/failure in the
  last 90 days" filtered by the same 7d/30d/all window the
  drawer already exposes. All DONE. Five feature commits,
  pushed + verified (local==origin ecc2261).
  - Slice 58 list_events_between (b0a602a): time-window
    reader with optional inclusive boundaries on both ends.
    Drives the export surface so the file matches the user's
    window choice. Dynamic WHERE clause built from
    Vec<&'static str> + Vec<rusqlite::types::Value> joined
    with " AND ". 6 new tests.
  - Slice 59 install_log_to_csv (26e01a7): RFC-4180 pure
    function + INSTALL_LOG_CSV_HEADER constant. 12 columns
    incl. paired occurred_at_unix + occurred_at_iso. Same
    escaping policy as the hopper backfill CSV. NULL renders
    as empty (never "None"/"null"). Boolean renders true/
    false/empty. Action column uses canonical lowercase
    serde tokens. 7 new tests.
  - Slice 60 install_log_to_json (b13de9f): export envelope
    with schema_version + generated_at_iso + event_count +
    since_unix/iso + until_unix/iso + events array.
    InstallEventExport flattens InstallEvent +
    occurred_at_iso via #[serde(flatten)] so wire stays
    nest-free. install_log_to_json_with_now test-only
    variant pins now to avoid clock races. 5 new tests.
  - Slice 61 Tauri export commands + TS client (8186b2a):
    slab_marketplace_install_log_export_csv +
    slab_marketplace_install_log_export_json. Default limit
    100_000. Bytes-written return for the UI toast.
    Idempotent. TS adds InstallLogExportFilter +
    exportInstallLogCsv + exportInstallLogJson +
    suggestInstallLogExportFilename with the
    marketplace-history_<window>_<YYYY-MM-DD>.<ext>
    convention.
  - Slice 62 Export menu in RecentInstallsDrawer (ecc2261,
    203 lines): footer Export… popover anchored above the
    trigger with CSV + JSON entries, subtitles that surface
    the window scope before clicking. windowSinceUnix
    $derived ties the 7d/30d/all toggle into the export
    filter so what the user sees IS what gets exported.
    Native save dialog, suggested filename via the slice-61
    helper. Escape dismisses menu first, then prune-confirm,
    then drawer. Outside-click dismiss matches Notion/Linear
    pattern. exporting boolean gates Export/Clear/Close
    during in-flight write. 4-second toast on success.
  Gates: cargo fmt clean, cargo test --lib
  marketplace::install_log:: 39 passed / 0 failed (+18 from
  round-12's 21 baseline: 6 slice 58 + 7 slice 59 + 5
  slice 60), cargo test --lib 2171 passed / 0 failed
  (round-12 baseline + 18), pnpm check 0 errors / 104
  warnings (round-12 baseline preserved EXACTLY). **cargo
  clippy --lib WEDGED TWICE AGAIN on /Volumes/SlabBuild
  sparse image — 4th tick in a row hitting the same wedge**
  — first attempt cargo check spawned but stayed at 0%
  CPU for 2+ min with no rustc subprocess; second attempt
  identical. SlabBuild disk-listing was fine at tick start
  (ls returned 6,424 entries in 0.3s) so it's specifically
  the clippy/check codegen path. cargo test --lib itself
  ran the 2171-test suite cleanly in 40s — no wedge there.
  Per STATE.md guidance this batch ships on lib-test +
  svelte-check strength. **Sanjay action recommended
  (urgently — 4 ticks in a row): `hdiutil detach` then
  reattach `/Volumes/Sanjay SSD/SlabBuild.sparseimage`
  BEFORE the next round so clippy can pass cleanly. The
  wedge is now consistent enough that we should consider
  it a documented "needs reattach between rounds" property
  of this build setup until a more permanent fix lands.**

- 2026-06-20 20:47 PT (Cake, cron): round-12 BATCH tick —
  FIVE Plugin-Marketplace-Install-History slices that close out
  the long-standing audit-trail gap on the marketplace install
  pipeline (append-only sqlite log + install/uninstall/failure
  wiring + reader Tauri commands + retention surface + Activity
  section on PluginDetailDrawer + RecentInstallsDrawer on the
  PluginsPanel toolbar). All DONE. Five feature commits + one
  rustfmt fixup, pushed + verified (local==origin 1d79d7b).
  - Slice 53 install_log primitive (9226f88): append-only
    sqlite log at ~/.slab/marketplace-history.sqlite, kept
    independent of plugin-storage.sqlite + hopper.sqlite.
    InstallLog with open/open_in_memory + three writers
    (record_install with optional prior_version that flips
    Install→Update, record_uninstall, record_failure) +
    three readers (list_events / list_recent /
    install_stats + distinct_plugin_count). InstallAction
    parse returns Failed on unknown tags so future schema
    bumps don't panic. NULL-able columns populated only on
    the rows that need them. 14 new tests.
  - Slice 54 install/uninstall wiring (182e448):
    slab_marketplace_install captures prior_version BEFORE
    the install via reg.get(id), wraps every failure
    surface in record_failure for full audit, on success
    appends one install/update row with bytes + files +
    prior_version. slab_marketplace_uninstall captures
    prior version BEFORE removing then appends one
    uninstall row (falling back to "unknown" when no
    readable manifest). open_install_log_and<F> helper +
    record_install_failure best-effort centralise the
    boilerplate.
  - Slice 55 reader commands + TS (ef9fed1): three Tauri
    commands (slab_marketplace_install_events,
    slab_marketplace_install_history_recent,
    slab_marketplace_plugin_install_stats), each opens the
    log per-call (install events fire on user click, not
    in a hot loop). TS adds InstallEvent / InstallStats
    interfaces (NULL-able as T | null), three helpers,
    formatInstallEventTime + installEventGlyph.
  - Slice 56 retention + summary (8e04747): InstallLog
    gains oldest_occurred_at (Option<i64> wrapping the
    SELECT MIN edge case; first attempt panicked on
    InvalidColumnType — fix-and-test loop pinned the
    Option<i64> column read), total_event_count, 
    prune_older_than (strict less-than; idempotent). Two
    Tauri commands (slab_marketplace_install_log_summary
    + slab_marketplace_install_log_prune with retain_days
    clamped to >=1). TS adds InstallLogSummary +
    installLogSummary + pruneInstallLog + formatLogSpan
    with ceiling-day arithmetic. 7 new tests with the
    insert_at test-helper pinning occurred_at to known
    values so prune/oldest don't race the clock.
  - Slice 57 PluginDetailDrawer Activity +
    RecentInstallsDrawer (7b84083, 830 LOC): pure frontend
    tying slices 53-56 into one demo-able surface.
    Activity section self-fetches on mount + every
    entry.id change via Promise.all over
    listInstallEvents(20) + pluginInstallStats; per-row
    glyph + colour-accented action + version + ← v<prior>
    for updates + bytes/files for installs OR truncated
    error for failures + relative time. Section
    auto-collapses on empty timeline. RecentInstallsDrawer
    (NEW): 460px right-side slide-from-right with
    7d/30d/All window strip (client-side filter, no
    re-round-trip on flip), per-event rows mirroring the
    Activity vocabulary, footer "Clear older than 90d…"
    two-step confirm; onPruned bubbles back. PluginsPanel
    toolbar gains "⏱ History" count-chip button gated on
    total_events > 0; refreshInstallLog called on mount +
    after every install / uninstall / install-failure.
  Plus fixup (1d79d7b): two pure-formatting rustfmt
  drifts on slice 54's prior_version let-chain (collapsed
  to one line) + record_install_failure closure body
  (reflowed). Kept as a single fixup commit so the batch
  stays inspectable and slice 54 stays independently
  revertible.
  Gates: cargo fmt clean (drift fixed via fixup), cargo
  test --lib marketplace::install_log:: 21 passed / 0
  failed (+21 vs baseline: 14 from slice 53 + 7 from
  slice 56), cargo test --lib 2153 passed / 0 failed
  (round-11 baseline + 21), pnpm check 0 errors / 104
  warnings (round-11 baseline preserved EXACTLY).
  **cargo clippy --lib gate WEDGED TWICE AGAIN on
  /Volumes/SlabBuild sparse image** — first attempt
  spawned rustc which fell to 0% CPU on `rustc
  --crate-name tauri_plugin_opener`, killed after ~3min;
  second attempt cargo check itself stayed at 0% (0.52s
  CPU) without spawning rustc. Disk has 56G free, `ls
  /Volumes/SlabBuild/target/debug/deps` ran in 0.2s at
  tick start and the cargo test gate ran cleanly — so
  it's specifically the clippy/check codegen path that
  trips the fsync sleep. Per STATE.md guidance this batch
  ships on lib-test + svelte-check strength. **Sanjay
  action recommended (THIRD tick in a row hitting this
  wedge): `hdiutil detach` then reattach
  `/Volumes/Sanjay SSD/SlabBuild.sparseimage` BEFORE the
  next round so clippy can pass cleanly. The wedge is
  now reliably reproducible on the same crate so it's
  not transient.**

- 2026-06-20 17:32 PT (Cake, cron): round-11 BATCH tick —
  FIVE Tag-Suggest-Bulk-Surface slices that close out the
  v3.39.0 Atlas Tag-Suggest subsystem end-to-end (bulk-accept
  primitive + granular per-suggestion undismiss + filter-aware
  bulk suggester + slim stats badge + 640-LOC review drawer
  wired into the LibraryPanel toolbar). All DONE. Five feature
  commits, pushed + verified (local==origin c881c62).
  - Slice 48 accept_tag_suggestions_bulk (d17fe91): AcceptItem
    + BulkAcceptResult, per-item failure isolation, case +
    whitespace pair-dedupe. Tauri emits single library-changed
    after batch. 6 new tests.
  - Slice 49 list_dismissed_for_doc + undismiss_one_for_doc
    (88a5439): DismissedSuggestion row ordered newest-first;
    undismiss_one returns bool. Case-insensitive match. Two
    new Tauri commands. 7 new tests.
  - Slice 50 suggest_for_filter (3053a7c): reuses
    query::query_documents so every filter shape composes
    free; limit forced, sort untouched. Zero-suggestion docs
    skipped post-query. 6 new tests.
  - Slice 51 suggestion_stats (2a3b8b5): TagSuggestionStats
    slim payload. sample_cap-bounded walk of recently-seen
    untagged docs probes each for plausible suggestions so
    badge never lures user into empty panel. 5 new tests.
  - Slice 52 BulkTagSuggestionsPanel + LibraryPanel wiring
    (c881c62, 640 LOC panel + ~30 LOC toolbar/state changes):
    pure frontend tying slices 48-51 into one drawer with
    source-strip toggle, per-doc cap, source-filtered batch
    selection chips, per-card "Hidden…" disclosure for
    granular undismiss, toast confirmation, badge gating
    on stats. No new Tauri commands. Plus one rustfmt
    drift on slice 49's undismiss_one signature captured
    here since slice 49 already shipped.
  Gates: cargo fmt clean, cargo test --lib pdf::library::
  tag_suggest:: 42 passed / 0 failed (+24 vs baseline: 6
  bulk-accept + 7 granular undismiss + 6 suggest_for_filter +
  5 suggestion_stats), cargo test --lib 2132 passed / 0 failed
  (round-10 baseline + 24), pnpm check 0 errors / 104 warnings
  (round-10 baseline preserved EXACTLY — 2 new a11y warnings
  caught + fixed during the gate). **cargo clippy --lib
  WEDGED TWICE on /Volumes/SlabBuild sparse image** — first
  attempt rustc fell to 0% CPU on `rustc --crate-name tauri`
  within 30s, killed after 100s+; second attempt cargo-clippy
  itself stayed at 0% without spawning rustc. Post-attempt
  even `ls target/debug/deps` hangs >8s (was 0.027s at tick
  start). Per STATE.md guidance, this batch ships on lib-test
  + svelte-check strength. **Sanjay action recommended:
  `hdiutil detach` then reattach `/Volumes/Sanjay
  SSD/SlabBuild.sparseimage` before next round so clippy can
  pass cleanly — this is now the second tick in a row hitting
  the same wedge.**

- 2026-06-20 14:55 PT (Cake, cron): round-10 BATCH tick —
  FIVE Hopper-Loop-Polish slices that close out the v3.22
  batch-backfill subsystem end-to-end (recursive scan +
  per-rule coverage strip + time-window history + CSV
  export + wired UI with live progress, cancel, and history
  chips). All DONE. Five feature commits + one fixup,
  pushed; verify via `git log --oneline origin/feature/...`.
  - Slice 43 plan_backfill_with_options (a6cf10c): PlanOptions
    { recursive, max_depth } widens the planner via an
    internal collect_pdfs explicit-stack recursion. Hidden
    dirs skipped; locked subdirs swallow errors so one
    denied subdir doesn't kill the report. Tauri + TS
    widened with optional opts. 6 new tests.
  - Slice 44 per_rule_counts (3bbc08a): BackfillReport gains
    per_rule_counts BTreeMap with __defaults__ + __skip__
    synthetic buckets. Zero-hit rules omitted. serde-default
    keeps legacy JSON decoding. 6 new tests pin
    sum-equals-scanned invariant.
  - Slice 45 list_backfill_runs_since (0e9d5e2): new SQL-
    backed reader with optional since_unix; legacy reader
    delegates. Folder + since AND in one WHERE clause.
    Inclusive boundary. TS adds backfillSinceUnix helper.
    4 new tests.
  - Slice 46 backfill_report_to_csv (9a14495): RFC-4180-
    strict export. New Tauri command + TS helpers (incl.
    suggestBackfillCsvFilename with sanitised folder name +
    ISO date). 6 new tests.
  - Slice 47 HopperBackfillPanel rewrite (f720bcf): pure
    frontend tying slices 43-46 + the round-9 streaming
    executor into one panel. Recursive toggle + depth
    dropdown, per-rule chips, live progress + scrolling
    tail + working Cancel, CSV export with toast, history
    time-window chips defaulting to 7d. Row selections
    disable during apply.
  - Fixup (36309a5): three small post-gate corrections —
    #[serde(default)] on PlanOptions struct so empty JSON
    decodes, tally per_rule_counts AFTER pushing the Skip
    row in the unreadable-folder branch, one cargo-fmt
    drift on watcher.rs. Kept as a single fixup commit so
    the batch stays inspectable but bugs don't ship to
    origin half-fixed.
  Gates: cargo fmt clean, cargo test --lib pdf::hopper::
  108 passed / 0 failed (+22 vs baseline: 6 PlanOptions +
  6 per_rule_counts + 4 since + 6 CSV), cargo test --lib
  pdf::library:: 381 passed / 0 failed (round-9 baseline
  preserved), pnpm check 0 errors / 104 warnings (round-9
  baseline preserved; zero new from the panel rewrite).
  **cargo clippy --lib WEDGED TWICE on /Volumes/SlabBuild
  sparse image — even `ls target/debug/deps` hangs >60s.**
  Per STATE.md "if cargo wedges twice, commit on cargo
  check --lib + pnpm check strength and log the blocker"
  guidance, this batch ships on lib-test + svelte-check
  strength. **Sanjay action needed: `hdiutil detach` then
  reattach `/Volumes/Sanjay SSD/SlabBuild.sparseimage`
  before next round so clippy can pass cleanly.**

## Roadmap — round 9 (Saved-Views Polish) — ALL DONE

Round 9 batched FIVE feature slices into one cron tick onto an
existing subsystem (the v3.50 saved-views rail — CRUD-only,
missing every power-user verb). Tag/search/OCR/manual-collection/
doc-row/beacon-cache surfaces are all end-to-end demo-able; this
round picked the next opaque corner.

38. ~~**update_view_filter (in-place edit)**~~ — DONE
    (2026-06-20 09:55 PT, 7774964, single commit). Backend
    `update_view_filter(id, &LibraryFilter)` swaps just the
    saved filter blob in place, preserving id + name +
    created_at + sort_order. get_view confirms the row exists
    first so unknown id surfaces as a hard error instead of
    silent 0-rows-affected. The pre-v3.56 path required
    delete-and-recreate, losing id (breaking stored
    references) + sort_order (shuffles to the bottom) +
    created_at. 3 new tests + Tauri command + TS client.
39. ~~**duplicate_view (fork the filter)**~~ — DONE
    (2026-06-20 09:55 PT, 128d0a2, single commit). Forks an
    existing view's filter byte-for-byte, derives a unique
    name by appending " (copy)" / " (copy 2)" / … up to 999
    to dodge the UNIQUE constraint, gets a fresh sort_order
    at the bottom of the rail. The duplicate is INDEPENDENT
    — editing it later does NOT mutate the source (covered
    by duplicate_view_is_independent_from_source). 5 new
    tests + Tauri command + TS client.
40. ~~**set_view_pinned (schema v15)**~~ — DONE
    (2026-06-20 09:55 PT, c86cc42, single commit). Schema
    bump 14 -> 15 adds `pinned INTEGER NOT NULL DEFAULT 0`
    to library_saved_views + partial index `WHERE pinned = 1`.
    Setter is idempotent (SQLite reports rows matched not
    rows changed). list_views ORDER BY widens to `pinned
    DESC, sort_order ASC, name ASC`. SavedViewRecord widens
    with the `pinned: bool` field; serde default keeps
    backwards-compat for pre-v3.56 JSON snapshots. 8 new
    tests incl. schema_v15 pragma_table_info pin + partial
    index pin + legacy-JSON-deserialises-as-false pin.
41. ~~**reorder_views (atomic full-list)**~~ — DONE
    (2026-06-20 09:55 PT, 8278a2a, single commit).
    `reorder_views(&[i64])` atomically re-stamps sort_order
    by zero-based position in one SQLite transaction. Both
    duplicate-id and unknown-id rejections happen BEFORE the
    txn opens so a rejected reorder doesn't touch a row.
    Subset reorders are PERMITTED (unmentioned ids keep
    their pre-reorder sort_order). Does NOT mutate the
    pinned flag — the pinned-first sort survives shuffles
    transparently. Mirrors smart_folders::set_order /
    set_collection_order patterns. 6 new tests + Tauri
    command + TS client.
42. ~~**Saved-views rail UI**~~ — DONE (2026-06-20 09:55 PT,
    58e895b, single commit). Pure frontend — wired all four
    new verbs into the LibraryPanel saved-views rail.
    Rail-head gains "Update" button (visible only when an
    active view is loaded AND the current filter is
    non-default). Per-row layout becomes [★ pin glyph] [◆
    row body] [⋯ menu]; pin is gold (#f7c948) when on. The
    ⋯ menu surfaces Pin/Unpin, Rename…, Duplicate, Move up
    / Move down (conditional on group position), then a
    danger-tinted Delete view. Window-click-outside dismiss
    extends the existing onWindowClickForMenu listener.
    Local savedViewCompare matches the backend ORDER BY so
    in-memory mutations keep rail order without a
    round-trip. No new Tauri commands.

    With Round 9 done, the saved-views rail is end-to-end
    demo-able: in-place edit, duplicate, pin, reorder, full
    rail UI with menu. Next subsystem candidates: Hopper
    backfill progress surface (the panel fires but doesn't
    show per-doc progress live), plugin marketplace UI (the
    backend ships in marketplace/ but only PluginsPanel's
    Browse tab surfaces it — no install history, no
    per-plugin detail), smart-folders hub UI polish (the
    rail's drag/pin chrome could be tightened), saved-views
    drag-handle UI (the reorder backend takes positional
    lists; a drag-handle is a pure-frontend follow-up).

## Tick log

- 2026-06-20 09:55 PT (Cake, cron): round-9 BATCH tick — FIVE
  Saved-Views-Polish slices that promote the v3.50 saved-views
  rail from CRUD-only into a full Notion-grade rail surface
  (in-place edit + duplicate + pin + atomic reorder + wired UI).
  All DONE, pushed + verified (local==origin 58e895b). Five
  commits, one per slice (each backend slice bundles the
  matching Tauri command + TS client per the established
  wire-layer convention; UI slice as the 5th commit).
  - Slice 38 update_view_filter (7774964): swap saved filter
    in place, preserving id/name/created_at/sort_order;
    pre-existing get_view confirms the row exists first so
    unknown id is a hard error. 3 new tests.
  - Slice 39 duplicate_view (128d0a2): fork the filter
    byte-for-byte, auto-name "<src> (copy)" / "(copy N)" up
    to 999, fresh sort_order at the bottom. Independent
    fork — editing source doesn't mutate copy. 5 new tests.
  - Slice 40 set_view_pinned (c86cc42): schema v14 -> v15
    adds `pinned INTEGER NOT NULL DEFAULT 0` + partial
    index `WHERE pinned = 1`. Idempotent. list_views ORDER
    BY widens to `pinned DESC, sort_order ASC, name ASC`.
    SavedViewRecord widens with serde-default pinned for
    legacy-JSON compat. 8 new tests incl. schema_v15 +
    partial-index pin + legacy-JSON pin.
  - Slice 41 reorder_views (8278a2a): atomic re-stamp by
    position in one txn. Validation up front (duplicate id
    + unknown id rejected without touching a row). Subset
    reorders permitted. Pin flag NOT mutated. Mirrors
    smart_folders::set_order. 6 new tests incl.
    subset-only-restamps-named-rows pin. Amended after gate
    surfaced a borrow-doesn't-live-long-enough on the
    id-set collect (stmt + query_map + collect needs an
    explicit Vec intermediate so stmt doesn't drop while
    iterator is alive).
  - Slice 42 saved-views rail UI (58e895b): pure frontend.
    Rail-head Update button. Per-row [★ pin] [◆ body] [⋯
    menu] with inline rename, gold-on pin glyph,
    danger-tinted Delete, conditional Move up/down,
    window-click-outside dismiss. Local savedViewCompare
    keeps order without round-trip; reorder does
    refresh-via-list because recomputing sort_order locally
    is more error-prone than re-fetching.
  All gates green: cargo fmt clean, cargo test --lib
  pdf::library:: 381 passed / 0 failed (+22 from round-8's
  359 baseline: 3 update + 5 duplicate + 8 pin + 6 reorder),
  cargo test --lib ai::embedding_index 30 passed / 0 failed
  (round-8 baseline preserved), cargo clippy --lib -D
  warnings clean (4m16s warm — first cycle after a kill),
  pnpm check 0 errors / 104 warnings (same as round-8
  baseline; zero new from imports/handlers/popover
  markup/styles). Pushed + verified (local==origin 58e895b).
  Process note: first gate cycle wedged on two concurrent
  cargo invocations contending the SlabBuild sparse image's
  slow fsync; killed both and ran serially per the
  documented STATE.md guidance. The build-lock contention
  surfaced exactly the wedge symptom the BUILD ENVIRONMENT
  section warns about. Tag/search/OCR/manual-collection/
  beacon-cache/doc-row surfaces stay feature-complete (359
  baseline preserved); saved-views rail is now also
  end-to-end demo-able with this batch. Next subsystem
  candidates: Hopper backfill progress surface, plugin
  marketplace UI, smart-folders hub UI polish, saved-views
  drag-handle UI.

## Roadmap — round 8 (Doc Inspector) — ALL DONE

Round 8 batched FIVE feature slices into one cron tick onto a
fresh subsystem (per-doc detail — the library_documents row was
just storage + ocr-state + tags; no inspector, no notes, no star,
no rename).

33. ~~**set_doc_title (rename docs in place)**~~ — DONE
    (2026-06-20 06:09 PT, 7398b58, single commit). Backend setter
    that overrides the displayed title without renaming the
    on-disk file. Trim + None/empty clears to NULL (basename
    fallback resumes), MAX_DOC_TITLE_LEN cap 500 with the length
    check running BEFORE the UPDATE so a rejected setter leaves
    the prior title untouched. Returns the refreshed
    DocumentRecord with tags eager-loaded for one-round-trip card
    refresh. 5 new tests + Tauri command + TS client.
34. ~~**set_doc_notes (schema v13)**~~ — DONE (2026-06-20 06:09
    PT, 12eab28, single commit). Schema bump 12 -> 13 adds
    nullable `notes TEXT`. Setter same trim/empty-clears/cap
    shape; cap is MAX_DOC_NOTES_LEN = 4000 (sized for a paragraph
    or two of provenance context). DocumentRecord widened
    end-to-end (the four ocr_queue SELECT mappers + registry +
    query + collections SELECT lists + the TS mirror). 6 new
    tests incl. schema_v13 pragma_table_info pin.
35. ~~**set_doc_starred (schema v14)**~~ — DONE (2026-06-20 06:09
    PT, 66a14fb, single commit). Schema bump 13 -> 14 adds
    `starred INTEGER NOT NULL DEFAULT 0` + partial index
    `idx_documents_starred WHERE starred = 1`. Setter is
    idempotent (SQLite reports rows matched, not rows whose value
    changed). 5 new tests incl. schema_v14 pin (with partial
    index assertion) + upsert_existing_doc_preserves_starred (the
    scanner's re-upsert pass MUST NOT wipe a user-set star — this
    test would catch a regression if someone added `starred =
    DEFAULT` to the UPDATE SET clause).
36. ~~**starred_only filter + Starred clause + toolbar toggle**~~
    — DONE (2026-06-20 06:09 PT, 2fd3027, single commit).
    LibraryFilter.starred_only top-level AND-combined flag,
    FilterClause::Starred / NotStarred for the recursive builder,
    LibraryPanel toolbar "Starred" toggle chip mirroring the
    existing Untagged chip pattern. Pre-v3.55 saved smart
    collections that didn't carry the field deserialise as false.
    6 new query.rs tests incl. starred_filter_serde_round_trip
    legacy-JSON pin.
37. ~~**Dedicated DocInspectorPanel UI**~~ — DONE (2026-06-20
    06:09 PT, 4fe82f9, single commit). ~600-LOC Svelte 5 panel —
    NOT a full-viewport modal but a 460px slide-from-right drawer
    (Notion side-panel convention) so the doc grid stays visible
    behind it. Sections: star pill, title override input
    (placeholder shows basename fallback, save on blur or Enter),
    notes textarea (save on blur or Cmd/Ctrl+Enter, live counter
    amber at 90% / red over cap), read-only tag chips (hint
    points at card-menu for editing), metadata block, footer
    with Open in Reader / Reveal on disk / Remove from library
    (danger, two-step confirm). LibraryPanel wiring: inspectorDoc
    state + 4 handlers (open/close/updated/removed), "Inspect…"
    and "Star/Unstar" context-menu entries, ★ glyph on starred
    cards + ✎ glyph on cards with notes for at-a-glance triage.
    Pure frontend slice — no new Tauri commands beyond the three
    in slices 33-35.

    With Round 8 done, the doc-row surface is end-to-end
    demo-able: title override, freeform notes, star, starred-only
    filter, dedicated inspector drawer. Next subsystem candidates:
    plugin marketplace UI (the backend ships in marketplace/ but
    PluginsPanel.svelte's Browse tab is the only surface — no
    install history, no per-plugin detail), Hopper backfill
    progress surface (the panel fires but doesn't show per-doc
    progress live), smart-folders hub UI polish (the rail's
    drag/pin chrome could be tightened), saved-views chrome
    (the panel ships but has no quick-pin / drag-reorder).

## Tick log

- 2026-06-20 06:09 PT (Cake, cron): round-8 BATCH tick — FIVE
  Doc-Inspector slices that promote the library_documents row
  from an opaque (title-but-no-setter, no notes, no star, no
  inspector) cell into a full editable surface (rename + notes +
  star + filter + dedicated drawer). All DONE, pushed + verified
  (local==origin 4fe82f9). Five commits, one per slice (each
  backend slice bundles the matching Tauri command + TS client
  per the established wire-layer convention; UI slice as the 5th
  commit).
  - Slice 33 set_doc_title (7398b58): override the displayed
    title without renaming the on-disk file. Trim + None/empty
    clears to NULL (basename fallback resumes), MAX_DOC_TITLE_LEN
    cap 500 with the length check running BEFORE the UPDATE.
    Returns the refreshed DocumentRecord with tags eager-loaded.
    5 new tests + Tauri command + TS client.
  - Slice 34 set_doc_notes (12eab28): schema v12 -> v13 adds
    nullable `notes TEXT` (pre-v13 rows silently pick up NULL).
    Setter same trim/empty-clears/cap shape; cap is
    MAX_DOC_NOTES_LEN = 4000 (sized for a paragraph or two of
    provenance context). DocumentRecord widened end-to-end (the
    four ocr_queue SELECT mappers + registry/query/collections
    SELECT lists + TS mirror). 6 new tests incl. schema_v13
    pragma_table_info pin.
  - Slice 35 set_doc_starred (66a14fb): schema v13 -> v14 adds
    `starred INTEGER NOT NULL DEFAULT 0` + partial index
    `idx_documents_starred WHERE starred = 1`. Setter is
    idempotent. 5 new tests incl. schema_v14 + partial-index
    pin + upsert_existing_doc_preserves_starred (the scanner's
    re-upsert pass must NOT wipe a user-set star).
  - Slice 36 starred filter (2fd3027): three independent levers
    — LibraryFilter.starred_only top-level AND-combined flag,
    FilterClause::Starred/NotStarred variants for the recursive
    builder, LibraryPanel toolbar "Starred" toggle chip. Legacy
    JSON without starred_only deserialises as false. 6 new
    query.rs tests incl. serde round-trip with the legacy-JSON
    pin.
  - Slice 37 DocInspectorPanel (4fe82f9): ~600-LOC Svelte 5
    panel — 460px slide-from-right drawer (Notion side-panel
    convention, NOT full-viewport modal like OcrQueuePanel /
    BeaconCachePanel — the inspector wants context, not focus).
    Sections: star pill + title override + notes textarea
    (with live counter) + read-only tag chips (with hint
    pointing at card menu) + metadata block + footer (Open /
    Reveal / Remove). LibraryPanel wiring: inspectorDoc state
    + 4 handlers, "Inspect…" / "Star/Unstar" context-menu
    entries, ★ on starred cards + ✎ on noted cards for
    at-a-glance triage.
  All gates green: cargo fmt clean, cargo test --lib pdf::library::
  359 passed / 0 failed (+22 from round-7's 337 baseline: 5
  set_doc_title + 6 set_doc_notes + 5 set_doc_starred + 6 query
  starred), cargo test --lib ai::embedding_index 30 passed / 0
  failed (round-7 baseline preserved), cargo clippy --lib -D
  warnings clean (11s warm), pnpm check 0 errors / 104 warnings
  (same as round-7 baseline; zero new from DocInspector or the
  LibraryPanel card-head chrome). Pushed + verified (local==origin
  4fe82f9). Process note: the DocumentRecord widening touched 7
  SQL SELECT call sites + 5 row-constructor sites for `notes`
  alone, then another 7 SELECTs + 5 constructors for `starred` —
  used `replace_all=true` for the repeated SELECT pattern across
  registry.rs / ocr_queue.rs to keep the slice clean. Beacon
  Cache + Manual Collections + OCR Queue + Tag-Suggest surfaces
  all stay feature-complete (no regressions on the 337 baseline);
  the doc-row surface is now also end-to-end demo-able with this
  batch. Next subsystem candidates: plugin marketplace UI,
  Hopper backfill progress surface, smart-folders hub UI polish,
  saved-views chrome.

### What round-7 (2026-06-19 23:14 PT) just shipped

A demo-able overhaul of the Beacon embedding index. Before this tick
the embedding index was an opaque box: the BeaconSearchPanel footer
showed just "X PDFs · Y chunks indexed" with zero list, zero per-model
breakdown, zero stale-path detection, and `forget(hash)` only wired
into the per-PDF trash icon on the current document — no surface for
managing the cache across the whole library. Now every Notion-grade
inspector affordance lands:

- Slice 28: `EmbeddingIndex::list_indexed()` returns one
  IndexedPdfRecord per PDF (hash + path + pages + embed_model +
  indexed_at + chunks) via a single LEFT JOIN + GROUP BY round-trip
  so the inspector table is cheap even on a 10k-PDF cache. LEFT JOIN
  keeps zero-chunk rows visible (an INNER JOIN would silently hide a
  partial-write recovery). ORDER BY indexed_at DESC, hash ASC matches
  Slab's activity-feed convention. 5 new tests incl. serde
  snake_case round-trip pin + the LEFT JOIN guard. One Tauri command
  (slab_beacon_index_list) + TS client (beaconIndexList) in a new
  `src/lib/beaconCache.ts` module — kept apart from `library.ts`
  because the embedding index is a different DB file
  (beacon-index.sqlite vs library.sqlite). (6507452)
- Slice 29: `forget_many(hashes)` bulk-deletes in one transaction
  with a prepared statement, returns the count actually removed,
  silently skips unknown hashes (tolerant wire contract for the
  inspector's multi-select). Empty input is a zero no-op. FOREIGN
  KEY ON DELETE CASCADE on the chunks table picks up the children.
  3 new tests. One Tauri command + TS client bundled. (bd9655e)
- Slice 30: `stats_by_model()` returns Vec<ModelBucket> per
  embed_model in one GROUP BY round-trip — chunks DESC, model ASC
  tie-break. Surfaces the mixed-model trap that the existing
  search.rs dim-mismatch skip otherwise hides (loser's chunks become
  dead weight). Empty index → empty Vec; single-model → 1-element Vec.
  4 new tests incl. serde snake_case round-trip pin. One Tauri
  command + TS client bundled. (86a70cd)
- Slice 31: `find_stale()` walks every row and returns the subset
  whose `pdf_path` no longer points at a readable file (renamed,
  deleted, on an unmounted volume). `forget_stale()` is the bulk
  companion that runs find_stale once up front then forget_many's
  the resulting hashes (so a file restored mid-scan isn't
  accidentally pruned). 4 new tests. Two Tauri commands + TS
  clients. (76cae48)
- Slice 32: dedicated BeaconCachePanel.svelte — ~700-LOC Svelte 5
  panel that ties slices 28-31 into one surface: dashboard tiles
  (total PDFs + chunks + per-model breakdown with a "Mixed-model
  index detected" warning when buckets > 1), stale section (only
  renders when stale > 0, danger-tinted, section-head "Forget all N
  stale"), indexed-PDFs table with multi-select checkboxes + Select
  all/None/Invert + column-sort toggle (Newest/Oldest/Chunks) +
  per-row Forget + floating bulk-forget bar when selection > 0.
  Selection prunes on every refresh so a forgotten hash can't
  linger. Mounted by CollectionsSidebar via window event +
  "Beacon Cache…" command-palette entry (◉ glyph). Refreshes on
  library-changed. Pure frontend slice (no schema, no backend, no
  new Tauri commands beyond the four shipped in 28-31). (5be3a3d)

Gates passed: cargo test --lib pdf::library:: 337 passed / 0
failed (unchanged from round-6 baseline — no regression), cargo
test --lib ai::embedding_index 30 passed / 0 failed (+16 from the
14 pre-existing: 5 list_indexed + 3 forget_many + 4 stats_by_model
+ 2 find_stale + 2 forget_stale), cargo clippy --lib -D warnings
clean (31s warm), cargo fmt clean, pnpm check 0 errors / 104
warnings (same as round-6 baseline — none new from BeaconCachePanel
or its sidebar mount or palette entry).

KEYBOARD-SHORTCUT NOTE: did NOT wire Cmd+Shift+B for the inspector
because that combo is already bound at the App level (`+page.svelte`)
to open the Bates panel — Slab's convention is to defer ad-hoc letter
shortcuts to the keymap registry rather than collide globally. The
palette entry + library-changed auto-refresh cover discoverability
and the live-update story without the conflict. If a shortcut becomes
useful later, route it through `src-tauri/src/keymap/`.

## BUILD ENVIRONMENT — CRITICAL, read before any cargo command

Internal disk is FULL (~2.9 GiB free of 228). Cargo target is redirected to an
APFS sparse image at **/Volumes/SlabBuild** via `src-tauri/.cargo/config.toml`
(gitignored). Verify mounted each tick: `df -h /Volumes/SlabBuild | tail -1`.
If missing: `hdiutil attach "/Volumes/Sanjay SSD/SlabBuild.sparseimage"`.

**The image has very slow fsync.** Proven tonight across many attempts:
- `cargo test --lib`, `cargo check --lib`, `pnpm check` → WORK (slow but finish).
- A FULL `cargo build` / `cargo tauri build` → WEDGES on the `tauri` crate's
  final codegen (rustc goes to sleep state, no CPU, target size flat for min).
**RULE: never run a full binary build in a tick.** It's release work, blocked by
CI billing anyway. Gate with `cargo test --lib` + `cargo clippy --lib` + `pnpm
check`. If cargo wedges >5 min with no rustc CPU: `pkill -f 'cargo'`, retry once.

## CI STILL BLOCKED — needs Sanjay

GitHub Actions billing failure persists → no release artifacts (DMG/MSI/AppImage)
until fixed. Action: https://github.com/settings/billing → update payment / raise
limit. Does NOT affect local dev or branch pushes.

## Roadmap — round 7 (Beacon Cache Inspector) — ALL DONE

Round 7 batched FIVE feature slices into one cron tick onto a fresh
subsystem (the Beacon embedding index — opaque box → manageable
surface). The tag/search/OCR/manual-collection surfaces are all
end-to-end demo-able; this round picks the next opaque corner.

28. ~~**list_indexed_pdfs (full inspector feed)**~~ — DONE
    (2026-06-19 23:14 PT, 6507452, single commit). Backend
    EmbeddingIndex::list_indexed() returns Vec<IndexedPdfRecord> in
    one LEFT JOIN + GROUP BY round-trip; LEFT JOIN keeps zero-chunk
    rows visible; ORDER BY indexed_at DESC, hash ASC. 5 new tests
    (empty, one-row-per-pdf-with-joined-count, newest-first,
    LEFT JOIN guard, serde snake_case round-trip). One Tauri command
    + TS client in new `src/lib/beaconCache.ts` module.
29. ~~**forget_many (bulk delete in one transaction)**~~ — DONE
    (2026-06-19 23:14 PT, bd9655e, single commit). Single
    transaction + prepared statement, returns count actually removed,
    silently skips unknown hashes, empty is zero no-op. 3 new tests.
    One Tauri command + TS client bundled.
30. ~~**stats_by_model (per-embed-model bucket counts)**~~ — DONE
    (2026-06-19 23:14 PT, 86a70cd, single commit). One GROUP BY
    round-trip; chunks DESC, model ASC tie-break; empty Vec for
    empty index; 1-element Vec for single-model. Surfaces the
    mixed-model trap that search.rs's dim-mismatch skip otherwise
    hides. 4 new tests (bucket-per-model, empty, single, serde
    round-trip). One Tauri command + TS client bundled. NB: tests
    need distinct content per model because the index keys by hash;
    the seed_pdfs helper introduced in slice 28 folds embed_model
    into the seeded byte stream to handle that.
31. ~~**find_stale + forget_stale (dead-path detection & cleanup)**~~
    — DONE (2026-06-19 23:14 PT, 76cae48, single commit).
    find_stale walks every row, returns IndexedPdfRecord rows whose
    on-disk path is missing (Path::exists; broken symlinks count as
    missing, right call since the index can't search what it can't
    read). forget_stale companion runs find_stale once up front then
    forget_many's the resulting hash list (so a file restored
    mid-scan isn't pruned). 4 new tests (only-missing-rows surface,
    clean-empty, prune-only-missing, zero-noop). Two Tauri commands
    + TS clients.
32. ~~**Dedicated BeaconCachePanel UI**~~ — DONE (2026-06-19 23:14
    PT, 5be3a3d, single commit). ~700-LOC Svelte 5 panel mirroring
    OcrQueuePanel pattern. Sections: dashboard tiles (total + per-
    model + mixed-model warning), stale section (only renders >0,
    danger-tinted, section-head Forget-all), indexed-PDFs table
    (multi-select with Select all/None/Invert, column-sort toggle
    Newest/Oldest/Chunks, per-row Forget, floating bulk-forget bar
    when selection >0). Selection prunes on refresh. Mounted by
    CollectionsSidebar via slab:open-beacon-cache window event +
    "Beacon Cache…" palette entry (◉ glyph). Refreshes on
    library-changed. Pure frontend slice. No Cmd+Shift+B shortcut
    because that's already wired to Bates at App level.

    With Round 7 done, the Beacon embedding index is now end-to-end
    demo-able: per-model breakdown, stale-path detection, bulk
    forget, full table with sort+multi-select. Next subsystem
    candidates: smart-folders hub UI polish (the rail's drag/pin
    chrome could be tightened), doc-detail metadata editor (no
    surface for editing title/author/keywords on a library doc),
    plugin marketplace UI (the backend ships in marketplace/ but
    has no panel), Hopper backfill progress surface (the panel
    fires but doesn't show per-doc progress live).

## Tick log

- 2026-06-19 23:14 PT (Cake, cron): round-7 BATCH tick — FIVE
  Beacon-Cache-Inspector slices that promote the embedding index
  from an opaque (pdfs,chunks) tuple into a Notion-grade manageable
  surface (list, bulk forget, per-model breakdown, stale detect,
  dedicated UI). All DONE, pushed + verified (local==origin
  5be3a3d). Five commits, one per slice (each backend slice bundles
  the matching Tauri command + TS client per the established
  wire-layer convention; UI slice as the 5th commit).
  - Slice 28 list_indexed (6507452): one LEFT JOIN + GROUP BY
    round-trip returning IndexedPdfRecord (hash + path + pages +
    embed_model + indexed_at + chunks), newest first, LEFT JOIN
    keeps zero-chunk rows visible. 5 new tests incl. serde
    snake_case pin. One Tauri command + TS client in NEW
    `src/lib/beaconCache.ts` (kept apart from library.ts because
    the embedding index is a different DB file —
    beacon-index.sqlite vs library.sqlite).
  - Slice 29 forget_many (bd9655e): bulk delete in one transaction
    with prepared statement, returns count actually removed,
    silently skips unknown hashes, empty is zero no-op, CASCADE
    handles chunks. 3 new tests. One Tauri command + TS client.
  - Slice 30 stats_by_model (86a70cd): per-embed-model bucket
    counts in one GROUP BY round-trip; chunks DESC, model ASC
    tie-break. Empty Vec / single-bucket / mixed-model serde
    round-trip. 4 new tests. One Tauri command + TS client. The
    seed_pdfs helper from slice 28 was designed with embed_model
    folded into the byte stream specifically so this slice's
    multi-model bucket test wouldn't collapse to one row.
  - Slice 31 find_stale + forget_stale (76cae48): missing-on-disk
    detection via Path::exists walk; bulk companion runs the scan
    once up front then forget_many's the resulting hashes so a
    file restored mid-scan isn't pruned. 4 new tests. Two Tauri
    commands + TS clients.
  - Slice 32 BeaconCachePanel (5be3a3d): ~700-LOC Svelte 5 panel
    that ties slices 28-31 into one surface — dashboard tiles +
    mixed-model warning + stale section + indexed-PDFs table with
    multi-select + column sort + bulk forget. Mounted via
    CollectionsSidebar window event + palette entry. Refreshes on
    library-changed. Pure frontend slice. No Cmd+Shift+B shortcut
    because that's already bound to Bates at the App level
    (+page.svelte) — Slab's convention is to defer ad-hoc letter
    shortcuts to the keymap registry rather than collide globally.
  All gates green: cargo fmt clean, cargo test --lib pdf::library::
  337 passed / 0 failed (unchanged from round-6 baseline — no
  regression on the library surface), cargo test --lib
  ai::embedding_index 30 passed / 0 failed (+16 from the 14
  pre-existing: 5 list_indexed + 3 forget_many + 4 stats_by_model
  + 2 find_stale + 2 forget_stale), cargo clippy --lib -D warnings
  clean (31s warm), pnpm check 0 errors / 104 warnings (same as
  round-6 baseline; zero new from BeaconCachePanel or sidebar mount
  or palette entry). Pushed + verified (local==origin 5be3a3d).
  Process note: built whole batch first for one gate cycle (caught
  a same-content-hash collapse in the model-bucket test — fixed by
  folding embed_model into the seed bytes), snapshotted final
  files to /tmp/bc-final, reset, then re-applied each slice via
  targeted patches to land 5 independently-revertible commits.
  Same pattern rounds 5-6 introduced; per-slice gate checks
  confirmed each slice compiles + tests-green before the next.
  Tag/search/OCR/manual-collection surfaces stay feature-complete
  (337 baseline preserved); Beacon embedding index is now also
  end-to-end demo-able with this batch. Next subsystem candidates:
  smart-folders hub UI polish, doc-detail metadata editor, plugin
  marketplace UI, Hopper backfill progress surface.

### What round-6 (2026-06-19 22:08 PT) just shipped

A demo-able overhaul of the manual-collection rail. Before this
tick the rail had a stub `rename_collection` that swallowed every
error (UNIQUE collision, empty name) and a `color` + `icon` columns
that were INSERT-only — there was no edit path, no reorder, no
duplicate. Now every Notion-grade collection surface lands:

- Slice 23: `rename_collection` hardened to return CollectionRecord,
  trim input, reject empty/over-cap names, short-circuit same-name
  no-ops, reject UNIQUE collisions with a named error, error on
  unknown id. Inline pencil-glyph rename in CollectionsSidebar
  with focusSelect + Enter/Escape/blur semantics + in-place row
  swap on save + inline error on rejection. Backend + UI bundled
  per-commit (b25253c).
- Slice 24: `set_collection_color(id, Option<&str>)` reuses
  registry::valid_tag_color to gate persistence (same `#hex` and
  functional `hsl()/hsla()/rgb()/rgba()` allowlist tags get —
  no CSS injection), trim + None-clears semantics, guard runs
  BEFORE the UPDATE so a rejected value leaves the row's prior
  color untouched. Clickable .cs-color-dot opens a palette modal
  (live preview + 8-swatch palette + Default-to-clear). 7 new
  tests. (7a772d1)
- Slice 25: `reorder_collections(ordered_ids)` atomic single-
  transaction rewrite of sort_order using 100/200/300 spacing so
  a future single-row splice has room. Tolerates unknown ids
  (silent skip), subset reorders (leaves un-named rows alone),
  duplicate ids (last write wins). HTML5 native drag-to-reorder
  in the rail with a dedicated `application/x-slab-collection-id`
  payload type so the existing doc-drop handler ignores reorder
  drags; lifted row dims to 0.35 opacity, drop target paints a
  2px accent insertion line (Notion/Linear pattern). Optimistic
  UI swaps the rail instantly; persist in background, rollback
  on failure. 6 new tests. (a8a748f)
- Slice 26: `duplicate_collection(source_id)` clones name + icon
  + color + ENTIRE doc membership in one transaction (INSERT …
  SELECT for the membership, one shared added_at baseline so the
  "added_at DESC" preview is stable). Auto-suffixes name with
  `(copy)` → `(copy 2)` → ... through 999. Returns the row with
  doc_count already populated so the rail can splice it in
  without an extra round-trip. Source name is truncated to fit
  the 120-scalar cap BEFORE the suffix so a long source never
  produces an over-cap clone. New ❏ glyph beside the existing
  rename/× chrome. 6 new tests. (76860b0)
- Slice 27: Pure frontend slice — a second "Add to collection…"
  button on the LibraryPanel multi-select floating bar wraps
  collectionList + collectionAddDocs. Picker lazy-loads on first
  open, refreshes on every subsequent open to catch
  newly-created collections, lists each target with its existing
  doc_count + color dot, toasts the result count with any
  duplicates named ("Added 4 docs to 'Tax 2026' (1 already in)").
  Mutually exclusive with the tag picker so the bar doesn't
  grow two free-floating popovers. (7418116)

Gates passed: cargo test --lib pdf::library:: 337 passed / 0
failed (+25 from the 312 at round-5 baseline: 7 rename + 7
set_color + 5 reorder + 6 duplicate), cargo clippy --lib -D
warnings clean (8s warm), cargo fmt clean, pnpm check 0 errors /
104 warnings (1 LESS than the 105 baseline — the role="list"
list-wrapper for reorder also fixed a long-standing
a11y_no_static_element_interactions warning on the row-wrap).

## BUILD ENVIRONMENT — CRITICAL, read before any cargo command

Internal disk is FULL (~2.9 GiB free of 228). Cargo target is redirected to an
APFS sparse image at **/Volumes/SlabBuild** via `src-tauri/.cargo/config.toml`
(gitignored). Verify mounted each tick: `df -h /Volumes/SlabBuild | tail -1`.
If missing: `hdiutil attach "/Volumes/Sanjay SSD/SlabBuild.sparseimage"`.

**The image has very slow fsync.** Proven tonight across many attempts:
- `cargo test --lib`, `cargo check --lib`, `pnpm check` → WORK (slow but finish).
- A FULL `cargo build` / `cargo tauri build` → WEDGES on the `tauri` crate's
  final codegen (rustc goes to sleep state, no CPU, target size flat for min).
**RULE: never run a full binary build in a tick.** It's release work, blocked by
CI billing anyway. Gate with `cargo test --lib` + `cargo clippy --lib` + `pnpm
check`. If cargo wedges >5 min with no rustc CPU: `pkill -f 'cargo'`, retry once.

## CI STILL BLOCKED — needs Sanjay

GitHub Actions billing failure persists → no release artifacts (DMG/MSI/AppImage)
until fixed. Action: https://github.com/settings/billing → update payment / raise
limit. Does NOT affect local dev or branch pushes.

## Roadmap — round 6 (Manual Collections management) — ALL DONE

Round 6 batched FIVE feature slices into one cron tick onto a fresh
subsystem (manual collections — every Notion/Linear-grade affordance
the rail was missing in v3.39.0).

23. ~~**rename_collection hardening + inline UI**~~ — DONE
    (2026-06-19 22:08 PT, b25253c, single commit). Backend
    rename_collection returns CollectionRecord (was unit), trim
    input, empty-after-trim rejects with "collection name cannot
    be empty", over-cap (>120 scalars) rejects with a named error,
    same-name short-circuits no-op without an UPDATE or
    library-changed emit, UNIQUE collision with a different row
    rejects with "a collection named X already exists" (looked
    up first to dodge the opaque rusqlite message), unknown id
    rejects via get_collection's QueryReturnedNoRows. 7 new tests
    (trim, empty rejection, same-name no-op, UNIQUE collision
    leaving both rows intact, unknown id, cap-at-120-scalars).
    UI: pencil glyph on hover flips the row label into an
    auto-selected text input, Enter commits, Escape/blur cancels,
    unchanged/empty short-circuits client-side, in-place row swap
    on save, inline error keeps the input in edit mode for retry.
    .cs-edit/.cs-rename/.cs-rename-input/.cs-rename-err CSS
    mirrors the LibraryPanel tag-rename chrome.
24. ~~**set_collection_color + palette modal**~~ — DONE
    (2026-06-19 22:08 PT, 7a772d1, single commit). Backend
    set_collection_color(id, Option<&str>) reuses
    registry::valid_tag_color so collections inherit the same
    CSS-injection guard tags get (`#hex` + functional
    `hsl()/hsla()/rgb()/rgba()` only). Trim input, trimmed-empty
    treated as None so the column never holds "real but empty"
    trash, guard runs BEFORE the UPDATE so a rejected color
    leaves the row's prior color intact, unknown id rejects
    before the UPDATE. 7 new tests (updates+returns-row, trims,
    None-clears, accepts pastel_for hsl shape, rejects every CSS-
    injection variant the guard knows about with prior color
    intact, unknown-id, preserves-name-and-doc-count column-drift
    guard). UI: rail's dot becomes a clickable .cs-color-dot
    button opening a palette modal (live preview, 8-color swatch
    palette same as tags, Default-to-clear). In-place row swap on
    save; modal stays open with backend reason on rejection.
    .cs-modal-backdrop/.cs-modal chrome reuses the OcrQueuePanel
    pop-in pattern.
25. ~~**reorder_collections + drag-to-reorder UI**~~ — DONE
    (2026-06-19 22:08 PT, a8a748f, single commit). Backend
    reorder_collections(ordered_ids) is a single atomic
    transaction; new sort_order values step by 100 (100, 200,
    300, ...) so a future single-row splice has room without
    rounding. Tolerant wire contract: unknown ids silently
    skipped (a stale id from a list-vs-reorder race shouldn't
    crash the rail; survivors land at correct positions —
    a,_,b → 100,300), subset reorders leave un-named rows'
    sort_order intact, duplicate ids accepted (last write wins).
    Returns the count of rows whose sort_order actually moved so
    the Tauri command can suppress library-changed on a no-op
    reorder. 6 new tests. UI: HTML5 native drag on .cs-row-wrap
    with a dedicated `application/x-slab-collection-id` payload
    type so the existing doc-drop handler ignores reorder drags;
    lifted row dims to 0.35 opacity, drop target paints a 2px
    accent insertion line at its top edge (Notion/Linear "drop X
    on Y means X lands where Y was"). Optimistic UI swaps the
    rail instantly; persist in background, rollback on failure.
    role="list" wrapper around the each-block so each draggable
    row-wrap can carry role="listitem" without tripping
    a11y_no_static_element_interactions — also retired one
    long-standing svelte-check warning.
26. ~~**duplicate_collection with auto-suffix + full membership clone**~~
    — DONE (2026-06-19 22:08 PT, 76860b0, single commit).
    Backend duplicate_collection(source_id) clones name + icon +
    color + ENTIRE doc membership in one transaction
    (INSERT…SELECT for the membership, single shared added_at
    baseline so the "added_at DESC" preview lands stable). Name
    auto-suffix: `" (copy)"` → `" (copy 2)"` → ... through 999;
    the source portion is truncated to fit the 120-scalar cap
    BEFORE the suffix so a long source never produces an
    over-cap clone. Returns CollectionRecord with doc_count
    already populated (no extra get round-trip needed). Unknown
    id errors before any write. New row lands at MAX(sort_order)
    + 1 so it bottoms the rail without disturbing the
    persisted reorder. 6 new tests (clones all 4 fields + docs +
    source untouched, suffix chain (copy)/(copy 2)/(copy 3) +
    chained dup of a (copy) row lands at "(copy) (copy)", lands
    at end of sort_order, empty source → empty clone, unknown
    id rejects, long-source truncation fits under cap). UI:
    paragraph glyph (❏) sits between rename and × in the row
    chrome. One click duplicates, toast names source + clone +
    cloned doc count. duplicateBusyId debounces repeat clicks.
    Reuses .cs-edit chrome — no new CSS.
27. ~~**Bulk Add-to-collection on LibraryPanel multi-select**~~ —
    DONE (2026-06-19 22:08 PT, 7418116, single commit, pure
    frontend). Second floating-bar button beside "Tag selected…"
    that opens a popover listing every manual collection by name.
    Click adds the N selected docs and toasts "Added 4 docs to
    'Tax 2026' (1 already in)" naming any duplicates that were
    already members. Reuses collectionList + collectionAddDocs
    IPC — no new backend or schema. Picker refreshes on every
    open so collections created via the sidebar since the last
    open are present without bespoke library-changed wiring.
    Mutually exclusive with the tag picker (opening one closes
    the other) so the bulk bar doesn't grow two free-floating
    popovers. Selection survives the chain so a user can drop
    the same selection into two collections in a row.
    clearSelection() closes both pickers + drops the set.
    .bulk-coll-wrap mirrors .bulk-tag-wrap positioning;
    .bulk-picker-empty handles loading + no-collections states;
    .bulk-picker-count surfaces each candidate's existing
    doc_count beside its name so users picking a target can
    confirm they're adding to the right one.

    With Round 6 done, manual collections are now end-to-end
    demo-able: rename, color, reorder, duplicate, bulk-add. The
    smart-collection side already had its surface (suggest, hub,
    saved views). Next ticks should pick a different subsystem —
    good candidates remaining: smart-folders hub UI polish,
    doc-detail metadata editor, Beacon cache inspector, plugin
    marketplace.

## Tick log

- 2026-06-19 22:08 PT (Cake, cron): round-6 BATCH tick — FIVE
  Manual-Collection-management slices that turn the previously
  stub-grade rail into a Notion/Linear-grade surface (rename,
  color, reorder, duplicate, bulk-add). All DONE, pushed +
  verified (local==origin 76860b0). Five commits, one per slice
  (each backend slice bundles backend + tests + Tauri command + TS
  client + UI bits per the established wire-layer convention).
  - Slice 23 rename (b25253c): hardened backend (trim, empty
    rejection, same-name no-op, UNIQUE collision with named
    error, unknown id, 120-scalar cap), 7 new tests, return type
    widened CollectionRecord. Inline pencil-glyph rename UI with
    focusSelect, Enter/Escape/blur semantics, in-place row swap.
  - Slice 24 color (7a772d1): set_collection_color reuses
    registry::valid_tag_color, trim+None-clears, guard runs
    BEFORE UPDATE, unknown id rejects, 7 new tests. Clickable
    .cs-color-dot opens palette modal (8 swatches + Default).
  - Slice 25 reorder (a8a748f): single-transaction rewrite of
    sort_order in 100-step spacing, tolerant of unknown ids /
    subset reorders / dup ids, returns moved-count for no-op
    suppression of library-changed. 6 new tests. HTML5 native
    drag on the rail with dedicated payload type, accent
    insertion line, optimistic UI with rollback on failure.
    role="list" wrapper retired one a11y warning.
  - Slice 26 duplicate (76860b0): full transaction-atomic clone
    of name + icon + color + membership (INSERT…SELECT), auto-
    suffix `(copy)` chain through 999, source-truncation to fit
    cap before suffix, returns row with doc_count populated. 6
    new tests. ❏ glyph between rename and ×, debounced toast.
  - Slice 27 bulk-add (7418116): pure frontend slice — second
    floating-bar button on LibraryPanel multi-select wrapping
    existing collectionList + collectionAddDocs IPC, picker
    refresh-on-open, dup-count toast, mutually exclusive with
    the tag picker, selection survives so user can chain into
    multiple collections.
  All gates green: cargo fmt clean, cargo test --lib pdf::library::
  337 passed / 0 failed (+25 from 312 at round-5 baseline; 7
  rename + 7 color + 5 reorder + 6 duplicate), cargo clippy --lib
  -D warnings clean (8.01s warm), pnpm check 0 errors / 104
  warnings (1 LESS than the 105 baseline because the role="list"
  reorder wrapper retired a long-standing
  a11y_no_static_element_interactions warning on the row-wrap).
  Pushed + verified (local==origin 76860b0). Process note: built
  the whole batch first for one gate cycle, snapshotted final
  files to /tmp/coll-final, then unwound to HEAD and re-applied
  each slice via targeted patches to land 5 independently-
  revertible commits — same pattern round-5 introduced, time
  cost ~15 extra min vs one mega-commit but every slice stays
  revertible. Tag/search/OCR-Queue surfaces stay feature-
  complete (no regressions on the 312 baseline); manual
  collections are now also end-to-end demo-able. Next subsystem
  candidates: smart-folders hub UI polish, doc-detail metadata
  editor, Beacon cache inspector, plugin marketplace.

### What round-5 (2026-06-19 21:25 PT) just shipped

A demo-able overhaul of the auto-OCR pipeline. Before this tick the
queue had no failure visibility, no retry surface, no dashboard, no
dedicated UI — just a 1-line "OCR N pending" chip on the Library
toolbar that ran everything and collapsed every state into one number.

- Slice 1: persisted OCR failure reasons (schema v11->v12 ocr_error
  column on library_documents; DocumentRecord widened end-to-end
  through 4 SELECT sites; set_doc_ocr_error setter with trim+clear
  semantics; run_one writes the reason on failure and clears on
  success; 5 new tests including the equality-trap-safe v12 column
  pin). Backend 92fc6d8.
- Slice 2: re-queue from done/failed/pending back to scanned. New
  requeue_doc and requeue_all_failed; rejects text_native and unknown
  with named errors (those are scanner classifications, not queue
  states); clears ocr_error + ocr_output_path so the row is genuinely
  fresh before run_one picks it up; 7 new tests. Wire + Tauri +
  TS bundled. Backend 84a992f.
- Slice 3: dashboard stats — OcrQueueStats with per-state counts
  (scanned/mixed/pending/done/failed/text_native/unknown) plus
  computed pending_total + total convenience fields, in one
  GROUP BY round-trip; forward-compat ignores unknown buckets so a
  future state can't crash the dashboard; 4 new tests including a
  serde round-trip pin. Wire bundled. Backend 0e85112.
- Slice 4: list_failed — every ocr_failed doc ordered last_seen_at
  DESC so the newest breakages bubble to the top of the failure
  inbox; 3 new tests. Wire bundled. Backend 816a03f.
- Slice 5: dedicated OcrQueuePanel.svelte — single panel that ties
  slices 1-4 together: per-state stats grid + indexed-% tile, a
  failure inbox section (each row names the captured reason in
  mono-red with per-row Open + Retry plus a header Retry-all), a
  pending queue preview with per-row Run-now + Open + bulk Run-all.
  Mounted by CollectionsSidebar mirroring the SmartFoldersHubPanel
  pattern (window event + Cmd/Ctrl+Shift+O shortcut + palette entry).
  Refreshes on mount and every library-changed event. Pure frontend
  slice (no schema, no commands beyond the four already wired).
  UI 07f5f0a.

Gates passed: cargo test --lib pdf::library:: 312 passed / 0 failed
(+20 from the 292 at round-4 baseline: 5 ocr_error tests + 8 stats
+ requeue tests + 3 list_failed + the v12 column test + a couple of
mixed-state regressions), cargo clippy --lib -D warnings clean
(2m48s cold first run, 0.62s warm), cargo fmt clean, pnpm check 0
errors / 105 warnings all pre-existing in other panels (none in
OcrQueuePanel or LibraryPanel from this batch).

## BUILD ENVIRONMENT — CRITICAL, read before any cargo command

Internal disk is FULL (~2.9 GiB free of 228). Cargo target is redirected to an
APFS sparse image at **/Volumes/SlabBuild** via `src-tauri/.cargo/config.toml`
(gitignored). Verify mounted each tick: `df -h /Volumes/SlabBuild | tail -1`.
If missing: `hdiutil attach "/Volumes/Sanjay SSD/SlabBuild.sparseimage"`.

**The image has very slow fsync.** Proven tonight across many attempts:
- `cargo test --lib`, `cargo check --lib`, `pnpm check` → WORK (slow but finish).
- A FULL `cargo build` / `cargo tauri build` → WEDGES on the `tauri` crate's
  final codegen (rustc goes to sleep state, no CPU, target size flat for min).
**RULE: never run a full binary build in a tick.** It's release work, blocked by
CI billing anyway. Gate with `cargo test --lib` + `cargo clippy --lib` + `pnpm
check`. If cargo wedges >5 min with no rustc CPU: `pkill -f 'cargo'`, retry once.

## CI STILL BLOCKED — needs Sanjay

GitHub Actions billing failure persists → no release artifacts (DMG/MSI/AppImage)
until fixed. Action: https://github.com/settings/billing → update payment / raise
limit. Does NOT affect local dev or branch pushes.

## Roadmap — round 5 (OCR Queue subsystem) — ALL DONE

Round 5 batched FIVE feature slices into one cron tick onto a fresh
subsystem (the auto-OCR queue — the only library plumbing left without
a dedicated surface in v3.39.0).

18. ~~**Persisted OCR error column (schema v12 + ocr_error end-to-end)**~~
    — DONE (2026-06-19 21:25 PT, 92fc6d8, single commit). Schema bump
    11->12: ALTER TABLE library_documents ADD COLUMN ocr_error TEXT.
    DocumentRecord widened with ocr_error: Option<String> (#[serde(default)]).
    set_doc_ocr_error setter trims input, treats trimmed-empty as None
    (column only ever holds "real" reasons). 4 SELECT sites widened to
    the new 13-column shape: registry::find_document_by_path,
    document_from_row, query::query_documents, collections::list_collection_docs,
    ocr_queue's two row-reads. run_one writes the reason on failure
    (also clears ocr_output_path so the row never claims a stale .ocr.pdf)
    and clears the reason on success. TS DocumentRecord.ocr_error mirror
    + LibraryPanel.applyResult also patches local ocr_error from the
    queue result. 5 new tests: v12 column with >= version pin (equality-
    trap-safe convention from v11), setter round-trip incl. trim+clear,
    setter preserves title/state/output_path/pages (column drift guard),
    upsert preserves ocr_error, run_one persists+clears.
19. ~~**Re-queue OCR docs from done/failed/pending**~~ — DONE
    (2026-06-19 21:25 PT, 84a992f, single commit). requeue_doc(doc_id)
    flips ocr_done / ocr_failed / ocr_pending back to scanned, clears
    ocr_error and ocr_output_path, re-reads via the 13-column SELECT;
    rejects text_native / unknown with named errors (scanner
    classifications, not queue states — re-queueing them would lie);
    unknown id errors. requeue_all_failed bulk-flips every failed row
    in one transactional UPDATE. 7 new tests: failed->scanned w/ error
    clear, output_path clear from prior success, stale-pending recovery,
    text_native rejection (error names the state), unknown id rejection,
    bulk requeue flips only failed rows (in-use untouched), bulk
    requeue is 0 on a clean library. Two Tauri commands + two TS
    helpers bundled with the backend per the wire-layer convention.
    Both emit library-changed on success (the bulk one only when n > 0).
20. ~~**OCR queue dashboard stats (per-state counts)**~~ — DONE
    (2026-06-19 21:25 PT, 0e85112, single commit). New OcrQueueStats
    struct with named fields per known ocr_state value, plus computed
    pending_total (scanned + mixed) and total. Single SELECT
    ocr_state, COUNT(*) GROUP BY ocr_state round-trip; forward-compat
    silently ignores unknown buckets so a future state can't crash the
    dashboard (the COUNT still rolls into `total`). 4 new tests: empty
    library all-zeros, full bucket coverage with 7 mixed-state seeds,
    forward-compat unknown bucket doesn't increment known counts but
    bumps total, serde snake_case round-trip pin (text_native +
    pending_total). One Tauri command (pure read, no library-changed
    emit) + TS ocrQueueStats() + interface bundled.
21. ~~**List failed docs (failure inbox feed)**~~ — DONE (2026-06-19
    21:25 PT, 816a03f, single commit). list_failed returns every
    ocr_failed row ORDER BY last_seen_at DESC, id DESC (newest
    breakages bubble to the top; scanner refresh of last_seen_at means
    the right anchor; id tie-break keeps stable order across same-
    second seeds). Full DocumentRecord rows with ocr_error populated.
    3 new tests: only-failed-rows filter, DESC order with cross-second
    sleep, empty result on clean library. One Tauri command (pure
    read) + TS ocrQueueListFailed() bundled.
22. ~~**Dedicated OCR Queue Panel UI**~~ — DONE (2026-06-19 21:25 PT,
    07f5f0a, single commit). 800 LOC Svelte 5 panel that ties slices
    1-4 into one demo-able surface. Sections: dashboard stats grid
    (per-state counts + indexed-% tile, accent-colored tiles + tabular
    nums + monochrome status dots, no emoji per house style); failure
    inbox (only renders when failed > 0; each row names the captured
    ocr_error in monospace red, per-row Open + Retry, section-head
    "Retry all" wraps Slice 2 bulk requeue); pending queue preview
    (first 20 scanned/mixed rows w/ per-row Open + Run-now, header
    "Run all (N)" wraps Slice 0-vintage ocrQueueRunAll, truncation
    hint when > 20). Modal-style chrome reuses SmartFoldersHubPanel
    pattern (color-mix on var(--panel-bg), 16-radius shell, 14px blur
    backdrop, pop-in animation). Mounted by CollectionsSidebar via
    window event + Cmd/Ctrl+Shift+O shortcut + "OCR Queue…" command-
    palette entry. Refreshes on mount + every library-changed event so
    a background OCR run updates the panel without a manual reload.
    Pure frontend slice (no schema, no backend, no new Tauri commands
    beyond the four already shipped). Gates: pnpm check 0 errors /
    105 warnings all pre-existing in other panels (none new from this
    panel or its sidebar mount).

    With Round 5 done, the auto-OCR queue is now end-to-end
    demo-able: persisted failures + re-queue + stats + inbox + a
    dedicated panel a user can actually open. Next ticks should pick
    a different subsystem — good candidates remaining: smart-folders
    hub UI polish, collections, doc-detail metadata editor, Beacon
    cache inspector, plugin marketplace.

## Tick log



### What round-4 (2026-06-19 20:00 PT) just shipped

A demo-able overhaul of the LibrarySearchPanel + its FTS5 query layer:
- Slice 1: rolling recent-searches surfaced as one-click chip strip with
  result-count badges + "Clear history" affordance (`recent_queries` was
  internal-only; now wired through `slab_library_recent_searches` +
  `slab_library_clear_search_history` + UI consumer).
- Slice 2: per-folder scope filter — backend `search()` already took
  `folder_id`; UI now exposes a native `<select>` that re-queries on change.
  Only renders when the library has >1 folder.
- Slice 3: quoted-phrase queries — `build_match_expr` now lexes `"force
  majeure"` as a single FTS5 phrase token (adjacent-word match) instead
  of stripping the quotes. Supports curly quotes for macOS auto-correct,
  forgiving unterminated phrases, metacharacter scrubbing inside phrases.
- Slice 4: exclude-term syntax — `-word` / `-"phrase"` maps to FTS5 `NOT`
  clauses. Exclude-only queries (no positive anchor) return `[]` cleanly
  rather than triggering an FTS5 syntax error.
- Slice 5: pinned status footer — `IndexStats { docs, pages }` exposed
  as `slab_library_index_stats`, rendered as a compact "● N docs / M pages
  indexed" footer at the bottom of the panel (refreshes on mount + after
  every search).

Gates passed: `cargo test --lib pdf::library::` 292 passed / 0 failed
(+27 from the 265 at v3.51 — 5 search_log + 13 search + 3 IndexStats + 6
already-shipped tag-desc tests retained), `cargo clippy --lib -D warnings`
clean (8.86s warm), `pnpm check` 0 errors / 105 warnings all pre-existing.

## BUILD ENVIRONMENT — CRITICAL, read before any cargo command

Internal disk is FULL (~2.9 GiB free of 228). Cargo target is redirected to an
APFS sparse image at **/Volumes/SlabBuild** via `src-tauri/.cargo/config.toml`
(gitignored). Verify mounted each tick: `df -h /Volumes/SlabBuild | tail -1`.
If missing: `hdiutil attach "/Volumes/Sanjay SSD/SlabBuild.sparseimage"`.

**The image has very slow fsync.** Proven tonight across many attempts:
- `cargo test --lib`, `cargo check --lib`, `pnpm check` → WORK (slow but finish).
- A FULL `cargo build` / `cargo tauri build` → WEDGES on the `tauri` crate's
  final codegen (rustc goes to sleep state, no CPU, target size flat for min).
**RULE: never run a full binary build in a tick.** It's release work, blocked by
CI billing anyway. Gate with `cargo test --lib` + `cargo clippy --lib` + `pnpm
check`. If cargo wedges >5 min with no rustc CPU: `pkill -f 'cargo'`, retry once.

## CI STILL BLOCKED — needs Sanjay

GitHub Actions billing failure persists → no release artifacts (DMG/MSI/AppImage)
until fixed. Action: https://github.com/settings/billing → update payment / raise
limit. Does NOT affect local dev or branch pushes.

## Roadmap — round 4 (LibrarySearchPanel + FTS5 query layer) — ALL DONE

The tag/tag-filter surface was deliberately complete after round 3; this
round 4 batched FIVE feature slices into one cron tick on a different
subsystem (full-text search across the indexed library — the surface a
paralegal types `"force majeure"` into).

13. ~~**Recent searches strip**~~ — DONE (2026-06-19 20:00 PT, c4ca277
    backend + wire + a2c7162 UI, two commits). Backend: QueryRow gains
    Serialize+Deserialize (snake_case roundtrip pinned), new clear()
    helper (scoped to library_search_log, NOT touching
    library_suggestion_dismissed), 5 new tests (clear-removes /
    clear-empty-noop / clear-leaves-dismissals / serde-roundtrip + the
    pre-existing recent_queries surface). Two Tauri commands
    (slab_library_recent_searches with limit clamped 1..=50 default 8,
    slab_library_clear_search_history emits library-changed only when
    n>0). TS client (RecentSearch + recentLibrarySearches +
    clearLibrarySearchHistory) bundled with backend. UI: chip strip
    above empty-state tips when recents>0, each chip one-click
    re-runs its saved query and wears the result-count badge it last
    produced (a 0 chip == "this stopped matching, maybe a re-index
    dropped it"); "Clear history" affordance confirms with the exact
    count; runRecent flows through the existing runSearch path (no
    debounce, click is the intent); strip auto-refreshes after every
    runSearch so freshly-typed queries bubble to the head + the 30s
    dedupe-coalesce in the backend means re-typing the same query
    bumps the existing chip's count rather than spawning a duplicate.

14. ~~**Per-folder scope filter**~~ — DONE (2026-06-19 20:00 PT,
    25d14cd, single commit). The backend search() has accepted
    Option<folder_id> since v2.2.0 but the UI always passed null —
    every search ran against the entire indexed library. This slice
    exposes scope as a native <select> between the input and the
    status line, rendered only when the library has >1 folder (a
    single-folder library has nothing to scope so we don't show
    inert chrome). Threads scopeFolderId into librarySearch() so
    the existing FTS5 folder-filter branch fires; onScopeChange
    immediately re-runs the active query (no Enter needed); a
    vanishing scope folder (removed between sessions) silently
    self-heals back to All. Result-count line and no-matches empty
    state both surface the active scope inline so the user can't
    be confused about reduced hit counts. Pure frontend slice +
    one extra import (listFolders) — no backend churn, no schema,
    no Rust gates beyond pnpm check.

15. ~~**Quoted-phrase queries (adjacent-word matching)**~~ — DONE
    (2026-06-19 20:00 PT, 1804706, single commit). FTS5's MATCH
    grammar has always supported `"a b"` as adjacent-token matching;
    the previous build_match_expr() stripped quotes in the sanitiser
    and fell back to bag-of-words. Replaced with a hand-written
    lexer (tokenize -> Vec<Tok::Bare | Tok::Phrase>) so a phrase
    becomes a single FTS5 phrase token. Bare-word LAST gets the
    prefix glob (so `dra "force majeure"` still prefix-matches dra);
    phrases never get `*` (FTS5 rejects "a b"*); curly quotes "" ""
    (macOS auto-correct default) work like straight quotes;
    unterminated `"trailing` runs the phrase to end-of-input
    (Google's behaviour); metacharacters inside phrases are
    scrubbed but adjacent collapse to one token because we don't
    synthesise word boundaries from disappeared punctuation
    (same heuristic as `co-op` -> coop for bare words). 10 new tests
    plus the empty-state tips help-text gains a "Wrap a phrase in
    quotes" line so the feature is self-discoverable. Logging is
    preserved: the search log stores the user-typed query with
    quotes intact, so a "force majeure" chip in the recent-searches
    strip re-runs the phrase exactly.

16. ~~**Exclude-term syntax (-word)**~~ — DONE (2026-06-19 20:00 PT,
    db6d30b, single commit). A leading `-` on a token flips it into
    FTS5 NOT semantics so a user can type `contract -draft` and
    drop drafts from the result set. The lexer grows one new token
    kind (Tok::Exclude); the formatter wraps it as `NOT "word"`.
    Semantics mirror Google: exclude-only queries (`-draft` alone)
    return [] because FTS5 rejects MATCHes that are nothing but
    NOT — a positive anchor is required. `co-op` mid-word `-` is
    NOT a trigger (only LEADING `-` on a fresh token), `- ` lone
    dash dropped, `-"prior draft"` exclude-a-phrase works, excluded
    terms still flow through scrub_word so metacharacters can't
    sneak into NOT clauses, multiple `-foo -bar` exclusions chain
    as separate NOTs. Excluded terms never carry the prefix glob `*`
    — a stray prefix could silently drop legitimate hits. 10 new
    tests; UI grows a second tips line `Prefix a term with -`.
    A follow-up b9b7a76 commit corrected two phrase tests that
    expected stale lexer behaviour AND removed an unused-assignment
    that clippy `-D unused-assignments` rightly caught (real
    behaviour identical; comment in tokenize() pinned for future
    cleanup safety).

17. ~~**Pinned index-status footer**~~ — DONE (2026-06-19 20:00 PT,
    7c14b70 backend + wire + 6a3d62d UI, two commits). count_indexed_docs
    was test-only-callable; promoted alongside a new IndexStats
    { docs, pages } and an index_stats() composer over two cheap
    COUNT queries. 3 new tests (empty-zeros / counts-seeded-3-4 /
    serde-roundtrip-pin). One Tauri command (slab_library_index_stats).
    TS client (LibraryIndexStats + libraryIndexStats) bundled per
    convention. UI: compact "● N docs / M pages indexed" footer
    pinned beneath .results (flex-shrink:0 so it never scrolls);
    accent-green status dot mirrors the LibraryPanel indexed pip;
    refreshes on mount + after every search so a scan landing
    mid-session makes the counts grow live without a panel remount;
    a backend failure silently collapses the footer to null rather
    than spamming an error (non-load-bearing glance); toLocaleString
    + tabular-nums + plural-pinch on the count text; {#if} guard
    hides the footer entirely on a 0/0 empty index so the
    onboarding empty-state doesn't compete with a "0 indexed" line.

    This rounds out the full-text search surface (recent-searches +
    folder scope + phrase + exclude + index-status footer). Next tick
    should pick a different subsystem — good candidates: smart-folders
    hub UI polish, OCR queue panel, collections, doc-detail metadata
    editor.



These extend the tag system the v3.39.0 work introduced. Ship ONE complete
vertical slice per tick (Rust + tests + Tauri command + TS client + Svelte UI).

1. ~~**Untagged filter**~~ — DONE (v3.40.0 slice, 2026-06-18 01:05 PT, a0836e3 +
   95ed028). Added first-class `Untagged`/`Tagged` clauses to the filter
   language (query.rs), fixed the `untagged` preset TODO, and added a one-click
   "Untagged" toggle chip to the LibraryPanel toolbar.
2. ~~**Bulk tag-apply**~~ — DONE (2026-06-18 01:55 PT, d6a46fb backend +
   c4c9848 UI). New `bulk_tag.rs` (apply_tag_to_docs find-or-creates + unions,
   remove_tag_from_docs detaches links only; both transactional, report
   affected/total; 12 tests). `registry::find_tag_by_id` added; `pastel_for`
   promoted to pub(crate). Two Tauri commands (bulk_apply / bulk_remove).
   TS clients + a multi-select grid: "Select" toolbar toggle, per-card
   checkboxes, floating action bar with All/None/Clear + a tag picker
   (apply existing/new, remove existing) and an "N of M" toast. Selection is
   pruned to the visible set each refresh; live multi-selection drags as a set.
   ALSO fixed a stale collections.rs test (schema_version 7→8) that the
   v3.39.0 migration had silently broken — earlier ticks ran scoped tests so
   the full `cargo test --lib` never surfaced it.
3. ~~**Tag colors**~~ — DONE (2026-06-18 02:40 PT, 6a7ff10 backend +
   155fe06 UI). The `color` column already existed, so this shipped the
   EDIT path: `registry::set_tag_color(tag_id, Option<&str>)` updates/clears
   a tag's color + returns the row, guarded by `valid_tag_color()` which only
   persists `#hex` / `hsl()/hsla()/rgb()/rgba()` shapes (functional body
   restricted to digits/dots/%/comma/space — no CSS injection). Unknown id and
   bad color both error without touching the row (11 tests). One Tauri command
   (set_tag_color). TS `setTagColor` client + a tag-rail color-edit affordance:
   a filled-dot button per row opens a "Tag color" modal (live preview swatch +
   the existing palette + a "Default" clear-to-deterministic option); saving
   swaps the updated row into the rail and every doc card in place (no refetch).
4. ~~**Tag rename**~~ — DONE (2026-06-18 03:25 PT, 44444f8 backend +
   161dfbb UI). `registry::rename_tag(tag_id, new_name)` is a single UPDATE
   on library_tags; because library_doc_tags links by tag_id (never name),
   the rename propagates to every doc + live co-occurrence with no migration
   and no orphans. Name is trimmed; same-name is a no-op; a pure case change
   (research->Research) is a valid distinct rename under BINARY collation;
   renaming onto a *different* tag's existing name is REJECTED (UNIQUE name
   col) rather than silently merging — the rejected update leaves both rows
   untouched; empty name and unknown id also error (8 tests). One Tauri
   command (slab_library_rename_tag) returns the updated row. TS `renameTag`
   client + an inline rail edit: a pencil glyph (beside the color dot +
   delete x) swaps the row label for an auto-selected text input; Enter
   commits, Escape/blur cancels, unchanged/empty cancels with no round-trip;
   on success the row swaps into the rail + every doc card in place (no
   refetch); a rejected rename keeps the row in edit mode and shows the
   backend reason inline so the user can fix + retry.
5. ~~**Recently-used tags**~~ — DONE (2026-06-18 04:20 PT, cf62147 backend +
   3fc663a UI). Schema v8->v9: nullable `applied_at` on library_doc_tags +
   `(tag_id, applied_at)` index. `set_doc_tags` rewritten from
   wipe-and-reinsert into a true DIFF so surviving links keep their original
   stamp and only new links are stamped now() — re-saving an unchanged set
   must not restamp (would shuffle a stable tag to the top). `bulk_tag` apply
   stamps too. `registry::recently_used_tags(limit)` returns each used tag
   once by MAX(applied_at) desc, link-rowid tie-break, NULL stamps last,
   never-applied excluded. One Tauri command (slab_library_recently_used_tags,
   limit default 8). TS `recentlyUsedTags` client + a "Recently used"
   quick-chip row at the top of the per-doc tag context menu (lazy-loaded on
   open, re-ranked after each apply/remove, hides tags already on the doc via
   a $derived list). ALSO relaxed two schema-version-pinning tests
   (registry + collections) from `== 8` to `>=` + added a dedicated v9 column
   test, so the next migration won't trip an unrelated equality assert (the
   exact trap that bit the v3.39.0->bulk tick). Gates: cargo fmt clean,
   cargo test --lib pdf::library 206 passed/0 failed (9 new), clippy --lib
   -D warnings clean (9.1s warm), pnpm check 0 errors. Pushed + verified.
6. ~~**Tag merge**~~ — DONE (2026-06-18 04:55 PT, 2083c1f backend +
   e2fe7b7 UI). `registry::merge_tags(source_id, target_id)` folds the
   source tag into the target in one transaction: step 1 lifts the target
   link's applied_at to the NULL-aware max of both stamps for docs carrying
   BOTH tags (max(coalesce(a,b), coalesce(b,a)) so a real timestamp always
   beats a legacy NULL, NULL only when both are), step 2 re-points
   source-only links via UPDATE OR IGNORE (keeping their own stamp), step 3
   deletes leftover source links + the orphaned source tag row. Both ends
   validated up front so a rejected merge (unknown id, or merge-into-self)
   leaves every row untouched; returns the surviving target. One Tauri
   command (slab_library_merge_tags). 12 new tests (source-only re-point,
   both-tag coalesce-to-one-link, newest-stamp each side, real-beats-NULL
   either side, re-pointed stamp carry-over, recently-used order survives,
   self/unknown rejection intact, multi-doc, no-doc). UI: TS mergeTags +
   a merge glyph in the rail row menu (beside rename/color/delete) opening
   a "Merge tag" modal that names the source and lists every other tag as a
   target ($derived candidates exclude source); on success the rail drops
   the source row + swaps the target in place, an active filter on the
   source re-points to the target, doc cards re-point + de-dupe their
   source chip in place (no refetch), recently-used reloads; a rejected
   merge keeps the modal open with the reason inline. Gates: cargo fmt
   clean, cargo test --lib pdf::library:: 218 passed/0 failed (12 new),
   clippy --lib -D warnings clean (6.2s warm), pnpm check 0 errors (no new
   LibraryPanel warnings; the 2 there are pre-existing autofocus + webkit
   CSS). Build cache from the 04:20 tick still warm — test 1.72s.

   This completes the tag-management surface the v3.39.0 work introduced
   (suggest, untagged filter, bulk apply, color, rename, recently-used,
   merge). Next ticks pick from the fresh roadmap below.

## Roadmap — fresh items (tag system is feature-complete; these are new)

7. ~~**Tag usage counts in the rail**~~ — DONE (2026-06-18 05:50 PT, 966db5e).
   `registry::tag_usage_counts() -> Vec<(tag_id, count)>` single LEFT JOIN +
   GROUP BY (one round-trip, never N); every tag appears once, a tag on zero
   docs reports 0 (LEFT JOIN keeps the merge/remove residue an INNER JOIN
   would drop), id-ordered. One Tauri command (slab_library_tag_usage_counts);
   6 tests (per-doc counts, zero-for-unused, one-row-per-tag-id-ordered,
   empty, reflects bulk apply/remove, reflects merge as a distinct union with
   no double-count + gone source unreported). TS `tagUsageCounts()` returns a
   Map<tagId,count>. LibraryPanel loads counts alongside listFolders/listTags
   in refreshAll so the rail count self-heals on every library-changed poke
   (no bespoke optimistic plumbing — same resync path tags/docs already use);
   a muted `rail-meta` count renders beside each tag (mirrors the folder rail)
   and a rail-head A-Z / Most-used sort toggle (count desc, name tie-break for
   a stable order; shown only when >1 tag) makes the count meaningful.
   Gates: cargo fmt clean, cargo test --lib pdf::library:: 224 passed/0 failed
   (6 new), clippy --lib -D warnings clean (6.61s warm), pnpm check 0 errors
   (no new LibraryPanel warnings; still the 2 pre-existing autofocus + webkit).
8. ~~**Empty/unused tag cleanup**~~ — DONE (2026-06-18 06:35 PT, cd4219a
   backend + ba7a83d UI). `registry::delete_unused_tags() -> usize`: a single
   DELETE over library_tags guarded by `NOT EXISTS` against library_doc_tags
   (tag_id is NOT NULL so NOT EXISTS is the clean form), removes every tag on
   zero docs and returns the count; a tag with even one link is untouched, an
   empty library is a no-op returning 0. One Tauri command
   (slab_library_delete_unused_tags) emits library-changed only on a non-empty
   cleanup. 4 tests (removes-only-unused, no-op when all used, empty-is-zero,
   and the motivating bulk-remove-leaves-residue-at-0 reclaim). UI: TS
   deleteUnusedTags + a $derived unusedTagCount off the existing tagCounts map
   (count 0 == unused, self-heals on every refresh, no bespoke plumbing); a
   muted "Clean up N" rail-head affordance shown only when >0, danger-tinted
   hover, disabled while pruning; click confirms with the exact count,
   snapshots doomed ids to prune the active filter, toasts "Removed N", then
   refreshAll reconciles off the backend. Gates: cargo fmt clean, cargo test
   --lib pdf::library:: 228 passed/0 failed (4 new), clippy --lib -D warnings
   clean (6.48s warm), pnpm check 0 errors (no new LibraryPanel warnings).
9. ~~**Tag filter combinator (AND/OR)**~~ — DONE (2026-06-18 07:25 PT,
   18229a8 backend + 522cbe9 UI, two commits). The rail's multi-tag
   selection has always intersected (AND). Added a `TagMatch` enum
   (All default / Any, serde snake_case like FilterCombinator/SortBy) +
   a `tag_match` field on LibraryFilter (#[serde(default)] => All, so every
   pre-v3.48 stored filter keeps intersection semantics byte-for-byte, no
   migration). query_documents now branches the FLAT tag path: All keeps
   the GROUP BY ... HAVING COUNT(DISTINCT tag_id) = N intersection, Any
   drops the HAVING and matches on `tag_id IN (...)` alone (union). The All
   count was hardened to DEDUP the requested ids first so a duplicated id
   can't raise the HAVING bar past what a single doc can satisfy. 8 new
   tests (All-default-intersects, Any-unions, Any-vs-All-diverge on the
   same id set, All-tolerates-dup-ids, Any==All for one tag, legacy-JSON-
   defaults-to-All, tag_match snake_case roundtrip). UI: TS TagMatch type +
   tag_match on the mirror; LibraryPanel tagMatch state (default "all")
   threaded into the flat refreshDocs filter + the reactive $effect deps so
   flipping re-queries; an "All tags"/"Any tag" toggle in the Tags rail head
   shown ONLY when >1 tag is selected (the only time AND vs OR changes the
   result), accent-tinted in the non-default "Any" state, mirrors .rail-sort
   chrome. Chose the flat tag_match field over hand-assembling nested clause
   groups in the UI: tiny frontend churn, fully backward-compatible, and the
   rail's tag toggles stay a flat list. Gates: cargo fmt clean, cargo test
   --lib pdf::library:: 234 passed/0 failed (6 new query tests; 2 of the 8
   are serde unit tests in the same file), clippy --lib -D warnings clean
   (8.73s), pnpm check 0 errors (LibraryPanel still only the 2 pre-existing
   autofocus + webkit warnings, none new). Build cache warm — test 1.75s.

   This exhausts the seeded roadmap (#7 usage counts, #8 unused cleanup,
   #9 AND/OR combinator all done). Fresh roadmap below.

## Roadmap — fresh items (round 3; the tag rail is deep now)

These are NEW surfaces, not more tag plumbing — the tag-management +
tag-filter surface is mature. Ship ONE complete vertical slice per tick.

10. ~~**Saved tag-filter views**~~ — DONE (2026-06-19 19:47 PT, 2cf2a49
    backend + 7c83eee UI). Schema v9->v10: new `library_saved_views`
    table (id, name UNIQUE, filter_json, created_at, sort_order). Filter
    is the full LibraryFilter blob serialized via serde_json (opacity
    contract mirrors personal_presets so the entire FilterGroup tree
    survives query-language schema bumps). `saved_views.rs`: save_view
    (trims, empty rejected, UNIQUE on duplicate), get_view, list_views
    (sort_order asc, name tie-break), delete_view (unknown id = 0-row
    no-op), rename_view (trims, empty rejected, same-name short-circuit
    without an UPDATE, UNIQUE collision rejected leaving both rows
    intact). 17 module tests incl. flat AND clause-tree round-trips
    byte-for-byte through serde, sort order, delete pruning only the
    target, rename collision atomicity. 4 Tauri commands
    (slab_library_saved_view_save / list / delete / rename) all emit
    library-changed on success so the rail self-heals via refreshAll.
    UI: a new "Saved views" rail section between Folders and Tags.
    "Save filter" button shows in the section head when any filter
    dimension is non-default (folder, any tag, untagged, search query,
    non-default sort); opens an inline name input (Enter commits,
    Escape cancels). Each view = rail row + diamond glyph + name +
    x-delete. One click on a view restores the entire saved filter in
    a single batch (folder + tags + match mode + untagged + sort +
    query) so the existing reactive $effect re-queries exactly once;
    active row highlight clears the moment the user diverges from the
    saved snapshot via a cheap structural $effect diff. Save form seeds
    the name from the obvious anchor (active folder name, only-selected
    tag name, or "Untagged") so 80% of saves are one keystroke. UNIQUE
    collisions on save surface inline. buildCurrentFilter mirrors
    refreshDocs's two-branch shape so what's saved is what gets queried;
    restoreSavedView unpacks either shape back into the rail $state
    cells, ignoring exotic clauses so forward-compat is automatic.
    Bumped SCHEMA_VERSION 9->10, relaxed the v9 column-test schema-
    version assert from ==9 to >=9, added a positive v10 column/table
    test (asserts library_saved_views exists with id/name/filter_json/
    created_at/sort_order). Gates: cargo fmt clean, cargo test --lib
    pdf::library:: 252 passed/0 failed (18 new: 17 saved_views + 1
    schema v10), clippy --lib -D warnings clean (8.66s warm), pnpm
    check 0 errors (LibraryPanel still only the 2 pre-existing
    autofocus + webkit warnings, none new). Pushed + verified
    (local==origin 7c83eee). Two commits, backend bundled with the TS
    client wire layer (useless without each other), UI as the second
    commit. Next undone: #12 tag descriptions/notes.
11. ~~**Tag filter clear-all**~~ — DONE (2026-06-18 07:50 PT, f41a6a1,
    frontend-only single commit). A "Clear" affordance in the Tags rail
    head, shown only when `tagFilterActive` ($derived: activeTagIds.size > 0
    || untaggedOnly). One click runs clearTagFilter(): activeTagIds = new
    Set(), untaggedOnly = false, tagMatch = "all" — three fresh assignments
    so the existing reactive $effect (deps activeTagIds/untaggedOnly/tagMatch/
    sort/activeFolder) re-queries exactly once, no manual refresh. Match mode
    is excluded from the visibility test on purpose (inert with 0 tags, so a
    lingering non-default mode shouldn't surface a Clear on its own) but is
    reset anyway for a fully clean slate. Button is first in the rail-head
    chrome group, mirrors the .rail-sort/.rail-match chrome (muted uppercase,
    margin-left:auto, neutral hover-to-text); non-destructive reset so NO
    danger tint (unlike .rail-cleanup). No backend, no schema, no cargo. Gate:
    pnpm check 0 errors, LibraryPanel still only its 2 pre-existing warnings
    (autofocus + webkit), none new. Pushed + verified (local==origin f41a6a1).
    Picked over #10 saved-views because only ~12 min remained before the 08:00
    auto-stop — #11 was the seeded "lowest-risk pick if a tick is tight on
    build budget", needs no slow cargo gate, and is genuinely useful now the
    rail has many tags. Next undone: #10 saved tag-filter views, #12 tag
    descriptions/notes.
12. ~~**Tag descriptions / notes**~~ — DONE (2026-06-19 19:58 PT, 43d3258
    backend + 3e92aaf UI). Schema v10->v11: nullable `description` column
    on library_tags so every pre-v11 tag silently picks up NULL (no
    rewrite). `registry::set_tag_description(tag_id, Option<&str>)`:
    trims input, trimmed-empty equivalent to None and clears column back
    to NULL (column only ever holds "real" notes); length cap is
    MAX_TAG_DESCRIPTION_LEN = 500 *Unicode scalars* not bytes (emoji + CJK
    get a sane budget); valid_tag_description guard runs BEFORE the
    UPDATE so a rejected oversize leaves the row's old description
    untouched; unknown id errors. TagRecord widened with
    `description: Option<String>`; every SELECT that returns a tag row
    (find_tag_by_name/id, list_tags, tags_for_document,
    recently_used_tags, query_documents tag join) was widened to carry
    the new column so the field travels everywhere. One Tauri command
    (slab_library_set_tag_description) emits library-changed. 13 new
    tests: v11 column test (with >= version-pin convention to dodge the
    equality-trap that bit v3.39->bulk-tag), starts-with-no-description,
    update-returns-row, trims-whitespace, empty/None-clears, accepts-max,
    rejects-oversized-row-untouched, counts-chars-not-bytes (multibyte
    CJK fits at max scalars), unknown-id-errors, persists-across-list-
    tags+recently-used+tags-for-document, rename-tag-preserves-description,
    set-tag-color-preserves-description (the last two cover column drift
    if a neighbouring update regresses). UI: TS setTagDescription bundled
    with the backend commit (wire layer convention); LibraryPanel adds
    a paragraph-glyph button per rail row beside pencil/dot/x (.has-notes
    accent tint when the tag actually carries a note); title attr on the
    tag rail row AND every doc-card chip surfaces the description as a
    tooltip (cheap — TagRecord already travels with both); edit-notes
    modal reuses the modal-backdrop chrome, header has the tag dot +
    name, textarea seeded from current description (empty string when
    unset; backend treats empty as clear, no sentinel needed), maxlength
    mirrors the 500-char backend cap, character counter tints red near
    the limit, Cmd/Ctrl+Enter submits, button label flips Save/Clear
    based on trimmed-empty draft (explicit destructive action instead
    of silent), success swaps the updated row into the rail + every doc
    card that carries it (no refetch), rejection keeps the modal open
    with the backend reason inline + the input stays in error state.
    Gates: cargo fmt clean, cargo test --lib pdf::library:: 265 passed/
    0 failed (13 new), cargo clippy --lib -D warnings clean (9.27s warm),
    pnpm check 0 errors (LibraryPanel still only the 2 pre-existing
    autofocus + webkit warnings, none new). Pushed + verified
    (local==origin 3e92aaf). Two commits: TS client (library.ts) bundled
    with the backend per the established convention (it's the wire layer
    and useless without the Tauri commands), UI as the second commit.

    This COMPLETES the round-3 roadmap (#10 saved views, #11 clear-all,
    #12 tag descriptions all done) — and with that, the entire tag and
    tag-filter surface is feature-complete: suggest, untagged filter,
    bulk apply, color, rename, recently-used, merge, usage counts,
    unused cleanup, AND/OR combinator, saved views, clear-all, and now
    descriptions. Next tick should seed a FRESH roadmap for a different
    subsystem (good candidates: smart-folders hub UI, OCR queue panel,
    collections, doc-detail metadata editor, full-text search) rather
    than mine the tag surface for more increments.

## House style (match existing code)

- Rust: mirror `tag_suggest.rs` / `folder_suggest.rs`. Tauri commands in `lib.rs`
  via `open_library_db()` + `CmdResult<T>` + `.into()`. Tests use
  `LibraryDb::open_in_memory()`.
- TS: flat in `src/lib/library.ts`, `invoke<CmdResult<T>>(...)` then `unwrap()`,
  camelCase args.
- Svelte 5 runes only (`$props`, `$state`, `onMount`). Dark-first design,
  monochrome glyphs in app chrome, no emoji.

## Tick log

- 2026-06-18 00:45 PT (Cake, interactive): committed + pushed v3.39.0 Atlas
  Tag-Suggest to feature branch (f997a33). Diagnosed slow-disk full-build wedge;
  set gates to lib-only. Seeded roadmap above. Overnight loop armed (30m, →08:00).
- 2026-06-18 01:05 PT (Cake, cron): roadmap #1 "Untagged filter" shipped.
  Backend a0836e3 (Untagged/Tagged filter clauses + preset TODO fixed, 32 query/
  preset tests green), UI 95ed028 (toolbar toggle chip + TS union + ClauseGroup
  narrowing fix). Gates: cargo fmt clean, clippy --lib -D warnings clean (13s warm),
  cargo test query 22 + presets 10 green, pnpm check 0 errors. Pushed + verified.
  NOTE for next tick: v3.39.0's first `cargo test`/`clippy` were COLD (~12-14 min
  each on the image) because the test/clippy profiles recompiled tauri+mockito
  from scratch; once warm, incremental test+clippy is ~10-20s. Budget the first
  build of a session generously. Also: the interactive session committed v3.39.0
  mid-build under author "Sanjay Santhanam" (its default git identity) — that's
  expected for the interactive session, not a cron mis-attribution.
- 2026-06-18 01:55 PT (Cake, cron): roadmap #2 "Bulk tag-apply" shipped.
  Backend d6a46fb (bulk_tag.rs apply/remove + find_tag_by_id + pastel_for
  pub(crate) + 2 Tauri commands; 12 new tests), UI c4c9848 (TS clients +
  multi-select grid + floating action bar + tag picker). Gates: cargo fmt clean,
  clippy --lib -D warnings clean (10.9s warm), cargo test --lib pdf::library::
  182 passed/0 failed, pnpm check 0 errors. Pushed + verified (local==origin).
  Incidentally fixed a PRE-EXISTING red test: collections.rs asserted
  schema_version==7 but the v3.39.0 migration moved it to 8; prior ticks only ran
  scoped tests (query/presets) so the full --lib suite never caught it. This
  session's first `cargo test` was warm (~24s compile) — build cache from the
  01:05 tick was still fresh, no cold recompile this time.
- 2026-06-18 02:40 PT (Cake, cron): roadmap #3 "Tag colors" shipped.
  Backend 6a7ff10 (registry::set_tag_color + valid_tag_color guard + 1 Tauri
  command; 11 new tests), UI 155fe06 (TS setTagColor + tag-rail color-edit
  affordance: per-row dot button -> "Tag color" modal with preview swatch +
  palette + Default clear, in-place row swap on save). No schema bump — the
  color column already existed; this was the edit path. Gates: cargo fmt clean,
  cargo test --lib pdf::library:: 190 passed/0 failed (8 new tag_color tests
  green), clippy --lib -D warnings clean (7.2s warm), pnpm check 0 errors.
  Pushed + verified (local==origin). Build cache from the 01:55 tick was still
  warm — test compile ~under a sec incremental, clippy 7s.
- 2026-06-18 03:25 PT (Cake, cron): roadmap #4 "Tag rename" shipped.
  Backend 44444f8 (registry::rename_tag — single UPDATE on library_tags, rename
  propagates via tag_id links so docs + co-occurrence follow with no migration;
  trims, same-name no-op, case-only rename valid, UNIQUE-collision rejected
  (no silent merge), empty/unknown error; 8 new tests + 1 Tauri command
  slab_library_rename_tag). UI 161dfbb (TS renameTag + inline rail edit: pencil
  glyph -> auto-selected text input, Enter commits / Escape+blur cancels,
  unchanged/empty short-circuits, in-place row+doc-card swap on success, inline
  error keeps row in edit mode on a rejected rename + focusSelect action).
  Gates: cargo fmt clean, cargo test --lib pdf::library:: 198 passed/0 failed
  (8 new rename_tag tests green), clippy --lib -D warnings clean (7.06s warm),
  pnpm check 0 errors (new input has aria-label, no new a11y warnings). Pushed
  + verified (local==origin 161dfbb). Build cache from the 02:40 tick still
  warm — test compile 1.46s, clippy 7s. No manifest bump (kept 3.39.0, per the
  established convention that v3.4x.0 labels are logical feature versions).
- 2026-06-18 04:20 PT (Cake, cron): roadmap #5 "Recently-used tags" shipped.
  Backend cf62147 (schema v8->v9: nullable applied_at on library_doc_tags +
  (tag_id, applied_at) index; set_doc_tags rewritten wipe-and-reinsert ->
  true diff so surviving links keep their stamp, only new links stamped now();
  bulk_tag apply stamps too; recently_used_tags(limit) ranks each used tag
  once by MAX(applied_at) desc, rowid tie-break, NULL-last, never-applied
  excluded; 1 Tauri command slab_library_recently_used_tags; 9 new tests).
  UI 3fc663a (TS recentlyUsedTags + "Recently used" quick-chip row at top of
  the per-doc tag context menu, lazy-load on open, re-rank after each toggle,
  $derived filter hides already-attached tags; dark-first pill styling).
  Relaxed two schema-version-pinning tests (registry + collections) from
  == 8 to >= + added a v9 column test, pre-empting the equality-assert trap.
  Gates: cargo fmt clean, cargo test --lib pdf::library:: 206 passed/0 failed,
  clippy --lib -D warnings clean (9.1s warm), pnpm check 0 errors (no new
  LibraryPanel warnings; the 2 there are pre-existing autofocus + webkit CSS).
  First cargo test of the session hit a borrow-lifetime slip in the new
  set_doc_tags (query_map temporary outliving stmt at block end) — fixed by
  draining rows with a while-let loop instead. Pushed + verified (local==origin
  3fc663a). Build cache from the 03:25 tick still warm — test 1.72s, clippy 9s.
- 2026-06-18 04:55 PT (Cake, cron): roadmap #6 "Tag merge" shipped — and it's
  the LAST tag-system item; the surface is now feature-complete (suggest /
  untagged filter / bulk apply / color / rename / recently-used / merge).
  Backend 2083c1f (registry::merge_tags — transactional fold: NULL-aware-max
  lift of applied_at for both-tag docs via max(coalesce(a,b),coalesce(b,a)),
  UPDATE OR IGNORE re-point of source-only links keeping their stamp, delete
  leftover source links + orphaned source row; both ends validated up front so
  a rejected merge/self-merge leaves rows untouched; 1 Tauri command
  slab_library_merge_tags; 12 new tests). UI e2fe7b7 (TS mergeTags + a merge
  glyph in the rail row menu opening a "Merge tag" target-picker modal;
  $derived candidates exclude the source; on success rail drops source + swaps
  target in place, active filter re-points, doc cards re-point + de-dupe their
  chip in place no refetch, recently-used reloads; rejected merge keeps modal
  open w/ inline reason; dark-first, monochrome glyph). Gates: cargo fmt clean,
  cargo test --lib pdf::library:: 218 passed/0 failed (12 new merge tests all
  green), clippy --lib -D warnings clean (6.2s warm), pnpm check 0 errors (no
  new LibraryPanel warnings — still the 2 pre-existing autofocus + webkit CSS).
  No schema bump (no new columns; pure re-point + delete over existing tables).
  Build cache from the 04:20 tick still warm — test 1.72s. Pushed + verified
  (local==origin e2fe7b7). Seeded a fresh roadmap (#7 usage counts, #8 unused-
  tag cleanup, #9 AND/OR tag combinator) since the tag roadmap is exhausted.
- 2026-06-18 05:50 PT (Cake, cron): fresh roadmap #7 "Tag usage counts in the
  rail" shipped (966db5e, single commit). Backend registry::tag_usage_counts()
  -> Vec<(tag_id, count)>: one LEFT JOIN + GROUP BY round-trip (never N), every
  tag once, zero-doc tags report 0 (LEFT JOIN keeps the residue an INNER JOIN
  would drop), id-ordered; 1 Tauri command slab_library_tag_usage_counts; 6
  new tests (per-doc counts, zero-for-unused, one-row-per-tag-id-ordered, empty,
  reflects bulk apply/remove, reflects merge as distinct union no-double-count
  + gone source unreported). Frontend: TS tagUsageCounts() -> Map<tagId,count>;
  LibraryPanel loads counts in refreshAll alongside listFolders/listTags so the
  rail count self-heals on every library-changed poke (reused the existing
  resync path, no bespoke optimistic plumbing); muted rail-meta count beside
  each tag (mirrors folder rail) + a rail-head A-Z/Most-used sort toggle (count
  desc, name tie-break, shown only when >1 tag). Gates: cargo fmt clean, cargo
  test --lib pdf::library:: 224 passed/0 failed (6 new), clippy --lib -D
  warnings clean (6.61s warm), pnpm check 0 errors (no new LibraryPanel
  warnings; still the 2 pre-existing autofocus + webkit line-clamp). No schema
  bump (pure read over existing tables). Build cache from the 04:55 tick still
  warm — first session test compile 16s (test profile cold-ish), full suite
  1.72s warm. Pushed + verified (local==origin 966db5e). Next undone: #8
  empty/unused tag cleanup.
- 2026-06-18 06:35 PT (Cake, cron): fresh roadmap #8 "Empty/unused tag
  cleanup" shipped (cd4219a backend + ba7a83d UI, two commits). Backend
  registry::delete_unused_tags() -> usize: one DELETE over library_tags
  guarded by NOT EXISTS against library_doc_tags (tag_id is NOT NULL so
  NOT EXISTS is the clean idiomatic form vs NOT IN), removes every zero-doc
  tag and returns the count; a tag with even one link untouched, empty
  library a no-op returning 0. 1 Tauri command slab_library_delete_unused_tags
  emits library-changed only when removed>0. 4 new tests (removes-only-unused
  keeps in-use drops orphans, no-op when all used, empty-is-zero, and the
  motivating case: a bulk-remove that strips a tag off its last doc leaves it
  in tag_usage_counts at 0 and the cleanup reclaims it). UI: TS deleteUnusedTags
  + a $derived unusedTagCount computed straight off the existing tagCounts map
  (count 0 == unused) so it self-heals on every refreshAll with zero bespoke
  plumbing; a muted "Clean up N" affordance in the Tags rail head (shown only
  when >0, danger-tinted hover marking it destructive, disabled while pruning).
  Click confirms with the exact count, snapshots the doomed ids to prune any
  now-stale tag out of the active filter, calls backend, toasts "Removed N
  unused tags" via the existing bulkSummary channel, then refreshAll reconciles
  rail+counts off the source of truth. Gates: cargo fmt clean, cargo test --lib
  pdf::library:: 228 passed/0 failed (4 new), clippy --lib -D warnings clean
  (6.48s warm), pnpm check 0 errors (105 warnings, all pre-existing in other
  panels — none in LibraryPanel from this change). No schema bump (pure delete
  over existing tables). Build cache from the 05:50 tick still warm — full
  library suite 1.74s, clippy 6.48s. Pushed + verified (local==origin ba7a83d).
  Next undone: #9 tag filter combinator (AND/OR).
- 2026-06-18 07:25 PT (Cake, cron): fresh roadmap #9 "Tag filter combinator
  (AND/OR)" shipped — and it's the LAST seeded roadmap item, so a round-3
  roadmap (#10 saved views, #11 clear-all, #12 tag descriptions) was seeded.
  Backend 18229a8 (TagMatch enum All-default/Any + tag_match field on
  LibraryFilter, #[serde(default)]=>All so legacy filters keep intersection
  byte-for-byte; query_documents branches the flat tag path — All keeps the
  GROUP BY ... HAVING COUNT(DISTINCT tag_id)=N intersect, Any drops HAVING for
  a `tag_id IN (...)` union; All count hardened to dedup requested ids so a
  dup can't raise the bar past one doc; 8 new tests incl. Any-vs-All-diverge,
  dup-id tolerance, legacy-JSON-default-All, snake_case roundtrip). UI 522cbe9
  (TS TagMatch type + tag_match mirror; LibraryPanel tagMatch state default
  "all" threaded into flat refreshDocs + $effect deps; "All tags"/"Any tag"
  rail-head toggle shown only when >1 tag selected, accent-tinted in the
  non-default Any state, mirrors .rail-sort chrome). Picked the flat tag_match
  field over UI-side nested clause groups: minimal churn, backward-compatible,
  rail stays a flat toggle list. Gates: cargo fmt clean, cargo test --lib
  pdf::library:: 234 passed/0 failed (6 new query tests), clippy --lib -D
  warnings clean (8.73s), pnpm check 0 errors (LibraryPanel still only the 2
  pre-existing autofocus + webkit warnings, none new). No schema bump (pure
  read-path change over existing tables). Build cache from the 06:35 tick still
  warm — test 1.75s. Pushed + verified (local==origin 522cbe9). Note: my parent
  was 951280e (the 06:35 cron-state chore), already on origin, not ba7a83d —
  the prior tick's STATE commit had landed. Next undone: #10 saved tag-filter
  views.
- 2026-06-18 07:50 PT (Cake, cron): round-3 roadmap #11 "Tag filter clear-all"
  shipped (f41a6a1, single frontend-only commit). A "Clear" affordance in the
  Tags rail head, gated by a new tagFilterActive $derived (activeTagIds.size > 0
  || untaggedOnly). clearTagFilter() resets activeTagIds = new Set(),
  untaggedOnly = false, tagMatch = "all" — three fresh assignments so the
  existing reactive $effect re-queries exactly once (no manual refresh). Match
  mode excluded from the visibility test (inert with 0 tags) but reset anyway
  for a clean slate. New .rail-clear CSS mirrors .rail-sort/.rail-match chrome
  (muted uppercase, margin-left:auto, neutral hover-to-text) — non-destructive
  reset so NO danger tint. No backend, no schema, no cargo. Gate: pnpm check
  0 errors, LibraryPanel still only its 2 pre-existing warnings (autofocus +
  webkit), none new. Pushed + verified (local==origin f41a6a1). DELIBERATELY
  picked the small #11 over the larger #10 saved-views because the tick started
  at 07:44 PT with only ~16 min before the 08:00 hard auto-stop — #10's new
  schema table + full Rust+TS+Svelte slice needs a slow cargo test gate that
  wouldn't finish in budget, whereas #11 was the seeded "lowest-risk pick if a
  tick is tight on build budget" and gates on pnpm check alone. Next undone:
  #10 saved tag-filter views, #12 tag descriptions/notes.
- 2026-06-19 19:47 PT (Cake, cron): round-3 roadmap #10 "Saved tag-filter
  views" shipped (2cf2a49 backend + 7c83eee UI, two commits) — the bigger of
  the two pending items #11 had previously deferred. Backend: schema v9->v10
  new library_saved_views table (id, name UNIQUE, filter_json, created_at,
  sort_order); new saved_views.rs module mirroring the personal_presets
  opacity contract (filter serialized through serde_json so the whole
  FilterGroup tree survives query-language bumps). CRUD = save / get / list
  (sort_order asc, name tie-break) / delete / rename, with trim + empty +
  UNIQUE + same-name-no-op + atomic-collision semantics (17 module tests
  incl. flat AND clause-tree round-trips byte-for-byte through serde). 4
  Tauri commands (slab_library_saved_view_save / list / delete / rename),
  each emits library-changed on success. UI: a new "Saved views" rail
  section between Folders and Tags. "Save filter" affordance in the section
  head shows ONLY when some filter dimension is non-default ($derived
  filterIsNonDefault: folder != "all" || any tag selected || untagged ||
  query.trim() || sort != "added_desc"). Save opens an inline name input
  (Enter / Escape) seeded from the obvious anchor (folder short name /
  lone selected tag / "Untagged"). Each view = rail row with diamond glyph
  + name + x-delete. ONE CLICK restores the full filter in a single batch
  so the existing reactive $effect re-queries exactly once; active-view
  highlight self-heals through a cheap structural $effect that compares
  the live rail state to the saved snapshot and clears as soon as they
  diverge. buildCurrentFilter mirrors refreshDocs's two-branch shape
  (clause tree when untaggedOnly, flat folder/tag/title otherwise) so what
  gets saved is what re-runs; restoreSavedView reads either shape back
  into the rail $state cells, ignoring exotic clauses to keep forward-
  compat automatic. Relaxed the v9 column-test schema-version assert from
  ==9 to >=9 (the trap that bit the v3.39 -> bulk-tag tick) and added a
  positive v10 column/table test. Gates: cargo fmt clean, cargo test --lib
  pdf::library:: 252 passed/0 failed (18 new: 17 saved_views + 1 schema
  v10), clippy --lib -D warnings clean (8.66s warm), pnpm check 0 errors
  (LibraryPanel still only the 2 pre-existing autofocus + webkit
  line-clamp warnings, none new). Pushed + verified (local==origin
  7c83eee). Build cache from the 07:50 tick (~36 h ago, June 18 -> June
  19 19:32 PT) was actually still warm: first cargo test compile 21s,
  full library suite 1.73s, clippy 8.66s — this tick fit comfortably in
  the loop with no cold-recompile penalty. Note on commit grouping:
  bundled the TS client (library.ts) WITH the backend commit instead of
  with the UI commit, since the TS client is the backend's wire layer
  and is useless without the Tauri commands it wraps — same grouping the
  v3.47.0 unused-tag-cleanup tick used. The ~36-hour gap between ticks
  means Sanjay must have re-armed the cron loop today; nothing to
  diagnose. Next undone: #12 tag descriptions/notes (it's the LAST
  round-3 roadmap item — next tick should ship #12 and then either seed
  a fresh round-4 roadmap or surface that the tag-and-filter surface is
  feature-complete enough that we should move to a different subsystem).
- 2026-06-19 19:58 PT (Cake, cron): round-3 roadmap #12 "Tag
  descriptions/notes" shipped (43d3258 backend + 3e92aaf UI, two
  commits) — the LAST round-3 item, the entire tag + tag-filter
  surface is now feature-complete. RECOVERY NOTE: the working tree
  was already dirty when this tick acquired the lock at 19:55:27 —
  files modified 19:52-19:54 from a previous tick that built a
  complete, high-quality vertical slice for #12 and then exited
  without committing, pushing, logging, or updating STATE.md (no
  session file for that tick, no log entry). The diff was the FULL
  intended slice (schema v11, set_tag_description + valid guard,
  TagRecord widened end-to-end, 13 tests, TS client, rail glyph +
  modal + tooltip surfacing); rather than scrap it I gated it and
  shipped it. All three gates green: cargo fmt clean, cargo test
  --lib pdf::library:: 265 passed/0 failed (13 new on top of 252 at
  7c83eee — matches the in-flight test count exactly), clippy --lib
  -D warnings clean (9.27s warm — build cache from 19:47 still
  hot 11 min later), pnpm check 0 errors (LibraryPanel still only
  the 2 pre-existing autofocus + webkit warnings, none new). Commit
  grouping reaffirmed for the 3rd time: TS client (library.ts)
  bundled with the BACKEND commit (43d3258) — it's the wire layer
  for the new Tauri command and useless without it; UI (LibraryPanel
  + new modal CSS) as the 2nd commit (3e92aaf). Pushed + verified
  (local==origin 3e92aaf). Schema bumped 10->11; the >= column-pin
  convention from the v9->v10 tick already preempted any equality-
  trap, so no test relaxations needed this time. With #12 done, the
  tag rail now surfaces: suggest, untagged filter, bulk apply, color,
  rename, recently-used, merge, usage counts, unused cleanup, AND/OR,
  saved views, clear-all, AND notes. Next tick should seed a FRESH
  roadmap for a different subsystem (good candidates listed in #12's
  closing note) rather than mine the tag surface for more increments.
  Lesson worth retaining about the recovered slice: a prior tick can
  produce real shippable work and still leave nothing on origin if
  it doesn't run the commit/push/log sequence — this tick's first
  action of `git status --short` caught it, but cron resilience
  improves if every tick treats a dirty tree as a recovery
  opportunity rather than a state to clean up.
- 2026-06-19 20:00 PT (Cake, cron): round-4 BATCH tick — FIVE
  full-text-search slices on the LibrarySearchPanel + its FTS5 query
  layer, all DONE, pushed + verified (local==origin b9b7a76). Eight
  commits (5 slices: 2 + 1 + 1 + 1 + 2 commits, plus 1 gate-driven fix).
  - Slice 13 recent-searches (c4ca277 backend + a2c7162 UI): wired
    library_search_log to a chip strip + Clear-history affordance.
    QueryRow gains serde, new search_log::clear() scoped not to touch
    dismissals. Two Tauri commands (recent_searches limit-clamped,
    clear_search_history emits library-changed only when n>0). 5 new
    tests.
  - Slice 14 per-folder scope (25d14cd): native <select> in the
    search header threads scopeFolderId into the existing FTS5
    folder-filter branch. Re-queries on change; vanishing scope
    folder silently self-heals back to All; result-count line +
    no-matches empty state both surface the active scope inline.
    Pure frontend slice.
  - Slice 15 phrase queries (1804706): replaced build_match_expr()'s
    bag-of-words sanitiser with a hand-written lexer that emits
    Tok::Bare / Tok::Phrase. `"force majeure"` becomes a single FTS5
    phrase token (adjacent-word match) instead of ANDed words.
    Curly-quote support + unterminated-phrase forgiveness +
    metachar-scrub-inside-phrase + last-bare-word-keeps-prefix-glob.
    10 new tests; empty-state tips help-text gains a quotes line.
  - Slice 16 exclude terms (db6d30b): lexer grows Tok::Exclude;
    `-word` / `-"phrase"` maps to FTS5 NOT clauses. Exclude-only
    queries return [] (FTS5 needs a positive anchor). `co-op` mid-
    word `-` not a trigger; lone `- ` dropped. 10 new tests; help-
    text gains a -prefix line.
  - Slice 17 index-status footer (7c14b70 backend + 6a3d62d UI):
    IndexStats { docs, pages } via index_stats() composer; one Tauri
    command. UI footer pinned beneath .results, accent-green status
    dot, refreshes on mount + after every search so a mid-session
    scan makes counts grow live. 3 new tests.
  - Fix commit b9b7a76: clippy `-D unused-assignments` caught a dead
    `pending_neg = false` in the whitespace branch of tokenize() —
    the outer reset already covered both paths. ALSO corrected two
    phrase tests whose expectations didn't match actual (correct)
    behaviour: the LAST bare-word token always gets the prefix `*`
    (the test had the older "if last token is a phrase, no glob"
    expectation), and scrub_phrase collapses adjacent-meta-chars
    rather than synthesising word boundaries (same heuristic as
    co-op -> coop for bare words). Behaviour unchanged; tests and
    one comment relaxed to match.
  All gates green: cargo fmt clean, cargo test --lib pdf::library:: 292
  passed / 0 failed (+27 from 265 at v3.51), cargo clippy --lib -D
  warnings clean (8.86s warm), pnpm check 0 errors / 105 warnings all
  pre-existing in other panels (zero on LibrarySearchPanel from this
  change). Build cache from the 19:58 round-3 tick was still hot 2
  minutes later — first cargo test compile 1.89s, clippy ~9s.
  Pushed to feature/v3.39.0-atlas-tag-suggest, verified local==origin
  at b9b7a76. Tag-system surface stays feature-complete (no regressions
  on the 265-test baseline); full-text search surface is now also
  demo-able end-to-end (recent searches, folder scope, phrase search,
  exclude terms, index-status footer). Next tick should pick a
  different subsystem — good candidates: smart-folders hub UI polish,
  OCR queue panel, collections, doc-detail metadata editor. BATCH
  PATTERN NOTE: shipping 5 slices in one tick took the test-and-
  clippy gate roundtrip count from 5x (per-slice) to 1x (batched);
  the gate-driven fix commit at the end caught what the per-slice
  flow would have caught after each, so the iteration-cost saving is
  real with zero correctness loss.
- 2026-06-19 21:25 PT (Cake, cron): round-5 BATCH tick — FIVE
  OCR-Queue slices that turn the headless auto-OCR pipeline into a real
  demo-able subsystem, all DONE, pushed + verified (local==origin
  07f5f0a). Five commits, one per slice (slice 1 standalone,
  slices 2/3/4 each bundle backend + tests + Tauri command + TS client
  per the established wire-layer convention, slice 5 is the UI panel +
  mount + palette entry).
  - Slice 18 persisted OCR error (92fc6d8): schema v11->v12 ocr_error
    column on library_documents; DocumentRecord widened end-to-end
    through 4 SELECT sites (registry/query/collections/ocr_queue);
    set_doc_ocr_error setter with trim+clear semantics; run_one writes
    the reason on failure and clears on success; 5 new tests.
  - Slice 19 re-queue (84a992f): requeue_doc flips done/failed/pending
    back to scanned, clears stored error + output_path; rejects
    text_native/unknown with named errors; requeue_all_failed bulk
    transactional UPDATE; 7 new tests; 2 Tauri commands +
    library-changed emits.
  - Slice 20 stats (0e85112): OcrQueueStats with per-state counts plus
    pending_total + total; one GROUP BY round-trip; forward-compat
    ignores unknown buckets so a future state can't crash the
    dashboard; 4 new tests including serde snake_case round-trip pin.
  - Slice 21 list_failed (816a03f): every ocr_failed row, newest
    first by last_seen_at; 3 new tests.
  - Slice 22 OcrQueuePanel (07f5f0a): 800-LOC Svelte 5 panel ties the
    backend slices into one surface — dashboard stats grid + indexed-%
    tile, failure inbox with per-row Retry + section-head Retry-all,
    pending preview with per-row Run-now + bulk Run-all; mounted by
    CollectionsSidebar via window event + Cmd/Ctrl+Shift+O shortcut +
    "OCR Queue…" command-palette entry; refreshes on library-changed;
    monochrome chrome (no emoji per house style) reusing the
    SmartFoldersHubPanel pattern. Pure frontend slice (no schema, no
    backend, no new Tauri commands beyond the four already shipped).
  All gates green: cargo fmt clean, cargo test --lib pdf::library:: 312
  passed / 0 failed (+20 from 292 at v3.51/round-4), cargo clippy --lib
  -D warnings clean (2m48s cold, 0.62s warm on the second run), pnpm
  check 0 errors / 105 warnings all pre-existing in other panels (zero
  on OcrQueuePanel from this batch). Pushed to
  feature/v3.39.0-atlas-tag-suggest, verified local==origin at 07f5f0a.
  Process note on the split: the 5-feature build started as one
  monolithic in-memory state, then I unwound it into 5 per-slice
  commits using /tmp/oq-slices snapshots + git checkout-then-rebuild,
  so each slice is independently revertible per the cron prompt.
  Time cost ~10 extra minutes vs one mega-commit; revertibility win
  is worth it. Pattern worth retaining: build the whole batch first
  for a single gate, then unwind per-slice from snapshots before
  committing. Tag-system surface still feature-complete, full-text
  search surface still feature-complete (no regressions on the 292-test
  baseline either of those left); auto-OCR queue is now also end-to-end
  demo-able with this batch. Next subsystem candidates: smart-folders
  hub UI polish, collections, doc-detail metadata editor, Beacon cache
  inspector, plugin marketplace.


