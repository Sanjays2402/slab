<script lang="ts">
  // LibraryPanel — v0.12.0 "Atlas" Slice 3.
  //
  // Two-pane Library experience that finally makes the Slice 1 backend
  // user-visible. Left rail: folder filter + tag filter. Main pane:
  // toolbar (Add Folder, Rescan All, search, sort) + responsive cards
  // grid. Click a card → opens that PDF in a new Reader tab (via a
  // `slab:open-library-doc` window event that `+page.svelte` listens
  // for). Right-click / "⋯" → context menu (Open, Add Tag, Remove).
  //
  // The whole panel is dumb-as-possible: state lives here, every action
  // goes through the typed `$lib/library` client, no global stores.
  //
  // What's intentionally NOT here yet:
  //   - Slice 2 will add `notify`-crate filesystem watching so this
  //     panel auto-refreshes when files change in registered folders.
  //   - Slice 4 will add a "Ask Beacon" button per-card and a top-of-
  //     panel "Ask across library" entry point.
  //   - Slice 5 will add saved searches (pin a LibraryFilter).
  //   - Virtualization (Slice 7-ish): a 5000-card grid will be slow.
  //     We accept that until we have a real workload to optimize against.

  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    addFolder,
    addTag,
    listDocuments,
    listFolders,
    listTags,
    removeDocument,
    removeFolder,
    removeTag,
    rescanAll,
    scanFolder,
    setDocumentTags,
    type DocumentRecord,
    type FolderRecord,
    type LibraryFilter,
    type LibrarySortBy,
    type TagRecord,
  } from "$lib/library";
  import { basename } from "$lib/types";
  import { formatRelTime } from "$lib/recent";

  // ---------- State ----------

  let folders = $state<FolderRecord[]>([]);
  let tags = $state<TagRecord[]>([]);
  let docs = $state<DocumentRecord[]>([]);
  let activeFolder = $state<number | "all">("all");
  let activeTagIds = $state<Set<number>>(new Set());
  let query = $state("");
  let sort = $state<LibrarySortBy>("added_desc");
  let loading = $state(false);
  let scanning = $state(false);
  let error = $state<string | null>(null);
  let initialized = $state(false);

  // Context menu state. `null` when closed.
  type Menu = {
    doc: DocumentRecord;
    x: number;
    y: number;
    submenu: "tag" | null;
  };
  let menu = $state<Menu | null>(null);

  // New-tag modal state.
  let newTagOpen = $state(false);
  let newTagName = $state("");
  let newTagColor = $state<string | null>(null);
  let pendingDocForTag = $state<DocumentRecord | null>(null);

  // Debounced search.
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  const TAG_PALETTE = [
    "#ff7a59", // accent
    "#6ab7ff",
    "#7ee787",
    "#f5c518",
    "#c084fc",
    "#f47272",
    "#79c0ff",
    "#a4a4b0",
  ];

  // ---------- Derived counts ----------

  let totalDocs = $derived(docs.length);
  let docsForActiveFolder = $derived(
    activeFolder === "all"
      ? docs.length
      : docs.filter((d) => d.folder_id === activeFolder).length,
  );

  // ---------- Lifecycle ----------

  onMount(async () => {
    await refreshAll();
    initialized = true;
    window.addEventListener("click", onWindowClickForMenu);
  });

  function onWindowClickForMenu(_e: MouseEvent) {
    // Close the menu whenever the user clicks elsewhere. Right-click
    // on a different card replaces it; this just handles the
    // "click outside" case.
    menu = null;
  }

  // ---------- Data loaders ----------

  async function refreshAll() {
    loading = true;
    error = null;
    try {
      const [f, t] = await Promise.all([listFolders(), listTags()]);
      folders = f;
      tags = t;
      await refreshDocs();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function refreshDocs() {
    const filter: LibraryFilter = {
      folder_id: activeFolder === "all" ? null : activeFolder,
      tag_ids: Array.from(activeTagIds),
      title_substring: query.trim() ? query.trim() : null,
      sort,
    };
    try {
      docs = await listDocuments(filter);
    } catch (e) {
      error = String(e);
    }
  }

  // Reactive search — debounce 150ms so we don't hammer sqlite on
  // every keystroke.
  function onQueryInput(e: Event) {
    query = (e.target as HTMLInputElement).value;
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      void refreshDocs();
    }, 150);
  }

  $effect(() => {
    // Re-run docs whenever folder / tags / sort changes. Search is
    // handled by its own debounce.
    if (!initialized) return;
    // Touching reactive deps deliberately:
    activeFolder;
    activeTagIds;
    sort;
    void refreshDocs();
  });

  // ---------- Toolbar actions ----------

  async function onAddFolder() {
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      const folder = await addFolder(picked);
      // Reload folder list immediately so the rail shows the new entry,
      // then kick off the scan in the background.
      folders = await listFolders();
      scanning = true;
      error = null;
      try {
        await scanFolder(folder.id);
      } catch (e) {
        error = `Scan failed for ${picked}: ${String(e)}`;
      } finally {
        scanning = false;
        await refreshDocs();
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function onRescanAll() {
    if (folders.length === 0) {
      error = "Add a folder before rescanning.";
      return;
    }
    scanning = true;
    error = null;
    try {
      const reports = await rescanAll();
      const totals = reports.reduce(
        (acc, r) => {
          acc.added += r.added ?? 0;
          acc.updated += r.updated ?? 0;
          acc.unchanged += r.unchanged ?? 0;
          return acc;
        },
        { added: 0, updated: 0, unchanged: 0 },
      );
      lastScanSummary = `Scanned ${folders.length} folder${folders.length === 1 ? "" : "s"}: ${totals.added} new, ${totals.updated} updated, ${totals.unchanged} unchanged`;
    } catch (e) {
      error = String(e);
    } finally {
      scanning = false;
      await refreshAll();
    }
  }
  let lastScanSummary = $state<string | null>(null);

  // ---------- Folder rail actions ----------

  function selectFolder(id: number | "all") {
    activeFolder = id;
  }

  async function onRemoveFolder(folder: FolderRecord) {
    const ok = window.confirm(
      `Stop tracking ${folder.path}?\n\nThe folder stays on disk — Slab just forgets about the PDFs inside it.`,
    );
    if (!ok) return;
    try {
      await removeFolder(folder.id);
      if (activeFolder === folder.id) activeFolder = "all";
      await refreshAll();
    } catch (e) {
      error = String(e);
    }
  }

  // ---------- Tag rail actions ----------

  function toggleTag(tag: TagRecord) {
    const next = new Set(activeTagIds);
    if (next.has(tag.id)) next.delete(tag.id);
    else next.add(tag.id);
    activeTagIds = next;
  }

  async function onCreateTopLevelTag() {
    pendingDocForTag = null;
    newTagName = "";
    newTagColor = TAG_PALETTE[0];
    newTagOpen = true;
  }

  async function onRemoveTag(tag: TagRecord) {
    const ok = window.confirm(
      `Delete tag "${tag.name}"? It will be removed from every document.`,
    );
    if (!ok) return;
    try {
      await removeTag(tag.id);
      const next = new Set(activeTagIds);
      next.delete(tag.id);
      activeTagIds = next;
      await refreshAll();
    } catch (e) {
      error = String(e);
    }
  }

  async function commitNewTag() {
    const name = newTagName.trim();
    if (!name) return;
    try {
      const created = await addTag(name, newTagColor);
      tags = await listTags();
      // If this was a per-doc "Add Tag → New tag…" flow, attach it.
      if (pendingDocForTag) {
        const existingIds = pendingDocForTag.tags.map((t) => t.id);
        await setDocumentTags(pendingDocForTag.id, [...existingIds, created.id]);
        pendingDocForTag = null;
        await refreshDocs();
      }
      newTagOpen = false;
      newTagName = "";
    } catch (e) {
      error = String(e);
    }
  }

  // ---------- Card / context menu actions ----------

  function openDocInTab(doc: DocumentRecord) {
    menu = null;
    window.dispatchEvent(
      new CustomEvent("slab:open-library-doc", { detail: { path: doc.path } }),
    );
  }

  function openMenuFor(e: MouseEvent, doc: DocumentRecord) {
    e.preventDefault();
    e.stopPropagation();
    menu = { doc, x: e.clientX, y: e.clientY, submenu: null };
  }

  function onMenuItemClick(e: MouseEvent) {
    // Prevent the window-level click handler from closing the menu
    // before we handle it.
    e.stopPropagation();
  }
  function onMenuKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") menu = null;
    e.stopPropagation();
  }

  async function onMenuToggleTag(doc: DocumentRecord, tag: TagRecord) {
    const existing = doc.tags.map((t) => t.id);
    const next = existing.includes(tag.id)
      ? existing.filter((id) => id !== tag.id)
      : [...existing, tag.id];
    try {
      await setDocumentTags(doc.id, next);
      await refreshDocs();
      // Re-load the menu's `doc` from the fresh list so future
      // toggles see the new tag set.
      const fresh = docs.find((d) => d.id === doc.id);
      if (fresh && menu) menu = { ...menu, doc: fresh };
    } catch (e) {
      error = String(e);
    }
  }

  async function onMenuRemoveDoc(doc: DocumentRecord) {
    menu = null;
    const ok = window.confirm(
      `Remove "${doc.title ?? basename(doc.path)}" from your library?\n\nThe file stays on disk; Slab just forgets about it until the next scan.`,
    );
    if (!ok) return;
    try {
      await removeDocument(doc.id);
      await refreshDocs();
    } catch (e) {
      error = String(e);
    }
  }

  function openNewTagForDoc(doc: DocumentRecord) {
    pendingDocForTag = doc;
    newTagName = "";
    newTagColor = TAG_PALETTE[0];
    newTagOpen = true;
    menu = null;
  }

  // ---------- Helpers ----------

  function displayTitle(d: DocumentRecord): string {
    return d.title ?? basename(d.path).replace(/\.pdf$/i, "");
  }
  function relPath(d: DocumentRecord): string {
    const f = folders.find((x) => x.id === d.folder_id);
    if (!f) return d.path;
    return d.path.startsWith(f.path)
      ? d.path.slice(f.path.length).replace(/^[/\\]+/, "")
      : d.path;
  }
  function folderShortName(p: string): string {
    return basename(p) || p;
  }
</script>

<header class="content-header">
  <h1>Library</h1>
  <p class="subtitle">
    Index folders of PDFs. Click any doc to open it in a new Reader tab.
  </p>
</header>

<section class="library">
  <!-- Toolbar -->
  <div class="toolbar">
    <button class="primary" onclick={onAddFolder} disabled={scanning}>
      + Add Folder
    </button>
    <button class="ghost" onclick={onRescanAll} disabled={scanning || folders.length === 0}>
      {scanning ? "Scanning…" : "↻ Rescan All"}
    </button>
    <div class="search">
      <span class="search-icon">⌕</span>
      <input
        type="search"
        placeholder="Search by title or filename…"
        value={query}
        oninput={onQueryInput}
      />
    </div>
    <div class="sort">
      <label>
        Sort:
        <select bind:value={sort}>
          <option value="added_desc">Recently added</option>
          <option value="last_seen_desc">Recently seen</option>
          <option value="title_asc">Title (A-Z)</option>
        </select>
      </label>
    </div>
  </div>

  {#if error}
    <div class="status err">✕ {error}</div>
  {/if}
  {#if lastScanSummary && !error && !scanning}
    <div class="status ok">✓ {lastScanSummary}</div>
  {/if}

  <div class="layout">
    <!-- Left rail -->
    <aside class="rail">
      <div class="rail-section">
        <div class="rail-head">
          <span class="rail-title">Folders</span>
          <span class="rail-count">{folders.length}</span>
        </div>
        <button
          class="rail-row"
          class:active={activeFolder === "all"}
          onclick={() => selectFolder("all")}
        >
          <span class="rail-icon">▦</span>
          <span class="rail-label">All</span>
          <span class="rail-meta">{totalDocs}</span>
        </button>
        {#each folders as f (f.id)}
          <div class="rail-row-wrap">
            <button
              class="rail-row"
              class:active={activeFolder === f.id}
              onclick={() => selectFolder(f.id)}
              title={f.path}
            >
              <span class="rail-icon">❐</span>
              <span class="rail-label">{folderShortName(f.path)}</span>
              <span class="rail-meta">
                {docs.filter((d) => d.folder_id === f.id).length}
              </span>
            </button>
            <button
              class="rail-row-x"
              title="Stop tracking {f.path}"
              aria-label="Stop tracking folder"
              onclick={() => onRemoveFolder(f)}
            >×</button>
          </div>
        {/each}
        {#if folders.length === 0 && initialized}
          <div class="rail-empty">No folders yet</div>
        {/if}
      </div>

      <div class="rail-section">
        <div class="rail-head">
          <span class="rail-title">Tags</span>
          <span class="rail-count">{tags.length}</span>
        </div>
        {#each tags as t (t.id)}
          <div class="rail-row-wrap">
            <button
              class="rail-row tag"
              class:active={activeTagIds.has(t.id)}
              onclick={() => toggleTag(t)}
            >
              <span class="rail-icon dot" style:background={t.color ?? "var(--text-3)"}></span>
              <span class="rail-label">{t.name}</span>
            </button>
            <button
              class="rail-row-x"
              title="Delete tag"
              aria-label="Delete tag"
              onclick={() => onRemoveTag(t)}
            >×</button>
          </div>
        {/each}
        <button class="rail-add" onclick={onCreateTopLevelTag}>+ New tag</button>
      </div>
    </aside>

    <!-- Main grid -->
    <main class="main">
      {#if loading && !initialized}
        <div class="empty">
          <div class="empty-icon">⌛</div>
          <div class="empty-title">Loading library…</div>
        </div>
      {:else if folders.length === 0}
        <div class="empty">
          <div class="empty-icon">❐</div>
          <div class="empty-title">Build your library</div>
          <div class="empty-sub">
            Add a folder of PDFs to index. Slab will track them, tag them,
            and let you search across the whole collection.
          </div>
          <button class="primary" onclick={onAddFolder}>+ Add Folder</button>
        </div>
      {:else if docs.length === 0}
        <div class="empty">
          <div class="empty-icon">⌕</div>
          <div class="empty-title">
            {query.trim() ? "No matches" : "No PDFs in this view"}
          </div>
          <div class="empty-sub">
            {query.trim()
              ? `No documents match "${query.trim()}". Try clearing filters or searching differently.`
              : "This folder has been indexed but contains no PDFs yet, or all PDFs are filtered out."}
          </div>
        </div>
      {:else}
        <div class="grid">
          {#each docs as d (d.id)}
            <div
              class="card"
              role="button"
              tabindex="0"
              oncontextmenu={(e) => openMenuFor(e, d)}
              onclick={() => openDocInTab(d)}
              onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") openDocInTab(d); }}
            >
              <div class="card-head">
                <div class="card-title" title={d.path}>{displayTitle(d)}</div>
                <button
                  class="card-menu"
                  title="More"
                  aria-label="Open menu"
                  onclick={(e) => { e.stopPropagation(); openMenuFor(e, d); }}
                >⋯</button>
              </div>
              <div class="card-meta">
                <span class="card-pages">{d.pages ?? "?"} pages</span>
                <span class="card-sep">·</span>
                <span class="card-seen">{formatRelTime(d.last_seen_at * 1000)}</span>
              </div>
              <div class="card-path" title={d.path}>{relPath(d)}</div>
              {#if d.tags.length > 0}
                <div class="card-tags">
                  {#each d.tags.slice(0, 3) as t (t.id)}
                    <span
                      class="chip"
                      style:border-left-color={t.color ?? "var(--text-3)"}
                    >{t.name}</span>
                  {/each}
                  {#if d.tags.length > 3}
                    <span class="chip more">+{d.tags.length - 3}</span>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </main>
  </div>
</section>

<!-- Context menu (right-click / ⋯) -->
{#if menu}
  <div
    class="menu"
    role="menu"
    tabindex="-1"
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    onclick={onMenuItemClick}
    onkeydown={onMenuKeyDown}
  >
    <button class="menu-item" onclick={() => openDocInTab(menu!.doc)}>
      <span>Open in Reader tab</span>
    </button>
    <div class="menu-sep"></div>
    <div class="menu-section">Tags</div>
    {#each tags as t (t.id)}
      {@const attached = menu.doc.tags.some((dt) => dt.id === t.id)}
      <button
        class="menu-item tag-toggle"
        onclick={() => onMenuToggleTag(menu!.doc, t)}
      >
        <span class="dot small" style:background={t.color ?? "var(--text-3)"}></span>
        <span class="menu-label">{t.name}</span>
        {#if attached}<span class="menu-check">✓</span>{/if}
      </button>
    {/each}
    <button class="menu-item" onclick={() => openNewTagForDoc(menu!.doc)}>
      <span class="menu-label">+ New tag…</span>
    </button>
    <div class="menu-sep"></div>
    <button class="menu-item danger" onclick={() => onMenuRemoveDoc(menu!.doc)}>
      <span>Remove from library</span>
    </button>
  </div>
{/if}

<!-- New tag modal -->
{#if newTagOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Close"
    onclick={() => (newTagOpen = false)}
    onkeydown={(e) => { if (e.key === "Escape") newTagOpen = false; }}
  >
    <div class="modal" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="modal-title">New tag</div>
      <input
        type="text"
        placeholder="Tag name"
        bind:value={newTagName}
        autofocus
        onkeydown={(e) => { if (e.key === "Enter") commitNewTag(); }}
      />
      <div class="palette">
        {#each TAG_PALETTE as c (c)}
          <button
            class="swatch"
            class:active={newTagColor === c}
            style:background={c}
            aria-label="Pick color {c}"
            onclick={() => (newTagColor = c)}
          ></button>
        {/each}
      </div>
      <div class="modal-actions">
        <button class="ghost" onclick={() => (newTagOpen = false)}>Cancel</button>
        <button class="primary" onclick={commitNewTag} disabled={!newTagName.trim()}>
          Create
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .library {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    padding: 12px 20px 20px;
    gap: 12px;
  }

  /* Toolbar */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .toolbar .primary {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    padding: 7px 14px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-weight: 600;
  }
  .toolbar .primary:hover:not(:disabled) { background: var(--accent-2); }
  .toolbar .primary:disabled { opacity: 0.55; }
  .toolbar .ghost {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 7px 14px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .toolbar .ghost:hover:not(:disabled) { border-color: var(--border-strong); }
  .toolbar .ghost:disabled { opacity: 0.55; }

  .search {
    flex: 1;
    min-width: 200px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 0 10px;
  }
  .search-icon { color: var(--text-3); font-size: 14px; }
  .search input {
    flex: 1;
    background: transparent;
    border: 0;
    outline: 0;
    color: var(--text);
    font-size: 13px;
    padding: 7px 0;
  }
  .search input::placeholder { color: var(--text-3); }

  .sort label {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-3);
    font-size: 12px;
  }
  .sort select {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 12px;
  }

  /* Layout */
  .layout {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: 16px;
  }
  .rail {
    width: 220px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    padding-right: 4px;
  }
  .main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding-right: 4px;
  }

  /* Rail */
  .rail-section { display: flex; flex-direction: column; gap: 2px; }
  .rail-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 0 6px 6px;
    color: var(--text-3);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .rail-title { font-weight: 600; }
  .rail-count {
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
  }
  .rail-row-wrap {
    display: flex;
    align-items: stretch;
    border-radius: var(--r-sm);
  }
  .rail-row-wrap:hover { background: var(--bg-3); }
  .rail-row {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 13px;
    text-align: left;
    min-width: 0;
  }
  .rail-row:hover:not(:disabled) { color: var(--text); }
  .rail-row.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .rail-icon {
    width: 14px;
    text-align: center;
    color: var(--accent);
    opacity: 0.85;
    flex-shrink: 0;
  }
  .rail-icon.dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin: 0 3px;
  }
  .rail-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rail-meta {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .rail-row-x {
    background: transparent;
    border: 0;
    color: var(--text-3);
    padding: 0 8px;
    cursor: pointer;
    opacity: 0;
    font-size: 14px;
  }
  .rail-row-wrap:hover .rail-row-x { opacity: 1; }
  .rail-row-x:hover { color: var(--accent); }
  .rail-add {
    background: transparent;
    border: 1px dashed var(--border);
    color: var(--text-3);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 12px;
    margin-top: 4px;
  }
  .rail-add:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .rail-empty {
    color: var(--text-3);
    font-size: 12px;
    padding: 4px 8px;
    font-style: italic;
  }

  /* Grid */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
  }
  .card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    cursor: pointer;
    transition: background 80ms ease, border-color 80ms ease;
  }
  .card:hover {
    background: var(--bg-3);
    border-color: var(--border-strong);
  }
  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 6px;
  }
  .card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    word-break: break-word;
  }
  .card-menu {
    background: transparent;
    border: 0;
    color: var(--text-3);
    font-size: 16px;
    line-height: 1;
    padding: 0 4px;
    cursor: pointer;
    opacity: 0;
  }
  .card:hover .card-menu { opacity: 1; }
  .card-menu:hover { color: var(--text); }
  .card-meta {
    font-size: 11px;
    color: var(--text-3);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .card-sep { color: var(--text-3); opacity: 0.5; }
  .card-path {
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  .chip {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-left: 2px solid var(--text-3);
    padding: 1px 6px;
    border-radius: 3px;
  }
  .chip.more {
    border-left-color: var(--text-3);
    color: var(--text-3);
  }

  /* Status messages (reused look from other panels) */
  .status {
    font-size: 12px;
    padding: 6px 10px;
    border-radius: var(--r-sm);
  }
  .status.ok {
    background: rgba(126, 231, 135, 0.08);
    border: 1px solid rgba(126, 231, 135, 0.3);
    color: #9ae8a1;
  }
  .status.err {
    background: rgba(244, 114, 114, 0.08);
    border: 1px solid rgba(244, 114, 114, 0.3);
    color: #f59292;
  }

  /* Empty state */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 60px 20px;
    color: var(--text-2);
    gap: 8px;
  }
  .empty-icon {
    font-size: 32px;
    color: var(--accent);
    opacity: 0.8;
    margin-bottom: 4px;
  }
  .empty-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }
  .empty-sub {
    font-size: 13px;
    color: var(--text-3);
    max-width: 380px;
    line-height: 1.5;
  }
  .empty .primary {
    margin-top: 8px;
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    padding: 8px 16px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-weight: 600;
  }

  /* Context menu */
  .menu {
    position: fixed;
    z-index: 1000;
    background: var(--bg-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    padding: 4px;
    min-width: 200px;
    max-height: 70vh;
    overflow-y: auto;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: transparent;
    border: 0;
    color: var(--text);
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .menu-item:hover { background: var(--bg-2); }
  .menu-item.danger { color: #f59292; }
  .menu-item.danger:hover { background: rgba(244, 114, 114, 0.1); }
  .menu-label { flex: 1; }
  .menu-check { color: var(--accent); font-size: 12px; }
  .menu-section {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    padding: 4px 10px 2px;
  }
  .menu-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
  .dot.small {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  /* Modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 999;
  }
  .modal {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 16px 18px;
    min-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  .modal-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .modal input {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 7px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
    outline: 0;
  }
  .modal input:focus { border-color: var(--accent); }
  .palette {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }
  .swatch.active {
    border-color: var(--text);
    transform: scale(1.1);
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .modal-actions .ghost,
  .modal-actions .primary {
    padding: 6px 14px;
    border-radius: var(--r-sm);
    font-size: 12px;
  }
  .modal-actions .ghost {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
  }
  .modal-actions .primary {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    font-weight: 600;
  }
  .modal-actions .primary:disabled { opacity: 0.5; }
</style>
