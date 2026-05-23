<script lang="ts">
  // v2.4.0 "Stack" — visual PDF diff. Side-by-side raster panes,
  // coral/mint change-box overlay, scroll-locked, n/N jumps between
  // change clusters. Acrobat Pro charges $239/yr for "Compare Files."
  // Slab gives it away, offline.
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  // Backend DTOs mirror src-tauri/src/pdf/visual_diff.rs.
  type ChangeBox = { x: number; y: number; w: number; h: number; mass: number };
  type VisualPage = {
    old_page: number | null;
    new_page: number | null;
    old_png_b64: string | null;
    new_png_b64: string | null;
    w: number;
    h: number;
    changes: ChangeBox[];
  };
  type VisualDiff = {
    old_path: string;
    new_path: string;
    dpi: number;
    pages: VisualPage[];
  };

  let oldPath = $state<string | null>(null);
  let newPath = $state<string | null>(null);
  let status = $state<Status>(idle);
  let diff = $state<VisualDiff | null>(null);
  let dpi = $state(150);
  let threshold = $state(20);
  let minMass = $state(8);
  let currentChangeIdx = $state(0);
  let leftPane: HTMLDivElement | null = $state(null);
  let rightPane: HTMLDivElement | null = $state(null);
  let scrollLocked = $state(true);
  let syncing = false;

  // Flat list of (pageIndex, changeIndex) across all pages — drives n/N.
  let allChanges = $derived.by(() => {
    if (!diff) return [] as Array<{ page: number; box: ChangeBox }>;
    const out: Array<{ page: number; box: ChangeBox }> = [];
    diff.pages.forEach((p, i) => p.changes.forEach((b) => out.push({ page: i, box: b })));
    return out;
  });

  let totalChanges = $derived(allChanges.length);
  let changedPages = $derived(diff ? diff.pages.filter((p) => p.changes.length > 0).length : 0);

  async function pickOld() {
    const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (typeof picked !== "string") return;
    oldPath = picked;
    diff = null;
  }

  async function pickNew() {
    const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (typeof picked !== "string") return;
    newPath = picked;
    diff = null;
  }

  async function runDiff() {
    if (!oldPath || !newPath) return;
    status = { kind: "working", msg: `Rendering at ${dpi} DPI…` };
    diff = null;
    try {
      const res = await invoke<CmdResult<VisualDiff>>("slab_visual_diff_pdfs", {
        old: oldPath,
        new: newPath,
        dpi,
        threshold,
        minMass,
      });
      if (res.kind === "ok") {
        diff = res.value;
        currentChangeIdx = 0;
        status = {
          kind: "ok",
          msg: `${diff.pages.length} pages compared — ${totalChanges} change regions across ${changedPages} pages.`,
        };
      } else {
        status = { kind: "err", msg: res.message ?? "visual diff failed" };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  // Scroll-lock: when one pane scrolls, the other follows.
  function onLeftScroll() {
    if (!scrollLocked || syncing || !leftPane || !rightPane) return;
    syncing = true;
    rightPane.scrollTop = leftPane.scrollTop;
    rightPane.scrollLeft = leftPane.scrollLeft;
    requestAnimationFrame(() => (syncing = false));
  }
  function onRightScroll() {
    if (!scrollLocked || syncing || !leftPane || !rightPane) return;
    syncing = true;
    leftPane.scrollTop = rightPane.scrollTop;
    leftPane.scrollLeft = rightPane.scrollLeft;
    requestAnimationFrame(() => (syncing = false));
  }

  function jumpToChange(idx: number) {
    if (totalChanges === 0) return;
    const wrapped = ((idx % totalChanges) + totalChanges) % totalChanges;
    currentChangeIdx = wrapped;
    const target = allChanges[wrapped];
    if (!diff || !leftPane || !rightPane) return;
    // Each page is rendered as a stack — sum heights of preceding pages.
    let offsetY = 0;
    for (let i = 0; i < target.page; i++) offsetY += diff.pages[i].h + 24; // 24 gap
    const targetY = offsetY + Math.max(0, target.box.y - 80);
    leftPane.scrollTo({ top: targetY, left: target.box.x, behavior: "smooth" });
    if (!scrollLocked) {
      rightPane.scrollTo({ top: targetY, left: target.box.x, behavior: "smooth" });
    }
  }

  function nextChange() {
    jumpToChange(currentChangeIdx + 1);
  }
  function prevChange() {
    jumpToChange(currentChangeIdx - 1);
  }

  function onKey(e: KeyboardEvent) {
    if (!diff || (e.target as HTMLElement)?.tagName === "INPUT") return;
    if (e.key === "n") {
      e.preventDefault();
      nextChange();
    } else if (e.key === "N") {
      e.preventDefault();
      prevChange();
    }
  }

  // PNG src helper.
  function src(b64: string | null): string | undefined {
    return b64 ? `data:image/png;base64,${b64}` : undefined;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="stack">
  <header class="toolbar">
    <div class="picker">
      <button class="pick" onclick={pickOld}>
        {oldPath ? basename(oldPath) : "Pick old PDF…"}
      </button>
      <span class="vs">→</span>
      <button class="pick" onclick={pickNew}>
        {newPath ? basename(newPath) : "Pick new PDF…"}
      </button>
    </div>
    <div class="knobs">
      <label
        >DPI
        <input type="number" min="36" max="300" step="6" bind:value={dpi} />
      </label>
      <label
        >Threshold
        <input type="number" min="0" max="255" bind:value={threshold} />
      </label>
      <label
        >Min mass
        <input type="number" min="1" max="500" bind:value={minMass} />
      </label>
      <label class="toggle">
        <input type="checkbox" bind:checked={scrollLocked} />
        Scroll-lock
      </label>
    </div>
    <button class="primary" onclick={runDiff} disabled={!oldPath || !newPath || status.kind === "working"}>
      {status.kind === "working" ? "Comparing…" : "Compare"}
    </button>
  </header>

  {#if status.kind === "working"}
    <p class="status busy">{status.msg}</p>
  {:else if status.kind === "ok"}
    <p class="status ok">{status.msg}</p>
  {:else if status.kind === "err"}
    <p class="status err">{status.msg}</p>
  {/if}

  {#if diff}
    <nav class="changes-bar" aria-label="Jump between changes">
      <button onclick={prevChange} disabled={totalChanges === 0} title="Previous change (N)">↑ Prev</button>
      <span class="counter">
        {totalChanges === 0 ? "No changes" : `${currentChangeIdx + 1} / ${totalChanges}`}
      </span>
      <button onclick={nextChange} disabled={totalChanges === 0} title="Next change (n)">Next ↓</button>
      <span class="spacer"></span>
      <span class="pill coral">{changedPages} pages changed</span>
      <span class="pill mint">{totalChanges} regions</span>
    </nav>

    <div class="split">
      <div class="pane left" bind:this={leftPane} onscroll={onLeftScroll}>
        <h4>Old · {basename(diff.old_path)}</h4>
        {#each diff.pages as page, i}
          <div class="page-card" style="--w:{page.w}px;--h:{page.h}px">
            {#if page.old_png_b64}
              <img src={src(page.old_png_b64)} alt="old page {i + 1}" width={page.w} height={page.h} />
              {#each page.changes as box}
                <span
                  class="box coral"
                  style="left:{box.x}px;top:{box.y}px;width:{box.w}px;height:{box.h}px"
                ></span>
              {/each}
            {:else}
              <div class="missing">Page only in new doc</div>
            {/if}
            <div class="page-label">old · page {page.old_page ?? "—"}</div>
          </div>
        {/each}
      </div>
      <div class="pane right" bind:this={rightPane} onscroll={onRightScroll}>
        <h4>New · {basename(diff.new_path)}</h4>
        {#each diff.pages as page, i}
          <div class="page-card" style="--w:{page.w}px;--h:{page.h}px">
            {#if page.new_png_b64}
              <img src={src(page.new_png_b64)} alt="new page {i + 1}" width={page.w} height={page.h} />
              {#each page.changes as box}
                <span
                  class="box mint"
                  style="left:{box.x}px;top:{box.y}px;width:{box.w}px;height:{box.h}px"
                ></span>
              {/each}
            {:else}
              <div class="missing">Page only in old doc</div>
            {/if}
            <div class="page-label">new · page {page.new_page ?? "—"}</div>
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <div class="empty">
      <h2>Visual diff for PDFs</h2>
      <p>
        Pick two revisions of the same document. Slab renders both side-by-side, highlights every changed pixel,
        and lets you jump between them with <kbd>n</kbd> / <kbd>N</kbd>.
      </p>
      <p class="hint">All offline. Files never leave your machine.</p>
    </div>
  {/if}
</div>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 12px;
    padding: 16px;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }
  .picker {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pick {
    background: var(--c-surface-2, #1c1c1f);
    border: 1px solid var(--c-border, #2a2a2e);
    color: var(--c-text, #e8e8ea);
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .pick:hover {
    background: var(--c-surface-3, #232328);
  }
  .vs {
    color: var(--c-muted, #888);
    font-weight: 600;
  }
  .knobs {
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 12px;
    color: var(--c-muted, #888);
  }
  .knobs input[type="number"] {
    width: 56px;
    margin-left: 4px;
    background: var(--c-surface-2);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    padding: 2px 6px;
    border-radius: 4px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .primary {
    background: var(--c-accent, #6c5ce7);
    color: white;
    border: 0;
    padding: 8px 16px;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .status {
    font-size: 12px;
    margin: 0;
  }
  .status.busy {
    color: var(--c-accent);
  }
  .status.ok {
    color: var(--c-success, #4ade80);
  }
  .status.err {
    color: var(--c-danger, #ef4444);
  }
  .changes-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--c-surface-2);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    font-size: 12px;
  }
  .changes-bar button {
    background: transparent;
    border: 1px solid var(--c-border);
    color: var(--c-text);
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .changes-bar button:hover:not(:disabled) {
    background: var(--c-surface-3);
  }
  .changes-bar button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .counter {
    color: var(--c-muted);
    min-width: 80px;
    text-align: center;
  }
  .spacer {
    flex: 1;
  }
  .pill {
    padding: 2px 10px;
    border-radius: 999px;
    font-weight: 600;
    font-size: 11px;
  }
  .pill.coral {
    background: rgba(255, 107, 107, 0.18);
    color: #ffb4b4;
  }
  .pill.mint {
    background: rgba(74, 222, 128, 0.18);
    color: #b3ffd0;
  }
  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    flex: 1;
    min-height: 0;
  }
  .pane {
    overflow: auto;
    background: var(--c-surface, #131316);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    padding: 12px;
  }
  .pane h4 {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--c-muted);
    font-weight: 500;
  }
  .page-card {
    position: relative;
    margin-bottom: 24px;
    background: white;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    width: var(--w);
  }
  .page-card img {
    display: block;
    width: 100%;
    height: auto;
  }
  .page-label {
    color: var(--c-muted);
    font-size: 11px;
    margin-top: 6px;
    background: var(--c-surface);
    padding: 4px 8px;
    border-radius: 4px;
    display: inline-block;
  }
  .box {
    position: absolute;
    border-radius: 2px;
    pointer-events: none;
    mix-blend-mode: multiply;
    transition: outline 240ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .box.coral {
    background: rgba(255, 107, 107, 0.25);
    outline: 2px solid rgba(255, 64, 64, 0.85);
  }
  .box.mint {
    background: rgba(74, 222, 128, 0.22);
    outline: 2px solid rgba(34, 197, 94, 0.85);
  }
  .missing {
    padding: 40px;
    text-align: center;
    color: var(--c-muted);
    font-style: italic;
    background: var(--c-surface-2);
    border-radius: 4px;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 8px;
    color: var(--c-muted);
  }
  .empty h2 {
    margin: 0;
    color: var(--c-text);
  }
  .empty p {
    max-width: 480px;
    margin: 0;
  }
  .empty .hint {
    font-size: 12px;
    opacity: 0.7;
  }
  kbd {
    background: var(--c-surface-2);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 2px 6px;
    font-size: 11px;
    font-family: ui-monospace, monospace;
  }
</style>
