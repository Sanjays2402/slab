<script lang="ts">
  // v3.11.0 "Signet Pro" — Batch Sign panel.
  //
  // Drop a folder of PDFs in, get signed PDFs out — in parallel, offline.
  // Acrobat Pro charges $239/yr for this exact workflow and only on Windows.
  // Slab does it free, on every desktop OS, with rayon parallelism and a
  // live progress event stream.

  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type Status } from "$lib/types";
  import { onMount, onDestroy } from "svelte";

  interface BatchEntry {
    input: string;
    output: string;
    ok: boolean;
    error: string | null;
    elapsed_ms: number;
  }
  interface BatchReport {
    total: number;
    succeeded: number;
    failed: number;
    elapsed_ms: number;
    success_rate: number;
    entries: BatchEntry[];
  }
  interface BatchProgress {
    done: number;
    total: number;
    fraction: number;
  }

  // ─── Identity inputs (same shape as SignetPanel — repeated here so users
  //     can run batch sign without leaving the panel). ────────────────────
  let certPath = $state<string | null>(null);
  let keyPath = $state<string | null>(null);
  let keyPassword = $state("");

  // ─── Batch inputs ──────────────────────────────────────────────
  let inputDir = $state<string | null>(null);
  let outputDir = $state<string | null>(null);
  let recursive = $state(false);
  let skipExisting = $state(false);
  let naming = $state<"suffix" | "mirror">("suffix");
  let reason = $state("");
  let location = $state("");
  let tsaUrl = $state("");

  // ─── Progress + result state ───────────────────────────────────
  let planning = $state(false);
  let plannedJobs = $state<BatchEntry[]>([]);
  let progress = $state<BatchProgress>({ done: 0, total: 0, fraction: 0 });
  let running = $state(false);
  let report = $state<BatchReport | null>(null);
  let status = $state<Status>(idle);

  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    unlisten = await listen<BatchProgress>(
      "signet-pro/batch-progress",
      (e) => {
        progress = e.payload;
      },
    );
  });
  onDestroy(() => {
    if (unlisten) unlisten();
  });

  // ─── File pickers ──────────────────────────────────────────────
  async function pickCert() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "PEM Certificate", extensions: ["pem", "crt", "cer"] }],
    });
    if (typeof sel === "string") certPath = sel;
  }
  async function pickKey() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "PEM Key", extensions: ["pem", "key"] }],
    });
    if (typeof sel === "string") keyPath = sel;
  }
  async function pickInputDir() {
    const sel = await open({ multiple: false, directory: true });
    if (typeof sel === "string") inputDir = sel;
  }
  async function pickOutputDir() {
    const sel = await open({ multiple: false, directory: true });
    if (typeof sel === "string") outputDir = sel;
  }

  // ─── Plan (dry run) ────────────────────────────────────────────
  async function planNow() {
    if (!inputDir || !outputDir) return;
    planning = true;
    status = { kind: "working", msg: "Walking folder…" };
    try {
      const r = await invoke<{ jobs: BatchEntry[] }>("signet_pro_batch_plan", {
        inputDir,
        outputDir,
        recursive,
        naming,
        skipExisting,
      });
      plannedJobs = r.jobs;
      status =
        r.jobs.length === 0
          ? { kind: "err", msg: "No PDFs found in input folder." }
          : { kind: "ok", msg: `${r.jobs.length} PDF${r.jobs.length === 1 ? "" : "s"} planned.` };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      planning = false;
    }
  }

  // ─── Sign all ──────────────────────────────────────────────────
  async function signAll() {
    if (!inputDir || !outputDir || !certPath || !keyPath) return;
    running = true;
    report = null;
    progress = { done: 0, total: plannedJobs.length || 0, fraction: 0 };
    status = { kind: "working", msg: "Signing…" };
    try {
      const r = await invoke<BatchReport>("signet_pro_batch_sign", {
        args: {
          input_dir: inputDir,
          output_dir: outputDir,
          cert_pem_path: certPath,
          key_pem_path: keyPath,
          key_password: keyPassword || null,
          recursive,
          naming,
          skip_existing: skipExisting,
          reason: reason || null,
          location: location || null,
          tsa_url: tsaUrl || null,
        },
      });
      report = r;
      progress = { done: r.total, total: r.total, fraction: 1 };
      status =
        r.failed === 0
          ? {
              kind: "ok",
              msg: `Signed ${r.succeeded}/${r.total} in ${(r.elapsed_ms / 1000).toFixed(1)}s.`,
            }
          : {
              kind: "err",
              msg: `${r.succeeded} signed, ${r.failed} failed in ${(r.elapsed_ms / 1000).toFixed(1)}s.`,
            };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      running = false;
    }
  }

  const canPlan = $derived(!!inputDir && !!outputDir && !running && !planning);
  const canSign = $derived(
    !!inputDir && !!outputDir && !!certPath && !!keyPath && !running && !planning,
  );
  const pct = $derived(Math.round(progress.fraction * 100));
</script>

<section class="batch">
  <header>
    <h2>Signet Pro — Batch Sign</h2>
    <p class="hint">
      Drop a folder of PDFs, get signed copies out. Runs in parallel across all
      CPU cores, 100% offline. Acrobat Pro charges $239/yr for this.
    </p>
  </header>

  <!-- ─── Identity ─── -->
  <fieldset>
    <legend>1. Signing identity</legend>
    <div class="row">
      <label for="cert-btn">Certificate (PEM)</label>
      <button id="cert-btn" onclick={pickCert}>
        {certPath ? basename(certPath) : "Choose…"}
      </button>
    </div>
    <div class="row">
      <label for="key-btn">Private key (PEM)</label>
      <button id="key-btn" onclick={pickKey}>
        {keyPath ? basename(keyPath) : "Choose…"}
      </button>
    </div>
    <div class="row">
      <label for="key-pw">Key password</label>
      <input id="key-pw" type="password" bind:value={keyPassword} placeholder="optional" />
    </div>
  </fieldset>

  <!-- ─── Folders ─── -->
  <fieldset>
    <legend>2. Input &amp; output folders</legend>
    <div class="row">
      <label for="in-btn">Input folder</label>
      <button id="in-btn" onclick={pickInputDir}>
        {inputDir ? basename(inputDir) : "Choose…"}
      </button>
    </div>
    <div class="row">
      <label for="out-btn">Output folder</label>
      <button id="out-btn" onclick={pickOutputDir}>
        {outputDir ? basename(outputDir) : "Choose…"}
      </button>
    </div>
    <div class="row toggles">
      <label><input type="checkbox" bind:checked={recursive} /> Recurse into subfolders</label>
      <label><input type="checkbox" bind:checked={skipExisting} /> Skip files already signed</label>
    </div>
    <div class="row">
      <label for="naming-sel">Output naming</label>
      <select id="naming-sel" bind:value={naming}>
        <option value="suffix">report.pdf → report-signed.pdf</option>
        <option value="mirror">report.pdf → report.pdf (mirror)</option>
      </select>
    </div>
    <div class="row">
      <label for="reason-in">Reason</label>
      <input id="reason-in" type="text" bind:value={reason} placeholder="e.g. Approved" />
    </div>
    <div class="row">
      <label for="loc-in">Location</label>
      <input id="loc-in" type="text" bind:value={location} placeholder="e.g. Seattle, WA" />
    </div>
    <div class="row">
      <label for="tsa-in">TSA URL</label>
      <input
        id="tsa-in"
        type="text"
        bind:value={tsaUrl}
        placeholder="optional — RFC 3161 (CAdES-T)"
      />
    </div>
  </fieldset>

  <!-- ─── Actions ─── -->
  <div class="actions">
    <button disabled={!canPlan} onclick={planNow}>Preview plan</button>
    <button class="primary" disabled={!canSign} onclick={signAll}>
      {running ? "Signing…" : "Sign all"}
    </button>
    {#if status.kind !== "idle"}
      <span class="status {status.kind}">{status.msg}</span>
    {/if}
  </div>

  <!-- ─── Progress ─── -->
  {#if running || (progress.total > 0 && !report)}
    <div class="progress" aria-label="Batch sign progress">
      <div class="bar"><div class="fill" style="width: {pct}%"></div></div>
      <div class="counter">{progress.done} / {progress.total} ({pct}%)</div>
    </div>
  {/if}

  <!-- ─── Plan preview ─── -->
  {#if plannedJobs.length > 0 && !report}
    <details open>
      <summary>Planned ({plannedJobs.length})</summary>
      <ol class="job-list">
        {#each plannedJobs as job (job.input)}
          <li><code>{basename(job.input)}</code> → <code>{basename(job.output)}</code></li>
        {/each}
      </ol>
    </details>
  {/if}

  <!-- ─── Results ─── -->
  {#if report}
    <fieldset class="report">
      <legend>Results</legend>
      <div class="summary">
        <span class="pill ok">{report.succeeded} signed</span>
        {#if report.failed > 0}
          <span class="pill warn">{report.failed} failed</span>
        {/if}
        <span class="pill muted">{(report.elapsed_ms / 1000).toFixed(2)}s total</span>
        <span class="pill muted">{Math.round(report.success_rate * 100)}% success</span>
      </div>
      <table class="results">
        <thead>
          <tr><th>File</th><th>Output</th><th>Time</th><th>Status</th></tr>
        </thead>
        <tbody>
          {#each report.entries as entry (entry.input)}
            <tr class:fail={!entry.ok}>
              <td><code>{basename(entry.input)}</code></td>
              <td><code>{basename(entry.output)}</code></td>
              <td>{entry.elapsed_ms}ms</td>
              <td>
                {#if entry.ok}
                  <span class="badge ok">✓</span>
                {:else}
                  <span class="badge warn" title={entry.error ?? "error"}>✗ {entry.error}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </fieldset>
  {/if}
</section>

<style>
  .batch {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    max-width: 900px;
    color: var(--fg, #1a1a1a);
  }
  header h2 {
    margin: 0 0 4px 0;
    font-size: 1.25rem;
  }
  .hint {
    margin: 0;
    color: var(--muted-fg, #666);
    font-size: 0.9rem;
  }
  fieldset {
    border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    border-radius: 10px;
    padding: 12px 14px;
    background: var(--surface, rgba(255, 255, 255, 0.55));
    backdrop-filter: blur(12px);
  }
  legend {
    padding: 0 6px;
    font-weight: 600;
    font-size: 0.9rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 6px 0;
  }
  .row label {
    width: 130px;
    font-size: 0.875rem;
    color: var(--muted-fg, #555);
  }
  .row.toggles {
    flex-wrap: wrap;
    gap: 14px;
  }
  .row.toggles label {
    width: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    color: inherit;
  }
  input[type="text"],
  input[type="password"],
  select {
    flex: 1;
    padding: 6px 10px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
    border-radius: 6px;
    background: var(--bg, white);
    color: inherit;
    font-size: 0.9rem;
  }
  button {
    padding: 6px 14px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
    border-radius: 6px;
    background: var(--bg, white);
    color: inherit;
    cursor: pointer;
    font-size: 0.9rem;
    transition: background 0.12s ease;
  }
  button:hover:not(:disabled) {
    background: var(--hover, rgba(0, 0, 0, 0.04));
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  button.primary {
    background: #2563eb;
    color: white;
    border-color: #1d4ed8;
  }
  button.primary:hover:not(:disabled) {
    background: #1d4ed8;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .status {
    font-size: 0.875rem;
  }
  .status.working {
    color: #2563eb;
  }
  .status.ok {
    color: #10b981;
  }
  .status.err {
    color: #d97706;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .bar {
    flex: 1;
    height: 8px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.08);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, #2563eb, #06b6d4);
    transition: width 120ms ease-out;
  }
  .counter {
    font-variant-numeric: tabular-nums;
    font-size: 0.875rem;
    min-width: 110px;
    text-align: right;
    color: var(--muted-fg, #555);
  }
  .job-list {
    margin: 8px 0 0 0;
    padding-left: 20px;
    font-size: 0.85rem;
    max-height: 200px;
    overflow-y: auto;
  }
  .job-list code {
    font-size: 0.8rem;
    background: rgba(0, 0, 0, 0.05);
    padding: 1px 4px;
    border-radius: 3px;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 6px 0 10px 0;
  }
  .pill {
    padding: 2px 10px;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 600;
  }
  .pill.ok {
    background: rgba(16, 185, 129, 0.15);
    color: #047857;
  }
  .pill.warn {
    background: rgba(217, 119, 6, 0.15);
    color: #92400e;
  }
  .pill.muted {
    background: rgba(0, 0, 0, 0.06);
    color: var(--muted-fg, #555);
  }
  table.results {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
  }
  table.results th,
  table.results td {
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.06));
  }
  table.results tr.fail td {
    background: rgba(217, 119, 6, 0.06);
  }
  .badge {
    display: inline-block;
    padding: 1px 8px;
    border-radius: 4px;
    font-size: 0.8rem;
    font-weight: 600;
  }
  .badge.ok {
    background: rgba(16, 185, 129, 0.15);
    color: #047857;
  }
  .badge.warn {
    background: rgba(217, 119, 6, 0.15);
    color: #92400e;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
</style>
