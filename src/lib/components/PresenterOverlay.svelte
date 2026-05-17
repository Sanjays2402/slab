<script lang="ts">
  // Presenter overlay (v0.15.0 Theater Slice 2)
  //
  // Single-window presenter UX: current slide rendered big, next slide
  // thumbnail, speaker notes, slide timer, simple keyboard navigation.
  // Designed to be mounted *over* the app (position: fixed, z-index high)
  // when a deck is being presented. The caller in SlidesPanel.svelte
  // controls open/close + supplies the deck path and slide notes.
  //
  // This is the "internal display" presenter — current slide + next +
  // notes + timer, all in one window. The dedicated dual-monitor
  // audience window is wired in a follow-up slice (Tauri multi-window
  // requires `tauri.conf.json` plumbing). For now this is already a
  // huge step up from any other open-source PDF viewer's presenter
  // mode, which usually has none of these.
  //
  // Rendering: we lean on pdfjs-dist (already a dep) but only the
  // `getDocument` + `getPage` + `render` APIs — no PDFViewer chrome
  // because we want full canvas control for the big slide.

  // @ts-expect-error - pdfjs-dist .mjs has no types index alias
  import * as pdfjsLib from "pdfjs-dist/build/pdf.mjs";
  import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import type { SlidePage } from "$lib/slides";
  import { onDestroy, onMount } from "svelte";
  import PresenterAnnotations from "./PresenterAnnotations.svelte";

  pdfjsLib.GlobalWorkerOptions.workerSrc = workerSrc;

  type Props = {
    inputPath: string;
    pages: SlidePage[];
    /** 1-indexed start page. */
    startPage?: number;
    onClose: () => void;
  };
  let { inputPath, pages, startPage = 1, onClose }: Props = $props();

  // ---------- State ----------
  let pdf = $state<unknown | null>(null);
  let pageCount = $state(0);
  let current = $state(1);
  let loadErr = $state<string | null>(null);
  let mainCanvas = $state<HTMLCanvasElement | null>(null);
  let nextCanvas = $state<HTMLCanvasElement | null>(null);
  let mainRenderTask: { cancel(): void } | null = null;
  let nextRenderTask: { cancel(): void } | null = null;

  // Annotation overlay (Slice 3) — ref via bind:this
  type ExportStroke = {
    page: number;
    tool: "pen" | "highlighter";
    color: [number, number, number];
    width_pt: number;
    points: Array<[number, number]>;
  };
  type AnnotationsApi = {
    setTool: (t: "none" | "laser" | "pen" | "highlighter") => void;
    eraseCurrent: () => void;
    eraseAll: () => void;
    toggleBlackout: () => void;
    toggleWhiteout: () => void;
    isBlackout: () => boolean;
    isWhiteout: () => boolean;
    togglePicker: () => void;
    closePicker: () => void;
    isPickerOpen: () => boolean;
    currentTool: () => "none" | "laser" | "pen" | "highlighter";
    getAllStrokes: () => ExportStroke[];
    hasStrokes: () => boolean;
  };
  let annotations = $state<AnnotationsApi | null>(null);
  let toolLabel = $state<"none" | "laser" | "pen" | "highlighter">("none");
  let blackoutOn = $state(false);
  let whiteoutOn = $state(false);
  let pickerOn = $state(false);

  function pickTool(t: "none" | "laser" | "pen" | "highlighter") {
    annotations?.setTool(t);
    toolLabel = t;
  }
  function eraseCurrent() {
    annotations?.eraseCurrent();
  }
  function toggleBlackout() {
    annotations?.toggleBlackout();
    blackoutOn = annotations?.isBlackout() ?? false;
    whiteoutOn = annotations?.isWhiteout() ?? false;
  }
  function toggleWhiteout() {
    annotations?.toggleWhiteout();
    blackoutOn = annotations?.isBlackout() ?? false;
    whiteoutOn = annotations?.isWhiteout() ?? false;
  }
  function togglePicker() {
    annotations?.togglePicker();
    pickerOn = annotations?.isPickerOpen() ?? false;
  }

  // ---------- Slice 5: Export annotated deck ----------
  let exporting = $state(false);
  let exportMsg = $state<{ kind: "ok" | "err"; text: string } | null>(null);
  let exportMsgTimer = 0;

  function flashExport(kind: "ok" | "err", text: string) {
    exportMsg = { kind, text };
    clearTimeout(exportMsgTimer);
    exportMsgTimer = window.setTimeout(() => {
      exportMsg = null;
    }, 4500);
  }

  async function exportAnnotated() {
    if (exporting) return;
    if (!annotations || !annotations.hasStrokes()) {
      flashExport("err", "No ink to export — draw something first.");
      return;
    }
    exporting = true;
    try {
      // Lazy-import Tauri APIs so we keep the component tree light.
      const [{ save }, { invoke }] = await Promise.all([
        import("@tauri-apps/plugin-dialog"),
        import("@tauri-apps/api/core"),
      ]);
      // Derive a default output path: "<orig stem>-annotated.pdf"
      const slash = Math.max(inputPath.lastIndexOf("/"), inputPath.lastIndexOf("\\"));
      const dir = slash >= 0 ? inputPath.slice(0, slash) : "";
      const base = slash >= 0 ? inputPath.slice(slash + 1) : inputPath;
      const dot = base.lastIndexOf(".");
      const stem = dot > 0 ? base.slice(0, dot) : base;
      const defaultName = `${stem}-annotated.pdf`;
      const defaultPath = dir ? `${dir}/${defaultName}` : defaultName;
      const out = await save({
        defaultPath,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!out) {
        exporting = false;
        return;
      }
      const strokes = annotations.getAllStrokes();
      const res = (await invoke("slab_theater_export_annotated", {
        input: inputPath,
        output: out,
        opts: { strokes },
      })) as { status: "ok"; value: number } | { status: "err"; message: string };
      if (res.status === "ok") {
        flashExport("ok", `Saved ${res.value} stroke${res.value === 1 ? "" : "s"} → ${out}`);
      } else {
        flashExport("err", res.message || "Export failed.");
      }
    } catch (e) {
      flashExport("err", String(e));
    } finally {
      exporting = false;
    }
  }

  // Timer
  let timerMs = $state(0);
  let timerStartedAt = $state<number | null>(null);
  let timerRaf = 0;
  let timerRunning = $state(false);

  function tickTimer() {
    if (timerStartedAt != null) {
      timerMs = Date.now() - timerStartedAt;
    }
    if (timerRunning) {
      timerRaf = requestAnimationFrame(tickTimer);
    }
  }

  function startOrPauseTimer() {
    if (timerRunning) {
      timerRunning = false;
      cancelAnimationFrame(timerRaf);
      // Bake elapsed into timerMs and clear start.
      if (timerStartedAt != null) timerMs = Date.now() - timerStartedAt;
      timerStartedAt = null;
    } else {
      timerRunning = true;
      // Resume from current timerMs.
      timerStartedAt = Date.now() - timerMs;
      timerRaf = requestAnimationFrame(tickTimer);
    }
  }

  function resetTimer() {
    timerRunning = false;
    cancelAnimationFrame(timerRaf);
    timerMs = 0;
    timerStartedAt = null;
  }

  function formatTimer(ms: number): string {
    const total = Math.floor(ms / 1000);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const mm = String(m).padStart(2, "0");
    const ss = String(s).padStart(2, "0");
    return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
  }

  // ---------- Loading ----------
  async function loadPdf(path: string): Promise<void> {
    try {
      // Tauri exposes a real file at the absolute path; convertFileSrc
      // gives us a usable URL that pdf.js can fetch.
      const { convertFileSrc } = await import("@tauri-apps/api/core");
      const url = convertFileSrc(path);
      const task = pdfjsLib.getDocument({ url, isEvalSupported: false });
      const doc = await task.promise;
      pdf = doc;
      pageCount = doc.numPages;
      current = Math.min(Math.max(startPage, 1), pageCount);
      await renderCurrent();
    } catch (e) {
      loadErr = String(e);
    }
  }

  async function renderInto(
    canvas: HTMLCanvasElement,
    pageNum: number,
  ): Promise<{ cancel(): void } | null> {
    if (!pdf || pageNum < 1 || pageNum > pageCount) return null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const page = await (pdf as any).getPage(pageNum);
    // Fit the page into the canvas's drawable size at devicePixelRatio.
    const dpr = window.devicePixelRatio || 1;
    const cw = canvas.clientWidth * dpr;
    const ch = canvas.clientHeight * dpr;
    const baseViewport = page.getViewport({ scale: 1 });
    const scale = Math.min(cw / baseViewport.width, ch / baseViewport.height);
    const viewport = page.getViewport({ scale });
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    canvas.style.width = `${viewport.width / dpr}px`;
    canvas.style.height = `${viewport.height / dpr}px`;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const task = page.render({ canvasContext: ctx, viewport });
    return task;
  }

  async function renderCurrent() {
    if (mainRenderTask) {
      mainRenderTask.cancel();
      mainRenderTask = null;
    }
    if (nextRenderTask) {
      nextRenderTask.cancel();
      nextRenderTask = null;
    }
    if (mainCanvas) {
      mainRenderTask = await renderInto(mainCanvas, current);
      try {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        await (mainRenderTask as any)?.promise;
      } catch {
        /* cancelled */
      }
    }
    if (nextCanvas) {
      const np = current + 1;
      if (np <= pageCount) {
        nextRenderTask = await renderInto(nextCanvas, np);
        try {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          await (nextRenderTask as any)?.promise;
        } catch {
          /* cancelled */
        }
      } else if (nextCanvas) {
        const ctx = nextCanvas.getContext("2d");
        if (ctx) ctx.clearRect(0, 0, nextCanvas.width, nextCanvas.height);
      }
    }
  }

  function goTo(n: number) {
    const clamped = Math.min(Math.max(n, 1), pageCount);
    if (clamped === current) return;
    current = clamped;
    void renderCurrent();
  }
  function next() {
    if (current < pageCount) goTo(current + 1);
  }
  function prev() {
    if (current > 1) goTo(current - 1);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (annotations?.isPickerOpen()) {
        annotations.closePicker();
        pickerOn = false;
        return;
      }
      if (annotations?.isBlackout() || annotations?.isWhiteout()) {
        if (annotations.isBlackout()) annotations.toggleBlackout();
        if (annotations.isWhiteout()) annotations.toggleWhiteout();
        blackoutOn = false;
        whiteoutOn = false;
        return;
      }
      if (toolLabel !== "none") {
        pickTool("none");
        return;
      }
      onClose();
      return;
    }
    if (e.key === "ArrowRight" || e.key === " " || e.key === "PageDown") {
      e.preventDefault();
      next();
      return;
    }
    if (e.key === "ArrowLeft" || e.key === "PageUp") {
      e.preventDefault();
      prev();
      return;
    }
    if (e.key === "Home") {
      e.preventDefault();
      goTo(1);
      return;
    }
    if (e.key === "End") {
      e.preventDefault();
      goTo(pageCount);
      return;
    }
    if (e.key === "t" || e.key === "T") {
      e.preventDefault();
      startOrPauseTimer();
      return;
    }
    if (e.key === "r" || e.key === "R") {
      e.preventDefault();
      resetTimer();
      return;
    }
    // Annotation shortcuts (Slice 3)
    if (e.key === "l" || e.key === "L") {
      e.preventDefault();
      pickTool(toolLabel === "laser" ? "none" : "laser");
      return;
    }
    if (e.key === "p" || e.key === "P") {
      e.preventDefault();
      pickTool(toolLabel === "pen" ? "none" : "pen");
      return;
    }
    if (e.key === "h" || e.key === "H") {
      e.preventDefault();
      pickTool(toolLabel === "highlighter" ? "none" : "highlighter");
      return;
    }
    if (e.key === "n" || e.key === "N") {
      e.preventDefault();
      pickTool("none");
      return;
    }
    if (e.key === "x" || e.key === "X") {
      e.preventDefault();
      eraseCurrent();
      return;
    }
    if (e.key === "b" || e.key === "B") {
      e.preventDefault();
      toggleBlackout();
      return;
    }
    if (e.key === "w" || e.key === "W") {
      e.preventDefault();
      toggleWhiteout();
      return;
    }
    if (e.key === "g" || e.key === "G") {
      e.preventDefault();
      togglePicker();
      return;
    }
    if (e.key === "s" || e.key === "S") {
      e.preventDefault();
      void exportAnnotated();
      return;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    void loadPdf(inputPath);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    cancelAnimationFrame(timerRaf);
    if (mainRenderTask) mainRenderTask.cancel();
    if (nextRenderTask) nextRenderTask.cancel();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (pdf as any)?.destroy?.();
  });

  // ---------- Derived ----------
  const currentNotes = $derived.by((): string => {
    const p = pages.find((x) => x.page === current);
    return p?.notes ?? "";
  });
</script>

<div
  class="presenter"
  role="dialog"
  aria-label="Presenter mode"
  aria-modal="true"
>
  <header class="bar">
    <div class="left">
      <button class="ghost" onclick={onClose} title="Exit presenter (Esc)">
        ✕ Exit
      </button>
      <span class="title">Presenter</span>
    </div>
    <div class="center">
      <span class="clock" class:running={timerRunning}>{formatTimer(timerMs)}</span>
      <button class="ghost" onclick={startOrPauseTimer} title="Start / pause timer (T)">
        {timerRunning ? "Pause" : "Start"} timer
      </button>
      <button class="ghost" onclick={resetTimer} title="Reset timer (R)">Reset</button>
    </div>
    <div class="right">
      <div class="toolset" role="toolbar" aria-label="Annotation tools">
        <button
          class="ghost tool"
          class:active={toolLabel === "laser"}
          onclick={() => pickTool(toolLabel === "laser" ? "none" : "laser")}
          title="Laser pointer (L)">⦿</button
        >
        <button
          class="ghost tool"
          class:active={toolLabel === "pen"}
          onclick={() => pickTool(toolLabel === "pen" ? "none" : "pen")}
          title="Pen (P)">✎</button
        >
        <button
          class="ghost tool"
          class:active={toolLabel === "highlighter"}
          onclick={() => pickTool(toolLabel === "highlighter" ? "none" : "highlighter")}
          title="Highlighter (H)">▮</button
        >
        <button class="ghost tool" onclick={eraseCurrent} title="Erase ink on this slide (X)"
          >⌫</button
        >
        <button
          class="ghost tool"
          class:active={blackoutOn}
          onclick={toggleBlackout}
          title="Blackout (B)">◼</button
        >
        <button
          class="ghost tool"
          class:active={whiteoutOn}
          onclick={toggleWhiteout}
          title="Whiteout (W)">◻</button
        >
        <button
          class="ghost tool"
          class:active={pickerOn}
          onclick={togglePicker}
          title="Jump to slide (G)">▦</button
        >
        <button
          class="ghost tool save"
          class:busy={exporting}
          onclick={exportAnnotated}
          disabled={exporting}
          title="Save annotated PDF (S)">⤓</button
        >
      </div>
      <button class="ghost" onclick={prev} disabled={current <= 1} title="Prev (←)">‹</button>
      <span class="pos">{current} / {pageCount || pages.length}</span>
      <button
        class="ghost"
        onclick={next}
        disabled={current >= pageCount && pageCount > 0}
        title="Next (→)">›</button
      >
    </div>
  </header>

  {#if loadErr}
    <div class="err">
      <div class="err-title">Couldn't open the deck for presenting</div>
      <pre>{loadErr}</pre>
      <button class="ghost" onclick={onClose}>Close</button>
    </div>
  {:else}
    <div class="stage">
      <section class="main">
        <div class="slide-wrap">
          <canvas bind:this={mainCanvas}></canvas>
          <PresenterAnnotations
            bind:this={annotations}
            currentPage={current}
            pageCount={pageCount || pages.length}
            onJumpTo={(n: number) => {
              goTo(n);
              pickerOn = false;
            }}
          />
        </div>
      </section>

      <aside class="side">
        <div class="next">
          <div class="next-label">Next</div>
          <div class="next-canvas-wrap">
            {#if current >= pageCount}
              <div class="next-end">End of deck</div>
            {:else}
              <canvas bind:this={nextCanvas}></canvas>
            {/if}
          </div>
        </div>
        <div class="notes">
          <div class="notes-label">Speaker notes</div>
          {#if currentNotes.trim().length > 0}
            <pre>{currentNotes}</pre>
          {:else}
            <div class="notes-empty">No notes for this slide.</div>
          {/if}
        </div>
      </aside>
    </div>

    <footer class="hints">
      <span>← prev</span>
      <span>→ / space / PgDn next</span>
      <span>Home / End jump</span>
      <span>T timer · R reset</span>
      <span>L laser · P pen · H highlighter · N none</span>
      <span>X erase slide · B blackout · W whiteout</span>
      <span>G grid · S save annotated · Esc exit</span>
    </footer>
  {/if}

  {#if exportMsg}
    <div class="toast {exportMsg.kind}" role="status" aria-live="polite">
      {exportMsg.text}
    </div>
  {/if}
</div>

<style>
  .presenter {
    position: fixed;
    inset: 0;
    background: #08090b;
    color: #eef0f3;
    z-index: 9000;
    display: grid;
    grid-template-rows: auto 1fr auto;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 14px;
    background: #0f1115;
    border-bottom: 1px solid #1f2228;
  }
  .left,
  .center,
  .right {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .title {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #8a92a3;
  }
  .clock {
    font-variant-numeric: tabular-nums;
    font-size: 18px;
    font-weight: 600;
    padding: 4px 10px;
    border-radius: 6px;
    background: #1a1d24;
    color: #c9d0db;
    min-width: 86px;
    text-align: center;
  }
  .clock.running {
    color: #4ade80;
    background: rgba(74, 222, 128, 0.08);
  }
  .pos {
    font-variant-numeric: tabular-nums;
    color: #c9d0db;
    min-width: 60px;
    text-align: center;
  }
  .ghost {
    background: transparent;
    border: 1px solid #2a2f37;
    border-radius: 6px;
    color: #c9d0db;
    padding: 4px 10px;
    font-size: 13px;
    cursor: pointer;
  }
  .ghost:hover {
    background: #1a1d24;
    color: #fff;
  }
  .ghost:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .stage {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 0;
    min-height: 0;
  }
  .main {
    display: flex;
    align-items: center;
    justify-content: center;
    background: #08090b;
    padding: 20px;
    min-height: 0;
    min-width: 0;
  }
  .main canvas {
    background: #fff;
    max-width: 100%;
    max-height: 100%;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.6);
    border-radius: 6px;
  }
  .slide-wrap {
    position: relative;
    display: inline-block;
    line-height: 0;
  }
  .toolset {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    background: #1a1d24;
    border: 1px solid #2a2f37;
    border-radius: 8px;
    margin-right: 8px;
  }
  .tool {
    border: none;
    background: transparent;
    color: #c9d0db;
    font-size: 14px;
    padding: 4px 8px;
    min-width: 28px;
  }
  .tool:hover {
    background: #20242c;
    color: #fff;
  }
  .tool.active {
    background: rgba(74, 222, 128, 0.12);
    color: #4ade80;
    border: 1px solid rgba(74, 222, 128, 0.35);
  }
  .tool.save {
    color: #93c5fd;
  }
  .tool.save:hover {
    background: rgba(147, 197, 253, 0.12);
    color: #c7defb;
  }
  .tool.busy {
    opacity: 0.55;
    cursor: progress;
  }
  .toast {
    position: fixed;
    bottom: 22px;
    right: 22px;
    z-index: 9500;
    max-width: 480px;
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
    line-height: 1.35;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
    font-feature-settings: "tnum";
  }
  .toast.ok {
    background: rgba(20, 70, 38, 0.92);
    color: #d3f5dd;
    border: 1px solid rgba(74, 222, 128, 0.55);
  }
  .toast.err {
    background: rgba(96, 24, 24, 0.92);
    color: #ffd6d6;
    border: 1px solid rgba(248, 113, 113, 0.5);
  }
  .side {
    background: #0f1115;
    border-left: 1px solid #1f2228;
    display: grid;
    grid-template-rows: auto 1fr;
    min-height: 0;
  }
  .next {
    padding: 14px;
    border-bottom: 1px solid #1f2228;
  }
  .next-label,
  .notes-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #6b7384;
    margin-bottom: 8px;
  }
  .next-canvas-wrap {
    aspect-ratio: 16 / 9;
    background: #1a1d24;
    border-radius: 6px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .next-canvas-wrap canvas {
    width: 100%;
    height: 100%;
    background: #fff;
  }
  .next-end {
    color: #6b7384;
    font-size: 13px;
  }
  .notes {
    padding: 14px;
    overflow: auto;
    min-height: 0;
  }
  .notes pre {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: 14px;
    line-height: 1.55;
    color: #d6dae3;
    margin: 0;
  }
  .notes-empty {
    color: #6b7384;
    font-size: 13px;
  }
  .hints {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    padding: 8px 14px;
    background: #0f1115;
    border-top: 1px solid #1f2228;
    font-size: 11px;
    color: #6b7384;
  }
  .err {
    margin: 60px auto;
    max-width: 720px;
    background: #1a1d24;
    border: 1px solid #2a2f37;
    padding: 20px;
    border-radius: 8px;
  }
  .err-title {
    font-weight: 600;
    margin-bottom: 8px;
    color: #f87171;
  }
  .err pre {
    white-space: pre-wrap;
    font-size: 12px;
    color: #c9d0db;
  }
  @media (max-width: 900px) {
    .stage {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr auto;
    }
    .side {
      border-left: none;
      border-top: 1px solid #1f2228;
    }
  }
</style>
