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

  type CompressReport = {
    input_bytes: number;
    output_bytes: number;
    saved_pct: number;
  };

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);
  let report = $state<CompressReport | null>(null);

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

  async function runCompress() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-compressed.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Compressing…" };
    report = null;
    try {
      const res = await invoke<CmdResult<CompressReport>>("slab_compress", {
        input,
        output,
      });
      if (res.kind === "ok") {
        report = res.value;
        const pct = res.value.saved_pct.toFixed(1);
        status = { kind: "ok", msg: `Saved ${pct}% → ${basename(output)}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Compress PDF</h1>
  <p class="subtitle">Re-stream and squeeze the file without re-rendering it.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Lossless object compression — quality is preserved.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Ready to compress</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="actions">
      <button class="primary" onclick={runCompress} disabled={status.kind === "working"}>
        {status.kind === "working" ? "Compressing…" : "Compress"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if report}
    <div class="stats">
      <div class="stat">
        <div class="stat-label">Before</div>
        <div class="stat-value">{formatBytes(report.input_bytes)}</div>
      </div>
      <div class="stat">
        <div class="stat-label">After</div>
        <div class="stat-value">{formatBytes(report.output_bytes)}</div>
      </div>
      <div class="stat highlight">
        <div class="stat-label">Saved</div>
        <div class="stat-value">{report.saved_pct.toFixed(1)}%</div>
      </div>
    </div>
  {/if}
</section>

<style>
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    margin-top: 8px;
  }
  .stat {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 14px;
    text-align: center;
  }
  .stat.highlight {
    border-color: var(--accent);
    color: var(--accent);
  }
  .stat-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    margin-bottom: 6px;
  }
  .stat.highlight .stat-label {
    color: var(--accent-2);
  }
  .stat-value {
    font-size: 18px;
    font-weight: 600;
  }
</style>
