<!--
  Atlas Collections sidebar rail section (v3.32.0).

  Renders user-curated Collections and Smart Collections under the
  Library panel's left rail. Pulses the count badge whenever the count
  grows — that's the "wow" moment when a freshly-scanned doc lands in a
  Smart Collection like "Recently added".

  Click a row -> emit `select` with the resolved DocumentRecord[]. The
  parent (LibraryPanel) decides what to do (we override its filtered
  view in v3.32.0).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    collectionList,
    collectionCreate,
    collectionDelete,
    collectionListDocs,
    collectionAddDocs,
    smartCollectionList,
    smartCollectionExpand,
    smartCollectionDelete,
    personalPresetSave,
    type CollectionRecord,
    type SmartCollectionRecord,
    type DocumentRecord,
  } from "$lib/library";
  import SmartCollectionBuilder from "./SmartCollectionBuilder.svelte";
  import PresetPicker from "./PresetPicker.svelte";
  import SmartFoldersHubPanel from "./SmartFoldersHubPanel.svelte";
  import OcrQueuePanel from "./OcrQueuePanel.svelte";

  type SelectPayload = {
    kind: "collection" | "smart";
    id: number;
    name: string;
    docs: DocumentRecord[];
  };

  let { onSelect = (_: SelectPayload) => {} }: { onSelect?: (p: SelectPayload) => void } = $props();

  let collections = $state<CollectionRecord[]>([]);
  let smart = $state<SmartCollectionRecord[]>([]);
  let activeId = $state<string | null>(null);
  let creating = $state(false);
  let newName = $state("");
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Smart collection builder
  let builderOpen = $state(false);
  let builderEditing = $state<SmartCollectionRecord | null>(null);

  // Preset picker (v3.35.0)
  let presetPickerOpen = $state(false);
  function openPresetPicker() {
    presetPickerOpen = true;
  }
  // Public hook so App.svelte can drive this from a keyboard shortcut
  // / command palette entry without rummaging through the DOM.
  export function openPresets() {
    openPresetPicker();
  }

  // Smart Folders Hub (v3.37.0)
  let smartHubOpen = $state(false);
  function openSmartHub() {
    smartHubOpen = true;
  }
  export function openSmartFoldersHub() {
    openSmartHub();
  }

  // OCR Queue Panel (v3.52.0 Atlas OCR-Queue Slice 5)
  let ocrQueueOpen = $state(false);
  function openOcrQueue() {
    ocrQueueOpen = true;
  }
  export function openOcrQueuePanel() {
    openOcrQueue();
  }

  function openNewSmart() {
    builderEditing = null;
    builderOpen = true;
  }
  function openEditSmart(s: SmartCollectionRecord) {
    builderEditing = s;
    builderOpen = true;
  }

  // Context menu (right-click on a smart row)
  let menu = $state<{
    x: number;
    y: number;
    smart: SmartCollectionRecord;
  } | null>(null);

  // Drag-and-drop target state. When a doc card is being dragged over a
  // manual collection row, we set `dragOverId` to it; smart rows show a
  // blocked cursor instead.
  let dragOverId = $state<number | null>(null);
  let dragBlockedId = $state<number | null>(null);

  // Toast (one-shot fade)
  let toastMsg = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;
  function toast(msg: string) {
    toastMsg = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toastMsg = null), 2400);
  }

  // Pulse tracking — { collectionId: previousCount } so we can detect a
  // delta on the next refresh and add a one-shot CSS class.
  let prevCounts = $state<Map<number, number>>(new Map());
  let pulsing = $state<Set<number>>(new Set());

  async function refresh() {
    try {
      loading = true;
      const [cs, ss] = await Promise.all([collectionList(), smartCollectionList()]);
      const next = new Map<number, number>();
      const grew = new Set<number>();
      for (const c of cs) {
        next.set(c.id, c.doc_count);
        const prev = prevCounts.get(c.id);
        if (prev !== undefined && c.doc_count > prev) grew.add(c.id);
      }
      collections = cs;
      smart = ss;
      prevCounts = next;
      if (grew.size > 0) {
        pulsing = new Set([...pulsing, ...grew]);
        setTimeout(() => {
          const fresh = new Set(pulsing);
          for (const id of grew) fresh.delete(id);
          pulsing = fresh;
        }, 700);
      }
      error = null;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  function handleGlobalKey(e: KeyboardEvent) {
    // Cmd/Ctrl + Shift + N → new smart collection.
    // Use e.code === "KeyN" so layouts that compose ñ etc. still work.
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === "KeyN") {
      e.preventDefault();
      openNewSmart();
    } else if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === "KeyP") {
      // Cmd/Ctrl + Shift + P → open Preset Picker (v3.35.0).
      e.preventDefault();
      openPresetPicker();
    } else if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === "KeyF") {
      // Cmd/Ctrl + Shift + F → open Smart Folders Hub (v3.37.0).
      e.preventDefault();
      openSmartHub();
    } else if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === "KeyO") {
      // Cmd/Ctrl + Shift + O → open OCR Queue Panel (v3.52.0).
      e.preventDefault();
      openOcrQueue();
    } else if (e.key === "Escape" && menu) {
      menu = null;
    }
  }

  onMount(() => {
    refresh();
    const libHandler = () => refresh();
    window.addEventListener("library-changed", libHandler);
    window.addEventListener("keydown", handleGlobalKey);
    // v3.35.0 — command palette opens the picker via these events.
    const openPicker = () => openPresetPicker();
    const openBuilder = () => openNewSmart();
    const openHub = () => openSmartHub();
    window.addEventListener("slab:open-preset-picker", openPicker);
    window.addEventListener("slab:open-smart-builder", openBuilder);
    window.addEventListener("slab:open-smart-folders-hub", openHub);
    const openOcrQ = () => openOcrQueue();
    window.addEventListener("slab:open-ocr-queue", openOcrQ);
    const clickAway = () => (menu = null);
    window.addEventListener("click", clickAway);
    return () => {
      window.removeEventListener("library-changed", libHandler);
      window.removeEventListener("keydown", handleGlobalKey);
      window.removeEventListener("slab:open-preset-picker", openPicker);
      window.removeEventListener("slab:open-smart-builder", openBuilder);
      window.removeEventListener("slab:open-smart-folders-hub", openHub);
      window.removeEventListener("slab:open-ocr-queue", openOcrQ);
      window.removeEventListener("click", clickAway);
    };
  });

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    try {
      await collectionCreate(name, "folder", "#a78bfa");
      newName = "";
      creating = false;
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function pickCollection(c: CollectionRecord) {
    activeId = `c:${c.id}`;
    const docs = await collectionListDocs(c.id);
    onSelect({ kind: "collection", id: c.id, name: c.name, docs });
  }

  async function pickSmart(s: SmartCollectionRecord) {
    activeId = `s:${s.id}`;
    const docs = await smartCollectionExpand(s.id);
    onSelect({ kind: "smart", id: s.id, name: s.name, docs });
  }

  async function handleDelete(c: CollectionRecord, ev: MouseEvent) {
    ev.stopPropagation();
    if (!confirm(`Delete collection "${c.name}"? (Docs inside stay in your library.)`)) return;
    await collectionDelete(c.id);
    await refresh();
  }

  async function handleSmartDelete(s: SmartCollectionRecord) {
    menu = null;
    if (!confirm(`Delete smart collection "${s.name}"? (Rules deleted; matching docs remain.)`)) return;
    try {
      await smartCollectionDelete(s.id);
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function saveAsPersonalPreset(s: SmartCollectionRecord) {
    const name = prompt(
      "Name this personal preset (visible in the Preset Picker):",
      s.name,
    );
    if (!name || !name.trim()) return;
    const description = prompt(
      "Short description (optional):",
      `Saved from "${s.name}"`,
    );
    try {
      // The smart collection stores its filter as JSON — parse it back.
      const filter = JSON.parse(s.query_json);
      await personalPresetSave({
        name: name.trim(),
        icon: s.icon,
        color: s.color,
        description: description?.trim() || null,
        filter,
      });
      toast(`Saved “${name.trim()}” to your personal presets.`);
    } catch (e) {
      error = (e as Error).message;
    }
  }

  function showSmartMenu(e: MouseEvent, s: SmartCollectionRecord) {
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY, smart: s };
  }

  // ---- Drag-and-drop handlers (manual collections only) ----
  function readDragIds(e: DragEvent): number[] | null {
    const raw = e.dataTransfer?.getData("application/x-slab-doc-ids");
    if (!raw) return null;
    try {
      const ids = JSON.parse(raw) as number[];
      return Array.isArray(ids) ? ids : null;
    } catch {
      return null;
    }
  }

  function onDocDragOver(e: DragEvent, c: CollectionRecord) {
    if (!e.dataTransfer?.types.includes("application/x-slab-doc-ids")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    dragOverId = c.id;
  }
  function onDocDragLeave(c: CollectionRecord) {
    if (dragOverId === c.id) dragOverId = null;
  }
  async function onDocDrop(e: DragEvent, c: CollectionRecord) {
    e.preventDefault();
    dragOverId = null;
    const ids = readDragIds(e);
    if (!ids || ids.length === 0) return;
    try {
      const added = await collectionAddDocs(c.id, ids);
      window.dispatchEvent(new CustomEvent("library-changed"));
      toast(
        `Added ${added} doc${added === 1 ? "" : "s"} to “${c.name}”` +
          (added < ids.length ? ` (${ids.length - added} already in)` : ""),
      );
    } catch (err) {
      error = (err as Error).message;
    }
  }

  function onSmartDragOver(e: DragEvent, s: SmartCollectionRecord) {
    if (!e.dataTransfer?.types.includes("application/x-slab-doc-ids")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "none";
    dragBlockedId = s.id;
  }
  function onSmartDragLeave(s: SmartCollectionRecord) {
    if (dragBlockedId === s.id) dragBlockedId = null;
  }
  function onSmartDrop(e: DragEvent, _s: SmartCollectionRecord) {
    e.preventDefault();
    dragBlockedId = null;
    toast("Smart collections auto-update — edit the rules to change membership.");
  }
</script>

<section class="cs-rail">
  <header class="cs-head">
    <span class="cs-title">Collections</span>
    <button class="cs-add" aria-label="New collection" onclick={() => (creating = !creating)}>
      +
    </button>
  </header>

  {#if creating}
    <form
      class="cs-new"
      onsubmit={(e) => {
        e.preventDefault();
        handleCreate();
      }}
    >
      <input
        class="cs-input"
        type="text"
        placeholder="Name your collection…"
        bind:value={newName}
      />
      <button class="cs-save" type="submit" disabled={!newName.trim()}>Create</button>
    </form>
  {/if}

  {#if loading && collections.length === 0 && smart.length === 0}
    <div class="cs-empty">Loading…</div>
  {:else}
    {#each collections as c (c.id)}
      <div
        class="cs-row-wrap"
        class:active={activeId === `c:${c.id}`}
        class:drag-over={dragOverId === c.id}
      >
        <button
          class="cs-row"
          class:active={activeId === `c:${c.id}`}
          onclick={() => pickCollection(c)}
          ondragover={(e) => onDocDragOver(e, c)}
          ondragleave={() => onDocDragLeave(c)}
          ondrop={(e) => onDocDrop(e, c)}
          title={c.name}
        >
          <span class="cs-dot" style:background={c.color ?? "var(--text-3)"}></span>
          <span class="cs-label">{c.name}</span>
          <span class="cs-count" class:pulse={pulsing.has(c.id)}>{c.doc_count}</span>
        </button>
        <button
          class="cs-x"
          aria-label="Delete {c.name}"
          onclick={(e) => handleDelete(c, e)}
        >×</button>
      </div>
    {/each}

    <div class="cs-sub-row">
      <span class="cs-sub">Smart</span>
      <div class="cs-sub-actions">
        <button
          class="cs-add small hub"
          aria-label="Smart Folders Hub"
          title="Smart Folders Hub (⌘⇧F)"
          onclick={openSmartHub}
        >🗂</button>
        <button
          class="cs-add small preset"
          aria-label="Add from preset"
          title="Add from preset (⌘⇧P)"
          onclick={openPresetPicker}
        >★</button>
        <button
          class="cs-add small"
          aria-label="New smart collection"
          title="New smart collection (⌘⇧N)"
          onclick={openNewSmart}
        >+</button>
      </div>
    </div>
    {#if smart.length > 0}
      {#each smart as s (s.id)}
        <button
          class="cs-row smart"
          class:active={activeId === `s:${s.id}`}
          class:drag-blocked={dragBlockedId === s.id}
          onclick={() => pickSmart(s)}
          ondblclick={() => openEditSmart(s)}
          oncontextmenu={(e) => showSmartMenu(e, s)}
          ondragover={(e) => onSmartDragOver(e, s)}
          ondragleave={() => onSmartDragLeave(s)}
          ondrop={(e) => onSmartDrop(e, s)}
          title={`${s.name} — double-click to edit`}
        >
          <span class="cs-dot diamond" style:background={s.color ?? "var(--accent)"}></span>
          <span class="cs-label">{s.name}</span>
        </button>
      {/each}
    {:else}
      <div class="cs-empty cs-sub-empty">
        Click + to build one without writing JSON.
      </div>
    {/if}

    {#if collections.length === 0 && smart.length === 0}
      <div class="cs-empty">No collections yet — click + to make one.</div>
    {/if}
  {/if}

  {#if error}
    <div class="cs-err">{error}</div>
  {/if}

  {#if toastMsg}
    <div class="cs-toast" role="status" aria-live="polite">{toastMsg}</div>
  {/if}
</section>

{#if menu}
  <div
    class="cs-menu"
    style="left: {menu.x}px; top: {menu.y}px;"
    role="menu"
    onclick={(e) => e.stopPropagation()}
    onkeydown={() => {}}
    tabindex="-1"
  >
    <button role="menuitem" onclick={() => { const s = menu!.smart; menu = null; openEditSmart(s); }}>
      Edit rules…
    </button>
    <button role="menuitem" onclick={() => { const s = menu!.smart; menu = null; saveAsPersonalPreset(s); }}>
      Save as personal preset…
    </button>
    <button role="menuitem" class="danger" onclick={() => handleSmartDelete(menu!.smart)}>
      Delete
    </button>
  </div>
{/if}

{#if builderOpen}
  <SmartCollectionBuilder
    editing={builderEditing}
    onClose={() => (builderOpen = false)}
    onSaved={() => {
      builderOpen = false;
      refresh();
    }}
  />
{/if}

{#if presetPickerOpen}
  <PresetPicker
    onClose={() => (presetPickerOpen = false)}
    onApplied={(p) => {
      toast(`Added preset “${p.name}”`);
      refresh();
    }}
  />
{/if}

<SmartFoldersHubPanel
  open={smartHubOpen}
  onClose={() => (smartHubOpen = false)}
/>

<OcrQueuePanel
  open={ocrQueueOpen}
  onClose={() => (ocrQueueOpen = false)}
/>

<style>
  .cs-rail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 6px;
  }
  .cs-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
  }
  .cs-title {
    font-weight: 600;
  }
  .cs-add {
    background: transparent;
    border: none;
    color: var(--text-2);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 6px;
    transition: background 120ms ease, color 120ms ease;
  }
  .cs-add:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .cs-new {
    display: flex;
    gap: 4px;
    padding: 4px 8px 8px;
  }
  .cs-input {
    flex: 1;
    background: var(--surface-2);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    padding: 4px 8px;
    color: var(--text-1);
    font-size: 12px;
    outline: none;
  }
  .cs-input:focus {
    border-color: var(--accent);
  }
  .cs-save {
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 0 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .cs-save:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .cs-row-wrap {
    display: flex;
    align-items: center;
  }
  .cs-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: transparent;
    border: none;
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    color: var(--text-2);
    font-size: 13px;
    transition: background 120ms ease, color 120ms ease;
  }
  .cs-row:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .cs-row.active {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--text-1);
  }
  .cs-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .cs-dot.diamond {
    transform: rotate(45deg);
    border-radius: 1px;
  }
  .cs-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cs-count {
    background: var(--surface-2);
    color: var(--text-3);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    transition:
      transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1),
      background 180ms ease,
      color 180ms ease;
  }
  .cs-count.pulse {
    background: var(--accent);
    color: #fff;
    transform: scale(1.18);
  }
  .cs-x {
    background: transparent;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 0 4px;
    font-size: 14px;
    line-height: 1;
    opacity: 0;
    transition: opacity 120ms ease, color 120ms ease;
  }
  .cs-row:hover .cs-x {
    opacity: 1;
  }
  .cs-x:hover {
    color: var(--danger, #f87171);
  }
  .cs-sub {
    padding: 12px 8px 4px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-3);
  }
  .cs-sub-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 6px;
  }
  .cs-sub-row .cs-sub {
    flex: 1;
  }
  .cs-add.small {
    font-size: 14px;
    padding: 0 6px;
  }
  .cs-sub-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .cs-add.small.preset {
    color: #facc15;
    font-size: 13px;
  }
  .cs-add.small.preset:hover {
    color: #fde047;
    filter: drop-shadow(0 0 4px rgba(250, 204, 21, 0.5));
  }
  .cs-row-wrap.drag-over,
  .cs-row-wrap.drag-over .cs-row {
    background: color-mix(in oklab, var(--accent) 22%, transparent);
    outline: 1px dashed color-mix(in oklab, var(--accent) 70%, transparent);
    outline-offset: -2px;
    border-radius: 8px;
  }
  .cs-row.smart.drag-blocked {
    cursor: not-allowed;
    background: color-mix(in oklab, #fb7185 16%, transparent);
    outline: 1px dashed color-mix(in oklab, #fb7185 50%, transparent);
    outline-offset: -2px;
  }
  .cs-toast {
    position: fixed;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(22, 24, 33, 0.96);
    color: rgba(235, 238, 246, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.1);
    padding: 9px 16px;
    border-radius: 9px;
    font-size: 13px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    z-index: 1200;
    animation: cs-toast-in 200ms ease-out;
  }
  @keyframes cs-toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 6px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }
  .cs-menu {
    position: fixed;
    background: rgba(22, 24, 33, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 9px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    z-index: 1300;
    padding: 4px;
    display: flex;
    flex-direction: column;
    min-width: 160px;
  }
  .cs-menu button {
    background: transparent;
    border: none;
    color: rgba(235, 238, 246, 0.92);
    text-align: left;
    padding: 7px 12px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
  }
  .cs-menu button:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .cs-menu button.danger {
    color: rgba(251, 113, 133, 0.95);
  }
  .cs-menu button.danger:hover {
    background: rgba(251, 113, 133, 0.12);
  }
  .cs-empty {
    padding: 8px;
    font-size: 12px;
    color: var(--text-3);
    font-style: italic;
  }
  .cs-err {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--danger, #f87171);
  }
</style>
