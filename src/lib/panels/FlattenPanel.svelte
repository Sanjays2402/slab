<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type FlattenMode =
    | { kind: "annotations" }
    | { kind: "raster"; dpi: number };
  type FlattenOpts = { include_widgets: boolean; mode: FlattenMode };
  type FlattenReport = {
    annotations_in: number;
    annotations_flattened: number;
    annotations_dropped: number;
    pages_with_annotations: number;
    had_acroform: boolean;
    pages_rasterized: number;
    dpi: number;
  };

  let input = $state<string | null>(null);
  let includeWidgets = $state(true);
  let modeKind = $state<"annotations" | "raster">("annotations");
  let dpi = $state<150 | 300>(150);
  let status = $state<Status>(idle);
  let report = $state<FlattenReport | null>(null);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
    report = null;
  }

  async function run() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    const suffix = modeKind === "raster" ? "-flat-raster" : "-flat";
    const output = await save({
      defaultPath: `${stripExt(basename(input))}${suffix}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    const mode: FlattenMode =
      modeKind === "raster"
        ? { kind: "raster", dpi }
        : { kind: "annotations" };

    status = {
      kind: "working",
      msg:
        modeKind === "raster"
          ? `Rasterizing pages @ ${dpi} DPI…`
          : "Flattening…",
    };
    report = null;
    try {
      const res = await invoke<CmdResult<FlattenReport>>("slab_flatten", {
        input,
        output,
        opts: { include_widgets: includeWidgets, mode },
      });
      if (res.kind === "ok") {
        report = res.value;
        status = { kind: "ok", msg: `Flattened → ${basename(output)}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Flatten</h1>
  <p class="subtitle">
    Bake annotations and form fields into the page so they can't be edited or
    moved. Strips <code>/AcroForm</code>. Optional full-raster mode produces a
    court-admissible PDF with zero editable text.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick the file you want to flatten.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Annotations will be baked into pages</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="mode-picker">
      <label class="radio" class:selected={modeKind === "annotations"}>
        <input type="radio" name="mode" value="annotations" bind:group={modeKind} />
        <div>
          <div class="radio-title">Burn annotations only</div>
          <div class="radio-sub">Fast · text stays searchable · default</div>
        </div>
      </label>
      <label class="radio" class:selected={modeKind === "raster"}>
        <input type="radio" name="mode" value="raster" bind:group={modeKind} />
        <div>
          <div class="radio-title">Full raster (legal-grade)</div>
          <div class="radio-sub">
            Every page becomes an image · zero editable text · court-admissible
          </div>
        </div>
      </label>

      {#if modeKind === "raster"}
        <div class="dpi-row">
          <span class="dpi-label">DPI</span>
          <label class="dpi-opt">
            <input type="radio" name="dpi" value={150} bind:group={dpi} />
            <span>150 <em>(recommended)</em></span>
          </label>
          <label class="dpi-opt">
            <input type="radio" name="dpi" value={300} bind:group={dpi} />
            <span>300 <em>(archival)</em></span>
          </label>
        </div>
        <div class="warn">
          ⚠ Raster mode is <strong>irreversible</strong>. Every page becomes a
          pixel image — searchability, selection, and copy-paste are lost. Save
          as a new file. Use for legal discovery, FDA submissions, ISO archives.
        </div>
      {/if}
    </div>

    <label class="checkbox">
      <input type="checkbox" bind:checked={includeWidgets} />
      <span>Include form widgets (recommended)</span>
    </label>

    <div class="actions">
      <button
        class="primary"
        onclick={run}
        disabled={status.kind === "working"}
      >
        {status.kind === "working"
          ? status.msg
          : modeKind === "raster"
            ? `Flatten as raster @ ${dpi} DPI`
            : "Flatten PDF"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if report}
    <div class="report">
      <div class="report-title">Flatten report</div>
      <div class="report-row">
        <span>Annotations input</span><span>{report.annotations_in}</span>
      </div>
      <div class="report-row">
        <span>Flattened into pages</span><span>{report.annotations_flattened}</span>
      </div>
      {#if report.annotations_dropped > 0}
        <div class="report-row">
          <span>Dropped (no /AP)</span><span>{report.annotations_dropped}</span>
        </div>
      {/if}
      <div class="report-row">
        <span>Pages with annotations</span><span>{report.pages_with_annotations}</span>
      </div>
      <div class="report-row">
        <span>AcroForm removed</span><span>{report.had_acroform ? "yes" : "no"}</span>
      </div>
      {#if report.pages_rasterized > 0}
        <div class="report-row hi">
          <span>Pages rasterized</span><span>{report.pages_rasterized} @ {report.dpi} DPI</span>
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .checkbox {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-2);
    margin-top: 10px;
  }
  .mode-picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 14px;
  }
  .radio {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }
  .radio:hover {
    border-color: var(--accent);
    background: var(--bg-2);
  }
  .radio.selected {
    border-color: var(--accent);
    background: var(--bg-2);
  }
  .radio input {
    margin-top: 3px;
  }
  .radio-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .radio-sub {
    font-size: 11px;
    color: var(--text-2);
    margin-top: 2px;
  }
  .dpi-row {
    display: flex;
    align-items: center;
    gap: 14px;
    font-size: 12px;
    padding-left: 28px;
    color: var(--text-2);
    margin-top: 2px;
  }
  .dpi-label {
    font-weight: 600;
    color: var(--text);
    letter-spacing: 0.4px;
    font-size: 11px;
    text-transform: uppercase;
  }
  .dpi-opt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .dpi-opt em {
    font-style: normal;
    color: var(--text-3, var(--text-2));
    font-size: 11px;
  }
  .warn {
    font-size: 12px;
    line-height: 1.5;
    color: #8a5a00;
    background: rgba(192, 138, 0, 0.08);
    border: 1px solid rgba(192, 138, 0, 0.35);
    border-radius: var(--r-sm);
    padding: 10px 12px;
    margin-top: 4px;
  }
  .warn strong {
    color: #6b4500;
  }
  .report {
    margin-top: 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 10px 14px;
    font-size: 12px;
  }
  .report-title {
    font-weight: 600;
    color: var(--text);
    margin-bottom: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .report-row {
    display: flex;
    justify-content: space-between;
    color: var(--text-2);
    padding: 2px 0;
  }
  .report-row.hi span:last-child {
    color: var(--accent);
    font-weight: 600;
  }
  .report-row span:last-child {
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  code {
    background: var(--bg-2);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 11px;
  }
</style>
