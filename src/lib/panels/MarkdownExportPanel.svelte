<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { isInTauri } from '$lib/tauri';

  type Report = {
    pages: number;
    paragraphs: number;
    headings: number;
    list_items: number;
    tables: number;
    bytes_written: number;
    duration_ms: number;
  };

  type Target = 'md' | 'html';

  let inputPath = $state('');
  let target: Target = $state('md');
  let detectTables = $state(true);
  let detectLists = $state(true);
  let flavourGfm = $state(true);
  let semanticTags = $state(true);
  let embedCss = $state(true);
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
      error = 'Markdown conversion runs in the desktop app — file system access required.';
      return;
    }
    const ext = target === 'md' ? 'md' : 'html';
    const filterName = target === 'md' ? 'Markdown' : 'HTML';
    const base = inputPath.replace(/\.pdf$/i, '');
    const suggested = `${base}.${ext}`;
    const out = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: filterName, extensions: [ext] }],
    });
    if (typeof out !== 'string') return;

    busy = true;
    error = '';
    report = null;
    outputPath = '';
    try {
      const cmd = target === 'md' ? 'slab_markdown_to_md' : 'slab_markdown_to_html';
      const args: Record<string, unknown> =
        target === 'md'
          ? {
              input: inputPath,
              output: out,
              detectTables,
              detectLists,
              flavourGfm,
            }
          : {
              input: inputPath,
              output: out,
              detectTables,
              detectLists,
              semanticTags,
              embedCss,
            };
      const res = await invoke<{ kind?: string; value?: Report; message?: string }>(cmd, args);
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
    <h2>Markdown <span class="codename">PDF → MD / HTML</span></h2>
    <p class="sub">
      Drop a PDF, get clean Markdown or semantic HTML. Perfect for Obsidian, ChatGPT,
      RAG pipelines, and static-site generators. Acrobat has no native Markdown export,
      and its HTML export produces ugly font-tag soup. Slab is offline, free, and ships
      headings, lists, and tables intact.
    </p>
  </header>

  <div class="target-switch" role="tablist" aria-label="Output format">
    <button
      role="tab"
      aria-selected={target === 'md'}
      class:active={target === 'md'}
      onclick={() => (target = 'md')}
      disabled={busy}
    >
      <span class="big">📝</span>
      <span class="lbl">Markdown <code>.md</code></span>
      <span class="hint">GFM tables, Obsidian-friendly</span>
    </button>
    <button
      role="tab"
      aria-selected={target === 'html'}
      class:active={target === 'html'}
      onclick={() => (target = 'html')}
      disabled={busy}
    >
      <span class="big">🌐</span>
      <span class="lbl">Semantic HTML <code>.html</code></span>
      <span class="hint">&lt;article&gt; / &lt;h1&gt; / &lt;table&gt; — clean</span>
    </button>
  </div>

  <div
    class="dropzone"
    class:dragging
    ondragover={onDragOver}
    ondragleave={onDragLeave}
    ondrop={onDrop}
    role="region"
    aria-label="Drop a PDF here"
  >
    <div class="dropzone-flow" aria-hidden="true">
      <span class="dz-icon">📄</span>
      <span class="dz-arrow">→</span>
      <span class="dz-icon">⚙️</span>
      <span class="dz-arrow">→</span>
      <span class="dz-icon">{target === 'md' ? '📝' : '🌐'}</span>
    </div>
    <p class="dropzone-headline">
      Drop a PDF here, get a {target === 'md' ? '.md' : '.html'} back.
    </p>
    <p class="dropzone-tagline">
      Headings, bullets, numbered lists, and tables come through. No cloud, no
      telemetry, no token limits.
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

  <details class="opts">
    <summary>Conversion options</summary>
    <div class="opts-grid">
      <label class="cb">
        <input type="checkbox" bind:checked={detectTables} disabled={busy} />
        Detect tables
      </label>
      <label class="cb">
        <input type="checkbox" bind:checked={detectLists} disabled={busy} />
        Detect bullet / numbered lists
      </label>
      {#if target === 'md'}
        <label class="cb">
          <input type="checkbox" bind:checked={flavourGfm} disabled={busy} />
          GitHub-flavoured Markdown (pipe tables)
        </label>
      {:else}
        <label class="cb">
          <input type="checkbox" bind:checked={semanticTags} disabled={busy} />
          Use semantic HTML5 tags (&lt;article&gt;)
        </label>
        <label class="cb">
          <input type="checkbox" bind:checked={embedCss} disabled={busy} />
          Embed default stylesheet
        </label>
      {/if}
    </div>
  </details>

  <div class="actions">
    <button class="primary" onclick={convert} disabled={!inputPath || busy}>
      {busy
        ? 'Converting…'
        : target === 'md'
          ? 'Convert to Markdown (.md)'
          : 'Convert to HTML (.html)'}
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
          <div class="stat-value">{report.tables}</div>
          <div class="stat-label">tables</div>
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
        {target === 'md'
          ? 'Open the .md in Obsidian, VS Code, Typora, or paste straight into ChatGPT — it’s clean CommonMark / GFM.'
          : 'Open the .html in any browser — it’s valid semantic HTML5 with optional embedded styles.'}
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
  .target-switch {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .target-switch button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 14px 16px;
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 10px;
    background: var(--surface-1, white);
    cursor: pointer;
    text-align: left;
  }
  .target-switch button.active {
    border-color: var(--accent, #2563eb);
    background: color-mix(in srgb, var(--accent, #2563eb) 6%, transparent);
  }
  .target-switch .big {
    font-size: 22px;
  }
  .target-switch .lbl {
    font-weight: 600;
    font-size: 14px;
  }
  .target-switch .hint {
    font-size: 12px;
    color: var(--text-muted, #888);
  }
  .dropzone {
    border: 2px dashed var(--border, #e5e5e5);
    border-radius: 12px;
    padding: 24px;
    text-align: center;
    background: var(--surface-2, #fafafa);
    transition:
      background 120ms ease,
      border-color 120ms ease;
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
  .opts {
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 8px;
    padding: 8px 12px;
  }
  .opts summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--text-muted, #666);
  }
  .opts-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 8px 16px;
    padding-top: 8px;
  }
  .cb {
    display: flex;
    align-items: center;
    gap: 8px;
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
