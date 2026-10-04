<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SavedFiles from "$lib/components/SavedFiles.svelte";
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
    if (busy) return;
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
    if (busy) return;
    loading = true;
    try {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked === "string") outDir = picked;
    } catch (e) { status = { kind: "err", msg: String(e) }; }
    finally { loading = false; }
  }

  async function runSplit() {
    if (!input || busy) return;
    if (plan.error) { status = { kind: "err", msg: plan.error }; return; }
    const ranges = plan.ranges;
    const source = input;
    outputs = [];
    status = { kind: "working", msg: `Splitting into ${ranges.length} PDF${ranges.length === 1 ? "" : "s"}…` };
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

<section class="panel" aria-busy={busy}>
  {#if !input}
    <button class="dropzone" onclick={pickInput} disabled={busy}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick a file, then describe how to slice it.</span>
    </button>
  {:else}
    <div class="file-card">
      <div class="source-info">
        <div class="file-name" title={input}>{basename(input)}</div>
        <div class="file-meta">
          {#if pageCount !== null}{pageCount} page{pageCount === 1 ? "" : "s"}{:else}…{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput} disabled={busy}>Change</button>
    </div>

    <div class="tabs" role="group" aria-label="Split method">
      <button aria-pressed={mode === "parts"} class:tab-active={mode === "parts"} onclick={() => (mode = "parts")} disabled={busy}>
        Into N files
      </button>
      <button aria-pressed={mode === "ranges"} class:tab-active={mode === "ranges"} onclick={() => (mode = "ranges")} disabled={busy}>
        By ranges
      </button>
      <button aria-pressed={mode === "every"} class:tab-active={mode === "every"} onclick={() => (mode = "every")} disabled={busy}>
        Every N pages
      </button>
    </div>

    {#if mode === "ranges"}
      <div class="field">
        <label class="field-label" for="split-ranges">Ranges</label>
        <input
          id="split-ranges" aria-describedby={`split-ranges-hint${plan.error ? " split-validation" : ""}`} aria-invalid={!!plan.error}
          type="text"
          placeholder="1-3, 5, 7-9"
          bind:value={rangeText}
          disabled={busy}
        />
        <span class="field-hint" id="split-ranges-hint">Comma-separated. Single pages or N-M ranges. Ranges must not overlap.</span>
      </div>
    {:else if mode === "every"}
      <div class="field">
        <label class="field-label" for="split-chunk">Pages per chunk</label>
        <input
          id="split-chunk" aria-describedby={`split-chunk-hint${plan.error ? " split-validation" : ""}`} aria-invalid={!!plan.error}
          type="number"
          min="1"
          bind:value={chunkSize}
          step="1"
          disabled={busy}
        />
        <span class="field-hint" id="split-chunk-hint">e.g. 2 → one PDF per 2-page chunk.</span>
      </div>
    {:else}
      <div class="field">
        <label class="field-label" for="split-parts">Number of output files</label>
        <input id="split-parts" aria-describedby={`split-parts-hint${plan.error ? " split-validation" : ""}`} aria-invalid={!!plan.error} type="number" min="1" max={pageCount ?? 1} step="1" bind:value={partCount} disabled={busy} />
        <span class="field-hint" id="split-parts-hint">Every page appears once. Extra pages go into the first files, so file lengths differ by at most one page.</span>
      </div>
    {/if}

    <div class="split-preview">
      <h2>Output preview</h2>
      {#if loading && pageCount === null}
        <p role="status">Reading page count…</p>
      {:else if plan.error}
        <p id="split-validation" class="validation-error" role="status">{plan.error}</p>
      {:else}
        <p class="preview-summary" role="status">{plan.ranges.length} PDF{plan.ranges.length === 1 ? "" : "s"} <span>· {selectedPages} of {pageCount} pages</span></p>
        {#if selectedPages < (pageCount ?? 0)}
          <p class="field-hint">{(pageCount ?? 0) - selectedPages} unselected page{(pageCount ?? 0) - selectedPages === 1 ? "" : "s"} will be omitted.</p>
        {/if}
        <ol>
          {#each plan.ranges.slice(0, 20) as range, i}
            <li>
              <strong>{stem}-{i + 1}-{range.start}-{range.end}.pdf</strong>
              <span>{range.start === range.end ? `Page ${range.start}` : `Pages ${range.start}–${range.end}`} · {range.end - range.start + 1} page{range.start === range.end ? "" : "s"}</span>
            </li>
          {/each}
        </ol>
        {#if plan.ranges.length > 20}<p>…and {plan.ranges.length - 20} more files.</p>{/if}
      {/if}
    </div>

    <div class="field">
      <label class="field-label" for="split-folder">Output folder</label>
      <div class="row">
        <input id="split-folder" title={outDir ?? ""} type="text" readonly value={outDir ?? ""} placeholder="Choose a folder…" />
        <button onclick={pickOutDir} disabled={busy}>Browse</button>
      </div>
    </div>

    <div class="actions">
      <button
        class="primary"
        onclick={runSplit}
        disabled={busy || !!plan.error}
      >
        {status.kind === "working" ? "Splitting…" : plan.error ? "Split PDF" : `Create ${plan.ranges.length} PDF${plan.ranges.length === 1 ? "" : "s"}`}
      </button>
    </div>
  {/if}

  {#if status.kind === "working"}
    <div class="status" role="status">{status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err" role="alert">✕ {status.msg}</div>
  {/if}
  <SavedFiles paths={outputs} />
</section>

<style>
  .source-info { min-width: 0; }
  .file-name { overflow-wrap: anywhere; }
  .tabs { flex-wrap: wrap; }
  .field-label, .field-hint { color: var(--text-2); }
  .row input { min-width: 0; }
  input[aria-invalid="true"] { border-color: var(--danger); }
  .split-preview { padding: 16px; background: var(--bg-2); border: 1px solid var(--border); border-radius: var(--r-md); }
  .split-preview h2 { margin: 0 0 8px; font-size: 14px; }
  .preview-summary { font-weight: 600; margin: 0 0 12px; }
  .preview-summary span { color: var(--text-2); font-weight: 400; }
  .validation-error { color: var(--text); border-left: 3px solid var(--danger); padding-left: 12px; margin: 0; }
  .split-preview ol { list-style: none; padding: 0; margin: 0; max-height: 280px; overflow-y: auto; }
  .split-preview li { padding: 10px 0; display: flex; flex-direction: column; gap: 3px; border-top: 1px solid var(--border); font-size: 12px; overflow-wrap: anywhere; }
  .split-preview li span { color: var(--text-2); }
</style>
