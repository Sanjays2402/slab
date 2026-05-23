<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  /** A redaction rectangle in page-percentage space. */
  type Rect = {
    page: number;
    left_pct: number;
    bottom_pct: number;
    right_pct: number;
    top_pct: number;
  };

  type SanitizeFlatReport = {
    info_fields_cleared: number;
    xmp_metadata_removed: boolean;
    embedded_files_removed: number;
    javascript_removed: number;
    structure_tree_removed: boolean;
  };
  type TrueRedactReport = {
    rects_painted: number;
    text_runs_excised: number;
    annotations_removed: number;
    sanitize: SanitizeFlatReport;
  };

  let inputPath = $state('');
  let outputPath = $state('');
  let gray = $state(0);
  let rects: Rect[] = $state([
    { page: 1, left_pct: 10, bottom_pct: 70, right_pct: 50, top_pct: 78 }
  ]);
  let busy = $state(false);
  let report: TrueRedactReport | null = $state(null);
  let error = $state('');

  function addRect() {
    const last = rects[rects.length - 1];
    rects = [
      ...rects,
      last
        ? { ...last }
        : { page: 1, left_pct: 10, bottom_pct: 70, right_pct: 50, top_pct: 78 }
    ];
  }
  function removeRect(i: number) {
    rects = rects.filter((_, idx) => idx !== i);
    if (rects.length === 0)
      rects = [{ page: 1, left_pct: 10, bottom_pct: 70, right_pct: 50, top_pct: 78 }];
  }

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') inputPath = sel;
  }
  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: 'redacted-true.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    if (typeof sel === 'string') outputPath = sel;
  }

  async function run() {
    error = '';
    report = null;
    if (!inputPath || !outputPath) {
      error = 'Pick an input PDF and choose where to save the redacted copy.';
      return;
    }
    if (rects.length === 0) {
      error = 'Add at least one redaction rectangle.';
      return;
    }
    busy = true;
    try {
      report = await invoke<TrueRedactReport>('slab_redact_true', {
        input: inputPath,
        output: outputPath,
        opts: { rects, gray }
      });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel">
  <header>
    <div class="title-row">
      <h2>Veil <span class="badge">True</span></h2>
    </div>
    <p class="hint">
      Destructive redaction that <strong>removes the underlying text, annotations, and
      metadata</strong> — not just a black bar on top. Compliance-grade. 100% offline.
    </p>
  </header>

  <div class="why">
    <div class="why-row"><span>✓</span> Text bytes excised from content stream (pdftotext recovers nothing)</div>
    <div class="why-row"><span>✓</span> Overlapping annotations dropped (no hidden /Contents leaks)</div>
    <div class="why-row"><span>✓</span> Document metadata scrubbed (Title, Author, XMP, embedded files, JS)</div>
    <div class="why-row"><span>✓</span> Incremental update history flattened (no old object versions)</div>
    <div class="why-row"><span>✓</span> Visible black bars painted on top for the human-eye check</div>
  </div>

  <div class="row">
    <span class="row-label">Input PDF</span>
    <div class="path-row">
      <input
        type="text"
        bind:value={inputPath}
        placeholder="/path/to/input.pdf"
        aria-label="Input PDF path"
      />
      <button onclick={pickInput} disabled={!isInTauri()}>Browse</button>
    </div>
  </div>

  <div class="row">
    <span class="row-label">Output PDF</span>
    <div class="path-row">
      <input
        type="text"
        bind:value={outputPath}
        placeholder="/path/to/redacted-true.pdf"
        aria-label="Output PDF path"
      />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row" role="group" aria-label="Rectangles">
    <span class="row-label">Redaction rectangles (page %)</span>
    <div class="rect-list">
      {#each rects as r, i (i)}
        <div class="rect-row">
          <div class="field">
            <span class="lbl">Page</span>
            <input type="number" min="1" bind:value={r.page} />
          </div>
          <div class="field">
            <span class="lbl">L %</span>
            <input
              type="number"
              min="0"
              max="100"
              step="0.5"
              bind:value={r.left_pct}
            />
          </div>
          <div class="field">
            <span class="lbl">B %</span>
            <input
              type="number"
              min="0"
              max="100"
              step="0.5"
              bind:value={r.bottom_pct}
            />
          </div>
          <div class="field">
            <span class="lbl">R %</span>
            <input
              type="number"
              min="0"
              max="100"
              step="0.5"
              bind:value={r.right_pct}
            />
          </div>
          <div class="field">
            <span class="lbl">T %</span>
            <input
              type="number"
              min="0"
              max="100"
              step="0.5"
              bind:value={r.top_pct}
            />
          </div>
          <button class="del" onclick={() => removeRect(i)} title="Remove">✕</button>
        </div>
      {/each}
      <button class="add" onclick={addRect}>+ Add rectangle</button>
    </div>
  </div>

  <div class="row">
    <span class="row-label">Bar shade</span>
    <div class="gray-row">
      <input
        type="range"
        min="0"
        max="1"
        step="0.05"
        bind:value={gray}
        aria-label="Bar gray level"
      />
      <span class="gray-val">
        {gray === 0 ? 'black' : gray === 1 ? 'white' : `gray ${(gray * 100).toFixed(0)}%`}
      </span>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Redacting…' : 'Veil it'}
    </button>
  </div>

  {#if report}
    <div class="result ok">
      <div class="ok-title">✓ Saved {outputPath}</div>
      <div class="report">
        <div><strong>{report.rects_painted}</strong> rectangle{report.rects_painted === 1 ? '' : 's'} painted</div>
        <div><strong>{report.text_runs_excised}</strong> text run{report.text_runs_excised === 1 ? '' : 's'} excised</div>
        <div><strong>{report.annotations_removed}</strong> annotation{report.annotations_removed === 1 ? '' : 's'} removed</div>
        <div>
          <strong>{report.sanitize.info_fields_cleared}</strong> metadata field{report.sanitize.info_fields_cleared === 1 ? '' : 's'} cleared
          {#if report.sanitize.xmp_metadata_removed}, XMP packet stripped{/if}
          {#if report.sanitize.javascript_removed > 0}, JavaScript stripped{/if}
          {#if report.sanitize.embedded_files_removed > 0}, embedded files dropped{/if}
          {#if report.sanitize.structure_tree_removed}, structure tree removed{/if}
        </div>
      </div>
    </div>
  {/if}
  {#if error}
    <div class="result err">⚠ {error}</div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
    max-width: 900px;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  header h2 {
    margin: 0 0 0.25rem 0;
    font-size: 1.5rem;
    color: var(--text);
  }
  .badge {
    display: inline-block;
    background: linear-gradient(135deg, #6366f1, #8b5cf6);
    color: #fff;
    font-size: 0.6875rem;
    font-weight: 600;
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    vertical-align: middle;
    margin-left: 0.5rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.875rem;
    line-height: 1.5;
  }
  .why {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.75rem 1rem;
    background: rgba(99, 102, 241, 0.06);
    border-left: 2px solid rgba(99, 102, 241, 0.5);
    border-radius: 4px;
    font-size: 0.8125rem;
    color: var(--text);
  }
  .why-row {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
  }
  .why-row span {
    color: rgb(99, 102, 241);
    font-weight: 600;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }
  .row-label {
    color: var(--muted);
    font-size: 0.8125rem;
    font-weight: 500;
  }
  .path-row {
    display: flex;
    gap: 0.5rem;
  }
  .path-row input {
    flex: 1;
  }
  input[type='text'],
  input[type='number'] {
    padding: 0.5rem 0.75rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-family: inherit;
    font-size: 0.875rem;
  }
  input[type='number'] {
    width: 100%;
  }
  button {
    padding: 0.5rem 1rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    cursor: pointer;
    font-size: 0.875rem;
    transition: background 0.1s;
  }
  button:hover {
    background: var(--bg-hover);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  button.primary {
    background: linear-gradient(135deg, #6366f1, #8b5cf6);
    border-color: #6366f1;
    color: #fff;
    font-weight: 600;
  }
  button.primary:hover {
    filter: brightness(1.1);
  }
  .rect-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .rect-row {
    display: grid;
    grid-template-columns: repeat(5, 1fr) auto;
    gap: 0.5rem;
    align-items: end;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .lbl {
    font-size: 0.6875rem;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .del {
    padding: 0.5rem 0.75rem;
    color: var(--muted);
  }
  .del:hover {
    color: #ff6b6b;
  }
  .add {
    align-self: flex-start;
    background: transparent;
    color: rgb(99, 102, 241);
    border-style: dashed;
  }
  .gray-row {
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }
  .gray-row input[type='range'] {
    flex: 1;
  }
  .gray-val {
    color: var(--muted);
    font-size: 0.8125rem;
    min-width: 5rem;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }
  .result {
    padding: 0.875rem 1rem;
    border-radius: 6px;
    font-size: 0.875rem;
  }
  .ok {
    background: rgba(99, 102, 241, 0.08);
    color: var(--text);
    border: 1px solid rgba(99, 102, 241, 0.3);
  }
  .ok-title {
    color: rgb(129, 140, 248);
    font-weight: 600;
    margin-bottom: 0.5rem;
  }
  .report {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    color: var(--muted);
    font-size: 0.8125rem;
  }
  .report strong {
    color: var(--text);
  }
  .err {
    background: rgba(239, 68, 68, 0.1);
    color: rgb(248, 113, 113);
    border: 1px solid rgba(239, 68, 68, 0.3);
  }
</style>
