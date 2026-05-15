<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Op = "rotate" | "delete" | "reorder";

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let op = $state<Op>("rotate");
  let pagesText = $state("");
  let rotation = $state<90 | 180 | 270>(90);
  let orderText = $state("");
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
    if (pageCount !== null) {
      orderText = Array.from({ length: pageCount }, (_, i) => i + 1).join(", ");
    }
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

  async function pickOutput(defaultSuffix: string): Promise<string | null> {
    if (!input) return null;
    const base = stripExt(basename(input));
    const out = await save({
      defaultPath: `${base}-${defaultSuffix}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    return typeof out === "string" ? out : null;
  }

  async function runOp() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }

    if (op === "rotate") {
      const parsed = parsePageList(pagesText);
      if (typeof parsed === "string") {
        status = { kind: "err", msg: parsed };
        return;
      }
      const output = await pickOutput(`rotated-${rotation}`);
      if (!output) return;
      status = { kind: "working", msg: `Rotating ${parsed.length} page(s) by ${rotation}°…` };
      try {
        const res = await invoke<CmdResult<number>>("slab_rotate", {
          input,
          pages: parsed,
          degrees: rotation,
          output,
        });
        if (res.kind === "ok") {
          status = { kind: "ok", msg: `Rotated ${res.value} page(s) → ${basename(output)}` };
        } else {
          status = { kind: "err", msg: res.message };
        }
      } catch (e) {
        status = { kind: "err", msg: String(e) };
      }
    } else if (op === "delete") {
      const parsed = parsePageList(pagesText);
      if (typeof parsed === "string") {
        status = { kind: "err", msg: parsed };
        return;
      }
      const output = await pickOutput("trimmed");
      if (!output) return;
      status = { kind: "working", msg: `Deleting ${parsed.length} page(s)…` };
      try {
        const res = await invoke<CmdResult<number>>("slab_delete_pages", {
          input,
          pages: parsed,
          output,
        });
        if (res.kind === "ok") {
          status = { kind: "ok", msg: `${res.value} page(s) remain → ${basename(output)}` };
        } else {
          status = { kind: "err", msg: res.message };
        }
      } catch (e) {
        status = { kind: "err", msg: String(e) };
      }
    } else {
      const parsed = parsePageList(orderText);
      if (typeof parsed === "string") {
        status = { kind: "err", msg: parsed };
        return;
      }
      const output = await pickOutput("reordered");
      if (!output) return;
      status = { kind: "working", msg: `Reordering ${parsed.length} page(s)…` };
      try {
        const res = await invoke<CmdResult<null>>("slab_reorder_pages", {
          input,
          order: parsed,
          output,
        });
        if (res.kind === "ok") {
          status = { kind: "ok", msg: `Reordered → ${basename(output)}` };
        } else {
          status = { kind: "err", msg: res.message };
        }
      } catch (e) {
        status = { kind: "err", msg: String(e) };
      }
    }
  }
</script>

<header class="content-header">
  <h1>Pages</h1>
  <p class="subtitle">Rotate, delete, or reorder pages without touching the rest.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Then pick what to do with its pages.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if pageCount !== null}{pageCount} page{pageCount === 1 ? "" : "s"}{:else}…{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="tabs">
      <button class:tab-active={op === "rotate"} onclick={() => (op = "rotate")}>Rotate</button>
      <button class:tab-active={op === "delete"} onclick={() => (op = "delete")}>Delete</button>
      <button class:tab-active={op === "reorder"} onclick={() => (op = "reorder")}>Reorder</button>
    </div>

    {#if op === "rotate"}
      <label class="field">
        <span class="field-label">Pages</span>
        <input type="text" placeholder="1, 3-5" bind:value={pagesText} />
        <span class="field-hint">Which pages to rotate.</span>
      </label>
      <label class="field">
        <span class="field-label">Angle</span>
        <div class="seg">
          <button class:tab-active={rotation === 90} onclick={() => (rotation = 90)}>90°</button>
          <button class:tab-active={rotation === 180} onclick={() => (rotation = 180)}>180°</button>
          <button class:tab-active={rotation === 270} onclick={() => (rotation = 270)}>270°</button>
        </div>
      </label>
    {:else if op === "delete"}
      <label class="field">
        <span class="field-label">Pages to delete</span>
        <input type="text" placeholder="2, 4-6" bind:value={pagesText} />
        <span class="field-hint">These pages will be removed.</span>
      </label>
    {:else}
      <label class="field">
        <span class="field-label">New order</span>
        <input type="text" bind:value={orderText} />
        <span class="field-hint">Comma-separated page numbers in the desired final order.</span>
      </label>
    {/if}

    <div class="actions">
      <button class="primary" onclick={runOp} disabled={status.kind === "working"}>
        {status.kind === "working" ? "Working…" : "Save copy"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>
