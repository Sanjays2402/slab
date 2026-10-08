<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type PageInk = { page: number; ink: number; blank: boolean };

  // Slider is a sensitivity step; each maps to a max ink fraction.
  const LEVELS = [
    { label: "Strict", value: 0.0001, hint: "Only pages that are completely empty." },
    { label: "Normal", value: 0.0005, hint: "Ignores tiny specks and scanner dust." },
    { label: "Lenient", value: 0.003, hint: "Also drops near-empty pages (a stray mark or page number)." },
  ];

  let input = $state<string | null>(null);
  let level = $state(1);
  let scan = $state<PageInk[] | null>(null);
  let status = $state<Status>(idle);

  let threshold = $derived(LEVELS[level].value);
  let blanks = $derived(scan ? scan.filter((p) => p.blank).map((p) => p.page) : []);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    scan = null;
    status = idle;
    await runScan();
  }

  async function runScan() {
    if (!input) return;
    status = { kind: "working", msg: "Scanning pages…" };
    try {
      const res = await invoke<CmdResult<PageInk[]>>("slab_scan_blank_pages", {
        input,
        threshold,
      });
      if (res.kind === "ok") {
        scan = res.value;
        status = idle;
      } else {
        scan = null;
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      scan = null;
      status = { kind: "err", msg: String(e) };
    }
  }

  async function setLevel(i: number) {
    level = i;
    await runScan();
  }

  async function runRemove() {
    if (!input || blanks.length === 0) return;
    const output = await save({
      defaultPath: `${stripExt(basename(input))}-no-blanks.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;
    const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();
    if (norm(output) === norm(input)) {
      status = { kind: "err", msg: "Pick an output file that isn't the input." };
      return;
    }
    status = { kind: "working", msg: "Removing blank pages…" };
    try {
      const res = await invoke<CmdResult<number[]>>("slab_remove_blank_pages", {
        input,
        output,
        threshold,
      });
      status =
        res.kind === "ok"
          ? { kind: "ok", msg: `Removed ${res.value.length} page(s). Saved to ${basename(output)}.` }
          : { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Remove Blank Pages</h1>
  <p class="subtitle">Find empty pages in scans and drop them. Needs Poppler (pdftoppm).</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Slab scans it and shows which pages are blank.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if scan}{scan.length} page{scan.length === 1 ? "" : "s"} · {blanks.length} blank{:else}…{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="tabs" role="radiogroup" aria-label="Sensitivity">
      {#each LEVELS as l, i}
        <button class:tab-active={level === i} onclick={() => setLevel(i)} title={l.hint}>
          {l.label}
        </button>
      {/each}
    </div>
    <p class="field-hint">{LEVELS[level].hint}</p>

    {#if scan}
      <p class="field-hint">
        {#if blanks.length === 0}
          No blank pages found.
        {:else}
          Blank pages: {blanks.join(", ")}
        {/if}
      </p>
    {/if}

    <div class="actions">
      <button
        class="primary"
        onclick={runRemove}
        disabled={status.kind === "working" || blanks.length === 0}
      >
        {blanks.length === 0 ? "Nothing to remove" : `Remove ${blanks.length} page${blanks.length === 1 ? "" : "s"}`}
      </button>
    </div>
  {/if}

  {#if status.kind === "working"}
    <div class="status">{status.msg}</div>
  {:else if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>
