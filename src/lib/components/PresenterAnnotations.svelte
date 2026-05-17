<script lang="ts">
  // Presenter Annotations (v0.15.0 Theater Slice 3)
  //
  // Overlay layer that sits on top of the main slide canvas in
  // PresenterOverlay. Provides:
  //   - Laser pointer (fading red dot trail while pointer is over slide)
  //   - Pen (freehand ink, per-slide-persistent)
  //   - Highlighter (translucent stroke, per-slide-persistent)
  //   - Eraser (clear all ink for current slide)
  //   - Blackout / Whiteout (full-screen overlay toggled via B/W keys)
  //   - Strokes are stored in a Map<pageNum, Stroke[]> in memory and
  //     redrawn whenever the slide changes or the slide is re-rendered.
  //
  // We deliberately keep this self-contained: it owns its own canvas
  // sized to the parent stage, and exposes simple imperative methods on
  // the bound component reference (`setTool`, `eraseCurrent`, ...) so
  // the parent PresenterOverlay can hook keyboard shortcuts.

  type Tool = "none" | "laser" | "pen" | "highlighter";

  type Stroke = {
    tool: "pen" | "highlighter";
    color: string;
    width: number;
    // Normalized coordinates in [0,1] so strokes survive resize.
    points: Array<{ x: number; y: number }>;
  };

  type Props = {
    /** 1-indexed current page in the deck. */
    currentPage: number;
    /** Total page count for the picker grid. */
    pageCount: number;
    /** Click on a thumbnail tile in the picker. */
    onJumpTo?: (page: number) => void;
  };
  let { currentPage, pageCount, onJumpTo }: Props = $props();

  // ---------- Tool / mode state ----------
  let tool = $state<Tool>("none");
  let penColor = $state<string>("#ff3b3b");
  let highlighterColor = $state<string>("rgba(255, 230, 0, 0.42)");
  let penWidth = $state<number>(3);
  let highlighterWidth = $state<number>(18);
  let blackout = $state<boolean>(false);
  let whiteout = $state<boolean>(false);
  let pickerOpen = $state<boolean>(false);

  // ---------- Stroke store: per-page persistent ink ----------
  const strokesByPage = new Map<number, Stroke[]>();
  let currentStroke: Stroke | null = null;
  let drawing = $state<boolean>(false);

  // Laser pointer trail: fading positions.
  type LaserDot = { x: number; y: number; t: number };
  let laserTrail: LaserDot[] = [];
  let pointerVisible = $state<boolean>(false);
  let pointerX = $state<number>(-1);
  let pointerY = $state<number>(-1);

  // Canvas refs
  let inkCanvas = $state<HTMLCanvasElement | null>(null);
  let laserCanvas = $state<HTMLCanvasElement | null>(null);
  let wrap = $state<HTMLDivElement | null>(null);

  let raf = 0;

  // ---------- Helpers ----------
  function fitCanvases() {
    if (!wrap) return;
    const r = wrap.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.floor(r.width * dpr));
    const h = Math.max(1, Math.floor(r.height * dpr));
    for (const c of [inkCanvas, laserCanvas]) {
      if (!c) continue;
      if (c.width !== w || c.height !== h) {
        c.width = w;
        c.height = h;
      }
      c.style.width = `${r.width}px`;
      c.style.height = `${r.height}px`;
    }
    redrawInk();
  }

  function redrawInk() {
    if (!inkCanvas) return;
    const ctx = inkCanvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, inkCanvas.width, inkCanvas.height);
    const list = strokesByPage.get(currentPage) ?? [];
    for (const s of list) drawStroke(ctx, s);
    if (currentStroke) drawStroke(ctx, currentStroke);
  }

  function drawStroke(ctx: CanvasRenderingContext2D, s: Stroke) {
    if (s.points.length < 2) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.strokeStyle = s.color;
    ctx.lineWidth = s.width * dpr;
    if (s.tool === "highlighter") {
      ctx.globalCompositeOperation = "multiply";
    } else {
      ctx.globalCompositeOperation = "source-over";
    }
    ctx.beginPath();
    const W = inkCanvas?.width ?? 1;
    const H = inkCanvas?.height ?? 1;
    const p0 = s.points[0];
    ctx.moveTo(p0.x * W, p0.y * H);
    for (let i = 1; i < s.points.length; i++) {
      const p = s.points[i];
      ctx.lineTo(p.x * W, p.y * H);
    }
    ctx.stroke();
    ctx.globalCompositeOperation = "source-over";
  }

  function normPos(e: PointerEvent): { x: number; y: number } | null {
    if (!wrap) return null;
    const r = wrap.getBoundingClientRect();
    const x = (e.clientX - r.left) / r.width;
    const y = (e.clientY - r.top) / r.height;
    if (x < 0 || x > 1 || y < 0 || y > 1) return null;
    return { x, y };
  }

  // ---------- Pointer handling ----------
  function onPointerDown(e: PointerEvent) {
    if (tool === "none" || tool === "laser") return;
    const p = normPos(e);
    if (!p) return;
    drawing = true;
    (e.target as Element).setPointerCapture?.(e.pointerId);
    currentStroke = {
      tool: tool === "pen" ? "pen" : "highlighter",
      color: tool === "pen" ? penColor : highlighterColor,
      width: tool === "pen" ? penWidth : highlighterWidth,
      points: [p],
    };
    redrawInk();
  }

  function onPointerMove(e: PointerEvent) {
    const p = normPos(e);
    if (p) {
      pointerVisible = true;
      pointerX = p.x;
      pointerY = p.y;
      if (tool === "laser") {
        laserTrail.push({ x: p.x, y: p.y, t: performance.now() });
        if (laserTrail.length > 80) laserTrail.shift();
      }
    } else {
      pointerVisible = false;
    }
    if (drawing && currentStroke && p) {
      currentStroke.points.push(p);
      redrawInk();
    }
  }

  function onPointerUp() {
    if (!drawing || !currentStroke) {
      drawing = false;
      currentStroke = null;
      return;
    }
    if (currentStroke.points.length >= 2) {
      const list = strokesByPage.get(currentPage) ?? [];
      list.push(currentStroke);
      strokesByPage.set(currentPage, list);
    }
    drawing = false;
    currentStroke = null;
    redrawInk();
  }

  function onPointerLeave() {
    pointerVisible = false;
  }

  // ---------- Laser animation loop ----------
  function laserTick() {
    if (!laserCanvas) {
      raf = requestAnimationFrame(laserTick);
      return;
    }
    const ctx = laserCanvas.getContext("2d");
    if (!ctx) {
      raf = requestAnimationFrame(laserTick);
      return;
    }
    ctx.clearRect(0, 0, laserCanvas.width, laserCanvas.height);
    const now = performance.now();
    // Trail (only when in laser tool)
    if (tool === "laser") {
      laserTrail = laserTrail.filter((d) => now - d.t < 600);
      const W = laserCanvas.width;
      const H = laserCanvas.height;
      for (const d of laserTrail) {
        const age = (now - d.t) / 600;
        const alpha = Math.max(0, 0.55 * (1 - age));
        const r = 6 + (1 - age) * 10;
        const grad = ctx.createRadialGradient(d.x * W, d.y * H, 0, d.x * W, d.y * H, r);
        grad.addColorStop(0, `rgba(255, 60, 60, ${alpha})`);
        grad.addColorStop(1, `rgba(255, 60, 60, 0)`);
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(d.x * W, d.y * H, r, 0, Math.PI * 2);
        ctx.fill();
      }
      if (pointerVisible) {
        const dpr = window.devicePixelRatio || 1;
        ctx.fillStyle = "#ff2b2b";
        ctx.beginPath();
        ctx.arc(pointerX * W, pointerY * H, 8 * dpr, 0, Math.PI * 2);
        ctx.fill();
        ctx.strokeStyle = "rgba(255,255,255,0.9)";
        ctx.lineWidth = 2 * dpr;
        ctx.stroke();
      }
    }
    raf = requestAnimationFrame(laserTick);
  }

  // ---------- Lifecycle ----------
  $effect(() => {
    fitCanvases();
    raf = requestAnimationFrame(laserTick);
    const onResize = () => fitCanvases();
    window.addEventListener("resize", onResize);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", onResize);
    };
  });

  // Redraw ink whenever the slide changes.
  $effect(() => {
    // Track currentPage
    const _p = currentPage;
    void _p;
    redrawInk();
  });

  // ---------- Imperative API for parent ----------
  export function setTool(t: Tool) {
    tool = t;
    // Drop a stale stroke if any.
    drawing = false;
    currentStroke = null;
    redrawInk();
  }
  export function eraseCurrent() {
    strokesByPage.delete(currentPage);
    redrawInk();
  }
  export function eraseAll() {
    strokesByPage.clear();
    redrawInk();
  }
  export function toggleBlackout() {
    if (whiteout) whiteout = false;
    blackout = !blackout;
  }
  export function toggleWhiteout() {
    if (blackout) blackout = false;
    whiteout = !whiteout;
  }
  export function isBlackout(): boolean {
    return blackout;
  }
  export function isWhiteout(): boolean {
    return whiteout;
  }
  export function togglePicker() {
    pickerOpen = !pickerOpen;
  }
  export function closePicker() {
    pickerOpen = false;
  }
  export function isPickerOpen(): boolean {
    return pickerOpen;
  }
  export function currentTool(): Tool {
    return tool;
  }

  /** Snapshot of all strokes for export (v0.15.0 Slice 5). */
  export type ExportStroke = {
    page: number;
    tool: "pen" | "highlighter";
    color: [number, number, number];
    width_pt: number;
    points: Array<[number, number]>;
  };
  /**
   * Return every stored stroke as a flat list, ready to ship to the
   * `slab_theater_export_annotated` Tauri command. Strokes are emitted in
   * page-ascending order; on-page order matches insertion order.
   *
   * Color is parsed back to an [r,g,b] triple in [0,1]. Width is mapped
   * from screen-ish CSS pixels to PDF points (1:1 — those happen to be
   * close enough at typical zooms, and the user can always tune later).
   */
  export function getAllStrokes(): ExportStroke[] {
    const out: ExportStroke[] = [];
    const pageKeys = Array.from(strokesByPage.keys()).sort((a, b) => a - b);
    for (const page of pageKeys) {
      const list = strokesByPage.get(page) ?? [];
      for (const s of list) {
        const color = parseRgb(s.color);
        if (!color) continue;
        out.push({
          page,
          tool: s.tool,
          color,
          width_pt: s.width,
          points: s.points.map((p) => [p.x, p.y] as [number, number]),
        });
      }
    }
    return out;
  }
  /** True iff we have at least one stored stroke to export. */
  export function hasStrokes(): boolean {
    for (const list of strokesByPage.values()) {
      if (list.length > 0) return true;
    }
    return false;
  }

  /**
   * Parse a CSS color literal into an [r,g,b] triple in [0,1].
   * Supports `#rgb`, `#rrggbb`, `rgb(...)`, and `rgba(...)`. Alpha is
   * ignored (the PDF stamp uses a per-tool ExtGState for alpha).
   */
  function parseRgb(css: string): [number, number, number] | null {
    const s = css.trim();
    if (s.startsWith("#")) {
      const hex = s.slice(1);
      if (hex.length === 3) {
        const r = parseInt(hex[0] + hex[0], 16);
        const g = parseInt(hex[1] + hex[1], 16);
        const b = parseInt(hex[2] + hex[2], 16);
        return [r / 255, g / 255, b / 255];
      }
      if (hex.length === 6) {
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        return [r / 255, g / 255, b / 255];
      }
      return null;
    }
    const m = /^rgba?\(([^)]+)\)$/i.exec(s);
    if (!m) return null;
    const parts = m[1].split(",").map((x) => parseFloat(x.trim()));
    if (parts.length < 3 || parts.some((n) => Number.isNaN(n))) return null;
    return [parts[0] / 255, parts[1] / 255, parts[2] / 255];
  }

  // Picker grid math: cap at 12 cols, scrollable.
  const cols = $derived.by((): number => {
    return Math.min(12, Math.max(4, Math.ceil(Math.sqrt(Math.max(pageCount, 1)))));
  });
</script>

<div class="layer" bind:this={wrap}>
  <canvas
    class="ink"
    bind:this={inkCanvas}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onpointerleave={onPointerLeave}
    class:active={tool !== "none"}
  ></canvas>
  <canvas class="laser" bind:this={laserCanvas}></canvas>

  {#if blackout}
    <div class="blank black" aria-hidden="true"></div>
  {/if}
  {#if whiteout}
    <div class="blank white" aria-hidden="true"></div>
  {/if}

  {#if pickerOpen}
    <button
      class="picker-scrim"
      aria-label="Close slide picker"
      onclick={() => (pickerOpen = false)}
    ></button>
    <div class="picker" role="dialog" aria-label="Jump to slide">
      <div class="picker-bar">
        <span class="picker-title">Jump to slide</span>
        <span class="picker-hint">G to toggle · Esc to close</span>
      </div>
      <div
        class="picker-grid"
        style="grid-template-columns: repeat({cols}, minmax(0, 1fr));"
      >
        {#each Array.from({ length: pageCount }, (_, i) => i + 1) as n (n)}
          <button
            type="button"
            class="tile"
            class:current={n === currentPage}
            onclick={() => {
              onJumpTo?.(n);
              pickerOpen = false;
            }}
          >
            <span class="num">{n}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  canvas.ink {
    /* The ink layer captures pointer events only when a drawing tool
       is active; otherwise the layer below (the slide) handles clicks.
       The .active class is toggled by the parent through setTool. */
    touch-action: none;
  }
  canvas.ink.active {
    pointer-events: auto;
    cursor: crosshair;
  }
  .blank {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .blank.black {
    background: #000;
  }
  .blank.white {
    background: #fff;
  }

  /* Slide picker overlay */
  .picker-scrim {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    border: none;
    padding: 0;
    margin: 0;
    cursor: pointer;
    pointer-events: auto;
  }
  .picker {
    position: absolute;
    inset: 24px;
    background: #0f1115;
    border: 1px solid #1f2228;
    border-radius: 10px;
    color: #eef0f3;
    display: grid;
    grid-template-rows: auto 1fr;
    pointer-events: auto;
    box-shadow: 0 20px 80px rgba(0, 0, 0, 0.6);
  }
  .picker-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid #1f2228;
  }
  .picker-title {
    font-size: 13px;
    font-weight: 600;
    color: #c9d0db;
  }
  .picker-hint {
    font-size: 11px;
    color: #6b7384;
  }
  .picker-grid {
    display: grid;
    gap: 10px;
    padding: 14px;
    overflow: auto;
    min-height: 0;
  }
  .tile {
    aspect-ratio: 16 / 9;
    background: #1a1d24;
    border: 1px solid #2a2f37;
    border-radius: 6px;
    color: #8a92a3;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-variant-numeric: tabular-nums;
    font-size: 13px;
  }
  .tile:hover {
    border-color: #3a4150;
    color: #fff;
    background: #20242c;
  }
  .tile.current {
    border-color: #4ade80;
    color: #4ade80;
    background: rgba(74, 222, 128, 0.08);
  }
  .num {
    pointer-events: none;
  }
</style>
