<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Status = 'linearized' | 'not_linearized' | 'damaged';
  type Stats = {
    first_page_prefix_bytes: number;
    total_bytes: number;
    hint_stream_bytes: number;
    page_count: number;
  };
  type Report = {
    input_path: string;
    output_path: string | null;
    before: Stats;
    after: Stats | null;
    status: Status;
    warnings: string[];
  };
  type AuditEntry = {
    path: string;
    name: string;
    total_bytes: number;
    first_page_prefix_bytes: number;
    page_count: number;
    status: Status;
    error: string | null;
  };
  type AuditReport = {
    root: string;
    recursive: boolean;
    entries: AuditEntry[];
    linearized_count: number;
    not_linearized_count: number;
    damaged_count: number;
    total_bytes: number;
    potential_savings_bytes: number;
    elapsed_ms: number;
  };

  type Mode = 'single' | 'audit';
  type SortKey = 'name' | 'size' | 'status' | 'pages';

  let mode: Mode = $state('single');

  // Single-file inspector state.
  let inputPath = $state('');
  let busy = $state(false);
  let report: Report | null = $state(null);
  let error = $state('');

  // Batch audit state.
  let auditFolder = $state('');
  let recursive = $state(true);
  let auditBusy = $state(false);
  let auditReport: AuditReport | null = $state(null);
  let auditError = $state('');
  let sortKey: SortKey = $state('size');
  let sortDesc = $state(true);
  let filterStatus: 'all' | Status = $state('all');

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function statusBlurb(status: Status): { label: string; color: string; emoji: string } {
    switch (status) {
      case 'linearized':
        return { label: 'Already Fast Web View', color: '#16a34a', emoji: '✓' };
      case 'not_linearized':
        return { label: 'Not optimized for web', color: '#d97706', emoji: '✗' };
      case 'damaged':
        return { label: 'File could not be parsed', color: '#dc2626', emoji: '!' };
    }
  }

  function statusPill(status: Status): { label: string; color: string } {
    switch (status) {
      case 'linearized':
        return { label: 'Optimized', color: '#16a34a' };
      case 'not_linearized':
        return { label: 'Needs optimize', color: '#d97706' };
      case 'damaged':
        return { label: 'Damaged', color: '#dc2626' };
    }
  }

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') {
      inputPath = sel;
      report = null;
      error = '';
    }
  }

  async function pickAuditFolder() {
    if (!isInTauri()) return;
    const sel = await openDialog({ directory: true, multiple: false });
    if (typeof sel === 'string') {
      auditFolder = sel;
      auditReport = null;
      auditError = '';
    }
  }

  async function inspect() {
    if (!inputPath) return;
    busy = true;
    error = '';
    report = null;
    try {
      const res = await invoke<{ Ok?: { value: Report }; Err?: { error: string } }>(
        'slab_streamline_inspect',
        { input: inputPath }
      );
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const r = res as any;
      if (r.Ok) report = r.Ok.value;
      else if (r.Err) error = r.Err.error;
      else report = r as Report;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function runAudit() {
    if (!auditFolder) return;
    auditBusy = true;
    auditError = '';
    auditReport = null;
    try {
      const res = await invoke<{ Ok?: { value: AuditReport }; Err?: { error: string } }>(
        'slab_streamline_audit',
        { folder: auditFolder, recursive, maxFiles: 5000 }
      );
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const r = res as any;
      if (r.Ok) auditReport = r.Ok.value;
      else if (r.Err) auditError = r.Err.error;
      else auditReport = r as AuditReport;
    } catch (e) {
      auditError = String(e);
    } finally {
      auditBusy = false;
    }
  }

  async function linearize() {
    if (!inputPath || !report) return;
    if (!isInTauri()) {
      error = 'Linearize is only available in the desktop app.';
      return;
    }
    // Suggest "<name>.lin.pdf" alongside the source.
    const base = inputPath.replace(/\.pdf$/i, '');
    const suggested = `${base}.lin.pdf`;
    const out = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    });
    if (typeof out !== 'string') return;

    busy = true;
    error = '';
    try {
      const res = await invoke<{ Ok?: { value: Report }; Err?: { error: string } }>(
        'slab_streamline_linearize',
        { input: inputPath, output: out }
      );
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const r = res as any;
      if (r.Ok) report = r.Ok.value;
      else if (r.Err) error = r.Err.error;
      else report = r as Report;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      sortDesc = !sortDesc;
    } else {
      sortKey = key;
      sortDesc = key !== 'name'; // names default ascending, numbers descending
    }
  }

  const filteredEntries = $derived.by(() => {
    if (!auditReport) return [] as AuditEntry[];
    let xs = auditReport.entries.slice();
    if (filterStatus !== 'all') {
      xs = xs.filter((e) => e.status === filterStatus);
    }
    const dir = sortDesc ? -1 : 1;
    xs.sort((a, b) => {
      switch (sortKey) {
        case 'name':
          return a.name.localeCompare(b.name) * dir;
        case 'size':
          return (a.total_bytes - b.total_bytes) * dir;
        case 'pages':
          return (a.page_count - b.page_count) * dir;
        case 'status': {
          const rank: Record<Status, number> = {
            not_linearized: 0,
            damaged: 1,
            linearized: 2,
          };
          return (rank[a.status] - rank[b.status]) * dir;
        }
      }
    });
    return xs;
  });

  function sortArrow(key: SortKey): string {
    if (sortKey !== key) return '';
    return sortDesc ? ' ↓' : ' ↑';
  }

  function exportCsv() {
    if (!auditReport) return;
    const rows = [
      ['path', 'name', 'status', 'page_count', 'total_bytes', 'first_page_prefix_bytes', 'error'],
      ...auditReport.entries.map((e) => [
        e.path,
        e.name,
        e.status,
        String(e.page_count),
        String(e.total_bytes),
        String(e.first_page_prefix_bytes),
        e.error ?? '',
      ]),
    ];
    const csv = rows
      .map((row) =>
        row
          .map((cell) => {
            if (cell.includes(',') || cell.includes('"') || cell.includes('\n')) {
              return `"${cell.replace(/"/g, '""')}"`;
            }
            return cell;
          })
          .join(',')
      )
      .join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `slab-streamline-audit-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }
</script>

<div class="panel">
  <header>
    <h2>Streamline <span class="codename">Fast Web View</span></h2>
    <p class="sub">
      Optimize PDFs so a streaming reader can render page 1 before the rest of the file
      finishes downloading. Acrobat Pro charges $239/yr for this and ships your files to
      their cloud. Slab does it offline.
    </p>
  </header>

  <div class="mode-tabs" role="tablist">
    <button
      role="tab"
      class:active={mode === 'single'}
      aria-selected={mode === 'single'}
      onclick={() => (mode = 'single')}
    >
      Single file
    </button>
    <button
      role="tab"
      class:active={mode === 'audit'}
      aria-selected={mode === 'audit'}
      onclick={() => (mode = 'audit')}
    >
      Batch audit
    </button>
  </div>

  {#if mode === 'single'}
    <div class="row">
      <label>
        Input PDF
        <div class="input-row">
          <input type="text" bind:value={inputPath} placeholder="/path/to/document.pdf" />
          <button onclick={pickInput} disabled={busy || !isInTauri()}>Browse…</button>
        </div>
      </label>
    </div>

    <div class="actions">
      <button class="primary" onclick={inspect} disabled={!inputPath || busy}>
        {busy ? 'Inspecting…' : 'Inspect'}
      </button>
      <button
        class="ghost"
        onclick={linearize}
        disabled={!report || report.status === 'linearized' || busy}
        title="Optimize this PDF for Fast Web View (linearize)"
      >
        Optimize for Fast Web View
      </button>
    </div>

    {#if error}
      <div class="error"><strong>Error:</strong> {error}</div>
    {/if}

    {#if report}
      {@const s = statusBlurb(report.status)}
      <div class="report">
        <div class="status-row" style="--accent: {s.color}">
          <span class="status-pill"
            ><span class="status-emoji">{s.emoji}</span> {s.label}</span
          >
          <span class="page-count">{report.before.page_count} pages</span>
        </div>
        <div class="stats-grid">
          <div class="stat">
            <div class="stat-label">First-page prefix</div>
            <div class="stat-value">{formatBytes(report.before.first_page_prefix_bytes)}</div>
            <div class="stat-hint">
              {report.status === 'linearized'
                ? 'Bytes needed before page 1 paints'
                : 'Reader must download the whole file'}
            </div>
          </div>
          <div class="stat">
            <div class="stat-label">Total size</div>
            <div class="stat-value">{formatBytes(report.before.total_bytes)}</div>
            <div class="stat-hint">File length on disk</div>
          </div>
          {#if report.before.hint_stream_bytes > 0}
            <div class="stat">
              <div class="stat-label">Hint stream</div>
              <div class="stat-value">{formatBytes(report.before.hint_stream_bytes)}</div>
              <div class="stat-hint">Per-page byte offsets</div>
            </div>
          {/if}
        </div>
        {#if report.status === 'not_linearized'}
          <div class="hint-block">
            <strong>Why this matters:</strong> when this PDF is served over HTTP, the
            reader has to download
            <strong>{formatBytes(report.before.total_bytes)}</strong> before page 1 paints.
            After Fast Web View, that drops to typically &lt; 200 KB regardless of total size.
          </div>
        {:else if report.status === 'linearized'}
          <div class="hint-block ok">
            This PDF is already optimized for streaming. A reader can render page 1 after
            fetching only <strong>{formatBytes(report.before.first_page_prefix_bytes)}</strong>
            of <strong>{formatBytes(report.before.total_bytes)}</strong>
            ({((report.before.first_page_prefix_bytes / report.before.total_bytes) * 100).toFixed(
              1
            )}% of the file).
          </div>
        {/if}
      </div>
    {/if}
  {:else}
    <div class="row">
      <label>
        Folder
        <div class="input-row">
          <input
            type="text"
            bind:value={auditFolder}
            placeholder="/path/to/folder/of/pdfs"
          />
          <button onclick={pickAuditFolder} disabled={auditBusy || !isInTauri()}>Browse…</button>
        </div>
      </label>
    </div>
    <div class="row">
      <label class="checkbox-row">
        <input type="checkbox" bind:checked={recursive} disabled={auditBusy} />
        <span>Include subfolders</span>
      </label>
    </div>
    <div class="actions">
      <button class="primary" onclick={runAudit} disabled={!auditFolder || auditBusy}>
        {auditBusy ? 'Auditing…' : 'Audit folder'}
      </button>
      <button class="ghost" onclick={exportCsv} disabled={!auditReport || auditBusy}>
        Export CSV
      </button>
    </div>

    {#if auditError}
      <div class="error"><strong>Error:</strong> {auditError}</div>
    {/if}

    {#if auditReport}
      <div class="audit-summary">
        <div class="summary-stat ok">
          <div class="summary-num">{auditReport.linearized_count}</div>
          <div class="summary-lbl">Already optimized</div>
        </div>
        <div class="summary-stat warn">
          <div class="summary-num">{auditReport.not_linearized_count}</div>
          <div class="summary-lbl">Need optimization</div>
        </div>
        <div class="summary-stat err">
          <div class="summary-num">{auditReport.damaged_count}</div>
          <div class="summary-lbl">Damaged</div>
        </div>
        <div class="summary-stat info">
          <div class="summary-num">{formatBytes(auditReport.total_bytes)}</div>
          <div class="summary-lbl">Total scanned</div>
        </div>
        <div class="summary-stat info">
          <div class="summary-num">{auditReport.elapsed_ms} ms</div>
          <div class="summary-lbl">Scan time</div>
        </div>
      </div>

      {#if auditReport.entries.length > 0}
        <div class="filter-row">
          <label>
            Filter:
            <select bind:value={filterStatus}>
              <option value="all">All files ({auditReport.entries.length})</option>
              <option value="not_linearized"
                >Needs optimize ({auditReport.not_linearized_count})</option
              >
              <option value="linearized"
                >Already optimized ({auditReport.linearized_count})</option
              >
              <option value="damaged">Damaged ({auditReport.damaged_count})</option>
            </select>
          </label>
          <span class="row-count">{filteredEntries.length} shown</span>
        </div>

        <div class="table-wrap">
          <table class="audit-table">
            <thead>
              <tr>
                <th class="sortable" onclick={() => toggleSort('name')}
                  >Name{sortArrow('name')}</th
                >
                <th class="sortable" onclick={() => toggleSort('status')}
                  >Status{sortArrow('status')}</th
                >
                <th class="sortable num" onclick={() => toggleSort('pages')}
                  >Pages{sortArrow('pages')}</th
                >
                <th class="sortable num" onclick={() => toggleSort('size')}
                  >Size{sortArrow('size')}</th
                >
              </tr>
            </thead>
            <tbody>
              {#each filteredEntries as e (e.path)}
                {@const sp = statusPill(e.status)}
                <tr>
                  <td title={e.path}>{e.name}</td>
                  <td>
                    <span class="pill" style="--accent: {sp.color}">{sp.label}</span>
                  </td>
                  <td class="num">{e.page_count || '—'}</td>
                  <td class="num">{formatBytes(e.total_bytes)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <div class="empty">
          No PDFs found in <code>{auditReport.root}</code>.
          {#if !auditReport.recursive}
            Try enabling <em>Include subfolders</em>.
          {/if}
        </div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .panel {
    padding: 20px 24px;
    max-width: 760px;
    color: var(--text-primary, #1f2937);
  }
  header h2 {
    margin: 0 0 4px;
    font-size: 22px;
    font-weight: 600;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .codename {
    font-size: 13px;
    font-weight: 400;
    color: var(--text-secondary, #6b7280);
  }
  .sub {
    margin: 0 0 18px;
    color: var(--text-secondary, #6b7280);
    line-height: 1.5;
    font-size: 13px;
  }
  .row {
    margin-bottom: 14px;
  }
  label {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary, #6b7280);
  }
  .input-row {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  input[type='text'] {
    flex: 1;
    padding: 7px 10px;
    border: 1px solid var(--border, #e5e7eb);
    border-radius: 6px;
    background: var(--bg-input, #fff);
    color: inherit;
    font-size: 13px;
  }
  button {
    padding: 7px 12px;
    border: 1px solid var(--border, #e5e7eb);
    border-radius: 6px;
    background: var(--bg-elev, #f9fafb);
    color: inherit;
    cursor: pointer;
    font-size: 13px;
    transition: background 120ms;
  }
  button:hover:not(:disabled) {
    background: var(--bg-hover, #f3f4f6);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button.primary {
    background: var(--accent-primary, #2563eb);
    color: white;
    border-color: var(--accent-primary, #2563eb);
  }
  button.primary:hover:not(:disabled) {
    background: var(--accent-primary-hover, #1d4ed8);
  }
  button.ghost {
    background: transparent;
  }
  .actions {
    display: flex;
    gap: 10px;
    margin: 14px 0 18px;
  }
  .error {
    padding: 10px 12px;
    border-radius: 6px;
    background: rgba(220, 38, 38, 0.08);
    border: 1px solid rgba(220, 38, 38, 0.2);
    color: #b91c1c;
    font-size: 13px;
    margin-bottom: 14px;
  }
  .report {
    border: 1px solid var(--border, #e5e7eb);
    border-radius: 10px;
    padding: 16px;
    background: var(--bg-elev, #fafafa);
  }
  .status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 14px;
  }
  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
    font-weight: 600;
    font-size: 13px;
  }
  .status-emoji {
    font-weight: 700;
  }
  .page-count {
    font-size: 12px;
    color: var(--text-secondary, #6b7280);
  }
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 12px;
    margin-bottom: 14px;
  }
  .stat {
    background: var(--bg-input, #fff);
    border: 1px solid var(--border, #e5e7eb);
    border-radius: 8px;
    padding: 10px 12px;
  }
  .stat-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary, #6b7280);
  }
  .stat-value {
    font-size: 18px;
    font-weight: 600;
    margin: 4px 0 2px;
    font-variant-numeric: tabular-nums;
  }
  .stat-hint {
    font-size: 11px;
    color: var(--text-tertiary, #9ca3af);
  }
  .hint-block {
    font-size: 13px;
    line-height: 1.55;
    padding: 10px 12px;
    border-radius: 6px;
    background: rgba(217, 119, 6, 0.08);
    border-left: 3px solid #d97706;
  }
  .hint-block.ok {
    background: rgba(22, 163, 74, 0.08);
    border-left-color: #16a34a;
  }
  .mode-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 16px;
    padding: 3px;
    background: var(--bg-elev, #f3f4f6);
    border-radius: 8px;
    width: fit-content;
  }
  .mode-tabs button {
    background: transparent;
    border: 1px solid transparent;
    padding: 5px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
  }
  .mode-tabs button.active {
    background: var(--bg-input, #fff);
    border-color: var(--border, #e5e7eb);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }
  .checkbox-row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .checkbox-row input {
    margin: 0;
  }
  .audit-summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    gap: 10px;
    margin-bottom: 16px;
  }
  .summary-stat {
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border, #e5e7eb);
    background: var(--bg-input, #fff);
  }
  .summary-stat.ok {
    border-left: 3px solid #16a34a;
  }
  .summary-stat.warn {
    border-left: 3px solid #d97706;
  }
  .summary-stat.err {
    border-left: 3px solid #dc2626;
  }
  .summary-stat.info {
    border-left: 3px solid #2563eb;
  }
  .summary-num {
    font-size: 20px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .summary-lbl {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary, #6b7280);
    margin-top: 2px;
  }
  .filter-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
    font-size: 13px;
    color: var(--text-secondary, #6b7280);
  }
  .filter-row select {
    margin-left: 6px;
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid var(--border, #e5e7eb);
    background: var(--bg-input, #fff);
    color: inherit;
    font-size: 13px;
  }
  .row-count {
    font-size: 12px;
    color: var(--text-tertiary, #9ca3af);
  }
  .table-wrap {
    border: 1px solid var(--border, #e5e7eb);
    border-radius: 8px;
    overflow: hidden;
    background: var(--bg-input, #fff);
    max-height: 480px;
    overflow-y: auto;
  }
  .audit-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .audit-table th {
    text-align: left;
    padding: 8px 12px;
    background: var(--bg-elev, #f9fafb);
    border-bottom: 1px solid var(--border, #e5e7eb);
    font-weight: 600;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary, #6b7280);
    position: sticky;
    top: 0;
  }
  .audit-table th.sortable {
    cursor: pointer;
    user-select: none;
  }
  .audit-table th.sortable:hover {
    background: var(--bg-hover, #f3f4f6);
  }
  .audit-table th.num,
  .audit-table td.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .audit-table td {
    padding: 7px 12px;
    border-bottom: 1px solid var(--border, #f3f4f6);
  }
  .audit-table tbody tr:last-child td {
    border-bottom: none;
  }
  .audit-table tbody tr:hover {
    background: var(--bg-hover, #fafafa);
  }
  .pill {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
    font-weight: 600;
    font-size: 11px;
  }
  .empty {
    padding: 14px 16px;
    background: var(--bg-elev, #fafafa);
    border: 1px dashed var(--border, #e5e7eb);
    border-radius: 8px;
    font-size: 13px;
    color: var(--text-secondary, #6b7280);
  }
  .empty code {
    background: var(--bg-input, #fff);
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 12px;
  }
</style>
