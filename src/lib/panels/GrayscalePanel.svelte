<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  let inputPath = $state('');
  let outputPath = $state('');
  let pagesText = $state('');
  let busy = $state(false);
  let result = $state('');
  let error = $state('');

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') inputPath = sel;
  }

  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: 'grayscale.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    if (typeof sel === 'string') outputPath = sel;
  }

  function parsePages(s: string): number[] {
    if (!s.trim()) return [];
    return s
      .split(',')
      .map((p) => Number(p.trim()))
      .filter((n) => Number.isFinite(n) && n > 0);
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
      const pages = parsePages(pagesText);
      const n = await invoke<number>('slab_grayscale', {
        input: inputPath,
        output: outputPath,
        opts: { pages }
      });
      result = `Rewrote ${n} content stream${n === 1 ? '' : 's'} → ${outputPath}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel">
  <header>
    <h2>Color → Grayscale</h2>
    <p class="hint">
      Convert RGB and CMYK fills/strokes to gray inside the PDF's content streams.
      Uses ITU-R BT.601 luminance (0.299R + 0.587G + 0.114B). Vector-true — no rasterization.
    </p>
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
      <input type="text" bind:value={outputPath} placeholder="/path/to/grayscale.pdf" aria-label="Output PDF path" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row">
    <label for="gs-pages">Pages (1-based, comma-separated — empty = all)</label>
    <input id="gs-pages" type="text" bind:value={pagesText} placeholder="e.g. 1,2,5  (or leave blank for every page)" />
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Converting…' : 'Convert to Grayscale'}
    </button>
  </div>

  {#if result}
    <div class="result ok">✅ {result}</div>
  {/if}
  {#if error}
    <div class="result err">⚠ {error}</div>
  {/if}

  <div class="note">
    <strong>Note:</strong> This rewrites color operators in vector content. Embedded raster
    images are <em>not</em> converted in this pass — for those, use a future "Image Compress"
    pipeline. Text and shapes go gray instantly.
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
  header h2 { margin: 0 0 0.25rem 0; font-size: 1.5rem; color: var(--text); }
  .hint { margin: 0; color: var(--muted); font-size: 0.875rem; line-height: 1.5; }
  .row { display: flex; flex-direction: column; gap: 0.375rem; }
  label { color: var(--muted); font-size: 0.8125rem; font-weight: 500; }
  .path-row { display: flex; gap: 0.5rem; }
  .path-row input { flex: 1; }
  input[type="text"] {
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
  button:hover { background: var(--bg-hover); }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 500;
  }
  button.primary:hover { background: var(--accent-hover); }
  .actions { display: flex; gap: 0.5rem; margin-top: 0.5rem; }
  .result { padding: 0.75rem 1rem; border-radius: 6px; font-size: 0.875rem; }
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
