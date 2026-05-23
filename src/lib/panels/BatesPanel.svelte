<script lang="ts">
  // v3.4.0 "Discovery" Slice 5 — buyer-magnet Bates numbering panel.
  //
  // Single-file Bates UI + batch-folder UI in one screen, with a live
  // preview SVG that re-renders on every keystroke. The preview is the
  // WOW moment: paralegals see exactly how `ACME000001` will sit in the
  // bottom-right corner before they touch a single document.
  //
  // Backend wiring (already shipped in Slices 1-4):
  //   - `slab_bates_apply` — single file, returns BatesReport
  //   - `slab_bates_batch` — ordered batch + CSV/JSON load file
  //
  // We keep the panel pure-frontend (no Rust knowledge beyond IPC).
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Position =
    | "bottom-right"
    | "bottom-center"
    | "bottom-left"
    | "top-right"
    | "top-center"
    | "top-left";

  type Mode = "single" | "batch";

  interface BatesOpts {
    prefix: string;
    start_at: number;
    digits: number;
    position: Position;
    font_size: number;
    gray: number;
  }

  interface BatesReport {
    pages_stamped: number;
    first_label: string;
    last_label: string;
    next_start: number;
  }

  interface FileReport {
    source: string;
    output: string;
    first_label: string;
    last_label: string;
    pages: number;
  }

  interface BatchReport {
    files_processed: number;
    pages_stamped: number;
    first_label: string;
    last_label: string;
    per_file: FileReport[];
    load_file_written: string | null;
  }

  // ------- shared form state -------
  let mode = $state<Mode>("single");
  let prefix = $state("ACME");
  let startAt = $state(1);
  let digits = $state(6);
  let position = $state<Position>("bottom-right");
  let fontSize = $state(10);
  let gray = $state(0); // 0 = black .. 100 = white
  let status = $state<Status>(idle);

  // ------- single-file state -------
  let input = $state<string | null>(null);

  // ------- batch state -------
  let batchInputs = $state<string[]>([]);
  let batchOutDir = $state<string | null>(null);
  let writeLoadFile = $state(true);
  let loadFormat = $state<"csv" | "json">("csv");
  let lastBatchReport = $state<BatchReport | null>(null);

  // ------- derived live-preview label -------
  const previewLabel = $derived(formatLabel(prefix, startAt, digits));

  function formatLabel(p: string, n: number, d: number): string {
    const safeDigits = Math.max(1, Math.min(12, Math.floor(d)));
    const safeN = Math.max(0, Math.floor(n));
    const body = String(safeN).padStart(safeDigits, "0");
    return `${p}${body}`;
  }

  function clampDigits() {
    if (!Number.isFinite(digits)) digits = 6;
    if (digits < 1) digits = 1;
    if (digits > 12) digits = 12;
  }

  function clampStart() {
    if (!Number.isFinite(startAt)) startAt = 1;
    if (startAt < 0) startAt = 0;
  }

  function clampFontSize() {
    if (!Number.isFinite(fontSize)) fontSize = 10;
    if (fontSize < 4) fontSize = 4;
    if (fontSize > 72) fontSize = 72;
  }

  // ------- live preview geometry (8.5x11 inch -> 612x792 pt) -------
  // We render at half scale -> 306x396 viewport, fits in a 380x52 strip if we
  // crop. Simpler: render the whole page miniature in 200x260 viewbox so the
  // user gets a real page-shape mental model. The "wow" is seeing the label
  // jump positions live as you click the segmented control.
  const PREVIEW_W_PT = 612; // US Letter width in PDF points
  const PREVIEW_H_PT = 792; // US Letter height in PDF points
  const PREVIEW_MARGIN_PT = 24;

  /** Returns { x, y, anchor } for the preview label, in PDF coords.
   *  PDF origin is bottom-left, so y grows upward. SVG origin is top-left,
   *  so the SVG conversion is y_svg = (H - y_pdf). */
  function previewXY(pos: Position): { x: number; y: number; anchor: "start" | "middle" | "end" } {
    const w = PREVIEW_W_PT;
    const h = PREVIEW_H_PT;
    const m = PREVIEW_MARGIN_PT;
    switch (pos) {
      case "top-left":
        return { x: m, y: h - m, anchor: "start" };
      case "top-center":
        return { x: w / 2, y: h - m, anchor: "middle" };
      case "top-right":
        return { x: w - m, y: h - m, anchor: "end" };
      case "bottom-left":
        return { x: m, y: m, anchor: "start" };
      case "bottom-center":
        return { x: w / 2, y: m, anchor: "middle" };
      case "bottom-right":
      default:
        return { x: w - m, y: m, anchor: "end" };
    }
  }

  const previewCoords = $derived(previewXY(position));
  const previewGray = $derived(Math.max(0, Math.min(100, gray)) / 100);
  const previewFill = $derived(grayToFill(previewGray));

  function grayToFill(g: number): string {
    const v = Math.round((1 - g) * 0 + g * 255); // g=0 -> black, g=1 -> white
    const c = 255 - v + 0; // invert so the rgb math reads naturally
    // simpler: a gray PDF value of `g` means CMYK gray g; on screen we treat
    // it as RGB(255*g, 255*g, 255*g). g=0 black, g=1 white.
    const rgb = Math.round(g * 255);
    return `rgb(${rgb}, ${rgb}, ${rgb})`;
    void c; void v;
  }

  // ------- single-file actions -------
  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
  }

  async function applySingle() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    clampDigits();
    clampStart();
    clampFontSize();

    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-bates.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    const opts: BatesOpts = {
      prefix,
      start_at: startAt,
      digits,
      position,
      font_size: fontSize,
      gray: previewGray,
    };

    status = { kind: "working", msg: "Stamping…" };
    try {
      const res = await invoke<CmdResult<BatesReport>>("slab_bates_apply", {
        input,
        output,
        opts,
      });
      if (res.kind === "ok") {
        const r = res.value;
        status = {
          kind: "ok",
          msg: `Stamped ${r.pages_stamped} pages: ${r.first_label} … ${r.last_label}. Next document starts at ${r.next_start}.`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  // ------- batch actions -------
  async function pickBatchInputs() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return;
    if (Array.isArray(picked)) {
      // Sort by basename so the Bates order is predictable; matches what a
      // paralegal would expect when they pre-named files 001_Invoice.pdf etc.
      batchInputs = picked
        .filter((p): p is string => typeof p === "string")
        .sort((a, b) => basename(a).localeCompare(basename(b)));
    } else if (typeof picked === "string") {
      batchInputs = [picked];
    }
    status = idle;
    lastBatchReport = null;
  }

  async function pickBatchOutDir() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") {
      batchOutDir = picked;
    }
  }

  function moveBatchFile(idx: number, dir: -1 | 1) {
    const next = idx + dir;
    if (next < 0 || next >= batchInputs.length) return;
    const arr = batchInputs.slice();
    [arr[idx], arr[next]] = [arr[next], arr[idx]];
    batchInputs = arr;
  }

  function removeBatchFile(idx: number) {
    batchInputs = batchInputs.slice(0, idx).concat(batchInputs.slice(idx + 1));
  }

  async function applyBatch() {
    if (batchInputs.length === 0) {
      status = { kind: "err", msg: "Pick at least one PDF for the batch." };
      return;
    }
    if (!batchOutDir) {
      status = { kind: "err", msg: "Choose an output folder for the stamped set." };
      return;
    }
    clampDigits();
    clampStart();
    clampFontSize();

    const opts: BatesOpts = {
      prefix,
      start_at: startAt,
      digits,
      position,
      font_size: fontSize,
      gray: previewGray,
    };

    const load_file = writeLoadFile
      ? {
          format: loadFormat,
          path:
            loadFormat === "csv"
              ? `${batchOutDir}/bates_load.csv`
              : `${batchOutDir}/bates_load.json`,
        }
      : null;

    status = { kind: "working", msg: `Stamping ${batchInputs.length} files…` };
    try {
      const res = await invoke<CmdResult<BatchReport>>("slab_bates_batch", {
        input: {
          inputs: batchInputs,
          output_dir: batchOutDir,
          opts,
          load_file,
        },
      });
      if (res.kind === "ok") {
        lastBatchReport = res.value;
        const tail = res.value.load_file_written
          ? ` Load file: ${basename(res.value.load_file_written)}.`
          : "";
        status = {
          kind: "ok",
          msg: `Stamped ${res.value.files_processed} files, ${res.value.pages_stamped} pages: ${res.value.first_label} … ${res.value.last_label}.${tail}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function posBtn(p: Position): string {
    return position === p ? "active" : "";
  }
</script>

<header class="content-header">
  <h1>Bates numbering</h1>
  <p class="subtitle">
    Stamp a prefix + zero-padded counter onto every page. Chain across an entire
    production set. Outputs a Relativity / Concordance / Everlaw load file.
  </p>
</header>

<section class="panel">
  <!-- Mode tabs -->
  <div class="mode-tabs">
    <button
      class={mode === "single" ? "tab active" : "tab"}
      onclick={() => {
        mode = "single";
        status = idle;
      }}
      type="button">Single file</button
    >
    <button
      class={mode === "batch" ? "tab active" : "tab"}
      onclick={() => {
        mode = "batch";
        status = idle;
      }}
      type="button">Batch (folder)</button
    >
  </div>

  <!-- Shared form: prefix / start / digits / position / size / gray -->
  <div class="form">
    <div class="row">
      <label class="field grow">
        <span class="field-label">Prefix</span>
        <input type="text" bind:value={prefix} placeholder="ACME" class="mono" maxlength="32" />
      </label>
      <label class="field">
        <span class="field-label">Start at</span>
        <input
          type="number"
          min="0"
          step="1"
          bind:value={startAt}
          onblur={clampStart}
          class="mono num"
        />
      </label>
      <label class="field">
        <span class="field-label">Digits</span>
        <input
          type="number"
          min="1"
          max="12"
          step="1"
          bind:value={digits}
          onblur={clampDigits}
          class="mono num"
        />
      </label>
    </div>

    <div class="row">
      <div class="field">
        <span class="field-label">Position</span>
        <div class="pos-grid" role="radiogroup" aria-label="Stamp position">
          <button class={posBtn("top-left")} onclick={() => (position = "top-left")} type="button"
            >↖</button
          >
          <button
            class={posBtn("top-center")}
            onclick={() => (position = "top-center")}
            type="button">↑</button
          >
          <button class={posBtn("top-right")} onclick={() => (position = "top-right")} type="button"
            >↗</button
          >
          <button
            class={posBtn("bottom-left")}
            onclick={() => (position = "bottom-left")}
            type="button">↙</button
          >
          <button
            class={posBtn("bottom-center")}
            onclick={() => (position = "bottom-center")}
            type="button">↓</button
          >
          <button
            class={posBtn("bottom-right")}
            onclick={() => (position = "bottom-right")}
            type="button">↘</button
          >
        </div>
      </div>
      <label class="field grow">
        <span class="field-label">Font size: {fontSize}pt</span>
        <input
          type="range"
          min="6"
          max="24"
          step="1"
          bind:value={fontSize}
          oninput={clampFontSize}
        />
      </label>
      <label class="field grow">
        <span class="field-label">Gray: {gray}%</span>
        <input type="range" min="0" max="100" step="5" bind:value={gray} />
      </label>
    </div>

    <!-- LIVE PREVIEW -->
    <div class="preview-wrap" aria-label="Live preview of stamped label">
      <svg
        class="preview"
        viewBox={`0 0 ${PREVIEW_W_PT} ${PREVIEW_H_PT}`}
        preserveAspectRatio="xMidYMid meet"
        role="img"
      >
        <!-- page rectangle -->
        <rect
          x="0"
          y="0"
          width={PREVIEW_W_PT}
          height={PREVIEW_H_PT}
          fill="white"
          stroke="rgba(0,0,0,0.15)"
          stroke-width="2"
        />
        <!-- faint body text lines, just for visual context -->
        {#each Array.from({ length: 22 }) as _, i}
          <rect
            x="72"
            y={108 + i * 28}
            width={i === 21 ? 380 : 468}
            height="6"
            fill="rgba(0,0,0,0.08)"
            rx="1"
          />
        {/each}
        <!-- the live label -->
        <text
          x={previewCoords.x}
          y={PREVIEW_H_PT - previewCoords.y}
          font-family="Helvetica, Arial, sans-serif"
          font-size={fontSize * 2}
          text-anchor={previewCoords.anchor}
          fill={previewFill}
          dominant-baseline="middle"
        >
          {previewLabel}
        </text>
      </svg>
      <div class="preview-caption">
        Preview · <span class="mono">{previewLabel}</span> at
        <span class="mono">{position}</span>
      </div>
    </div>
  </div>

  <!-- Mode-specific bottom block -->
  {#if mode === "single"}
    <div class="single-block">
      {#if !input}
        <button class="dropzone" onclick={pickInput} type="button">
          <span class="dz-icon">+</span>
          <span class="dz-title">Choose a PDF</span>
          <span class="dz-hint">We'll stamp without re-rasterizing.</span>
        </button>
      {:else}
        <div class="file-card">
          <div>
            <div class="file-name">{basename(input)}</div>
            <div class="file-meta">Ready · first label will be {previewLabel}</div>
          </div>
          <button class="ghost" onclick={pickInput} type="button">Change</button>
        </div>
        <div class="actions">
          <button
            class="primary"
            onclick={applySingle}
            disabled={status.kind === "working"}
            type="button"
          >
            {status.kind === "working" ? status.msg : "Apply Bates numbering"}
          </button>
        </div>
      {/if}
    </div>
  {:else}
    <div class="batch-block">
      <div class="batch-toolbar">
        <button class="ghost" onclick={pickBatchInputs} type="button">
          {batchInputs.length === 0 ? "Choose PDFs…" : "Add / replace PDFs…"}
        </button>
        <button class="ghost" onclick={pickBatchOutDir} type="button">
          {batchOutDir ? `Output: ${basename(batchOutDir)}` : "Output folder…"}
        </button>
        <label class="inline-check">
          <input type="checkbox" bind:checked={writeLoadFile} />
          <span>Write load file</span>
        </label>
        {#if writeLoadFile}
          <div class="load-format">
            <button
              class={loadFormat === "csv" ? "active" : ""}
              onclick={() => (loadFormat = "csv")}
              type="button">CSV</button
            >
            <button
              class={loadFormat === "json" ? "active" : ""}
              onclick={() => (loadFormat = "json")}
              type="button">JSON</button
            >
          </div>
        {/if}
      </div>

      {#if batchInputs.length > 0}
        <ul class="file-list">
          {#each batchInputs as p, i (p + i)}
            <li>
              <span class="ix">{i + 1}.</span>
              <span class="fn" title={p}>{basename(p)}</span>
              <span class="spacer"></span>
              <button class="tiny" onclick={() => moveBatchFile(i, -1)} disabled={i === 0} type="button"
                >▲</button
              >
              <button
                class="tiny"
                onclick={() => moveBatchFile(i, 1)}
                disabled={i === batchInputs.length - 1}
                type="button">▼</button
              >
              <button class="tiny danger" onclick={() => removeBatchFile(i)} type="button">✕</button>
            </li>
          {/each}
        </ul>
        <p class="hint">
          Order determines Bates order. First file gets <span class="mono">{previewLabel}</span>;
          counter chains across every page of every file.
        </p>
      {:else}
        <p class="hint dim">Pick the PDFs you want stamped in production order.</p>
      {/if}

      <div class="actions">
        <button
          class="primary"
          onclick={applyBatch}
          disabled={status.kind === "working" || batchInputs.length === 0 || !batchOutDir}
          type="button"
        >
          {status.kind === "working"
            ? status.msg
            : `Stamp ${batchInputs.length} file${batchInputs.length === 1 ? "" : "s"}`}
        </button>
      </div>

      {#if lastBatchReport}
        <div class="batch-report">
          <div class="report-head">
            <strong>{lastBatchReport.files_processed}</strong> files ·
            <strong>{lastBatchReport.pages_stamped}</strong> pages ·
            <span class="mono">{lastBatchReport.first_label}</span> …
            <span class="mono">{lastBatchReport.last_label}</span>
          </div>
          {#if lastBatchReport.load_file_written}
            <div class="report-load">
              Load file written: <span class="mono">{lastBatchReport.load_file_written}</span>
            </div>
          {/if}
          <details>
            <summary>Per-file breakdown</summary>
            <table>
              <thead>
                <tr>
                  <th>File</th>
                  <th>Pages</th>
                  <th>First</th>
                  <th>Last</th>
                </tr>
              </thead>
              <tbody>
                {#each lastBatchReport.per_file as f}
                  <tr>
                    <td title={f.source}>{basename(f.source)}</td>
                    <td class="num">{f.pages}</td>
                    <td class="mono">{f.first_label}</td>
                    <td class="mono">{f.last_label}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </details>
        </div>
      {/if}
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

<style>
  .mode-tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 16px;
  }
  .mode-tabs .tab {
    background: transparent;
    border: none;
    padding: 8px 14px;
    color: var(--text-2);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .mode-tabs .tab:hover {
    color: var(--text-1);
  }
  .mode-tabs .tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
    min-width: 140px;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  .num {
    width: 88px;
  }

  .pos-grid {
    display: grid;
    grid-template-columns: repeat(3, 32px);
    grid-template-rows: repeat(2, 32px);
    gap: 4px;
  }
  .pos-grid button {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    font-size: 14px;
    cursor: pointer;
  }
  .pos-grid button:hover {
    background: var(--bg-2);
  }
  .pos-grid button.active {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }

  .preview-wrap {
    margin-top: 4px;
    background: linear-gradient(180deg, var(--bg-2), var(--bg-1));
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .preview {
    width: 220px;
    height: 285px;
    filter: drop-shadow(0 4px 16px rgba(0, 0, 0, 0.18));
    transition: filter 0.15s ease;
  }
  .preview-caption {
    font-size: 11px;
    color: var(--text-3);
  }

  .single-block,
  .batch-block {
    margin-top: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .batch-toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .inline-check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--text-2);
  }
  .load-format {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    overflow: hidden;
  }
  .load-format button {
    background: var(--bg-1);
    color: var(--text-2);
    border: none;
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .load-format button.active {
    background: var(--accent);
    color: white;
  }

  .file-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    max-height: 240px;
    overflow-y: auto;
    background: var(--bg-1);
  }
  .file-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }
  .file-list li:last-child {
    border-bottom: none;
  }
  .file-list .ix {
    color: var(--text-3);
    width: 28px;
    font-family: ui-monospace, monospace;
    font-size: 11px;
  }
  .file-list .fn {
    flex: 1;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file-list .spacer {
    flex: 0 0 8px;
  }
  .tiny {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-2);
    border-radius: var(--r-sm);
    width: 24px;
    height: 24px;
    font-size: 11px;
    cursor: pointer;
    padding: 0;
  }
  .tiny:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
  .tiny.danger:hover {
    background: rgba(220, 53, 69, 0.12);
    color: #dc3545;
    border-color: rgba(220, 53, 69, 0.4);
  }

  .batch-report {
    margin-top: 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 12px 14px;
    background: var(--bg-2);
    font-size: 13px;
  }
  .report-head {
    color: var(--text-1);
  }
  .report-load {
    margin-top: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .batch-report details {
    margin-top: 8px;
  }
  .batch-report summary {
    cursor: pointer;
    color: var(--text-2);
    font-size: 12px;
  }
  .batch-report table {
    width: 100%;
    margin-top: 8px;
    border-collapse: collapse;
    font-size: 12px;
  }
  .batch-report th,
  .batch-report td {
    text-align: left;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
  }
  .batch-report th {
    color: var(--text-3);
    font-weight: 500;
  }
  .batch-report td.num {
    text-align: right;
    font-family: ui-monospace, monospace;
  }

  .hint {
    font-size: 11px;
    color: var(--text-3);
    margin: 0;
  }
  .hint.dim {
    opacity: 0.7;
  }

  @media (max-width: 720px) {
    .row {
      flex-direction: column;
      align-items: stretch;
    }
    .num {
      width: auto;
    }
  }
</style>
