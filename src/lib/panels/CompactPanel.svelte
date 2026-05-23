<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Preset = 'screen' | 'ebook' | 'printer' | 'prepress' | 'custom';

  type ImageEstimate = {
    object_id_num: number;
    original_bytes: number;
    projected_bytes: number;
    will_resample: boolean;
    reason: string;
  };
  type EstimateReport = {
    original_bytes: number;
    projected_bytes: number;
    projected_ratio: number;
    images_total: number;
    images_resampled: number;
    per_image: ImageEstimate[];
  };
  type CompactReport = {
    original_bytes: number;
    new_bytes: number;
    ratio: number;
    images_total: number;
    images_rewritten: number;
    images_skipped: number;
    bytes_saved_images: number;
    thumbnails_dropped: number;
    metadata_stripped: boolean;
    embedded_files_stripped: boolean;
    js_stripped: boolean;
    warnings: string[];
  };

  let inputPath = $state('');
  let outputPath = $state('');
  let preset: Preset = $state('ebook');
  let busy = $state(false);
  let estimating = $state(false);
  let estimate: EstimateReport | null = $state(null);
  let report: CompactReport | null = $state(null);
  let error = $state('');

  const PRESET_BLURBS: Record<Preset, { label: string; sub: string; dpi: number; q: number }> =
    {
      screen: { label: 'Screen', sub: 'Email / screen reading', dpi: 72, q: 60 },
      ebook: { label: 'eBook', sub: 'Tablets, laptops (recommended)', dpi: 150, q: 75 },
      printer: { label: 'Printer', sub: 'Safe for laser print', dpi: 300, q: 85 },
      prepress: { label: 'Prepress', sub: 'Minimal loss, keep metadata', dpi: 300, q: 90 },
      custom: { label: 'Custom', sub: 'Bring your own knobs', dpi: 150, q: 75 }
    };

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function pickInput() {
    if (!isInTauri()) return;
    const sel = await openDialog({ filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (typeof sel === 'string') {
      inputPath = sel;
      estimate = null;
      report = null;
      // Auto-derive a sensible default output path.
      outputPath = inputPath.replace(/\.pdf$/i, '') + '-compact.pdf';
    }
  }
  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: outputPath || 'compact.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    });
    if (typeof sel === 'string') outputPath = sel;
  }

  async function runEstimate() {
    if (!inputPath) {
      error = 'Pick an input PDF first.';
      return;
    }
    error = '';
    estimate = null;
    estimating = true;
    try {
      estimate = await invoke<EstimateReport>('slab_compactor_estimate', {
        input: inputPath,
        preset,
        custom: null
      });
    } catch (e) {
      error = String(e);
    } finally {
      estimating = false;
    }
  }

  async function runCompact() {
    if (!inputPath || !outputPath) {
      error = 'Pick an input PDF and an output path.';
      return;
    }
    error = '';
    report = null;
    busy = true;
    try {
      report = await invoke<CompactReport>('slab_compactor_compact', {
        input: inputPath,
        output: outputPath,
        preset,
        custom: null
      });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function reductionPct(orig: number, next: number): number {
    if (orig <= 0) return 0;
    return Math.max(0, Math.min(100, Math.round((1 - next / orig) * 100)));
  }
</script>

<div class="panel">
  <header>
    <div class="title-row">
      <h2>Compactor <span class="badge">Reduce file size</span></h2>
    </div>
    <p class="hint">
      Shrink large PDFs by <strong>downsampling images, re-encoding as JPEG,
      and dropping metadata + embedded files + JavaScript</strong>. The same
      "Reduce File Size" feature Adobe Acrobat Pro charges $239/yr for —
      free, offline, on your machine.
    </p>
  </header>

  <div class="why">
    <div class="why-row"><span>✓</span> Real downsample (not just stream re-flate — that saves &lt;2%)</div>
    <div class="why-row"><span>✓</span> Four presets matching Ghostscript naming (Screen / eBook / Printer / Prepress)</div>
    <div class="why-row"><span>✓</span> Dry-run estimate before you commit — see exactly how much you'll save</div>
    <div class="why-row"><span>✓</span> 100% offline — files never leave your machine (Smallpdf, iLovePDF: they upload)</div>
  </div>

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
      <input type="text" bind:value={outputPath} placeholder="/path/to/compact.pdf" />
      <button onclick={pickOutput} disabled={!isInTauri()}>Save as</button>
    </div>
  </div>

  <div class="row">
    <span class="row-label">Quality preset</span>
    <div class="preset-grid">
      {#each (['screen', 'ebook', 'printer', 'prepress'] as Preset[]) as p}
        <button
          class="preset-card"
          class:active={preset === p}
          onclick={() => {
            preset = p;
            estimate = null;
          }}
        >
          <div class="preset-label">{PRESET_BLURBS[p].label}</div>
          <div class="preset-sub">{PRESET_BLURBS[p].sub}</div>
          <div class="preset-spec">{PRESET_BLURBS[p].dpi} dpi · JPEG q{PRESET_BLURBS[p].q}</div>
        </button>
      {/each}
    </div>
  </div>

  <div class="actions">
    <button
      class="secondary"
      onclick={runEstimate}
      disabled={estimating || busy || !inputPath}
    >
      {estimating ? 'Estimating…' : 'Estimate savings'}
    </button>
    <button class="primary" onclick={runCompact} disabled={busy || !inputPath || !outputPath}>
      {busy ? 'Compacting…' : 'Compact'}
    </button>
  </div>

  {#if estimate && !report}
    <div class="dial-wrap" aria-live="polite">
      <div class="dial">
        <div class="dial-num">{reductionPct(estimate.original_bytes, estimate.projected_bytes)}%</div>
        <div class="dial-cap">smaller (est.)</div>
      </div>
      <div class="dial-info">
        <div class="dial-line">
          <span>Original</span><strong>{formatBytes(estimate.original_bytes)}</strong>
        </div>
        <div class="dial-line">
          <span>Projected</span><strong>{formatBytes(estimate.projected_bytes)}</strong>
        </div>
        <div class="dial-line">
          <span>Images</span><strong>{estimate.images_resampled} / {estimate.images_total} will resample</strong>
        </div>
      </div>
    </div>
  {/if}

  {#if report}
    <div class="dial-wrap done" aria-live="polite">
      <div class="dial">
        <div class="dial-num">{reductionPct(report.original_bytes, report.new_bytes)}%</div>
        <div class="dial-cap">smaller</div>
      </div>
      <div class="dial-info">
        <div class="dial-line">
          <span>Original</span><strong>{formatBytes(report.original_bytes)}</strong>
        </div>
        <div class="dial-line">
          <span>New</span><strong>{formatBytes(report.new_bytes)}</strong>
        </div>
        <div class="dial-line">
          <span>Images rewritten</span
          ><strong>{report.images_rewritten} / {report.images_total}</strong>
        </div>
        {#if report.thumbnails_dropped > 0}
          <div class="dial-line">
            <span>Thumbnails dropped</span><strong>{report.thumbnails_dropped}</strong>
          </div>
        {/if}
        {#if report.metadata_stripped}
          <div class="dial-line tag">Metadata stripped</div>
        {/if}
        {#if report.embedded_files_stripped}
          <div class="dial-line tag">Embedded files stripped</div>
        {/if}
        {#if report.js_stripped}
          <div class="dial-line tag">JavaScript stripped</div>
        {/if}
      </div>
    </div>
    {#if report.warnings.length > 0}
      <details class="warnings">
        <summary>{report.warnings.length} warning(s) — click for details</summary>
        <ul>
          {#each report.warnings as w}
            <li>{w}</li>
          {/each}
        </ul>
      </details>
    {/if}
  {/if}

  {#if error}
    <div class="error" role="alert">{error}</div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    color: var(--text-primary, #e5e5e5);
  }
  header h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--text-secondary, #b0b0b0);
    font-weight: 500;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 13px;
    color: var(--text-secondary, #b0b0b0);
    line-height: 1.5;
  }
  .why {
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 12.5px;
  }
  .why-row {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .why-row span {
    color: #5cd994;
    font-weight: 600;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row-label {
    font-size: 12px;
    color: var(--text-secondary, #b0b0b0);
    font-weight: 500;
  }
  .path-row {
    display: flex;
    gap: 8px;
  }
  .path-row input {
    flex: 1;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 8px 10px;
    color: inherit;
    font-size: 13px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .path-row button {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 8px 12px;
    color: inherit;
    cursor: pointer;
    font-size: 12.5px;
  }
  .path-row button:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .preset-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(155px, 1fr));
    gap: 8px;
  }
  .preset-card {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 10px 12px;
    color: inherit;
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
  }
  .preset-card:hover {
    background: rgba(255, 255, 255, 0.05);
    border-color: rgba(255, 255, 255, 0.18);
  }
  .preset-card.active {
    background: rgba(92, 217, 148, 0.08);
    border-color: rgba(92, 217, 148, 0.45);
  }
  .preset-label {
    font-weight: 600;
    font-size: 13.5px;
  }
  .preset-sub {
    font-size: 11.5px;
    color: var(--text-secondary, #999);
    margin-top: 2px;
  }
  .preset-spec {
    font-size: 11px;
    color: var(--text-secondary, #777);
    margin-top: 4px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .actions {
    display: flex;
    gap: 10px;
    margin-top: 4px;
  }
  .actions button {
    flex: 1;
    border-radius: 10px;
    padding: 11px 14px;
    font-size: 13.5px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
    transition: all 0.12s ease;
  }
  .secondary {
    background: rgba(255, 255, 255, 0.05);
    border-color: rgba(255, 255, 255, 0.1);
    color: inherit;
  }
  .secondary:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.09);
  }
  .primary {
    background: linear-gradient(180deg, #5cd994 0%, #3fb87a 100%);
    color: #0d2419;
    border-color: rgba(0, 0, 0, 0.2);
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.05);
  }
  .actions button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .dial-wrap {
    display: flex;
    gap: 20px;
    align-items: center;
    padding: 16px;
    background: rgba(92, 217, 148, 0.04);
    border: 1px solid rgba(92, 217, 148, 0.2);
    border-radius: 12px;
    animation: fadeIn 0.25s ease;
  }
  .dial-wrap.done {
    background: rgba(92, 217, 148, 0.08);
    border-color: rgba(92, 217, 148, 0.45);
  }
  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .dial {
    flex-shrink: 0;
    width: 110px;
    height: 110px;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, rgba(92, 217, 148, 0.35), rgba(63, 184, 122, 0.1));
    border: 2px solid rgba(92, 217, 148, 0.5);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }
  .dial-num {
    font-size: 30px;
    font-weight: 700;
    color: #b4f5cf;
    line-height: 1;
  }
  .dial-cap {
    font-size: 11px;
    color: var(--text-secondary, #9bd9b3);
    margin-top: 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .dial-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  .dial-line {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .dial-line span {
    color: var(--text-secondary, #b0b0b0);
  }
  .dial-line strong {
    font-weight: 600;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .dial-line.tag {
    font-size: 11.5px;
    color: #9bd9b3;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    padding-top: 4px;
    margin-top: 2px;
  }
  .warnings {
    background: rgba(244, 196, 88, 0.05);
    border: 1px solid rgba(244, 196, 88, 0.25);
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 12.5px;
  }
  .warnings summary {
    cursor: pointer;
    color: #f4c458;
  }
  .warnings ul {
    margin: 8px 0 0;
    padding-left: 18px;
    color: var(--text-secondary, #b0b0b0);
  }
  .error {
    background: rgba(220, 90, 90, 0.08);
    border: 1px solid rgba(220, 90, 90, 0.3);
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 13px;
    color: #ffb3b3;
  }
</style>
