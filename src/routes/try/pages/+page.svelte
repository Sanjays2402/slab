<script lang="ts">
  import { onMount } from "svelte";
  import { loadSample, SAMPLES } from "$lib/try/samples";
  import {
    rotatePages,
    removePages,
    reorderPages,
  } from "$lib/try/pdfOps";
  import DownloadWall from "$lib/try/DownloadWall.svelte";

  let bytes: Uint8Array | null = null;
  let pageCount = 0;
  // The order array IS the source of truth for the canvas grid.
  // newOrder[i] = index in the ORIGINAL doc to render at position i.
  let order: number[] = [];
  let selected = new Set<number>();
  let busy = false;
  let status = "";
  let sourceLabel = "";
  let thumbs: string[] = []; // dataURL per ORIGINAL page index
  let wallFeature = "";
  let wallOpen = false;

  async function loadFromQuery() {
    const params = new URLSearchParams(window.location.search);
    const slug = params.get("sample");
    const source = params.get("source");
    busy = true;
    try {
      if (source === "user") {
        const name = sessionStorage.getItem("try:user-pdf-name") ?? "your PDF";
        const b64 = sessionStorage.getItem("try:user-pdf-bytes");
        if (!b64) throw new Error("no user PDF in session");
        const bin = atob(b64);
        const arr = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
        bytes = arr;
        sourceLabel = name;
      } else {
        const chosen = slug && SAMPLES.find((s) => s.slug === slug)
          ? slug
          : SAMPLES[0].slug;
        bytes = await loadSample(chosen);
        const meta = SAMPLES.find((s) => s.slug === chosen)!;
        sourceLabel = meta.label;
      }
      await renderAllThumbs();
    } catch (err) {
      status = `Could not load: ${(err as Error).message}`;
    } finally {
      busy = false;
    }
  }

  async function renderAllThumbs() {
    if (!bytes) return;
    // Lazy-import pdfjs only when actually rendering.
    const pdfjs = await import("pdfjs-dist");
    // Configure worker — same-origin only.
    (pdfjs as any).GlobalWorkerOptions.workerSrc =
      new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).href;
    const doc = await (pdfjs as any).getDocument({ data: bytes.slice() }).promise;
    pageCount = doc.numPages;
    order = Array.from({ length: pageCount }, (_, i) => i);
    selected = new Set();
    thumbs = new Array(pageCount).fill("");
    for (let i = 0; i < pageCount; i++) {
      const page = await doc.getPage(i + 1);
      const vp = page.getViewport({ scale: 0.4 });
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(vp.width);
      canvas.height = Math.ceil(vp.height);
      const ctx = canvas.getContext("2d")!;
      await page.render({ canvasContext: ctx, viewport: vp }).promise;
      thumbs[i] = canvas.toDataURL("image/png");
      thumbs = thumbs;
    }
  }

  function toggle(i: number, ev?: MouseEvent) {
    const next = new Set(selected);
    if (ev?.shiftKey && selected.size > 0) {
      // Range select
      const last = Math.max(...selected);
      const [a, b] = i < last ? [i, last] : [last, i];
      for (let k = a; k <= b; k++) next.add(k);
    } else if (next.has(i)) {
      next.delete(i);
    } else {
      next.add(i);
    }
    selected = next;
  }

  async function doRotate() {
    if (!bytes || selected.size === 0) return;
    busy = true;
    status = "Rotating…";
    try {
      // Rotate by ORIGINAL index — translate visual positions through `order`.
      const origIndices = [...selected].map((vi) => order[vi]);
      bytes = await rotatePages(bytes, origIndices, 90);
      await renderAllThumbs();
      status = `Rotated ${origIndices.length} page(s).`;
    } finally {
      busy = false;
    }
  }

  async function doRemove() {
    if (!bytes || selected.size === 0) return;
    if (selected.size >= pageCount) {
      status = "Cannot remove every page.";
      return;
    }
    busy = true;
    status = "Removing…";
    try {
      const origIndices = [...selected].map((vi) => order[vi]);
      bytes = await removePages(bytes, origIndices);
      await renderAllThumbs();
      status = `Removed ${origIndices.length} page(s).`;
    } finally {
      busy = false;
    }
  }

  async function doMoveUp() {
    if (selected.size !== 1) return;
    const i = [...selected][0];
    if (i === 0) return;
    const newOrder = order.slice();
    [newOrder[i - 1], newOrder[i]] = [newOrder[i], newOrder[i - 1]];
    order = newOrder;
    selected = new Set([i - 1]);
    await commitReorder();
  }
  async function doMoveDown() {
    if (selected.size !== 1) return;
    const i = [...selected][0];
    if (i === pageCount - 1) return;
    const newOrder = order.slice();
    [newOrder[i + 1], newOrder[i]] = [newOrder[i], newOrder[i + 1]];
    order = newOrder;
    selected = new Set([i + 1]);
    await commitReorder();
  }
  async function commitReorder() {
    if (!bytes) return;
    busy = true;
    try {
      bytes = await reorderPages(bytes, order);
      // Thumbs were already keyed to original indices — re-render against new bytes.
      await renderAllThumbs();
    } finally {
      busy = false;
    }
  }

  async function downloadResult() {
    if (!bytes) return;
    const blob = new Blob([bytes], { type: "application/pdf" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `slab-edited-${Date.now()}.pdf`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    status = "Saved to your downloads.";
  }

  function openWall(feat: string) {
    wallFeature = feat;
    wallOpen = true;
  }

  onMount(() => {
    loadFromQuery();
    const onKey = (e: KeyboardEvent) => {
      if (busy) return;
      if (e.key === "r" || e.key === "R") doRotate();
      else if (e.key === "Delete" || e.key === "Backspace") doRemove();
      else if (e.key === "ArrowUp" && (e.metaKey || e.ctrlKey)) doMoveUp();
      else if (e.key === "ArrowDown" && (e.metaKey || e.ctrlKey)) doMoveDown();
      else if (e.key === "s" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        downloadResult();
      } else if (e.key === "Escape") selected = new Set();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<svelte:head>
  <title>Page tools — Slab in your browser</title>
</svelte:head>

<header class="hd">
  <div>
    <div class="crumb">Try Slab · Page tools</div>
    <h1>{sourceLabel || "Loading…"}</h1>
  </div>
  <div class="hd-actions">
    <button class="primary" disabled={!bytes || busy} on:click={downloadResult}>
      Save as PDF
    </button>
  </div>
</header>

<div class="toolbar">
  <button disabled={selected.size === 0 || busy} on:click={doRotate}
          title="Rotate selected (R)">
    ↻ Rotate
  </button>
  <button disabled={selected.size === 0 || busy} on:click={doRemove}
          title="Remove selected (Del)">
    ✕ Remove
  </button>
  <button disabled={selected.size !== 1 || busy} on:click={doMoveUp}
          title="Move up (⌘↑)">
    ↑ Up
  </button>
  <button disabled={selected.size !== 1 || busy} on:click={doMoveDown}
          title="Move down (⌘↓)">
    ↓ Down
  </button>

  <div class="spacer"></div>

  <button on:click={() => openWall("ocr")} class="gated">🔒 OCR</button>
  <button on:click={() => openWall("sign")} class="gated">🔒 Sign</button>
  <button on:click={() => openWall("redact")} class="gated">🔒 Redact</button>
  <button on:click={() => openWall("beacon")} class="gated">🔒 Beacon AI</button>
  <button on:click={() => openWall("compress")} class="gated">🔒 Compress</button>

  <div class="status">{status}</div>
</div>

<div class="grid" class:busy>
  {#each order as origIdx, visualIdx}
    <button class="thumb" class:sel={selected.has(visualIdx)}
            type="button"
            on:click={(e) => toggle(visualIdx, e)}>
      <div class="thumb-num">{visualIdx + 1}</div>
      {#if thumbs[origIdx]}
        <img src={thumbs[origIdx]} alt="Page {visualIdx + 1}" />
      {:else}
        <div class="thumb-skel"></div>
      {/if}
    </button>
  {/each}
</div>

<p class="hint">
  Click to select · shift-click for range · <kbd>R</kbd> rotate · <kbd>Del</kbd>
  remove · <kbd>⌘↑</kbd>/<kbd>⌘↓</kbd> reorder · <kbd>⌘S</kbd> save · <kbd>Esc</kbd> clear
</p>

<DownloadWall bind:open={wallOpen} feature={wallFeature} />

<style>
  .hd {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 16px;
    margin-bottom: 18px;
  }
  .crumb {
    font-size: 12px;
    color: rgba(243, 243, 245, 0.55);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    margin-bottom: 4px;
  }
  .hd h1 {
    margin: 0;
    font-size: 28px;
    letter-spacing: -0.02em;
  }
  .primary {
    padding: 10px 18px;
    border-radius: 10px;
    background: linear-gradient(135deg, #ffbf00, #ff8b00);
    color: #1a1a1a;
    border: 0;
    font-weight: 600;
    cursor: pointer;
    font-size: 14px;
    box-shadow: 0 4px 18px rgba(255, 140, 0, 0.3);
  }
  .primary:disabled { opacity: 0.4; cursor: not-allowed; }

  .toolbar {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 10px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  .toolbar button {
    padding: 8px 12px;
    background: rgba(255, 255, 255, 0.06);
    color: #f3f3f5;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .toolbar button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }
  .toolbar button:disabled { opacity: 0.35; cursor: not-allowed; }
  .toolbar button.gated {
    background: rgba(255, 191, 0, 0.08);
    border-color: rgba(255, 191, 0, 0.2);
    color: rgba(255, 216, 102, 0.95);
  }
  .toolbar button.gated:hover {
    background: rgba(255, 191, 0, 0.18);
  }
  .spacer { flex: 1; }
  .status {
    margin-left: auto;
    font-size: 12px;
    color: rgba(243, 243, 245, 0.55);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 12px;
    transition: opacity 0.15s;
  }
  .grid.busy { opacity: 0.5; pointer-events: none; }

  .thumb {
    position: relative;
    background: rgba(255, 255, 255, 0.04);
    border: 2px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 8px;
    cursor: pointer;
    transition: border-color 0.12s, background 0.12s, transform 0.08s;
  }
  .thumb:hover { background: rgba(255, 255, 255, 0.07); }
  .thumb.sel {
    border-color: #ffbf00;
    background: rgba(255, 191, 0, 0.08);
    box-shadow: 0 0 0 1px rgba(255, 191, 0, 0.4);
  }
  .thumb img {
    width: 100%;
    height: auto;
    display: block;
    background: #fff;
    border-radius: 4px;
  }
  .thumb-skel {
    width: 100%;
    aspect-ratio: 3/4;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 4px;
  }
  .thumb-num {
    position: absolute;
    top: 4px;
    left: 4px;
    z-index: 2;
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 6px;
    font-weight: 600;
  }

  .hint {
    margin-top: 18px;
    font-size: 12px;
    color: rgba(243, 243, 245, 0.5);
  }
  kbd {
    background: rgba(255, 255, 255, 0.08);
    padding: 1px 6px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    border: 1px solid rgba(255, 255, 255, 0.1);
  }
</style>
