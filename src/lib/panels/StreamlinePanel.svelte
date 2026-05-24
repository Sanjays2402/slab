<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
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

  let inputPath = $state('');
  let busy = $state(false);
  let report: Report | null = $state(null);
  let error = $state('');

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

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') {
      inputPath = sel;
      report = null;
      error = '';
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
      // Tauri serializes our externally-tagged CmdResult as { Ok: {...} } | { Err: {...} }.
      // Be defensive: accept both shapes.
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

  async function linearize() {
    if (!inputPath || !report) return;
    // Writer lands in Task 6 of the plan — surface the WIP state today.
    error =
      'Linearization writer ships in v3.13.0 — landing in subsequent ticks. ' +
      'The inspector you just used is the first half of the feature.';
  }
</script>

<div class="panel">
  <header>
    <h2>Streamline <span class="codename">Fast Web View</span></h2>
    <p class="sub">
      Optimize PDFs so a streaming reader can render page 1 before the rest of the file
      finishes downloading. Acrobat Pro charges $239/yr for this. Slab does it free, offline.
    </p>
  </header>

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
      title="Linearize this PDF (writer lands in v3.13.0)"
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
</style>
