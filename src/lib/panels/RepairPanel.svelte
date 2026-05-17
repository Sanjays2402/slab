<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import {
    idle,
    basename,
    stripExt,
    formatBytes,
    type CmdResult,
    type Status,
  } from "$lib/types";

  type RepairReport = {
    objects_before: number;
    objects_after: number;
    bytes_before: number;
    bytes_after: number;
    objects_pruned: number;
  };

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);
  let report = $state<RepairReport | null>(null);

  // Pretty delta: "−12.3 KB" / "+0.4 KB" / "no change"
  let sizeDelta = $derived.by(() => {
    if (!report) return "";
    const d = report.bytes_after - report.bytes_before;
    if (d === 0) return "no change";
    const sign = d < 0 ? "−" : "+";
    return `${sign}${formatBytes(Math.abs(d))}`;
  });

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
      defaultPath: `${stripExt(basename(input))}-repaired.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Repairing…" };
    report = null;
    try {
      const res = await invoke<CmdResult<RepairReport>>("slab_repair", {
        input,
        output,
      });
      if (res.kind === "ok") {
        report = res.value;
        status = { kind: "ok", msg: `Repaired → ${basename(output)}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Repair</h1>
  <p class="subtitle">
    Rebuild a corrupt or bloated PDF — fix broken xref tables, prune orphan
    objects, recompress streams.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick a damaged or bloated PDF to repair.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          xref will be rebuilt, unreachable objects pruned
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="note">
      Repair is what people usually mean when they say "open it in Acrobat and
      Save As to fix it." Most won't-open PDFs are broken xref tables, which
      this handles. It won't fix encrypted-with-lost-key files or malformed
      content streams.
    </div>

    <div class="actions">
      <button
        class="primary"
        onclick={run}
        disabled={status.kind === "working"}
      >
        {status.kind === "working" ? status.msg : "Repair PDF"}
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
      <div class="report-title">Repair report</div>
      <div class="report-row">
        <span>Objects before</span><span>{report.objects_before}</span>
      </div>
      <div class="report-row">
        <span>Objects after</span><span>{report.objects_after}</span>
      </div>
      <div class="report-row">
        <span>Objects pruned</span><span>{report.objects_pruned}</span>
      </div>
      <div class="report-row">
        <span>Size before</span><span>{formatBytes(report.bytes_before)}</span>
      </div>
      <div class="report-row">
        <span>Size after</span><span>{formatBytes(report.bytes_after)}</span>
      </div>
      <div class="report-row delta">
        <span>Delta</span><span>{sizeDelta}</span>
      </div>
    </div>
  {/if}
</section>

<style>
  .note {
    font-size: 12px;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: var(--r-sm);
    line-height: 1.55;
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
  .report-row.delta {
    margin-top: 4px;
    padding-top: 6px;
    border-top: 1px dashed var(--border);
    font-weight: 500;
  }
</style>
