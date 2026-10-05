<script lang="ts">
  import SavedFiles from "$lib/components/SavedFiles.svelte";
  import { parseMergeRanges } from "$lib/mergeRanges";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { isInTauri } from "$lib/tauri";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  let inputs = $state<{ path: string; pages: string }[]>([]);
  let status = $state<Status>(idle);
  let choosing = $state(false);
  let outputs = $state<string[]>([]);
  const busy = $derived(choosing || status.kind === "working");
  let dragIndex = $state<number | null>(null);

  function needsDesktop(): boolean {
    if (!isInTauri()) {
      status = { kind: "err", msg: "Merging needs the Slab desktop app — the browser preview can't write files." };
      return true;
    }
    return false;
  }

  async function pickInputs() {
    if (busy || needsDesktop()) return;
    choosing = true;
    try {
      const picked = await open({ multiple: true, filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (!picked) return;
      const arr = Array.isArray(picked) ? picked : [picked];
      inputs = [...inputs, ...arr.map((path) => ({ path, pages: "" }))];
      status = idle;
    } catch (e) { status = { kind: "err", msg: String(e) }; }
    finally { choosing = false; }
  }

  function removeInput(i: number) {
    if (busy) return;
    inputs = inputs.filter((_, idx) => idx !== i);
  }

  function moveUp(i: number) {
    if (busy) return;
    if (i === 0) return;
    const next = [...inputs];
    [next[i - 1], next[i]] = [next[i], next[i - 1]];
    inputs = next;
  }

  function moveDown(i: number) {
    if (busy) return;
    if (i === inputs.length - 1) return;
    const next = [...inputs];
    [next[i + 1], next[i]] = [next[i], next[i + 1]];
    inputs = next;
  }

  function onDragStart(i: number) {
    if (busy) return;
    dragIndex = i;
  }

  function onDragOver(e: DragEvent, _i: number) {
    e.preventDefault();
  }

  function onDrop(i: number) {
    if (busy) return;
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

  // "1-3, 5" → [{start:1,end:3},{start:5,end:5}]. "" → [] (whole file).
  // Throws on anything that isn't a page number or range, so a typo can
  // never silently become "take every page".
  async function runMerge() {
    if (busy) return;
    if (inputs.length < 2) {
      status = { kind: "err", msg: "Add at least two PDFs to merge." };
      return;
    }
    if (needsDesktop()) return;

    // Parse page ranges up front so a typo fails before the save dialog.
    let parsed: { path: string; ranges: { start: number; end: number }[] }[];
    try {
      parsed = inputs.map((f) => {
        try {
          return { path: f.path, ranges: parseMergeRanges(f.pages) };
        } catch {
          throw new Error(
            `${basename(f.path)}: bad page range — try something like 1-3, 5.`
          );
        }
      });
    } catch (e) {
      status = { kind: "err", msg: (e as Error).message };
      return;
    }

    choosing = true;
    try {
      const output = await save({
        defaultPath: "merged.pdf",
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (typeof output !== "string") return;
      // Never let the merged output overwrite a source file.
      const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();
      if (parsed.some((p) => norm(p.path) === norm(output))) {
        status = { kind: "err", msg: "Pick an output file that isn't one of the inputs — merging onto a source would destroy it." };
        return;
      }

      status = { kind: "working", msg: "Merging…" };
      outputs = [];
      const useRanges = parsed.some((p) => p.ranges.length > 0);
      const res = useRanges
        ? await invoke<CmdResult<string>>("slab_merge_ranges", {
            inputs: parsed,
            output,
          })
        : await invoke<CmdResult<string>>("slab_merge", {
            inputs: parsed.map((p) => p.path),
            output,
          });
      if (res.kind === "ok") {
        outputs = [res.value];
        status = { kind: "ok", msg: `Saved → ${res.value}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally { choosing = false; }
  }
</script>

<header class="content-header">
  <h1>Merge PDFs</h1>
  <p class="subtitle">Stitch any number of PDFs into one clean file. Drag to reorder, pick pages per file, save anywhere.</p>
</header>

<section class="panel" aria-busy={busy}>
  {#if inputs.length === 0}
    <button class="dropzone" onclick={pickInputs} disabled={busy}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose PDFs to merge</span>
      <span class="dz-hint">Two or more. Files stay on your machine.</span>
    </button>
  {:else}
    <p class="merge-hint" role="status">{inputs.length} PDF{inputs.length === 1 ? "" : "s"} in merge order. {inputs.length < 2 ? "Add one more PDF to continue." : "Drag files or use the arrows to reorder."}</p>
    <ul class="file-list" aria-label="PDFs in merge order">
      {#each inputs as file, i (file.path + i)}
        <li
          class="file-row"
          class:dragging={dragIndex === i}
          draggable={!busy}
          ondragend={() => (dragIndex = null)}
          ondragstart={() => onDragStart(i)}
          ondragover={(e) => onDragOver(e, i)}
          ondrop={() => onDrop(i)}
        >
          <span class="row-handle" aria-hidden="true">⋮⋮</span>
          <span class="row-idx">{i + 1}</span>
          <span class="row-name" title={file.path}>{basename(file.path)}</span>
          <input
            disabled={busy}
            class="row-pages"
            bind:value={file.pages}
            placeholder="All pages"
            spellcheck={false}
            title="Pages to take from this file — e.g. 1-3, 5. Leave blank for the whole file."
            aria-label={`Pages to take from ${basename(file.path)}`}
          />
          <div class="row-actions">
            <button class="ghost" onclick={() => moveUp(i)} disabled={busy || i === 0} aria-label={`Move ${basename(file.path)} up`} title="Move up">↑</button>
            <button class="ghost" onclick={() => moveDown(i)} disabled={busy || i === inputs.length - 1} aria-label={`Move ${basename(file.path)} down`} title="Move down">↓</button>
            <button
              class="ghost remove"
              onclick={() => removeInput(i)}
              disabled={busy} aria-label={`Remove ${basename(file.path)}`} title="Remove PDF">✕</button
            >
          </div>
        </li>
      {/each}
    </ul>

    <div class="actions">
      <button onclick={pickInputs} disabled={busy}>+ Add PDFs</button>
      <button
        class="primary"
        onclick={runMerge}
        disabled={busy || inputs.length < 2}
      >
        {status.kind === "working"
          ? "Merging…"
          : `Merge ${inputs.length} PDFs`}
      </button>
    </div>
  {/if}

  {#if status.kind === "working"}
    <div class="status" role="status">{status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err" role="alert">✕ {status.msg}</div>
  {/if}
  <SavedFiles paths={outputs} />
</section>

<style>
  .merge-hint { margin: 0; color: var(--text-2); font-size: 12px; }
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
    min-width: 0;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-pages {
    width: 104px;
    flex: none;
    font-size: 12px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  .row-pages::placeholder {
    color: var(--text-3);
  }
  .row-pages:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
  @media (max-width: 620px) {
    .file-row { display: grid; grid-template-columns: auto auto minmax(0, 1fr); gap: 8px; cursor: default; }
    .row-pages { grid-column: 2 / 4; width: 100%; }
    .row-actions { grid-column: 2 / 4; justify-content: flex-end; }
  }
</style>
