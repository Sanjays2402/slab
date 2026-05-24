<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Report = {
    pages: number;
    paragraphs: number;
    headings: number;
    list_items: number;
    table_rows: number;
    bytes_written: number;
    duration_ms: number;
  };

  let inputPath = $state('');
  let busy = $state(false);
  let report: Report | null = $state(null);
  let outputPath = $state('');
  let error = $state('');
  let dragging = $state(false);

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
      error = 'Reflow runs in the desktop app — file system access required.';
      return;
    }
    // Default output: source.pdf -> source.docx alongside the input.
    const base = inputPath.replace(/\.pdf$/i, '');
    const suggested = `${base}.docx`;
    const out = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: 'Word document', extensions: ['docx'] }],
    });
    if (typeof out !== 'string') return;

    busy = true;
    error = '';
    report = null;
    outputPath = '';
    try {
      const res = await invoke<{ kind?: string; value?: Report; message?: string }>(
        'slab_reflow_to_docx',
        { input: inputPath, output: out }
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
    // In Tauri 2, dragged files expose their absolute path via the `path`
    // property on a (non-standard) File extension. Fall back to the name if
    // that isn't available (browser preview mode).
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
    <h2>Reflow <span class="codename">PDF → Word</span></h2>
    <p class="sub">
      Turn any PDF into a fully-editable Microsoft Word document — locally, offline, free.
      Adobe Acrobat Pro charges $239/yr for this and uploads your file to their cloud. Slab
      doesn't.
    </p>
  </header>

  <div
    class="dropzone"
    class:dragging
    ondragover={onDragOver}
    ondragleave={onDragLeave}
    ondrop={onDrop}
    role="region"
    aria-label="Drop a PDF here to convert to Word"
  >
    <div class="dropzone-flow" aria-hidden="true">
      <span class="dz-icon">📄</span>
      <span class="dz-arrow">→</span>
      <span class="dz-icon">⚙️</span>
      <span class="dz-arrow">→</span>
      <span class="dz-icon">📝</span>
    </div>
    <p class="dropzone-headline">Drop a PDF here, get a .docx back.</p>
    <p class="dropzone-tagline">
      Headings, bullets, numbered lists and tables come through. Pure offline — no cloud,
      no telemetry, no upload limits.
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

  <div class="actions">
    <button class="primary" onclick={convert} disabled={!inputPath || busy}>
      {busy ? 'Converting…' : 'Convert to Word (.docx)'}
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
          <div class="stat-value">{report.pages}</div>
          <div class="stat-label">pages</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.paragraphs}</div>
          <div class="stat-label">paragraphs</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.headings}</div>
          <div class="stat-label">headings</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.list_items}</div>
          <div class="stat-label">list items</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.table_rows}</div>
          <div class="stat-label">table rows</div>
        </div>
        <div class="stat">
          <div class="stat-value">{formatBytes(report.bytes_written)}</div>
          <div class="stat-label">file size</div>
        </div>
        <div class="stat">
          <div class="stat-value">{report.duration_ms} ms</div>
          <div class="stat-label">conversion time</div>
        </div>
      </div>
      <p class="result-hint">
        Open the file in Word, Pages, LibreOffice, or Google Docs — it's a real OOXML
        document, not an export-by-print stub.
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
