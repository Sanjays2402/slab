<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { mdToPdf, type MdToPdfOptions } from "$lib/try/mdToPdf";
  import DownloadWall from "$lib/try/DownloadWall.svelte";

  const SAMPLE = `# Welcome to Slab

Markdown → PDF, **right in your browser.** No upload. No account. No watermark.

## What works here

- Headings H1–H6
- **Bold**, *italic*, and \`inline code\`
- Bullet lists and numbered lists
- Block quotes
- Fenced code blocks
- Horizontal rules
- Auto-pagination

> Privacy is the wedge. Watch the *0 bytes uploaded* counter at the bottom of this page.

## Why this matters

Smallpdf and iLovePDF charge \$7–\$12/mo to do this conversion — *and they upload your file to their servers*. Slab does it free, in this tab, and the bits never leave your machine.

### Code example

\`\`\`
function hello(name) {
  return 'Hello, ' + name + '!';
}
\`\`\`

---

Edit on the left. Watch the PDF update on the right.

When you want **image embedding**, **custom fonts**, **footnotes**, or **math typesetting**, hop into the desktop app — that's where the heavy machinery lives.
`;

  let markdown = $state(SAMPLE);
  let pageSize = $state<"A4" | "Letter" | "Legal">("Letter");
  let pdfBytes = $state<Uint8Array | null>(null);
  let previewDataUrl = $state<string>("");
  let busy = $state(false);
  let error = $state("");
  let pageCount = $state(0);
  let wallOpen = $state(false);
  let wallFeature = $state<string>("markdown");

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let editor: HTMLTextAreaElement | null = $state(null);

  async function rebuild() {
    error = "";
    busy = true;
    try {
      const opts: MdToPdfOptions = {
        pageSize,
        title: "Slab Markdown export",
        author: "try.slab.app",
      };
      pdfBytes = await mdToPdf(markdown, opts);
      await renderPreview(pdfBytes);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function renderPreview(bytes: Uint8Array) {
    // Lazy-import pdfjs — same pattern as /try/pages.
    const pdfjs = await import("pdfjs-dist");
    (pdfjs as any).GlobalWorkerOptions.workerSrc = new URL(
      "pdfjs-dist/build/pdf.worker.min.mjs",
      import.meta.url,
    ).href;
    const doc = await (pdfjs as any).getDocument({ data: bytes.slice() })
      .promise;
    pageCount = doc.numPages;
    // Render only page 1 for the live preview — keeps re-render fast.
    const page = await doc.getPage(1);
    const vp = page.getViewport({ scale: 1.0 });
    const canvas = document.createElement("canvas");
    canvas.width = Math.ceil(vp.width);
    canvas.height = Math.ceil(vp.height);
    const ctx = canvas.getContext("2d")!;
    await page.render({ canvasContext: ctx, viewport: vp }).promise;
    previewDataUrl = canvas.toDataURL("image/png");
  }

  function scheduleRebuild() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(rebuild, 400);
  }

  function download() {
    if (!pdfBytes) return;
    const blob = new Blob([pdfBytes.slice() as BlobPart], {
      type: "application/pdf",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "slab-markdown.pdf";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function wrapSelection(prefix: string, suffix = prefix) {
    if (!editor) return;
    const start = editor.selectionStart;
    const end = editor.selectionEnd;
    const before = markdown.slice(0, start);
    const sel = markdown.slice(start, end);
    const after = markdown.slice(end);
    markdown = `${before}${prefix}${sel}${suffix}${after}`;
    // Restore selection just inside the new wrappers.
    requestAnimationFrame(() => {
      if (!editor) return;
      editor.focus();
      editor.selectionStart = start + prefix.length;
      editor.selectionEnd = end + prefix.length;
    });
    scheduleRebuild();
  }

  function openWall(feature: string) {
    wallFeature = feature;
    wallOpen = true;
  }

  function handleKey(ev: KeyboardEvent) {
    const mod = ev.metaKey || ev.ctrlKey;
    if (!mod) return;
    if (ev.key === "s" || ev.key === "S") {
      ev.preventDefault();
      download();
    } else if (ev.key === "b" || ev.key === "B") {
      ev.preventDefault();
      wrapSelection("**");
    } else if (ev.key === "i" || ev.key === "I") {
      ev.preventDefault();
      wrapSelection("*");
    }
  }

  onMount(() => {
    rebuild();
    window.addEventListener("keydown", handleKey);
  });

  onDestroy(() => {
    if (typeof window !== "undefined") {
      window.removeEventListener("keydown", handleKey);
    }
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  // Re-render on every markdown / pageSize change.
  $effect(() => {
    // Touch the reactive deps so $effect tracks them.
    void markdown;
    void pageSize;
    scheduleRebuild();
  });
</script>

<svelte:head>
  <title>Markdown to PDF — try.slab.app</title>
  <meta
    name="description"
    content="Convert Markdown to PDF in your browser. Zero upload. Free. The thing Smallpdf and iLovePDF charge for."
  />
</svelte:head>

<div class="md-shell">
  <header class="bar">
    <div class="brand">
      <a href="/try" aria-label="Back to /try"
        ><span class="dot"></span> Slab playground</a
      >
      <span class="slash">›</span>
      <strong>Markdown → PDF</strong>
    </div>
    <div class="actions">
      <label class="size">
        Page
        <select bind:value={pageSize}>
          <option value="Letter">Letter</option>
          <option value="A4">A4</option>
          <option value="Legal">Legal</option>
        </select>
      </label>
      <span class="hint">
        {#if busy}rendering…{:else}{pageCount} page{pageCount === 1 ? "" : "s"}{/if}
      </span>
      <button class="primary" onclick={download} disabled={!pdfBytes}>
        Download PDF <kbd>⌘S</kbd>
      </button>
      <button class="ghost" onclick={() => openWall("markdown")}>
        Want images / fonts / footnotes?
      </button>
    </div>
  </header>

  {#if error}
    <div class="error">⚠ {error}</div>
  {/if}

  <main class="split">
    <section class="editor-pane">
      <div class="pane-head">
        <span>Markdown</span>
        <div class="toolbar">
          <button onclick={() => wrapSelection("**")} title="Bold (⌘B)"
            ><strong>B</strong></button
          >
          <button onclick={() => wrapSelection("*")} title="Italic (⌘I)"
            ><em>I</em></button
          >
          <button onclick={() => wrapSelection("`")} title="Inline code"
            ><code>{"</>"}</code></button
          >
          <button onclick={() => wrapSelection("\n```\n", "\n```\n")}
            title="Code block">⌽</button
          >
          <button onclick={() => wrapSelection("\n> ", "")} title="Quote">❝</button>
        </div>
      </div>
      <textarea
        bind:this={editor}
        bind:value={markdown}
        spellcheck="false"
        autocomplete="off"
        aria-label="Markdown editor"
      ></textarea>
    </section>

    <section class="preview-pane">
      <div class="pane-head">
        <span>PDF preview</span>
        <span class="muted">page 1 of {pageCount}</span>
      </div>
      <div class="preview-canvas">
        {#if previewDataUrl}
          <img src={previewDataUrl} alt="PDF preview of page 1" />
        {:else}
          <div class="placeholder">Generating preview…</div>
        {/if}
      </div>
    </section>
  </main>
</div>

<DownloadWall feature={wallFeature} bind:open={wallOpen} />

<style>
  .md-shell {
    display: flex;
    flex-direction: column;
    height: 100dvh;
    background: #f6f7f9;
    color: #15171b;
    font-family: -apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", sans-serif;
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px;
    background: rgba(255, 255, 255, 0.7);
    backdrop-filter: saturate(180%) blur(20px);
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    font-size: 13px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #444;
  }
  .brand a {
    color: inherit;
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .brand a:hover {
    color: #111;
  }
  .brand .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: linear-gradient(135deg, #ff80b5, #9089fc);
    display: inline-block;
  }
  .slash {
    color: #999;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .actions .size {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: #555;
  }
  .actions select {
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 6px;
    padding: 3px 6px;
    background: white;
    font: inherit;
  }
  .actions .hint {
    color: #888;
    font-variant-numeric: tabular-nums;
    min-width: 70px;
    text-align: right;
  }

  .primary,
  .ghost {
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 7px;
    padding: 6px 12px;
    font: inherit;
    cursor: pointer;
    background: white;
    color: #15171b;
    transition: transform 80ms, box-shadow 80ms;
  }
  .primary {
    background: linear-gradient(135deg, #1f2937, #0f172a);
    color: white;
    border-color: transparent;
  }
  .primary:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .primary:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 14px rgba(15, 23, 42, 0.25);
  }
  .ghost:hover {
    background: #eef2f7;
  }
  kbd {
    font: inherit;
    font-size: 11px;
    padding: 1px 5px;
    margin-left: 5px;
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.1);
  }

  .error {
    background: #fff3cd;
    color: #7a5c00;
    padding: 8px 18px;
    font-size: 13px;
    border-bottom: 1px solid #f0d97a;
  }

  .split {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: rgba(0, 0, 0, 0.06);
    overflow: hidden;
  }

  .editor-pane,
  .preview-pane {
    background: white;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .pane-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 14px;
    font-size: 12px;
    color: #5a5e66;
    background: #fafbfc;
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
  }
  .pane-head .muted {
    color: #999;
  }

  .toolbar {
    display: flex;
    gap: 4px;
  }
  .toolbar button {
    width: 26px;
    height: 24px;
    border: 1px solid rgba(0, 0, 0, 0.08);
    background: white;
    border-radius: 5px;
    cursor: pointer;
    font-size: 12px;
  }
  .toolbar button:hover {
    background: #f0f2f5;
  }

  textarea {
    flex: 1;
    border: none;
    padding: 18px 22px;
    font-family: "SF Mono", "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
    font-size: 13.5px;
    line-height: 1.6;
    resize: none;
    outline: none;
    color: #1a1d23;
    background: white;
    tab-size: 2;
  }

  .preview-canvas {
    flex: 1;
    overflow: auto;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    background: #e9ecf1;
    padding: 24px;
  }
  .preview-canvas img {
    max-width: 100%;
    height: auto;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.12), 0 1px 3px rgba(0, 0, 0, 0.08);
    background: white;
  }
  .placeholder {
    color: #888;
    padding: 40px;
    font-size: 13px;
  }

  @media (max-width: 880px) {
    .split {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 1fr;
    }
    .actions .ghost {
      display: none;
    }
  }
</style>
