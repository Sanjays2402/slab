<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type SanitizeOpts = { keep_links: boolean };
  type SanitizeReport = {
    js_removed: number;
    embedded_files_removed: number;
    launch_removed: number;
    uri_removed: number;
    open_action_removed: boolean;
    catalog_aa_removed: boolean;
    xfa_removed: boolean;
    pages_aa_removed: number;
  };

  // Human-readable labels for the report grid. Keys MUST match SanitizeReport.
  const LABELS: Record<keyof SanitizeReport, string> = {
    js_removed: "JavaScript actions",
    embedded_files_removed: "Embedded files",
    launch_removed: "Launch actions",
    uri_removed: "URI actions",
    open_action_removed: "OpenAction",
    catalog_aa_removed: "Catalog /AA",
    xfa_removed: "XFA forms",
    pages_aa_removed: "Page /AA actions",
  };

  let input = $state<string | null>(null);
  let keepLinks = $state(false);
  let status = $state<Status>(idle);
  let report = $state<SanitizeReport | null>(null);

  // Pre-computed list of non-zero/true rows for the report grid.
  let nonEmptyRows = $derived(
    report
      ? (Object.entries(report) as [keyof SanitizeReport, number | boolean][])
          .filter(([, v]) => (typeof v === "boolean" ? v : v > 0))
          .map(([k, v]) => ({
            label: LABELS[k],
            value: typeof v === "boolean" ? "yes" : String(v),
          }))
      : [],
  );

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
      defaultPath: `${stripExt(basename(input))}-clean.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Sanitizing…" };
    report = null;
    try {
      const res = await invoke<CmdResult<SanitizeReport>>("slab_sanitize", {
        input,
        output,
        opts: { keep_links: keepLinks },
      });
      if (res.kind === "ok") {
        report = res.value;
        status = { kind: "ok", msg: `Sanitized → ${basename(output)}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Sanitize</h1>
  <p class="subtitle">
    Strip JavaScript, embedded files, launch actions, XFA forms, and other
    active content from a PDF.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick the file you want to clean.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Active content will be stripped</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <label class="checkbox">
      <input type="checkbox" bind:checked={keepLinks} />
      <span>Keep external links (/URI actions)</span>
    </label>

    <div class="note">
      Default behavior is paranoid: <strong>JavaScript</strong>,
      <strong>embedded files</strong>, <strong>Launch</strong> actions, XFA
      forms, additional-actions trees, and external URLs are all removed.
      Toggle the option above to keep <code>/URI</code> links if you trust them.
    </div>

    <div class="actions">
      <button
        class="primary"
        onclick={run}
        disabled={status.kind === "working"}
      >
        {status.kind === "working" ? status.msg : "Sanitize PDF"}
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
      <div class="report-title">What was removed</div>
      {#if nonEmptyRows.length === 0}
        <div class="report-empty">
          File was already clean — nothing to strip.
        </div>
      {:else}
        {#each nonEmptyRows as row}
          <div class="report-row">
            <span>{row.label}</span><span>{row.value}</span>
          </div>
        {/each}
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
    margin-top: 6px;
  }
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
  .note code {
    background: var(--bg);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 11px;
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
  .report-empty {
    color: var(--text-3);
    font-style: italic;
  }
</style>
