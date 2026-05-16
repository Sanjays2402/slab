<script lang="ts">
  // Annotation layer for the PDF reader.
  //
  // Two modes:
  //   * "highlight" — user selects text in the pdf.js text layer, mouseup
  //     triggers a sweep: each client rect of the selection becomes a quad
  //     in PDF user space (via `viewport.convertToPdfPoint()`).
  //   * "note" — user clicks anywhere on a page, we prompt for text, and
  //     drop a sticky-note annotation at that PDF coord.
  //
  // Pending annotations are previewed inline (yellow div overlay) and listed
  // in a side panel. "Save" calls the `slab_append_annotations` Tauri
  // command, writing the input PDF to a chosen output path with the new
  // /Annot dicts attached.

  import { invoke } from "@tauri-apps/api/core";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";

  export type AnnotMode = "off" | "highlight" | "note";

  export type PendingAnnotation =
    | {
        kind: "highlight";
        page_index: number;
        quads: number[][]; // each is 8 floats
        contents: string;
        author: string;
      }
    | {
        kind: "note";
        page_index: number;
        xy: [number, number];
        contents: string;
        author: string;
      };

  type Props = {
    /** Path of the PDF currently open in the reader (input for save). */
    path: string;
    /** The pdfjs PDFViewer instance — we read page viewports off it. */
    viewer: any;
    /** The viewer's outer scroll container (used to walk page elements). */
    viewerEl: HTMLElement | null;
    /** Annotation tool currently active. */
    mode: AnnotMode;
    /** Called when the user saves a file; the parent reloads from the path. */
    onsaved?: (path: string) => void;
    /** Called when the layer wants to switch the mode off (e.g. ESC). */
    onmodechange?: (m: AnnotMode) => void;
  };

  const { path, viewer, viewerEl, mode, onsaved, onmodechange }: Props = $props();

  let pending: PendingAnnotation[] = $state([]);
  let saving = $state(false);
  let savingError = $state<string | null>(null);

  // ---- coordinate helpers ----

  /** Find the .page element holding a given DOM node, and its 1-based page#. */
  function pageElForNode(node: Node | null): { pageEl: HTMLElement; pageNumber: number } | null {
    if (!viewerEl) return null;
    let el: HTMLElement | null = node?.nodeType === Node.ELEMENT_NODE
      ? (node as HTMLElement)
      : (node?.parentElement ?? null);
    while (el && !el.classList?.contains("page")) {
      el = el.parentElement;
    }
    if (!el) return null;
    const num = parseInt(el.getAttribute("data-page-number") ?? "0", 10);
    if (!num) return null;
    return { pageEl: el, pageNumber: num };
  }

  /** Get the pdf.js page view (with viewport) for a 1-based page#. */
  function pageView(pageNumber: number): any | null {
    if (!viewer) return null;
    try {
      return viewer.getPageView(pageNumber - 1) ?? null;
    } catch {
      return null;
    }
  }

  /** Convert a DOMRect (in client coords) to PDF user-space [x1,y1,x2,y2]. */
  function clientRectToPdf(
    pageEl: HTMLElement,
    pv: any,
    r: DOMRect,
  ): [number, number, number, number] | null {
    if (!pv?.viewport) return null;
    const pr = pageEl.getBoundingClientRect();
    // CSS pixels within the page, top-left origin.
    const x1 = r.left - pr.left;
    const y1 = r.top - pr.top;
    const x2 = r.right - pr.left;
    const y2 = r.bottom - pr.top;
    // pdfjs convertToPdfPoint takes view coords (which are the same as CSS
    // px within the page rect at the current scale) and returns PDF points.
    const [pdfX1, pdfY2] = pv.viewport.convertToPdfPoint(x1, y2); // bottom-left
    const [pdfX2, pdfY1] = pv.viewport.convertToPdfPoint(x2, y1); // top-right
    return [pdfX1, pdfY1, pdfX2, pdfY2];
  }

  // ---- highlight mode (selection → quads) ----

  function handleMouseUp(ev: MouseEvent) {
    if (mode !== "highlight") return;
    // Skip if the click landed outside the viewer scroll region.
    if (!viewerEl || !viewerEl.contains(ev.target as Node)) return;

    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;

    const range = sel.getRangeAt(0);
    const rects = Array.from(range.getClientRects()).filter(
      (r) => r.width > 1 && r.height > 1,
    );
    if (!rects.length) return;

    // Group rects by the page they belong to (selections can span pages).
    const groups = new Map<number, { pageEl: HTMLElement; rects: DOMRect[] }>();
    for (const r of rects) {
      const pt = document.elementFromPoint(r.left + 1, r.top + 1);
      const info = pageElForNode(pt);
      if (!info) continue;
      const slot = groups.get(info.pageNumber);
      if (slot) slot.rects.push(r);
      else groups.set(info.pageNumber, { pageEl: info.pageEl, rects: [r] });
    }
    if (groups.size === 0) return;

    const newAnnots: PendingAnnotation[] = [];
    for (const [pageNumber, { pageEl, rects }] of groups) {
      const pv = pageView(pageNumber);
      if (!pv?.viewport) continue;

      const quads: number[][] = [];
      for (const r of rects) {
        const pdf = clientRectToPdf(pageEl, pv, r);
        if (!pdf) continue;
        const [x1, y1, x2, y2] = pdf;
        // QuadPoints order is TL, TR, BL, BR. y1 > y2 in PDF space.
        quads.push([x1, y1, x2, y1, x1, y2, x2, y2]);
      }
      if (!quads.length) continue;

      newAnnots.push({
        kind: "highlight",
        page_index: pageNumber - 1,
        quads,
        contents: "",
        author: "Slab",
      });
    }

    if (newAnnots.length) {
      pending = [...pending, ...newAnnots];
      sel.removeAllRanges();
    }
  }

  // ---- note mode (click → sticky) ----

  function handleClick(ev: MouseEvent) {
    if (mode !== "note") return;
    if (!viewerEl || !viewerEl.contains(ev.target as Node)) return;

    // Don't drop notes when the click lands in the text layer mid-selection.
    if (window.getSelection()?.toString()) return;

    const info = pageElForNode(ev.target as Node);
    if (!info) return;
    const pv = pageView(info.pageNumber);
    if (!pv?.viewport) return;

    const pr = info.pageEl.getBoundingClientRect();
    const cssX = ev.clientX - pr.left;
    const cssY = ev.clientY - pr.top;
    const [pdfX, pdfY] = pv.viewport.convertToPdfPoint(cssX, cssY);

    const body = window.prompt("Note text:", "");
    if (body == null) return; // user cancelled

    pending = [
      ...pending,
      {
        kind: "note",
        page_index: info.pageNumber - 1,
        xy: [pdfX, pdfY],
        contents: body,
        author: "Slab",
      },
    ];
  }

  function handleKey(ev: KeyboardEvent) {
    if (ev.key === "Escape" && mode !== "off") {
      onmodechange?.("off");
    }
  }

  $effect(() => {
    document.addEventListener("mouseup", handleMouseUp);
    document.addEventListener("click", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mouseup", handleMouseUp);
      document.removeEventListener("click", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  });

  function removePending(i: number) {
    pending = pending.filter((_, idx) => idx !== i);
  }

  function clearAll() {
    pending = [];
  }

  async function save() {
    if (pending.length === 0) return;
    savingError = null;
    saving = true;
    try {
      const target = await saveDialog({
        title: "Save annotated PDF as…",
        defaultPath: path.replace(/\.pdf$/i, "-annotated.pdf"),
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!target) {
        saving = false;
        return;
      }
      await invoke<number>("slab_append_annotations", {
        input: path,
        output: target,
        annotations: pending.map((a) =>
          a.kind === "highlight"
            ? {
                kind: "highlight",
                page_index: a.page_index,
                quads: a.quads,
                contents: a.contents,
                author: a.author,
              }
            : {
                kind: "note",
                page_index: a.page_index,
                xy: a.xy,
                contents: a.contents,
                author: a.author,
              },
        ),
      });
      pending = [];
      onsaved?.(target);
    } catch (err) {
      savingError = String(err);
    } finally {
      saving = false;
    }
  }
</script>

<div class="annot-panel" class:active={mode !== "off"}>
  <header class="annot-head">
    <h3>Annotations</h3>
    <span class="annot-mode">
      {#if mode === "highlight"}Highlight — select text{:else if mode === "note"}Note — click a page{:else}Off{/if}
    </span>
  </header>

  {#if pending.length === 0}
    <p class="annot-empty">
      No pending annotations.
      {#if mode === "off"}<br />Pick a tool to start.{/if}
    </p>
  {:else}
    <ul class="annot-list">
      {#each pending as a, i (i)}
        <li class="annot-item">
          <span class="annot-tag">{a.kind === "highlight" ? "HL" : "Note"}</span>
          <span class="annot-page">p.{a.page_index + 1}</span>
          <span class="annot-body">
            {#if a.kind === "highlight"}
              {a.quads.length} quad{a.quads.length === 1 ? "" : "s"}
            {:else}
              {a.contents || "(empty)"}
            {/if}
          </span>
          <button class="annot-remove" onclick={() => removePending(i)} title="Remove">×</button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if savingError}
    <p class="annot-error">{savingError}</p>
  {/if}

  <footer class="annot-foot">
    <button class="annot-btn ghost" onclick={clearAll} disabled={pending.length === 0 || saving}
      >Clear</button
    >
    <button class="annot-btn primary" onclick={save} disabled={pending.length === 0 || saving}>
      {saving ? "Saving…" : `Save (${pending.length})`}
    </button>
  </footer>
</div>

<style>
  .annot-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--bg-1, #1a1a1a);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 8px;
    min-width: 240px;
    max-width: 320px;
    color: var(--text, #eee);
    opacity: 0.7;
  }
  .annot-panel.active {
    opacity: 1;
    border-color: var(--accent, #f5cd47);
  }
  .annot-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .annot-head h3 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3, #888);
    font-weight: 600;
  }
  .annot-mode {
    font-size: 11px;
    color: var(--text-2, #bbb);
  }
  .annot-empty {
    margin: 0;
    font-size: 12px;
    color: var(--text-3, #888);
    line-height: 1.5;
  }
  .annot-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 240px;
    overflow-y: auto;
  }
  .annot-item {
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    gap: 6px;
    align-items: center;
    padding: 4px 6px;
    border-radius: 4px;
    font-size: 12px;
    background: var(--bg-2, #222);
  }
  .annot-tag {
    font-size: 10px;
    background: var(--accent, #f5cd47);
    color: #000;
    padding: 1px 4px;
    border-radius: 3px;
    font-weight: 600;
  }
  .annot-page {
    color: var(--text-3, #888);
    font-variant-numeric: tabular-nums;
  }
  .annot-body {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .annot-remove {
    background: transparent;
    border: none;
    color: var(--text-3, #888);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0 4px;
  }
  .annot-remove:hover {
    color: var(--text, #eee);
  }
  .annot-error {
    margin: 0;
    color: #ff6b6b;
    font-size: 12px;
  }
  .annot-foot {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .annot-btn {
    padding: 5px 10px;
    border-radius: 5px;
    font-size: 12px;
    cursor: pointer;
    border: 1px solid var(--border, #2a2a2a);
  }
  .annot-btn.ghost {
    background: transparent;
    color: var(--text-2, #bbb);
  }
  .annot-btn.primary {
    background: var(--accent, #f5cd47);
    color: #000;
    border-color: transparent;
    font-weight: 600;
  }
  .annot-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
