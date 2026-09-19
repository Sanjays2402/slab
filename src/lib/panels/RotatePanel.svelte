<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let degrees = $state<90 | 180 | 270>(90);
  let rangeText = $state("");
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

  // "1-3, 5" -> [1, 2, 3, 5]. Empty string means every page.
  function parsePages(text: string): number[] | string {
    const parts = text.split(",").map((s) => s.trim()).filter(Boolean);
    if (parts.length === 0) return [];
    const out = new Set<number>();
    for (const p of parts) {
      const m = p.match(/^(\d+)(?:-(\d+))?$/);
      if (!m) return `Bad page range: "${p}". Try something like 1-3, 5.`;
      const start = parseInt(m[1], 10);
      const end = m[2] ? parseInt(m[2], 10) : start;
      if (start < 1 || end < start) return `Bad page range: "${p}". Try something like 1-3, 5.`;
      for (let n = start; n <= end; n++) out.add(n);
    }
    return [...out].sort((a, b) => a - b);
  }

  async function runRotate() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    const pages = parsePages(rangeText);
    if (typeof pages === "string") {
      status = { kind: "err", msg: pages };
      return;
    }

    const output = await save({
      defaultPath: `${stripExt(basename(input))}-rotated.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;
    // Don't rotate onto the source file — the backend may still be reading it.
    const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();
    if (norm(output) === norm(input)) {
      status = { kind: "err", msg: "Pick an output file that isn't the input — rotating onto the source would destroy it." };
      return;
    }

    status = { kind: "working", msg: `Rotating ${pages.length === 0 ? "all pages" : `${pages.length} page(s)`}…` };
    try {
      const res = await invoke<CmdResult<number>>("slab_rotate", {
        input,
        output,
        degrees,
        pages,
      });
      if (res.kind === "ok") {
        status = { kind: "ok", msg: `Rotated ${res.value} page(s) by ${degrees}°. Saved to ${basename(output)}.` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Rotate Pages</h1>
  <p class="subtitle">Turn pages sideways scans right-side up. Rotations add up.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Pick a file, then choose an angle.</span>
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

    <div class="tabs" role="radiogroup" aria-label="Rotation angle">
      <button
        class:tab-active={degrees === 90}
        onclick={() => (degrees = 90)}
        title="Quarter turn clockwise"
      >90°</button>
      <button
        class:tab-active={degrees === 180}
        onclick={() => (degrees = 180)}
        title="Upside down"
      >180°</button>
      <button
        class:tab-active={degrees === 270}
        onclick={() => (degrees = 270)}
        title="Quarter turn counter-clockwise"
      >270°</button>
    </div>

    <label class="field">
      <span class="field-label">Pages</span>
      <input
        type="text"
        placeholder="1-3, 5"
        bind:value={rangeText}
      />
      <span class="field-hint">Leave blank for every page. Rotating twice by 90° equals 180°.</span>
    </label>

    <div class="actions">
      <button
        class="primary"
        onclick={runRotate}
        disabled={status.kind === "working"}
      >
        {status.kind === "working" ? "Rotating…" : `Rotate ${degrees}°`}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>
