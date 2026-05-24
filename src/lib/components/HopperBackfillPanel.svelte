<script lang="ts">
  // HopperBackfillPanel — v3.22.0 "Hopper Loop" wow surface.
  //
  // The Rules Editor lets you write a rule chain that fires when *new*
  // PDFs arrive. But every paralegal, every legal-discovery person, every
  // researcher has the same problem: "I just wrote 8 great rules — and I
  // already have 4,000 PDFs sitting in this folder I want them applied
  // to." Watching new arrivals doesn't help.
  //
  // This panel closes the loop. Two-step UX:
  //
  //   1. Click "Test on this folder" → backend dry-runs the current rule
  //      chain against the watch's source_dir and returns a table of
  //      every PDF + which rule matched + where it would land. Zero
  //      files moved. Pure preview. (The wow.)
  //
  //   2. Eyeball the table. Click "Apply all" (or future "Apply
  //      selected"). Backend commits the moves idempotently and
  //      persists the run to sqlite. A toast shows applied/skipped/
  //      errored counts; the "Recent backfills" disclosure picks up
  //      the new row.
  //
  // Liquid Glass styling matches HopperRulesEditor (sibling component).

  import { onMount } from "svelte";
  import {
    slabHopperPlanBackfill,
    slabHopperExecuteBackfill,
    slabHopperListBackfillRuns,
    basename,
    formatBytes,
    type BackfillReport,
    type BackfillRun,
    type PlannedAction,
    type BackfillActionKind,
  } from "$lib/hopper";

  // -------------------------------------------------------------------
  // Props — opened in-context from HopperRulesEditor.
  // -------------------------------------------------------------------

  interface Props {
    watchId: number;
    watchSource: string;
    /** Called when the user dismisses the panel (X button / Cancel /
     *  Apply-finished). The parent unmounts us. */
    onClose: () => void;
  }

  let { watchId, watchSource, onClose }: Props = $props();

  // -------------------------------------------------------------------
  // State
  // -------------------------------------------------------------------

  let report = $state<BackfillReport | null>(null);
  let lastRun = $state<BackfillRun | null>(null);
  let recentRuns = $state<BackfillRun[]>([]);

  let planning = $state(false);
  let applying = $state(false);
  let errorMsg = $state<string | null>(null);

  // Track which planned rows the user wants to apply. Default: all
  // matchable rows (Move/Copy) checked; Skip/NoMatch rows are visually
  // disabled but tracked separately so the count math stays right.
  let selected = $state<Set<number>>(new Set());

  // -------------------------------------------------------------------
  // Derived counts — drive the action-bar labels.
  // -------------------------------------------------------------------

  const counts = $derived.by(() => {
    if (!report) return { total: 0, willMove: 0, skip: 0, noMatch: 0 };
    let willMove = 0;
    let skip = 0;
    let noMatch = 0;
    for (const p of report.planned) {
      if (p.action === "move" || p.action === "copy") willMove++;
      else if (p.action === "skip") skip++;
      else noMatch++;
    }
    return { total: report.planned.length, willMove, skip, noMatch };
  });

  const selectedCount = $derived(selected.size);
  const isStale = $derived.by(() => {
    if (!report) return false;
    return Date.now() / 1000 - report.generated_at > 60;
  });

  // -------------------------------------------------------------------
  // Lifecycle — kick off the plan on mount + load history.
  // -------------------------------------------------------------------

  onMount(async () => {
    await Promise.all([runPlan(), loadHistory()]);
  });

  async function runPlan(): Promise<void> {
    planning = true;
    errorMsg = null;
    try {
      const r = await slabHopperPlanBackfill(watchId);
      report = r;
      // Pre-select every actionable row.
      const fresh = new Set<number>();
      r.planned.forEach((p, i) => {
        if (p.action === "move" || p.action === "copy") fresh.add(i);
      });
      selected = fresh;
    } catch (e) {
      errorMsg = `Plan failed: ${String(e)}`;
    } finally {
      planning = false;
    }
  }

  async function loadHistory(): Promise<void> {
    try {
      recentRuns = await slabHopperListBackfillRuns(watchSource, 5);
    } catch {
      // Non-fatal — history is a nice-to-have, not a blocker.
      recentRuns = [];
    }
  }

  async function applyPlan(): Promise<void> {
    if (!report) return;
    if (selectedCount === 0) {
      errorMsg = "Nothing selected — pick at least one row.";
      return;
    }
    // Filter the report down to only selected rows. Backend doesn't
    // know about selection; we just hand it a trimmed plan.
    const trimmed: BackfillReport = {
      ...report,
      planned: report.planned.filter((_, i) => selected.has(i)),
      scanned: selectedCount,
    };
    applying = true;
    errorMsg = null;
    try {
      const run = await slabHopperExecuteBackfill(trimmed);
      lastRun = run;
      // Refresh history + re-plan (so any leftover skipped files are
      // visible). Don't await re-plan — let the user dismiss freely.
      void loadHistory();
    } catch (e) {
      errorMsg = `Apply failed: ${String(e)}`;
    } finally {
      applying = false;
    }
  }

  function toggle(i: number, p: PlannedAction): void {
    if (p.action !== "move" && p.action !== "copy") return; // locked
    const next = new Set(selected);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    selected = next;
  }

  function toggleAll(): void {
    if (!report) return;
    if (selectedCount > 0) {
      selected = new Set();
    } else {
      const fresh = new Set<number>();
      report.planned.forEach((p, i) => {
        if (p.action === "move" || p.action === "copy") fresh.add(i);
      });
      selected = fresh;
    }
  }

  // Pretty action label + class for the row badge.
  function actionLabel(k: BackfillActionKind): string {
    switch (k) {
      case "move":
        return "Move";
      case "copy":
        return "Copy";
      case "skip":
        return "Skip";
      case "no-match":
        return "No rule";
    }
  }

  function fmtTime(unix: number): string {
    if (!unix) return "—";
    return new Date(unix * 1000).toLocaleString();
  }

  function elapsed(start: number, end: number): string {
    const ms = Math.max(0, (end - start) * 1000);
    if (ms < 1000) return `${Math.round(ms)} ms`;
    return `${(ms / 1000).toFixed(1)} s`;
  }
</script>

<div class="backfill-overlay" role="dialog" aria-modal="true" aria-label="Backfill folder">
  <section class="backfill-panel">
    <header class="head">
      <div>
        <h3>Test on this folder</h3>
        <p class="sub">
          Dry-run your rule chain against every PDF already in
          <code>{basename(watchSource) || watchSource}</code>. Nothing
          moves until you click <strong>Apply</strong>.
        </p>
      </div>
      <button class="close" onclick={onClose} aria-label="Close">×</button>
    </header>

    {#if errorMsg}
      <div class="error">{errorMsg}</div>
    {/if}

    {#if planning}
      <div class="state-block">
        <span class="spinner"></span> Scanning folder + evaluating rules…
      </div>
    {:else if !report}
      <div class="state-block muted">No plan yet.</div>
    {:else if report.planned.length === 0}
      <div class="state-block muted">
        <strong>No PDFs found in this folder.</strong><br />
        Drop some PDFs into <code>{watchSource}</code> and try again, or
        adjust your watch's source directory.
      </div>
    {:else}
      <!-- Summary strip -->
      <div class="summary">
        <span class="stat">
          <strong>{counts.total}</strong> file{counts.total === 1 ? "" : "s"} scanned
        </span>
        <span class="stat ok">
          <strong>{counts.willMove}</strong> will move
        </span>
        {#if counts.skip > 0}
          <span class="stat warn"><strong>{counts.skip}</strong> skip</span>
        {/if}
        {#if counts.noMatch > 0}
          <span class="stat muted"><strong>{counts.noMatch}</strong> no rule</span>
        {/if}
        {#if isStale}
          <button class="link" onclick={runPlan}>Plan is &gt;60s old — rescan?</button>
        {/if}
      </div>

      <!-- The table -->
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th class="cb">
                <input
                  type="checkbox"
                  checked={selectedCount === counts.willMove && counts.willMove > 0}
                  onchange={toggleAll}
                  aria-label="Toggle all"
                />
              </th>
              <th>File</th>
              <th>Size</th>
              <th>Matched rule</th>
              <th>Destination</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {#each report.planned as p, i (p.source_path)}
              {@const locked = p.action !== "move" && p.action !== "copy"}
              <tr class:locked>
                <td class="cb">
                  <input
                    type="checkbox"
                    checked={selected.has(i)}
                    disabled={locked}
                    onchange={() => toggle(i, p)}
                    aria-label="Select {basename(p.source_path)}"
                  />
                </td>
                <td class="file" title={p.source_path}>
                  {basename(p.source_path)}
                </td>
                <td class="size">{formatBytes(p.size_bytes)}</td>
                <td class="rule">
                  {#if p.matched_rule}
                    <span class="chip">{p.matched_rule}</span>
                  {:else}
                    <span class="chip muted">defaults</span>
                  {/if}
                </td>
                <td class="dest" title={p.destination ?? p.reason}>
                  {#if p.destination}
                    {basename(p.destination)}
                  {:else}
                    <span class="muted">{p.reason}</span>
                  {/if}
                </td>
                <td>
                  <span class="badge {p.action}">{actionLabel(p.action)}</span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <!-- Action bar -->
      <footer class="actions">
        <button class="ghost" onclick={onClose} disabled={applying}>Cancel</button>
        <button class="ghost" onclick={runPlan} disabled={applying || planning}>
          Re-scan
        </button>
        <div class="spacer"></div>
        <button
          class="primary"
          onclick={applyPlan}
          disabled={applying || selectedCount === 0}
        >
          {#if applying}
            <span class="spinner"></span> Applying…
          {:else}
            Apply {selectedCount} file{selectedCount === 1 ? "" : "s"}
          {/if}
        </button>
      </footer>

      {#if lastRun}
        <div class="last-run" role="status">
          ✓ Applied {lastRun.applied} · skipped {lastRun.skipped}
          {#if lastRun.errored > 0}· <strong>{lastRun.errored} errored</strong>{/if}
          · {elapsed(lastRun.started_at, lastRun.finished_at)}
        </div>
      {/if}
    {/if}

    <!-- Recent backfills disclosure -->
    {#if recentRuns.length > 0}
      <details class="history">
        <summary>Recent backfills ({recentRuns.length})</summary>
        <ul>
          {#each recentRuns as r}
            <li>
              <span class="when">{fmtTime(r.finished_at)}</span>
              <span class="counts">
                {r.applied} moved
                {#if r.skipped > 0}· {r.skipped} skipped{/if}
                {#if r.errored > 0}· <strong>{r.errored} errored</strong>{/if}
              </span>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  </section>
</div>

<style>
  .backfill-overlay {
    position: fixed;
    inset: 0;
    background: rgba(10, 12, 18, 0.55);
    backdrop-filter: blur(8px);
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }
  .backfill-panel {
    width: min(960px, 100%);
    max-height: calc(100vh - 4rem);
    overflow-y: auto;
    background: var(--surface-1, rgba(28, 30, 38, 0.95));
    border: 1px solid var(--border-1, rgba(255, 255, 255, 0.08));
    border-radius: 14px;
    padding: 1.4rem 1.6rem;
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.45);
    color: var(--text-1, #e9eaf0);
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }
  .head h3 {
    margin: 0 0 0.25rem;
    font-size: 1.1rem;
    font-weight: 600;
  }
  .head .sub {
    margin: 0;
    color: var(--text-2, #b6b9c4);
    font-size: 0.86rem;
    max-width: 56ch;
  }
  .close {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    font-size: 1.4rem;
    line-height: 1;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    cursor: pointer;
  }
  .close:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-1);
  }
  .error {
    background: rgba(220, 80, 80, 0.12);
    border: 1px solid rgba(220, 80, 80, 0.4);
    color: #ffb6b6;
    padding: 0.55rem 0.8rem;
    border-radius: 8px;
    font-size: 0.85rem;
    margin-bottom: 0.8rem;
  }
  .state-block {
    padding: 2rem;
    text-align: center;
    color: var(--text-2);
    font-size: 0.92rem;
    background: rgba(255, 255, 255, 0.02);
    border-radius: 10px;
  }
  .state-block.muted {
    color: var(--text-3, #8c91a0);
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 1.2rem;
    align-items: center;
    padding: 0.6rem 0.9rem;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 10px;
    margin-bottom: 0.75rem;
    font-size: 0.86rem;
  }
  .stat strong {
    font-weight: 700;
    color: var(--text-1);
  }
  .stat.ok strong {
    color: #7ad27a;
  }
  .stat.warn strong {
    color: #ffc46b;
  }
  .stat.muted {
    color: var(--text-3);
  }
  .link {
    background: none;
    border: none;
    color: var(--accent, #7ab6ff);
    cursor: pointer;
    font-size: 0.84rem;
    text-decoration: underline;
    padding: 0;
    margin-left: auto;
  }
  .table-wrap {
    max-height: 50vh;
    overflow-y: auto;
    border: 1px solid var(--border-1);
    border-radius: 10px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.86rem;
  }
  thead {
    position: sticky;
    top: 0;
    background: var(--surface-2, rgba(40, 42, 52, 0.95));
    z-index: 1;
  }
  th,
  td {
    text-align: left;
    padding: 0.5rem 0.7rem;
    border-bottom: 1px solid var(--border-1);
    vertical-align: middle;
  }
  th {
    font-weight: 600;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  th.cb,
  td.cb {
    width: 28px;
    padding-right: 0;
  }
  tr.locked {
    opacity: 0.55;
  }
  td.file {
    font-weight: 500;
    max-width: 24ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  td.size {
    color: var(--text-3);
    white-space: nowrap;
  }
  td.dest {
    color: var(--text-2);
    max-width: 18ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip {
    display: inline-block;
    padding: 0.1rem 0.55rem;
    border-radius: 999px;
    background: rgba(122, 182, 255, 0.14);
    color: #c9defe;
    font-size: 0.78rem;
  }
  .chip.muted {
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-3);
  }
  .muted {
    color: var(--text-3);
  }
  .badge {
    display: inline-block;
    padding: 0.12rem 0.55rem;
    border-radius: 6px;
    font-size: 0.76rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .badge.move {
    background: rgba(122, 210, 122, 0.18);
    color: #b8e7b8;
  }
  .badge.copy {
    background: rgba(122, 182, 255, 0.18);
    color: #c9defe;
  }
  .badge.skip {
    background: rgba(255, 196, 107, 0.18);
    color: #ffd89b;
  }
  .badge.no-match {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-3);
  }
  .actions {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    margin-top: 0.9rem;
  }
  .actions .spacer {
    flex: 1;
  }
  button.primary,
  button.ghost {
    border-radius: 8px;
    padding: 0.5rem 0.95rem;
    font-size: 0.88rem;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid transparent;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  button.primary {
    background: var(--accent, #4d8af0);
    color: white;
  }
  button.primary:hover:not(:disabled) {
    background: #5a9aff;
  }
  button.primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  button.ghost {
    background: transparent;
    border-color: var(--border-1);
    color: var(--text-1);
  }
  button.ghost:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.04);
  }
  button.ghost:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    display: inline-block;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .last-run {
    margin-top: 0.7rem;
    padding: 0.6rem 0.85rem;
    background: rgba(122, 210, 122, 0.12);
    border: 1px solid rgba(122, 210, 122, 0.35);
    border-radius: 8px;
    font-size: 0.86rem;
    color: #c8efc8;
  }
  .history {
    margin-top: 1rem;
    font-size: 0.84rem;
  }
  .history summary {
    cursor: pointer;
    color: var(--text-2);
    padding: 0.35rem 0;
  }
  .history ul {
    list-style: none;
    margin: 0.4rem 0 0;
    padding: 0;
  }
  .history li {
    display: flex;
    justify-content: space-between;
    padding: 0.3rem 0;
    border-bottom: 1px dashed var(--border-1);
    color: var(--text-2);
  }
  .history li:last-child {
    border-bottom: none;
  }
  .when {
    color: var(--text-3);
  }
  code {
    background: rgba(255, 255, 255, 0.06);
    padding: 0.05rem 0.35rem;
    border-radius: 4px;
    font-size: 0.84em;
  }
</style>
