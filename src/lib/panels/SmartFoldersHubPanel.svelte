<!--
  Smart Folders Hub (v3.37.0 "Atlas Smart Folders Hub").

  Unified panel listing every built-in preset and every personal preset
  side-by-side, with:
    - search-filter input
    - pin / unpin (★)
    - HTML5 drag-to-reorder (persisted via slab_smart_folders_reorder)
    - one-click "Apply" — materializes the preset into a real smart
      collection (built-ins via slab_preset_apply, personal via
      slab_personal_preset_apply)
    - "Export all as pack…" — one-shot .slabpresets dump for personal entries

  Triggered from:
    - Command palette ("Smart Folders Hub")
    - Keyboard shortcut Cmd/Ctrl+Shift+F
    - Sidebar gear-menu "Smart Folders Hub…"

  Open/close is driven by a `slab:open-smart-folders-hub` window event,
  mirroring the pattern used by PresetPicker.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    smartFoldersList,
    smartFoldersReorder,
    smartFoldersPin,
    presetApply,
    personalPresetApply,
    personalPresetsExport,
    type SmartFolderEntry,
  } from "$lib/library";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { writeTextFile } from "@tauri-apps/plugin-fs";
  import SuggestedFolders from "./SuggestedFolders.svelte";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  const { open, onClose }: Props = $props();

  let entries = $state<SmartFolderEntry[]>([]);
  let query = $state("");
  let loading = $state(false);
  let error = $state<string | null>(null);
  let applyingId = $state<string | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  // Drag state
  let draggingIdx = $state<number | null>(null);
  let dragOverIdx = $state<number | null>(null);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        e.description.toLowerCase().includes(q) ||
        e.id.toLowerCase().includes(q),
    );
  });

  const personalCount = $derived(
    entries.filter((e) => e.kind === "personal").length,
  );

  async function refresh() {
    loading = true;
    error = null;
    try {
      entries = await smartFoldersList();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  function showToast(msg: string) {
    toast = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 2200);
  }

  async function togglePin(e: SmartFolderEntry) {
    try {
      await smartFoldersPin(
        e.kind,
        e.id,
        !e.pinned,
      );
      await refresh();
    } catch (err) {
      error = (err as Error).message;
    }
  }

  async function apply(e: SmartFolderEntry) {
    applyingId = `${e.kind}:${e.id}`;
    try {
      if (e.kind === "builtin") {
        await presetApply(e.id);
      } else {
        await personalPresetApply(Number(e.id));
      }
      showToast(`Applied “${e.name}”`);
    } catch (err) {
      error = (err as Error).message;
    } finally {
      applyingId = null;
    }
  }

  async function exportAllPersonal() {
    if (personalCount === 0) {
      showToast("No personal presets to export");
      return;
    }
    try {
      const json = await personalPresetsExport([]); // empty = all
      const filePath = await saveDialog({
        title: "Export Personal Presets",
        defaultPath: "my-presets.slabpresets",
        filters: [{ name: "Slab Preset Pack", extensions: ["slabpresets"] }],
      });
      if (!filePath) return;
      await writeTextFile(filePath, json);
      showToast(`Exported ${personalCount} preset${personalCount === 1 ? "" : "s"}`);
    } catch (err) {
      error = (err as Error).message;
    }
  }

  // ----- drag handlers -----
  function onDragStart(idx: number, ev: DragEvent) {
    draggingIdx = idx;
    if (ev.dataTransfer) {
      ev.dataTransfer.effectAllowed = "move";
      // Required by Firefox to actually start a drag.
      ev.dataTransfer.setData("text/plain", String(idx));
    }
  }

  function onDragOver(idx: number, ev: DragEvent) {
    ev.preventDefault();
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = "move";
    dragOverIdx = idx;
  }

  function onDragEnd() {
    draggingIdx = null;
    dragOverIdx = null;
  }

  async function onDrop(targetIdx: number, ev: DragEvent) {
    ev.preventDefault();
    const src = draggingIdx;
    draggingIdx = null;
    dragOverIdx = null;
    if (src === null || src === targetIdx) return;

    // Reorder the local list optimistically.
    const next = entries.slice();
    const [moved] = next.splice(src, 1);
    next.splice(targetIdx, 0, moved);
    entries = next;

    // Persist full order.
    try {
      await smartFoldersReorder(
        next.map((e, i) => ({ kind: e.kind, id: e.id, sort_order: i })),
      );
    } catch (err) {
      error = (err as Error).message;
      await refresh();
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    refresh();
    window.addEventListener("keydown", handleKey);
    const libHandler = () => refresh();
    window.addEventListener("library-changed", libHandler);
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("library-changed", libHandler);
      if (toastTimer) clearTimeout(toastTimer);
    };
  });

  $effect(() => {
    if (open) refresh();
  });
</script>

{#if open}
  <div
    class="hub-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Smart Folders Hub"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}
    onkeydown={handleKey}
    tabindex="-1"
  >
    <div class="hub-shell">
      <header class="hub-head">
        <div class="hub-title">
          <span class="folder-glyph">🗂</span>
          <div>
            <h2>Smart Folders</h2>
            <p class="subtitle">
              {entries.length} folder{entries.length === 1 ? "" : "s"}
              · {personalCount} personal · drag to reorder
            </p>
          </div>
        </div>
        <div class="hub-actions">
          <input
            class="hub-search"
            type="search"
            placeholder="Search folders…"
            bind:value={query}
            aria-label="Search smart folders"
          />
          <button
            class="hub-btn ghost"
            onclick={exportAllPersonal}
            disabled={personalCount === 0}
            title={personalCount === 0
              ? "No personal presets to export"
              : `Export ${personalCount} personal preset${personalCount === 1 ? "" : "s"} as .slabpresets pack`}
          >
            ⬇ Export pack
          </button>
          <button class="hub-btn" onclick={onClose} aria-label="Close">Close</button>
        </div>
      </header>

      <SuggestedFolders onAccepted={refresh} />

      {#if error}
        <div class="hub-error" role="alert">{error}</div>
      {/if}

      {#if loading && entries.length === 0}
        <div class="hub-empty">Loading…</div>
      {:else if filtered.length === 0}
        <div class="hub-empty">
          {#if query}
            No folders match “{query}”.
          {:else}
            No smart folders yet. (This shouldn't happen — built-ins ship with Slab.)
          {/if}
        </div>
      {:else}
        <ul class="hub-list" aria-label="Smart folders, drag to reorder">
          {#each filtered as e, i (e.kind + ":" + e.id)}
            <li
              class="hub-row"
              class:dragging={draggingIdx === i}
              class:dragover={dragOverIdx === i && draggingIdx !== i}
              class:personal={e.kind === "personal"}
              draggable="true"
              ondragstart={(ev) => onDragStart(i, ev)}
              ondragover={(ev) => onDragOver(i, ev)}
              ondragend={onDragEnd}
              ondrop={(ev) => onDrop(i, ev)}
            >
              <span class="drag-handle" aria-hidden="true">⋮⋮</span>

              <span
                class="row-icon"
                style="--row-color: {e.color};"
                aria-hidden="true"
              >
                {e.icon || "📁"}
              </span>

              <div class="row-body">
                <div class="row-name-line">
                  <span class="row-name">{e.name}</span>
                  <span class="row-kind">
                    {e.kind === "builtin" ? "Built-in" : "Personal"}
                  </span>
                </div>
                {#if e.description}
                  <div class="row-desc">{e.description}</div>
                {/if}
              </div>

              <button
                class="row-pin"
                class:pinned={e.pinned}
                onclick={() => togglePin(e)}
                aria-label={e.pinned ? "Unpin" : "Pin to top"}
                title={e.pinned ? "Unpin" : "Pin to top"}
              >
                {e.pinned ? "★" : "☆"}
              </button>

              <button
                class="row-apply"
                onclick={() => apply(e)}
                disabled={applyingId === `${e.kind}:${e.id}`}
              >
                {applyingId === `${e.kind}:${e.id}` ? "…" : "Apply"}
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if toast}
        <div class="hub-toast" role="status">{toast}</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .hub-backdrop {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, black 45%, transparent);
    backdrop-filter: blur(14px) saturate(140%);
    -webkit-backdrop-filter: blur(14px) saturate(140%);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1500;
    animation: fade-in 140ms ease-out;
  }
  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .hub-shell {
    width: min(820px, 92vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--panel-bg, #181826);
    color: var(--text, #e7e7f0);
    border: 1px solid color-mix(in srgb, white 8%, transparent);
    border-radius: 16px;
    box-shadow:
      0 24px 64px rgba(0, 0, 0, 0.55),
      0 0 0 1px color-mix(in srgb, white 4%, transparent);
    overflow: hidden;
    animation: pop-in 160ms cubic-bezier(.2,.9,.3,1);
  }
  @keyframes pop-in {
    from { transform: scale(.97); opacity: 0; }
    to   { transform: scale(1);   opacity: 1; }
  }

  .hub-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid color-mix(in srgb, white 7%, transparent);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, white 4%, transparent),
      transparent
    );
  }

  .hub-title {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .folder-glyph {
    font-size: 28px;
    line-height: 1;
  }
  .hub-title h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .subtitle {
    margin: 2px 0 0;
    font-size: 12px;
    opacity: 0.62;
  }

  .hub-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .hub-search {
    appearance: none;
    border: 1px solid color-mix(in srgb, white 10%, transparent);
    background: color-mix(in srgb, white 4%, transparent);
    color: inherit;
    border-radius: 8px;
    padding: 6px 10px;
    font-size: 13px;
    width: 200px;
    outline: none;
    transition: border-color 120ms, background 120ms;
  }
  .hub-search:focus {
    border-color: color-mix(in srgb, #7c8cff 70%, transparent);
    background: color-mix(in srgb, white 7%, transparent);
  }

  .hub-btn {
    appearance: none;
    border: 1px solid color-mix(in srgb, white 12%, transparent);
    background: color-mix(in srgb, white 6%, transparent);
    color: inherit;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    transition: background 120ms, transform 80ms;
  }
  .hub-btn:hover:not(:disabled) {
    background: color-mix(in srgb, white 11%, transparent);
  }
  .hub-btn:active:not(:disabled) {
    transform: translateY(1px);
  }
  .hub-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .hub-btn.ghost {
    background: transparent;
  }

  .hub-error {
    margin: 10px 22px 0;
    padding: 8px 12px;
    background: color-mix(in srgb, #ff5d6c 18%, transparent);
    border: 1px solid color-mix(in srgb, #ff5d6c 40%, transparent);
    color: #ffb8be;
    border-radius: 8px;
    font-size: 12px;
  }

  .hub-empty {
    padding: 56px 22px;
    text-align: center;
    opacity: 0.55;
    font-size: 13px;
  }

  .hub-list {
    list-style: none;
    margin: 0;
    padding: 6px 8px 14px;
    overflow-y: auto;
    flex: 1;
  }

  .hub-row {
    display: grid;
    grid-template-columns: 20px 36px 1fr auto auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    transition: background 120ms, transform 80ms, border-color 120ms;
    border: 1px solid transparent;
    cursor: grab;
  }
  .hub-row + .hub-row {
    margin-top: 2px;
  }
  .hub-row:hover {
    background: color-mix(in srgb, white 5%, transparent);
  }
  .hub-row.dragging {
    opacity: 0.45;
    cursor: grabbing;
  }
  .hub-row.dragover {
    border-color: color-mix(in srgb, #7c8cff 55%, transparent);
    background: color-mix(in srgb, #7c8cff 12%, transparent);
  }

  .drag-handle {
    color: color-mix(in srgb, white 25%, transparent);
    font-size: 14px;
    letter-spacing: -2px;
    user-select: none;
  }

  .row-icon {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--row-color, #777) 22%, transparent);
    border: 1px solid color-mix(in srgb, var(--row-color, #777) 50%, transparent);
    border-radius: 9px;
    font-size: 18px;
  }

  .row-body { min-width: 0; }

  .row-name-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .row-name {
    font-size: 14px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 2px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, white 8%, transparent);
    color: color-mix(in srgb, white 70%, transparent);
  }
  .hub-row.personal .row-kind {
    background: color-mix(in srgb, #f0b86e 22%, transparent);
    color: #f7d39a;
  }
  .row-desc {
    margin-top: 2px;
    font-size: 12px;
    opacity: 0.62;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-pin {
    appearance: none;
    border: 0;
    background: transparent;
    color: color-mix(in srgb, white 40%, transparent);
    font-size: 18px;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    cursor: pointer;
    transition: color 120ms, background 120ms, transform 120ms;
  }
  .row-pin:hover { background: color-mix(in srgb, white 8%, transparent); }
  .row-pin.pinned {
    color: #ffcf66;
    transform: scale(1.05);
  }

  .row-apply {
    appearance: none;
    border: 1px solid color-mix(in srgb, #7c8cff 60%, transparent);
    background: color-mix(in srgb, #7c8cff 22%, transparent);
    color: #d9deff;
    padding: 5px 12px;
    border-radius: 7px;
    font-size: 12px;
    cursor: pointer;
    transition: background 120ms, transform 80ms;
  }
  .row-apply:hover:not(:disabled) {
    background: color-mix(in srgb, #7c8cff 32%, transparent);
  }
  .row-apply:active:not(:disabled) { transform: translateY(1px); }
  .row-apply:disabled { opacity: 0.55; cursor: progress; }

  .hub-toast {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 16px;
    background: color-mix(in srgb, #1f1f2e 92%, transparent);
    color: #e7e7f0;
    border: 1px solid color-mix(in srgb, white 12%, transparent);
    border-radius: 999px;
    font-size: 12px;
    box-shadow: 0 8px 24px rgba(0,0,0,.4);
    animation: toast-in 160ms ease-out;
  }
  @keyframes toast-in {
    from { opacity: 0; transform: translate(-50%, 8px); }
    to   { opacity: 1; transform: translate(-50%, 0); }
  }
</style>
