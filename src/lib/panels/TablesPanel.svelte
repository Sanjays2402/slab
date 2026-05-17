<script lang="ts">
  // TablesPanel — v0.13.0 "Lens" Slice 3.
  //
  // Detect tables on a chosen page of a text-native PDF and let the user
  // copy or save each one as CSV. Driven entirely by the backend
  // `pdf::table_extract` module (pdftotext -bbox-layout → row/column
  // clustering → 2-D Table). Works on any PDF that already has real
  // text; scanned PDFs need to go through OCR first (the Reader's
  // auto-OCR banner handles that flow).
  //
  // Layout:
  //   - Toolbar: file picker, page input, Detect Tables button.
  //   - Empty state: friendly hint.
  //   - Result: one card per detected table with a 4-row preview
  //     (collapsible to show all rows) plus Copy CSV and Save CSV
  //     buttons.

  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    slabExtractTables,
    slabTableToCsv,
    slabTableSaveCsv,
    type Table,
  } from "$lib/lens";
  import { basename, stripExt } from "$lib/types";

  let input = $state<string | null>(null);
  let page = $state(1);
  let minRows = $state(2);
  let minCols = $state(2);
  let tables = $state<Table[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let expanded = $state<Set<number>>(new Set());
  let toast = $state<{ kind: "ok" | "err"; msg: string } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function flashToast(kind: "ok" | "err", msg: string) {
    if (toastTimer) clearTimeout(toastTimer);
    toast = { kind, msg };
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 2200);
  }

  async function pickInput() {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    tables = [];
    error = null;
    expanded = new Set();
  }

  async function detect() {
    if (!input) {
      error = "Pick a PDF first.";
      return;
    }
    if (!Number.isInteger(page) || page < 1) {
      error = "Page must be ≥ 1.";
      return;
    }
    loading = true;
    error = null;
    tables = [];
    expanded = new Set();
    try {
      const out = await slabExtractTables(input, {
        page,
        min_rows: minRows,
        min_cols: minCols,
      });
      tables = out;
      if (out.length === 0) {
        error = `No tables detected on page ${page} (try lowering min rows/cols).`;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function toggleExpanded(idx: number) {
    const next = new Set(expanded);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    expanded = next;
  }

  async function copyCsv(t: Table) {
    try {
      const csv = await slabTableToCsv(t);
      await navigator.clipboard.writeText(csv);
      flashToast("ok", `Copied ${t.rows.length} rows × ${t.columns} cols`);
    } catch (e) {
      flashToast("err", String(e));
    }
  }

  async function saveCsv(t: Table) {
    if (!input) return;
    const stem = stripExt(basename(input));
    const defaultPath = `${stem}-p${t.page}-t${t.index}.csv`;
    const out = await saveDialog({
      defaultPath,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (typeof out !== "string") return;
    try {
      const saved = await slabTableSaveCsv(t, out);
      flashToast("ok", `Saved → ${basename(saved)}`);
    } catch (e) {
      flashToast("err", String(e));
    }
  }

  function previewRows(t: Table, idx: number): string[][] {
    if (expanded.has(idx)) return t.rows;
    return t.rows.slice(0, 4);
  }
</script>

<header class="content-header">
  <h1>Tables</h1>
  <p class="subtitle">
    Detect 2-D tables on a page of a text-native PDF and export them as CSV.
    Scanned PDFs must be OCR'd first.
  </p>
</header>

<section class="panel">
  <div class="toolbar">
    <div class="file-slot">
      {#if input}
        <div class="file-name" title={input}>{basename(input)}</div>
        <button class="ghost" onclick={pickInput}>Change</button>
      {:else}
        <button class="primary" onclick={pickInput}>Choose PDF…</button>
      {/if}
    </div>

    <label class="num">
      <span>Page</span>
      <input type="number" min="1" bind:value={page} />
    </label>

    <label class="num small" title="Minimum rows for a candidate to count">
      <span>Min rows</span>
      <input type="number" min="1" max="20" bind:value={minRows} />
    </label>

    <label class="num small" title="Minimum columns for a candidate to count">
      <span>Min cols</span>
      <input type="number" min="1" max="20" bind:value={minCols} />
    </label>

    <button class="primary" onclick={detect} disabled={!input || loading}>
      {loading ? "Detecting…" : "Detect Tables"}
    </button>
  </div>

  {#if error}
    <div class="status err">⚠ {error}</div>
  {/if}

  {#if !input}
    <div class="empty">
      <div class="empty-title">No PDF selected</div>
      <div class="empty-hint">
        Pick a text-native PDF to scan a page for tables. (For scanned PDFs,
        run OCR first via the Reader's auto-OCR banner.)
      </div>
    </div>
  {:else if tables.length === 0 && !loading && !error}
    <div class="empty">
      <div class="empty-title">Ready</div>
      <div class="empty-hint">
        Set the page number and click <strong>Detect Tables</strong>.
      </div>
    </div>
  {/if}

  {#each tables as t, idx (idx)}
    <article class="table-card">
      <header class="card-head">
        <div>
          <div class="card-title">Table {t.index} · page {t.page}</div>
          <div class="card-meta">
            {t.rows.length} rows × {t.columns} cols
          </div>
        </div>
        <div class="card-actions">
          <button class="ghost" onclick={() => copyCsv(t)}>Copy CSV</button>
          <button class="ghost" onclick={() => saveCsv(t)}>Save CSV…</button>
        </div>
      </header>

      <div class="table-scroll">
        <table class="preview">
          <tbody>
            {#each previewRows(t, idx) as row, ri (ri)}
              <tr class:header-row={ri === 0}>
                {#each row as cell, ci (ci)}
                  <td>{cell}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      {#if t.rows.length > 4}
        <button class="link" onclick={() => toggleExpanded(idx)}>
          {expanded.has(idx)
            ? "Show fewer rows"
            : `Show all ${t.rows.length} rows`}
        </button>
      {/if}
    </article>
  {/each}

  {#if toast}
    <div class="toast {toast.kind}">{toast.msg}</div>
  {/if}
</section>

<style>
  .toolbar {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .file-slot {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 200px;
    flex: 1;
  }
  .file-name {
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 360px;
  }
  .file-slot .ghost {
    align-self: flex-start;
  }
  .num {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .num input {
    width: 88px;
    padding: 6px 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: var(--r-sm);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .num.small input {
    width: 72px;
  }
  .empty {
    border: 1px dashed var(--border);
    border-radius: var(--r-md);
    padding: 24px;
    text-align: center;
    color: var(--text-2);
    background: var(--bg-2);
  }
  .empty-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 4px;
  }
  .empty-hint {
    font-size: 12px;
  }
  .status.err {
    background: rgba(244, 114, 114, 0.1);
    color: #f47272;
    border: 1px solid rgba(244, 114, 114, 0.3);
    padding: 8px 12px;
    border-radius: var(--r-sm);
    font-size: 13px;
    margin-bottom: 12px;
  }
  .table-card {
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-2);
    margin-bottom: 16px;
    overflow: hidden;
  }
  .card-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }
  .card-title {
    font-weight: 600;
    color: var(--text);
    font-size: 13px;
  }
  .card-meta {
    color: var(--text-2);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    margin-top: 2px;
  }
  .card-actions {
    display: flex;
    gap: 6px;
  }
  .table-scroll {
    overflow-x: auto;
    max-width: 100%;
  }
  table.preview {
    border-collapse: collapse;
    font-size: 12px;
    width: 100%;
    table-layout: auto;
  }
  table.preview td {
    border: 1px solid var(--border);
    padding: 4px 8px;
    vertical-align: top;
    color: var(--text);
    white-space: nowrap;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  tr.header-row td {
    background: var(--bg-3);
    font-weight: 600;
    color: var(--text);
  }
  .link {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    padding: 8px 14px;
    font-size: 12px;
    text-align: left;
    width: 100%;
  }
  .link:hover {
    text-decoration: underline;
  }
  .toast {
    position: fixed;
    bottom: 24px;
    right: 24px;
    padding: 10px 14px;
    border-radius: var(--r-sm);
    font-size: 13px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    z-index: 100;
  }
  .toast.ok {
    background: rgba(126, 231, 135, 0.15);
    color: #7ee787;
    border: 1px solid rgba(126, 231, 135, 0.4);
  }
  .toast.err {
    background: rgba(244, 114, 114, 0.15);
    color: #f47272;
    border: 1px solid rgba(244, 114, 114, 0.4);
  }
</style>
