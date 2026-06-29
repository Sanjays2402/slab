<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { readFile, writeFile } from "@tauri-apps/plugin-fs";
  import { invoke } from "@tauri-apps/api/core";
  import { join, tempDir } from "@tauri-apps/api/path";
  import { basename, formatBytes, stripExt } from "$lib/types";
  import { isInTauri } from "$lib/tauri";
  import { recordRecent, recordRecentProgress, getRecentProgress, listRecent, formatRelTime, setRecentThumb, getRecentThumb, pinRecent, removeRecent, type RecentFile } from "$lib/recent";
  import RecentsHome from "$lib/components/RecentsHome.svelte";
  import { clampFlyoutTop, shouldShowPreview, previewLabel, classifyThumbPreviewKey, nextPreviewPage } from "$lib/readerThumbView";
  import { filterOutlineTree, describeOutlineFilter, countOutlineNodes, type FilteredOutlineNode } from "$lib/readerOutlineView";
  import { notify } from "$lib/notify";
  import { pluginsStore, runPluginPdfAction, type ActivePdfAction } from "$lib/plugins";
  import OutlineEditor from "$lib/OutlineEditor.svelte";
  import AnnotateLayer, { type AnnotMode } from "$lib/AnnotateLayer.svelte";
  import DecryptModal from "$lib/components/DecryptModal.svelte";
  import BeaconSelectionBubble from "$lib/components/BeaconSelectionBubble.svelte";
  import { slabScanAudit, nonEmptyPages, type ScanAuditReport } from "$lib/lens";
  import { analyzeSlides, type SlideReport } from "$lib/slides";
  import PresenterOverlay from "$lib/components/PresenterOverlay.svelte";
  import {
    interpretFindState,
    idleFindStatus,
    describeFindStatus,
    findStatusTone,
    announceFindStatus,
    buildFindDispatch,
    defaultFindOptions,
    toggleFindOption,
    describeFindOptions,
    FIND_OPTION_TOGGLES,
    pushFindHistory,
    suggestFindHistory,
    suggestionSegments,
    classifyFindGlobalKey,
    classifyFindDropdownKey,
    type FindStatus,
    type FindOptions,
    type FindSuggestion,
  } from "$lib/readerFindView";
  import { classifyPaletteNav, nextPaletteIndex } from "$lib/paletteSearch";
  // @ts-expect-error - pdfjs-dist .mjs has no types index alias
  import * as pdfjsLib from "pdfjs-dist/build/pdf.mjs";
  import { EventBus, PDFFindController, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
  import "pdfjs-dist/web/pdf_viewer.css";
  import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";

  pdfjsLib.GlobalWorkerOptions.workerSrc = workerSrc;

  // ---------- Multi-tab props (Lathe Slice 5) ----------
  // The shell mounts one ReaderPanel per open PDF tab. Each instance owns its
  // own pdfjs viewer, so the props below just gate global event handlers so
  // only the visible tab reacts to keyboard / drag-drop / open-recent events.
  type ReaderProps = {
    /** Stable id of this tab. Lets event listeners ignore other tabs. */
    tabId?: string;
    /** True when this tab is the visible one. Other tabs short-circuit handlers. */
    active?: boolean;
    /** Path to load on mount (or when the shell swaps it). Optional. */
    initialPath?: string | null;
    /** Atlas v2.2.0 — page to jump to (1-based) once the doc is loaded.
     *  Used by LibrarySearchPanel: clicking a hit opens the doc + jumps
     *  to the matching page. Consumed exactly once on initial load.
     *  Subsequent jumps come through the `slab:reader-jump` window event. */
    initialPage?: number | null;
    /** Atlas v2.2.0 — query string to highlight across the document
     *  after load. Triggers pdfjs PDFFindController with highlightAll=true
     *  so every occurrence is visible at once. */
    initialHighlight?: string | null;
    /** Callback fired after a successful load so the shell can update the tab title. */
    onTitleChange?: (title: string) => void;
  };
  let {
    tabId = "primary",
    active = true,
    initialPath = null,
    initialPage = null,
    initialHighlight = null,
    onTitleChange,
  }: ReaderProps = $props();

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

  // Atlas Lite: debounced reading-progress emitter. We persist the user's
  // last-viewed page so re-opening jumps straight back there.
  let progressTimer: ReturnType<typeof setTimeout> | null = null;
  let lastSavedProgress: { path: string; page: number } | null = null;
  function scheduleProgressSave(path: string, page: number, total: number) {
    // Skip duplicate writes (e.g. user lands on page 5, scrolls within page).
    if (lastSavedProgress && lastSavedProgress.path === path && lastSavedProgress.page === page) {
      return;
    }
    if (progressTimer) clearTimeout(progressTimer);
    progressTimer = setTimeout(() => {
      try {
        recordRecentProgress(path, { lastPage: page, totalPages: total });
        lastSavedProgress = { path, page };
      } catch { /* localStorage off — silently skip */ }
      progressTimer = null;
    }, 800);
  }
  function flushProgressSave() {
    if (progressTimer) {
      clearTimeout(progressTimer);
      progressTimer = null;
      // Force-write whatever the current state is so closing the doc
      // doesn't lose the last 800ms of scrolling.
      if (doc && currentPage >= 1) {
        try {
          recordRecentProgress(doc.path, { lastPage: currentPage, totalPages: doc.pageCount });
        } catch { /* ignore */ }
      }
    }
  }

  // Atlas v2.2.0 — runtime page-jump + highlight state.
  //
  // `pendingJump` queues a (page, highlight) directive that arrives via
  // the `slab:reader-jump` event *before* the doc has loaded (e.g. user
  // clicked a search result, we're still fetching bytes). It's consumed
  // and cleared the moment loadBytes finishes.
  //
  // `jumpHalo` powers the WOW: a 720ms gold cubic-bezier pulse on the
  // jumped-to page, drawing the eye to the find-highlighted matches.
  let pendingJump: { page: number | null; highlight: string | null } | null = $state(null);
  let jumpHalo = $state(false);
  let zoomLabel = $state("page-width"); // sync with PDFViewer.currentScaleValue
  let zoomPct = $state(100);
  let findOpen = $state(false);
  let findQuery = $state("");
  let findStatus = $state<FindStatus>(idleFindStatus());
  let findOptions = $state<FindOptions>(defaultFindOptions());
  // Atlas IV: recent-search MRU ring (persisted) + live suggestion dropdown.
  let findHistory = $state<string[]>(loadFindHistory());
  let findSuggestOpen = $state(false);
  let findSuggestCursor = $state(-1);
  // Atlas IV: aria-live announcement, debounced by equality so the same
  // phrase isn't re-read as pdf.js fires repeated progress events.
  let findAnnounce = $state("");
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
  // Round 59: filter-as-you-type over the outline tree. A long PDF can carry
  // 100+ nested headings; outlineFilter drives the pure filterOutlineTree
  // (keep a branch if it OR a descendant matches, ancestors force-expanded,
  // matched span <mark>-highlighted). A null result = no filter active, so
  // the normal expand/collapse tree renders unchanged.
  let outlineFilter = $state("");
  const filteredOutline = $derived(filterOutlineTree(outline, outlineFilter));
  const outlineNodeCount = $derived(countOutlineNodes(outline));
  let annotMode = $state<AnnotMode>("off");
  let ocrRunning = $state(false);
  let ocrStatus = $state<string>("");
  // Scan-audit (v0.13.0 Lens Slice 1): when a PDF opens we audit it for
  // scanned pages and surface a one-click OCR banner if needed.
  let scanReport = $state<ScanAuditReport | null>(null);
  let scanAuditing = $state(false);
  let scanBannerDismissed = $state(false);
  // Slide-deck audit (v0.15.0 Theater Slice 4): runs in the background
  // after open; if it concludes the doc looks like a slide deck we
  // surface a one-click "Present" banner.
  let slideReport = $state<SlideReport | null>(null);
  let slideBannerDismissed = $state(false);
  let presenting = $state(false);
  let cheatsheetOpen = $state(false);
  let invert = $state(false);
  let dropActive = $state(false);

  // Foundry Slice 9 — plugin-contributed PDF actions. Live snapshot of
  // the active list + the open/close flag for the toolbar dropdown.
  let pluginActions = $state<ActivePdfAction[]>([]);
  let pluginActionsOpen = $state(false);
  const unsubPluginActions = pluginsStore.subscribe((s) => (pluginActions = s.pdfActions));

  // Locked-PDF flow — when pdf.js refuses to open an encrypted file we
  // surface a DecryptModal. `decryptPending` holds the original (encrypted)
  // path so the modal can call slab_decrypt and reopen the plaintext copy.
  let decryptPending = $state<string | null>(null);

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
  let pdfViewer: any = $state(null);
  let thumbsAbortController: AbortController | null = null;

  // ---------- File loading ----------
  // isInTauri is imported from $lib/tauri

  // Extensions accepted by the v0.8.1 polyglot bridge — must stay in sync
  // with `pdf::polyglot::supported_extension` in the Rust crate. PDF is
  // intentionally absent from here (handled directly, not via markitdown).
  const POLYGLOT_EXTS = [
    "docx", "pptx", "xlsx", "xls",
    "html", "htm", "epub",
    "csv", "json", "xml", "rtf", "odt",
    "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp",
    "wav", "mp3", "m4a", "flac", "ogg",
  ];
  const POLYGLOT_LABEL =
    "Documents (PDF, Office, HTML, EPUB, CSV, JSON, XML, RTF, ODT)";

  function extOf(path: string): string {
    const i = path.lastIndexOf(".");
    return i >= 0 ? path.slice(i + 1).toLowerCase() : "";
  }

  function isPdfPath(path: string): boolean {
    return extOf(path) === "pdf";
  }

  function isPolyglotPath(path: string): boolean {
    return POLYGLOT_EXTS.includes(extOf(path));
  }

  /// Compute a stable temp path for the converted PDF. Uses Tauri's
  /// `tempDir()` so we land in OS-temp without poking user folders, and
  /// includes a millisecond timestamp so re-opening the same source
  /// doesn't clobber a previous (possibly still-open) PDF.
  async function polyglotTmpOutput(input: string): Promise<string> {
    const base = stripExt(basename(input)) || "slab-polyglot";
    const stamp = Date.now().toString(36);
    const safe = base.replace(/[^A-Za-z0-9._-]/g, "_");
    const dir = await tempDir();
    return await join(dir, `slab-polyglot-${safe}-${stamp}.pdf`);
  }

  /// Convert a markitdown-error string into something a human can act on.
  /// Mirrors how `runOcr` surfaces missing-binary failures, keyed on the
  /// canonical phrases produced by `pdf::polyglot::require_markitdown`.
  function friendlyPolyglotError(raw: string): string {
    if (raw.includes("markitdown not found")) {
      return (
        "markitdown isn’t installed. Run " +
        "`pipx install 'markitdown[all]'` and try again."
      );
    }
    if (raw.includes("unsupported polyglot input")) {
      return "This file type isn’t supported yet.";
    }
    if (raw.includes("empty document")) {
      return "markitdown couldn’t extract any text from this file.";
    }
    return raw;
  }

  /// Branch on extension: PDFs open directly, polyglot inputs are
  /// converted via the `slab_polyglot` Tauri command and then opened.
  async function openAny(path: string) {
    if (isPdfPath(path)) {
      await loadPath(path);
      return;
    }
    if (!isPolyglotPath(path)) {
      loadError = `Unsupported file type: ${basename(path)}`;
      return;
    }
    loading = true;
    loadError = null;
    ocrStatus = `Converting ${basename(path)}…`;
    try {
      const out = await polyglotTmpOutput(path);
      await invoke("slab_polyglot", {
        input: path,
        output: out,
        opts: { page_size: "A4" },
      });
      ocrStatus = `✓ Converted ${basename(path)}`;
      setTimeout(() => (ocrStatus = ""), 3000);
      await loadPath(out);
    } catch (e) {
      const raw = e instanceof Error ? e.message : String(e);
      loadError = friendlyPolyglotError(raw);
      ocrStatus = "";
    } finally {
      loading = false;
    }
  }

  async function pickFile() {
    if (isInTauri()) {
      const picked = await open({
        multiple: false,
        filters: [
          { name: POLYGLOT_LABEL, extensions: ["pdf", ...POLYGLOT_EXTS] },
          { name: "PDF only", extensions: ["pdf"] },
        ],
      });
      if (typeof picked !== "string") return;
      await openAny(picked);
    } else {
      // Browser dev fallback — uses the native file input. Note: the
      // browser fallback only handles PDF (no Tauri = no `slab_polyglot`).
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

  /// Audit the freshly-opened PDF for scanned pages. Runs in the background,
  /// silent on failure. Reused by the OCR banner — when it returns an
  /// `OcrAll` recommendation we surface the prompt automatically.
  async function runScanAudit(path: string) {
    scanReport = null;
    scanBannerDismissed = false;
    if (!isInTauri()) return;
    scanAuditing = true;
    try {
      const r = await slabScanAudit(path);
      scanReport = r;
    } catch {
      // Audit failures are non-fatal — the user can still OCR manually.
      scanReport = null;
    } finally {
      scanAuditing = false;
    }
  }

  /// Slide-deck audit (v0.15.0 Theater Slice 4): silently probe the doc.
  /// If the heuristic says it's a deck we surface a Present banner.
  async function runSlideAudit(path: string) {
    slideReport = null;
    slideBannerDismissed = false;
    if (!isInTauri()) return;
    try {
      const r = await analyzeSlides(path);
      slideReport = r;
    } catch {
      slideReport = null;
    }
  }

  function dismissSlideBanner() {
    slideBannerDismissed = true;
  }

  function startPresenting() {
    if (!doc) return;
    presenting = true;
  }

  function stopPresenting() {
    presenting = false;
  }

  function dismissScanBanner() {
    scanBannerDismissed = true;
  }

  // Recommendation surfaced as a banner. Returns null when nothing to show.
  let scanBannerText = $derived.by<string | null>(() => {
    if (!scanReport || scanBannerDismissed) return null;
    const r = scanReport.recommended_action;
    if (r === "none") return null;
    const total = nonEmptyPages(scanReport);
    if (r === "ocr_all") {
      return `This PDF looks fully scanned (${total} page${total === 1 ? "" : "s"}). Run OCR to make it searchable?`;
    }
    // ocr_some
    const scanned = scanReport.image_pages + scanReport.mixed_pages;
    return `${scanned} of ${total} page${total === 1 ? "" : "s"} look scanned. Run OCR to make text selectable?`;
  });

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
      // Notify the shell so it can update the tab title.
      onTitleChange?.(basename(path));
      // Atlas Lite: if this file has a saved reading position, resume there
      // (but not if the shell asked for a specific page via initialPage /
      // pendingJump — explicit navigation always wins over implicit resume).
      const savedProgress = getRecentProgress(path);
      const hasExplicitJump = !!pendingJump || !!initialPage;
      if (
        !hasExplicitJump &&
        savedProgress &&
        savedProgress.lastPage > 1 &&
        savedProgress.lastPage <= pdf.numPages
      ) {
        const resumePage = savedProgress.lastPage;
        // Wait one microtask for pdfViewer to attach.
        queueMicrotask(() => {
          try {
            pdfViewer.currentPageNumber = resumePage;
            currentPage = resumePage;
          } catch { /* viewer not ready — pagesinit will recover */ }
        });
        notify.info(`Resumed at page ${resumePage} of ${pdf.numPages}`, { duration: 4000 });
      }
      // Kick off a scan-audit in the background — non-blocking. Only for
      // real file-system PDFs (skip data: URLs etc — `path` is a real path
      // by contract here).
      void runScanAudit(path);
      // Slide-deck audit (Theater Slice 4) — same fire-and-forget pattern.
      void runSlideAudit(path);
      // Workshop v2.0.0 Slice 6.8: notify any enabled plugins with a
      // runtime that this document just opened. Fire-and-forget —
      // dispatch is best-effort and asynchronous on the Rust side.
      // `lastPluginPath` tracks the most recent path we announced an
      // open for so a subsequent load/teardown in this panel can pair
      // it with a `_closed` event. We capture path-at-call-time to
      // avoid losing the value across reactive updates.
      void notifyPluginsDocumentOpened(path);
      // Atlas v2.2.0 — once the doc is loaded, honour any pending jump
      // (page + highlight) staged by the shell. We schedule on next tick
      // so pdfViewer has a chance to render the first page first.
      if (pendingJump) {
        const j = pendingJump;
        pendingJump = null;
        queueMicrotask(() => applyJump(j.page, j.highlight));
      } else if (initialPage || initialHighlight) {
        // First-time consume of mount-time hints from the shell.
        const p = initialPage;
        const q = initialHighlight;
        initialPage = null;
        initialHighlight = null;
        queueMicrotask(() => applyJump(p, q));
      }
    } catch (e: any) {
      // pdf.js raises a `PasswordException` for both `NEED_PASSWORD` (no
      // password supplied) and `INCORRECT_PASSWORD`. We can't supply one
      // through pdf.js V1/RC4 documents directly without bridging the
      // password callback API, so we delegate to the Rust side: pop the
      // DecryptModal, run `slab_decrypt`, then reopen the plaintext copy.
      if (e?.name === "PasswordException") {
        loadError = null;
        tearDownDoc();
        doc = null;
        decryptPending = path;
        return;
      }
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
    // Atlas Lite: persist any pending progress before we lose the doc handle.
    flushProgressSave();
    // Workshop v2.0.0 Slice 6.8: fire `slab.document.onClose` for the
    // previously-open doc (if any) before we tear down its state.
    // Must run BEFORE we null out `doc` so we can read the path; plugin
    // handlers observe `getActive() === null` because the actor's
    // event loop clears `active_doc` before dispatching onClose (see
    // `run_actor` in src-tauri/src/plugins/runtime/actor.rs).
    if (lastPluginPath) {
      void notifyPluginsDocumentClosed(lastPluginPath);
      lastPluginPath = null;
    }
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

  // ---------- Workshop v2.0.0 Slice 6.8: plugin document lifecycle ----------
  //
  // Two tiny helpers around the Tauri commands `slab_plugins_document_opened`
  // and `slab_plugins_document_closed`. Both are fire-and-forget: failures
  // are swallowed (a missing Tauri context, e.g. SSR or unit test, must
  // not break the viewer) and never block the load path.
  //
  // We track the last path we announced via `lastPluginPath` so the close
  // event always pairs with a real open: tearDownDoc fires close BEFORE
  // loadBytes fires open for the next doc, matching the pattern
  // "every open has a matching close eventually".

  let lastPluginPath: string | null = null;

  async function notifyPluginsDocumentOpened(path: string): Promise<void> {
    lastPluginPath = path;
    if (!isInTauri()) return;
    try {
      await invoke("slab_plugins_document_opened", { path });
    } catch (e) {
      // Plugin lifecycle dispatch is best-effort. Log to console for
      // diagnostics but never surface to the user — a buggy plugin
      // shouldn't break the viewer.
      console.debug("[plugins] document_opened dispatch failed:", e);
    }
  }

  async function notifyPluginsDocumentClosed(path: string): Promise<void> {
    if (!isInTauri()) return;
    try {
      await invoke("slab_plugins_document_closed", { path });
    } catch (e) {
      console.debug("[plugins] document_closed dispatch failed:", e);
    }
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
      // Atlas Lite: debounced save to the recent-files store so the next
      // reopen jumps straight to where the user was. We pick 800ms — long
      // enough to ignore rapid scroll-bursts, short enough that closing
      // the tab while still scrolling still records something useful.
      if (doc) scheduleProgressSave(doc.path, e.pageNumber, doc.pageCount);
    });
    eventBus.on("scalechanging", () => {
      syncZoom();
    });
    eventBus.on("updatefindcontrolstate", (s: any) => {
      applyFindEvent(s);
    });
    eventBus.on("updatefindmatchescount", (s: any) => {
      applyFindEvent(s);
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

  // Atlas v2.2.0 — apply a (page, highlight) jump.
  //
  // - jumps to the page (1-based clamp)
  // - runs the find controller with highlightAll so every match glows
  // - flashes a 720 ms gold halo around the jumped-to page (WOW)
  //
  // Reduced-motion users get an instant non-pulsing accent ring instead
  // (CSS picks up `prefers-reduced-motion` automatically).
  function applyJump(page: number | null, highlight: string | null) {
    if (page && page > 0) {
      jumpTo(page);
    }
    if (highlight && highlight.trim()) {
      findQuery = highlight;
      // Don't pop the find bar by default — the highlights are enough.
      // If the user wants to keep navigating matches they can hit Cmd+F.
      runFind(highlight);
    }
    // Trigger the halo on the next frame so the scroll has time to land.
    jumpHalo = false;
    requestAnimationFrame(() => {
      jumpHalo = true;
      // Auto-clear after the animation finishes so a repeat jump replays.
      window.setTimeout(() => { jumpHalo = false; }, 900);
    });
  }

  /** Atlas v2.2.0 — window-level entry point for cross-tab jump requests.
   *  +page.svelte's onLibraryOpen fires this when the user clicks a hit
   *  for a doc that's already open in some tab — we don't want to spawn
   *  another tab, we want the existing one to scroll + highlight. */
  function onReaderJump(e: CustomEvent<{ tabId: string; page?: number | null; highlight?: string | null }>) {
    const d = e.detail;
    if (!d) return;
    if (d.tabId !== tabId) return;
    if (doc) {
      applyJump(d.page ?? null, d.highlight ?? null);
    } else {
      // Doc not loaded yet — stash for the loadBytes() finish handler.
      pendingJump = { page: d.page ?? null, highlight: d.highlight ?? null };
    }
  }

  // ---------- Glass II Vim adapter (v1.2.0 Slice 2) ----------
  //
  // The reader subscribes to `slab:vim-reader:*` events emitted by
  // `runReaderVim()`. Only the visible tab reacts — others short-circuit
  // on `active === false`. We keep the handlers here next to the nav
  // primitives because they're trivial fan-outs to existing functions.

  function vimScrollContainer(direction: "up" | "down", deltaPx: number) {
    if (!containerEl) return;
    const dy = direction === "down" ? deltaPx : -deltaPx;
    containerEl.scrollBy({ top: dy, behavior: "smooth" });
  }

  function onVimPage(e: Event) {
    if (!active) return;
    const d = (e as CustomEvent<{ direction: "next" | "prev"; count?: number }>).detail;
    const n = Math.max(1, d?.count ?? 1);
    for (let i = 0; i < n; i++) {
      if (d.direction === "next") nextPage();
      else prevPage();
    }
  }

  function onVimGoto(e: Event) {
    if (!active) return;
    if (!doc) return;
    const d = (e as CustomEvent<{ page: number }>).detail;
    // page === -1 is the "last page" sentinel from `G`.
    const target = d.page === -1 ? doc.pageCount : d.page;
    jumpTo(target);
  }

  function onVimScroll(e: Event) {
    if (!active) return;
    const d = (e as CustomEvent<{
      kind: "line" | "half" | "full";
      direction: "up" | "down";
      count?: number;
    }>).detail;
    const containerH = containerEl?.clientHeight ?? 800;
    const px =
      d.kind === "line"
        ? 80 * Math.max(1, d.count ?? 1)
        : d.kind === "half"
          ? Math.floor(containerH / 2)
          : containerH; // full
    vimScrollContainer(d.direction, px);
  }

  function onVimFindOpen() {
    if (!active) return;
    if (!findOpen) toggleFind();
  }

  function onVimFindSet(e: Event) {
    if (!active) return;
    const d = (e as CustomEvent<{ query: string }>).detail;
    findQuery = d.query;
    if (!findOpen) toggleFind();
    runFind(findQuery);
  }

  function onVimFindNext(e: Event) {
    if (!active) return;
    const d = (e as CustomEvent<{ backward: boolean }>).detail;
    if (d.backward) findPrev();
    else findNext();
  }

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

  // ---------- Find (Atlas IV: tested pure core $lib/readerFindView) ----------
  const FIND_HISTORY_KEY = "slab.reader.find.history.v1";

  function loadFindHistory(): string[] {
    if (typeof localStorage === "undefined") return [];
    try {
      const raw = localStorage.getItem(FIND_HISTORY_KEY);
      if (!raw) return [];
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === "string") : [];
    } catch {
      return [];
    }
  }
  function saveFindHistory(h: string[]): void {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(FIND_HISTORY_KEY, JSON.stringify(h));
    } catch {
      /* localStorage full — best effort */
    }
  }

  // Slice 3: live suggestion list derived from the ring + current query.
  // Always computed (not gated on findSuggestOpen) so runFind can read its
  // length to decide whether to keep the dropdown open without a circular
  // dependency; the *rendering* is gated on findSuggestOpen in the markup.
  let findSuggestions = $derived<FindSuggestion[]>(suggestFindHistory(findHistory, findQuery));

  function focusFindInput() {
    queueMicrotask(() => {
      const inp = document.querySelector<HTMLInputElement>(".find-input");
      inp?.focus();
      inp?.select();
    });
  }

  function toggleFind() {
    findOpen = !findOpen;
    if (findOpen) {
      // Opening on an empty box surfaces the recent-search dropdown.
      findSuggestOpen = true;
      findSuggestCursor = -1;
      focusFindInput();
    } else {
      closeFind();
    }
  }

  function closeFind() {
    findOpen = false;
    findSuggestOpen = false;
    findSuggestCursor = -1;
    findStatus = idleFindStatus();
    findAnnounce = "";
    dispatchFind("clear");
  }

  // Slice 2: every pdf.js find dispatch flows through ONE builder.
  function dispatchFind(action: Parameters<typeof buildFindDispatch>[0], query: string = findQuery) {
    if (!eventBus) return;
    eventBus.dispatch("find", buildFindDispatch(action, query, findOptions));
  }

  function runFind(q: string) {
    findSuggestOpen = q.length === 0 ? true : findSuggestions.length > 0;
    findSuggestCursor = -1;
    if (q.trim()) {
      findHistory = pushFindHistory(findHistory, q);
      saveFindHistory(findHistory);
    }
    dispatchFind("find", q);
  }

  function findNext() {
    dispatchFind("again-next");
  }
  function findPrev() {
    dispatchFind("again-prev");
  }

  // Slice 2: re-run after toggling an option chip (case / word / diacritics).
  function setFindOption(key: keyof FindOptions) {
    findOptions = toggleFindOption(findOptions, key);
    if (findQuery) dispatchFind("options");
  }

  // Slice 3: commit a recent-search suggestion into the box + run it.
  function commitSuggestion(q: string) {
    findQuery = q;
    findSuggestOpen = false;
    findSuggestCursor = -1;
    focusFindInput();
    runFind(q);
  }

  // Slice 4: keystrokes inside the find input — dropdown nav first, then
  // the find box's own Enter/Escape. stopPropagation on every handled key
  // so the window-level reader keymap (page nav, Escape-closes-find) never
  // double-fires while the user is interacting with the find input.
  function onFindInputKey(e: KeyboardEvent) {
    if (findSuggestOpen && findSuggestions.length > 0) {
      const intent = classifyFindDropdownKey(e, findSuggestCursor >= 0);
      if (intent === "next" || intent === "prev") {
        e.preventDefault();
        e.stopPropagation();
        const nav = classifyPaletteNav({ key: e.key });
        if (nav) findSuggestCursor = nextPaletteIndex(nav, findSuggestCursor, findSuggestions.length);
        return;
      }
      if (intent === "commit") {
        e.preventDefault();
        e.stopPropagation();
        commitSuggestion(findSuggestions[findSuggestCursor].query);
        return;
      }
      if (intent === "close") {
        // First Escape closes only the dropdown; the bar stays open.
        e.preventDefault();
        e.stopPropagation();
        findSuggestOpen = false;
        findSuggestCursor = -1;
        return;
      }
    }
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      findSuggestOpen = false;
      if (e.shiftKey) findPrev();
      else findNext();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      toggleFind();
    }
  }

  // Slice 1 + 5: turn the two pdf.js find events into one clean status,
  // then narrate it to the aria-live region (deduped by equality).
  function applyFindEvent(s: { state?: number | null; matchesCount?: { current?: number | null; total?: number | null } | null }) {
    findStatus = interpretFindState(s, findQuery);
    const phrase = announceFindStatus(findStatus);
    if (phrase && phrase !== findAnnounce) findAnnounce = phrase;
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
    if (!active) return;
    const isMod = e.metaKey || e.ctrlKey;
    // Atlas Lite (v3.31.0): Cmd+0 → Recents Home, Cmd+Shift+0 → Continue.
    // These need to work whether or not a doc is open, so we handle them
    // before the no-doc early return below.
    if (isMod && e.key === "0" && !e.shiftKey) {
      e.preventDefault();
      onHomeOpen();
      return;
    }
    if (isMod && e.shiftKey && (e.key === "0" || e.key === ")")) {
      e.preventDefault();
      onHomeContinue();
      return;
    }
    if (!doc) return;
    // Slice 4 (Atlas IV): global find chords classified by the tested core —
    // Cmd/Ctrl+F opens/focuses, F3 / Cmd+G cycle matches from anywhere
    // (Shift reverses). Shift+Cmd+F (library search) is deliberately not
    // claimed by classifyFindGlobalKey.
    const findIntent = classifyFindGlobalKey(e);
    if (findIntent === "open") {
      e.preventDefault();
      if (!findOpen) toggleFind();
      else focusFindInput();
      return;
    } else if (findIntent === "again-next" || findIntent === "again-prev") {
      // Only cycle when there's a live query to step through.
      if (findQuery) {
        e.preventDefault();
        if (!findOpen) findOpen = true;
        if (findIntent === "again-prev") findPrev();
        else findNext();
        return;
      }
    }
    if (isMod && (e.key === "=" || e.key === "+")) {
      e.preventDefault();
      zoomIn();
    } else if (isMod && e.key === "-") {
      e.preventDefault();
      zoomOut();
    } else if (e.key === "Escape" && findOpen) {
      closeFind();
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
    // Atlas Lite (v3.31.0): the palette + Cmd+0 hotkey dispatch into here.
    window.addEventListener("slab:home-open", onHomeOpen as EventListener);
    window.addEventListener("slab:home-continue", onHomeContinue as EventListener);
    // Glass II Vim adapter — only the active tab actually reacts (gated
    // inside each handler), but every tab subscribes so the registration
    // matches the unsubscribe path.
    window.addEventListener("slab:vim-reader:page", onVimPage as EventListener);
    window.addEventListener("slab:vim-reader:goto", onVimGoto as EventListener);
    window.addEventListener("slab:vim-reader:scroll", onVimScroll as EventListener);
    window.addEventListener("slab:vim-reader:find-open", onVimFindOpen as EventListener);
    window.addEventListener("slab:vim-reader:find-set", onVimFindSet as EventListener);
    window.addEventListener("slab:vim-reader:find-next", onVimFindNext as EventListener);
    window.addEventListener("slab:reader-jump", onReaderJump as EventListener);

    // If the shell handed us a path on mount, load it.
    if (initialPath && isInTauri()) {
      void openAny(initialPath);
    }

    // Native drag-and-drop on the whole viewer.
    const onDragOver = (e: DragEvent) => {
      if (!active) return;
      if (e.dataTransfer?.types?.includes("Files")) {
        e.preventDefault();
        dropActive = true;
      }
    };
    const onDragLeave = (e: DragEvent) => {
      if (!active) return;
      if (e.target === document.body || (e as any).relatedTarget === null) {
        dropActive = false;
      }
    };
    const onDrop = async (e: DragEvent) => {
      if (!active) return;
      e.preventDefault();
      dropActive = false;
      const file = e.dataTransfer?.files?.[0];
      if (!file) return;
      const lower = file.name.toLowerCase();
      const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".") + 1) : "";
      if (ext === "pdf") {
        const buf = await file.arrayBuffer();
        await loadBytes(file.name, new Uint8Array(buf), file.size);
        return;
      }
      if (!POLYGLOT_EXTS.includes(ext)) {
        ocrStatus = `✗ Unsupported file: ${file.name}`;
        setTimeout(() => (ocrStatus = ""), 3000);
        return;
      }
      // Polyglot path needs a filesystem path for `slab_polyglot`. The
      // dropped File only gives us bytes, so we round-trip via Tauri tempDir.
      if (!isInTauri()) {
        ocrStatus = `✗ Polyglot drop requires the desktop app`;
        setTimeout(() => (ocrStatus = ""), 3000);
        return;
      }
      try {
        const buf = await file.arrayBuffer();
        const safe = file.name.replace(/[^A-Za-z0-9._-]/g, "_");
        const stamp = Date.now().toString(36);
        const dir = await tempDir();
        const tmpIn = await join(dir, `slab-polyglot-in-${stamp}-${safe}`);
        await writeFile(tmpIn, new Uint8Array(buf));
        await openAny(tmpIn);
      } catch (err) {
        const raw = err instanceof Error ? err.message : String(err);
        loadError = friendlyPolyglotError(raw);
      }
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
    window.removeEventListener("slab:home-open", onHomeOpen as EventListener);
    window.removeEventListener("slab:home-continue", onHomeContinue as EventListener);
    window.removeEventListener("slab:vim-reader:page", onVimPage as EventListener);
    window.removeEventListener("slab:vim-reader:goto", onVimGoto as EventListener);
    window.removeEventListener("slab:vim-reader:scroll", onVimScroll as EventListener);
    window.removeEventListener("slab:vim-reader:find-open", onVimFindOpen as EventListener);
    window.removeEventListener("slab:vim-reader:find-set", onVimFindSet as EventListener);
    window.removeEventListener("slab:vim-reader:find-next", onVimFindNext as EventListener);
    window.removeEventListener("slab:reader-jump", onReaderJump as EventListener);
    const dnd = (onMount as any)._slabDnd;
    if (dnd) {
      window.removeEventListener("dragover", dnd.onDragOver);
      window.removeEventListener("dragleave", dnd.onDragLeave);
      window.removeEventListener("drop", dnd.onDrop);
    }
    unsubPluginActions();
    tearDownDoc();
  });

  // Foundry Slice 9 — run a plugin-contributed PDF action against the
  // currently-open doc. Prompts for an output path via the Tauri save
  // dialog (suggesting a sensible default near the input), then invokes
  // `slab_plugins_run_pdf_action` and surfaces a toast keyed by status.
  async function dispatchPluginAction(a: ActivePdfAction): Promise<void> {
    if (!doc?.path) {
      notify.warning(a.label, { detail: "Open a PDF first." });
      return;
    }
    const out = await saveDialog({
      defaultPath: doc.path.replace(/\.pdf$/i, `.${a.id}.pdf`),
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!out) {
      pluginActionsOpen = false;
      return;
    }
    try {
      const rep = await runPluginPdfAction(a.plugin_id, a.id, doc.path, out);
      if (rep.status === "ok") {
        notify.success(a.label, { detail: `Wrote ${out} · ${rep.duration_ms}ms` });
      } else if (rep.status === "nonzeroexit") {
        notify.warning(a.label, {
          detail: rep.stderr.trim().slice(0, 200) || "Non-zero exit",
        });
      } else if (rep.status === "timeout") {
        notify.error(a.label, { detail: "Action timed out" });
      } else {
        notify.error(a.label, {
          detail: rep.stderr.trim().slice(0, 200) || "Failed to spawn",
        });
      }
    } catch (e) {
      notify.error(a.label, { detail: e instanceof Error ? e.message : String(e) });
    } finally {
      pluginActionsOpen = false;
    }
  }

  // Foundry Slice 9 — click-outside dismissal for the plugin actions
  // dropdown. Captures so it fires even when the click target is inside
  // a stopPropagation-y handler somewhere else in the toolbar.
  $effect(() => {
    if (!pluginActionsOpen) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Element | null;
      if (!target?.closest(".plugin-actions-wrap")) pluginActionsOpen = false;
    };
    window.addEventListener("click", handler, true);
    return () => window.removeEventListener("click", handler, true);
  });

  function onOpenRecentEvent(e: CustomEvent<RecentFile>) {
    if (!active) return;
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

  // Atlas Lite (v3.31.0): "Go to Recents Home" — close the active doc so
  // the empty-state RecentsHome renders. Only the active tab acts.
  function onHomeOpen() {
    if (!active) return;
    if (doc) {
      tearDownDoc();
      doc = null;
      onTitleChange?.("New tab");
    }
  }

  // Atlas Lite (v3.31.0): "Continue reading" — pick the recent file with
  // the freshest in-progress lastPage, fall back to the most recent file
  // if no progress exists, then dispatch through the open path.
  function onHomeContinue() {
    if (!active) return;
    const files = listRecent();
    const withProgress = files
      .filter((r) => r.lastPage && r.totalPages && r.lastPage < r.totalPages)
      .sort((a, b) => (b.lastReadAt ?? b.openedAt) - (a.lastReadAt ?? a.openedAt));
    const target = withProgress[0] ?? files[0];
    if (!target) {
      notify.info("No recent files to continue", { duration: 2500 });
      return;
    }
    onOpenRecentEvent({ detail: target } as CustomEvent<RecentFile>);
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

  // Hover-zoom preview: hovering a rail thumbnail pops a larger render
  // beside it so you can read the page before clicking. Top is clamped on
  // screen by the tested clampFlyoutTop; gated by shouldShowPreview (open
  // rail, multi-page doc, in range). previewCanvas renders on demand.
  let previewPage = $state(0);
  let previewTop = $state(8);
  let previewCanvas = $state<HTMLCanvasElement | null>(null);
  const previewVisible = $derived(shouldShowPreview(previewPage, doc?.pageCount ?? 0, thumbsOpen));
  function onThumbHover(n: number, el: HTMLElement) {
    if (!shouldShowPreview(n, doc?.pageCount ?? 0, thumbsOpen)) return;
    previewPage = n;
    const r = el.getBoundingClientRect();
    previewTop = clampFlyoutTop({ top: r.top, height: r.height }, 360, window.innerHeight);
    void renderPreview(n);
  }
  function onThumbLeave() { previewPage = 0; }
  // Keyboard twin of hover: a focused rail thumb drives the SAME preview
  // flyout with Up/Down (prev/next), Home/End to ends, Esc to dismiss —
  // tested by classifyThumbPreviewKey + nextPreviewPage.
  function onThumbKey(n: number, e: KeyboardEvent) {
    const action = classifyThumbPreviewKey(e);
    if (!action) return;
    e.preventDefault();
    if (action === "dismiss") { previewPage = 0; return; }
    const target = nextPreviewPage(previewPage || n, doc?.pageCount ?? 0, action);
    const btn = thumbButtons.get(target);
    if (btn) { btn.focus(); onThumbHover(target, btn); }
  }
  async function renderPreview(n: number) {
    if (!pdfDocument || !previewVisible) return;
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    const c = previewCanvas;
    if (!c || previewPage !== n) return;
    try {
      const page = await pdfDocument.getPage(n);
      if (previewPage !== n) return;
      const base = page.getViewport({ scale: 1 });
      const scale = 280 / base.width;
      const vp = page.getViewport({ scale });
      const dpr = window.devicePixelRatio || 1;
      c.width = Math.floor(vp.width * dpr);
      c.height = Math.floor(vp.height * dpr);
      c.style.width = `${vp.width}px`;
      c.style.height = `${vp.height}px`;
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      await page.render({ canvasContext: ctx, viewport: vp, canvas: c }).promise;
    } catch { /* preview is best-effort */ }
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

  // When this tab becomes the visible one, fit-width/fit-page need to be
  // re-applied because pdf.js sized the canvases against a `display:none`
  // container while it was hidden. `requestAnimationFrame` lets the browser
  // paint the now-visible container before we measure it.
  $effect(() => {
    if (!active) return;
    if (!pdfViewer) return;
    requestAnimationFrame(() => rescaleOnResize());
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

{#snippet filteredOutlineList(nodes: FilteredOutlineNode<OutlineNode>[], depth: number)}
  <ul class="outline-list" class:nested={depth > 0}>
    {#each nodes as fn, i (fn.node.title + i + depth)}
      <li class="outline-item">
        <div class="outline-row" style="padding-left: {depth * 12}px">
          {#if fn.items.length > 0}
            <span class="outline-twist open" aria-hidden="true">▸</span>
          {:else}
            <span class="outline-twist spacer" aria-hidden="true"></span>
          {/if}
          <button
            class="outline-label"
            class:match={fn.selfMatch}
            onclick={() => jumpToOutline(fn.node)}
            title={fn.node.title}
          >
            {#each fn.segments as seg}{#if seg.hit}<mark class="outline-hit">{seg.text}</mark>{:else}{seg.text}{/if}{/each}
          </button>
        </div>
        {#if fn.items.length > 0}
          {@render filteredOutlineList(fn.items, depth + 1)}
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
      Open any PDF — or drop in Office, HTML, EPUB, CSV, images. Files stay on your machine.
    {/if}
  </p>
</header>

{#if !doc}
  <section class="panel home-panel">
    <RecentsHome
      onOpen={(r) => onOpenRecentEvent({ detail: r } as CustomEvent<RecentFile>)}
      onPick={pickFile}
      onContinue={() => { /* hook reserved for analytics / cmd-palette signal */ }}
      loading={loading}
    />
    {#if loadError}
      <div class="status err">✕ {loadError}</div>
    {/if}
  </section>
{/if}

<div class="reader-shell" class:hidden={!doc} data-tab-id={tabId}>
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
          aria-label="Current page"
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
      {#if pluginActions.length > 0}
        <div class="plugin-actions-wrap">
          <button
            class="tb-btn"
            class:active={pluginActionsOpen}
            disabled={!doc}
            onclick={() => (pluginActionsOpen = !pluginActionsOpen)}
            title="Plugin PDF actions"
          >✦ Plugin</button>
          {#if pluginActionsOpen}
            <div class="plugin-actions-menu" role="menu">
              {#each pluginActions as a (`${a.plugin_id}:${a.id}`)}
                <button
                  class="plugin-action-item"
                  onclick={() => dispatchPluginAction(a)}
                  title={`${a.plugin_id}: ${a.cli}`}
                >
                  <span class="plugin-action-label">{a.label}</span>
                  <span class="plugin-action-from">{a.plugin_id}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
      <button class="tb-btn" class:active={infoOpen} disabled={!doc} onclick={() => (infoOpen = !infoOpen)} title="Document info">ⓘ Info</button>
      <button class="tb-btn" onclick={() => (cheatsheetOpen = true)} title="Keyboard shortcuts (?)">?</button>
    </div>
  </div>

  {#if findOpen}
    <div class="findbar">
      <div class="find-field">
        <input
          class="find-input"
          placeholder="Find in document"
          aria-label="Find in document"
          autocomplete="off"
          spellcheck="false"
          role="combobox"
          aria-expanded={findSuggestOpen && findSuggestions.length > 0}
          aria-controls="find-suggest-list"
          aria-activedescendant={findSuggestCursor >= 0 ? `find-suggest-${findSuggestCursor}` : undefined}
          bind:value={findQuery}
          oninput={() => runFind(findQuery)}
          onkeydown={onFindInputKey}
          onfocus={() => { findSuggestOpen = true; }}
          onblur={() => { setTimeout(() => { findSuggestOpen = false; findSuggestCursor = -1; }, 120); }}
        />
        {#if findSuggestOpen && findSuggestions.length > 0}
          <ul class="find-suggest" id="find-suggest-list" role="listbox" aria-label="Recent searches">
            {#each findSuggestions as s, i (s.query)}
              <li role="presentation">
                <button
                  id={`find-suggest-${i}`}
                  type="button"
                  role="option"
                  aria-selected={i === findSuggestCursor}
                  class="find-suggest-item"
                  class:active={i === findSuggestCursor}
                  onmouseenter={() => (findSuggestCursor = i)}
                  onclick={() => commitSuggestion(s.query)}
                >
                  <span class="find-suggest-glyph" aria-hidden="true">{findQuery ? "\u2197" : "\u21BA"}</span>
                  <span class="find-suggest-text">
                    {#each suggestionSegments(s) as seg (seg.text + (seg.hit ? "1" : "0"))}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="find-opts" role="group" aria-label="Find options">
        {#each FIND_OPTION_TOGGLES as opt (opt.key)}
          <button
            type="button"
            class="find-opt-chip"
            class:on={findOptions[opt.key]}
            aria-pressed={findOptions[opt.key]}
            title={opt.title}
            aria-label={opt.title}
            onclick={() => setFindOption(opt.key)}
          >{opt.label}</button>
        {/each}
      </div>
      <span class="find-count" data-tone={findStatusTone(findStatus)} title={describeFindOptions(findOptions)}>
        {describeFindStatus(findStatus)}
      </span>
      {#if findStatus.wrapped}
        <span class="find-wrapped" title="Search wrapped past the end of the document">wrapped</span>
      {/if}
      <button class="tb-btn icon" onclick={findPrev} disabled={!findQuery} title="Previous match (Shift+F3)" aria-label="Previous match">↑</button>
      <button class="tb-btn icon" onclick={findNext} disabled={!findQuery} title="Next match (F3)" aria-label="Next match">↓</button>
      <button class="tb-btn icon" onclick={closeFind} title="Close find (Esc)" aria-label="Close find">×</button>
    </div>
    <div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{findAnnounce}</div>
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
          <!-- Outline skeleton: indented shimmer bars in the tree shape so the
               panel settles in place instead of flashing a bare "Loading…".
               Decorative; one SR label carries the loading state. -->
          <div class="outline-skeleton" aria-busy="true" aria-label="Loading outline">
            {#each [0, 1, 0, 2, 1, 0, 1, 0] as depth, i (i)}
              <span class="ol-skel-bar" style="margin-left: {depth * 14}px; width: {64 - depth * 10}%"></span>
            {/each}
          </div>
        {:else if outline.length === 0}
          <div class="outline-empty">
            <p>No outline in this PDF.</p>
            <button class="outline-add-btn" onclick={() => (outlineEditorOpen = true)}>+ Create outline</button>
          </div>
        {:else}
          <div class="outline-filter">
            <svg class="outline-filter-icon" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
              <circle cx="7" cy="7" r="4.4" stroke="currentColor" stroke-width="1.3" fill="none" />
              <path d="M10.4 10.4L14 14" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            </svg>
            <input
              type="text"
              class="outline-filter-input"
              placeholder={outlineNodeCount > 0 ? `Filter ${outlineNodeCount} heading${outlineNodeCount === 1 ? "" : "s"}…` : "Filter outline…"}
              bind:value={outlineFilter}
              aria-label="Filter outline"
              onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); outlineFilter = ""; } }}
            />
            {#if outlineFilter.trim()}
              <button
                class="outline-filter-clear"
                onclick={() => (outlineFilter = "")}
                title="Clear filter"
                aria-label="Clear outline filter"
              >×</button>
            {/if}
          </div>
          {#if filteredOutline !== null && describeOutlineFilter(filteredOutline)}
            <div class="outline-filter-count" role="status" aria-live="polite">{describeOutlineFilter(filteredOutline)}</div>
          {/if}
          {#if filteredOutline === null}
            <nav class="outline-tree">
              {@render outlineList(outline, 0)}
            </nav>
          {:else if filteredOutline.length === 0}
            <div class="outline-empty outline-no-match">
              <p>No headings match “{outlineFilter.trim()}”.</p>
              <button class="outline-add-btn" onclick={() => (outlineFilter = "")}>Clear filter</button>
            </div>
          {:else}
            <nav class="outline-tree">
              {@render filteredOutlineList(filteredOutline, 0)}
            </nav>
          {/if}
        {/if}
      </aside>
    {:else if thumbsOpen && doc}
      <aside class="thumbs" onmouseleave={onThumbLeave}>
        {#each Array.from({ length: doc.pageCount }, (_, i) => i + 1) as n (n)}
          <button
            class="thumb"
            class:active={n === currentPage}
            onclick={() => jumpTo(n)}
            onmouseenter={(e) => onThumbHover(n, e.currentTarget)}
            onfocus={(e) => onThumbHover(n, e.currentTarget)}
            onkeydown={(e) => onThumbKey(n, e)}
            use:attachThumbBtn={n}
          >
            <canvas use:attachThumb={n}></canvas>
            <span class="thumb-num">{n}</span>
          </button>
        {/each}
      </aside>
      {#if previewVisible}
        <div class="thumb-preview" style="top: {previewTop}px" role="presentation">
          <canvas bind:this={previewCanvas}></canvas>
          <span class="thumb-preview-cap">{previewLabel(previewPage, doc.pageCount)}</span>
        </div>
      {/if}
    {/if}

    <div class="pdfjs-container" class:invert class:jump-halo={jumpHalo} bind:this={containerEl}>
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

    {#if scanBannerText && !ocrRunning}
      <div class="scan-banner" role="status">
        <div class="scan-banner-icon" aria-hidden="true">👁</div>
        <div class="scan-banner-text">{scanBannerText}</div>
        <div class="scan-banner-actions">
          <button class="sb-primary" onclick={runOcr} disabled={ocrRunning}>
            OCR now
          </button>
          <button class="sb-dismiss" onclick={dismissScanBanner} title="Dismiss">
            Dismiss
          </button>
        </div>
      </div>
    {/if}

    {#if slideReport && slideReport.is_slides && !slideBannerDismissed && !presenting}
      <div class="slide-banner" role="status">
        <div class="slide-banner-icon" aria-hidden="true">▷</div>
        <div class="slide-banner-text">
          <strong>This looks like a slide deck.</strong>
          <span class="slide-banner-sub">
            {slideReport.page_count} slides · {slideReport.dominant_label}
            {#if slideReport.pages_with_notes > 0}
              · {slideReport.pages_with_notes} page{slideReport.pages_with_notes === 1 ? "" : "s"} with notes
            {/if}
          </span>
        </div>
        <div class="slide-banner-actions">
          <button class="sb-primary" onclick={startPresenting}>
            ▷ Present
          </button>
          <button class="sb-dismiss" onclick={dismissSlideBanner} title="Dismiss">
            Dismiss
          </button>
        </div>
      </div>
    {/if}

    <!-- Beacon selection bubble: floats above any text selection inside the
         PDF viewer. Mounted at the panel level so absolute positioning works
         relative to the page, not constrained by the viewer's overflow. -->
    <BeaconSelectionBubble host={containerEl ?? null} />

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

{#if presenting && doc && slideReport}
  <PresenterOverlay
    inputPath={doc.path}
    pages={slideReport.pages}
    startPage={currentPage}
    onClose={stopPresenting}
  />
{/if}

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

{#if decryptPending}
  <DecryptModal
    input={decryptPending}
    onUnlock={(decryptedPath) => {
      decryptPending = null;
      // Open the plaintext copy from temp. The reader still records this
      // path in recents — we may want to swap to the original locked path
      // in a future tick, but for now the unlocked copy is what's open.
      void loadPath(decryptedPath);
    }}
    onCancel={() => {
      decryptPending = null;
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

  /* Foundry Slice 9 — plugin PDF actions dropdown. */
  .plugin-actions-wrap { position: relative; display: inline-block; }
  .plugin-actions-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    min-width: 220px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.28);
    padding: 4px;
    z-index: 50;
    display: flex;
    flex-direction: column;
  }
  .plugin-action-item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px 10px;
    background: transparent;
    border: 0;
    border-radius: var(--r-sm);
    cursor: pointer;
    text-align: left;
  }
  .plugin-action-item:hover { background: var(--bg-3); }
  .plugin-action-label { font-size: 13px; color: var(--text); }
  .plugin-action-from {
    font-size: 11px;
    color: var(--text-2);
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  }

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
    position: relative;
    z-index: 5;
  }
  .find-field {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .find-input {
    width: 100%;
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
  /* Slice 3: recent-search dropdown */
  .find-suggest {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--bg-1, var(--bg));
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.32);
    z-index: 20;
    max-height: 240px;
    overflow-y: auto;
  }
  .find-suggest-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: transparent;
    border: none;
    border-radius: calc(var(--r-sm) - 2px);
    padding: 6px 8px;
    color: var(--text-2);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .find-suggest-item.active {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text);
  }
  .find-suggest-glyph {
    color: var(--text-3);
    font-size: 12px;
    width: 14px;
    flex-shrink: 0;
    text-align: center;
  }
  .find-suggest-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .find-suggest-text :global(mark) {
    background: transparent;
    color: var(--accent);
    font-weight: 600;
  }
  /* Slice 2: option chips */
  .find-opts {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .find-opt-chip {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text-3);
    border-radius: var(--r-sm);
    padding: 4px 7px;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
    transition:
      color 0.12s,
      border-color 0.12s,
      background 0.12s;
  }
  .find-opt-chip:hover {
    color: var(--text);
    border-color: var(--text-3);
  }
  .find-opt-chip.on {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .find-count {
    font-size: 12px;
    color: var(--text-3);
    min-width: 64px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .find-count[data-tone="warn"] {
    color: #ffb648;
  }
  .find-count[data-tone="normal"] {
    color: var(--text);
  }
  /* Slice 1: wrapped pill */
  .find-wrapped {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 999px;
    padding: 1px 6px;
    flex-shrink: 0;
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

  /* Hover-zoom preview: a larger render beside the rail (round 55). Fixed
     so the inline top (from clampFlyoutTop vs innerHeight) is viewport-
     relative; left sits just past the ~150px rail. */
  .thumb-preview {
    position: fixed;
    left: 156px;
    z-index: 40;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    pointer-events: none;
    animation: thumb-preview-in 0.1s ease-out;
  }
  .thumb-preview canvas {
    background: white;
    border-radius: 2px;
    max-height: 360px;
  }
  .thumb-preview-cap {
    font-size: 11px;
    color: var(--text-3);
  }
  @keyframes thumb-preview-in {
    from { opacity: 0; transform: translateX(-4px); }
    to { opacity: 1; transform: translateX(0); }
  }
  @media (prefers-reduced-motion: reduce) {
    .thumb-preview { animation: none; }
  }

  /* PDFViewer needs its container to be position:relative or absolute */
  .pdfjs-container {
    position: relative;
    overflow: auto;
    background: var(--bg);
  }

  /* Atlas v2.2.0 — gold halo pulse when a search jump lands on a page.
   * 720 ms cubic-bezier "settle" — same easing family the Pages Visual
   * rotate-tilt uses, for product-wide consistency. The halo lives on
   * the active page (".pdfViewer .page[data-page-number] :global") so
   * it reads as "this is the page we jumped to" rather than a giant
   * frame around the whole reader. */
  .pdfjs-container.jump-halo :global(.pdfViewer .page) {
    animation: slab-jump-halo 720ms cubic-bezier(0.34, 1.56, 0.64, 1);
    will-change: box-shadow, transform;
  }
  @keyframes slab-jump-halo {
    0%   { box-shadow: 0 2px 8px rgba(0,0,0,0.5), 0 0 0 0 rgba(245, 158, 11, 0.0); transform: scale(1); }
    24%  { box-shadow: 0 2px 8px rgba(0,0,0,0.5), 0 0 0 14px rgba(245, 158, 11, 0.55); transform: scale(1.014); }
    100% { box-shadow: 0 2px 8px rgba(0,0,0,0.5), 0 0 0 0 rgba(245, 158, 11, 0); transform: scale(1); }
  }
  @media (prefers-reduced-motion: reduce) {
    .pdfjs-container.jump-halo :global(.pdfViewer .page) {
      animation: none;
      box-shadow: 0 2px 8px rgba(0,0,0,0.5), 0 0 0 3px rgba(245, 158, 11, 0.85);
      transition: box-shadow 240ms ease-out;
    }
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
  /* Glass: recent-card pin/remove overlay actions */
  .recent-card-wrap {
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .recent-card-wrap.pinned .recent-card {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent) inset;
  }
  .recent-pin-flag {
    position: absolute;
    top: 6px;
    left: 6px;
    font-size: 13px;
    background: var(--bg);
    border: 1px solid var(--accent);
    color: var(--accent);
    border-radius: 999px;
    padding: 1px 6px;
    line-height: 1.2;
    pointer-events: none;
  }
  .recent-card-actions {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 120ms;
    pointer-events: none;
  }
  .recent-card-wrap:hover .recent-card-actions,
  .recent-card-wrap:focus-within .recent-card-actions {
    opacity: 1;
    pointer-events: auto;
  }
  .recent-act {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    width: 24px;
    height: 24px;
    padding: 0;
    font-size: 11px;
    color: var(--text-2);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color 120ms, color 120ms, background 120ms;
  }
  .recent-act:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .recent-act.active {
    border-color: var(--accent);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, var(--bg));
  }
  .recent-act.danger:hover {
    border-color: #e0654a;
    color: #e0654a;
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
  /* Round 59 — outline filter input. Compact palette-grade filter bar that
     sits under the outline header; matches the dark-first input styling used
     across Slab's search surfaces. */
  .outline-filter {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 8px 6px;
    padding: 4px 8px;
    border-radius: 7px;
    background: var(--bg-2, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--border-1, rgba(255, 255, 255, 0.08));
    transition: border-color 0.12s ease;
  }
  .outline-filter:focus-within {
    border-color: color-mix(in srgb, var(--accent, #7c8cff) 55%, transparent);
  }
  .outline-filter-icon {
    flex: none;
    color: var(--text-3, #888);
  }
  .outline-filter-input {
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-1, #fff);
    font-size: 12px;
    padding: 1px 0;
  }
  .outline-filter-input::placeholder {
    color: var(--text-3, #888);
  }
  .outline-filter-clear {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-3, #888);
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .outline-filter-clear:hover {
    background: var(--bg-3, rgba(255, 255, 255, 0.08));
    color: var(--text-1, #fff);
  }
  .outline-filter-count {
    font-size: 10.5px;
    color: var(--text-3, #888);
    padding: 0 12px 4px;
  }
  .outline-no-match {
    padding-top: 6px;
  }
  .outline-label.match {
    color: var(--text-1, #fff);
  }
  .outline-hit {
    background: color-mix(in srgb, var(--accent, #7c8cff) 32%, transparent);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }
  /* First-load skeleton — indented shimmer bars matching the outline tree.
     Same shimmer family as SmartFolders/Convert so loaders feel like one app. */
  .outline-skeleton {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px 12px;
  }
  .ol-skel-bar {
    height: 11px;
    border-radius: 4px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--text-1, #fff) 7%, transparent) 0%,
      color-mix(in srgb, var(--text-1, #fff) 14%, transparent) 50%,
      color-mix(in srgb, var(--text-1, #fff) 7%, transparent) 100%
    );
    background-size: 200% 100%;
    animation: ol-shimmer 1.4s ease-in-out infinite;
  }
  @keyframes ol-shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .ol-skel-bar { animation: none; }
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

  /* ---- Scan-audit banner (Lens) ---- */
  .scan-banner {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-radius: 10px;
    background: linear-gradient(180deg, #1e2433, #161a25);
    color: #e8ecf5;
    border: 1px solid #2c3447;
    font-size: 13px;
    box-shadow: 0 6px 22px rgba(0, 0, 0, 0.45);
    max-width: min(640px, 90%);
    animation: scan-banner-in 220ms ease-out;
  }
  @keyframes scan-banner-in {
    from {
      opacity: 0;
      transform: translate(-50%, -8px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }
  .scan-banner-icon {
    font-size: 18px;
    line-height: 1;
    flex: 0 0 auto;
  }
  .scan-banner-text {
    flex: 1 1 auto;
    line-height: 1.4;
    white-space: normal;
  }
  .scan-banner-actions {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
  }
  .scan-banner-actions button {
    border: 1px solid #2c3447;
    background: #232a3a;
    color: #e8ecf5;
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .scan-banner-actions button:hover:not(:disabled) {
    background: #2c3447;
    border-color: #3a455c;
  }
  .scan-banner-actions button.sb-primary {
    background: #3a73c8;
    border-color: #3a73c8;
    color: #fff;
  }
  .scan-banner-actions button.sb-primary:hover:not(:disabled) {
    background: #4a86db;
    border-color: #4a86db;
  }
  .scan-banner-actions button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .scan-banner-actions button.sb-dismiss {
    background: transparent;
    border-color: transparent;
    color: #99a3b5;
  }
  .scan-banner-actions button.sb-dismiss:hover {
    color: #e8ecf5;
    background: #232a3a;
    border-color: #2c3447;
  }

  /* ---- Slide-deck banner (Theater Slice 4) ---- */
  .slide-banner {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-radius: 10px;
    background: linear-gradient(180deg, #1c2e22, #15211a);
    color: #e8ecf5;
    border: 1px solid #2c5a3f;
    font-size: 13px;
    box-shadow: 0 6px 22px rgba(0, 0, 0, 0.45);
    max-width: min(720px, 90%);
    animation: scan-banner-in 220ms ease-out;
  }
  .slide-banner-icon {
    font-size: 18px;
    line-height: 1;
    flex: 0 0 auto;
    color: #4ade80;
  }
  .slide-banner-text {
    flex: 1 1 auto;
    line-height: 1.4;
    white-space: normal;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .slide-banner-text strong {
    font-weight: 600;
  }
  .slide-banner-sub {
    font-size: 11px;
    color: #99a3b5;
  }
  .slide-banner-actions {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
  }
  .slide-banner-actions button {
    border: 1px solid #2c5a3f;
    background: #1a3424;
    color: #e8ecf5;
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .slide-banner-actions button:hover:not(:disabled) {
    background: #234630;
    border-color: #3a7a55;
  }
  .slide-banner-actions button.sb-primary {
    background: #3a8c5a;
    border-color: #3a8c5a;
    color: #fff;
  }
  .slide-banner-actions button.sb-primary:hover:not(:disabled) {
    background: #4aa370;
    border-color: #4aa370;
  }
  .slide-banner-actions button.sb-dismiss {
    background: transparent;
    border-color: transparent;
    color: #99a3b5;
  }
  .slide-banner-actions button.sb-dismiss:hover {
    color: #e8ecf5;
    background: #1a3424;
    border-color: #2c5a3f;
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
