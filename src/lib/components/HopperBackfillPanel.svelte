<script lang="ts">
  // HopperBackfillPanel — v3.39 round-10 wired UI.
  //
  // The Rules Editor lets you write a rule chain that fires when *new*
  // PDFs arrive. But every paralegal, every legal-discovery person, every
  // researcher has the same problem: "I just wrote 8 great rules — and I
  // already have 4,000 PDFs sitting in this folder I want them applied
  // to." Watching new arrivals doesn't help.
  //
  // Round-10 ties the backend Hopper Loop together end-to-end:
  //
  //   1. Recursive-scan toggle + depth cap dropdown so paralegals can
  //      point Hopper at `discovery/` once and sweep every sub-folder.
  //   2. Per-rule coverage chips ("Tax: 17 · Invoices: 23 · No rule: 4")
  //      under the summary strip — see coverage at a glance before
  //      clicking Apply.
  //   3. Apply uses the STREAMING executor — live progress bar
  //      ("142 of 4,213 · 138 moved · 4 skipped") + scrolling per-file
  //      tail + working Cancel button (was a no-op stub before).
  //   4. "Export CSV…" affordance writes the dry-run plan to disk
  //      via a native save-as dialog — the audit-trail paralegals
  //      need to send to a partner before applying.
  //   5. History chips ("Last 24h / Last 7d / All") scope the
  //      Recent Backfills disclosure without re-fetching the full
  //      table client-side.
  //
  // Liquid Glass styling matches HopperRulesEditor (sibling component).

  import { onMount, onDestroy } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import {
    slabHopperPlanBackfill,
    slabHopperExecuteBackfillAsync,
    slabHopperCancelBackfill,
    slabHopperListBackfillRuns,
    slabHopperExportBackfillCsv,
    listenBackfillProgress,
    newBackfillRunId,
    backfillSinceUnix,
    backfillBucketLabel,
    suggestBackfillCsvFilename,
    formatBytes,
    basename,
    BACKFILL_BUCKET_DEFAULTS,
    BACKFILL_BUCKET_SKIP,
    type BackfillReport,
    type BackfillRun,
    type BackfillProgress,
    type PlannedAction,
    type BackfillActionKind,
    type PlanOptions,
  } from "$lib/hopper";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  // -------------------------------------------------------------------
  // Props — opened in-context from HopperRulesEditor.
  // -------------------------------------------------------------------

  interface Props {
    watchId: number;
    watchSource: string;
    /** Called when the user dismisses the panel (X / Cancel / Apply-
     *  finished). The parent unmounts us. */
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
  let cancelling = $state(false);
  let errorMsg = $state<string | null>(null);
  let toastMsg = $state<string | null>(null);

  // Plan-options state — recursive toggle + depth dropdown.
  let recursive = $state(false);
  /** Depth value bound to the dropdown. "all" sentinel maps to
   *  null (unbounded); finite values map to the matching integer. */
  let maxDepth = $state<"all" | "1" | "3" | "5">("all");

  // Track which planned rows the user wants to apply. Default: all
  // matchable rows (Move/Copy) checked; Skip/NoMatch rows are visually
  // disabled but tracked separately so the count math stays right.
  let selected = $state<Set<number>>(new Set());

  // Streaming-apply state — per-file progress + scrolling tail.
  let progress = $state<BackfillProgress | null>(null);
  /** Last ~12 outcomes for the scrolling tail strip. */
  let recentTail = $state<BackfillProgress[]>([]);
  let currentRunId = $state<number | null>(null);
  let progressUnlisten: UnlistenFn | null = null;

  // History filter — "Last 24h / Last 7d / All".
  let historyWindow = $state<"24h" | "7d" | "all">("7d");

  // -------------------------------------------------------------------
  // Derived
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
    if (!report || applying) return false;
    return Date.now() / 1000 - report.generated_at > 60;
  });

  /** Per-rule chips for the summary strip — sorted: matched rules first
   *  by descending count, then the synthetic defaults / skip buckets
   *  pinned at the end. Empty when there are no buckets. */
  const ruleChips = $derived.by(() => {
    const buckets = report?.per_rule_counts ?? {};
    const matched: Array<{ key: string; label: string; count: number; kind: "rule" | "defaults" | "skip" }> = [];
    let defaultsBucket: { key: string; label: string; count: number; kind: "defaults" } | null = null;
    let skipBucket: { key: string; label: string; count: number; kind: "skip" } | null = null;
    for (const [k, n] of Object.entries(buckets)) {
      if (k === BACKFILL_BUCKET_DEFAULTS) {
        defaultsBucket = { key: k, label: backfillBucketLabel(k), count: n, kind: "defaults" };
      } else if (k === BACKFILL_BUCKET_SKIP) {
        skipBucket = { key: k, label: backfillBucketLabel(k), count: n, kind: "skip" };
      } else {
        matched.push({ key: k, label: k, count: n, kind: "rule" });
      }
    }
    matched.sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
    if (defaultsBucket) matched.push(defaultsBucket);
    if (skipBucket) matched.push(skipBucket);
    return matched;
  });

  const planOptions = $derived<PlanOptions>({
    recursive,
    max_depth: !recursive || maxDepth === "all" ? null : Number(maxDepth),
  });

  // -------------------------------------------------------------------
  // Lifecycle — kick off the plan on mount + load history + subscribe
  // to streaming progress events.
  // -------------------------------------------------------------------

  onMount(async () => {
    progressUnlisten = await listenBackfillProgress((e) => {
      // Filter to our own run — the wire contract allows concurrent
      // runs (today's UI gates to one, but the filter future-proofs).
      if (currentRunId !== null && e.run_id !== currentRunId) return;
      progress = e.progress;
      // Keep the tail capped so the DOM doesn't grow unbounded on a
      // 10,000-file run. Newest first; the strip renders bottom-up.
      recentTail = [e.progress, ...recentTail].slice(0, 12);
    });
    await Promise.all([runPlan(), loadHistory()]);
  });

  onDestroy(() => {
    if (progressUnlisten) progressUnlisten();
  });

  async function runPlan(): Promise<void> {
    planning = true;
    errorMsg = null;
    try {
      const r = await slabHopperPlanBackfill(watchId, undefined, planOptions);
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
      const windowHours =
        historyWindow === "24h" ? 24 : historyWindow === "7d" ? 24 * 7 : null;
      const since = backfillSinceUnix(windowHours);
      recentRuns = await slabHopperListBackfillRuns(
        watchSource,
        20,
        since ?? undefined,
      );
    } catch {
      recentRuns = [];
    }
  }

  async function applyPlan(): Promise<void> {
    if (!report) return;
    if (selectedCount === 0) {
      errorMsg = "Nothing selected — pick at least one row.";
      return;
    }
    const trimmed: BackfillReport = {
      ...report,
      planned: report.planned.filter((_, i) => selected.has(i)),
      scanned: selectedCount,
    };
    applying = true;
    cancelling = false;
    errorMsg = null;
    progress = { processed: 0, total: trimmed.planned.length, applied: 0, skipped: 0, errored: 0, current: null };
    recentTail = [];
    const runId = newBackfillRunId();
    currentRunId = runId;
    try {
      const run = await slabHopperExecuteBackfillAsync(trimmed, runId);
      lastRun = run;
      void loadHistory();
    } catch (e) {
      errorMsg = `Apply failed: ${String(e)}`;
    } finally {
      applying = false;
      cancelling = false;
      currentRunId = null;
    }
  }

  async function cancelApply(): Promise<void> {
    if (currentRunId === null || cancelling) return;
    cancelling = true;
    try {
      await slabHopperCancelBackfill(currentRunId);
      // The worker still drains the remaining files (each marked
      // skipped); UI keeps applying=true until the await above
      // resolves. No further wiring needed.
    } catch {
      // Cancel failed (run probably already finished) — non-fatal.
      cancelling = false;
    }
  }

  async function exportCsv(): Promise<void> {
    if (!report) return;
    try {
      const defaultPath = suggestBackfillCsvFilename(report);
      const targetPath = await save({
        defaultPath,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!targetPath) return; // user cancelled
      const bytes = await slabHopperExportBackfillCsv(report, targetPath);
      toastMsg = `Exported ${report.planned.length} rows (${formatBytes(bytes)})`;
      window.setTimeout(() => {
        toastMsg = null;
      }, 4000);
    } catch (e) {
      errorMsg = `Export failed: ${String(e)}`;
    }
  }

  function toggle(i: number, p: PlannedAction): void {
    if (p.action !== "move" && p.action !== "copy") return;
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

  function onRecursiveToggle(): void {
    recursive = !recursive;
    void runPlan();
  }

  function onMaxDepthChange(): void {
    if (recursive) void runPlan();
  }

  function onHistoryWindowChange(w: "24h" | "7d" | "all"): void {
    historyWindow = w;
    void loadHistory();
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

    <!-- Scan options strip — recursive + depth cap -->
    <div class="opts">
      <label class="opt">
        <input
          type="checkbox"
          checked={recursive}
          onchange={onRecursiveToggle}
          disabled={applying || planning}
        />
        Include sub-folders
      </label>
      {#if recursive}
        <label class="opt depth">
          Depth
          <select
            bind:value={maxDepth}
            onchange={onMaxDepthChange}
            disabled={applying || planning}
          >
            <option value="all">No limit</option>
            <option value="1">1 level</option>
            <option value="3">3 levels</option>
            <option value="5">5 levels</option>
          </select>
        </label>
      {/if}
    </div>

    {#if errorMsg}
      <div class="error">{errorMsg}</div>
    {/if}

    {#if toastMsg}
      <div class="toast">{toastMsg}</div>
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
        adjust your watch's source directory{recursive ? " — or try a deeper recursion depth" : ""}.
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

      <!-- Per-rule coverage chips (round-10 slice 44) -->
      {#if ruleChips.length > 0}
        <div class="coverage" aria-label="Per-rule coverage">
          {#each ruleChips as c (c.key)}
            <span class="cover-chip {c.kind}" title="{c.count} files">
              <span class="cover-label">{c.label}</span>
              <span class="cover-count">{c.count}</span>
            </span>
          {/each}
        </div>
      {/if}

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
                    disabled={locked || applying}
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

      <!-- Streaming progress (round-10 slice 47) -->
      {#if applying && progress}
        <div class="progress" role="status" aria-live="polite">
          <div class="progress-bar">
            <div
              class="progress-fill"
              style="width: {progress.total > 0 ? (progress.processed / progress.total) * 100 : 0}%"
            ></div>
          </div>
          <div class="progress-meta">
            <span><strong>{progress.processed.toLocaleString()}</strong> of {progress.total.toLocaleString()}</span>
            <span class="muted">·</span>
            <span class="ok"><strong>{progress.applied}</strong> moved</span>
            {#if progress.skipped > 0}
              <span class="muted">·</span>
              <span class="warn"><strong>{progress.skipped}</strong> skipped</span>
            {/if}
            {#if progress.errored > 0}
              <span class="muted">·</span>
              <span class="err"><strong>{progress.errored}</strong> errored</span>
            {/if}
          </div>
          {#if recentTail.length > 0}
            <ul class="tail">
              {#each recentTail as t (t.processed)}
                {#if t.current}
                  <li class={t.current.status}>
                    <span class="tail-status">{t.current.status === "moved" ? "✓" : t.current.status === "skipped" ? "↷" : "✗"}</span>
                    <span class="tail-name" title={t.current.source_path}>{basename(t.current.source_path)}</span>
                    {#if t.current.error}
                      <span class="tail-err">{t.current.error}</span>
                    {/if}
                  </li>
                {/if}
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      <!-- Action bar -->
      <footer class="actions">
        <button class="ghost" onclick={onClose} disabled={applying}>Close</button>
        <button class="ghost" onclick={runPlan} disabled={applying || planning}>
          Re-scan
        </button>
        <button class="ghost" onclick={exportCsv} disabled={applying || planning}>
          Export CSV…
        </button>
        <div class="spacer"></div>
        {#if applying}
          <button class="ghost danger" onclick={cancelApply} disabled={cancelling}>
            {cancelling ? "Cancelling…" : "Cancel"}
          </button>
        {/if}
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

      {#if lastRun && !applying}
        <div class="last-run" role="status">
          ✓ Applied {lastRun.applied} · skipped {lastRun.skipped}
          {#if lastRun.errored > 0}· <strong>{lastRun.errored} errored</strong>{/if}
          · {elapsed(lastRun.started_at, lastRun.finished_at)}
        </div>
      {/if}
    {/if}

    <!-- Recent backfills disclosure with time-window chips -->
    <details class="history">
      <summary>
        Recent backfills{#if recentRuns.length > 0} ({recentRuns.length}){/if}
      </summary>
      <div class="history-chips">
        {#each [["24h", "Last 24h"], ["7d", "Last 7 days"], ["all", "All"]] as [w, label] (w)}
          <button
            class="history-chip"
            class:active={historyWindow === w}
            onclick={() => onHistoryWindowChange(w as "24h" | "7d" | "all")}
          >
            {label}
          </button>
        {/each}
      </div>
      {#if recentRuns.length === 0}
        <p class="history-empty">No backfills in this window.</p>
      {:else}
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
      {/if}
    </details>
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
    margin-bottom: 0.9rem;
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
  .opts {
    display: flex;
    flex-wrap: wrap;
    gap: 0.8rem 1.2rem;
    align-items: center;
    padding: 0.5rem 0.8rem;
    background: rgba(255, 255, 255, 0.025);
    border-radius: 8px;
    margin-bottom: 0.7rem;
    font-size: 0.84rem;
    color: var(--text-2);
  }
  .opt {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    cursor: pointer;
  }
  .opt input[type="checkbox"] {
    cursor: pointer;
  }
  .opt.depth select {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-1);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    padding: 0.18rem 0.35rem;
    font-size: 0.82rem;
  }
  .error {
    background: rgba(220, 80, 80, 0.12);
    border: 1px solid rgba(220, 80, 80, 0.4);
    color: #ffb6b6;
    padding: 0.55rem 0.8rem;
    border-radius: 8px;
    font-size: 0.85rem;
    margin-bottom: 0.7rem;
  }
  .toast {
    background: rgba(122, 210, 122, 0.14);
    border: 1px solid rgba(122, 210, 122, 0.4);
    color: #c8efc8;
    padding: 0.5rem 0.8rem;
    border-radius: 8px;
    font-size: 0.85rem;
    margin-bottom: 0.7rem;
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
    margin-bottom: 0.55rem;
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
  .coverage {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-bottom: 0.75rem;
  }
  .cover-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.18rem 0.55rem;
    border-radius: 999px;
    background: rgba(122, 182, 255, 0.13);
    color: #c9defe;
    font-size: 0.78rem;
    border: 1px solid rgba(122, 182, 255, 0.18);
  }
  .cover-chip.defaults {
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-2);
    border-color: rgba(255, 255, 255, 0.1);
  }
  .cover-chip.skip {
    background: rgba(255, 196, 107, 0.12);
    color: #ffd89b;
    border-color: rgba(255, 196, 107, 0.25);
  }
  .cover-chip .cover-count {
    font-weight: 700;
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
    max-height: 45vh;
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
  .progress {
    margin-top: 0.8rem;
    padding: 0.7rem 0.85rem;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--border-1);
    border-radius: 10px;
  }
  .progress-bar {
    width: 100%;
    height: 6px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #4d8af0 0%, #7ad27a 100%);
    transition: width 0.18s ease-out;
  }
  .progress-meta {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    margin-top: 0.45rem;
    font-size: 0.84rem;
    color: var(--text-2);
  }
  .progress-meta .ok strong {
    color: #7ad27a;
  }
  .progress-meta .warn strong {
    color: #ffc46b;
  }
  .progress-meta .err strong {
    color: #ffb6b6;
  }
  .tail {
    list-style: none;
    margin: 0.55rem 0 0;
    padding: 0;
    max-height: 140px;
    overflow-y: auto;
    font-size: 0.8rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .tail li {
    display: flex;
    gap: 0.45rem;
    padding: 0.15rem 0;
    align-items: center;
    color: var(--text-2);
  }
  .tail li.moved {
    color: #b8e7b8;
  }
  .tail li.skipped {
    color: #ffd89b;
  }
  .tail li.failed {
    color: #ffb6b6;
  }
  .tail-status {
    width: 1ch;
    text-align: center;
  }
  .tail-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 30ch;
  }
  .tail-err {
    color: var(--text-3);
    font-size: 0.75rem;
    margin-left: auto;
  }
  .actions {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    margin-top: 0.9rem;
    flex-wrap: wrap;
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
  button.ghost.danger {
    color: #ffb6b6;
    border-color: rgba(220, 80, 80, 0.4);
  }
  button.ghost.danger:hover:not(:disabled) {
    background: rgba(220, 80, 80, 0.08);
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
  .history-chips {
    display: flex;
    gap: 0.35rem;
    margin: 0.4rem 0 0.5rem;
    flex-wrap: wrap;
  }
  .history-chip {
    background: transparent;
    border: 1px solid var(--border-1);
    color: var(--text-2);
    border-radius: 999px;
    padding: 0.15rem 0.7rem;
    font-size: 0.78rem;
    cursor: pointer;
  }
  .history-chip:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .history-chip.active {
    background: rgba(122, 182, 255, 0.18);
    color: #c9defe;
    border-color: rgba(122, 182, 255, 0.4);
  }
  .history-empty {
    margin: 0.3rem 0;
    color: var(--text-3);
    font-size: 0.84rem;
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
