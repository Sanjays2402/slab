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
    ocrQueueRunAll,
    ocrQueueRequeue,
    ocrQueueRequeueAllFailed,
    ocrQueueStats,
    type DocumentRecord,
    type OcrQueueResult,
    type OcrQueueStats,
  } from "$lib/library";

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
  let requeueingAll = $state(false);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

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

  function basename(path: string): string {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return i >= 0 ? path.slice(i + 1) : path;
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

  async function runAllPending() {
    if (runningAll || (stats?.pending_total ?? 0) === 0) return;
    runningAll = true;
    error = null;
    try {
      const results = await ocrQueueRunAll(null);
      const ok = results.filter((r) => r.state_after === "ocr_done").length;
      const fail = results.filter((r) => r.state_after === "ocr_failed").length;
      showToast(
        `OCR queue: ${ok} succeeded, ${fail} failed (of ${results.length})`,
      );
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      runningAll = false;
    }
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
    if (open) refresh();
  });

  // ---------- derived display helpers ----------

  const pendingPreview = $derived(pending.slice(0, 20));
  const hasMorePending = $derived(pending.length > 20);
  const hasFailures = $derived(failed.length > 0);
  const totalDocs = $derived(stats?.total ?? 0);
  const indexedShare = $derived(() => {
    if (!stats || stats.total === 0) return null;
    const indexed = stats.done + stats.text_native;
    const pct = Math.round((indexed * 100) / stats.total);
    return { indexed, pct };
  });
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
            onclick={runAllPending}
            disabled={runningAll || (stats?.pending_total ?? 0) === 0}
            title="Run OCR on every scanned/mixed document in the queue"
          >
            {runningAll
              ? `Running ${stats?.pending_total ?? 0}…`
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

        {#if hasFailures}
          <section class="oq-section" aria-label="OCR failures">
            <header class="oq-section-head">
              <h3>
                <span class="dot fail" aria-hidden="true"></span>
                Failures
                <span class="count">({failed.length})</span>
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
            <ul class="oq-list">
              {#each failed as doc (doc.id)}
                <li class="oq-row failed">
                  <div class="row-main">
                    <div class="row-name" title={doc.path}>
                      {doc.title ?? basename(doc.path)}
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
          </section>
        {/if}

        <section class="oq-section" aria-label="Pending OCR queue">
          <header class="oq-section-head">
            <h3>
              <span class="dot pend" aria-hidden="true"></span>
              Pending
              <span class="count">
                ({stats?.pending_total ?? 0})
              </span>
            </h3>
          </header>
          {#if (stats?.pending_total ?? 0) === 0}
            <div class="oq-empty">
              Nothing pending. Add a folder of scanned PDFs to seed the
              queue — Slab classifies each file on import.
            </div>
          {:else}
            <ul class="oq-list">
              {#each pendingPreview as doc (doc.id)}
                <li class="oq-row pending">
                  <div class="row-main">
                    <div class="row-name" title={doc.path}>
                      {doc.title ?? basename(doc.path)}
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
            {#if hasMorePending}
              <div class="oq-more">
                +{pending.length - 20} more queued — Run all to process every doc.
              </div>
            {/if}
          {/if}
        </section>
      </div>

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
</style>
