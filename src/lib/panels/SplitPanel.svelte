<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let rangeText = $state("");
  let chunkSize = $state(1);
  let mode = $state<"ranges" | "every">("ranges");
  let outDir = $state<string | null>(null);
  let status = $state<Status>(idle);
  let outputs = $state<string[]>([]);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    outputs = [];
    status = idle;
    const res = await invoke<CmdResult<number>>("slab_page_count", { input: picked });
    pageCount = res.kind === "ok" ? res.value : null;
  }

  async function pickOutDir() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    outDir = picked;
  }

  function parseRanges(text: string): { start: number; end: number }[] | string {
    const parts = text.split(",").map((s) => s.trim()).filter(Boolean);
    if (parts.length === 0) return "Add at least one range, e.g. 1-3, 5, 7-9.";
    const result: { start: number; end: number }[] = [];
    for (const p of parts) {
      const m = p.match(/^(\d+)(?:-(\d+))?$/);
      if (!m) return `Invalid range: "${p}".`;
      const start = parseInt(m[1], 10);
      const end = m[2] ? parseInt(m[2], 10) : start;
      if (start < 1 || end < start) return `Invalid range: "${p}".`;
      result.push({ start, end });
    }
    return result;
  }

  async function runSplit() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    let dest = outDir;
    if (!dest) {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      dest = picked;
      outDir = dest;
    }

    if (mode === "ranges") {
      const parsed = parseRanges(rangeText);
      if (typeof parsed === "string") {
        status = { kind: "err", msg: parsed };
        return;
      }
      status = { kind: "working", msg: `Splitting ${parsed.length} part(s)…` };
      try {
        const res = await invoke<CmdResult<string[]>>("slab_split_ranges", {
          input,
          ranges: parsed,
          outDir: dest,
        });
        if (res.kind === "ok") {
          outputs = res.value;
          status = { kind: "ok", msg: `Wrote ${res.value.length} file(s) to ${dest}` };
        } else {
          status = { kind: "err", msg: res.message };
        }
      } catch (e) {
        status = { kind: "err", msg: String(e) };
      }
    } else {
      if (chunkSize < 1) {
        status = { kind: "err", msg: "Chunk size must be at least 1." };
        return;
      }
      status = { kind: "working", msg: `Splitting every ${chunkSize} page(s)…` };
      try {
        const res = await invoke<CmdResult<string[]>>("slab_split_every", {
          input,
          chunkSize,
          outDir: dest,
        });
        if (res.kind === "ok") {
          outputs = res.value;
          status = { kind: "ok", msg: `Wrote ${res.value.length} file(s) to ${dest}` };
        } else {
          status = { kind: "err", msg: res.message };
        }
      } catch (e) {
        status = { kind: "err", msg: String(e) };
      }
    }
  }
</script>

<header class="content-header">
  <h1>Split PDF</h1>
  <p class="subtitle">Cut a PDF into pieces by page range or every N pages.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
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
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="tabs">
      <button class:tab-active={mode === "ranges"} onclick={() => (mode = "ranges")}>
        By ranges
      </button>
      <button class:tab-active={mode === "every"} onclick={() => (mode = "every")}>
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
        />
        <span class="field-hint">Comma-separated. Single pages or N-M ranges.</span>
      </label>
    {:else}
      <label class="field">
        <span class="field-label">Pages per chunk</span>
        <input
          type="number"
          min="1"
          bind:value={chunkSize}
        />
        <span class="field-hint">e.g. 2 → one PDF per 2-page chunk.</span>
      </label>
    {/if}

    <label class="field">
      <span class="field-label">Output folder</span>
      <div class="row">
        <input type="text" readonly value={outDir ?? ""} placeholder="Choose a folder…" />
        <button onclick={pickOutDir}>Browse</button>
      </div>
    </label>

    <div class="actions">
      <button
        class="primary"
        onclick={runSplit}
        disabled={status.kind === "working"}
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
