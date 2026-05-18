<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  let inputPath = $state('');
  let outputPath = $state('');
  let n = $state(4);
  let gap = $state(6);
  let margin = $state(12);
  let busy = $state(false);
  let result = $state('');
  let error = $state('');

  const nOptions = [2, 4, 6, 9];

  // Grid layout for preview
  const layouts: Record<number, { cols: number; rows: number }> = {
    2: { cols: 2, rows: 1 },
    4: { cols: 2, rows: 2 },
    6: { cols: 3, rows: 2 },
    9: { cols: 3, rows: 3 }
  };

  let layout = $derived(layouts[n]);

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') inputPath = sel;
  }

  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: `${n}up.pdf`,
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
    busy = true;
    try {
      const out = await invoke<string>('slab_nup', {
        input: inputPath,
        output: outputPath,
        opts: { n, gap_pt: gap, margin_pt: margin }
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
    <h2>N-up</h2>
    <p class="hint">Combine multiple pages onto a single sheet — great for printing handouts or saving paper.</p>
  </header>

  <div class="row">
    <label>Input PDF</label>
    <div class="path-row">
      <input type="text" bind:value={inputPath} placeholder="/path/to/input.pdf" aria-label="Input PDF path" />
      <button onclick={pickInput} disabled={!isInTauri()}>Browse</button>
    </div>
  </div>

  <div class="row">
    <label>Output PDF</label>
    <div class="path-row">
      <input type="text" bind:value={outputPath} placeholder="/path/to/nup.pdf" aria-label="Output PDF path" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row">
    <label>Pages per sheet</label>
    <div class="seg">
      {#each nOptions as opt (opt)}
        <button
          class="seg-btn"
          class:active={n === opt}
          onclick={() => (n = opt)}
        >
          {opt}-up
        </button>
      {/each}
    </div>
  </div>

  <div class="grid-2">
    <div class="row">
      <label for="nup-gap">Gap (pt)</label>
      <input id="nup-gap" type="number" min="0" max="60" step="1" bind:value={gap} />
    </div>
    <div class="row">
      <label for="nup-margin">Margin (pt)</label>
      <input id="nup-margin" type="number" min="0" max="60" step="1" bind:value={margin} />
    </div>
  </div>

  <div class="row">
    <label>Preview</label>
    <div class="preview">
      <div
        class="sheet"
        style="grid-template-columns: repeat({layout.cols}, 1fr); grid-template-rows: repeat({layout.rows}, 1fr); padding: {margin / 4}px; gap: {gap / 4}px;"
      >
        {#each Array(n) as _, i (i)}
          <div class="cell">{i + 1}</div>
        {/each}
      </div>
      <span class="preview-label">{layout.cols} × {layout.rows} grid</span>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Composing…' : `Make ${n}-up PDF`}
    </button>
  </div>

  {#if result}
    <div class="result ok">✅ Wrote {result}</div>
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
    max-width: 720px;
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
  .grid-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
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
  .seg {
    display: flex;
    gap: 0.25rem;
    background: var(--bg-input);
    padding: 0.25rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    width: fit-content;
  }
  .seg-btn {
    padding: 0.375rem 1rem;
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
  }
  .seg-btn.active {
    background: var(--accent);
    color: #fff;
  }
  .preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }
  .sheet {
    width: 180px;
    height: 240px;
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
  }
  .cell {
    background: #f1f5f9;
    border: 1px dashed #cbd5e1;
    border-radius: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #64748b;
    font-size: 0.75rem;
    font-weight: 600;
  }
  .preview-label {
    color: var(--muted);
    font-size: 0.75rem;
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
</style>
