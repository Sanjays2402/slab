<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type FlattenOpts = { include_widgets: boolean };
  type FlattenReport = {
    annotations_in: number;
    annotations_flattened: number;
    annotations_dropped: number;
    pages_with_annotations: number;
    had_acroform: boolean;
  };

  let input = $state<string | null>(null);
  let includeWidgets = $state(true);
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
    const output = await save({
      defaultPath: `${stripExt(basename(input))}-flat.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Flattening…" };
    report = null;
    try {
      const res = await invoke<CmdResult<FlattenReport>>("slab_flatten", {
        input,
        output,
        opts: { include_widgets: includeWidgets },
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
    moved. Strips <code>/AcroForm</code>.
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
        {status.kind === "working" ? status.msg : "Flatten PDF"}
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
    margin-top: 6px;
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
