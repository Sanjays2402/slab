<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  const SAMPLE = `# Welcome to Slab

This is a **Markdown** to *PDF* converter built right into the app.

## Features

- Headings (H1–H6)
- **Bold**, *italic*, \`code\`
- Bullet & numbered lists
- Block code & blockquotes
- Auto pagination

> No fonts get embedded — output stays tiny.

---

Try editing this and clicking **Convert**.
`;

  let markdown = $state(SAMPLE);
  let pageSize = $state<'A4' | 'Letter' | 'Legal'>('A4');
  let outputPath = $state('');
  let busy = $state(false);
  let result = $state('');
  let error = $state('');

  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: 'document.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    if (typeof sel === 'string') outputPath = sel;
  }

  async function run() {
    error = '';
    result = '';
    if (!outputPath) {
      error = 'Pick an output path first';
      return;
    }
    if (!markdown.trim()) {
      error = 'Markdown is empty';
      return;
    }
    busy = true;
    try {
      const n = await invoke<number>('slab_md2pdf', {
        output: outputPath,
        opts: { markdown, page_size: pageSize }
      });
      result = `Wrote ${n} page${n === 1 ? '' : 's'} → ${outputPath}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  let charCount = $derived(markdown.length);
  let lineCount = $derived(markdown.split('\n').length);
</script>

<div class="panel">
  <header>
    <h2>Markdown → PDF</h2>
    <p class="hint">Convert Markdown text directly into a clean, lightweight PDF. No font embedding — output is tiny.</p>
  </header>

  <div class="row editor-row">
    <label for="md-source">Markdown source <span class="muted">({charCount} chars · {lineCount} lines)</span></label>
    <textarea id="md-source" bind:value={markdown} spellcheck="false"></textarea>
  </div>

  <div class="row inline">
    <div class="field">
      <label for="md-page-size">Page size</label>
      <select id="md-page-size" bind:value={pageSize}>
        <option value="A4">A4</option>
        <option value="Letter">US Letter</option>
        <option value="Legal">US Legal</option>
      </select>
    </div>
  </div>

  <div class="row">
    <label>Output PDF</label>
    <div class="path-row">
      <input type="text" bind:value={outputPath} placeholder="/path/to/document.pdf" aria-label="Output PDF path" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Converting…' : 'Convert to PDF'}
    </button>
  </div>

  {#if result}
    <div class="result ok">✅ {result}</div>
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
  .row.inline {
    flex-direction: row;
    gap: 1rem;
  }
  .editor-row textarea {
    min-height: 320px;
    padding: 0.75rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-family: ui-monospace, "SF Mono", "Menlo", monospace;
    font-size: 0.8125rem;
    line-height: 1.5;
    resize: vertical;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    min-width: 160px;
  }
  label {
    color: var(--muted);
    font-size: 0.8125rem;
    font-weight: 500;
  }
  .muted {
    color: var(--muted);
    font-weight: 400;
    font-size: 0.75rem;
  }
  .path-row {
    display: flex;
    gap: 0.5rem;
  }
  .path-row input {
    flex: 1;
  }
  input[type="text"], select {
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
