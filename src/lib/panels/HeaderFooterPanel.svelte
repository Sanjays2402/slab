<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Align = "left" | "center" | "right";

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);

  let header = $state("");
  let footer = $state("Page {n} of {total}");
  let headerAlign = $state<Align>("center");
  let footerAlign = $state<Align>("center");
  let fontSize = $state(10);
  let margin = $state(24);
  let gray = $state(0.3);
  let pagesText = $state("");

  const today = new Date().toISOString().slice(0, 10);

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

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
  }

  async function run() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (!header.trim() && !footer.trim()) {
      status = { kind: "err", msg: "Provide a header or a footer." };
      return;
    }
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-hf.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Stamping…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_header_footer", {
        input,
        output,
        opts: {
          header: header.trim() ? header : null,
          header_align: headerAlign,
          footer: footer.trim() ? footer : null,
          footer_align: footerAlign,
          font_size: fontSize,
          margin,
          gray,
          filename: stripExt(basename(input)),
          date: today,
          pages: parsePages(pagesText),
        },
      });
      status =
        res.kind === "ok"
          ? { kind: "ok", msg: `Stamped ${res.value} pages → ${basename(output)}` }
          : { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function alignBtn(current: Align, target: Align): string {
    return current === target ? "active" : "";
  }
</script>

<header class="content-header">
  <h1>Headers & Footers</h1>
  <p class="subtitle">Add a header and footer band on every page. Templates supported.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">We'll stamp without re-rasterizing.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Ready to stamp</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="row">
      <label class="field grow">
        <span class="field-label">Header</span>
        <input type="text" bind:value={header} placeholder={"Optional. e.g. Slab — {filename}"} />
      </label>
      <div class="align-seg">
        <button class={alignBtn(headerAlign, "left")} onclick={() => (headerAlign = "left")}>⟸</button
        >
        <button class={alignBtn(headerAlign, "center")} onclick={() => (headerAlign = "center")}>—</button
        >
        <button class={alignBtn(headerAlign, "right")} onclick={() => (headerAlign = "right")}>⟹</button
        >
      </div>
    </div>

    <div class="row">
      <label class="field grow">
        <span class="field-label">Footer</span>
        <input type="text" bind:value={footer} placeholder={"e.g. Page {n} of {total}"} />
      </label>
      <div class="align-seg">
        <button class={alignBtn(footerAlign, "left")} onclick={() => (footerAlign = "left")}>⟸</button
        >
        <button class={alignBtn(footerAlign, "center")} onclick={() => (footerAlign = "center")}>—</button
        >
        <button class={alignBtn(footerAlign, "right")} onclick={() => (footerAlign = "right")}>⟹</button
        >
      </div>
    </div>

    <p class="hint">
      Placeholders: <code>{`{n}`}</code> · <code>{`{total}`}</code> ·
      <code>{`{date}`}</code> · <code>{`{filename}`}</code>.
    </p>

    <div class="grid">
      <label class="field">
        <span class="field-label">Font size: {fontSize}pt</span>
        <input type="range" min="6" max="24" bind:value={fontSize} />
      </label>
      <label class="field">
        <span class="field-label">Margin: {margin}pt</span>
        <input type="range" min="12" max="72" bind:value={margin} />
      </label>
      <label class="field">
        <span class="field-label">Gray: {gray.toFixed(2)}</span>
        <input type="range" min="0" max="1" step="0.05" bind:value={gray} />
      </label>
      <label class="field">
        <span class="field-label">Pages (blank = all)</span>
        <input type="text" bind:value={pagesText} placeholder="1,3,5-9" />
      </label>
    </div>

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working" ? status.msg : "Stamp"}
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
  .row {
    display: flex;
    align-items: flex-end;
    gap: 12px;
  }
  .grow {
    flex: 1;
  }
  .align-seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    overflow: hidden;
    height: 36px;
  }
  .align-seg button {
    width: 36px;
    background: var(--bg-1);
    color: var(--text-2);
    border: none;
    cursor: pointer;
  }
  .align-seg button.active {
    background: var(--accent);
    color: white;
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
    margin: 4px 0 0;
  }
  .hint code {
    background: var(--bg-2);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .row {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
