<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Rect = { page: number; x: number; y: number; w: number; h: number };

  let inputPath = $state('');
  let outputPath = $state('');
  let rects: Rect[] = $state([{ page: 1, x: 10, y: 10, w: 30, h: 5 }]);
  let busy = $state(false);
  let result = $state('');
  let error = $state('');

  function addRect() {
    const last = rects[rects.length - 1];
    rects = [...rects, last ? { ...last, page: last.page } : { page: 1, x: 10, y: 10, w: 30, h: 5 }];
  }

  function removeRect(i: number) {
    rects = rects.filter((_, idx) => idx !== i);
    if (rects.length === 0) rects = [{ page: 1, x: 10, y: 10, w: 30, h: 5 }];
  }

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') inputPath = sel;
  }

  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: 'redacted.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    if (typeof sel === 'string') outputPath = sel;
  }

  async function run() {
    error = '';
    result = '';
    if (!inputPath || !outputPath) {
      error = 'Pick input and output PDFs';
      return;
    }
    if (rects.length === 0) {
      error = 'Add at least one redaction rectangle';
      return;
    }
    busy = true;
    try {
      const out = await invoke<string>('slab_redact', {
        input: inputPath,
        output: outputPath,
        opts: { rects }
      });
      result = out;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel">
  <header>
    <h2>Redact</h2>
    <p class="hint">Paint solid black rectangles over sensitive regions. Coordinates in % of page size.</p>
  </header>

  <div class="row">
    <label>Input PDF</label>
    <div class="path-row">
      <input type="text" bind:value={inputPath} placeholder="/path/to/input.pdf" />
      <button onclick={pickInput} disabled={!isInTauri()}>Browse</button>
    </div>
  </div>

  <div class="row">
    <label>Output PDF</label>
    <div class="path-row">
      <input type="text" bind:value={outputPath} placeholder="/path/to/redacted.pdf" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row">
    <label>Rectangles</label>
    <div class="rect-list">
      {#each rects as r, i (i)}
        <div class="rect-row">
          <div class="field">
            <span class="lbl">Page</span>
            <input type="number" min="1" bind:value={r.page} />
          </div>
          <div class="field">
            <span class="lbl">X %</span>
            <input type="number" min="0" max="100" step="0.5" bind:value={r.x} />
          </div>
          <div class="field">
            <span class="lbl">Y %</span>
            <input type="number" min="0" max="100" step="0.5" bind:value={r.y} />
          </div>
          <div class="field">
            <span class="lbl">W %</span>
            <input type="number" min="0" max="100" step="0.5" bind:value={r.w} />
          </div>
          <div class="field">
            <span class="lbl">H %</span>
            <input type="number" min="0" max="100" step="0.5" bind:value={r.h} />
          </div>
          <button class="del" onclick={() => removeRect(i)} title="Remove">✕</button>
        </div>
      {/each}
      <button class="add" onclick={addRect}>+ Add rectangle</button>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Redacting…' : 'Redact'}
    </button>
  </div>

  {#if result}
    <div class="result ok">✅ Wrote {result}</div>
  {/if}
  {#if error}
    <div class="result err">⚠ {error}</div>
  {/if}

  <div class="note">
    <strong>Note:</strong> This covers content visually. The underlying text/images remain in the file — true content-stream redaction is on the roadmap. For permanent removal, run the redacted PDF through Compress.
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
    max-width: 900px;
  }
  header h2 {
    margin: 0 0 0.25rem 0;
    font-size: 1.5rem;
    color: var(--text);
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.875rem;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }
  label {
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
  input[type="text"],
  input[type="number"] {
    padding: 0.5rem 0.75rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-family: inherit;
    font-size: 0.875rem;
  }
  input[type="number"] {
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
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 500;
  }
  button.primary:hover {
    background: var(--accent-hover);
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
    color: var(--accent);
    border-style: dashed;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }
  .result {
    padding: 0.75rem 1rem;
    border-radius: 6px;
    font-size: 0.875rem;
  }
  .ok {
    background: rgba(34, 197, 94, 0.1);
    color: rgb(74, 222, 128);
    border: 1px solid rgba(34, 197, 94, 0.3);
  }
  .err {
    background: rgba(239, 68, 68, 0.1);
    color: rgb(248, 113, 113);
    border: 1px solid rgba(239, 68, 68, 0.3);
  }
  .note {
    padding: 0.75rem 1rem;
    background: rgba(251, 191, 36, 0.08);
    border-left: 2px solid rgba(251, 191, 36, 0.5);
    border-radius: 4px;
    color: var(--muted);
    font-size: 0.8125rem;
    line-height: 1.5;
  }
</style>
