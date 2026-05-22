<script lang="ts">
  // PagesVisualPanel — drag-and-drop visual page manager.
  //
  // Renders every page of the open PDF as a small thumbnail via pdfjs-dist,
  // lets the user reorder them with HTML5 drag-and-drop (Shift-click for
  // multi-select; the whole selection drags as one), and exposes a right-click
  // context menu with Rotate / Delete / Duplicate / Insert blank after.
  //
  // The "Apply" button materializes the current grid order to a new PDF by
  // calling slab_reorder_pages with the computed permutation. Pending mutations
  // (rotations, deletions, blank-insertions, duplications) are batched into a
  // single "Apply" run that chains the appropriate slab_* commands sequentially.

  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";
  import { isInTauri } from "$lib/tauri";
  import { createPagesHistory, type PageOp } from "$lib/pagesHistory";

  // Arranger (v2.1.2, issue #26) — 50-deep undo/redo command stack.
  // Hotkeys (when the panel is focused / mounted):
  //   R / Shift+R       Permanent rotate selection by ±90°
  //   Delete/Backspace  Delete selection
  //   D                 Duplicate selection
  //   Cmd/Ctrl+Z        Undo last op
  //   Cmd/Ctrl+Shift+Z  Redo
  const history = createPagesHistory();
  const histStore = history.store;

  // ---- pdfjs lazy import (matches ConvertPanel's pattern) ----
  type PdfjsModule = typeof import("pdfjs-dist");
  let pdfjsLib: PdfjsModule | null = null;
  let pdfjsReady = $state(false);

  onMount(async () => {
    pdfjsLib = await import("pdfjs-dist");
    const workerUrl = (await import("pdfjs-dist/build/pdf.worker.min.mjs?url")).default;
    pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;
    pdfjsReady = true;
  });

  // ---- Each cell on the grid is one (logical) page. ----
  // `originalIndex` is the 1-based source page; multiple cells can share
  // an originalIndex (duplicate) or have originalIndex = null (blank insert).
  type Cell = {
    id: string;
    originalIndex: number | null; // null = blank insert
    rotation: 0 | 90 | 180 | 270;
    thumb: string | null; // data: URL
    width: number;
    height: number;
  };

  let input = $state<string | null>(null);
  let pdfBytes = $state<Uint8Array | null>(null);
  let pageCount = $state(0);
  let cells = $state<Cell[]>([]);
  let selection = $state<Set<string>>(new Set());
  let dragIds = $state<string[] | null>(null);
  let dragOverId = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; cellId: string } | null>(null);
  let status = $state<Status>(idle);
  let progress = $state<{ done: number; total: number } | null>(null);

  let nextCellSeq = 0;
  function newCell(originalIndex: number | null): Cell {
    return {
      id: `c${++nextCellSeq}`,
      originalIndex,
      rotation: 0,
      thumb: null,
      width: 0,
      height: 0,
    };
  }

  // ---- File pick + thumbnail render ----
  async function pickInput() {
    if (!isInTauri()) {
      status = { kind: "err", msg: "Pages (visual) needs the desktop app." };
      return;
    }
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    selection = new Set();
    menu = null;
    status = idle;
    progress = null;

    // Read bytes once; pdfjs renders into the page canvases.
    try {
      const { readFile } = await import("@tauri-apps/plugin-fs");
      pdfBytes = await readFile(picked);
    } catch (e) {
      status = { kind: "err", msg: `Couldn't read PDF: ${e}` };
      return;
    }

    await renderAllThumbs();
  }

  async function renderAllThumbs() {
    if (!pdfjsLib || !pdfBytes) return;
    status = { kind: "working", msg: "Rendering thumbnails…" };
    const task = pdfjsLib.getDocument({ data: pdfBytes.slice() });
    const doc = await task.promise;
    pageCount = doc.numPages;

    const fresh: Cell[] = [];
    for (let i = 1; i <= doc.numPages; i++) {
      fresh.push(newCell(i));
    }
    cells = fresh;
    progress = { done: 0, total: doc.numPages };

    for (let i = 1; i <= doc.numPages; i++) {
      try {
        const page = await doc.getPage(i);
        const viewport = page.getViewport({ scale: 1 });
        const targetW = 180;
        const scale = targetW / viewport.width;
        const v = page.getViewport({ scale });
        const canvas = document.createElement("canvas");
        canvas.width = Math.round(v.width);
        canvas.height = Math.round(v.height);
        const ctx = canvas.getContext("2d");
        if (!ctx) throw new Error("no 2d ctx");
        // pdfjs >= 4 accepts {canvasContext, viewport}; older releases also accepted `canvas`.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        await page.render({ canvasContext: ctx, viewport: v } as any).promise;
        const dataUrl = canvas.toDataURL("image/png");
        // mutate the matching cell (still 1-1 with originals at this point)
        const idx = cells.findIndex((c) => c.originalIndex === i);
        if (idx >= 0) {
          cells[idx] = {
            ...cells[idx],
            thumb: dataUrl,
            width: v.width,
            height: v.height,
          };
          cells = cells;
        }
        page.cleanup();
        progress = { done: i, total: doc.numPages };
      } catch (e) {
        console.error("Thumb render failed for page", i, e);
      }
    }
    await doc.destroy();
    status = idle;
    progress = null;
  }

  // ---- Selection ----
  function selectCell(e: MouseEvent, cellId: string) {
    if (menu) menu = null;
    if (e.shiftKey || e.metaKey || e.ctrlKey) {
      const next = new Set(selection);
      if (next.has(cellId)) next.delete(cellId);
      else next.add(cellId);
      selection = next;
    } else {
      selection = new Set([cellId]);
    }
  }
  function clearSelection() {
    selection = new Set();
    menu = null;
  }

  // ---- Drag-and-drop ----
  function onDragStart(e: DragEvent, cellId: string) {
    // If the dragged cell isn't selected, treat it as a single-item drag.
    let ids: string[];
    if (selection.has(cellId) && selection.size > 1) {
      ids = cells.filter((c) => selection.has(c.id)).map((c) => c.id);
    } else {
      ids = [cellId];
      selection = new Set(ids);
    }
    dragIds = ids;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", ids.join(","));
    }
  }
  function onDragOver(e: DragEvent, cellId: string) {
    if (!dragIds) return;
    e.preventDefault();
    dragOverId = cellId;
  }
  function onDragLeave(cellId: string) {
    if (dragOverId === cellId) dragOverId = null;
  }
  function onDrop(e: DragEvent, targetCellId: string) {
    e.preventDefault();
    if (!dragIds) {
      dragOverId = null;
      return;
    }
    if (dragIds.includes(targetCellId)) {
      // dropping onto itself or on a member of the moving set → no-op
      dragIds = null;
      dragOverId = null;
      return;
    }
    const moving = cells.filter((c) => dragIds!.includes(c.id));
    const remaining = cells.filter((c) => !dragIds!.includes(c.id));
    const targetIdx = remaining.findIndex((c) => c.id === targetCellId);
    if (targetIdx < 0) {
      dragIds = null;
      dragOverId = null;
      return;
    }
    const next = [
      ...remaining.slice(0, targetIdx),
      ...moving,
      ...remaining.slice(targetIdx),
    ];
    cells = next;
    dragIds = null;
    dragOverId = null;
  }
  function onDragEnd() {
    dragIds = null;
    dragOverId = null;
  }

  // ---- Context menu actions ----
  function openMenu(e: MouseEvent, cellId: string) {
    e.preventDefault();
    if (!selection.has(cellId)) {
      selection = new Set([cellId]);
    }
    menu = { x: e.clientX, y: e.clientY, cellId };
  }
  function closeMenu() {
    menu = null;
  }
  function recordOp(op: PageOp) {
    history.push(op);
  }

  // ⭐ WOW — 280ms cubic-bezier rotate tilt with cascading gold-accent halo
  // on neighbours (40ms stagger). Respects prefers-reduced-motion.
  function animateRotate(cellIndex: number) {
    if (typeof window === "undefined") return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const root = document.querySelector(`[data-page="${cellIndex}"]`);
    if (!root) return;
    root.classList.add("slab-rotate-tilt");
    window.setTimeout(() => root.classList.remove("slab-rotate-tilt"), 320);
    // Halo pulse cascades to ±1 then ±2 neighbours.
    for (let d = 1; d <= 2; d++) {
      for (const sign of [-1, 1]) {
        const n = document.querySelector(`[data-page="${cellIndex + sign * d}"]`);
        if (!n) continue;
        window.setTimeout(() => {
          n.classList.add("slab-halo-pulse");
          window.setTimeout(() => n.classList.remove("slab-halo-pulse"), 280);
        }, d * 40);
      }
    }
  }

  function animateDelete(cellId: string) {
    if (typeof window === "undefined") return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const el = document.querySelector(`[data-cell-id="${cellId}"]`);
    if (!el) return;
    el.classList.add("slab-delete-out");
  }

  function actionRotate(deg: 90 | 180 | 270) {
    // Record one op per selected cell so undo backs them out one at a time.
    let firstAt: number | null = null;
    const next = cells.map((c, idx) => {
      if (!selection.has(c.id)) return c;
      const r = ((c.rotation + deg) % 360) as Cell["rotation"];
      if (firstAt === null) firstAt = idx + 1;
      recordOp({ kind: "rotate", at: idx + 1, degrees: deg });
      return { ...c, rotation: r };
    });
    cells = next;
    if (firstAt !== null) animateRotate(firstAt);
    closeMenu();
  }
  function actionDelete() {
    if (selection.size === cells.length) {
      status = { kind: "err", msg: "Can't delete every page." };
      closeMenu();
      return;
    }
    // Walk in reverse so positions stay valid as ops record.
    const idxs = cells
      .map((c, i) => ({ id: c.id, idx: i + 1, selected: selection.has(c.id) }))
      .filter((x) => x.selected)
      .sort((a, b) => b.idx - a.idx);
    for (const x of idxs) {
      recordOp({ kind: "delete", at: x.idx });
      animateDelete(x.id);
    }
    cells = cells.filter((c) => !selection.has(c.id));
    selection = new Set();
    closeMenu();
  }
  function actionDuplicate() {
    if (selection.size === 0) return;
    const next: Cell[] = [];
    cells.forEach((c, i) => {
      next.push(c);
      if (selection.has(c.id)) {
        const copy = newCell(c.originalIndex);
        copy.thumb = c.thumb;
        copy.width = c.width;
        copy.height = c.height;
        copy.rotation = c.rotation;
        next.push(copy);
        recordOp({ kind: "duplicate", from: i + 1, to: i + 2 });
      }
    });
    cells = next;
    closeMenu();
  }
  function actionInsertBlank() {
    if (selection.size === 0) return;
    const next: Cell[] = [];
    for (const c of cells) {
      next.push(c);
      if (selection.has(c.id)) {
        const blank = newCell(null);
        // Use the average dimensions of currently rendered cells, fall back to letter.
        const sized = cells.find((x) => x.width > 0);
        blank.width = sized?.width ?? 180;
        blank.height = sized?.height ?? 233;
        next.push(blank);
      }
    }
    cells = next;
    closeMenu();
  }
  function actionReset() {
    selection = new Set();
    menu = null;
    cells = cells
      .filter((c) => c.originalIndex !== null)
      .sort((a, b) => (a.originalIndex ?? 0) - (b.originalIndex ?? 0))
      .map((c) => ({ ...c, rotation: 0 }));
    history.clear();
  }

  // ---- Undo / Redo: replays history.applied() against the original page set ----
  function reverseLast() {
    const op = history.undo();
    if (!op) return;
    replayHistory();
    if (op.kind === "rotate") animateRotate(Math.min(op.at, cells.length));
  }
  function replayForward() {
    const op = history.redo();
    if (!op) return;
    replayHistory();
    if (op.kind === "rotate") animateRotate(Math.min(op.at, cells.length));
  }

  /** Rebuild `cells` from scratch by replaying every applied op. */
  function replayHistory() {
    // Start from the original page sequence preserving rendered thumbs.
    const origByIdx = new Map<number, Cell>();
    for (const c of cells) {
      if (c.originalIndex !== null && !origByIdx.has(c.originalIndex)) {
        origByIdx.set(c.originalIndex, { ...c, rotation: 0 });
      }
    }
    let next: Cell[] = [];
    for (let i = 1; i <= pageCount; i++) {
      const tmpl = origByIdx.get(i);
      next.push(tmpl ? { ...tmpl, id: `c${++nextCellSeq}`, rotation: 0 } : newCell(i));
    }
    for (const op of history.applied()) {
      if (op.kind === "rotate") {
        const k = op.at - 1;
        if (k >= 0 && k < next.length) {
          const r = ((next[k].rotation + op.degrees) % 360) as Cell["rotation"];
          next[k] = { ...next[k], rotation: r };
        }
      } else if (op.kind === "delete") {
        const k = op.at - 1;
        if (k >= 0 && k < next.length) next.splice(k, 1);
      } else if (op.kind === "duplicate") {
        const k = op.from - 1;
        if (k >= 0 && k < next.length) {
          const src = next[k];
          const copy = { ...src, id: `c${++nextCellSeq}` };
          next.splice(op.to - 1, 0, copy);
        }
      } else if (op.kind === "reorder") {
        const ordered: Cell[] = [];
        for (const idx of op.order) {
          if (idx - 1 >= 0 && idx - 1 < next.length) ordered.push(next[idx - 1]);
        }
        next = ordered;
      } else if (op.kind === "insert_blank") {
        for (let n = 0; n < op.count; n++) {
          const blank = newCell(null);
          blank.width = op.width;
          blank.height = op.height;
          next.splice(op.at - 1 + n, 0, blank);
        }
      }
      // insert_pdf / insert_image are persisted server-side only — their
      // visual rehearsal here would require a re-render which is out of
      // scope for the undo replay loop. They flush on Apply.
    }
    cells = next;
    selection = new Set();
  }

  // ---- Apply: write a new PDF reflecting the current grid ----
  // One Tauri round-trip via slab_pages_build. The kernel handles
  // permutations, duplicates, blank inserts, and per-cell rotation in
  // a single composite operation — no chained intermediate files visible
  // to the user.
  async function applyChanges() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (cells.length === 0) {
      status = { kind: "err", msg: "No cells to apply." };
      return;
    }
    if (cells.every((c) => c.originalIndex === null)) {
      status = { kind: "err", msg: "Layout has no source pages." };
      return;
    }

    const base = stripExt(basename(input));
    const out = await save({
      defaultPath: `${base}-rearranged.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof out !== "string") return;

    status = { kind: "working", msg: "Applying changes…" };
    progress = null;

    // Pick a blank size: average of currently-rendered cells, fall back to Letter.
    const sized = cells.find((c) => c.width > 0 && c.height > 0);
    // Cells store thumbnail pixel sizes — to get PDF points we need the
    // original page's viewport. We rendered at scale = 180 / vw, so
    // points-per-pixel = vw / 180; multiplying cell.width (px) by that
    // gives back the original viewport width in PDF points. Approximate
    // with `cell.width * (612 / 180)` since the actual ratio is the same
    // regardless of source page — but easier: read the first sized cell
    // and treat its (w,h) as Letter-equivalent by ratio.
    // To keep this honest, just pass Letter when we can't pin it down.
    const blank = sized
      ? { width: 612, height: Math.round((612 / sized.width) * sized.height) }
      : { width: 612, height: 792 };

    const payload = {
      input,
      opts: {
        cells: cells.map((c) => ({
          source: c.originalIndex, // null = blank
          rotation: c.rotation,
        })),
        blank,
      },
      output: out,
    };

    try {
      const res = await invoke<CmdResult<number>>("slab_pages_build", payload);
      if (res.kind !== "ok") {
        status = { kind: "err", msg: res.message };
        return;
      }
      status = { kind: "ok", msg: `Saved ${res.value} pages → ${basename(out)}` };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      progress = null;
    }
  }

  // Close menu on outside click / Escape
  function onGlobalClick(e: MouseEvent) {
    if (!menu) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest(".ctx-menu")) return;
    menu = null;
  }
  function onGlobalKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      menu = null;
      selection = new Set();
      return;
    }
    // Don't hijack typing in inputs / textareas / contenteditable.
    const tgt = e.target as HTMLElement | null;
    if (
      tgt &&
      (tgt.tagName === "INPUT" ||
        tgt.tagName === "TEXTAREA" ||
        tgt.isContentEditable)
    ) {
      return;
    }
    // Only act when this panel has loaded a doc.
    if (!input || cells.length === 0) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod && (e.key === "z" || e.key === "Z")) {
      e.preventDefault();
      if (e.shiftKey) replayForward();
      else reverseLast();
      return;
    }
    if (!mod && (e.key === "r" || e.key === "R")) {
      if (selection.size === 0) return;
      e.preventDefault();
      actionRotate(e.shiftKey ? 270 : 90);
      return;
    }
    if (!mod && (e.key === "d" || e.key === "D")) {
      if (selection.size === 0) return;
      e.preventDefault();
      actionDuplicate();
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      if (selection.size === 0) return;
      e.preventDefault();
      actionDelete();
      return;
    }
  }
  onMount(() => {
    window.addEventListener("click", onGlobalClick);
    window.addEventListener("keydown", onGlobalKey);
    return () => {
      window.removeEventListener("click", onGlobalClick);
      window.removeEventListener("keydown", onGlobalKey);
    };
  });
</script>

<header class="content-header">
  <h1>Pages</h1>
  <p class="subtitle">
    Drag thumbnails to reorder. Shift-click for multi-select. Right-click for more
    actions.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput} disabled={!pdfjsReady}>
      <span class="dz-icon">+</span>
      <span class="dz-title"
        >{pdfjsReady ? "Choose a PDF" : "Loading PDF.js…"}</span
      >
      <span class="dz-hint"
        >Then drag pages around, rotate, delete, or duplicate.</span
      >
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {pageCount} source page{pageCount === 1 ? "" : "s"} ·
          {cells.length} cell{cells.length === 1 ? "" : "s"}
          {#if selection.size > 0}· {selection.size} selected{/if}
        </div>
      </div>
      <div class="file-actions">
        <button class="ghost" onclick={actionReset} title="Reset to original order"
          >Reset</button
        >
        <button class="ghost" onclick={pickInput}>Change</button>
      </div>
    </div>

    {#if progress}
      <div class="progress">
        Rendering {progress.done} / {progress.total}…
      </div>
    {/if}

    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="grid" onclick={clearSelection}>
      {#each cells as cell, i (cell.id)}
        <div
          class="cell"
          data-page={i + 1}
          data-cell-id={cell.id}
          class:selected={selection.has(cell.id)}
          class:drag-over={dragOverId === cell.id}
          class:dragging={dragIds?.includes(cell.id) ?? false}
          draggable="true"
          ondragstart={(e) => onDragStart(e, cell.id)}
          ondragover={(e) => onDragOver(e, cell.id)}
          ondragleave={() => onDragLeave(cell.id)}
          ondrop={(e) => onDrop(e, cell.id)}
          ondragend={onDragEnd}
          onclick={(e) => {
            e.stopPropagation();
            selectCell(e, cell.id);
          }}
          oncontextmenu={(e) => openMenu(e, cell.id)}
        >
          <div class="thumb-wrap">
            {#if cell.originalIndex === null}
              <div class="blank">blank</div>
            {:else if cell.thumb}
              <img
                src={cell.thumb}
                alt={`Page ${cell.originalIndex}`}
                style={`transform: rotate(${cell.rotation}deg);`}
              />
            {:else}
              <div class="loading">…</div>
            {/if}
          </div>
          <div class="cell-label">
            <span class="cell-index">{i + 1}</span>
            {#if cell.originalIndex !== null}
              <span class="cell-source">p{cell.originalIndex}</span>
            {/if}
            {#if cell.rotation !== 0}
              <span class="cell-rot">{cell.rotation}°</span>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    <div class="actions">
      <button
        class="ghost"
        onclick={reverseLast}
        disabled={!$histStore.canUndo}
        title={$histStore.undoLabel ? `Undo ${$histStore.undoLabel} (⌘Z)` : "Nothing to undo"}
      >
        ↶ Undo
      </button>
      <button
        class="ghost"
        onclick={replayForward}
        disabled={!$histStore.canRedo}
        title={$histStore.redoLabel ? `Redo ${$histStore.redoLabel} (⌘⇧Z)` : "Nothing to redo"}
      >
        ↷ Redo
      </button>
      <button
        class="primary"
        onclick={applyChanges}
        disabled={status.kind === "working" || cells.length === 0}
      >
        {status.kind === "working" ? "Working…" : "Apply → save copy"}
      </button>
      <span class="hint">
        <kbd>R</kbd> rotate · <kbd>D</kbd> duplicate · <kbd>⌫</kbd> delete · <kbd>⌘Z</kbd> undo
      </span>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

{#if menu}
  <div class="ctx-menu" style={`left: ${menu.x}px; top: ${menu.y}px;`}>
    <button onclick={() => actionRotate(90)}>Rotate 90° ↻</button>
    <button onclick={() => actionRotate(180)}>Rotate 180°</button>
    <button onclick={() => actionRotate(270)}>Rotate 270° ↺</button>
    <hr />
    <button onclick={actionDuplicate}>Duplicate</button>
    <button onclick={actionInsertBlank}>Insert blank after</button>
    <button class="danger" onclick={actionDelete}>Delete</button>
  </div>
{/if}

<style>
  /* ⭐ Arranger WOW — rotate-tilt + halo pulse cascade (v2.1.2, issue #26).
     280ms cubic-bezier with gold-accent halo cascading across neighbours.
     Honours prefers-reduced-motion. */
  .cell.slab-rotate-tilt {
    animation: slab-rotate-tilt 280ms cubic-bezier(0.34, 1.56, 0.64, 1);
    transform-origin: center center;
    z-index: 5;
  }
  @keyframes slab-rotate-tilt {
    0% {
      transform: rotate(0deg) scale(1);
      filter: none;
    }
    55% {
      transform: rotate(8deg) scale(1.06);
      filter: drop-shadow(0 0 12px rgba(212, 160, 74, 0.7));
    }
    100% {
      transform: rotate(0deg) scale(1);
      filter: none;
    }
  }
  .cell.slab-halo-pulse {
    animation: slab-halo-pulse 280ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  @keyframes slab-halo-pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(212, 160, 74, 0);
    }
    50% {
      box-shadow: 0 0 18px 4px rgba(212, 160, 74, 0.32);
    }
    100% {
      box-shadow: 0 0 0 0 rgba(212, 160, 74, 0);
    }
  }
  .cell.slab-delete-out {
    animation: slab-delete-out 220ms ease-out forwards;
    pointer-events: none;
  }
  @keyframes slab-delete-out {
    0% {
      opacity: 1;
      transform: translateX(0) scale(1);
    }
    100% {
      opacity: 0;
      transform: translateX(-32px) scale(0.85);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .cell.slab-rotate-tilt,
    .cell.slab-halo-pulse,
    .cell.slab-delete-out {
      animation: none !important;
    }
  }
  .hint kbd {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.78em;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--panel-bg-2, rgba(255, 255, 255, 0.08));
    border: 1px solid var(--panel-border, rgba(0, 0, 0, 0.12));
  }

  .content-header h1 {
    font-size: 22px;
    font-weight: 600;
    margin: 0 0 4px;
  }
  .subtitle {
    color: var(--text-3);
    font-size: 13px;
    margin: 0 0 16px;
  }
  .panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    gap: 14px;
  }
  .dropzone {
    border: 1px dashed var(--border);
    background: var(--bg);
    color: var(--text-2);
    padding: 50px;
    border-radius: var(--r);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .dropzone:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .dz-icon {
    font-size: 28px;
    color: var(--accent);
  }
  .dz-title {
    font-size: 15px;
    font-weight: 500;
  }
  .dz-hint {
    font-size: 12px;
    color: var(--text-3);
  }
  .file-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
  }
  .file-name {
    font-weight: 500;
    font-size: 13px;
  }
  .file-meta {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .file-actions {
    display: flex;
    gap: 6px;
  }
  .ghost {
    background: transparent;
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 5px 10px;
    font-size: 12px;
  }
  .ghost:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .progress {
    font-size: 12px;
    color: var(--text-3);
    padding: 0 4px;
  }
  .grid {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 14px;
    padding: 10px 4px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg);
    min-height: 200px;
  }
  .cell {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px;
    display: flex;
    flex-direction: column;
    align-items: center;
    cursor: grab;
    user-select: none;
    transition:
      transform 90ms ease,
      border-color 90ms ease,
      box-shadow 90ms ease;
  }
  .cell:hover {
    border-color: var(--text-3);
  }
  .cell.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .cell.drag-over {
    border-color: var(--accent);
    transform: translateY(-2px);
  }
  .cell.dragging {
    opacity: 0.4;
  }
  .thumb-wrap {
    width: 100%;
    aspect-ratio: 3 / 4;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #fff;
    border-radius: 3px;
    overflow: hidden;
    margin-bottom: 6px;
  }
  .thumb-wrap img {
    max-width: 100%;
    max-height: 100%;
    transition: transform 120ms ease;
  }
  .blank,
  .loading {
    color: var(--text-3);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .blank {
    background: repeating-linear-gradient(
      45deg,
      #fff,
      #fff 8px,
      #f3f4f6 8px,
      #f3f4f6 14px
    );
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .cell-label {
    display: flex;
    gap: 6px;
    align-items: baseline;
    font-size: 11px;
    color: var(--text-2);
  }
  .cell-index {
    font-weight: 600;
    color: var(--text);
  }
  .cell-source {
    color: var(--text-3);
  }
  .cell-rot {
    color: var(--accent);
    font-weight: 600;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    border: none;
    padding: 8px 16px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-weight: 500;
  }
  .primary:disabled {
    opacity: 0.6;
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
  }
  .status {
    font-size: 12px;
    padding: 8px 12px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
  }
  .status.ok {
    color: #4ade80;
    border-color: #16a34a44;
    background: #16a34a11;
  }
  .status.err {
    color: #f87171;
    border-color: #dc262644;
    background: #dc262611;
  }
  .ctx-menu {
    position: fixed;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 4px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    z-index: 1000;
    min-width: 180px;
    display: flex;
    flex-direction: column;
  }
  .ctx-menu button {
    background: transparent;
    color: var(--text);
    border: none;
    text-align: left;
    padding: 7px 12px;
    border-radius: 3px;
    font-size: 12px;
  }
  .ctx-menu button:hover {
    background: var(--bg-3);
  }
  .ctx-menu button.danger {
    color: #f87171;
  }
  .ctx-menu hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
</style>
