<script lang="ts">
  // HopperRulesEditor — v3.21.0 "Hopper Conditions" wow surface.
  //
  // For a given watch, lets the user write an ordered chain of routing
  // rules. Each rule says "IF this predicate matches, THEN route the
  // file through this recipe / output dir / rename pattern instead of
  // the watch defaults". First match wins; non-matches fall through.
  //
  // The wow is the right-hand live preview pane: shows up to five
  // candidate filenames (last log rows + a user-typed test filename)
  // and renders green/grey chips per rule indicating which would match.
  // Updates within ~150ms of any edit — the test runs entirely in Rust
  // against the in-flight, unsaved rule list, so users see "yes this
  // catches my tax PDFs" before clicking Save.
  //
  // Auto-save is debounced 600ms after the last edit; an explicit
  // "Saved ✓" toast confirms persistence.
  //
  // Persisted as JSON in `watches.rules_json`. See
  // `src-tauri/src/pdf/hopper/rules.rs` and `cmds.rs` for the backend.

  import { onMount } from "svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    slabHopperGetRules,
    slabHopperSetRules,
    slabHopperTestRules,
    slabHopperListRuns,
    slabHopperRuleCoverage,
    slabHopperSampleDrilldown,
    slabHopperExportDrilldownCsv,
    slabHopperExportDrilldownJson,
    slabHopperExportCoverageCsv,
    slabHopperExportCoverageJson,
    suggestDrilldownExportFilename,
    suggestCoverageExportFilename,
    fallthroughPercent,
    ruleMatchPercent,
    ruleCoverageDiagnostic,
    summarizeCoverage,
    summarizeCoverageHealth,
    ruleBucket,
    FALLTHROUGH_BUCKET,
    sampleBucketEquals,
    describeDrilldown,
    describeBucket,
    filterCoverageByDiagnostic,
    coverageHealthClickTarget,
    formatCoverageFilterSummary,
    COVERAGE_FILTER_KINDS,
    PREDICATE_KINDS,
    predicateLabel,
    emptyPredicate,
    emptyAction,
    formatPredicate,
    basename,
    type Rule,
    type RulePredicate,
    type RuleTestResult,
    type RuleCoverageReport,
    type CoverageDiagnosticFilter,
    type SampleBucket,
    type SampleDrilldown,
  } from "$lib/hopper";
  import HopperBackfillPanel from "./HopperBackfillPanel.svelte";

  // -------------------------------------------------------------------
  // Props
  // -------------------------------------------------------------------

  interface Props {
    watchId: number;
    watchSource: string;
    watchOutput: string;
    watchRecipeId: string | null;
  }

  let { watchId, watchSource, watchOutput, watchRecipeId }: Props = $props();

  // -------------------------------------------------------------------
  // State
  // -------------------------------------------------------------------

  let rules = $state<Rule[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let savedAt = $state<number | null>(null);
  let errorMsg = $state<string | null>(null);

  /** Files the preview pane evaluates against. Seeded from the
   *  watch's recent run log (so users see *their* files) plus a
   *  user-typed test filename for what-if scenarios. */
  let candidateFiles = $state<string[]>([]);
  let testFilename = $state("invoice_acme_2026-05-24.pdf");

  /** matchMatrix[fileIdx] = per-rule preview rows + final routing. */
  let matchMatrix = $state<
    {
      file: string;
      perRule: { matched: boolean }[];
      resolved: RuleTestResult;
    }[]
  >([]);
  let previewing = $state(false);

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  /** v3.40 Slice 82 — coverage analyzer state.
   *  The coverage panel below the rule chain answers "what would my
   *  rules do against my last N real files?". Hidden by default so
   *  the editor isn't busier than it needs to be on first load;
   *  toggled open via the "Coverage" button. */
  let coverageOpen = $state(false);
  let coverageLoading = $state(false);
  let coverage = $state<RuleCoverageReport | null>(null);
  let coverageError = $state<string | null>(null);
  let coverageSampleLimit = $state<number>(100);
  let coverageDebounce: ReturnType<typeof setTimeout> | null = null;

  /** v3.40 Slice 86 — sample drilldown state.
   *  When the user clicks a coverage row, we fetch the list of files
   *  in that bucket and surface them in an in-panel popover. State:
   *  - `openBucket`: which bucket's popover is currently open (null
   *    when nothing is). Identity-stable via `sampleBucketEquals`.
   *  - `drilldown`: the result for `openBucket`, or null while
   *    loading / on error.
   *  - `drilldownLoading` / `drilldownError`: loader status.
   *  - `drilldownPreviewCap`: payload-sized cap, separate from the
   *    coverage `sampleLimit`. Defaults to 25 (matches the server
   *    default); user can bump to see more without re-walking the
   *    sample input. */
  let openBucket = $state<SampleBucket | null>(null);
  let drilldown = $state<SampleDrilldown | null>(null);
  let drilldownLoading = $state(false);
  let drilldownError = $state<string | null>(null);
  let drilldownPreviewCap = $state<number>(25);

  /** v3.40 Slice 91 — drilldown CSV export state.
   *  `drilldownExporting` gates the Export button while the save
   *  dialog + write is in flight; `drilldownExportToast` carries
   *  the 4s success notice ("Exported 23 files as CSV (1.4 KB)")
   *  so the paralegal knows the file landed without staring at the
   *  disk. Slice 96 added the JSON variant; both formats share the
   *  same in-flight gate (a user can't open the save dialog twice)
   *  so a click on the JSON button mid-CSV-export gets ignored. */
  let drilldownExporting = $state(false);
  let drilldownExportToast = $state<string | null>(null);
  let drilldownExportToastTimer: ReturnType<typeof setTimeout> | null = null;

  /** v3.40 Slice 127 — coverage CSV+JSON export state.
   *  Mirrors the drilldown export state above: one in-flight gate
   *  (`coverageExporting`) so back-to-back CSV/JSON clicks don't
   *  pile up; one shared toast cell (`coverageExportToast`) for the
   *  "Exported N rules as CSV/JSON (X.X KB)" success notice;
   *  `coverageExportMenuOpen` controls the inline Export… popover
   *  visibility (CSV/JSON branches live in the popover, not as
   *  sibling buttons, to keep the cov-head action row compact). */
  let coverageExporting = $state(false);
  let coverageExportMenuOpen = $state(false);
  let coverageExportToast = $state<string | null>(null);
  let coverageExportToastTimer: ReturnType<typeof setTimeout> | null = null;

  /** v3.22.0 Hopper Loop — when true, the BackfillPanel overlay is
   *  mounted. Driven by the "Test on this folder" button below and by
   *  the public `openBackfill()` API used by the command palette. */
  let backfillOpen = $state(false);

  /** Public-ish entry point — used by the command palette /
   *  Cmd+Shift+B to open the backfill panel from anywhere. */
  export function openBackfill(): void {
    backfillOpen = true;
  }

  // -------------------------------------------------------------------
  // Load
  // -------------------------------------------------------------------

  onMount(async () => {
    try {
      rules = await slabHopperGetRules(watchId);
      // Seed preview files from this watch's recent runs.
      try {
        const recent = await slabHopperListRuns(20);
        const mine = recent
          .filter((r) => r.watch_id === watchId)
          .map((r) => basename(r.input_path));
        // De-dupe and cap at 4 (test filename is the 5th slot).
        candidateFiles = Array.from(new Set(mine)).slice(0, 4);
      } catch {
        candidateFiles = [];
      }
    } catch (e) {
      errorMsg = `Failed to load rules: ${String(e)}`;
    } finally {
      loading = false;
    }
    schedulePreview(0);
  });

  // -------------------------------------------------------------------
  // Derived: all filenames the preview should evaluate.
  // -------------------------------------------------------------------

  function allPreviewFiles(): string[] {
    const set = new Set<string>();
    for (const f of candidateFiles) if (f) set.add(f);
    if (testFilename.trim()) set.add(testFilename.trim());
    return Array.from(set).slice(0, 5);
  }

  // -------------------------------------------------------------------
  // Live preview — runs every rule against every file via the Rust
  // test endpoint (uses the in-flight unsaved `rules` array).
  // -------------------------------------------------------------------

  async function recomputePreview() {
    previewing = true;
    try {
      const files = allPreviewFiles();
      const next: typeof matchMatrix = [];
      for (const file of files) {
        // Per-rule individual evaluation: ask the server for each
        // single-rule chain, so we know which specific rule(s) matched.
        const perRule: { matched: boolean }[] = [];
        for (const r of rules) {
          const single = await slabHopperTestRules(watchId, file, {
            candidateRules: [r],
          });
          perRule.push({ matched: single.matched_index !== null });
        }
        // Then ask for the resolved routing (first-match-wins over full chain).
        const resolved = await slabHopperTestRules(watchId, file, {
          candidateRules: rules,
        });
        next.push({ file, perRule, resolved });
      }
      matchMatrix = next;
    } catch (e) {
      console.warn("hopper rules preview failed", e);
    } finally {
      previewing = false;
    }
  }

  function schedulePreview(delay = 150) {
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(recomputePreview, delay);
  }

  // -------------------------------------------------------------------
  // v3.40 Slice 82 — coverage loader
  // -------------------------------------------------------------------
  //
  // Pulled on demand when the user opens the coverage panel; refreshed
  // (debounced 400ms) on every rule edit so the bars reshape live
  // alongside the live preview chips. Server-side filtering caps the
  // sample set at [1, 1000]; the UI clamps the input to that range.

  async function refreshCoverage() {
    if (!coverageOpen) return;
    coverageLoading = true;
    coverageError = null;
    try {
      coverage = await slabHopperRuleCoverage(watchId, {
        candidateRules: rules,
        sampleLimit: coverageSampleLimit,
      });
    } catch (e) {
      coverageError = `Failed to compute coverage: ${String(e)}`;
    } finally {
      coverageLoading = false;
    }
  }

  function scheduleCoverage(delay = 400) {
    if (coverageDebounce) clearTimeout(coverageDebounce);
    coverageDebounce = setTimeout(refreshCoverage, delay);
  }

  function toggleCoverage() {
    coverageOpen = !coverageOpen;
    if (coverageOpen && coverage === null) {
      refreshCoverage();
    }
  }

  function setSampleLimit(next: number) {
    const clamped = Math.max(1, Math.min(1000, Math.round(next)));
    if (clamped === coverageSampleLimit) return;
    coverageSampleLimit = clamped;
    if (coverageOpen) scheduleCoverage(150);
  }

  /** v3.40 Slice 127 — chain-health summary derived from the
   *  in-flight coverage report. Null when no report has loaded yet
   *  (the UI hides the chip in that case). Reactive — recomputes
   *  whenever the coverage state cell changes, which means a rule
   *  edit + 400ms-debounced coverage refresh paints the new chip
   *  the moment the report lands. */
  let coverageHealth = $derived.by(() =>
    coverage ? summarizeCoverageHealth(coverage) : null,
  );

  /** v3.40 Slice 132 — diagnostic filter state for the coverage
   *  panel. The user clicks the chain-health chip (or a filter chip
   *  in the cov-filters row) to narrow the rule list to one
   *  diagnostic kind. State: `"all"` shows every rule (the default;
   *  no filter active); `"dead"` / `"shadowed"` / `"zero"` /
   *  `"healthy"` narrows to that bucket.
   *
   *  Driven through `setCoverageFilter` so all set-paths (chip
   *  click, filter chip click, Escape clear) share validation +
   *  exporter-menu dismissal. Cleared automatically when the chain
   *  has no rules (filter would be meaningless on an empty chain). */
  let coverageFilter = $state<CoverageDiagnosticFilter>("all");

  /** v3.40 Slice 132 — click target for the chain-health chip.
   *  Reactive over `coverageHealth` so a rule edit that flips the
   *  chip's kind (e.g. a freshly-shadowed rule promotes warn -> warn-
   *  shadowed) updates the click target without the chip's click
   *  handler having to re-derive it. Null disables the click. */
  let coverageHealthTarget = $derived.by(() =>
    coverageHealthClickTarget(coverageHealth),
  );

  /** v3.40 Slice 132 — the report the per-rule list actually renders.
   *  Identity transform when filter === "all" (the panel reads the
   *  raw coverage); a narrowing filter swaps in a filtered report
   *  with `fallthrough` + `total_samples` preserved so the bars
   *  beneath the per-rule rows + the fall-through synthetic row
   *  still reflect the underlying chain run. */
  let displayedCoverage = $derived.by(() => {
    if (coverage === null) return null;
    if (coverageFilter === "all") return coverage;
    return filterCoverageByDiagnostic(coverage, coverageFilter);
  });

  /** v3.40 Slice 132 — copy for the cov-filter sub-line.
   *  Renders "Showing all 6 rules" / "Showing 2 of 6 rules — dead" /
   *  "Showing 0 of 6 rules — dead" etc. via the slice-129 helper. */
  let coverageFilterSummary = $derived.by(() => {
    if (coverage === null) return "";
    return formatCoverageFilterSummary(
      coverageFilter,
      displayedCoverage?.rules.length ?? 0,
      coverage.rules.length,
    );
  });

  /** v3.40 Slice 132 — set the active diagnostic filter. Validates
   *  the input + auto-closes the export menu (the menu's text says
   *  "Export N rules" — the count is about to change). Setting the
   *  same filter twice is a no-op. */
  function setCoverageFilter(next: CoverageDiagnosticFilter) {
    if (next === coverageFilter) return;
    coverageFilter = next;
    coverageExportMenuOpen = false;
  }

  /** v3.40 Slice 132 — clear the active filter (reset to "all").
   *  Used by the Clear button + the Escape chain. */
  function clearCoverageFilter() {
    setCoverageFilter("all");
  }

  /** v3.40 Slice 132 — handle a click on the chain-health chip.
   *  Routes through coverageHealthClickTarget (slice 131) so the
   *  Svelte component never knows the priority chain — the helper
   *  owns the dead > shadowed > zero ordering. No-op when the
   *  target is null (healthy / empty / warn-only-fall-through). */
  function clickCoverageHealth() {
    if (coverageHealthTarget === null) return;
    setCoverageFilter(coverageHealthTarget);
  }

  // -------------------------------------------------------------------
  // v3.40 Slice 86 — sample drilldown
  // -------------------------------------------------------------------
  //
  // Click a coverage row to fetch the files in its bucket. Loader
  // shares wire semantics with `refreshCoverage` (same candidate
  // rules, same sample_limit) so the drilldown evaluates the EXACT
  // same chain + samples the coverage counted. Auto-refreshes when
  // the rule set changes (via `scheduleSave`) so the user sees the
  // bucket reshape live alongside the bars above.

  async function openDrilldown(bucket: SampleBucket) {
    // Toggle off if clicking the already-open bucket — matches the
    // Notion-style "click row, then click again to close" pattern.
    if (openBucket && sampleBucketEquals(openBucket, bucket)) {
      closeDrilldown();
      return;
    }
    openBucket = bucket;
    await refreshDrilldown();
  }

  async function refreshDrilldown() {
    if (openBucket === null) return;
    drilldownLoading = true;
    drilldownError = null;
    try {
      drilldown = await slabHopperSampleDrilldown(watchId, openBucket, {
        candidateRules: rules,
        sampleLimit: coverageSampleLimit,
        previewCap: drilldownPreviewCap,
      });
    } catch (e) {
      drilldownError = `Failed to load drilldown: ${String(e)}`;
      drilldown = null;
    } finally {
      drilldownLoading = false;
    }
  }

  function closeDrilldown() {
    openBucket = null;
    drilldown = null;
    drilldownError = null;
  }

  function setDrilldownPreviewCap(next: number) {
    const clamped = Math.max(1, Math.min(1000, Math.round(next)));
    if (clamped === drilldownPreviewCap) return;
    drilldownPreviewCap = clamped;
    if (openBucket !== null) refreshDrilldown();
  }

  // ─── Slice 91 / 96 — drilldown CSV+JSON export handler ──────────────
  //
  // Wires the popover's "Export CSV" + "Export JSON" buttons. Resolves
  // the suggested filename via the slice-90/95 helper (passing the
  // requested `ext`), opens the native save-as dialog, ships the
  // in-state drilldown + the current rule-name array to the slice-89
  // (CSV) / slice-94 (JSON) command, then shows a 4s success toast.
  // Cancellation (user dismisses the dialog) is a clean no-op — the
  // toast doesn't fire and `drilldownExporting` resets in the finally.
  //
  // Both formats share the same `drilldownExporting` gate, the same
  // toast cell, and the same in-state-snapshot semantics — the only
  // diffs are the suggested filename's suffix, the save dialog's
  // filter, and which Tauri command runs the write. Keeping the
  // dispatch in ONE function means a future audit-export change
  // (e.g. logging exports to a per-watch audit trail) lands once
  // and applies to both formats.
  //
  // We pass the LOADED drilldown verbatim (not re-fetched) so the
  // export matches exactly what the popover is currently rendering —
  // a background rule edit can't sneak in a different bucket between
  // "click Export" and "click Save".

  async function exportDrilldown(format: "csv" | "json") {
    if (drilldown === null || openBucket === null) return;
    if (drilldownExporting) return;
    drilldownExporting = true;
    drilldownExportToast = null;
    try {
      const ruleNames = rules.map((r) => r.name);
      const defaultPath = suggestDrilldownExportFilename(openBucket, ruleNames, {
        watchId,
        ext: format,
      });
      const filterName = format === "csv" ? "CSV" : "JSON";
      const title =
        format === "csv"
          ? "Export drilldown as CSV"
          : "Export drilldown as JSON";
      const target = await saveDialog({
        defaultPath,
        filters: [{ name: filterName, extensions: [format] }],
        title,
      });
      if (!target) return; // user cancelled
      const bytes =
        format === "csv"
          ? await slabHopperExportDrilldownCsv(drilldown, ruleNames, target)
          : await slabHopperExportDrilldownJson(drilldown, ruleNames, target);
      const fileCount = drilldown.samples.length;
      drilldownExportToast =
        `Exported ${fileCount} ${fileCount === 1 ? "file" : "files"}` +
        ` as ${filterName} (${formatBytes(bytes)})`;
      if (drilldownExportToastTimer) clearTimeout(drilldownExportToastTimer);
      drilldownExportToastTimer = setTimeout(() => {
        drilldownExportToast = null;
      }, 4000);
    } catch (e) {
      drilldownError = `Export failed: ${String(e)}`;
    } finally {
      drilldownExporting = false;
    }
  }

  /** Thin wrapper kept for the slice-91 "Export CSV" button's existing
   *  binding. Equivalent to `exportDrilldown("csv")`. */
  async function exportDrilldownCsv() {
    await exportDrilldown("csv");
  }

  /** Slice 96 - JSON variant. Same dispatch as exportDrilldownCsv but
   *  picks the slice-94 JSON command + envelope filename. */
  async function exportDrilldownJson() {
    await exportDrilldown("json");
  }

  // ─── Slice 127 — coverage CSV+JSON export handler ───────────────────
  //
  // Mirrors exportDrilldown exactly in shape: resolve the suggested
  // filename via the slice-125 helper (passing the requested `ext`),
  // open the native save-as dialog, ship the in-state coverage
  // verbatim (the LOADED report, NOT a re-fetch — same in-state-
  // snapshot semantics as the drilldown export so "click Export"
  // and "click Save" carry the SAME report a background rule edit
  // can't sneak past) to the slice-125 command, then flash a 4s
  // success toast.
  //
  // Cancellation (user dismisses the dialog) is a clean no-op — the
  // toast doesn't fire and `coverageExporting` resets in the
  // finally. Both formats share the same in-flight gate so the
  // user can't double-trigger or split the popover state.
  //
  // The Export… popover closes on a successful export so the panel
  // doesn't carry stale state; on cancellation it stays open so the
  // user can retry with the other format.

  async function exportCoverage(format: "csv" | "json") {
    if (coverage === null) return;
    if (coverageExporting) return;
    // Slice 132: the filtered view IS what gets exported — "export
    // what's visible" semantics match the existing
    // exportDrilldown behaviour (slices 91/96), and the filename
    // slug + the file's per-rule rows agree on what bucket of
    // rules they contain.
    const reportToExport = displayedCoverage ?? coverage;
    if (coverageExporting) return;
    coverageExporting = true;
    coverageExportToast = null;
    try {
      const defaultPath = suggestCoverageExportFilename({
        watchId,
        ext: format,
        // Slice 132: pin the filter slug into the filename when a
        // narrowing filter is active. "all" omits the slot for
        // back-compat with round-26 export filenames.
        filter: coverageFilter,
      });
      const filterName = format === "csv" ? "CSV" : "JSON";
      const title =
        format === "csv"
          ? "Export coverage as CSV"
          : "Export coverage as JSON";
      const target = await saveDialog({
        defaultPath,
        filters: [{ name: filterName, extensions: [format] }],
        title,
      });
      if (!target) return; // user cancelled
      const bytes =
        format === "csv"
          ? await slabHopperExportCoverageCsv(reportToExport, target)
          : await slabHopperExportCoverageJson(reportToExport, target);
      // Count rules in the body; the fall-through synthetic row is
      // implicit (the CSV adds it, the JSON envelope carries
      // fallthrough_count). Toast reads "rules" rather than "rows"
      // so the user knows what they exported without parsing the
      // file.
      const ruleCount = reportToExport.rules.length;
      const ruleNoun = ruleCount === 1 ? "rule" : "rules";
      const filterSuffix =
        coverageFilter === "all" ? "" : ` (filtered: ${coverageFilter})`;
      coverageExportToast =
        `Exported ${ruleCount} ${ruleNoun} as ${filterName} (${formatBytes(bytes)})${filterSuffix}`;
      coverageExportMenuOpen = false;
      if (coverageExportToastTimer) clearTimeout(coverageExportToastTimer);
      coverageExportToastTimer = setTimeout(() => {
        coverageExportToast = null;
      }, 4000);
    } catch (e) {
      coverageError = `Export failed: ${String(e)}`;
    } finally {
      coverageExporting = false;
    }
  }

  /** Local KB/MB formatter for the export toast. Mirrors
   *  RecentInstallsDrawer's formatBytes shape (1 decimal place,
   *  K/M/G suffixes). Kept local because the hopper.ts formatBytes
   *  is for predicate display ("size > 1 MB") and uses a slightly
   *  different signature. */
  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }

  /** Window-level Escape closes the drilldown popover. We don't
   *  install a click-outside listener — clicking elsewhere on the
   *  page is typically a deliberate navigation, and the explicit
   *  Close button is always visible inside the popover.
   *
   *  Slice 127: the coverage Export… popover is ALSO dismissed on
   *  Escape, taking priority over the drilldown popover so a user
   *  who opened both gets the most-recently-opened one closed first
   *  (Notion-style stacked-overlay chain).
   *
   *  Slice 132: the coverage filter clears LAST on Escape — after
   *  any open popover or drilldown is dismissed. A user with a
   *  filter active + an Export menu open hits Escape -> menu closes
   *  first; second Escape -> filter clears. The filter is the
   *  least-modal of the three states (it persists across rule
   *  edits), so it's the deepest stack entry. */
  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (coverageExportMenuOpen) {
      e.stopPropagation();
      coverageExportMenuOpen = false;
      return;
    }
    if (openBucket !== null) {
      e.stopPropagation();
      closeDrilldown();
      return;
    }
    if (coverageFilter !== "all") {
      e.stopPropagation();
      clearCoverageFilter();
    }
  }

  // -------------------------------------------------------------------
  // Save (debounced)
  // -------------------------------------------------------------------

  function scheduleSave() {
    schedulePreview();
    scheduleCoverage();
    // If a drilldown is open, refresh it too so the bucket reshapes
    // as the user edits. Cheap (cap of 25 samples vs 100 for coverage).
    if (openBucket !== null) refreshDrilldown();
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      saving = true;
      try {
        await slabHopperSetRules(watchId, rules);
        savedAt = Date.now();
      } catch (e) {
        errorMsg = `Save failed: ${String(e)}`;
      } finally {
        saving = false;
      }
    }, 600);
  }

  // -------------------------------------------------------------------
  // Rule mutations
  // -------------------------------------------------------------------

  function addRule() {
    rules = [
      ...rules,
      {
        name: `Rule ${rules.length + 1}`,
        predicate: { kind: "filename-glob", pattern: "*.pdf" },
        action: emptyAction(),
      },
    ];
    scheduleSave();
  }

  function removeRule(i: number) {
    rules = rules.filter((_, j) => j !== i);
    scheduleSave();
  }

  function moveUp(i: number) {
    if (i === 0) return;
    const next = rules.slice();
    [next[i - 1], next[i]] = [next[i], next[i - 1]];
    rules = next;
    scheduleSave();
  }

  function moveDown(i: number) {
    if (i >= rules.length - 1) return;
    const next = rules.slice();
    [next[i], next[i + 1]] = [next[i + 1], next[i]];
    rules = next;
    scheduleSave();
  }

  function setPredicateKind(i: number, kind: RulePredicate["kind"]) {
    rules[i].predicate = emptyPredicate(kind);
    rules = rules;
    scheduleSave();
  }

  function setNeedles(i: number, joined: string) {
    const p = rules[i].predicate;
    if (p.kind === "text-contains-all") {
      p.needles = joined
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      rules = rules;
      scheduleSave();
    }
  }

  // -------------------------------------------------------------------
  // Display helpers
  // -------------------------------------------------------------------

  function ruleNameAt(idx: number): string {
    return rules[idx]?.name?.trim() || `Rule ${idx + 1}`;
  }

  function savedLabel(): string {
    if (saving) return "Saving…";
    if (!savedAt) return "";
    const ago = Math.floor((Date.now() - savedAt) / 1000);
    if (ago < 2) return "Saved ✓";
    if (ago < 60) return `Saved ${ago}s ago`;
    return "Saved";
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<section class="rules-editor" data-tour="hopper-rules">
  <header class="head">
    <div>
      <h3>Routing rules</h3>
      <p class="sub">
        First match wins. Files that match no rule fall through to this
        watch's defaults
        {#if watchRecipeId}<span class="kbd">{watchRecipeId}</span>{/if}
        → <code>{basename(watchOutput) || watchOutput}</code>.
      </p>
    </div>
    <div class="head-actions">
      {#if savedLabel()}
        <span class="saved" class:err={errorMsg}>{savedLabel()}</span>
      {/if}
      <button
        class="ghost"
        onclick={() => (backfillOpen = true)}
        title="Apply these rules to PDFs already in this folder (Cmd+Shift+B)"
      >
        Test on this folder…
      </button>
      <button
        class="ghost"
        class:active={coverageOpen}
        onclick={toggleCoverage}
        title="Show per-rule coverage against the last {coverageSampleLimit} runs"
        aria-expanded={coverageOpen}
        aria-controls="hopper-coverage-panel"
      >
        Coverage{coverage ? ` · ${coverage.total_samples}` : ""}
      </button>
      <button class="primary" onclick={addRule}>+ Add rule</button>
    </div>
  </header>

  {#if errorMsg}
    <div class="error">{errorMsg}</div>
  {/if}

  <div class="split">
    <!-- ============== LEFT: rule chain ============== -->
    <ol class="rules">
      {#if loading}
        <li class="empty"><span class="spinner" /> Loading rules…</li>
      {:else if rules.length === 0}
        <li class="empty">
          <div class="empty-icon">🎯</div>
          <div>
            <strong>No rules yet.</strong>
            All files in <code>{basename(watchSource) || watchSource}</code>
            run the watch defaults.
            <br />
            Add a rule to route, say, tax PDFs to a different folder.
          </div>
        </li>
      {:else}
        {#each rules as rule, i (i)}
          <li class="rule">
            <div class="rule-head">
              <div class="reorder">
                <button
                  class="rbtn"
                  disabled={i === 0}
                  onclick={() => moveUp(i)}
                  title="Move up">↑</button
                >
                <button
                  class="rbtn"
                  disabled={i === rules.length - 1}
                  onclick={() => moveDown(i)}
                  title="Move down">↓</button
                >
              </div>
              <span class="prio">#{i + 1}</span>
              <input
                class="name"
                bind:value={rule.name}
                oninput={scheduleSave}
                placeholder="Rule name"
              />
              <button
                class="del"
                onclick={() => removeRule(i)}
                title="Delete this rule">×</button
              >
            </div>

            <div class="rule-body">
              <div class="row">
                <label>IF</label>
                <select
                  value={rule.predicate.kind}
                  onchange={(e) =>
                    setPredicateKind(
                      i,
                      (e.currentTarget as HTMLSelectElement)
                        .value as RulePredicate["kind"],
                    )}
                >
                  {#each PREDICATE_KINDS as k (k)}
                    <option value={k}>{predicateLabel(k)}</option>
                  {/each}
                </select>

                {#if rule.predicate.kind === "filename-glob" || rule.predicate.kind === "filename-regex"}
                  <input
                    class="grow mono"
                    bind:value={rule.predicate.pattern}
                    oninput={scheduleSave}
                    placeholder={rule.predicate.kind === "filename-glob"
                      ? "tax_*.pdf"
                      : "receipt|invoice"}
                  />
                {:else if rule.predicate.kind === "text-contains-all"}
                  <input
                    class="grow mono"
                    value={rule.predicate.needles.join(", ")}
                    oninput={(e) =>
                      setNeedles(
                        i,
                        (e.currentTarget as HTMLInputElement).value,
                      )}
                    placeholder="invoice, due date, total"
                  />
                {:else if rule.predicate.kind === "page-count-between"}
                  <input
                    type="number"
                    min="1"
                    bind:value={rule.predicate.min}
                    oninput={scheduleSave}
                  />
                  <span class="dash">to</span>
                  <input
                    type="number"
                    min="1"
                    bind:value={rule.predicate.max}
                    oninput={scheduleSave}
                  />
                {:else if rule.predicate.kind === "size-over"}
                  <input
                    type="number"
                    min="0"
                    step="1000"
                    bind:value={rule.predicate.bytes}
                    oninput={scheduleSave}
                  />
                  <span class="dash">bytes</span>
                {/if}
              </div>

              <div class="row">
                <label>THEN</label>
                <input
                  class="grow"
                  bind:value={rule.action.recipe_id}
                  oninput={scheduleSave}
                  placeholder="recipe id (blank = inherit)"
                />
                <input
                  class="grow"
                  bind:value={rule.action.output_dir}
                  oninput={scheduleSave}
                  placeholder="→ output dir (blank = inherit)"
                />
                <input
                  class="grow mono"
                  bind:value={rule.action.rename_pattern}
                  oninput={scheduleSave}
                  placeholder="{`{date}_{ai_title}.pdf`}"
                />
              </div>
              <div class="summary">{formatPredicate(rule.predicate)}</div>
            </div>
          </li>
        {/each}
      {/if}
    </ol>

    <!-- ============== RIGHT: live preview ============== -->
    <aside class="preview">
      <div class="preview-head">
        <h4>Live preview</h4>
        {#if previewing}<span class="dot"></span>{/if}
      </div>
      <p class="hint">
        Watching the last few files in
        <code>{basename(watchSource) || watchSource}</code>. Edits
        re-evaluate instantly.
      </p>

      <label class="testlabel">
        What-if filename
        <input
          class="mono"
          bind:value={testFilename}
          oninput={() => schedulePreview()}
          placeholder="test_filename.pdf"
        />
      </label>

      {#if matchMatrix.length === 0}
        <div class="preview-empty">
          {rules.length === 0
            ? "Add a rule to see how it routes."
            : "No candidate files yet — type one above."}
        </div>
      {:else}
        <ul class="matrix">
          {#each matchMatrix as row (row.file)}
            <li class="mrow">
              <div class="mfile mono">{row.file}</div>
              <div class="chips">
                {#each row.perRule as cell, idx (idx)}
                  <span
                    class="chip"
                    class:on={cell.matched}
                    class:win={row.resolved.matched_index === idx}
                    title={cell.matched
                      ? `Matches: ${ruleNameAt(idx)}`
                      : `Skipped: ${ruleNameAt(idx)}`}
                  >
                    {cell.matched ? "✓" : "·"}
                    <span class="chip-label">{ruleNameAt(idx)}</span>
                  </span>
                {/each}
              </div>
              <div class="dest">
                <span class="arrow">→</span>
                {#if row.resolved.matched_rule}
                  <span class="badge win"
                    >{row.resolved.matched_rule}</span
                  >
                {:else}
                  <span class="badge default">default</span>
                {/if}
                <code class="outdir"
                  >{basename(row.resolved.output_dir) ||
                    row.resolved.output_dir}</code
                >
                {#if row.resolved.recipe_id}
                  <span class="badge recipe">{row.resolved.recipe_id}</span>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>
  </div>

  {#if coverageOpen}
    <section
      id="hopper-coverage-panel"
      class="coverage"
      aria-label="Rule coverage analyzer"
    >
      <header class="cov-head">
        <div class="cov-title">
          <h4>Coverage</h4>
          <span class="cov-summary">
            {coverage ? summarizeCoverage(coverage) : "Loading…"}
          </span>
          {#if coverageHealth && coverageHealth.kind !== "empty"}
            {#if coverageHealthTarget !== null}
              <button
                type="button"
                class="cov-health cov-health-btn"
                class:healthy={coverageHealth.kind === "healthy"}
                class:warn={coverageHealth.kind === "warn"}
                class:critical={coverageHealth.kind === "critical"}
                class:active={coverageFilter === coverageHealthTarget}
                title={coverageFilter === coverageHealthTarget
                  ? `Clear ${coverageHealthTarget}-rule filter`
                  : `Show only ${coverageHealthTarget} rules — ${coverageHealth.text}`}
                aria-label="Chain health: {coverageHealth.text}. Click to filter to {coverageHealthTarget} rules."
                onclick={clickCoverageHealth}
              >{coverageHealth.text}</button>
            {:else}
              <span
                class="cov-health"
                class:healthy={coverageHealth.kind === "healthy"}
                class:warn={coverageHealth.kind === "warn"}
                class:critical={coverageHealth.kind === "critical"}
                title={coverageHealth.text}
                aria-label="Chain health: {coverageHealth.text}"
              >{coverageHealth.text}</span>
            {/if}
          {/if}
          {#if coverageLoading}
            <span class="dot"></span>
          {/if}
        </div>
        <div class="cov-actions">
          <label class="cov-limit">
            Sample size
            <input
              type="number"
              min="1"
              max="1000"
              step="10"
              value={coverageSampleLimit}
              onchange={(e) =>
                setSampleLimit(
                  Number((e.currentTarget as HTMLInputElement).value),
                )}
            />
          </label>
          <button
            class="ghost"
            onclick={refreshCoverage}
            disabled={coverageLoading}
            title="Recompute coverage"
          >Refresh</button>
          <div class="cov-export-anchor">
            <button
              type="button"
              class="ghost"
              onclick={() => (coverageExportMenuOpen = !coverageExportMenuOpen)}
              disabled={coverage === null
                || coverage.rules.length === 0
                || coverageExporting}
              aria-haspopup="menu"
              aria-expanded={coverageExportMenuOpen}
              title={coverage && coverage.rules.length > 0
                ? `Save the coverage report (${coverage.rules.length} rule${coverage.rules.length === 1 ? "" : "s"})`
                : "Add a rule before exporting the coverage report"}
            >{coverageExporting ? "Exporting…" : "Export…"}</button>
            {#if coverageExportMenuOpen && coverage && coverage.rules.length > 0}
              <div class="cov-export-menu" role="menu">
                <button
                  type="button"
                  role="menuitem"
                  onclick={() => void exportCoverage("csv")}
                  disabled={coverageExporting}
                  title="RFC-4180 CSV with per-rule rows + trailing fall-through row"
                >Export as CSV</button>
                <button
                  type="button"
                  role="menuitem"
                  onclick={() => void exportCoverage("json")}
                  disabled={coverageExporting}
                  title="Self-describing JSON envelope with chain-health totals"
                >Export as JSON</button>
              </div>
            {/if}
          </div>
        </div>
      </header>
      {#if coverageExportToast}
        <p class="cov-export-toast" role="status">{coverageExportToast}</p>
      {/if}

      {#if coverageError}
        <div class="cov-error" role="alert">{coverageError}</div>
      {/if}

      {#if coverage}
        {#if coverage.total_samples === 0}
          <div class="cov-empty">
            No recent runs in this watch's log yet. Drop a file into
            <code>{basename(watchSource) || watchSource}</code>
            and re-open coverage to see the rules light up.
          </div>
        {:else if coverage.rules.length === 0}
          <div class="cov-empty">
            No rules yet — every sample falls through to the watch
            defaults. Add a rule above to start routing.
          </div>
        {:else}
          <!-- Slice 132 — diagnostic filter row. The chain-health
               chip (above) is the one-click affordance; this row
               surfaces the explicit "narrow to" controls + the
               clear button. Always visible when the chain has any
               rules so a user can audit by diagnostic kind even on
               a "healthy" chain (e.g. before-and-after a rule edit). -->
          <div class="cov-filters" role="group" aria-label="Filter rules by diagnostic">
            {#each COVERAGE_FILTER_KINDS as kind (kind)}
              {@const k = kind as CoverageDiagnosticFilter}
              <button
                type="button"
                class="cov-filter-chip"
                class:active={coverageFilter === k}
                onclick={() => setCoverageFilter(k)}
                title={k === "all"
                  ? "Show every rule in the chain"
                  : `Narrow to rules whose diagnostic is "${k}"`}
                aria-pressed={coverageFilter === k}
              >{k === "all" ? "All" : k.charAt(0).toUpperCase() + k.slice(1)}</button>
            {/each}
            <span class="cov-filter-summary" aria-live="polite">
              {coverageFilterSummary}
            </span>
            {#if coverageFilter !== "all"}
              <button
                type="button"
                class="cov-filter-clear"
                onclick={clearCoverageFilter}
                title="Reset to show every rule (Esc)"
              >Clear filter</button>
            {/if}
          </div>
          {#if displayedCoverage && displayedCoverage.rules.length === 0}
            <div class="cov-empty cov-empty-filter">
              No rules match the
              <strong>{coverageFilter}</strong> filter — every rule
              is in a different diagnostic bucket. Try another
              filter, or
              <button
                type="button"
                class="link"
                onclick={clearCoverageFilter}
              >clear the filter</button>.
            </div>
          {/if}
          <ul class="cov-list" class:filtered={coverageFilter !== "all"}>
            {#each displayedCoverage?.rules ?? [] as row (row.index)}
              {@const diagnostic = ruleCoverageDiagnostic(row)}
              {@const firstPct = ruleMatchPercent(row, coverage)}
              {@const wouldPct = coverage.total_samples
                ? (row.would_match / coverage.total_samples) * 100
                : 0}
              {@const bucket = ruleBucket(row.index)}
              {@const isOpen = openBucket !== null && sampleBucketEquals(openBucket, bucket)}
              <li class="cov-row-wrap">
                <button
                  type="button"
                  class="cov-row"
                  class:dead={diagnostic === "dead"}
                  class:open={isOpen}
                  onclick={() => openDrilldown(bucket)}
                  aria-expanded={isOpen}
                  aria-controls="hopper-drilldown-rule-{row.index}"
                  title={row.first_match === 0
                    ? `${row.name || "(unnamed)"} — no samples in this bucket; click for empty-state details`
                    : `Show the ${row.first_match} sample${row.first_match === 1 ? "" : "s"} this rule routed`}
                >
                  <div class="cov-name">
                    <span class="cov-idx">#{row.index + 1}</span>
                    <span class="cov-rname">{row.name || "(unnamed)"}</span>
                    {#if diagnostic === "dead"}
                      <span class="cov-chip dead" title="No samples reach this rule — shadowed by an earlier rule. Reorder it earlier to fire."
                        >Dead at position</span>
                    {:else if diagnostic === "zero"}
                      <span class="cov-chip zero" title="Predicate too narrow — matches none of the sampled files in isolation either."
                        >No matches</span>
                    {:else if diagnostic === "shadowed"}
                      <span class="cov-chip shadow" title="Predicate matches {row.would_match} samples in isolation but only routes {row.first_match} at this position — partly shadowed by an earlier rule."
                        >Partly shadowed</span>
                    {/if}
                  </div>
                  <div class="cov-bar">
                    <div
                      class="cov-bar-would"
                      style="width: {wouldPct}%"
                      title="Would match {row.would_match} samples in isolation ({wouldPct.toFixed(0)}%)"
                    ></div>
                    <div
                      class="cov-bar-first"
                      style="width: {firstPct}%"
                      title="Routes {row.first_match} samples at this position ({firstPct.toFixed(0)}%)"
                    ></div>
                  </div>
                  <div class="cov-counts">
                    <span class="cov-num">{row.first_match}</span>
                    <span class="cov-sep">/</span>
                    <span class="cov-num would">{row.would_match}</span>
                    <span class="cov-chev" aria-hidden="true">{isOpen ? "▾" : "▸"}</span>
                  </div>
                </button>
                {#if isOpen}
                  <div
                    id="hopper-drilldown-rule-{row.index}"
                    class="cov-drilldown"
                    role="region"
                    aria-label="Files in {describeBucket(bucket, rules.map((r) => r.name))}"
                  >
                    {@render renderDrilldownBody()}
                  </div>
                {/if}
              </li>
            {/each}
            {#if coverageFilter === "all"}
              <!-- Slice 132 — fall-through synthetic row only renders
                   when the diagnostic filter is "all". A narrowing
                   filter narrows to RULES, and the fall-through is
                   not a rule bucket — hiding it while filtered
                   keeps the panel's focus on the user's question
                   ("which rules are dead?") and avoids confusing
                   the per-rule export filename's "_dead_" slug
                   with the unrelated fall-through count. -->
              <li class="cov-row-wrap">
              <button
                type="button"
                class="cov-row fallthrough"
                class:open={openBucket !== null && sampleBucketEquals(openBucket, FALLTHROUGH_BUCKET)}
                onclick={() => openDrilldown(FALLTHROUGH_BUCKET)}
                aria-expanded={openBucket !== null && sampleBucketEquals(openBucket, FALLTHROUGH_BUCKET)}
                aria-controls="hopper-drilldown-fallthrough"
                title="Show the {coverage.fallthrough} file{coverage.fallthrough === 1 ? '' : 's'} that fell through to the watch defaults"
              >
                <div class="cov-name">
                  <span class="cov-idx">—</span>
                  <span class="cov-rname">Fall-through to watch defaults</span>
                </div>
                <div class="cov-bar">
                  <div
                    class="cov-bar-fall"
                    style="width: {fallthroughPercent(coverage)}%"
                    title="{coverage.fallthrough} samples ({fallthroughPercent(coverage).toFixed(0)}%) fell through to the watch defaults"
                  ></div>
                </div>
                <div class="cov-counts">
                  <span class="cov-num fall">{coverage.fallthrough}</span>
                  <span class="cov-chev" aria-hidden="true">{openBucket !== null && sampleBucketEquals(openBucket, FALLTHROUGH_BUCKET) ? "▾" : "▸"}</span>
                </div>
              </button>
              {#if openBucket !== null && sampleBucketEquals(openBucket, FALLTHROUGH_BUCKET)}
                <div
                  id="hopper-drilldown-fallthrough"
                  class="cov-drilldown"
                  role="region"
                  aria-label="Files that fell through to the watch defaults"
                >
                  {@render renderDrilldownBody()}
                </div>
              {/if}
            </li>
            {/if}
          </ul>
          <p class="cov-legend">
            Solid bar = samples this rule actually routes (first-match).
            Lighter overlay = samples it would catch in isolation.
            <strong>Dead at position</strong> means the rule never fires
            because an earlier rule wins first; move it up to fix.
            Click any row to see which files landed in that bucket.
          </p>
        {/if}
      {/if}
    </section>
  {/if}
</section>

{#snippet renderDrilldownBody()}
  <header class="drill-head">
    <div class="drill-title">
      <strong>{openBucket ? describeBucket(openBucket, rules.map((r) => r.name)) : ""}</strong>
      <span class="drill-sub">
        {drilldown ? describeDrilldown(drilldown) : (drilldownLoading ? "Loading…" : "")}
      </span>
    </div>
    <div class="drill-actions">
      <label class="drill-cap">
        Show
        <input
          type="number"
          min="1"
          max="1000"
          step="5"
          value={drilldownPreviewCap}
          onchange={(e) =>
            setDrilldownPreviewCap(
              Number((e.currentTarget as HTMLInputElement).value),
            )}
        />
      </label>
      <button
        type="button"
        class="ghost mini"
        onclick={() => void refreshDrilldown()}
        disabled={drilldownLoading}
        title="Reload this bucket"
      >Reload</button>
      <button
        type="button"
        class="ghost mini"
        onclick={() => void exportDrilldownCsv()}
        disabled={drilldownExporting
          || drilldownLoading
          || drilldown === null
          || drilldown.samples.length === 0}
        title={drilldown && drilldown.samples.length > 0
          ? `Save ${drilldown.samples.length} ${drilldown.samples.length === 1 ? "file" : "files"} as CSV`
          : "No files in this bucket to export"}
      >{drilldownExporting ? "Exporting…" : "Export CSV"}</button>
      <button
        type="button"
        class="ghost mini"
        onclick={() => void exportDrilldownJson()}
        disabled={drilldownExporting
          || drilldownLoading
          || drilldown === null
          || drilldown.samples.length === 0}
        title={drilldown && drilldown.samples.length > 0
          ? `Save ${drilldown.samples.length} ${drilldown.samples.length === 1 ? "file" : "files"} as JSON envelope`
          : "No files in this bucket to export"}
      >{drilldownExporting ? "Exporting…" : "Export JSON"}</button>
      <button
        type="button"
        class="ghost mini"
        onclick={closeDrilldown}
        title="Close drilldown (Esc)"
        aria-label="Close drilldown"
      >Close</button>
    </div>
  </header>
  {#if drilldownExportToast}
    <p class="drill-export-toast" role="status">{drilldownExportToast}</p>
  {/if}
  {#if drilldownError}
    <p class="drill-error" role="alert">{drilldownError}</p>
  {:else if drilldownLoading && !drilldown}
    <p class="drill-loading">Loading sample files…</p>
  {:else if drilldown}
    {#if drilldown.samples.length === 0}
      <p class="drill-empty">
        {#if openBucket?.kind === "fallthrough"}
          No samples fell through — every recent file matched at least one rule.
        {:else}
          No samples in this bucket. Either no recent files matched this rule, or
          an earlier rule won first (look for the "Dead at position" / "Partly shadowed"
          chips above).
        {/if}
      </p>
    {:else}
      <ul class="drill-list" aria-label="Files in this bucket">
        {#each drilldown.samples as s (s.filename)}
          <li class="drill-item" title={s.filename}>
            <span class="drill-glyph" aria-hidden="true">▸</span>
            <span class="drill-fname">{s.filename}</span>
          </li>
        {/each}
      </ul>
      {#if drilldown.truncated}
        <p class="drill-trunc">
          Showing {drilldown.samples.length} of {drilldown.total_in_bucket}.
          Increase “Show” above to see more.
        </p>
      {/if}
    {/if}
  {/if}
{/snippet}

{#if backfillOpen}
  <HopperBackfillPanel
    {watchId}
    watchSource={watchSource}
    onClose={() => (backfillOpen = false)}
  />
{/if}

<style>
  .rules-editor {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 16px 18px;
    backdrop-filter: blur(24px) saturate(140%);
    margin: 12px 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 12px;
  }
  .head h3 {
    margin: 0;
    font-size: 14px;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-strong, #fff);
  }
  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-muted, #999);
  }
  .head-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .saved {
    font-size: 11px;
    color: rgba(120, 230, 160, 0.9);
    font-variant-numeric: tabular-nums;
  }
  .saved.err {
    color: rgb(240, 120, 120);
  }
  .primary {
    background: rgba(110, 165, 255, 0.18);
    border: 1px solid rgba(110, 165, 255, 0.4);
    color: rgb(180, 210, 255);
    border-radius: 8px;
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .primary:hover {
    background: rgba(110, 165, 255, 0.28);
  }
  .error {
    background: rgba(240, 80, 80, 0.12);
    border: 1px solid rgba(240, 80, 80, 0.3);
    color: rgb(255, 180, 180);
    padding: 6px 10px;
    border-radius: 8px;
    margin-bottom: 10px;
    font-size: 12px;
  }

  .split {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(280px, 1fr);
    gap: 16px;
  }
  @media (max-width: 1100px) {
    .split {
      grid-template-columns: 1fr;
    }
  }

  .rules {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .empty {
    display: flex;
    gap: 12px;
    padding: 14px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px dashed rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    color: var(--text-muted, #aaa);
    font-size: 13px;
  }
  .empty-icon {
    font-size: 22px;
  }
  .empty code {
    color: rgb(180, 210, 255);
  }

  .rule {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rule-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .reorder {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .rbtn {
    background: none;
    border: none;
    color: var(--text-muted, #888);
    cursor: pointer;
    font-size: 10px;
    line-height: 1;
    padding: 1px 4px;
  }
  .rbtn:disabled {
    opacity: 0.2;
    cursor: default;
  }
  .prio {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: rgba(110, 165, 255, 0.8);
    width: 28px;
  }
  .name {
    flex: 1;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--text-strong, #fff);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 13px;
  }
  .del {
    background: none;
    border: none;
    color: rgba(240, 120, 120, 0.7);
    cursor: pointer;
    font-size: 18px;
    width: 22px;
    height: 22px;
    border-radius: 4px;
  }
  .del:hover {
    background: rgba(240, 80, 80, 0.15);
    color: rgb(255, 180, 180);
  }

  .rule-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 0 2px 36px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .row label {
    font-family: ui-monospace, monospace;
    font-size: 10px;
    color: var(--text-muted, #888);
    width: 30px;
    letter-spacing: 0.05em;
  }
  .row input,
  .row select {
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--text-strong, #fff);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 12px;
  }
  .row input[type="number"] {
    width: 86px;
  }
  .grow {
    flex: 1 1 120px;
    min-width: 0;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", monospace;
  }
  .dash {
    color: var(--text-muted, #888);
    font-size: 11px;
  }
  .summary {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: rgba(180, 210, 255, 0.6);
    padding-left: 30px;
  }

  /* ---------- preview pane ---------- */
  .preview {
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 12px;
    align-self: start;
  }
  .preview-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }
  .preview-head h4 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-strong, #fff);
  }
  .preview .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: rgb(120, 230, 160);
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
  }
  .hint {
    font-size: 11px;
    color: var(--text-muted, #999);
    margin: 0 0 10px;
  }
  .hint code {
    color: rgb(180, 210, 255);
    font-size: 11px;
  }
  .testlabel {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 10px;
    color: var(--text-muted, #888);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 10px;
  }
  .testlabel input {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--text-strong, #fff);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }
  .preview-empty {
    color: var(--text-muted, #777);
    font-size: 12px;
    font-style: italic;
    padding: 10px 0;
  }
  .matrix {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .mrow {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
  }
  .mfile {
    font-size: 12px;
    color: rgb(220, 230, 240);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10px;
    background: rgba(160, 160, 160, 0.12);
    color: var(--text-muted, #888);
    border: 1px solid rgba(160, 160, 160, 0.18);
    transition: all 0.15s ease;
  }
  .chip.on {
    background: rgba(80, 200, 120, 0.22);
    color: rgb(140, 240, 180);
    border-color: rgba(80, 200, 120, 0.4);
  }
  .chip.win {
    background: rgba(80, 200, 120, 0.42);
    color: #fff;
    border-color: rgb(120, 230, 160);
    box-shadow: 0 0 0 2px rgba(80, 200, 120, 0.15);
  }
  .chip-label {
    font-family: ui-monospace, monospace;
    font-size: 10px;
  }
  .dest {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 11px;
  }
  .arrow {
    color: var(--text-muted, #888);
  }
  .badge {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
  }
  .badge.win {
    background: rgba(80, 200, 120, 0.2);
    color: rgb(140, 240, 180);
  }
  .badge.default {
    background: rgba(160, 160, 160, 0.18);
    color: var(--text-muted, #aaa);
  }
  .badge.recipe {
    background: rgba(255, 200, 100, 0.18);
    color: rgb(255, 220, 150);
  }
  .outdir {
    color: rgb(180, 210, 255);
    font-size: 11px;
  }
  .kbd {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(255, 200, 100, 0.18);
    color: rgb(255, 220, 150);
    margin: 0 2px;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: rgb(180, 210, 255);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* v3.40 Slice 82 — coverage panel ---------------------------------- */
  .coverage {
    margin-top: 14px;
    padding: 12px 14px 14px;
    background: rgba(255, 255, 255, 0.025);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 12px;
    animation: cov-in 140ms ease-out;
  }
  @keyframes cov-in {
    from { opacity: 0; transform: translateY(-2px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .cov-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
    flex-wrap: wrap;
  }
  .cov-title {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .cov-title h4 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-strong, #fff);
  }
  .cov-summary {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.62);
  }
  .cov-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .cov-limit {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
  }
  .cov-limit input {
    width: 64px;
    padding: 4px 6px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    color: inherit;
    font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }

  /* Slice 127 — chain-health chip beside the summary line. Three
     visual treatments: neutral healthy, warm warn, danger critical.
     Empty kind is filtered upstream so the chip never renders for
     "no samples yet" — the cov-summary's "Loading…" / "No recent
     runs" copy carries that state. */
  .cov-health {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid transparent;
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.78);
    font-weight: 500;
    letter-spacing: 0.01em;
  }
  .cov-health.healthy {
    background: color-mix(in srgb, #6edc9a 14%, transparent);
    border-color: color-mix(in srgb, #6edc9a 32%, transparent);
    color: #b6f1cd;
  }
  .cov-health.warn {
    background: color-mix(in srgb, #ffb648 14%, transparent);
    border-color: color-mix(in srgb, #ffb648 32%, transparent);
    color: #ffd9a2;
  }
  .cov-health.critical {
    background: color-mix(in srgb, #ff5d6c 16%, transparent);
    border-color: color-mix(in srgb, #ff5d6c 38%, transparent);
    color: #ffb8be;
  }

  /* Slice 132 — chain-health chip as a button when clickable. The
     chip stays styled as a chip (same color treatment via class:);
     adding `cov-health-btn` overrides browser button defaults and
     gives it a pointer cursor + an `active` state when the chip's
     filter is currently selected. */
  .cov-health-btn {
    background: rgba(255, 255, 255, 0);
    font-family: inherit;
    line-height: 1;
    cursor: pointer;
    /* Reset default button border so the class:critical/warn/healthy
       color treatments take effect. */
    border-style: solid;
    border-width: 1px;
    transition:
      background 120ms ease,
      border-color 120ms ease,
      transform 60ms ease;
  }
  .cov-health-btn:hover {
    transform: translateY(-1px);
  }
  .cov-health-btn:focus-visible {
    outline: 2px solid color-mix(in srgb, #7c8cff 60%, transparent);
    outline-offset: 2px;
  }
  /* Filter-active chip — same color as the kind but with the
     active ring so the user knows the click landed somewhere. */
  .cov-health-btn.active {
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18);
  }

  /* Slice 132 — diagnostic filter row. Inline row of chip-style
     toggle buttons + the live "Showing X of Y" summary + a Clear
     button when a narrowing filter is active. Sits above the
     cov-list and below the cov-actions row. */
  .cov-filters {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }
  .cov-filter-chip {
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    padding: 4px 9px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    text-transform: lowercase;
    letter-spacing: 0.01em;
    transition:
      background 120ms ease,
      border-color 120ms ease,
      color 120ms ease;
  }
  .cov-filter-chip:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.92);
  }
  .cov-filter-chip:focus-visible {
    outline: 2px solid color-mix(in srgb, #7c8cff 60%, transparent);
    outline-offset: 2px;
  }
  .cov-filter-chip.active {
    background: color-mix(in srgb, #7c8cff 18%, transparent);
    border-color: color-mix(in srgb, #7c8cff 42%, transparent);
    color: #d4dcff;
  }
  .cov-filter-summary {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
    margin-left: 4px;
    /* push to next visual cluster on flex-wrap */
    flex: 0 0 auto;
  }
  .cov-filter-clear {
    font-family: inherit;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    margin-left: auto;
  }
  .cov-filter-clear:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.92);
  }
  /* Filtered list gets a subtle accent rail on the left so a user
     glancing at the panel knows the list isn't the full chain. */
  .cov-list.filtered {
    border-left: 2px solid color-mix(in srgb, #7c8cff 38%, transparent);
    padding-left: 6px;
  }
  /* Empty state when the filter has no matching rules — explains
     what's happening + gives the clear-filter affordance inline. */
  .cov-empty-filter {
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px dashed rgba(255, 255, 255, 0.14);
    border-radius: 6px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
    margin-bottom: 8px;
  }
  .cov-empty-filter .link {
    background: none;
    border: none;
    color: #b6c4ff;
    cursor: pointer;
    padding: 0;
    font: inherit;
    text-decoration: underline;
  }
  .cov-empty-filter .link:hover {
    color: #d4dcff;
  }

  /* Slice 127 — Export popover anchor + menu. Same shape as the
     drilldown popover Export… affordance but lives on the coverage
     section's action row (cov-actions). The anchor is position:
     relative so the absolute-positioned menu lands right-aligned to
     the trigger; menu z-index sits above the drilldown popover so
     the user can layer them. */
  .cov-export-anchor {
    position: relative;
    display: inline-block;
  }
  .cov-export-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 12;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px;
    background: rgba(20, 24, 36, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    min-width: 160px;
  }
  .cov-export-menu button {
    background: transparent;
    color: rgba(255, 255, 255, 0.86);
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 6px 10px;
    text-align: left;
    font-size: 12px;
    cursor: pointer;
  }
  .cov-export-menu button:hover {
    background: rgba(124, 140, 255, 0.14);
    border-color: rgba(124, 140, 255, 0.32);
  }
  .cov-export-menu button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .cov-export-toast {
    margin: 0 0 8px;
    padding: 6px 10px;
    font-size: 11px;
    color: rgb(170, 230, 195);
    background: rgba(110, 220, 154, 0.1);
    border: 1px solid rgba(110, 220, 154, 0.22);
    border-radius: 6px;
    font-variant-numeric: tabular-nums;
    animation: cov-export-toast-fade-in 0.16s ease-out;
  }
  @keyframes cov-export-toast-fade-in {
    from { opacity: 0; transform: translateY(-2px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .cov-error {
    margin-bottom: 8px;
    padding: 6px 10px;
    background: color-mix(in srgb, #ff5d6c 14%, transparent);
    border: 1px solid color-mix(in srgb, #ff5d6c 38%, transparent);
    color: #ffb8be;
    border-radius: 6px;
    font-size: 12px;
  }
  .cov-empty {
    padding: 18px 12px;
    text-align: center;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.55);
  }
  .cov-empty code {
    font-family: ui-monospace, SFMono-Regular, monospace;
    background: rgba(255, 255, 255, 0.06);
    padding: 1px 5px;
    border-radius: 4px;
  }

  .cov-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .cov-row-wrap {
    list-style: none;
    display: flex;
    flex-direction: column;
  }
  .cov-row {
    display: grid;
    grid-template-columns: minmax(180px, 22%) 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid transparent;
    transition: background 120ms, border-color 120ms;
    /* Button reset so .cov-row reads like a row, not a button. */
    width: 100%;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .cov-row:hover { background: rgba(255, 255, 255, 0.04); }
  .cov-row:focus-visible {
    outline: none;
    border-color: rgba(124, 140, 255, 0.55);
    box-shadow: 0 0 0 2px rgba(124, 140, 255, 0.18);
  }
  .cov-row.open {
    background: rgba(124, 140, 255, 0.08);
    border-color: rgba(124, 140, 255, 0.32);
  }
  .cov-row.dead {
    border-color: color-mix(in srgb, #ff7b56 40%, transparent);
    background: color-mix(in srgb, #ff7b56 6%, transparent);
  }
  .cov-row.dead.open {
    border-color: color-mix(in srgb, #ff7b56 70%, transparent);
    background: color-mix(in srgb, #ff7b56 12%, transparent);
  }
  .cov-row.fallthrough {
    margin-top: 6px;
    background: rgba(255, 255, 255, 0.015);
    border-top: 1px dashed rgba(255, 255, 255, 0.1);
    border-radius: 0;
    padding-top: 10px;
  }
  .cov-row.fallthrough.open {
    background: rgba(124, 140, 255, 0.06);
    border-color: rgba(124, 140, 255, 0.28);
    border-radius: 0 0 8px 8px;
    border-top-style: solid;
  }

  .cov-name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .cov-idx {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    min-width: 22px;
  }
  .cov-rname {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cov-chip {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .cov-chip.dead {
    background: color-mix(in srgb, #ff7b56 28%, transparent);
    color: #ffc1a8;
    border: 1px solid color-mix(in srgb, #ff7b56 55%, transparent);
  }
  .cov-chip.shadow {
    background: color-mix(in srgb, #d9b04c 22%, transparent);
    color: #f4d986;
    border: 1px solid color-mix(in srgb, #d9b04c 45%, transparent);
  }
  .cov-chip.zero {
    background: color-mix(in srgb, white 8%, transparent);
    color: rgba(255, 255, 255, 0.55);
    border: 1px solid color-mix(in srgb, white 15%, transparent);
  }

  .cov-bar {
    position: relative;
    height: 12px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 6px;
    overflow: hidden;
  }
  .cov-bar-would,
  .cov-bar-first,
  .cov-bar-fall {
    position: absolute;
    top: 0;
    left: 0;
    bottom: 0;
    transition: width 220ms ease-out;
    border-radius: 6px;
  }
  .cov-bar-would {
    background: color-mix(in srgb, #7c8cff 22%, transparent);
  }
  .cov-bar-first {
    background: color-mix(in srgb, #7c8cff 75%, transparent);
  }
  .cov-bar-fall {
    background: color-mix(in srgb, #888 35%, transparent);
  }
  .cov-row.dead .cov-bar-would {
    background: color-mix(in srgb, #ff7b56 24%, transparent);
  }

  .cov-counts {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.78);
    white-space: nowrap;
    min-width: 64px;
    text-align: right;
  }
  .cov-num.would {
    color: rgba(255, 255, 255, 0.45);
  }
  .cov-num.fall {
    color: rgba(255, 255, 255, 0.6);
  }
  .cov-sep {
    color: rgba(255, 255, 255, 0.3);
    margin: 0 2px;
  }

  .cov-legend {
    margin: 10px 2px 0;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.48);
    line-height: 1.5;
  }
  .cov-legend strong {
    color: #ffc1a8;
    font-weight: 600;
  }

  .cov-chev {
    display: inline-block;
    width: 12px;
    margin-left: 8px;
    color: rgba(255, 255, 255, 0.35);
    transition: color 120ms;
  }
  .cov-row.open .cov-chev,
  .cov-row:hover .cov-chev {
    color: rgba(255, 255, 255, 0.7);
  }

  /* ── Slice 86 — drilldown popover ─────────────────────────────── */
  .cov-drilldown {
    margin: 0 6px 6px;
    padding: 10px 12px 12px;
    background: rgba(124, 140, 255, 0.045);
    border: 1px solid rgba(124, 140, 255, 0.18);
    border-top: none;
    border-radius: 0 0 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .drill-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    flex-wrap: wrap;
  }
  .drill-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .drill-title strong {
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.88);
  }
  .drill-sub {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    font-variant-numeric: tabular-nums;
  }
  .drill-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .drill-cap {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
  }
  .drill-cap input {
    width: 56px;
    padding: 3px 5px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    color: inherit;
    font: inherit;
    font-size: 11px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .drill-cap input:focus {
    outline: none;
    border-color: rgba(124, 140, 255, 0.5);
  }
  .ghost.mini {
    padding: 3px 9px;
    font-size: 11px;
  }
  .drill-error {
    margin: 0;
    padding: 6px 10px;
    font-size: 11px;
    color: rgb(255, 180, 180);
    background: rgba(240, 80, 80, 0.1);
    border-radius: 4px;
  }
  .drill-export-toast {
    margin: 0;
    padding: 6px 10px;
    font-size: 11px;
    color: rgb(170, 230, 195);
    background: rgba(110, 220, 154, 0.1);
    border: 1px solid rgba(110, 220, 154, 0.22);
    border-radius: 4px;
    font-variant-numeric: tabular-nums;
    animation: drill-toast-fade-in 0.16s ease-out;
  }
  @keyframes drill-toast-fade-in {
    from { opacity: 0; transform: translateY(-2px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .drill-loading,
  .drill-empty {
    margin: 4px 0 0;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
    line-height: 1.5;
  }
  .drill-list {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    max-height: 260px;
    overflow-y: auto;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.18);
  }
  .drill-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11.5px;
    color: rgba(255, 255, 255, 0.82);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  .drill-item:last-child { border-bottom: none; }
  .drill-item:hover { background: rgba(255, 255, 255, 0.03); }
  .drill-glyph {
    color: rgba(124, 140, 255, 0.55);
    flex-shrink: 0;
    width: 10px;
  }
  .drill-fname {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .drill-trunc {
    margin: 4px 0 0;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.48);
    font-style: italic;
  }

  .ghost.active {
    background: rgba(124, 140, 255, 0.18);
    border-color: rgba(124, 140, 255, 0.42);
  }
</style>
