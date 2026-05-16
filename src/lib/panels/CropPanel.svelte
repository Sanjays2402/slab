<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);

  // Crop edges as percentages of the page; matches the backend opts.
  let leftPct = $state(5);
  let bottomPct = $state(5);
  let rightPct = $state(95);
  let topPct = $state(95);
  let alsoResize = $state(true);
  let pagesText = $state("");

  let widthPct = $derived(Math.max(0, rightPct - leftPct));
  let heightPct = $derived(Math.max(0, topPct - bottomPct));

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
  }

  function parsePages(s: string): number[] {
    const out: number[] = [];
    for (const part of s.split(",")) {
      const p = part.trim();
      if (!p) continue;
      if (p.includes("-")) {
        const [a, b] = p.split("-").map((x) => parseInt(x.trim(), 10));
        if (Number.isFinite(a) && Number.isFinite(b)) {
          for (let i = Math.min(a, b); i <= Math.max(a, b); i++) out.push(i);
        }
      } else {
        const n = parseInt(p, 10);
        if (Number.isFinite(n)) out.push(n);
      }
    }
    return out;
  }

  async function run() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (widthPct < 1 || heightPct < 1) {
      status = { kind: "err", msg: "Crop area is empty." };
      return;
    }
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-cropped.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Cropping…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_crop", {
        input,
        output,
        opts: {
          left_pct: leftPct,
          bottom_pct: bottomPct,
          right_pct: rightPct,
          top_pct: topPct,
          also_resize_media: alsoResize,
        },
        pages: parsePages(pagesText),
      });
      status =
        res.kind === "ok"
          ? { kind: "ok", msg: `Cropped ${res.value} pages → ${basename(output)}` }
          : { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Crop</h1>
  <p class="subtitle">Trim margins, kill bleed. Drag the edges or punch in numbers.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">We'll crop without re-rasterizing.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Ready to crop</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="crop-preview">
      <div class="page-bg">
        <div
          class="crop-rect"
          style="left:{leftPct}%; bottom:{bottomPct}%; width:{widthPct}%; height:{heightPct}%;"
        ></div>
      </div>
      <div class="crop-readout">
        {widthPct.toFixed(0)}% × {heightPct.toFixed(0)}%
      </div>
    </div>

    <div class="grid">
      <label class="field">
        <span class="field-label">Left: {leftPct.toFixed(0)}%</span>
        <input type="range" min="0" max="100" step="1" bind:value={leftPct} />
      </label>
      <label class="field">
        <span class="field-label">Right: {rightPct.toFixed(0)}%</span>
        <input type="range" min="0" max="100" step="1" bind:value={rightPct} />
      </label>
      <label class="field">
        <span class="field-label">Bottom: {bottomPct.toFixed(0)}%</span>
        <input type="range" min="0" max="100" step="1" bind:value={bottomPct} />
      </label>
      <label class="field">
        <span class="field-label">Top: {topPct.toFixed(0)}%</span>
        <input type="range" min="0" max="100" step="1" bind:value={topPct} />
      </label>
    </div>

    <label class="field">
      <span class="field-label">Pages (blank = all)</span>
      <input type="text" bind:value={pagesText} placeholder="1,3,5-9" />
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={alsoResize} />
      <span>Also shrink MediaBox (recommended for printing)</span>
    </label>

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working" ? status.msg : "Crop pages"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 16px;
  }
  .crop-preview {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .page-bg {
    position: relative;
    background:
      repeating-linear-gradient(
        45deg,
        var(--bg-2),
        var(--bg-2) 6px,
        var(--bg-1) 6px,
        var(--bg-1) 12px
      );
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    aspect-ratio: 8.5 / 11;
    max-width: 240px;
  }
  .crop-rect {
    position: absolute;
    background: rgba(80, 160, 255, 0.18);
    border: 2px dashed var(--accent);
  }
  .crop-readout {
    font-size: 11px;
    color: var(--text-3);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-2);
  }
  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
