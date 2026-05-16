<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Mode = "blank" | "pdf";
  let mode = $state<Mode>("blank");

  let input = $state<string | null>(null);
  let donor = $state<string | null>(null);
  let status = $state<Status>(idle);

  let position = $state(1);
  let blankCount = $state(1);
  let blankSize = $state<"a4" | "letter" | "legal">("a4");

  const sizes = {
    a4: { w: 595, h: 842, label: "A4 (595×842)" },
    letter: { w: 612, h: 792, label: "US Letter (612×792)" },
    legal: { w: 612, h: 1008, label: "Legal (612×1008)" },
  };

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
  }

  async function pickDonor() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    donor = picked;
    status = idle;
  }

  async function run() {
    if (!input) {
      status = { kind: "err", msg: "Pick a host PDF first." };
      return;
    }
    if (mode === "pdf" && !donor) {
      status = { kind: "err", msg: "Pick the PDF to insert." };
      return;
    }
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-inserted.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Inserting…" };
    const source =
      mode === "blank"
        ? {
            blank: {
              count: blankCount,
              width: sizes[blankSize].w,
              height: sizes[blankSize].h,
            },
          }
        : { pdf: { path: donor! } };

    try {
      const res = await invoke<CmdResult<number>>("slab_insert", {
        input,
        output,
        opts: { at: position, source },
      });
      status =
        res.kind === "ok"
          ? { kind: "ok", msg: `New total: ${res.value} pages → ${basename(output)}` }
          : { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Insert Pages</h1>
  <p class="subtitle">Splice another PDF or add blank pages at any position.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose host PDF</span>
      <span class="dz-hint">Where new pages will land.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Ready to insert</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="seg">
      <button class:active={mode === "blank"} onclick={() => (mode = "blank")}>
        Blank pages
      </button>
      <button class:active={mode === "pdf"} onclick={() => (mode = "pdf")}>
        Another PDF
      </button>
    </div>

    {#if mode === "blank"}
      <div class="grid">
        <label class="field">
          <span class="field-label">How many</span>
          <input type="number" min="1" max="500" bind:value={blankCount} />
        </label>
        <label class="field">
          <span class="field-label">Size</span>
          <select bind:value={blankSize}>
            {#each Object.entries(sizes) as [k, v] (k)}
              <option value={k}>{v.label}</option>
            {/each}
          </select>
        </label>
      </div>
    {:else if !donor}
      <button class="dropzone secondary" onclick={pickDonor}>
        <span class="dz-icon">+</span>
        <span class="dz-title">Pick PDF to insert</span>
      </button>
    {:else}
      <div class="file-card">
        <div>
          <div class="file-name">{basename(donor)}</div>
          <div class="file-meta">Will be spliced in</div>
        </div>
        <button class="ghost" onclick={pickDonor}>Change</button>
      </div>
    {/if}

    <label class="field">
      <span class="field-label">Insert at page #</span>
      <input type="number" min="1" bind:value={position} />
      <span class="hint">1 = before first page; large value = append at end.</span>
    </label>

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working" ? status.msg : "Insert"}
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
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    padding: 10px;
    background: var(--bg-1);
    color: var(--text-2);
    border: none;
    cursor: pointer;
    font-size: 13px;
  }
  .seg button:hover {
    background: var(--bg-2);
  }
  .seg button.active {
    background: var(--accent);
    color: white;
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 4px;
  }
  .dropzone.secondary {
    min-height: 88px;
  }
  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
