<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { readFile } from "@tauri-apps/plugin-fs";
  import { invoke } from "@tauri-apps/api/core";
  import { basename, formatBytes } from "$lib/types";
  import { isInTauri } from "$lib/tauri";
  import { recordRecent, listRecent, formatRelTime, setRecentThumb, getRecentThumb, type RecentFile } from "$lib/recent";
  import OutlineEditor from "$lib/OutlineEditor.svelte";
  import AnnotateLayer, { type AnnotMode } from "$lib/AnnotateLayer.svelte";
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

  type DocMeta = {
    title?: string;
    author?: string;
    subject?: string;
    keywords?: string;
    creator?: string;
    producer?: string;
    creationDate?: string;
    modDate?: string;
    pdfVersion?: string;
    pageSize?: string;     // e.g. "612 × 792 pt (Letter)"
    fileSize?: number;     // bytes (if known)
    encrypted?: boolean;
  };

  let doc = $state<Doc | null>(null);
  let docMeta = $state<DocMeta | null>(null);
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
  let infoOpen = $state(false);
  let outlineOpen = $state(false);
  let recents = $state<RecentFile[]>(listRecent());

  // Outline tree (TOC) — populated after a PDF loads. Each node may have nested
  // children; `dest` is the pdf.js destination reference, resolved on click to
  // a 1-indexed page number.
  type OutlineNode = {
    title: string;
    dest: unknown;
    items: OutlineNode[];
    expanded: boolean;
  };
  let outline = $state<OutlineNode[]>([]);
  let outlineLoading = $state(false);
  let outlineEditorOpen = $state(false);
  let annotMode = $state<AnnotMode>("off");
  let ocrRunning = $state(false);
  let ocrStatus = $state<string>("");
  let cheatsheetOpen = $state(false);
  let invert = $state(false);
  let dropActive = $state(false);

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
  // isInTauri is imported from $lib/tauri

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

  // ---------- OCR ----------
  async function runOcr() {
    if (!doc || ocrRunning) return;
    const inputName = basename(doc.path).replace(/\.pdf$/i, "");
    const defaultName = `${inputName}-ocr.pdf`;
    const out = await saveDialog({
      title: "Save OCR'd PDF",
      defaultPath: defaultName,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!out) return;
    ocrRunning = true;
    ocrStatus = "Running OCR (this can take a while)…";
    try {
      const report = await invoke<{ pages: number; lang: string; dpi: number }>(
        "slab_ocr",
        {
          input: doc.path,
          output: out,
          opts: { lang: "eng", dpi: 300 },
        }
      );
      ocrStatus = `✓ OCR'd ${report.pages} page${report.pages === 1 ? "" : "s"}`;
      // Load the new file in the reader.
      await loadPath(out);
      // Clear status after a moment.
      setTimeout(() => (ocrStatus = ""), 3000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      ocrStatus = `✗ OCR failed: ${msg}`;
      setTimeout(() => (ocrStatus = ""), 6000);
    } finally {
      ocrRunning = false;
    }
  }

  // ---------- Export annotations to Markdown ----------
  async function exportAnnotsToMd() {
    if (!doc) return;
    const inputName = basename(doc.path).replace(/\.pdf$/i, "");
    const out = await saveDialog({
      title: "Export annotations as Markdown",
      defaultPath: `${inputName}-annotations.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!out) return;
    ocrStatus = "Exporting annotations…";
    try {
      const count = await invoke<number>("slab_export_annotations_md", {
        input: doc.path,
        output: out,
        label: basename(doc.path),
      });
      ocrStatus = count === 0
        ? "✓ Exported (no annotations found)"
        : `✓ Exported ${count} annotation${count === 1 ? "" : "s"}`;
      setTimeout(() => (ocrStatus = ""), 3000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      ocrStatus = `✗ Export failed: ${msg}`;
      setTimeout(() => (ocrStatus = ""), 6000);
    }
  }

  async function loadPath(path: string) {
    loading = true;
    loadError = null;
    try {
      const bytes = await readFile(path);
      await loadBytes(path, new Uint8Array(bytes), bytes.byteLength);
    } catch (e: any) {
      loadError = e?.message || String(e);
      tearDownDoc();
      doc = null;
      loading = false;
    }
  }

  async function loadBytes(path: string, data: Uint8Array, fileSize?: number) {
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

      // Capture metadata for the info panel
      void extractMeta(pdf, fileSize ?? data.byteLength);
      // Capture outline / TOC for the outline sidebar
      void extractOutline(pdf);

      // Record into recent files
      recordRecent({ path, name: basename(path), pageCount: pdf.numPages });
      recents = listRecent();
      // Render a small thumbnail of page 1 for the recents grid
      void renderRecentThumb(pdf, path);
    } catch (e: any) {
      loadError = e?.message || String(e);
      tearDownDoc();
      doc = null;
    } finally {
      loading = false;
    }
  }

  async function extractMeta(pdf: any, fileSize: number) {
    try {
      const { info, metadata } = await pdf.getMetadata();
      // First page determines size for display purposes.
      let pageSize: string | undefined;
      try {
        const first = await pdf.getPage(1);
        const vp = first.getViewport({ scale: 1 });
        const w = Math.round(vp.width);
        const h = Math.round(vp.height);
        pageSize = `${w} × ${h} pt${describePageSize(w, h)}`;
      } catch { /* ignore */ }

      const xmpTitle = metadata?.get?.("dc:title");
      const xmpAuthor = metadata?.get?.("dc:creator");

      docMeta = {
        title: info?.Title || xmpTitle || undefined,
        author: info?.Author || xmpAuthor || undefined,
        subject: info?.Subject || undefined,
        keywords: info?.Keywords || undefined,
        creator: info?.Creator || undefined,
        producer: info?.Producer || undefined,
        creationDate: formatPdfDate(info?.CreationDate),
        modDate: formatPdfDate(info?.ModDate),
        pdfVersion: info?.PDFFormatVersion || undefined,
        pageSize,
        fileSize,
        encrypted: !!info?.IsAcroFormPresent ? undefined : undefined, // placeholder
      };
    } catch {
      docMeta = { fileSize };
    }
  }

  async function extractOutline(pdf: any) {
    outlineLoading = true;
    outline = [];
    try {
      const raw = await pdf.getOutline();
      if (!raw || raw.length === 0) {
        outline = [];
        return;
      }
      const walk = (items: any[], depth: number): OutlineNode[] =>
        items.map((it) => ({
          title: (it?.title || "").trim() || "(untitled)",
          dest: it?.dest,
          items: it?.items?.length ? walk(it.items, depth + 1) : [],
          expanded: depth < 1, // only top level expanded by default
        }));
      outline = walk(raw, 0);
    } catch {
      outline = [];
    } finally {
      outlineLoading = false;
    }
  }

  // Render a 240px-wide JPEG thumbnail of page 1 and persist it. Best-effort —
  // any failure is silently ignored (no thumb just means the recents row shows
  // the placeholder icon).
  async function renderRecentThumb(pdf: any, path: string) {
    try {
      const page = await pdf.getPage(1);
      const baseViewport = page.getViewport({ scale: 1 });
      const targetW = 240;
      const scale = targetW / baseViewport.width;
      const viewport = page.getViewport({ scale });
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      await page.render({ canvasContext: ctx, viewport, canvas }).promise;
      const dataUrl = canvas.toDataURL("image/jpeg", 0.7);
      setRecentThumb(path, dataUrl);
      // Force the recents grid to re-read so the new thumb shows on next load
      recents = listRecent();
    } catch {
      /* ignore — thumbnail is best-effort */
    }
  }

  async function jumpToOutline(node: OutlineNode) {
    if (!pdfDocument || !node.dest) return;
    try {
      let dest = node.dest;
      if (typeof dest === "string") {
        dest = await pdfDocument.getDestination(dest);
      }
      if (!Array.isArray(dest)) return;
      const ref = dest[0];
      const pageIndex = await pdfDocument.getPageIndex(ref);
      jumpTo(pageIndex + 1);
    } catch {
      // Ignore — broken dest, no-op.
    }
  }

  function toggleOutlineNode(node: OutlineNode) {
    node.expanded = !node.expanded;
  }

  function describePageSize(w: number, h: number): string {
    const near = (a: number, b: number) => Math.abs(a - b) <= 4;
    const sizes: [string, number, number][] = [
      ["Letter", 612, 792],
      ["Legal", 612, 1008],
      ["Tabloid", 792, 1224],
      ["A3", 842, 1191],
      ["A4", 595, 842],
      ["A5", 420, 595],
      ["A6", 298, 420],
    ];
    for (const [name, pw, ph] of sizes) {
      if ((near(w, pw) && near(h, ph)) || (near(w, ph) && near(h, pw))) {
        const orientation = w > h ? " landscape" : "";
        return ` (${name}${orientation})`;
      }
    }
    return "";
  }

  // PDF dates look like "D:YYYYMMDDHHmmSS±HH'mm'"
  function formatPdfDate(s: string | undefined): string | undefined {
    if (!s) return undefined;
    const m = s.match(/^D:(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?/);
    if (!m) return s;
    const [, y, mo = "01", d = "01", hh = "00", mm = "00"] = m;
    const dt = new Date(Date.UTC(+y, +mo - 1, +d, +hh, +mm));
    if (isNaN(dt.getTime())) return s;
    return dt.toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
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
    outline = [];
    docMeta = null;
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

  // When the viewer container resizes (info / thumbs sidebar toggled, window
  // resize, etc.) re-apply the current fit-* zoom so the page rescales.
  function rescaleOnResize() {
    if (!pdfViewer) return;
    const v = pdfViewer.currentScaleValue;
    if (v === "page-width" || v === "page-fit" || v === "auto") {
      try {
        pdfViewer.currentScaleValue = v;
      } catch { /* ignore */ }
    }
  }

  $effect(() => {
    // Trigger rescale when either sidebar toggles.
    // (Read state inside the effect so Svelte tracks the deps.)
    void thumbsOpen;
    void infoOpen;
    void outlineOpen;
    queueMicrotask(() => rescaleOnResize());
  });

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
    } else if (e.key === "Escape" && cheatsheetOpen) {
      cheatsheetOpen = false;
    } else if (e.key === "?" && !(e.target as HTMLElement)?.matches("input,textarea")) {
      e.preventDefault();
      cheatsheetOpen = !cheatsheetOpen;
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
    window.addEventListener("slab:open-recent", onOpenRecentEvent as EventListener);

    // Native drag-and-drop on the whole viewer.
    const onDragOver = (e: DragEvent) => {
      if (e.dataTransfer?.types?.includes("Files")) {
        e.preventDefault();
        dropActive = true;
      }
    };
    const onDragLeave = (e: DragEvent) => {
      if (e.target === document.body || (e as any).relatedTarget === null) {
        dropActive = false;
      }
    };
    const onDrop = async (e: DragEvent) => {
      e.preventDefault();
      dropActive = false;
      const file = e.dataTransfer?.files?.[0];
      if (!file) return;
      if (!file.name.toLowerCase().endsWith(".pdf")) {
        ocrStatus = `✗ Not a PDF: ${file.name}`;
        setTimeout(() => (ocrStatus = ""), 3000);
        return;
      }
      const buf = await file.arrayBuffer();
      await loadBytes(file.name, new Uint8Array(buf), file.size);
    };
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("dragleave", onDragLeave);
    window.addEventListener("drop", onDrop);
    // Store handlers so onDestroy can remove them.
    (onMount as any)._slabDnd = { onDragOver, onDragLeave, onDrop };
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("slab:open-recent", onOpenRecentEvent as EventListener);
    const dnd = (onMount as any)._slabDnd;
    if (dnd) {
      window.removeEventListener("dragover", dnd.onDragOver);
      window.removeEventListener("dragleave", dnd.onDragLeave);
      window.removeEventListener("drop", dnd.onDrop);
    }
    tearDownDoc();
  });

  function onOpenRecentEvent(e: CustomEvent<RecentFile>) {
    const file = e.detail;
    if (!file) return;
    if (isInTauri()) {
      void loadPath(file.path);
    } else {
      // Browser dev fallback — try to refetch from /static for the demo path,
      // otherwise tell the user we can't reopen from disk without Tauri.
      void (async () => {
        try {
          const resp = await fetch(file.path);
          if (resp.ok) {
            const buf = await resp.arrayBuffer();
            await loadBytes(file.path, new Uint8Array(buf), buf.byteLength);
            return;
          }
        } catch { /* ignore */ }
        loadError = `Reopening "${file.name}" needs the desktop app. Use Open to pick it again.`;
      })();
    }
  }

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

{#snippet outlineList(nodes: OutlineNode[], depth: number)}
  <ul class="outline-list" class:nested={depth > 0}>
    {#each nodes as node, i (node.title + i + depth)}
      <li class="outline-item">
        <div class="outline-row" style="padding-left: {depth * 12}px">
          {#if node.items.length > 0}
            <button
              class="outline-twist"
              class:open={node.expanded}
              onclick={() => toggleOutlineNode(node)}
              aria-label={node.expanded ? "Collapse" : "Expand"}
            >▸</button>
          {:else}
            <span class="outline-twist spacer" aria-hidden="true"></span>
          {/if}
          <button class="outline-label" onclick={() => jumpToOutline(node)} title={node.title}>
            {node.title}
          </button>
        </div>
        {#if node.expanded && node.items.length > 0}
          {@render outlineList(node.items, depth + 1)}
        {/if}
      </li>
    {/each}
  </ul>
{/snippet}

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
      <span class="dz-hint">Single click, anywhere on your machine. <span class="dz-kbd">⌘K</span> to jump anywhere.</span>
    </button>
    {#if loadError}
      <div class="status err">✕ {loadError}</div>
    {/if}
    {#if recents.length > 0}
      <div class="recent-block">
        <div class="recent-head">
          <span class="recent-label">Recent</span>
        </div>
        <div class="recent-grid">
          {#each recents as r (r.path)}
            {@const thumb = getRecentThumb(r.path)}
            <button class="recent-card" onclick={() => onOpenRecentEvent({ detail: r } as CustomEvent<RecentFile>)} title={r.path}>
              <div class="recent-thumb">
                {#if thumb}
                  <img src={thumb} alt="" loading="lazy" />
                {:else}
                  <span class="recent-thumb-placeholder">PDF</span>
                {/if}
              </div>
              <div class="recent-card-body">
                <span class="recent-card-name">{r.name}</span>
                <span class="recent-card-meta">
                  {#if r.pageCount}{r.pageCount} pages · {/if}{formatRelTime(r.openedAt)}
                </span>
              </div>
            </button>
          {/each}
        </div>
      </div>
    {/if}
  </section>
{/if}

<div class="reader-shell" class:hidden={!doc}>
  <div class="toolbar">
    <div class="tb-group">
      <button class="tb-btn" onclick={pickFile} title="Open another PDF">⊕ Open</button>
    </div>

    <div class="tb-group">
      <button class="tb-btn icon" class:active={thumbsOpen} onclick={() => (thumbsOpen = !thumbsOpen)} title="Toggle thumbnails">▦</button>
      <button class="tb-btn icon" class:active={outlineOpen} disabled={!doc} onclick={() => (outlineOpen = !outlineOpen)} title={outline.length === 0 ? "No outline in this PDF — open to add one" : "Toggle outline"}>☰</button>
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

    <div class="tb-group">
      <button
        class="tb-btn"
        class:active={annotMode === "highlight"}
        disabled={!doc}
        onclick={() => (annotMode = annotMode === "highlight" ? "off" : "highlight")}
        title="Highlight text (select to highlight)"
      >🖍 Highlight</button>
      <button
        class="tb-btn"
        class:active={annotMode === "note"}
        disabled={!doc}
        onclick={() => (annotMode = annotMode === "note" ? "off" : "note")}
        title="Add sticky note (click on page)"
      >📝 Note</button>
      <button
        class="tb-btn"
        class:active={ocrRunning}
        disabled={!doc || ocrRunning}
        onclick={runOcr}
        title="Make scanned PDF searchable (Tesseract)"
      >{ocrRunning ? "⏳ OCR…" : "👁 OCR"}</button>
      <button
        class="tb-btn"
        disabled={!doc}
        onclick={exportAnnotsToMd}
        title="Export highlights and notes to Markdown"
      >📤 Export</button>
    </div>

    <div class="tb-group right">
      <button
        class="tb-btn"
        class:active={invert}
        disabled={!doc}
        onclick={() => (invert = !invert)}
        title="Toggle dark mode invert (whites→darks)"
      >🌙 Invert</button>
      <button class="tb-btn" class:active={findOpen} disabled={!doc} onclick={toggleFind} title="Find (⌘F)">🔍 Find</button>
      <button class="tb-btn" class:active={infoOpen} disabled={!doc} onclick={() => (infoOpen = !infoOpen)} title="Document info">ⓘ Info</button>
      <button class="tb-btn" onclick={() => (cheatsheetOpen = true)} title="Keyboard shortcuts (?)">?</button>
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

  <div class="viewer-grid" class:no-thumbs={!thumbsOpen && !outlineOpen} class:with-info={infoOpen}>
    {#if outlineOpen && doc}
      <aside class="outline-sidebar">
        <div class="outline-head">
          <span class="outline-title-label">Outline</span>
          <button class="outline-edit" onclick={() => (outlineEditorOpen = true)} title="Edit outline">✎</button>
          <button class="outline-close" onclick={() => (outlineOpen = false)} title="Close">×</button>
        </div>
        {#if outlineLoading}
          <div class="outline-empty">Loading…</div>
        {:else if outline.length === 0}
          <div class="outline-empty">
            <p>No outline in this PDF.</p>
            <button class="outline-add-btn" onclick={() => (outlineEditorOpen = true)}>+ Create outline</button>
          </div>
        {:else}
          <nav class="outline-tree">
            {@render outlineList(outline, 0)}
          </nav>
        {/if}
      </aside>
    {:else if thumbsOpen && doc}
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

    <div class="pdfjs-container" class:invert bind:this={containerEl}>
      <div class="pdfViewer" bind:this={viewerEl}></div>
    </div>

    {#if infoOpen && doc}
      <aside class="info-sidebar">
        <div class="info-head">
          <span class="info-title-label">Document info</span>
          <button class="info-close" onclick={() => (infoOpen = false)} title="Close">×</button>
        </div>
        <dl class="info-grid">
          <dt>File</dt>
          <dd class="info-mono">{basename(doc.path)}</dd>

          {#if docMeta?.title}
            <dt>Title</dt>
            <dd>{docMeta.title}</dd>
          {/if}
          {#if docMeta?.author}
            <dt>Author</dt>
            <dd>{docMeta.author}</dd>
          {/if}
          {#if docMeta?.subject}
            <dt>Subject</dt>
            <dd>{docMeta.subject}</dd>
          {/if}
          {#if docMeta?.keywords}
            <dt>Keywords</dt>
            <dd>{docMeta.keywords}</dd>
          {/if}

          <dt>Pages</dt>
          <dd>{doc.pageCount}</dd>

          {#if docMeta?.pageSize}
            <dt>Page size</dt>
            <dd>{docMeta.pageSize}</dd>
          {/if}
          {#if docMeta?.fileSize !== undefined}
            <dt>File size</dt>
            <dd>{formatBytes(docMeta.fileSize)}</dd>
          {/if}
          {#if docMeta?.pdfVersion}
            <dt>PDF version</dt>
            <dd>{docMeta.pdfVersion}</dd>
          {/if}

          {#if docMeta?.creator}
            <dt>Creator</dt>
            <dd>{docMeta.creator}</dd>
          {/if}
          {#if docMeta?.producer}
            <dt>Producer</dt>
            <dd>{docMeta.producer}</dd>
          {/if}
          {#if docMeta?.creationDate}
            <dt>Created</dt>
            <dd>{docMeta.creationDate}</dd>
          {/if}
          {#if docMeta?.modDate}
            <dt>Modified</dt>
            <dd>{docMeta.modDate}</dd>
          {/if}
        </dl>
        <div class="info-foot">
          <span class="info-foot-hint">Read straight from the PDF metadata. Stays on your machine.</span>
        </div>
      </aside>
    {/if}

    {#if annotMode !== "off" && doc}
      <aside class="annot-sidebar">
        <AnnotateLayer
          path={doc.path}
          viewer={pdfViewer}
          viewerEl={containerEl ?? null}
          mode={annotMode}
          onsaved={(p) => { annotMode = "off"; loadPath(p); }}
          onmodechange={(m) => (annotMode = m)}
        />
      </aside>
    {/if}

    {#if ocrStatus}
      <div class="ocr-toast" class:err={ocrStatus.startsWith("✗")}>{ocrStatus}</div>
    {/if}

    {#if dropActive}
      <div class="drop-overlay">
        <div class="drop-inner">
          <div class="drop-icon">📄</div>
          <div class="drop-text">Drop PDF to open</div>
        </div>
      </div>
    {/if}

    {#if cheatsheetOpen}
      <button class="cheatsheet-backdrop" aria-label="Close shortcuts"
        onclick={() => (cheatsheetOpen = false)}></button>
      <div class="cheatsheet" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
        <header>
          <h3>Keyboard shortcuts</h3>
          <button class="cs-close" onclick={() => (cheatsheetOpen = false)} title="Close (Esc)">✕</button>
        </header>
        <div class="cs-grid">
          <div class="cs-row"><kbd>⌘F</kbd><span>Find in document</span></div>
          <div class="cs-row"><kbd>⌘+</kbd><kbd>⌘-</kbd><span>Zoom in / out</span></div>
          <div class="cs-row"><kbd>→</kbd><kbd>PgDn</kbd><span>Next page</span></div>
          <div class="cs-row"><kbd>←</kbd><kbd>PgUp</kbd><span>Previous page</span></div>
          <div class="cs-row"><kbd>Esc</kbd><span>Close find / cheatsheet</span></div>
          <div class="cs-row"><kbd>?</kbd><span>Toggle this cheatsheet</span></div>
          <div class="cs-row"><kbd>Drag</kbd><span>Drop any PDF on the window to open</span></div>
          <div class="cs-row"><kbd>🖍</kbd><span>Highlight (select text first)</span></div>
          <div class="cs-row"><kbd>📝</kbd><span>Sticky note (click on page)</span></div>
          <div class="cs-row"><kbd>👁</kbd><span>OCR scanned PDF (Tesseract)</span></div>
          <div class="cs-row"><kbd>🌙</kbd><span>Invert colors (dark-mode reading)</span></div>
          <div class="cs-row"><kbd>📤</kbd><span>Export annotations as Markdown</span></div>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if outlineEditorOpen && doc}
  <OutlineEditor
    path={doc.path}
    pageCount={doc.pageCount}
    onclose={() => (outlineEditorOpen = false)}
    onsaved={(savedPath) => {
      outlineEditorOpen = false;
      // Reload from the saved path so the in-app outline reflects the edit
      // (whether the user overwrote the original or used Save As).
      void loadPath(savedPath);
    }}
  />
{/if}

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
    position: relative;
  }
  .viewer-grid.no-thumbs {
    grid-template-columns: 1fr;
  }
  .viewer-grid.with-info {
    grid-template-columns: 150px 1fr 280px;
  }
  .viewer-grid.with-info.no-thumbs {
    grid-template-columns: 1fr 280px;
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

  /* ---- Recent files block (empty state) ---- */
  .dz-kbd {
    display: inline-block;
    margin-left: 6px;
    padding: 1px 5px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-2);
    font-size: 10px;
    letter-spacing: 0.5px;
  }
  .recent-block {
    margin-top: 18px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
  }
  .recent-head {
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .recent-label {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.6px;
  }
  .recent-list {
    display: flex;
    flex-direction: column;
  }
  .recent-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 14px;
    padding: 16px;
  }
  .recent-card {
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 0;
    color: var(--text-2);
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition: border-color 120ms, transform 120ms, box-shadow 120ms;
  }
  .recent-card:hover {
    border-color: var(--accent);
    color: var(--text);
    transform: translateY(-1px);
    box-shadow: 0 6px 18px -10px var(--accent);
  }
  .recent-thumb {
    position: relative;
    aspect-ratio: 8.5 / 11;
    background: var(--bg-3);
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .recent-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .recent-thumb-placeholder {
    color: var(--text-3);
    font-size: 22px;
    letter-spacing: 2px;
    font-weight: 700;
  }
  .recent-card-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px 12px;
    min-width: 0;
  }
  .recent-card-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-card-meta {
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .recent-row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 14px;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    text-align: left;
    cursor: pointer;
  }
  .recent-row:last-child { border-bottom: none; }
  .recent-row:hover { background: var(--bg-3); color: var(--text); }
  .recent-icon {
    color: var(--accent);
    width: 18px;
    text-align: center;
  }
  .recent-name {
    flex: 1;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-meta {
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
  }

  /* ---- Outline sidebar ---- */
  .outline-sidebar {
    background: var(--bg);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }
  .outline-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .outline-title-label {
    font-size: 11px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.5px;
    font-weight: 600;
  }
  .outline-close {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-3);
    font-size: 16px;
    line-height: 1;
    padding: 0 6px;
    cursor: pointer;
    border-radius: 4px;
  }
  .outline-edit {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-3);
    font-size: 14px;
    line-height: 1;
    padding: 0 6px;
    cursor: pointer;
    border-radius: 4px;
    margin-left: auto;
  }
  .outline-edit:hover { color: var(--text); background: var(--bg-2); }
  .outline-add-btn {
    margin-top: 8px;
    background: var(--bg-2, #1a1a1a);
    color: var(--text, #eee);
    border: 1px solid var(--border, #2a2a2a);
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  .outline-add-btn:hover { background: var(--bg-1, #222); }
  .outline-close:hover { color: var(--text); background: var(--bg-2); }
  .outline-empty {
    color: var(--text-3);
    font-size: 12px;
    padding: 14px 12px;
    font-style: italic;
  }
  .outline-tree {
    overflow-y: auto;
    padding: 6px 6px 14px;
    flex: 1;
  }
  .outline-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .outline-list.nested {
    margin: 0;
  }
  .outline-item { margin: 0; }
  .outline-row {
    display: flex;
    align-items: center;
    gap: 4px;
    border-radius: 6px;
  }
  .outline-row:hover { background: var(--bg-2); }
  .outline-twist {
    background: transparent;
    border: none;
    color: var(--text-3);
    font-size: 9px;
    width: 18px;
    height: 24px;
    padding: 0;
    cursor: pointer;
    transition: transform 120ms ease;
    flex-shrink: 0;
  }
  .outline-twist.open {
    transform: rotate(90deg);
    color: var(--text-2);
  }
  .outline-twist.spacer {
    pointer-events: none;
    cursor: default;
  }
  .outline-label {
    flex: 1;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    font-size: 12.5px;
    padding: 4px 8px 4px 2px;
    border-radius: 4px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .outline-label:hover { color: var(--text); }

  /* ---- Annotation sidebar ---- */
  .annot-sidebar {
    position: absolute;
    top: 56px;
    right: 12px;
    z-index: 30;
    /* The AnnotateLayer component carries its own background + border. */
  }

  /* ---- OCR toast ---- */
  .ocr-toast {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 40;
    padding: 10px 16px;
    border-radius: 8px;
    background: var(--bg-elev, #1c1c20);
    color: var(--fg, #fff);
    border: 1px solid var(--border, #2a2a30);
    font-size: 13px;
    font-weight: 500;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    pointer-events: none;
    max-width: 80%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ocr-toast.err {
    border-color: #c14545;
    color: #ffb3b3;
  }

  /* ---- Invert (dark-mode reading) ---- */
  .pdfjs-container.invert :global(.page),
  .pdfjs-container.invert :global(canvas) {
    filter: invert(1) hue-rotate(180deg) brightness(0.95) contrast(0.95);
  }

  /* ---- Drag-and-drop overlay ---- */
  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 50;
    background: rgba(0, 122, 255, 0.08);
    border: 3px dashed #007aff;
    border-radius: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    backdrop-filter: blur(2px);
  }
  .drop-inner {
    background: rgba(0, 0, 0, 0.75);
    color: #fff;
    padding: 24px 40px;
    border-radius: 12px;
    text-align: center;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }
  .drop-icon { font-size: 48px; line-height: 1; margin-bottom: 8px; }
  .drop-text { font-size: 16px; font-weight: 600; letter-spacing: 0.02em; }

  /* ---- Cheatsheet modal ---- */
  .cheatsheet-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
    z-index: 100;
    border: 0;
    padding: 0;
    cursor: default;
  }
  .cheatsheet {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 101;
    width: min(480px, 90vw);
    background: var(--bg-elev, #1c1c20);
    color: var(--fg, #fff);
    border: 1px solid var(--border, #2a2a30);
    border-radius: 12px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.6);
    padding: 20px 24px;
  }
  .cheatsheet header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
  }
  .cheatsheet h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .cs-close {
    background: transparent;
    color: var(--fg-mute, #888);
    border: 0;
    font-size: 18px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
  }
  .cs-close:hover { background: var(--bg-hover, #2a2a30); color: var(--fg, #fff); }
  .cs-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
  }
  .cs-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .cs-row kbd {
    background: var(--bg, #0e0e10);
    border: 1px solid var(--border, #2a2a30);
    border-radius: 4px;
    padding: 2px 8px;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 11px;
    min-width: 28px;
    text-align: center;
  }
  .cs-row span { color: var(--fg-mute, #aaa); margin-left: 4px; }

  /* ---- Info sidebar ---- */
  .info-sidebar {
    background: var(--bg);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .info-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .info-title-label {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.6px;
  }
  .info-close {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-3);
    border-radius: 4px;
    padding: 1px 7px;
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
  }
  .info-close:hover { color: var(--text); background: var(--bg-3); }
  .info-grid {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 6px 10px;
    padding: 12px;
    margin: 0;
    flex: 1;
  }
  .info-grid dt {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.5px;
    padding-top: 2px;
  }
  .info-grid dd {
    font-size: 12px;
    color: var(--text);
    margin: 0;
    word-break: break-word;
  }
  .info-mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
  }
  .info-foot {
    padding: 10px 12px;
    border-top: 1px solid var(--border);
  }
  .info-foot-hint {
    font-size: 10px;
    color: var(--text-3);
    line-height: 1.4;
  }
</style>
