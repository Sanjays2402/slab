<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Report = {
    pages: number;
    sheets: number;
    tables: number;
    rows: number;
    cells: number;
    numericCells: number;
    dateCells: number;
    bytesWritten: number;
    durationMs: number;
  };

  let inputPath = $state('');
  let busy = $state(false);
  let report: Report | null = $state(null);
  let outputPath = $state('');
  let error = $state('');
  let dragging = $state(false);

  // Three knobs — sensible defaults match the Rust side.
  let typeNumbers = $state(true);
  let typeDates = $state(true);
  let includeNonTableText = $state(false);

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(2)} MB`;
  }

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') {
      inputPath = sel;
      report = null;
      outputPath = '';
      error = '';
    }
  }

  async function convert() {
    if (!inputPath) return;
    if (!isInTauri()) {
      error = 'Tabulate runs in the desktop app — file system access required.';
      return;
    }
    const base = inputPath.replace(/\.pdf$/i, '');
    const suggested = `${base}.xlsx`;
    const out = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: 'Excel workbook', extensions: ['xlsx'] }],
    });
    if (typeof out !== 'string') return;

    busy = true;
    error = '';
    report = null;
    outputPath = '';
    try {
      const res = await invoke<{ kind?: string; value?: Report; message?: string }>(
        'slab_tabulate_to_xlsx',
        {
          input: inputPath,
          output: out,
          typeNumbers,
          typeDates,
          includeNonTableText,
        }
      );
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const r = res as any;
      if (r.kind === 'ok' || r.value) {
        report = (r.value ?? r) as Report;
        outputPath = out;
      } else if (r.kind === 'err' || r.message) {
        error = r.message ?? 'Conversion failed.';
      } else {
        report = r as Report;
        outputPath = out;
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function onDragOver(ev: DragEvent) {
    ev.preventDefault();
    dragging = true;
  }
  function onDragLeave(ev: DragEvent) {
    ev.preventDefault();
    dragging = false;
  }
  async function onDrop(ev: DragEvent) {
    ev.preventDefault();
    dragging = false;
    const file = ev.dataTransfer?.files?.[0];
    if (!file) return;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const anyFile = file as any;
    if (typeof anyFile.path === 'string') {
      inputPath = anyFile.path;
      report = null;
      outputPath = '';
      error = '';
    } else {
      error = 'Drag-and-drop is only available inside the desktop app — use Browse instead.';
    }
  }
</script>

<div class="panel">
  <header>
    <h2>Tabulate <span class="codename">PDF → Excel</span></h2>
    <p class="sub">
      Detect every table in a PDF and emit a real Excel workbook — one sheet per page,
      numbers typed as numbers, dates as dates, ready for formulas. Adobe Acrobat Pro
      charges $239/yr and uploads your file to their cloud. Slab does it free, offline,
      batchable.
    </p>
  </header>

  <div
    class="dropzone"
    class:dragging
    ondragover={onDragOver}
    ondragleave={onDragLeave}
    ondrop={onDrop}
    role="region"
    aria-label="Drop a PDF here to convert to Excel"
  >
    <div class="dropzone-flow" aria-hidden="true">
      <span class="dz-icon">📄</span>
      <span class="dz-arrow">→</span>
      <span class="dz-icon">⚙️</span>
      <span class="dz-arrow">→</span>
      <span class="dz-icon">📊</span>
    </div>
    <p class="dropzone-headline">Drop a PDF here, get an .xlsx back.</p>
    <p class="dropzone-tagline">
      Detects aligned-column tables, types numbers and dates, preserves headers.
      Open the result in Excel, Numbers, Google Sheets or LibreOffice Calc.
    </p>
  </div>

  <div class="row">
    <label>
      Input PDF
      <div class="input-row">
        <input type="text" bind:value={inputPath} placeholder="/path/to/document.pdf" />
        <button onclick={pickInput} disabled={busy || !isInTauri()}>Browse…</button>
      </div>
    </label>
  </div>

  <div class="knobs">
    <label class="knob">
      <input type="checkbox" bind:checked={typeNumbers} />
      <div>
        <div class="knob-label">Type numbers</div>
        <div class="knob-help">Parse "$1,234.50", "12.5%", "(1,200)" as numeric cells.</div>
      </div>
    </label>
    <label class="knob">
      <input type="checkbox" bind:checked={typeDates} />
      <div>
        <div class="knob-label">Type dates</div>
        <div class="knob-help">Parse ISO 8601, US, EU and long-month formats as date cells.</div>
      </div>
    </label>
    <label class="knob">
      <input type="checkbox" bind:checked={includeNonTableText} />
      <div>
        <div class="knob-label">Include body text</div>
        <div class="knob-help">Add a "Body Text" sheet with non-table paragraphs.</div>
      </div>
    </label>
  </div>

  <div class="actions">
    <button class="primary" onclick={convert} disabled={!inputPath || busy}>
      {busy ? 'Converting…' : 'Convert to Excel (.xlsx)'}
    </button>
  </div>

  {#if error}
    <div class="error"><strong>Error:</strong> {error}</div>
  {/if}

  {#if report && outputPath}
    <div class="result">
      <div class="result-headline">
        <span class="check">✓</span> Done — saved as <code>{outputPath}</code>
      </div>
      <div class="stats-grid">
        <div class="stat">
          <div class="stat-value">{report.sheets}</div>
          <div class="stat-label">sheets</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.tables}</div>
          <div class="stat-label">tables</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.rows}</div>
          <div class="stat-label">rows</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.cells}</div>
          <div class="stat-label">cells</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.numericCells}</div>
          <div class="stat-label">typed numbers</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.dateCells}</div>
          <div class="stat-label">typed dates</div>
        </div>
        <div class="stat">
          <div class="stat-value">{formatBytes(report.bytesWritten)}</div>
          <div class="stat-label">file size</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.durationMs} ms</div>
          <div class="stat-label">conversion time</div>
        </div>
      </div>
      <p class="result-hint">
        Open in Excel, Numbers, Google Sheets or LibreOffice Calc — it's a real OOXML
        workbook with typed cells and a date number-format style. Formulas can reference
        any cell directly.
      </p>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 24px;
    max-width: 880px;
  }
  header h2 {
    margin: 0 0 4px 0;
    font-size: 22px;
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .codename {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-muted, #888);
    letter-spacing: 0.02em;
  }
  .sub {
    margin: 0;
    color: var(--text-muted, #888);
    font-size: 14px;
    line-height: 1.5;
  }
  .dropzone {
    border: 2px dashed var(--border, #e5e5e5);
    border-radius: 12px;
    padding: 24px;
    text-align: center;
    background: var(--surface-2, #fafafa);
    transition: background 120ms ease, border-color 120ms ease;
  }
  .dropzone.dragging {
    border-color: var(--accent, #2563eb);
    background: color-mix(in srgb, var(--accent, #2563eb) 8%, transparent);
  }
  .dropzone-flow {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
    font-size: 32px;
    margin-bottom: 10px;
  }
  .dz-arrow {
    color: var(--text-muted, #888);
    font-size: 20px;
  }
  .dropzone-headline {
    font-size: 16px;
    font-weight: 600;
    margin: 0 0 4px 0;
  }
  .dropzone-tagline {
    margin: 0;
    color: var(--text-muted, #888);
    font-size: 13px;
    line-height: 1.5;
  }
  .row label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    color: var(--text-muted, #666);
  }
  .input-row {
    display: flex;
    gap: 8px;
  }
  .input-row input {
    flex: 1;
    padding: 8px 10px;
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 6px;
    font-size: 13px;
  }
  .knobs {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 10px;
    background: var(--surface-2, #fafafa);
  }
  .knob {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    cursor: pointer;
    font-size: 13px;
  }
  .knob input[type='checkbox'] {
    margin-top: 2px;
  }
  .knob-label {
    font-weight: 600;
    margin-bottom: 2px;
  }
  .knob-help {
    color: var(--text-muted, #888);
    font-size: 12px;
    line-height: 1.4;
  }
  .actions {
    display: flex;
    gap: 10px;
  }
  button {
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 13px;
    border: 1px solid var(--border, #e5e5e5);
    background: var(--surface-2, #fafafa);
    cursor: pointer;
  }
  button.primary {
    background: var(--accent, #2563eb);
    color: white;
    border-color: var(--accent, #2563eb);
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .error {
    padding: 12px;
    border-radius: 8px;
    background: color-mix(in srgb, #dc2626 10%, transparent);
    color: #b91c1c;
    font-size: 13px;
  }
  .result {
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 12px;
    padding: 20px;
    background: var(--surface-1, white);
  }
  .result-headline {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    margin-bottom: 16px;
    word-break: break-all;
  }
  .result-headline .check {
    display: inline-flex;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: #16a34a;
    color: white;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
    gap: 12px;
    margin-bottom: 12px;
  }
  .stat {
    padding: 10px;
    border-radius: 8px;
    background: var(--surface-2, #fafafa);
    text-align: center;
  }
  .stat-value {
    font-size: 18px;
    font-weight: 600;
  }
  .stat-label {
    font-size: 11px;
    color: var(--text-muted, #888);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .result-hint {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted, #888);
    line-height: 1.5;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
    background: var(--surface-2, #fafafa);
    padding: 2px 4px;
    border-radius: 3px;
  }
</style>
