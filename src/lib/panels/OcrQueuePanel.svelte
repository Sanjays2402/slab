<!--
  OCR Queue Panel (v3.52.0 "Atlas OCR-Queue").

  Headless modal-style panel that surfaces the entire library OCR
  pipeline in one demo-able place:

    - Top status footer: per-state counts (pending / done / failed,
      total, text-native) backed by `slab_library_ocr_queue_stats`.
    - Failure inbox: every `ocr_failed` doc, newest first, each row
      naming the captured `ocr_error` (e.g. "tesseract not on PATH").
      Per-row "Retry" re-queues that single doc; header "Retry all"
      re-queues every failure in one shot.
    - Pending queue preview: the next N rows the worker would pick up
      (scanned + mixed), with file basename + folder hint. "Run all
      pending" wraps the existing `ocrQueueRunAll`; per-row "Run now"
      wraps `ocrQueueRunOne`. Per-row "Open" launches the source PDF.

  Triggered from:
    - Command palette ("OCR Queue…")
    - Keyboard shortcut Cmd/Ctrl+Shift+O
    - `slab:open-ocr-queue` window event (panel-mounted CollectionsSidebar
      pattern, mirrors SmartFoldersHubPanel).

  All five panel slices are self-contained — opening this panel is
  exactly the demo. No emoji in chrome (per Slab house style); the
  status dot mirrors LibrarySearchPanel's accent-green pip.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    ocrQueueListPending,
    ocrQueueListFailed,
    ocrQueueRunOne,
    ocrQueueRequeue,
    ocrQueueRequeueAllFailed,
    ocrQueueStats,
    type DocumentRecord,
    type OcrQueueResult,
    type OcrQueueStats,
  } from "$lib/library";
  import {
    ocrBasename,
    searchOcrDocs,
    sortOcrDocs,
    cycleOcrSort,
    ocrSortLabel,
    OCR_SORT_FIELDS,
    groupFailureReasons,
    filterByReason,
    reconcileReasonFacet,
    describeDominantReason,
    collectReasonRetryIds,
    describeReasonRetry,
    groupPendingStates,
    filterByPendingState,
    reconcilePendingStateFacet,
    pendingStateLabel,
    flattenOcrRows,
    classifyOcrTableKey,
    nextOcrCursor,
    clampOcrCursor,
    summarizePending,
    describeOcrImpact,
    describeRunAllProgress,
    describeRunAllOutcome,
    planRunRemaining,
    describeRunRemaining,
    describeOcrView,
    type OcrSort,
    type OcrSortField,
  } from "$lib/ocrQueueView";
  import { splitHighlight } from "$lib/paletteSearch";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  const { open, onClose }: Props = $props();

  let pending = $state<DocumentRecord[]>([]);
  let failed = $state<DocumentRecord[]>([]);
  let stats = $state<OcrQueueStats | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  /** Per-doc id of an in-flight run/requeue, used to disable buttons. */
  let busy = $state<Set<number>>(new Set());
  let runningAll = $state(false);
  /** Slice 5c: set true when the user hits Cancel mid Run-all; the per-doc
      loop checks it before each iteration and breaks after the current doc
      (the in-flight doc always finishes — there is no mid-OCR abort). */
  let cancelRequested = $state(false);
  /** Slice 5b: live Run-all progress (docs + pages done) so the overlay
      shows a REAL determinate bar instead of a frozen "Running N…". */
  let runAllDone = $state(0);
  let runAllPagesDone = $state(0);
  let runAllTotal = $state(0);
  let runAllPagesTotal = $state(0);
  /** Slice 5d: the un-run tail of the last canceled Run-all (the docs the
      loop never reached). Non-empty only after a cancel stopped a batch
      early; cleared when a fresh Run-all starts or the remainder is run.
      Powers the one-click "Run remaining (N)" resume affordance. */
  let resumeBatch = $state<DocumentRecord[]>([]);
  let requeueingAll = $state(false);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  // --- Atlas VI view-core state ---------------------------------------
  /** Slice 1: filter-as-you-type query over both lists. */
  let search = $state("");
  let searchEl = $state<HTMLInputElement | null>(null);
  /** Slice 2: shared sort column + direction across both lists. */
  let sort = $state<OcrSort>({ field: "name", dir: "asc" });
  /** Slice 3: active failure-reason facet (one bucket) or null for all. */
  let reasonFacet = $state<string | null>(null);
  /** Slice 3b: active pending-state facet (image-only/mixed) or null. */
  let pendingStateFacet = $state<string | null>(null);
  /** Slice 4: virtual cursor index spanning failures-then-pending. */
  let cursor = $state(0);
  let rowEls = $state<Array<HTMLLIElement | null>>([]);
  /** True once the list has keyboard focus (drives the cursor ring). */
  let listFocused = $state(false);

  function showToast(msg: string) {
    toast = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 2400);
  }

  function setBusy(id: number, on: boolean) {
    const next = new Set(busy);
    if (on) next.add(id);
    else next.delete(id);
    busy = next;
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      const [s, p, f] = await Promise.all([
        ocrQueueStats(),
        ocrQueueListPending(),
        ocrQueueListFailed(),
      ]);
      stats = s;
      pending = p;
      failed = f;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  // Single source of truth for "the name" — delegates to the view-core's
  // ocrBasename so display + the search highlight ranges can never
  // disagree on where a row's basename starts.
  function basename(path: string): string {
    return ocrBasename(path);
  }

  /** Compact folder hint (the path minus the basename, trimmed). */
  function folderHint(path: string): string {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    if (i <= 0) return "";
    const dir = path.slice(0, i);
    if (dir.length <= 40) return dir;
    // Long path: show first segment + ellipsis + last segment.
    const parts = dir.split(/[/\\]/);
    if (parts.length <= 3) return dir;
    return `${parts[0] || parts[1]}/…/${parts[parts.length - 1]}`;
  }

  async function runOne(doc: DocumentRecord) {
    if (busy.has(doc.id)) return;
    setBusy(doc.id, true);
    error = null;
    try {
      const r: OcrQueueResult = await ocrQueueRunOne(doc.id, null);
      if (r.error) {
        showToast(`Failed: ${basename(doc.path)} — ${r.error}`);
      } else {
        showToast(`OCR'd ${basename(doc.path)}`);
      }
      // refresh re-pulls stats + both lists; the changed row will hop
      // between buckets so a partial in-place patch would be noisier
      // than a clean reload.
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      setBusy(doc.id, false);
    }
  }

  async function runAllPending(explicitBatch?: DocumentRecord[]) {
    // A resume passes the snapshotted un-run tail; a fresh Run-all reads
    // the live pending list. Guard on the live queue only for a fresh run
    // (a resume's batch is already captured + may outlive a stats refresh).
    const isResume = Array.isArray(explicitBatch) && explicitBatch.length > 0;
    if (runningAll) return;
    if (!isResume && (stats?.pending_total ?? 0) === 0) return;
    // Snapshot the queue up front so the bar's denominator is fixed even
    // as rows hop buckets; measure the true page workload for the label.
    const batch = isResume ? explicitBatch.slice() : pending.slice();
    if (batch.length === 0) return;
    // Starting any run clears a stale resume offer — this batch supersedes it.
    resumeBatch = [];
    const workload = summarizePending(batch);
    runAllDone = 0;
    runAllPagesDone = 0;
    runAllTotal = workload.docs;
    runAllPagesTotal = workload.pages;
    cancelRequested = false;
    runningAll = true;
    error = null;
    let ok = 0;
    let fail = 0;
    try {
      // Run docs one at a time (not the blanket ocrQueueRunAll) so the
      // overlay can tick a REAL determinate bar after every doc, AND so a
      // Cancel can break the loop between docs. Per-doc OCR failures are
      // tallied, never thrown — a single bad PDF can't abort the batch.
      for (const doc of batch) {
        // Slice 5c: honour a Cancel requested since the last doc. The doc
        // currently in flight always finishes (OCR has no mid-page abort);
        // we just stop picking up new ones.
        if (cancelRequested) break;
        try {
          const r: OcrQueueResult = await ocrQueueRunOne(doc.id, null);
          if (r.state_after === "ocr_failed" || r.error) fail++;
          else ok++;
        } catch {
          fail++;
        }
        runAllDone++;
        if (typeof doc.pages === "number" && doc.pages > 0) runAllPagesDone += doc.pages;
      }
      // Slice 5d: if a Cancel stopped the loop early, capture the un-run
      // tail of THIS batch so the user can resume exactly where it left
      // off (planRunRemaining clamps the cut + measures the remainder).
      if (cancelRequested) {
        resumeBatch = planRunRemaining(batch, runAllDone).remaining;
      }
      // describeRunAllOutcome names a canceled-before-end run honestly
      // ("… canceled (13 of 47)") vs a clean finish ("… (of 47)").
      showToast(describeRunAllOutcome(ok, fail, batch.length, cancelRequested).label);
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      runningAll = false;
      cancelRequested = false;
    }
  }

  /** Slice 5d: resume a canceled Run-all — re-run exactly the un-run tail
      captured when Cancel stopped the loop. A no-op when nothing is
      pending resume or a run is already in flight. */
  function runRemaining() {
    if (runningAll || resumeBatch.length === 0) return;
    void runAllPending(resumeBatch);
  }

  /** Slice 5c: request cancellation of the in-flight Run-all. The loop
      checks the flag before its next doc and stops; the doc currently
      being OCR'd still completes. Idempotent + a no-op when nothing runs. */
  function cancelRunAll() {
    if (!runningAll) return;
    cancelRequested = true;
  }

  async function requeue(doc: DocumentRecord) {
    if (busy.has(doc.id)) return;
    setBusy(doc.id, true);
    error = null;
    try {
      await ocrQueueRequeue(doc.id);
      showToast(`Re-queued ${basename(doc.path)}`);
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      setBusy(doc.id, false);
    }
  }

  async function requeueAllFailed() {
    if (requeueingAll || failed.length === 0) return;
    requeueingAll = true;
    error = null;
    try {
      const n = await ocrQueueRequeueAllFailed();
      showToast(
        n === 0
          ? "Nothing failed to re-queue"
          : `Re-queued ${n} failed ${n === 1 ? "doc" : "docs"}`,
      );
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      requeueingAll = false;
    }
  }

  /** True while a per-reason "Retry all <reason>" loop is in flight. */
  let requeueingReason = $state(false);

  /** Slice 3c: re-queue ONLY the docs that failed for the active reason
      facet — not every failure. Loops `ocrQueueRequeue` over exactly the
      faceted ids (the same membership `filterByReason` decides), then
      refreshes once. Re-queueing flips each row back to `scanned` so the
      next Run picks it up; we leave the running to the queue rather than
      OCR'ing inline so the action stays cheap + non-blocking. */
  async function requeueReason() {
    if (requeueingReason || !reasonFacet) return;
    const ids = collectReasonRetryIds(failed, reasonFacet);
    if (ids.length === 0) return;
    requeueingReason = true;
    error = null;
    const label = reasonFacet;
    try {
      let n = 0;
      for (const id of ids) {
        await ocrQueueRequeue(id);
        n++;
      }
      showToast(`Re-queued ${n} ${n === 1 ? "doc" : "docs"} — ${label}`);
      // The facet's bucket is now empty; reconcile will clear it, and the
      // refresh re-pulls both lists so the retried rows reappear as pending.
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      requeueingReason = false;
    }
  }

  async function openInReader(doc: DocumentRecord) {
    // Mirror LibraryPanel.requestOpen: dispatch a window event the
    // root +page.svelte translates into a Reader tab. Works in both
    // main and detached windows.
    window.dispatchEvent(
      new CustomEvent("slab:open-library-doc", { detail: { path: doc.path } }),
    );
    onClose();
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    // Slice 4: let the virtual cursor claim arrows / Enter / o / Escape
    // first; it bails when focus is in the search box or on a button.
    if (handleTableKey(e)) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    refresh();
    window.addEventListener("keydown", handleKey);
    const libHandler = () => refresh();
    window.addEventListener("library-changed", libHandler);
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("library-changed", libHandler);
      if (toastTimer) clearTimeout(toastTimer);
    };
  });

  $effect(() => {
    if (open) {
      refresh();
    } else {
      // Reset the keyboard cursor + filters when the panel closes so a
      // reopen starts clean (mirrors the Beacon inspector).
      cursor = 0;
      listFocused = false;
      search = "";
      reasonFacet = null;
      pendingStateFacet = null;
    }
  });

  // ---------- Atlas VI: search / sort / facet / cursor derived --------

  /** Slice 3: failure-reason buckets (dominant cause first). */
  const reasonBuckets = $derived(groupFailureReasons(failed));
  const dominantReason = $derived(describeDominantReason(reasonBuckets));

  /** Slice 3c: how many failures the active reason facet covers + its
      "Retry N · <reason>" button label (empty string when no facet). */
  const reasonRetryCount = $derived(
    reasonFacet ? collectReasonRetryIds(failed, reasonFacet).length : 0,
  );
  const reasonRetryLabel = $derived(describeReasonRetry(reasonFacet, reasonRetryCount));

  /** Slice 3b: pending-state buckets (image-only/mixed, dominant first). */
  const pendingStateBuckets = $derived(groupPendingStates(pending));

  /** Slice 3 then 1 then 2: failures after reason facet -> search -> sort. */
  const facetedFailed = $derived(filterByReason(failed, reasonFacet));
  const failedHits = $derived(searchOcrDocs(facetedFailed, search));
  const sortedFailed = $derived(
    sortOcrDocs(failedHits.map((h) => h.record), sort),
  );

  /** Slice 3b then 1 then 2: pending after state facet -> search -> sort. */
  const facetedPending = $derived(filterByPendingState(pending, pendingStateFacet));
  const pendingHits = $derived(searchOcrDocs(facetedPending, search));
  const sortedPending = $derived(
    sortOcrDocs(pendingHits.map((h) => h.record), sort),
  );
  /** Cap the rendered pending rows so an unfiltered 5k queue isn't a wall;
      filtering/sorting now makes any specific doc reachable within the cap. */
  const PENDING_CAP = 60;
  const visiblePending = $derived(sortedPending.slice(0, PENDING_CAP));
  const hiddenPending = $derived(Math.max(0, sortedPending.length - PENDING_CAP));

  /** id -> basename highlight ranges, so each row template can paint. */
  const nameRangesById = $derived(
    new Map([...failedHits, ...pendingHits].map((h) => [h.record.id, h.nameRanges])),
  );

  const isFiltering = $derived(
    search.trim().length > 0 || reasonFacet !== null || pendingStateFacet !== null,
  );

  /** Slice 4: one flat cursor space across both rendered lists (failures
      then the capped pending preview), so a single arrow walk crosses
      exactly the rows on screen. */
  const flatRows = $derived(flattenOcrRows(sortedFailed, visiblePending));

  /** Slice 5: pending workload preview + context-aware footer line. */
  const pendingImpact = $derived(summarizePending(pending));
  const pendingImpactLabel = $derived(describeOcrImpact(pendingImpact));
  /** Slice 5b: determinate Run-all progress model (docs + pages done vs
      the snapshotted workload) — drives the overlay bar + label. */
  const runAllProgress = $derived(
    describeRunAllProgress(runAllDone, runAllTotal, runAllPagesDone, runAllPagesTotal),
  );
  /** Compose both facets (failure-reason + pending-state) into one footer
      narration slot — they live in different sections but the footer is a
      single line, so join them when both happen to be active. */
  const facetLabel = $derived(
    [reasonFacet, pendingStateFacet ? pendingStateLabel(pendingStateFacet) : null]
      .filter((s): s is string => !!s)
      .join(" + ") || null,
  );
  const viewSummary = $derived(
    describeOcrView({
      shownFailed: sortedFailed.length,
      shownPending: sortedPending.length,
      totalFailed: failed.length,
      totalPending: pending.length,
      inFlight: stats?.pending ?? 0,
      reasonFacet: facetLabel,
      query: search,
    }),
  );

  const hasFailures = $derived(failed.length > 0);
  const totalDocs = $derived(stats?.total ?? 0);
  const indexedShare = $derived(() => {
    if (!stats || stats.total === 0) return null;
    const indexed = stats.done + stats.text_native;
    const pct = Math.round((indexed * 100) / stats.total);
    return { indexed, pct };
  });

  function setSort(field: OcrSortField) {
    sort = cycleOcrSort(sort, field);
  }

  function toggleReasonFacet(reason: string) {
    reasonFacet = reasonFacet === reason ? null : reason;
  }

  function togglePendingStateFacet(state: string) {
    pendingStateFacet = pendingStateFacet === state ? null : state;
  }

  // Keep a stale facet from silently emptying the inbox after a retry /
  // refresh drops its last failure (Slice 3 reconcile).
  $effect(() => {
    const live = reconcileReasonFacet(reasonFacet, reasonBuckets);
    if (live !== reasonFacet) reasonFacet = live;
  });

  // Keep a stale pending-state facet from silently emptying the pending
  // list after its last image-only/mixed doc is run (Slice 3b reconcile).
  $effect(() => {
    const live = reconcilePendingStateFacet(pendingStateFacet, pendingStateBuckets);
    if (live !== pendingStateFacet) pendingStateFacet = live;
  });

  // Keep the virtual cursor in range as the filtered/sorted lists grow or
  // shrink so it can never point past the end (Slice 4 clamp).
  $effect(() => {
    void flatRows.length;
    cursor = clampOcrCursor(cursor, flatRows.length);
  });

  function scrollCursorIntoView() {
    queueMicrotask(() => {
      rowEls[cursor]?.scrollIntoView({ block: "nearest" });
    });
  }

  /** Slice 4: drive both lists from the keyboard. Returns true when it
      consumed the event so the caller can skip the panel-close path.
      Bails when focus is in the search box or on a button so typing and
      native activation still work. */
  function handleTableKey(e: KeyboardEvent): boolean {
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "BUTTON" || tag === "SELECT") {
      return false;
    }
    const action = classifyOcrTableKey(e);
    if (!action) return false;
    const rows = flatRows;
    switch (action.kind) {
      case "move": {
        if (rows.length === 0) return false;
        e.preventDefault();
        listFocused = true;
        cursor = nextOcrCursor(action.intent, cursor, rows.length);
        scrollCursorIntoView();
        return true;
      }
      case "activate": {
        const row = rows[cursor];
        if (!row || !listFocused) return false;
        e.preventDefault();
        // Enter runs a pending doc or retries a failed one, by section.
        if (row.section === "pending") void runOne(row.record);
        else void requeue(row.record);
        return true;
      }
      case "open": {
        const row = rows[cursor];
        if (!row || !listFocused) return false;
        e.preventDefault();
        void openInReader(row.record);
        return true;
      }
      case "clear": {
        if (!listFocused) return false;
        e.preventDefault();
        listFocused = false;
        return true;
      }
    }
    return false;
  }

</script>

{#if open}
  <div
    class="oq-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="OCR Queue"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}
    onkeydown={handleKey}
    tabindex="-1"
  >
    <div class="oq-shell">
      <header class="oq-head">
        <div class="oq-title">
          <span class="oq-glyph" aria-hidden="true">◳</span>
          <div>
            <h2>OCR Queue</h2>
            <p class="subtitle">
              {#if stats}
                {totalDocs} {totalDocs === 1 ? "doc" : "docs"} indexed ·
                {stats.pending_total} pending ·
                {stats.failed} failed ·
                {stats.done} done
              {:else if loading}
                Loading…
              {:else}
                {error ?? "Queue empty"}
              {/if}
            </p>
          </div>
        </div>
        <div class="oq-actions">
          <button
            class="oq-btn primary"
            onclick={() => runAllPending()}
            disabled={runningAll || (stats?.pending_total ?? 0) === 0}
            title="Run OCR on every scanned/mixed document in the queue"
          >
            {runningAll
              ? `Running… ${runAllProgress.percent}%`
              : `Run all (${stats?.pending_total ?? 0})`}
          </button>
          <button
            class="oq-btn"
            onclick={refresh}
            disabled={loading}
            aria-label="Refresh queue"
            title="Refresh"
          >
            ↻
          </button>
          <button
            class="oq-btn ghost"
            onclick={onClose}
            aria-label="Close"
          >
            Close
          </button>
        </div>
      </header>

      {#if runningAll}
        <div
          class="oq-runall"
          role="progressbar"
          aria-label="Running OCR on the queue"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={runAllProgress.percent}
          aria-valuetext={runAllProgress.label}
        >
          <div class="oq-runall-track">
            <div class="oq-runall-fill" style={`width:${runAllProgress.percent}%`}></div>
          </div>
          <span class="oq-runall-label tabular">{runAllProgress.label}</span>
          <button
            type="button"
            class="oq-runall-cancel"
            onclick={cancelRunAll}
            disabled={cancelRequested}
            title="Stop after the current document finishes"
          >
            {cancelRequested ? "Stopping…" : "Cancel"}
          </button>
        </div>
      {/if}

      {#if !runningAll && resumeBatch.length > 0}
        <!-- Slice 5d: a canceled Run-all left an un-run tail. Offer a
             one-click resume of exactly those docs (planRunRemaining's
             snapshot), plus a dismiss to forget the offer. -->
        <div class="oq-resume" role="status">
          <span class="oq-resume-text">
            Canceled with {resumeBatch.length}
            {resumeBatch.length === 1 ? "document" : "documents"} left to run.
          </span>
          <div class="oq-resume-actions">
            <button
              type="button"
              class="oq-btn primary sm"
              onclick={runRemaining}
              title="Resume OCR on the documents the canceled run never reached"
            >
              {describeRunRemaining(summarizePending(resumeBatch))}
            </button>
            <button
              type="button"
              class="oq-btn ghost sm"
              onclick={() => (resumeBatch = [])}
              title="Dismiss this resume prompt"
            >
              Dismiss
            </button>
          </div>
        </div>
      {/if}

      {#if error}
        <div class="oq-error" role="alert">{error}</div>
      {/if}

      <div class="oq-body">
        {#if stats}
          <section class="oq-stats" aria-label="OCR queue counts">
            <div class="stat">
              <span class="stat-num scanned">{stats.scanned}</span>
              <span class="stat-lbl">scanned</span>
            </div>
            <div class="stat">
              <span class="stat-num mixed">{stats.mixed}</span>
              <span class="stat-lbl">mixed</span>
            </div>
            <div class="stat">
              <span class="stat-num pending">{stats.pending}</span>
              <span class="stat-lbl">in flight</span>
            </div>
            <div class="stat">
              <span class="stat-num done">{stats.done}</span>
              <span class="stat-lbl">done</span>
            </div>
            <div class="stat">
              <span class="stat-num failed">{stats.failed}</span>
              <span class="stat-lbl">failed</span>
            </div>
            <div class="stat">
              <span class="stat-num text">{stats.text_native}</span>
              <span class="stat-lbl">text-native</span>
            </div>
            {#if indexedShare()}
              <div class="stat indexed">
                <span class="stat-num">{indexedShare()!.pct}%</span>
                <span class="stat-lbl">indexed</span>
              </div>
            {/if}
          </section>
        {/if}

        {#if failed.length > 0 || pending.length > 0}
          <div class="oq-toolbar">
            <div class="oq-search">
              <input
                bind:this={searchEl}
                class="oq-search-input"
                type="text"
                placeholder="Filter by name, folder, state, or reason…"
                bind:value={search}
                spellcheck="false"
                autocomplete="off"
                aria-label="Filter OCR queue"
                onkeydown={(e) => {
                  if (e.key === "Escape" && search) {
                    e.preventDefault();
                    e.stopPropagation();
                    search = "";
                  }
                }}
              />
              {#if isFiltering}
                <button
                  class="oq-search-clear"
                  onclick={() => {
                    search = "";
                    reasonFacet = null;
                    pendingStateFacet = null;
                    searchEl?.focus();
                  }}
                  aria-label="Clear filter"
                  title="Clear filter (Esc)"
                >Clear</button>
              {/if}
            </div>
            <div class="oq-sort" role="group" aria-label="Sort OCR queue">
              {#each OCR_SORT_FIELDS as field (field)}
                <button
                  class="oq-sort-btn"
                  class:active={sort.field === field}
                  onclick={() => setSort(field)}
                  aria-pressed={sort.field === field}
                  title={`Sort by ${ocrSortLabel(field)}${sort.field === field ? (sort.dir === "asc" ? " (ascending)" : " (descending)") : ""}`}
                >
                  {ocrSortLabel(field)}
                  {#if sort.field === field}
                    <span class="oq-caret" aria-hidden="true">{sort.dir === "asc" ? "\u2191" : "\u2193"}</span>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        {#if hasFailures}
          <section class="oq-section" aria-label="OCR failures">
            <header class="oq-section-head">
              <h3>
                <span class="dot fail" aria-hidden="true"></span>
                Failures
                <span class="count">
                  {#if isFiltering}({sortedFailed.length} of {failed.length}){:else}({failed.length}){/if}
                </span>
              </h3>
              <button
                class="oq-btn small danger"
                onclick={requeueAllFailed}
                disabled={requeueingAll}
                title="Flip every failed doc back to scanned so the queue retries them all"
              >
                {requeueingAll ? "Re-queueing…" : "Retry all"}
              </button>
            </header>

            {#if reasonBuckets.length > 1}
              <div class="oq-reasons" role="group" aria-label="Filter by failure reason">
                <span class="oq-reasons-lede" title={dominantReason}>{dominantReason}</span>
                {#each reasonBuckets as bucket (bucket.reason)}
                  <button
                    class="oq-reason"
                    class:active={reasonFacet === bucket.reason}
                    onclick={() => toggleReasonFacet(bucket.reason)}
                    aria-pressed={reasonFacet === bucket.reason}
                    title={reasonFacet === bucket.reason
                      ? `Showing only “${bucket.reason}” — click to clear`
                      : `Show only the ${bucket.count} ${bucket.count === 1 ? "doc" : "docs"} that failed: ${bucket.reason}`}
                  >
                    {bucket.reason}
                    <span class="oq-reason-count tabular">{bucket.count}</span>
                  </button>
                {/each}
                {#if reasonRetryLabel}
                  <button
                    class="oq-reason-retry"
                    onclick={requeueReason}
                    disabled={requeueingReason}
                    title={`Re-queue only the ${reasonRetryCount} ${reasonRetryCount === 1 ? "doc" : "docs"} that failed: ${reasonFacet} — leaving every other reason untouched`}
                  >
                    {requeueingReason ? "Re-queueing…" : reasonRetryLabel}
                  </button>
                {/if}
              </div>
            {/if}

            {#if sortedFailed.length === 0}
              <div class="oq-empty">
                {#if reasonFacet && search.trim()}
                  No <span class="oq-empty-q">{reasonFacet}</span> failures match
                  <span class="oq-empty-q">“{search.trim()}”</span>.
                {:else if reasonFacet}
                  No failures under <span class="oq-empty-q">{reasonFacet}</span>.
                {:else}
                  No failures match <span class="oq-empty-q">“{search.trim()}”</span>.
                {/if}
                <button
                  class="oq-link"
                  onclick={() => {
                    search = "";
                    reasonFacet = null;
                  }}
                >Clear filters</button>
              </div>
            {:else}
              <ul class="oq-list">
                {#each sortedFailed as doc, i (doc.id)}
                  <li
                    bind:this={rowEls[i]}
                    class="oq-row failed"
                    class:cursor={listFocused && i === cursor}
                  >
                    <div class="row-main">
                      <div class="row-name" title={doc.path}>
                        {#if doc.title}
                          {doc.title}
                        {:else}
                          {#each splitHighlight(basename(doc.path), nameRangesById.get(doc.id) ?? []) as seg}
                            {#if seg.hit}<mark class="oq-hl">{seg.text}</mark>{:else}{seg.text}{/if}
                          {/each}
                        {/if}
                      </div>
                      <div class="row-meta">
                        <span class="row-folder">{folderHint(doc.path)}</span>
                        {#if doc.ocr_error}
                          <span class="row-reason" title={doc.ocr_error}>
                            {doc.ocr_error}
                          </span>
                        {:else}
                          <span class="row-reason muted">
                            (no reason captured)
                          </span>
                        {/if}
                      </div>
                    </div>
                    <div class="row-actions">
                      <button
                        class="oq-btn small"
                        onclick={() => openInReader(doc)}
                        title="Open in Reader"
                      >
                        Open
                      </button>
                      <button
                        class="oq-btn small"
                        onclick={() => requeue(doc)}
                        disabled={busy.has(doc.id)}
                        title="Flip back to scanned + clear stored error so the next Run picks it up"
                      >
                        {busy.has(doc.id) ? "…" : "Retry"}
                      </button>
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        {/if}

        <section class="oq-section" aria-label="Pending OCR queue">
          <header class="oq-section-head">
            <h3>
              <span class="dot pend" aria-hidden="true"></span>
              Pending
              <span class="count">
                {#if isFiltering}({sortedPending.length} of {pending.length}){:else}({pending.length}){/if}
              </span>
            </h3>
          </header>

          {#if pendingStateBuckets.length > 1}
            <div class="oq-reasons" role="group" aria-label="Filter pending by kind">
              {#each pendingStateBuckets as bucket (bucket.state)}
                <button
                  class="oq-reason"
                  class:active={pendingStateFacet === bucket.state}
                  onclick={() => togglePendingStateFacet(bucket.state)}
                  aria-pressed={pendingStateFacet === bucket.state}
                  title={pendingStateFacet === bucket.state
                    ? `Showing only ${pendingStateLabel(bucket.state)} — click to clear`
                    : `Show only the ${bucket.count} ${bucket.count === 1 ? "doc" : "docs"} that are ${pendingStateLabel(bucket.state)}`}
                >
                  {pendingStateLabel(bucket.state)}
                  <span class="oq-reason-count tabular">{bucket.count}</span>
                </button>
              {/each}
            </div>
          {/if}
          {#if pending.length === 0}
            <div class="oq-empty">
              {#if loading}
                Loading…
              {:else}
                Nothing pending. Add a folder of scanned PDFs to seed the
                queue — Slab classifies each file on import.
              {/if}
            </div>
          {:else if sortedPending.length === 0}
            <div class="oq-empty">
              {#if pendingStateFacet && search.trim()}
                No <span class="oq-empty-q">{pendingStateLabel(pendingStateFacet)}</span> docs match
                <span class="oq-empty-q">“{search.trim()}”</span>.
              {:else if pendingStateFacet}
                No <span class="oq-empty-q">{pendingStateLabel(pendingStateFacet)}</span> docs pending.
              {:else}
                No pending docs match <span class="oq-empty-q">“{search.trim()}”</span>.
              {/if}
              <button
                class="oq-link"
                onclick={() => {
                  search = "";
                  pendingStateFacet = null;
                }}
              >Clear filter</button>
            </div>
          {:else}
            <ul class="oq-list">
              {#each visiblePending as doc, i (doc.id)}
                <li
                  bind:this={rowEls[sortedFailed.length + i]}
                  class="oq-row pending"
                  class:cursor={listFocused && sortedFailed.length + i === cursor}
                >
                  <div class="row-main">
                    <div class="row-name" title={doc.path}>
                      {#if doc.title}
                        {doc.title}
                      {:else}
                        {#each splitHighlight(basename(doc.path), nameRangesById.get(doc.id) ?? []) as seg}
                          {#if seg.hit}<mark class="oq-hl">{seg.text}</mark>{:else}{seg.text}{/if}
                        {/each}
                      {/if}
                    </div>
                    <div class="row-meta">
                      <span class="row-folder">{folderHint(doc.path)}</span>
                      <span class="row-state">
                        {doc.ocr_state === "mixed" ? "mixed pages" : "image-only"}
                      </span>
                    </div>
                  </div>
                  <div class="row-actions">
                    <button
                      class="oq-btn small"
                      onclick={() => openInReader(doc)}
                      title="Open in Reader"
                    >
                      Open
                    </button>
                    <button
                      class="oq-btn small primary"
                      onclick={() => runOne(doc)}
                      disabled={busy.has(doc.id) || runningAll}
                      title="Run OCR on this document now"
                    >
                      {busy.has(doc.id) ? "OCR'ing…" : "Run now"}
                    </button>
                  </div>
                </li>
              {/each}
            </ul>
            {#if hiddenPending > 0}
              <div class="oq-more">
                +{hiddenPending.toLocaleString()} more — filter to narrow, or
                Run all to process every doc.
              </div>
            {/if}
          {/if}
        </section>
      </div>

      <footer class="oq-foot">
        <span class="oq-foot-summary" aria-live="polite">{viewSummary}</span>
        {#if (stats?.pending_total ?? 0) > 0}
          <span class="oq-foot-impact" title="Total workload Run all would process">
            Run all: {pendingImpactLabel}
          </span>
        {/if}
        <span class="oq-kbd-hint" aria-hidden="true">
          <kbd>↑</kbd><kbd>↓</kbd> move · <kbd>↵</kbd> run/retry · <kbd>O</kbd> open
        </span>
      </footer>

      {#if toast}
        <div class="oq-toast" role="status">{toast}</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .oq-backdrop {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, black 45%, transparent);
    backdrop-filter: blur(14px) saturate(140%);
    -webkit-backdrop-filter: blur(14px) saturate(140%);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1500;
    animation: oq-fade-in 140ms ease-out;
  }
  @keyframes oq-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .oq-shell {
    width: min(900px, 94vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    background: var(--panel-bg, #181826);
    color: var(--text, #e7e7f0);
    border: 1px solid color-mix(in srgb, white 8%, transparent);
    border-radius: 16px;
    box-shadow:
      0 24px 64px rgba(0, 0, 0, 0.55),
      0 0 0 1px color-mix(in srgb, white 4%, transparent);
    overflow: hidden;
    animation: oq-pop-in 160ms cubic-bezier(.2,.9,.3,1);
    position: relative;
  }
  @keyframes oq-pop-in {
    from { transform: scale(.97); opacity: 0; }
    to   { transform: scale(1); opacity: 1; }
  }

  .oq-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid color-mix(in srgb, white 7%, transparent);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, white 4%, transparent),
      transparent
    );
  }
  .oq-title { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .oq-glyph {
    font-size: 22px;
    line-height: 1;
    color: color-mix(in srgb, #7ce0c4 70%, transparent);
  }
  .oq-title h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .subtitle { margin: 2px 0 0; font-size: 12px; opacity: 0.62; }

  .oq-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .oq-btn {
    appearance: none;
    border: 1px solid color-mix(in srgb, white 12%, transparent);
    background: color-mix(in srgb, white 6%, transparent);
    color: inherit;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    transition: background 120ms, transform 80ms, border-color 120ms;
  }
  .oq-btn:hover:not(:disabled) { background: color-mix(in srgb, white 11%, transparent); }
  .oq-btn:active:not(:disabled) { transform: translateY(1px); }
  .oq-btn:disabled { opacity: 0.45; cursor: not-allowed; }
  .oq-btn.ghost { background: transparent; }
  .oq-btn.small { padding: 4px 10px; font-size: 12px; }
  .oq-btn.primary {
    border-color: color-mix(in srgb, #7c8cff 60%, transparent);
    background: color-mix(in srgb, #7c8cff 22%, transparent);
    color: #d9deff;
  }
  .oq-btn.primary:hover:not(:disabled) {
    background: color-mix(in srgb, #7c8cff 32%, transparent);
  }
  .oq-btn.danger {
    border-color: color-mix(in srgb, #ff7474 55%, transparent);
    color: #ffb8b8;
  }
  .oq-btn.danger:hover:not(:disabled) {
    background: color-mix(in srgb, #ff7474 18%, transparent);
  }

  .oq-error {
    margin: 10px 22px 0;
    padding: 8px 12px;
    background: color-mix(in srgb, #ff5d6c 18%, transparent);
    border: 1px solid color-mix(in srgb, #ff5d6c 40%, transparent);
    color: #ffb8be;
    border-radius: 8px;
    font-size: 12px;
  }

  /* Slice 5b: determinate Run-all progress bar. Sits just under the
     header while the per-doc batch runs, ticking after every doc. */
  .oq-runall {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 10px 22px 0;
  }
  .oq-runall-track {
    flex: 1;
    height: 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--fg, #fff) 12%, transparent);
    overflow: hidden;
  }
  .oq-runall-fill {
    height: 100%;
    border-radius: 999px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--accent, #7c8cff) 80%, transparent),
      var(--accent, #7c8cff)
    );
    transition: width 200ms ease;
  }
  .oq-runall-label {
    font-size: 11px;
    color: var(--fg-muted, #9aa0aa);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  /* Slice 5c: cancel the in-flight Run-all. A subtle danger-tinted ghost
     button so it reads as an escape hatch, not the primary action. */
  .oq-runall-cancel {
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 600;
    padding: 3px 10px;
    border-radius: 7px;
    border: 1px solid color-mix(in srgb, var(--danger, #e5484d) 35%, transparent);
    background: color-mix(in srgb, var(--danger, #e5484d) 10%, transparent);
    color: color-mix(in srgb, var(--danger, #e5484d) 90%, var(--fg, #fff));
    cursor: pointer;
    white-space: nowrap;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .oq-runall-cancel:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger, #e5484d) 18%, transparent);
    border-color: color-mix(in srgb, var(--danger, #e5484d) 55%, transparent);
  }
  .oq-runall-cancel:disabled {
    opacity: 0.55;
    cursor: default;
  }

  /* Slice 5d: resume-a-canceled-run banner. A calm info strip that sits
     where the progress bar was, offering a one-click "Run remaining" of
     the un-run tail plus a dismiss. */
  .oq-resume {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 10px;
    margin: 10px 22px 0;
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid color-mix(in srgb, var(--accent, #7c8cff) 28%, transparent);
    background: color-mix(in srgb, var(--accent, #7c8cff) 8%, transparent);
  }
  .oq-resume-text {
    font-size: 12px;
    color: var(--fg, #e8eaed);
  }
  .oq-resume-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  /* Compact variant of oq-btn for the inline resume actions. */
  .oq-btn.sm {
    font-size: 11px;
    padding: 4px 10px;
  }

  .oq-body {
    flex: 1;
    overflow-y: auto;
    padding: 14px 18px 18px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  /* ----- stats grid ----- */
  .oq-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(98px, 1fr));
    gap: 8px;
    padding: 12px;
    background: color-mix(in srgb, white 3%, transparent);
    border: 1px solid color-mix(in srgb, white 6%, transparent);
    border-radius: 12px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 8px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, white 2%, transparent);
  }
  .stat-num {
    font-size: 22px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }
  .stat-num.scanned, .stat-num.mixed { color: #c4d0ff; }
  .stat-num.pending { color: #ffd082; }
  .stat-num.done    { color: #7ce0c4; }
  .stat-num.failed  { color: #ff9494; }
  .stat-num.text    { color: color-mix(in srgb, white 60%, transparent); }
  .stat-lbl {
    margin-top: 2px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.6;
  }
  .stat.indexed { background: color-mix(in srgb, #7ce0c4 12%, transparent); }

  /* ----- section heads ----- */
  .oq-section { display: flex; flex-direction: column; }
  .oq-section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 4px 8px;
  }
  .oq-section-head h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.78;
  }
  .count { opacity: 0.5; font-weight: 400; }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    display: inline-block;
  }
  .dot.fail { color: #ff9494; }
  .dot.pend { color: #ffd082; }

  /* ----- list rows ----- */
  .oq-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .oq-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
    border-radius: 10px;
    background: color-mix(in srgb, white 3%, transparent);
    border: 1px solid transparent;
    transition: background 120ms, border-color 120ms;
  }
  .oq-row:hover { background: color-mix(in srgb, white 6%, transparent); }
  .oq-row.failed {
    border-color: color-mix(in srgb, #ff7474 30%, transparent);
    background: color-mix(in srgb, #ff7474 6%, transparent);
  }
  .row-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .row-name {
    font-size: 13px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 11px;
    opacity: 0.74;
    min-width: 0;
  }
  .row-folder {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.7;
    max-width: 38%;
  }
  .row-state {
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.05em;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, #7c8cff 18%, transparent);
    color: #c4d0ff;
  }
  .row-reason {
    color: #ffb0b0;
    font-family: ui-monospace, SF Mono, Menlo, monospace;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .row-reason.muted { color: color-mix(in srgb, white 45%, transparent); }
  .row-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .oq-empty {
    padding: 28px;
    text-align: center;
    opacity: 0.55;
    font-size: 12px;
    border: 1px dashed color-mix(in srgb, white 12%, transparent);
    border-radius: 12px;
  }
  .oq-more {
    margin-top: 6px;
    padding: 6px 12px;
    font-size: 11px;
    opacity: 0.55;
    text-align: center;
  }

  .oq-toast {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 16px;
    background: color-mix(in srgb, #1f1f2e 92%, transparent);
    color: #e7e7f0;
    border: 1px solid color-mix(in srgb, white 12%, transparent);
    border-radius: 999px;
    font-size: 12px;
    box-shadow: 0 8px 24px rgba(0,0,0,.4);
    animation: oq-toast-in 160ms ease-out;
    max-width: 60%;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @keyframes oq-toast-in {
    from { opacity: 0; transform: translate(-50%, 8px); }
    to   { opacity: 1; transform: translate(-50%, 0); }
  }

  /* ----- Atlas VI: search + sort toolbar ----- */
  .oq-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .oq-search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 220px;
  }
  .oq-search-input {
    flex: 1;
    appearance: none;
    background: color-mix(in srgb, white 4%, transparent);
    border: 1px solid color-mix(in srgb, white 10%, transparent);
    color: inherit;
    font-size: 12px;
    padding: 7px 11px;
    padding-right: 60px;
    border-radius: 8px;
    outline: none;
    transition: border-color 120ms, background 120ms;
  }
  .oq-search-input::placeholder { color: inherit; opacity: 0.4; }
  .oq-search-input:focus {
    border-color: color-mix(in srgb, #7c8cff 50%, transparent);
    background: color-mix(in srgb, #7c8cff 6%, transparent);
  }
  .oq-search-clear {
    position: absolute;
    right: 6px;
    appearance: none;
    background: transparent;
    border: none;
    color: inherit;
    opacity: 0.55;
    font-size: 11px;
    padding: 4px 7px;
    border-radius: 6px;
    cursor: pointer;
    transition: opacity 120ms, background 120ms;
  }
  .oq-search-clear:hover {
    opacity: 0.95;
    background: color-mix(in srgb, white 8%, transparent);
  }
  .oq-sort {
    display: flex;
    align-items: center;
    gap: 2px;
    background: color-mix(in srgb, white 3%, transparent);
    border: 1px solid color-mix(in srgb, white 7%, transparent);
    border-radius: 8px;
    padding: 2px;
  }
  .oq-sort-btn {
    appearance: none;
    background: transparent;
    color: inherit;
    border: none;
    padding: 4px 10px;
    font-size: 11px;
    border-radius: 6px;
    cursor: pointer;
    opacity: 0.6;
    transition: background 120ms, opacity 120ms, color 120ms;
  }
  .oq-sort-btn:hover {
    background: color-mix(in srgb, white 6%, transparent);
    opacity: 0.9;
  }
  .oq-sort-btn.active {
    background: color-mix(in srgb, #7c8cff 22%, transparent);
    color: #d9deff;
    opacity: 1;
  }
  .oq-caret { margin-left: 3px; font-size: 10px; opacity: 0.85; }

  /* ----- Atlas VI: failure-reason facets ----- */
  .oq-reasons {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 0 4px 10px;
  }
  .oq-reasons-lede {
    font-size: 11px;
    opacity: 0.6;
    margin-right: 4px;
    white-space: nowrap;
  }
  .oq-reason {
    appearance: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: color-mix(in srgb, #ff7474 8%, transparent);
    border: 1px solid color-mix(in srgb, #ff7474 24%, transparent);
    color: #ffc4c4;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 999px;
    cursor: pointer;
    transition: background 120ms, border-color 120ms;
  }
  .oq-reason:hover {
    background: color-mix(in srgb, #ff7474 16%, transparent);
  }
  .oq-reason.active {
    background: color-mix(in srgb, #ff7474 26%, transparent);
    border-color: color-mix(in srgb, #ff7474 55%, transparent);
    color: #fff;
  }
  .oq-reason-count {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    opacity: 0.8;
    background: color-mix(in srgb, black 22%, transparent);
    padding: 0 5px;
    border-radius: 999px;
  }
  /* Slice 3c: per-reason retry — a clear call-to-action pushed to the end
     of the facet row, only present while a reason is selected. Reads as a
     warm accent rather than the destructive-red the pills use, since
     re-queueing is constructive (flip back to scanned + retry). */
  .oq-reason-retry {
    appearance: none;
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 11px;
    font-size: 11px;
    font-weight: 600;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--accent, #7c8cff) 50%, transparent);
    background: color-mix(in srgb, var(--accent, #7c8cff) 22%, transparent);
    color: var(--fg, #f4f5f7);
    cursor: pointer;
    white-space: nowrap;
    transition: background 120ms, border-color 120ms, opacity 120ms;
  }
  .oq-reason-retry:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent, #7c8cff) 34%, transparent);
    border-color: color-mix(in srgb, var(--accent, #7c8cff) 70%, transparent);
  }
  .oq-reason-retry:disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* ----- Atlas VI: name highlight + cursor ring + filtered empty ----- */
  .oq-hl {
    background: color-mix(in srgb, #7c8cff 40%, transparent);
    color: inherit;
    border-radius: 3px;
    padding: 0 1px;
  }
  .oq-row.cursor {
    border-color: color-mix(in srgb, #7c8cff 65%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, #7c8cff 45%, transparent);
    background: color-mix(in srgb, #7c8cff 10%, transparent);
  }
  .oq-empty-q { color: #c4d0ff; font-weight: 600; }
  .oq-link {
    appearance: none;
    background: none;
    border: none;
    color: #9aa9ff;
    cursor: pointer;
    font-size: inherit;
    padding: 0 2px;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .oq-link:hover { color: #c4d0ff; }

  /* ----- Atlas VI: context-aware footer ----- */
  .oq-foot {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 22px;
    border-top: 1px solid color-mix(in srgb, white 7%, transparent);
    background: color-mix(in srgb, white 2%, transparent);
    font-size: 11px;
    flex-wrap: wrap;
  }
  .oq-foot-summary { opacity: 0.72; }
  .oq-foot-impact {
    opacity: 0.7;
    padding-left: 14px;
    border-left: 1px solid color-mix(in srgb, white 10%, transparent);
  }
  .oq-kbd-hint {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    opacity: 0.5;
    font-size: 10px;
  }
  .oq-kbd-hint kbd {
    font-family: ui-monospace, SF Mono, Menlo, monospace;
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 4px;
    background: color-mix(in srgb, white 8%, transparent);
    border: 1px solid color-mix(in srgb, white 12%, transparent);
  }
  .tabular { font-variant-numeric: tabular-nums; }
</style>
