<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";
  import { splitPlan, type SplitMode, type SplitRange } from "$lib/splitPlan";

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let rangeText = $state("");
  let chunkSize = $state(1);
  let partCount = $state(2);
  let mode = $state<SplitMode>("parts");
  let outDir = $state<string | null>(null);
  let status = $state<Status>(idle);
  let outputs = $state<string[]>([]);
  let loading = $state(false);
  const busy = $derived(loading || status.kind === "working");
  const plan = $derived.by((): { ranges: SplitRange[]; error: string } => {
    try {
      return { ranges: splitPlan(pageCount, mode, rangeText, chunkSize, partCount), error: "" };
    } catch (e) {
      return { ranges: [], error: e instanceof Error ? e.message : String(e) };
    }
  });
  const selectedPages = $derived(plan.ranges.reduce((n, r) => n + r.end - r.start + 1, 0));
  const stem = $derived(input ? stripExt(basename(input)) : "split");

  async function pickInput() {
    loading = true;
    try {
      const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (typeof picked !== "string") return;
      input = picked;
      pageCount = null;
      outputs = [];
      status = idle;
      const res = await invoke<CmdResult<number>>("slab_page_count", { input: picked });
      if (res.kind === "ok") {
        pageCount = res.value;
        partCount = Math.min(partCount, Math.max(1, res.value));
      } else status = { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally { loading = false; }
  }

  async function pickOutDir() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") outDir = picked;
  }

  async function runSplit() {
    if (!input || busy) return;
    if (plan.error) { status = { kind: "err", msg: plan.error }; return; }
    const ranges = plan.ranges;
    const source = input;
    outputs = [];
    status = { kind: "working", msg: `Splitting into ${ranges.length} file(s)…` };
    try {
      let dest = outDir;
      if (!dest) {
        const picked = await open({ directory: true, multiple: false });
        if (typeof picked !== "string") { status = idle; return; }
        dest = picked;
        outDir = dest;
      }
      const res = await invoke<CmdResult<string[]>>("slab_split_ranges", { input: source, ranges, outDir: dest });
      if (res.kind === "ok") {
        outputs = res.value;
        status = { kind: "ok", msg: `Wrote ${res.value.length} file(s) to ${dest}` };
      } else status = { kind: "err", msg: res.message };
    } catch (e) { status = { kind: "err", msg: String(e) }; }
  }
</script>

<header class="content-header">
  <h1>Split PDF</h1>
  <p class="subtitle">Cut a PDF into pieces by page range, every N pages, or into an exact number of balanced files.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput} disabled={busy}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick a file, then describe how to slice it.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if pageCount !== null}{pageCount} page{pageCount === 1 ? "" : "s"}{:else}…{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput} disabled={busy}>Change</button>
    </div>

    <div class="tabs">
      <button class:tab-active={mode === "parts"} onclick={() => (mode = "parts")} disabled={busy}>
        Into N files
      </button>
      <button class:tab-active={mode === "ranges"} onclick={() => (mode = "ranges")} disabled={busy}>
        By ranges
      </button>
      <button class:tab-active={mode === "every"} onclick={() => (mode = "every")} disabled={busy}>
        Every N pages
      </button>
    </div>

    {#if mode === "ranges"}
      <label class="field">
        <span class="field-label">Ranges</span>
        <input
          type="text"
          placeholder="1-3, 5, 7-9"
          bind:value={rangeText}
          disabled={busy}
        />
        <span class="field-hint">Comma-separated. Single pages or N-M ranges. Ranges must not overlap.</span>
      </label>
    {:else if mode === "every"}
      <label class="field">
        <span class="field-label">Pages per chunk</span>
        <input
          type="number"
          min="1"
          bind:value={chunkSize}
          step="1"
          disabled={busy}
        />
        <span class="field-hint">e.g. 2 → one PDF per 2-page chunk.</span>
      </label>
    {:else}
      <label class="field">
        <span class="field-label">Number of output files</span>
        <input type="number" min="1" max={pageCount ?? 1} step="1" bind:value={partCount} disabled={busy} />
        <span class="field-hint">Every page appears once. Extra pages go into the first files, so file lengths differ by at most one page.</span>
      </label>
    {/if}

    <div class="split-preview" aria-live="polite">
      <h2>Output preview</h2>
      {#if loading}
        <p>Reading page count…</p>
      {:else if plan.error}
        <p class="field-hint">{plan.error}</p>
      {:else}
        <p>{plan.ranges.length} file(s) · {selectedPages} of {pageCount} pages</p>
        {#if selectedPages < (pageCount ?? 0)}
          <p class="field-hint">{(pageCount ?? 0) - selectedPages} unselected page(s) will be omitted from the output files.</p>
        {/if}
        <ol>
          {#each plan.ranges.slice(0, 20) as range, i}
            <li><strong>{stem}-{i + 1}-{range.start}-{range.end}.pdf</strong> — pages {range.start}–{range.end} ({range.end - range.start + 1})</li>
          {/each}
        </ol>
        {#if plan.ranges.length > 20}<p>…and {plan.ranges.length - 20} more files.</p>{/if}
      {/if}
    </div>

    <label class="field">
      <span class="field-label">Output folder</span>
      <div class="row">
        <input type="text" readonly value={outDir ?? ""} placeholder="Choose a folder…" />
        <button onclick={pickOutDir} disabled={busy}>Browse</button>
      </div>
    </label>

    <div class="actions">
      <button
        class="primary"
        onclick={runSplit}
        disabled={busy || !!plan.error}
      >
        {status.kind === "working" ? "Splitting…" : "Split"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if outputs.length > 0}
    <details class="output-list" open>
      <summary>{outputs.length} file(s) written</summary>
      <ul>
        {#each outputs as o}
          <li>{basename(o)}</li>
        {/each}
      </ul>
    </details>
  {/if}
</section>

<style>
  .split-preview { margin: 1rem 0; padding: 1rem; border: 1px solid var(--border); border-radius: 8px; }
  .split-preview h2 { margin: 0 0 .5rem; font-size: 1rem; }
  .split-preview ol { padding-left: 1.5rem; overflow-wrap: anywhere; }
  .split-preview li { margin: .4rem 0; font-size: .85rem; }
</style>
