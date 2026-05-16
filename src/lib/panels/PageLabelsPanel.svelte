<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Range = { start_page: number; style: string; prefix: string; start: number };

  let inputPath = $state('');
  let outputPath = $state('');
  let ranges: Range[] = $state([
    { start_page: 0, style: 'r', prefix: '', start: 1 },
    { start_page: 5, style: 'D', prefix: '', start: 1 }
  ]);
  let busy = $state(false);
  let result = $state('');
  let error = $state('');

  function addRange() {
    const last = ranges[ranges.length - 1];
    const next = last
      ? { ...last, start_page: last.start_page + 5 }
      : { start_page: 0, style: 'D', prefix: '', start: 1 };
    ranges = [...ranges, next];
  }
  function removeRange(i: number) {
    ranges = ranges.filter((_, idx) => idx !== i);
  }

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') inputPath = sel;
  }
  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: 'labeled.pdf',
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
    if (ranges.length === 0) {
      error = 'Add at least one label range';
      return;
    }
    busy = true;
    try {
      const n = await invoke<number>('slab_page_labels', {
        input: inputPath,
        output: outputPath,
        opts: { ranges }
      });
      result = `Set labels for ${n} page${n === 1 ? '' : 's'} → ${outputPath}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function previewLabel(style: string, prefix: string, start: number): string {
    const examples = [start, start + 1, start + 2];
    return examples.map((n) => prefix + formatNumber(style, n)).join(', ') + ', …';
  }

  function formatNumber(style: string, n: number): string {
    switch (style) {
      case 'D': return String(n);
      case 'R': return toRoman(n, true);
      case 'r': return toRoman(n, false);
      case 'A': return toLetters(n, true);
      case 'a': return toLetters(n, false);
      default:  return ''; // prefix-only
    }
  }
  function toRoman(num: number, upper: boolean): string {
    const m = [['M',1000],['CM',900],['D',500],['CD',400],['C',100],['XC',90],['L',50],['XL',40],['X',10],['IX',9],['V',5],['IV',4],['I',1]] as const;
    let n = num, out = '';
    for (const [r, v] of m) { while (n >= (v as number)) { out += r; n -= (v as number); } }
    return upper ? out : out.toLowerCase();
  }
  function toLetters(num: number, upper: boolean): string {
    const base = upper ? 'A'.charCodeAt(0) : 'a'.charCodeAt(0);
    const cycle = Math.ceil(num / 26);
    const letter = String.fromCharCode(base + ((num - 1) % 26));
    return letter.repeat(cycle);
  }
</script>

<div class="panel">
  <header>
    <h2>Page Labels</h2>
    <p class="hint">
      Set how page numbers display in PDF readers — roman numerals for front matter, arabic for body, prefixes for chapters.
      The actual page positions don't change; only the labels shown in viewers do.
    </p>
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
      <input type="text" bind:value={outputPath} placeholder="/path/to/labeled.pdf" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row">
    <label>Label Ranges</label>
    <div class="range-list">
      {#each ranges as r, i (i)}
        <div class="range-row">
          <div class="field">
            <span class="lbl">From page (0-based)</span>
            <input type="number" min="0" bind:value={r.start_page} />
          </div>
          <div class="field">
            <span class="lbl">Style</span>
            <select bind:value={r.style}>
              <option value="D">Decimal (1, 2, 3)</option>
              <option value="R">Uppercase Roman (I, II, III)</option>
              <option value="r">Lowercase Roman (i, ii, iii)</option>
              <option value="A">Uppercase Letters (A, B, C)</option>
              <option value="a">Lowercase Letters (a, b, c)</option>
              <option value="">Prefix only</option>
            </select>
          </div>
          <div class="field">
            <span class="lbl">Prefix</span>
            <input type="text" bind:value={r.prefix} placeholder="Ch-" />
          </div>
          <div class="field">
            <span class="lbl">Start at</span>
            <input type="number" min="1" bind:value={r.start} />
          </div>
          <button class="del" onclick={() => removeRange(i)} title="Remove">✕</button>
        </div>
        <div class="preview">Preview: <code>{previewLabel(r.style, r.prefix, r.start)}</code></div>
      {/each}
      <button class="add" onclick={addRange}>+ Add range</button>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Applying…' : 'Apply Labels'}
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
    max-width: 1000px;
  }
  header h2 { margin: 0 0 0.25rem 0; font-size: 1.5rem; color: var(--text); }
  .hint { margin: 0; color: var(--muted); font-size: 0.875rem; line-height: 1.5; }
  .row { display: flex; flex-direction: column; gap: 0.375rem; }
  label { color: var(--muted); font-size: 0.8125rem; font-weight: 500; }
  .path-row { display: flex; gap: 0.5rem; }
  .path-row input { flex: 1; }
  input[type="text"], input[type="number"], select {
    padding: 0.5rem 0.75rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-family: inherit;
    font-size: 0.875rem;
  }
  input[type="number"] { width: 100%; }
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
  .range-list { display: flex; flex-direction: column; gap: 0.5rem; }
  .range-row {
    display: grid;
    grid-template-columns: 1fr 1.5fr 1fr 1fr auto;
    gap: 0.5rem;
    align-items: end;
  }
  .field { display: flex; flex-direction: column; gap: 0.25rem; }
  .lbl {
    font-size: 0.6875rem;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .preview {
    font-size: 0.75rem;
    color: var(--muted);
    padding-left: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .preview code {
    background: var(--bg-input);
    padding: 0.125rem 0.375rem;
    border-radius: 3px;
    font-family: ui-monospace, monospace;
    color: var(--text);
  }
  .del { padding: 0.5rem 0.75rem; color: var(--muted); }
  .del:hover { color: #ff6b6b; }
  .add {
    align-self: flex-start;
    background: transparent;
    color: var(--accent);
    border-style: dashed;
  }
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
</style>
