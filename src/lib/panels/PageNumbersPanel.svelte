<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Position =
    | "top-left"
    | "top-center"
    | "top-right"
    | "bottom-left"
    | "bottom-center"
    | "bottom-right";

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);

  let template = $state("Page {n} of {total}");
  let position = $state<Position>("bottom-center");
  let fontSize = $state(11);
  let startAt = $state(1);
  let skipFirst = $state(0);
  let gray = $state(0.2);

  // Live preview value (purely cosmetic — server stamps the real numbers).
  let previewLabel = $derived(template.replace("{n}", String(startAt)).replace("{total}", "10"));

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
    if (!template.trim()) {
      status = { kind: "err", msg: "Template can't be empty." };
      return;
    }
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-numbered.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Stamping…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_page_numbers", {
        input,
        output,
        opts: {
          template,
          position,
          font_size: fontSize,
          start_at: startAt,
          skip_first: skipFirst,
          gray,
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

  const POSITIONS: { id: Position; label: string }[] = [
    { id: "top-left", label: "↖" },
    { id: "top-center", label: "↑" },
    { id: "top-right", label: "↗" },
    { id: "bottom-left", label: "↙" },
    { id: "bottom-center", label: "↓" },
    { id: "bottom-right", label: "↘" },
  ];
</script>

<header class="content-header">
  <h1>Page Numbers</h1>
  <p class="subtitle">Stamp page numbers anywhere. Templates, custom start, skip cover.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Drop a file to stamp.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Ready to number</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <label class="field">
      <span class="field-label">Template</span>
      <input type="text" bind:value={template} placeholder={"Page {n} of {total}"} />
      <span class="hint">
        Use <code>{`{n}`}</code> for the page number and <code>{`{total}`}</code> for the count.
      </span>
    </label>

    <div class="preview">
      <span class="preview-label">Preview</span>
      <div class="page-mock pos-{position}">
        <span class="num">{previewLabel}</span>
      </div>
    </div>

    <label class="field">
      <span class="field-label">Position</span>
      <div class="pos-grid">
        {#each POSITIONS as p (p.id)}
          <button
            class="pos-btn"
            class:active={position === p.id}
            onclick={() => (position = p.id)}
            title={p.id}
          >
            {p.label}
          </button>
        {/each}
      </div>
    </label>

    <div class="grid">
      <label class="field">
        <span class="field-label">Font size: {fontSize}pt</span>
        <input type="range" min="8" max="32" step="1" bind:value={fontSize} />
      </label>
      <label class="field">
        <span class="field-label">Start at</span>
        <input type="number" min="1" bind:value={startAt} />
      </label>
      <label class="field">
        <span class="field-label">Skip first N pages</span>
        <input type="number" min="0" bind:value={skipFirst} />
      </label>
      <label class="field">
        <span class="field-label">Gray: {gray.toFixed(2)}</span>
        <input type="range" min="0" max="1" step="0.05" bind:value={gray} />
      </label>
    </div>

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working" ? status.msg : "Stamp page numbers"}
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
  .hint {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 4px;
  }
  .hint code {
    background: var(--bg-2);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 11px;
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .preview-label {
    font-size: 11px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .page-mock {
    position: relative;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    aspect-ratio: 8.5 / 11;
    max-width: 220px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
  }
  .num {
    position: absolute;
    font-size: 10px;
    color: var(--text-3);
    padding: 6px 10px;
  }
  .pos-top-left .num {
    top: 0;
    left: 0;
  }
  .pos-top-center .num {
    top: 0;
    left: 50%;
    transform: translateX(-50%);
  }
  .pos-top-right .num {
    top: 0;
    right: 0;
  }
  .pos-bottom-left .num {
    bottom: 0;
    left: 0;
  }
  .pos-bottom-center .num {
    bottom: 0;
    left: 50%;
    transform: translateX(-50%);
  }
  .pos-bottom-right .num {
    bottom: 0;
    right: 0;
  }

  .pos-grid {
    display: grid;
    grid-template-columns: repeat(3, 40px);
    gap: 4px;
  }
  .pos-btn {
    width: 40px;
    height: 40px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    border-radius: var(--r-sm);
    cursor: pointer;
    font-size: 16px;
  }
  .pos-btn:hover {
    background: var(--bg-3);
    color: var(--text-1);
  }
  .pos-btn.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
