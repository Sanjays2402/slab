<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  let inputs = $state<string[]>([]);
  let status = $state<Status>(idle);
  let dragIndex = $state<number | null>(null);

  async function pickInputs() {
    const picked = await open({
      multiple: true,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return;
    const arr = Array.isArray(picked) ? picked : [picked];
    inputs = [...inputs, ...arr];
    status = idle;
  }

  function removeInput(i: number) {
    inputs = inputs.filter((_, idx) => idx !== i);
  }

  function moveUp(i: number) {
    if (i === 0) return;
    const next = [...inputs];
    [next[i - 1], next[i]] = [next[i], next[i - 1]];
    inputs = next;
  }

  function moveDown(i: number) {
    if (i === inputs.length - 1) return;
    const next = [...inputs];
    [next[i + 1], next[i]] = [next[i], next[i + 1]];
    inputs = next;
  }

  function onDragStart(i: number) {
    dragIndex = i;
  }

  function onDragOver(e: DragEvent, _i: number) {
    e.preventDefault();
  }

  function onDrop(i: number) {
    if (dragIndex === null || dragIndex === i) {
      dragIndex = null;
      return;
    }
    const next = [...inputs];
    const [moved] = next.splice(dragIndex, 1);
    next.splice(i, 0, moved);
    inputs = next;
    dragIndex = null;
  }

  async function runMerge() {
    if (inputs.length < 2) {
      status = { kind: "err", msg: "Add at least two PDFs to merge." };
      return;
    }
    const output = await save({
      defaultPath: "merged.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Merging…" };
    try {
      const res = await invoke<CmdResult<string>>("slab_merge", {
        inputs,
        output,
      });
      if (res.kind === "ok") {
        status = { kind: "ok", msg: `Saved → ${res.value}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Merge PDFs</h1>
  <p class="subtitle">Stitch any number of PDFs into one clean file. Drag to reorder, save anywhere.</p>
</header>

<section class="panel">
  {#if inputs.length === 0}
    <button class="dropzone" onclick={pickInputs}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Drop PDFs to merge</span>
      <span class="dz-hint">Two or more. Files stay on your machine.</span>
    </button>
  {:else}
    <ul class="file-list">
      {#each inputs as path, i (path + i)}
        <li
          class="file-row"
          class:dragging={dragIndex === i}
          draggable="true"
          ondragstart={() => onDragStart(i)}
          ondragover={(e) => onDragOver(e, i)}
          ondrop={() => onDrop(i)}
        >
          <span class="row-handle" aria-hidden="true">⋮⋮</span>
          <span class="row-idx">{i + 1}</span>
          <span class="row-name" title={path}>{basename(path)}</span>
          <div class="row-actions">
            <button class="ghost" onclick={() => moveUp(i)} aria-label="Up">↑</button>
            <button class="ghost" onclick={() => moveDown(i)} aria-label="Down">↓</button>
            <button
              class="ghost remove"
              onclick={() => removeInput(i)}
              aria-label="Remove">✕</button
            >
          </div>
        </li>
      {/each}
    </ul>

    <div class="actions">
      <button onclick={pickInputs}>+ Add more</button>
      <button
        class="primary"
        onclick={runMerge}
        disabled={status.kind === "working" || inputs.length < 2}
      >
        {status.kind === "working"
          ? "Merging…"
          : `Merge ${inputs.length} files`}
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
  .file-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    cursor: grab;
  }
  .file-row.dragging {
    opacity: 0.5;
  }
  .file-row:hover {
    border-color: var(--border-strong);
  }
  .row-handle {
    color: var(--text-3);
    font-size: 11px;
    user-select: none;
  }
  .row-idx {
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--bg-3);
    color: var(--text-2);
    font-size: 11px;
    font-weight: 600;
  }
  .row-name {
    flex: 1;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-actions {
    display: flex;
    gap: 4px;
  }
  .row-actions button {
    padding: 4px 8px;
    font-size: 12px;
    border-radius: 6px;
  }
  .row-actions .remove:hover {
    color: var(--danger);
  }
</style>
