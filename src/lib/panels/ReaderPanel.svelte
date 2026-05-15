<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { readFile } from "@tauri-apps/plugin-fs";
  import { basename } from "$lib/types";
  // @ts-expect-error - pdfjs-dist .mjs has no types index alias
  import * as pdfjsLib from "pdfjs-dist/build/pdf.mjs";
  import { EventBus, PDFFindController, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
  import "pdfjs-dist/web/pdf_viewer.css";
  import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";

  pdfjsLib.GlobalWorkerOptions.workerSrc = workerSrc;

  type Doc = {
    path: string;
    pageCount: number;
  };

  let doc = $state<Doc | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);

  let currentPage = $state(1);
  let zoomLabel = $state("page-width"); // sync with PDFViewer.currentScaleValue
  let zoomPct = $state(100);
  let findOpen = $state(false);
  let findQuery = $state("");
  let findStatus = $state<{ current: number; total: number }>({ current: 0, total: 0 });
  let findCaseSensitive = $state(false);
  let findWholeWord = $state(false);
  let thumbsOpen = $state(true);

  // Refs
  let containerEl: HTMLDivElement | undefined = $state();
  let viewerEl: HTMLDivElement | undefined = $state();
  let thumbCanvases: Map<number, HTMLCanvasElement> = new Map();
  let thumbButtons: Map<number, HTMLButtonElement> = new Map();

  // pdf.js objects
  let pdfDocument: any = null;
  let eventBus: any = null;
  let linkService: any = null;
  let findController: any = null;
  let pdfViewer: any = null;
  let thumbsAbortController: AbortController | null = null;

  // ---------- File loading ----------
  function isInTauri(): boolean {
    return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
  }

  async function pickFile() {
    if (isInTauri()) {
      const picked = await open({
        multiple: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (typeof picked !== "string") return;
      await loadPath(picked);
    } else {
      // Browser dev fallback — uses the native file input
      const input = document.createElement("input");
      input.type = "file";
      input.accept = "application/pdf,.pdf";
      input.onchange = async () => {
        const f = input.files?.[0];
        if (!f) return;
        const buf = await f.arrayBuffer();
        await loadBytes(f.name, new Uint8Array(buf));
      };
      input.click();
    }
  }

  async function loadPath(path: string) {
    loading = true;
    loadError = null;
    try {
      const bytes = await readFile(path);
      await loadBytes(path, new Uint8Array(bytes));
    } catch (e: any) {
      loadError = e?.message || String(e);
      tearDownDoc();
      doc = null;
      loading = false;
    }
  }

  async function loadBytes(path: string, data: Uint8Array) {
    loading = true;
    loadError = null;
    try {
      tearDownDoc();
      const task = pdfjsLib.getDocument({ data, isEvalSupported: false });
      const pdf = await task.promise;
      pdfDocument = pdf;

      buildViewer();

      pdfViewer.setDocument(pdf);
      linkService.setDocument(pdf, null);

      doc = { path, pageCount: pdf.numPages };
      currentPage = 1;
      thumbsAbortController = new AbortController();
      void renderThumbsBatch(thumbsAbortController.signal);
    } catch (e: any) {
      loadError = e?.message || String(e);
      tearDownDoc();
      doc = null;
    } finally {
      loading = false;
    }
  }

  function tearDownDoc() {
    thumbsAbortController?.abort();
    thumbsAbortController = null;
    thumbCanvases.clear();
    if (pdfViewer) {
      try { pdfViewer.setDocument(null); } catch { /* ignore */ }
      try { pdfViewer.cleanup(); } catch { /* ignore */ }
    }
    if (pdfDocument) {
      try { pdfDocument.destroy(); } catch { /* ignore */ }
    }
    pdfDocument = null;
  }

  function buildViewer() {
    if (pdfViewer) return; // build once per session
    if (!containerEl || !viewerEl) return;

    eventBus = new EventBus();
    linkService = new PDFLinkService({ eventBus });
    findController = new PDFFindController({ eventBus, linkService });

    pdfViewer = new PDFViewer({
      container: containerEl,
      viewer: viewerEl,
      eventBus,
      linkService,
      findController,
      textLayerMode: 2, // ENABLE = 2 (selectable + searchable)
      annotationMode: 1, // ENABLE = 1 (just render, no editing)
      annotationEditorMode: 0, // NONE
      removePageBorders: false,
    });
    linkService.setViewer(pdfViewer);

    eventBus.on("pagesinit", () => {
      pdfViewer.currentScaleValue = "page-width";
      syncZoom();
    });
    eventBus.on("pagechanging", (e: any) => {
      currentPage = e.pageNumber;
    });
    eventBus.on("scalechanging", () => {
      syncZoom();
    });
    eventBus.on("updatefindcontrolstate", (s: any) => {
      findStatus = {
        current: s.matchesCount?.current ?? 0,
        total: s.matchesCount?.total ?? 0,
      };
    });
    eventBus.on("updatefindmatchescount", (s: any) => {
      findStatus = {
        current: s.matchesCount?.current ?? 0,
        total: s.matchesCount?.total ?? 0,
      };
    });
  }

  function syncZoom() {
    if (!pdfViewer) return;
    const scale = pdfViewer.currentScale;
    if (typeof scale === "number") zoomPct = Math.round(scale * 100);
    const v = pdfViewer.currentScaleValue;
    if (typeof v === "string") zoomLabel = v;
  }

  // ---------- Navigation / Zoom ----------
  function jumpTo(n: number) {
    if (!pdfViewer || !doc) return;
    const clamped = Math.max(1, Math.min(doc.pageCount, n));
    pdfViewer.currentPageNumber = clamped;
  }
  function nextPage() { jumpTo(currentPage + 1); }
  function prevPage() { jumpTo(currentPage - 1); }

  function setZoomValue(v: string | number) {
    if (!pdfViewer) return;
    pdfViewer.currentScaleValue = v;
    syncZoom();
  }
  function zoomIn() {
    if (!pdfViewer) return;
    setZoomValue(Math.min(4, +(pdfViewer.currentScale * 1.2).toFixed(2)));
  }
  function zoomOut() {
    if (!pdfViewer) return;
    setZoomValue(Math.max(0.25, +(pdfViewer.currentScale / 1.2).toFixed(2)));
  }

  // ---------- Find ----------
  function toggleFind() {
    findOpen = !findOpen;
    if (findOpen) {
      queueMicrotask(() => {
        const inp = document.querySelector<HTMLInputElement>(".find-input");
        inp?.focus();
        inp?.select();
      });
    } else {
      runFind("", "find");
    }
  }

  function runFind(q: string, type: string = "find") {
    if (!eventBus) return;
    eventBus.dispatch("find", {
      source: null,
      type,
      query: q,
      caseSensitive: findCaseSensitive,
      entireWord: findWholeWord,
      highlightAll: true,
      findPrevious: false,
    });
  }
  function findNext() {
    if (!eventBus) return;
    eventBus.dispatch("find", {
      source: null,
      type: "again",
      query: findQuery,
      caseSensitive: findCaseSensitive,
      entireWord: findWholeWord,
      highlightAll: true,
      findPrevious: false,
    });
  }
  function findPrev() {
    if (!eventBus) return;
    eventBus.dispatch("find", {
      source: null,
      type: "again",
      query: findQuery,
      caseSensitive: findCaseSensitive,
      entireWord: findWholeWord,
      highlightAll: true,
      findPrevious: true,
    });
  }

  // ---------- Thumbnails ----------
  async function renderThumbsBatch(signal: AbortSignal) {
    if (!doc || !pdfDocument) return;
    const total = doc.pageCount;
    for (let i = 1; i <= total; i++) {
      if (signal.aborted) return;
      // wait for the canvas to mount via {#each}
      const canvas = thumbCanvases.get(i);
      if (!canvas) {
        // attach event hasn't fired yet — retry next tick
        await new Promise((r) => setTimeout(r, 30));
        if (signal.aborted) return;
      }
      const c = thumbCanvases.get(i);
      if (!c) continue;
      try {
        const page = await pdfDocument.getPage(i);
        if (signal.aborted) return;
        const baseViewport = page.getViewport({ scale: 1 });
        const targetW = 120;
        const scale = targetW / baseViewport.width;
        const viewport = page.getViewport({ scale });
        const dpr = window.devicePixelRatio || 1;
        c.width = Math.floor(viewport.width * dpr);
        c.height = Math.floor(viewport.height * dpr);
        c.style.width = `${viewport.width}px`;
        c.style.height = `${viewport.height}px`;
        const ctx = c.getContext("2d");
        if (!ctx) continue;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        await page.render({ canvasContext: ctx, viewport, canvas: c }).promise;
      } catch {
        /* swallow page error */
      }
      await new Promise((r) => setTimeout(r, 0));
    }
  }

  // ---------- Keyboard ----------
  function onKey(e: KeyboardEvent) {
    if (!doc) return;
    const isMod = e.metaKey || e.ctrlKey;
    if (isMod && e.key === "f") {
      e.preventDefault();
      toggleFind();
    } else if (isMod && (e.key === "=" || e.key === "+")) {
      e.preventDefault();
      zoomIn();
    } else if (isMod && e.key === "-") {
      e.preventDefault();
      zoomOut();
    } else if (e.key === "Escape" && findOpen) {
      toggleFind();
    } else if (!findOpen && (e.target as HTMLElement)?.tagName !== "INPUT") {
      if (e.key === "ArrowRight" || e.key === "PageDown") {
        e.preventDefault();
        nextPage();
      } else if (e.key === "ArrowLeft" || e.key === "PageUp") {
        e.preventDefault();
        prevPage();
      }
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    tearDownDoc();
  });

  function attachThumb(el: HTMLCanvasElement, n: number) {
    thumbCanvases.set(n, el);
    return {
      destroy() { thumbCanvases.delete(n); },
    };
  }

  function attachThumbBtn(el: HTMLButtonElement, n: number) {
    thumbButtons.set(n, el);
    return {
      destroy() { thumbButtons.delete(n); },
    };
  }

  // Auto-scroll thumbnail sidebar when currentPage changes
  $effect(() => {
    const n = currentPage;
    if (!thumbsOpen) return;
    queueMicrotask(() => {
      const btn = thumbButtons.get(n);
      btn?.scrollIntoView({ behavior: "smooth", block: "nearest" });
    });
  });
</script>

<header class="content-header reader-header">
  <h1>Reader</h1>
  <p class="subtitle">
    {#if doc}
      {basename(doc.path)} · {doc.pageCount} page{doc.pageCount === 1 ? "" : "s"}
    {:else}
      Open any PDF — read, search, zoom. Files stay on your machine.
    {/if}
  </p>
</header>

{#if !doc}
  <section class="panel">
    <button class="dropzone" onclick={pickFile} disabled={loading}>
      <span class="dz-icon">+</span>
      <span class="dz-title">{loading ? "Loading…" : "Open a PDF"}</span>
      <span class="dz-hint">Single click, anywhere on your machine.</span>
    </button>
    {#if loadError}
      <div class="status err">✕ {loadError}</div>
    {/if}
  </section>
{/if}

<div class="reader-shell" class:hidden={!doc}>
  <div class="toolbar">
    <div class="tb-group">
      <button class="tb-btn" onclick={pickFile} title="Open another PDF">⊕ Open</button>
    </div>

    <div class="tb-group">
      <button class="tb-btn icon" onclick={() => (thumbsOpen = !thumbsOpen)} title="Toggle thumbnails">▦</button>
    </div>

    <div class="tb-group">
      <button class="tb-btn icon" onclick={prevPage} disabled={!doc || currentPage <= 1} title="Previous">↑</button>
      <span class="tb-pg">
        <input
          type="number"
          min="1"
          max={doc?.pageCount ?? 1}
          value={currentPage}
          onchange={(e) => jumpTo(parseInt((e.currentTarget as HTMLInputElement).value, 10))}
        />
        <span class="tb-pg-total">/ {doc?.pageCount ?? "—"}</span>
      </span>
      <button class="tb-btn icon" onclick={nextPage} disabled={!doc || currentPage >= (doc?.pageCount ?? 0)} title="Next">↓</button>
    </div>

    <div class="tb-group">
      <button class="tb-btn icon" onclick={zoomOut} disabled={!doc} title="Zoom out (⌘-)">−</button>
      <span class="tb-zoom">{zoomPct}%</span>
      <button class="tb-btn icon" onclick={zoomIn} disabled={!doc} title="Zoom in (⌘+)">+</button>
      <button
        class="tb-btn"
        class:active={zoomLabel === "page-width"}
        disabled={!doc}
        onclick={() => setZoomValue("page-width")}
      >Fit width</button>
      <button
        class="tb-btn"
        class:active={zoomLabel === "page-fit"}
        disabled={!doc}
        onclick={() => setZoomValue("page-fit")}
      >Fit page</button>
    </div>

    <div class="tb-group right">
      <button class="tb-btn" class:active={findOpen} disabled={!doc} onclick={toggleFind} title="Find (⌘F)">🔍 Find</button>
    </div>
  </div>

  {#if findOpen}
    <div class="findbar">
      <input
        class="find-input"
        placeholder="Search in document"
        bind:value={findQuery}
        oninput={() => runFind(findQuery, "find")}
        onkeydown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); if (e.shiftKey) findPrev(); else findNext(); }
          else if (e.key === "Escape") { e.preventDefault(); toggleFind(); }
        }}
      />
      <label class="find-opt">
        <input type="checkbox" bind:checked={findCaseSensitive} onchange={() => runFind(findQuery, "highlightallchange")} />
        Aa
      </label>
      <label class="find-opt">
        <input type="checkbox" bind:checked={findWholeWord} onchange={() => runFind(findQuery, "highlightallchange")} />
        Word
      </label>
      <span class="find-count">
        {findStatus.total > 0
          ? `${findStatus.current} / ${findStatus.total}`
          : (findQuery ? "no matches" : "")}
      </span>
      <button class="tb-btn icon" onclick={findPrev} disabled={!findQuery} title="Previous">↑</button>
      <button class="tb-btn icon" onclick={findNext} disabled={!findQuery} title="Next">↓</button>
    </div>
  {/if}

  <div class="viewer-grid" class:no-thumbs={!thumbsOpen}>
    {#if thumbsOpen && doc}
      <aside class="thumbs">
        {#each Array.from({ length: doc.pageCount }, (_, i) => i + 1) as n (n)}
          <button
            class="thumb"
            class:active={n === currentPage}
            onclick={() => jumpTo(n)}
            use:attachThumbBtn={n}
          >
            <canvas use:attachThumb={n}></canvas>
            <span class="thumb-num">{n}</span>
          </button>
        {/each}
      </aside>
    {/if}

    <div class="pdfjs-container" bind:this={containerEl}>
      <div class="pdfViewer" bind:this={viewerEl}></div>
    </div>
  </div>
</div>

<style>
  .reader-header { margin-bottom: 12px; flex-shrink: 0; }

  .reader-shell {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
  }
  .reader-shell.hidden {
    /* keep the DOM around so the PDFViewer container/element exists even before doc loads;
       but visually hide it. Required because PDFViewer constructor needs a real div. */
    display: none;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tb-group {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px;
    border-right: 1px solid var(--border);
  }
  .tb-group:last-of-type { border-right: none; }
  .tb-group.right { margin-left: auto; border-right: none; }
  .tb-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 5px 10px;
    border-radius: var(--r-sm);
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }
  .tb-btn.icon { padding: 5px 9px; min-width: 28px; font-weight: 600; }
  .tb-btn:hover:not(:disabled) {
    background: var(--bg-3);
    color: var(--text);
  }
  .tb-btn.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .tb-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .tb-pg {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin: 0 4px;
  }
  .tb-pg input {
    width: 48px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 4px 6px;
    border-radius: var(--r-sm);
    font-size: 12px;
    text-align: center;
  }
  .tb-pg-total { font-size: 12px; color: var(--text-3); }
  .tb-zoom {
    font-size: 12px;
    color: var(--text-2);
    width: 42px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .findbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .find-input {
    flex: 1;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 6px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .find-input:focus {
    outline: none;
    border-color: var(--accent);
  }
  .find-opt {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-3);
    cursor: pointer;
  }
  .find-count {
    font-size: 12px;
    color: var(--text-3);
    width: 80px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .viewer-grid {
    display: grid;
    grid-template-columns: 150px 1fr;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .viewer-grid.no-thumbs {
    grid-template-columns: 1fr;
  }

  .thumbs {
    overflow-y: auto;
    background: var(--bg);
    border-right: 1px solid var(--border);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .thumb {
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    padding: 4px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .thumb:hover { background: var(--bg-2); }
  .thumb.active {
    background: var(--bg-2);
    border-color: var(--accent);
  }
  .thumb canvas {
    max-width: 100%;
    background: white;
    border-radius: 2px;
    box-shadow: 0 1px 3px rgba(0,0,0,0.4);
  }
  .thumb-num {
    font-size: 10px;
    color: var(--text-3);
  }
  .thumb.active .thumb-num { color: var(--accent); }

  /* PDFViewer needs its container to be position:relative or absolute */
  .pdfjs-container {
    position: relative;
    overflow: auto;
    background: var(--bg);
  }

  /* Tweak pdf.js viewer chrome */
  :global(.pdfjs-container .pdfViewer .page) {
    margin: 12px auto;
    border: none;
    box-shadow: 0 2px 8px rgba(0,0,0,0.5);
    background-color: white;
  }
  :global(.pdfjs-container .pdfViewer .textLayer ::selection) {
    background: rgba(245, 158, 11, 0.45);
  }
  :global(.pdfjs-container .pdfViewer .textLayer .highlight) {
    background: rgba(245, 158, 11, 0.35);
    border-radius: 2px;
  }
  :global(.pdfjs-container .pdfViewer .textLayer .highlight.selected) {
    background: rgba(245, 158, 11, 0.75);
  }
</style>
