<script lang="ts">
  /*
    TheaterCanvas.svelte (v2.3.0 Slice 5)

    Standalone pdfjs renderer used by the dedicated Theater windows
    (audience fullscreen + presenter control). Stripped-down copy of the
    ReaderPanel render path — no text layer, no annotation layer, no
    scroll virtualization. Just one page at a time, "fit: contain" to the
    container, repainted whenever `path` or `page` changes.

    Why a separate component instead of reusing ReaderPanel? The audience
    window has zero chrome — no toolbar, no sidebar, no page input. It
    needs a canvas that fills its parent and re-renders on prop change,
    nothing else. ReaderPanel ships ~500 lines of UX irrelevant here.
  */
  // @ts-expect-error - pdfjs-dist .mjs has no types index alias
  import * as pdfjsLib from "pdfjs-dist/build/pdf.mjs";
  import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import { onDestroy, onMount, tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";

  pdfjsLib.GlobalWorkerOptions.workerSrc = workerSrc;

  type Props = {
    /** Absolute filesystem path to the PDF. Reloaded if changed. */
    path: string;
    /** 1-based page index. Rerenders if changed. */
    page: number;
    /**
     * Optional sub-page region to fit: `[x0, y0, x1, y1]` in normalised
     * 0..1 page coordinates, top-left origin. Used by the spotlight
     * mode to render only the highlighted region. Defaults to the full
     * page when null.
     */
    region?: [number, number, number, number] | null;
    /** Optional max device-pixel scale cap; defaults to 2 (Retina). */
    maxDpr?: number;
  };
  let { path, page, region = null, maxDpr = 2 }: Props = $props();

  // ---- Local state ----
  let canvas = $state<HTMLCanvasElement | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let pdf: any | null = null;
  let pageCount = 0;
  let loadedPath: string | null = null;
  let renderToken = 0;
  let loadErr = $state<string | null>(null);
  let renderTask: { cancel(): void } | null = null;

  // ---- Lifecycle ----
  onMount(async () => {
    await ensureLoaded(path);
    await renderCurrent();
  });

  onDestroy(() => {
    try {
      renderTask?.cancel();
    } catch {
      // ignore: cancel after destroy is fine
    }
    renderTask = null;
    if (pdf) {
      try {
        pdf.destroy?.();
      } catch {
        // ignore
      }
    }
  });

  // ---- Reactivity: rerender on prop change ----
  $effect(() => {
    // Touch the props so Svelte tracks them.
    const _p = path;
    const _pg = page;
    const _r = region;
    const _m = maxDpr;
    void _p;
    void _pg;
    void _r;
    void _m;
    rerender();
  });

  async function rerender() {
    await ensureLoaded(path);
    await renderCurrent();
  }

  // ---- Loading ----
  async function ensureLoaded(p: string): Promise<void> {
    if (loadedPath === p && pdf) return;
    try {
      const url = convertFileSrc(p);
      const task = pdfjsLib.getDocument({ url, isEvalSupported: false });
      const doc = await task.promise;
      pdf = doc;
      pageCount = doc.numPages;
      loadedPath = p;
      loadErr = null;
    } catch (e) {
      loadErr = e instanceof Error ? e.message : String(e);
      pdf = null;
      pageCount = 0;
    }
  }

  // ---- Rendering ----
  async function renderCurrent(): Promise<void> {
    if (!pdf || !canvas) return;
    const myToken = ++renderToken;
    const targetPage = Math.min(Math.max(page, 1), Math.max(pageCount, 1));
    if (renderTask) {
      try {
        renderTask.cancel();
      } catch {
        // ignore
      }
      renderTask = null;
    }
    // Make sure layout is up-to-date so clientWidth/Height are accurate.
    await tick();
    if (myToken !== renderToken) return;

    let pdfPage: unknown;
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      pdfPage = await (pdf as any).getPage(targetPage);
    } catch (e) {
      loadErr = e instanceof Error ? e.message : String(e);
      return;
    }
    if (myToken !== renderToken) return;

    const dpr = Math.min(window.devicePixelRatio || 1, maxDpr);
    const cw = (canvas.clientWidth || canvas.parentElement?.clientWidth || 1280) * dpr;
    const ch = (canvas.clientHeight || canvas.parentElement?.clientHeight || 720) * dpr;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const baseViewport = (pdfPage as any).getViewport({ scale: 1 });
    // If a region is requested, we still render the full page at the
    // scale that fits the region into the canvas — then translate the
    // viewport so the region's top-left lands at canvas origin. This
    // keeps text crisp without a second pdfjs render path.
    let regionRect: { x: number; y: number; w: number; h: number } | null = null;
    if (region) {
      const [x0, y0, x1, y1] = region;
      const rx = Math.max(0, Math.min(1, x0));
      const ry = Math.max(0, Math.min(1, y0));
      const rx1 = Math.max(rx, Math.min(1, x1));
      const ry1 = Math.max(ry, Math.min(1, y1));
      regionRect = {
        x: rx * baseViewport.width,
        y: ry * baseViewport.height,
        w: Math.max(1, (rx1 - rx) * baseViewport.width),
        h: Math.max(1, (ry1 - ry) * baseViewport.height),
      };
    }
    const targetW = regionRect ? regionRect.w : baseViewport.width;
    const targetH = regionRect ? regionRect.h : baseViewport.height;
    const scale = Math.min(cw / targetW, ch / targetH);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const viewport = (pdfPage as any).getViewport({ scale });
    canvas.width = Math.floor((regionRect ? regionRect.w : viewport.width) * (scale / scale));
    // Simpler: pin canvas to the region rect (or full page) at `scale`.
    const outW = (regionRect ? regionRect.w : baseViewport.width) * scale;
    const outH = (regionRect ? regionRect.h : baseViewport.height) * scale;
    canvas.width = Math.floor(outW);
    canvas.height = Math.floor(outH);
    canvas.style.width = `${outW / dpr}px`;
    canvas.style.height = `${outH / dpr}px`;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (regionRect) {
      ctx.save();
      ctx.translate(-regionRect.x * scale, -regionRect.y * scale);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const task = (pdfPage as any).render({ canvasContext: ctx, viewport });
    renderTask = task;
    try {
      await task.promise;
    } catch {
      // cancelled: harmless
    } finally {
      if (regionRect) ctx.restore();
      if (myToken === renderToken) renderTask = null;
    }
  }
</script>

<div class="theater-canvas-wrap">
  {#if loadErr}
    <div class="err" role="alert">
      <div class="err-title">Couldn’t load PDF</div>
      <div class="err-body">{loadErr}</div>
    </div>
  {/if}
  <canvas bind:this={canvas} class="theater-canvas"></canvas>
</div>

<style>
  .theater-canvas-wrap {
    position: relative;
    inset: 0;
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    background: transparent;
    overflow: hidden;
  }
  .theater-canvas {
    display: block;
    image-rendering: -webkit-optimize-contrast;
    image-rendering: crisp-edges;
    background: #fff;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.35);
  }
  .err {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.85);
    color: #ffb4b4;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    padding: 24px;
    text-align: center;
    z-index: 2;
  }
  .err-title {
    font-size: 18px;
    font-weight: 600;
    margin-bottom: 8px;
  }
  .err-body {
    font-size: 13px;
    opacity: 0.8;
    max-width: 60ch;
  }
</style>
