<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Preset = { id: string; label: string; description: string };
  const PRESETS: Preset[] = [
    { id: 'email', label: 'Email addresses', description: 'name@domain.com style' },
    { id: 'ssn',   label: 'US SSN',          description: 'XXX-XX-XXXX' },
    { id: 'phone', label: 'Phone numbers',   description: 'NANP (US/Canada) format' },
    { id: 'cc',    label: 'Credit cards',    description: '16-digit card numbers' }
  ];

  let inputPath = $state('');
  let outputPath = $state('');
  let selected: Record<string, boolean> = $state({ email: true, ssn: true, phone: false, cc: false });
  let customPatterns: string[] = $state([]);
  let newPattern = $state('');
  let gray = $state(0);
  let busy = $state(false);
  let result = $state('');
  let error = $state('');

  function addPattern() {
    const p = newPattern.trim();
    if (!p) return;
    customPatterns = [...customPatterns, p];
    newPattern = '';
  }
  function removePattern(i: number) {
    customPatterns = customPatterns.filter((_, idx) => idx !== i);
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
    const presets = Object.entries(selected).filter(([, v]) => v).map(([k]) => k);
    if (presets.length === 0 && customPatterns.length === 0) {
      error = 'Select at least one preset or add a custom pattern';
      return;
    }
    busy = true;
    try {
      const n = await invoke<number>('slab_auto_redact', {
        input: inputPath,
        output: outputPath,
        opts: { presets, patterns: customPatterns, gray }
      });
      result = n === 0
        ? `No matches found. Copy of input written → ${outputPath}`
        : `Redacted ${n} match${n === 1 ? '' : 'es'} → ${outputPath}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel">
  <header>
    <h2>Auto-Redact</h2>
    <p class="hint">
      Find sensitive patterns (emails, SSNs, phone numbers, credit cards) across the PDF
      and paint solid bars over them. Run any custom regex too. Visual redaction —
      see note below for permanent removal.
    </p>
  </header>

  <div class="row">
    <span class="row-label">Input PDF</span>
    <div class="path-row">
      <input type="text" bind:value={inputPath} placeholder="/path/to/input.pdf" />
      <button onclick={pickInput} disabled={!isInTauri()}>Browse</button>
    </div>
  </div>

  <div class="row">
    <span class="row-label">Output PDF</span>
    <div class="path-row">
      <input type="text" bind:value={outputPath} placeholder="/path/to/redacted.pdf" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row" role="group" aria-label="Built-in Presets">
    <span class="row-label">Built-in Presets</span>
    <div class="preset-grid">
      {#each PRESETS as p (p.id)}
        <label class="preset">
          <input type="checkbox" bind:checked={selected[p.id]} />
          <div class="preset-text">
            <div class="preset-label">{p.label}</div>
            <div class="preset-desc">{p.description}</div>
          </div>
        </label>
      {/each}
    </div>
  </div>

  <div class="row" role="group" aria-label="Custom Regex Patterns">
    <span class="row-label">Custom Regex Patterns</span>
    <div class="pattern-input-row">
      <input
        type="text"
        bind:value={newPattern}
        placeholder="e.g. \bACME-\d{6}\b"
        onkeydown={(e) => e.key === 'Enter' && addPattern()}
      />
      <button onclick={addPattern}>Add</button>
    </div>
    {#if customPatterns.length > 0}
      <div class="pattern-list">
        {#each customPatterns as p, i (p)}
          <div class="pattern-pill">
            <code>{p}</code>
            <button class="pill-del" onclick={() => removePattern(i)} title="Remove">✕</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="row inline">
    <div class="field">
      <label>
        <span class="row-label">Bar Color (gray)</span>
        <input type="range" min="0" max="1" step="0.05" bind:value={gray} />
      </label>
      <span class="muted">{(gray * 100).toFixed(0)}% — {gray === 0 ? 'pure black' : gray === 1 ? 'white' : 'gray'}</span>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={run} disabled={busy}>
      {busy ? 'Scanning…' : 'Find & Redact'}
    </button>
  </div>

  {#if result}
    <div class="result ok">✅ {result}</div>
  {/if}
  {#if error}
    <div class="result err">⚠ {error}</div>
  {/if}

  <div class="note">
    <strong>Visual redaction only:</strong> the matching text bytes remain in the underlying
    content stream. For permanent removal, run the output through Compress (rewrites streams)
    or convert to image-only via a future export feature. Bounding boxes are line-level
    approximations — a full bar covers the line containing each match.
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
  .row.inline { flex-direction: row; gap: 1rem; }
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
  .preset-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .preset {
    display: flex;
    gap: 0.5rem;
    padding: 0.625rem 0.75rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
    align-items: flex-start;
  }
  .preset:hover { background: var(--bg-hover); }
  .preset input { margin-top: 0.125rem; }
  .preset-text { display: flex; flex-direction: column; gap: 0.125rem; }
  .preset-label { color: var(--text); font-size: 0.875rem; font-weight: 500; }
  .preset-desc { color: var(--muted); font-size: 0.75rem; }
  .pattern-input-row { display: flex; gap: 0.5rem; }
  .pattern-input-row input { flex: 1; font-family: ui-monospace, monospace; }
  .pattern-list { display: flex; flex-wrap: wrap; gap: 0.375rem; margin-top: 0.5rem; }
  .pattern-pill {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem 0.5rem;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.8125rem;
  }
  .pattern-pill code {
    font-family: ui-monospace, monospace;
    color: var(--text);
  }
  .pill-del {
    padding: 0 0.25rem;
    background: transparent;
    border: none;
    color: var(--muted);
    font-size: 0.75rem;
  }
  .pill-del:hover { color: #ff6b6b; }
  .field { display: flex; flex-direction: column; gap: 0.25rem; min-width: 200px; }
  .muted { color: var(--muted); font-size: 0.75rem; }
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
