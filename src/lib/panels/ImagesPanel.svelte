<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let format = $state<"png" | "jpeg">("png");
  let dpi = $state(150);
  let rangeText = $state("");
  let status = $state<Status>(idle);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
    const res = await invoke<CmdResult<number>>("slab_page_count", { input: picked });
    pageCount = res.kind === "ok" ? res.value : null;
  }

  // "1-3, 5" -> [1, 2, 3, 5]. Empty string means every page.
  function parsePages(text: string): number[] | string {
    const parts = text.split(",").map((s) => s.trim()).filter(Boolean);
    const out = new Set<number>();
    for (const p of parts) {
      const m = p.match(/^(\d+)(?:-(\d+))?$/);
      if (!m) return `Bad page range: "${p}". Try something like 1-3, 5.`;
      const start = parseInt(m[1], 10);
      const end = m[2] ? parseInt(m[2], 10) : start;
      if (start < 1 || end < start) return `Bad page range: "${p}". Try something like 1-3, 5.`;
      if (pageCount !== null && end > pageCount) return `Page ${end} is past the end (this PDF has ${pageCount} pages).`;
      if (end - start > 100000) return `Range "${p}" is too large.`;
      for (let n = start; n <= end; n++) out.add(n);
    }
    return [...out].sort((a, b) => a - b);
  }

  async function runExport() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (!Number.isInteger(dpi) || dpi < 36 || dpi > 600) {
      status = { kind: "err", msg: "Resolution must be a whole number from 36 to 600 DPI." };
      return;
    }
    const pages = parsePages(rangeText);
    if (typeof pages === "string") {
      status = { kind: "err", msg: pages };
      return;
    }
    const parent = await open({ directory: true, multiple: false });
    if (typeof parent !== "string") return;
    const sep = parent.includes("\\") ? "\\" : "/";
    const outDir = `${parent}${sep}${stripExt(basename(input))}-images`;

    status = { kind: "working", msg: "Rendering pages…" };
    try {
      const res = await invoke<CmdResult<string[]>>("slab_pdf_to_images", {
        input,
        outputDir: outDir,
        dpi,
        format,
        pages,
      });
      status =
        res.kind === "ok"
          ? { kind: "ok", msg: `Saved ${res.value.length} image(s) to ${basename(outDir)}.` }
          : { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>PDF to Images</h1>
  <p class="subtitle">Save pages as PNG or JPEG pictures. Needs Poppler (pdftoppm).</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick a file, then choose format and resolution.</span>
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

    <div class="tabs" role="radiogroup" aria-label="Image format">
      <button class:tab-active={format === "png"} onclick={() => (format = "png")}>PNG</button>
      <button class:tab-active={format === "jpeg"} onclick={() => (format = "jpeg")}>JPEG</button>
    </div>

    <label class="field">
      <span class="field-label">Resolution (DPI)</span>
      <input type="number" min="36" max="600" step="1" bind:value={dpi} />
      <span class="field-hint">72 for screens, 150 for general use, 300 for print.</span>
    </label>

    <label class="field">
      <span class="field-label">Pages</span>
      <input type="text" placeholder="1-3, 5" bind:value={rangeText} />
      <span class="field-hint">Leave blank for every page.</span>
    </label>

    <div class="actions">
      <button class="primary" onclick={runExport} disabled={status.kind === "working"}>
        {status.kind === "working" ? "Rendering…" : "Export images"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>
