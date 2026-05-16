<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);
  let pages = $state<string[]>([]);
  let view = $state<"preview" | "save">("preview");

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    pages = [];
    status = idle;
  }

  async function runExtract() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = { kind: "working", msg: "Extracting text…" };
    try {
      const res = await invoke<CmdResult<string[]>>("slab_extract_text", { input });
      if (res.kind === "ok") {
        pages = res.value;
        const chars = pages.reduce((acc, p) => acc + p.length, 0);
        status = {
          kind: "ok",
          msg: `${pages.length} page(s), ${chars.toLocaleString()} characters`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function saveAsTxt() {
    if (!input) return;
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}.txt`,
      filters: [{ name: "Text", extensions: ["txt"] }],
    });
    if (typeof output !== "string") return;
    status = { kind: "working", msg: "Writing .txt…" };
    try {
      const res = await invoke<CmdResult<string>>("slab_extract_text_save", {
        input,
        output,
      });
      if (res.kind === "ok") {
        status = { kind: "ok", msg: `Wrote ${basename(res.value)}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function copyAll() {
    const text = pages.join("\n\n---\n\n");
    try {
      await navigator.clipboard.writeText(text);
      status = { kind: "ok", msg: "Copied to clipboard." };
    } catch (e) {
      status = { kind: "err", msg: `Copy failed: ${e}` };
    }
  }
</script>

<header class="content-header">
  <h1>Extract Text</h1>
  <p class="subtitle">Pull the text out of a PDF. Preview it or save to .txt.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Native text first — scanned PDFs? Use 👁 OCR in the Reader panel.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if pages.length > 0}{pages.length} page(s) extracted{:else}Ready{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="actions">
      <button onclick={runExtract} disabled={status.kind === "working"}>
        {status.kind === "working" && status.msg.startsWith("Extracting")
          ? "Extracting…"
          : pages.length > 0
            ? "Re-extract"
            : "Extract"}
      </button>
      {#if pages.length > 0}
        <button onclick={copyAll}>Copy all</button>
        <button class="primary" onclick={saveAsTxt}>Save .txt</button>
      {/if}
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if pages.length > 0}
    <div class="preview">
      {#each pages as page, i}
        <details open={i === 0}>
          <summary>Page {i + 1} <span class="ch">({page.length} chars)</span></summary>
          <pre>{page || "(no text on this page)"}</pre>
        </details>
      {/each}
    </div>
  {/if}
</section>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }
  .preview details {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 10px 12px;
  }
  .preview summary {
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
  }
  .preview .ch {
    color: var(--text-3);
    font-size: 11px;
    margin-left: 6px;
  }
  .preview pre {
    margin: 10px 0 0;
    padding: 10px;
    background: var(--bg);
    border-radius: var(--r-sm);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 280px;
    overflow-y: auto;
  }
</style>
