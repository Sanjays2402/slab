<script lang="ts">
  // Outline editor — edit the bookmark tree of the currently-open PDF.
  //
  // The component fetches the current outline from the Rust backend
  // (`slab_read_outline`), lets the user rename / reorder / indent / delete
  // entries, and writes back the new tree (`slab_write_outline`). When the
  // save succeeds the parent is notified via the `onsaved` callback so it can
  // re-load the document.
  //
  // Tree model:
  //   - We keep a `roots: EditNode[]` with each node carrying its own
  //     `children: EditNode[]`. All mutations operate on the array that owns
  //     the node, so reactivity is preserved without manual cloning.
  //   - Each node gets a stable client-side `id` so the {#each} key is
  //     stable across edits (titles are not unique).

  import { invoke } from "@tauri-apps/api/core";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { isInTauri } from "$lib/tauri";

  type Props = {
    /** Path of the currently-open PDF — the file we read and write. */
    path: string;
    /** Total page count, used to clamp page-index inputs. */
    pageCount: number;
    /** Called when the dialog should close (cancelled or saved). */
    onclose: () => void;
    /** Called after a successful save. `path` = output path. */
    onsaved: (path: string) => void;
  };

  let { path, pageCount, onclose, onsaved }: Props = $props();

  type RawNode = {
    title: string;
    page_index: number | null;
    children: RawNode[];
  };

  type EditNode = {
    id: number;
    title: string;
    pageIndex: number | null; // 0-based
    children: EditNode[];
  };

  let nextId = 1;
  function makeId() {
    return nextId++;
  }

  function toEdit(raw: RawNode): EditNode {
    return {
      id: makeId(),
      title: raw.title,
      pageIndex: raw.page_index ?? null,
      children: raw.children.map(toEdit),
    };
  }

  function toRaw(node: EditNode): RawNode {
    return {
      title: node.title,
      page_index: node.pageIndex,
      children: node.children.map(toRaw),
    };
  }

  let roots = $state<EditNode[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  // Loading happens on mount. We use $effect with a `loaded` flag rather than
  // onMount so reactivity is consistent with the rest of Slab's Svelte 5 code.
  let loaded = false;
  $effect(() => {
    if (loaded) return;
    loaded = true;
    void load();
  });

  async function load() {
    loading = true;
    loadError = null;
    try {
      if (isInTauri()) {
        const raw = await invoke<RawNode[]>("slab_read_outline", { input: path });
        roots = raw.map(toEdit);
      } else {
        // Browser dev fallback: start with an empty tree so the UI is usable.
        roots = [];
      }
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  // Locate a node's parent array + index. Returns null if not found.
  function locate(target: EditNode, arr: EditNode[] = roots): { arr: EditNode[]; idx: number } | null {
    for (let i = 0; i < arr.length; i++) {
      if (arr[i].id === target.id) return { arr, idx: i };
      const inner = locate(target, arr[i].children);
      if (inner) return inner;
    }
    return null;
  }

  function moveUp(node: EditNode) {
    const loc = locate(node);
    if (!loc || loc.idx === 0) return;
    const a = loc.arr;
    [a[loc.idx - 1], a[loc.idx]] = [a[loc.idx], a[loc.idx - 1]];
    roots = [...roots];
  }

  function moveDown(node: EditNode) {
    const loc = locate(node);
    if (!loc || loc.idx >= loc.arr.length - 1) return;
    const a = loc.arr;
    [a[loc.idx + 1], a[loc.idx]] = [a[loc.idx], a[loc.idx + 1]];
    roots = [...roots];
  }

  function indent(node: EditNode) {
    const loc = locate(node);
    if (!loc || loc.idx === 0) return; // nothing to nest under
    const prev = loc.arr[loc.idx - 1];
    loc.arr.splice(loc.idx, 1);
    prev.children.push(node);
    roots = [...roots];
  }

  function outdent(node: EditNode) {
    // Find the parent node by searching for one whose children array
    // contains `node`.
    function findParent(arr: EditNode[], parent: EditNode | null): EditNode | null {
      for (const n of arr) {
        if (n.children.some((c) => c.id === node.id)) return n;
        const deeper = findParent(n.children, n);
        if (deeper) return deeper;
      }
      return null;
    }
    const parent = findParent(roots, null);
    if (!parent) return; // already at root level
    const idxInParent = parent.children.findIndex((c) => c.id === node.id);
    if (idxInParent < 0) return;
    parent.children.splice(idxInParent, 1);

    // Insert immediately after `parent` in `parent`'s own owner array.
    const parentLoc = locate(parent);
    if (!parentLoc) {
      // Shouldn't happen; restore.
      parent.children.splice(idxInParent, 0, node);
      return;
    }
    parentLoc.arr.splice(parentLoc.idx + 1, 0, node);
    roots = [...roots];
  }

  function addSibling(node: EditNode) {
    const loc = locate(node);
    if (!loc) return;
    loc.arr.splice(loc.idx + 1, 0, {
      id: makeId(),
      title: "New entry",
      pageIndex: node.pageIndex,
      children: [],
    });
    roots = [...roots];
  }

  function addChild(node: EditNode) {
    node.children.push({
      id: makeId(),
      title: "New entry",
      pageIndex: node.pageIndex,
      children: [],
    });
    roots = [...roots];
  }

  function addTopLevel() {
    roots = [
      ...roots,
      {
        id: makeId(),
        title: "New entry",
        pageIndex: 0,
        children: [],
      },
    ];
  }

  function remove(node: EditNode) {
    const loc = locate(node);
    if (!loc) return;
    loc.arr.splice(loc.idx, 1);
    roots = [...roots];
  }

  function clampPage(value: number): number {
    if (!Number.isFinite(value)) return 0;
    return Math.max(0, Math.min(pageCount - 1, Math.floor(value)));
  }

  function updatePage(node: EditNode, raw: string) {
    if (raw === "") {
      node.pageIndex = null;
    } else {
      const v = Number.parseInt(raw, 10);
      node.pageIndex = Number.isFinite(v) ? clampPage(v - 1) : node.pageIndex;
    }
    roots = [...roots];
  }

  async function saveOverwrite() {
    await doSave(path);
  }

  async function saveAs() {
    if (!isInTauri()) {
      saveError = "Save As only works in the desktop app.";
      return;
    }
    const target = await saveDialog({
      defaultPath: suggestSaveName(),
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof target !== "string") return;
    await doSave(target);
  }

  function suggestSaveName(): string {
    const base = path.split(/[\/\\]/).pop() ?? "outline.pdf";
    return base.replace(/\.pdf$/i, "") + "-outline.pdf";
  }

  async function doSave(output: string) {
    if (!isInTauri()) {
      saveError = "Saving requires the desktop app.";
      return;
    }
    saving = true;
    saveError = null;
    try {
      const nodes = roots.map(toRaw);
      await invoke<number>("slab_write_outline", { input: path, output, nodes });
      onsaved(output);
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="outline-editor-backdrop" role="presentation" onclick={onclose}></div>
<section class="outline-editor" role="dialog" aria-labelledby="outline-editor-title">
  <header class="oe-head">
    <h2 id="outline-editor-title">Edit outline</h2>
    <button class="oe-close" onclick={onclose} title="Close (Esc)">×</button>
  </header>

  <div class="oe-body">
    {#if loading}
      <div class="oe-status">Loading outline…</div>
    {:else if loadError}
      <div class="oe-status err">Couldn't load outline: {loadError}</div>
    {:else if roots.length === 0}
      <div class="oe-empty">
        <p>This PDF has no outline yet.</p>
        <button class="oe-btn primary" onclick={addTopLevel}>Add first entry</button>
      </div>
    {:else}
      <ul class="oe-list">
        {@render renderLevel(roots, 0)}
      </ul>
      <div class="oe-row-actions">
        <button class="oe-btn ghost" onclick={addTopLevel}>+ Add top-level entry</button>
      </div>
    {/if}
  </div>

  {#if saveError}
    <div class="oe-status err">{saveError}</div>
  {/if}

  <footer class="oe-foot">
    <button class="oe-btn" onclick={onclose} disabled={saving}>Cancel</button>
    <span class="oe-spacer"></span>
    <button class="oe-btn" onclick={saveAs} disabled={saving || loading}>Save as…</button>
    <button class="oe-btn primary" onclick={saveOverwrite} disabled={saving || loading || !isInTauri()}>
      {saving ? "Saving…" : "Save"}
    </button>
  </footer>
</section>

{#snippet renderLevel(nodes: EditNode[], depth: number)}
  {#each nodes as node (node.id)}
    <li class="oe-item">
      <div class="oe-row" style="padding-left: {depth * 16}px">
        <input
          class="oe-title"
          type="text"
          bind:value={node.title}
          placeholder="Untitled"
          aria-label="Outline entry title"
        />
        <span class="oe-page-wrap" title="Target page (1-based)">
          <input
            class="oe-page"
            type="number"
            min="1"
            max={pageCount}
            value={node.pageIndex !== null ? node.pageIndex + 1 : ""}
            aria-label="Outline entry target page"
            oninput={(e) => updatePage(node, (e.currentTarget as HTMLInputElement).value)}
            placeholder="—"
          />
        </span>
        <span class="oe-actions">
          <button class="oe-icon" onclick={() => moveUp(node)} title="Move up">↑</button>
          <button class="oe-icon" onclick={() => moveDown(node)} title="Move down">↓</button>
          <button class="oe-icon" onclick={() => outdent(node)} title="Outdent">⇤</button>
          <button class="oe-icon" onclick={() => indent(node)} title="Indent">⇥</button>
          <button class="oe-icon" onclick={() => addChild(node)} title="Add child">＋⤵</button>
          <button class="oe-icon" onclick={() => addSibling(node)} title="Add below">＋</button>
          <button class="oe-icon danger" onclick={() => remove(node)} title="Delete">×</button>
        </span>
      </div>
      {#if node.children.length > 0}
        <ul class="oe-list nested">
          {@render renderLevel(node.children, depth + 1)}
        </ul>
      {/if}
    </li>
  {/each}
{/snippet}

<style>
  .outline-editor-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.42);
    backdrop-filter: blur(2px);
    z-index: 80;
  }

  .outline-editor {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(820px, 92vw);
    max-height: 86vh;
    background: var(--bg-1, #111);
    color: var(--text, #eee);
    border-radius: 14px;
    border: 1px solid var(--border, #2a2a2a);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.55);
    display: flex;
    flex-direction: column;
    z-index: 81;
    overflow: hidden;
  }

  .oe-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border, #2a2a2a);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(255, 255, 255, 0));
  }
  .oe-head h2 { margin: 0; font-size: 16px; font-weight: 600; letter-spacing: 0.1px; }
  .oe-close {
    border: 0; background: transparent; color: var(--muted, #aaa);
    font-size: 22px; line-height: 1; cursor: pointer; padding: 4px 8px; border-radius: 6px;
  }
  .oe-close:hover { background: var(--bg-2, #1a1a1a); color: var(--text, #eee); }

  .oe-body {
    overflow: auto;
    padding: 12px 14px;
    flex: 1;
  }

  .oe-status {
    padding: 10px 12px;
    background: var(--bg-2, #1a1a1a);
    border-radius: 8px;
    font-size: 13px;
    color: var(--muted, #aaa);
  }
  .oe-status.err { color: #ff8a8a; }

  .oe-empty {
    text-align: center;
    padding: 28px 8px;
    color: var(--muted, #aaa);
  }
  .oe-empty p { margin: 0 0 12px; }

  .oe-list { list-style: none; padding: 0; margin: 0; }
  .oe-list.nested { margin-top: 2px; }
  .oe-item { padding: 1px 0; }

  .oe-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px;
    border-radius: 8px;
  }
  .oe-row:hover { background: var(--bg-2, #1a1a1a); }

  .oe-title {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text, #eee);
    padding: 6px 8px;
    border-radius: 6px;
    font-size: 13px;
  }
  .oe-title:focus {
    outline: none;
    border-color: var(--accent, #6ea9ff);
    background: var(--bg-0, #0c0c0c);
  }

  .oe-page-wrap { display: inline-flex; align-items: center; }
  .oe-page {
    width: 64px;
    background: var(--bg-0, #0c0c0c);
    color: var(--text, #eee);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    padding: 4px 6px;
    font-size: 12px;
    text-align: center;
  }
  .oe-page:focus {
    outline: none;
    border-color: var(--accent, #6ea9ff);
  }

  .oe-actions { display: inline-flex; gap: 2px; }
  .oe-icon {
    background: transparent;
    border: 1px solid transparent;
    color: var(--muted, #aaa);
    cursor: pointer;
    width: 28px; height: 28px;
    border-radius: 6px;
    font-size: 13px;
    display: inline-flex; align-items: center; justify-content: center;
  }
  .oe-icon:hover { background: var(--bg-1, #111); color: var(--text, #eee); border-color: var(--border, #2a2a2a); }
  .oe-icon.danger:hover { color: #ff8a8a; border-color: #5a2222; }

  .oe-row-actions {
    margin-top: 6px;
    padding: 4px 8px 0;
  }

  .oe-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border, #2a2a2a);
    background: linear-gradient(0deg, rgba(255, 255, 255, 0.03), rgba(255, 255, 255, 0));
  }
  .oe-spacer { flex: 1; }

  .oe-btn {
    background: var(--bg-2, #1a1a1a);
    color: var(--text, #eee);
    border: 1px solid var(--border, #2a2a2a);
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .oe-btn:hover { background: var(--bg-1, #222); }
  .oe-btn.primary {
    background: var(--accent, #6ea9ff);
    border-color: var(--accent, #6ea9ff);
    color: #0a0a0a;
    font-weight: 600;
  }
  .oe-btn.primary:hover { filter: brightness(1.06); }
  .oe-btn.ghost { background: transparent; }
  .oe-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
