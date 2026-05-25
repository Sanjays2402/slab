<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";
  import { isInTauri } from "$lib/tauri";

  const inTauri = isInTauri();

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let text = $state("CONFIDENTIAL");
  let opacity = $state(0.25);
  let fontSize = $state(72);
  let rotation = $state(45);
  let gray = $state(0.55);
  let scope = $state<"all" | "list">("all");
  let pagesText = $state("");
  let status = $state<Status>(idle);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
    const res = await invoke<CmdResult<number>>("slab_page_count", { input: picked });
    pageCount = res.kind === "ok" ? res.value : null;
  }

  function parsePageList(text: string): number[] | string {
    const out: number[] = [];
    const parts = text.split(",").map((s) => s.trim()).filter(Boolean);
    if (parts.length === 0) return "Add at least one page number.";
    for (const p of parts) {
      const m = p.match(/^(\d+)(?:-(\d+))?$/);
      if (!m) return `Invalid: "${p}"`;
      const a = parseInt(m[1], 10);
      const b = m[2] ? parseInt(m[2], 10) : a;
      if (a < 1 || b < a) return `Invalid: "${p}"`;
      for (let i = a; i <= b; i++) out.push(i);
    }
    return out;
  }

  async function run() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (!text.trim()) {
      status = { kind: "err", msg: "Watermark text required." };
      return;
    }

    let pages: number[];
    if (scope === "all") {
      if (pageCount === null) {
        status = { kind: "err", msg: "Page count unknown." };
        return;
      }
      pages = Array.from({ length: pageCount }, (_, i) => i + 1);
    } else {
      const parsed = parsePageList(pagesText);
      if (typeof parsed === "string") {
        status = { kind: "err", msg: parsed };
        return;
      }
      pages = parsed;
    }

    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-watermarked.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Stamping watermark…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_watermark", {
        input,
        output,
        opts: {
          text,
          opacity,
          font_size: fontSize,
          rotation_deg: rotation,
          gray,
        },
        pages,
      });
      if (res.kind === "ok") {
        status = {
          kind: "ok",
          msg: `Stamped ${res.value} page(s) → ${basename(output)}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Watermark</h1>
  <p class="subtitle">Stamp diagonal text across pages — DRAFT, CONFIDENTIAL, your name.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput} disabled={!inTauri}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Then dial in the stamp.</span>
    </button>
    {#if !inTauri}
      <div class="note">
        Watermark stamping needs the Slab desktop app — the web preview can&rsquo;t
        open local files or write PDFs. The controls below still preview how the
        stamp will look; open this panel in the installed Slab app to apply it.
      </div>
    {/if}
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if pageCount !== null}{pageCount} page{pageCount === 1 ? "" : "s"}{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <label class="field">
      <span class="field-label">Text</span>
      <input type="text" bind:value={text} placeholder="CONFIDENTIAL" />
    </label>

    <div class="grid-2">
      <label class="field">
        <span class="field-label">Opacity <span class="num">{opacity.toFixed(2)}</span></span>
        <input type="range" min="0.05" max="1" step="0.05" bind:value={opacity} />
      </label>
      <label class="field">
        <span class="field-label">Font size <span class="num">{fontSize}pt</span></span>
        <input type="range" min="24" max="144" step="4" bind:value={fontSize} />
      </label>
      <label class="field">
        <span class="field-label">Rotation <span class="num">{rotation}°</span></span>
        <input type="range" min="-90" max="90" step="5" bind:value={rotation} />
      </label>
      <label class="field">
        <span class="field-label">Gray <span class="num">{gray.toFixed(2)}</span></span>
        <input type="range" min="0" max="1" step="0.05" bind:value={gray} />
      </label>
    </div>

    <div class="preview-stamp">
      <span
        style="
          opacity: {opacity};
          transform: rotate({rotation}deg);
          color: rgb({Math.round(gray * 255)}, {Math.round(gray * 255)}, {Math.round(gray * 255)});
          font-size: {Math.max(18, fontSize / 3)}px;
        "
      >
        {text || "WATERMARK"}
      </span>
    </div>

    <div class="tabs">
      <button class:tab-active={scope === "all"} onclick={() => (scope = "all")}>All pages</button>
      <button class:tab-active={scope === "list"} onclick={() => (scope = "list")}>Specific</button>
    </div>

    {#if scope === "list"}
      <label class="field">
        <span class="field-label">Pages</span>
        <input type="text" placeholder="1, 3-5" bind:value={pagesText} />
      </label>
    {/if}

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working" ? "Stamping…" : "Apply watermark"}
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
  .grid-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .num {
    font-family: var(--font-mono);
    color: var(--accent);
    font-size: 11px;
    margin-left: 6px;
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .preview-stamp {
    height: 140px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--r-md);
    background:
      linear-gradient(45deg, var(--bg-2) 25%, transparent 25%) 0 0,
      linear-gradient(-45deg, var(--bg-2) 25%, transparent 25%) 0 0,
      var(--bg);
    background-size: 16px 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .preview-stamp span {
    font-weight: 700;
    letter-spacing: 0.05em;
    white-space: nowrap;
    user-select: none;
  }
  .note {
    margin-top: 12px;
    font-size: 12px;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: var(--r-sm);
  }
</style>
